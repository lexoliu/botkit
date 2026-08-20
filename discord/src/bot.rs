use std::future::Future;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use async_io::Timer;
use botkit_core::{Bot, BotBuilder, BotError, Context, Event, IntoHandler, Response, Shutdown};
use executor_core::spawn;
use tracing::{debug, error, info, warn};

use crate::client::{
    CHANNEL_MESSAGE_WITH_SOURCE, DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE, DEFERRED_UPDATE_MESSAGE,
    DiscordClient, EPHEMERAL_FLAG, INTERACTION_PONG,
};
use crate::event::{DiscordContextData, MessageContextData};
use crate::gateway::{Gateway, GatewayConnection, GatewayEvent, GatewayIntents, Session};
use crate::types::{Interaction, InteractionData, InteractionType, Message};

/// Longest wait between reconnect attempts.
const MAX_RECONNECT_BACKOFF: Duration = Duration::from_secs(60);
/// First wait between reconnect attempts; doubles up to the max.
const INITIAL_RECONNECT_BACKOFF: Duration = Duration::from_secs(1);
/// How many sessions Discord may reject in a row before we call it fatal.
const MAX_REJECTED_SESSIONS: u32 = 3;

/// Discord bot builder
///
/// Create a bot with command and button handlers, then call `run()` to start.
/// The connection reconnects and resumes on its own, so `run` only returns on
/// shutdown or an unrecoverable error such as a bad token.
///
/// # Example
/// ```ignore
/// // Simple handler - no Context needed!
/// async fn ping() -> &'static str {
///     "Pong!"
/// }
///
/// // With extractors
/// async fn greet(user: User) -> String {
///     format!("Hello, {}!", user.name)
/// }
///
/// let bot = DiscordBot::new(token, app_id, GatewayIntents::GUILDS)
///     .command("ping", ping)
///     .command("greet", greet);
///
/// bot.run().await?;
/// ```
pub struct DiscordBot {
    token: String,
    application_id: String,
    intents: GatewayIntents,
    builder: BotBuilder,
    register_commands: bool,
}

impl DiscordBot {
    /// Create a new Discord bot
    pub fn new(
        token: impl Into<String>,
        application_id: impl Into<String>,
        intents: GatewayIntents,
    ) -> Self {
        Self {
            token: token.into(),
            application_id: application_id.into(),
            intents,
            builder: BotBuilder::new(),
            register_commands: true,
        }
    }

    /// Register a command handler
    pub fn command<H, Args>(mut self, name: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.command(name, handler);
        self
    }

    /// Register a command handler with the description shown in Discord's UI
    pub fn command_with_description<H, Args>(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        handler: H,
    ) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self
            .builder
            .command_with_description(name, description, handler);
        self
    }

    /// Register a button handler with pattern matching
    ///
    /// Pattern can end with `*` for prefix matching (e.g., "confirm_*")
    pub fn button<H, Args>(mut self, pattern: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.button(pattern, handler);
        self
    }

    /// Register a message handler
    ///
    /// Requires the `MESSAGE_CONTENT` intent for the bot to see message text in
    /// guilds; without it, Discord delivers messages with empty content.
    pub fn message<H, Args>(mut self, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.message(handler);
        self
    }

    /// Stop publishing the registered commands to Discord on startup
    ///
    /// On by default. Turn it off when commands are managed elsewhere, or to
    /// avoid the extra API call while iterating locally.
    pub fn skip_command_registration(mut self) -> Self {
        self.register_commands = false;
        self
    }
}

impl Bot for DiscordBot {
    async fn run_until(self, shutdown: Shutdown) -> Result<(), BotError> {
        let client = DiscordClient::new(&self.token, &self.application_id);
        let gateway = Gateway::new(&self.token, self.intents);

        if self.register_commands && self.builder.has_commands() {
            let commands: Vec<_> = self
                .builder
                .commands()
                .map(|command| {
                    serde_json::json!({
                        "name": command.name,
                        // Discord rejects commands with an empty description.
                        "description": if command.description.is_empty() {
                            command.name
                        } else {
                            command.description
                        },
                        "type": 1,
                    })
                })
                .collect();

            if let Err(e) = client.set_global_commands(&commands).await {
                warn!("Failed to register slash commands: {e}");
            }
        }

        let bot = Arc::new(BotState {
            builder: self.builder,
            client,
        });

        run_gateway(gateway, bot, shutdown).await
    }
}

/// Shared, immutable state each dispatched event needs.
struct BotState {
    builder: BotBuilder,
    client: DiscordClient,
}

/// Connect, dispatch, and reconnect until shutdown or an unrecoverable error.
async fn run_gateway(
    gateway: Gateway,
    bot: Arc<BotState>,
    shutdown: Shutdown,
) -> Result<(), BotError> {
    let mut backoff = INITIAL_RECONNECT_BACKOFF;
    // Carried across reconnects so missed events can be replayed.
    let mut session = None;
    // Consecutive sessions Discord rejected outright. A valid token gets a
    // `READY` on the first try, so repeated rejections mean bad credentials or
    // intents rather than a transient blip, and retrying forever would hide it.
    let mut rejected_sessions = 0u32;

    loop {
        if shutdown.is_shutdown() {
            return Ok(());
        }

        let connect = async {
            match &session {
                Some(session) => gateway.resume(session).await,
                None => gateway.connect().await,
            }
        };

        let connection = match race_shutdown(connect, &shutdown).await {
            Shutdownable::Shutdown => return Ok(()),
            Shutdownable::Value(Ok(connection)) => connection,
            Shutdownable::Value(Err(e)) => {
                warn!("Gateway connection failed: {e}");
                session = None;
                if wait_before_retry(&mut backoff, &shutdown).await.is_break() {
                    return Ok(());
                }
                continue;
            }
        };

        match pump_events(connection, &bot, &shutdown).await {
            Outcome::Shutdown => return Ok(()),
            Outcome::Reconnect(next_session) => {
                // A clean session hand-off means the connection was healthy, so
                // don't hold a stale backoff against it.
                backoff = INITIAL_RECONNECT_BACKOFF;
                rejected_sessions = 0;
                session = next_session;
            }
            Outcome::Rejected => {
                rejected_sessions += 1;
                if rejected_sessions >= MAX_REJECTED_SESSIONS {
                    return Err(BotError::Auth(
                        "Discord rejected the session repeatedly; check the bot token and intents"
                            .to_string(),
                    ));
                }

                session = None;
                if wait_before_retry(&mut backoff, &shutdown).await.is_break() {
                    return Ok(());
                }
            }
        }
    }
}

/// Why the event pump gave up on a connection.
enum Outcome {
    /// Shutdown was requested.
    Shutdown,
    /// Reconnect, resuming the carried session when there is one.
    Reconnect(Option<Session>),
    /// Discord threw the session out; reconnect from scratch after a backoff.
    Rejected,
}

async fn pump_events(
    mut connection: GatewayConnection,
    bot: &Arc<BotState>,
    shutdown: &Shutdown,
) -> Outcome {
    loop {
        let event = match race_shutdown(connection.recv(), shutdown).await {
            Shutdownable::Shutdown => {
                let _ = connection.close().await;
                return Outcome::Shutdown;
            }
            Shutdownable::Value(event) => event,
        };

        match event {
            Ok(GatewayEvent::Ready) => info!("Discord gateway ready"),
            Ok(GatewayEvent::Resumed) => info!("Discord gateway session resumed"),
            Ok(GatewayEvent::InteractionCreate(interaction)) => {
                let bot = Arc::clone(bot);
                spawn(async move {
                    if let Err(e) = handle_interaction(&bot, *interaction).await {
                        error!("Interaction error: {e}");
                    }
                })
                .detach();
            }
            Ok(GatewayEvent::MessageCreate(message)) => {
                let bot = Arc::clone(bot);
                spawn(async move {
                    if let Err(e) = handle_message(&bot, *message).await {
                        error!("Message error: {e}");
                    }
                })
                .detach();
            }
            Ok(GatewayEvent::Reconnect) => {
                debug!("Discord asked us to reconnect");
                return Outcome::Reconnect(connection.session());
            }
            Ok(GatewayEvent::InvalidSession { resumable }) => {
                if resumable {
                    debug!("Discord invalidated the session; resuming");
                    return Outcome::Reconnect(connection.session());
                }
                warn!("Discord invalidated the session; starting a new one");
                return Outcome::Rejected;
            }
            Err(e) => {
                warn!("Gateway connection lost: {e}");
                return Outcome::Reconnect(connection.session());
            }
        }
    }
}

async fn handle_interaction(bot: &BotState, interaction: Interaction) -> Result<(), BotError> {
    if interaction.interaction_type == InteractionType::Ping {
        return bot
            .client
            .respond_interaction(
                &interaction.id,
                &interaction.token,
                INTERACTION_PONG,
                serde_json::json!({}),
            )
            .await;
    }

    // Route before building a context, so events with no handler cost nothing.
    let handler = match (interaction.interaction_type, &interaction.data) {
        (
            InteractionType::ApplicationCommand,
            Some(InteractionData::ApplicationCommand { name, .. }),
        ) => bot.builder.route(Event::Command(name)),
        (
            InteractionType::MessageComponent,
            Some(InteractionData::MessageComponent { custom_id, .. }),
        ) => bot.builder.route(Event::Button(custom_id)),
        _ => None,
    };

    let Some(handler) = handler.cloned() else {
        return Ok(());
    };

    let is_component = interaction.interaction_type == InteractionType::MessageComponent;
    let data = DiscordContextData::new(interaction, bot.client.clone());
    // The interaction has to outlive the handler to answer it.
    let interaction_id = data.interaction().id.clone();
    let interaction_token = data.interaction().token.clone();
    let channel_id = data.channel_id_opt().map(str::to_owned);

    let response = handler.call(Context::new(data)).await;

    send_interaction_response(
        &bot.client,
        &interaction_id,
        &interaction_token,
        channel_id.as_deref(),
        is_component,
        response,
    )
    .await
}

async fn handle_message(bot: &BotState, message: Message) -> Result<(), BotError> {
    let Some(handler) = bot.builder.route(Event::Message).cloned() else {
        return Ok(());
    };

    let channel_id = message.channel_id.clone();
    let data = MessageContextData::new(message, bot.client.clone());
    let response = handler.call(Context::new(data)).await;

    send_channel_response(&bot.client, &channel_id, response).await
}

async fn send_interaction_response(
    client: &DiscordClient,
    interaction_id: &str,
    interaction_token: &str,
    channel_id: Option<&str>,
    is_component: bool,
    mut response: Response,
) -> Result<(), BotError> {
    // Discord shows the user "This interaction failed" unless it hears back
    // within three seconds, so even "no reply" has to be acknowledged.
    if response.is_empty() {
        if is_component {
            // Silently acknowledges without touching the message.
            return client
                .respond_interaction(
                    interaction_id,
                    interaction_token,
                    DEFERRED_UPDATE_MESSAGE,
                    serde_json::json!({}),
                )
                .await;
        }

        // Application commands have no silent acknowledgement: Discord requires
        // a visible reply. Defer so the user sees a pending state rather than a
        // failure, and tell the developer their handler needs to return one.
        warn!(
            "command handler returned an empty response; Discord requires a reply, \
             so the interaction was deferred instead"
        );
        return client
            .respond_interaction(
                interaction_id,
                interaction_token,
                DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE,
                serde_json::json!({}),
            )
            .await;
    }

    if response.is_acknowledge() {
        return client
            .respond_interaction(
                interaction_id,
                interaction_token,
                DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE,
                serde_json::json!({}),
            )
            .await;
    }

    // Attachments can't ride on the interaction callback, so acknowledge first
    // and upload through the follow-up webhook.
    if let Some(file) = response.take_file() {
        if let Some(channel_id) = channel_id {
            let _ = client.trigger_typing(channel_id).await;
        }

        client
            .respond_interaction(
                interaction_id,
                interaction_token,
                DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE,
                serde_json::json!({}),
            )
            .await?;

        return client
            .send_followup_file(
                interaction_token,
                file.file,
                file.filename.as_deref().unwrap_or("file"),
                file.caption.as_deref(),
            )
            .await;
    }

    let mut data = message_payload(&response)?;
    if response.is_ephemeral() {
        data["flags"] = serde_json::json!(EPHEMERAL_FLAG);
    }

    client
        .respond_interaction(
            interaction_id,
            interaction_token,
            CHANNEL_MESSAGE_WITH_SOURCE,
            data,
        )
        .await
}

async fn send_channel_response(
    client: &DiscordClient,
    channel_id: &str,
    mut response: Response,
) -> Result<(), BotError> {
    if response.is_empty() || response.is_acknowledge() {
        return Ok(());
    }

    if let Some(file) = response.take_file() {
        let _ = client.trigger_typing(channel_id).await;
        return client
            .send_file(
                channel_id,
                file.file,
                file.filename.as_deref().unwrap_or("file"),
                file.caption.as_deref(),
            )
            .await;
    }

    let payload = message_payload(&response)?;
    client.send_message_payload(channel_id, &payload).await
}

/// Render the shared message fields Discord accepts on both endpoints.
fn message_payload(response: &Response) -> Result<serde_json::Value, BotError> {
    let mut data = serde_json::json!({ "content": response.content().unwrap_or("") });

    let embeds = response.embeds();
    if !embeds.is_empty() {
        data["embeds"] = serde_json::to_value(embeds)
            .map_err(|e| BotError::Other(format!("failed to serialize embeds: {e}")))?;
    }

    let components = crate::types::component::to_discord(response.components());
    if !components.is_empty() {
        data["components"] = serde_json::Value::Array(components);
    }

    Ok(data)
}

enum Shutdownable<T> {
    Shutdown,
    Value(T),
}

/// Await `future`, giving up early if shutdown is requested.
async fn race_shutdown<F: Future>(future: F, shutdown: &Shutdown) -> Shutdownable<F::Output> {
    futures_lite::future::or(async { Shutdownable::Value(future.await) }, async {
        shutdown.wait().await;
        Shutdownable::Shutdown
    })
    .await
}

/// Sleep out the current backoff and double it, unless shutdown intervenes.
async fn wait_before_retry(backoff: &mut Duration, shutdown: &Shutdown) -> ControlFlow<()> {
    let wait = *backoff;
    *backoff = (*backoff * 2).min(MAX_RECONNECT_BACKOFF);

    debug!("Reconnecting to Discord in {wait:?}");
    match race_shutdown(Timer::after(wait), shutdown).await {
        Shutdownable::Shutdown => ControlFlow::Break(()),
        Shutdownable::Value(_) => ControlFlow::Continue(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use botkit_core::types::{ActionRow, Button, Component, Embed};

    #[test]
    fn plain_text_payloads_carry_only_content() {
        let payload = message_payload(&Response::text("hi")).unwrap();
        assert_eq!(payload, serde_json::json!({ "content": "hi" }));
    }

    #[test]
    fn embeds_and_components_are_serialized_when_present() {
        let response = Response::text("hi")
            .with_embed(Embed::new().title("T"))
            .with_components(vec![Component::ActionRow(ActionRow::buttons(vec![
                Button::primary("id", "Label"),
            ]))]);

        let payload = message_payload(&response).unwrap();
        assert_eq!(payload["content"], "hi");
        assert_eq!(payload["embeds"][0]["title"], "T");
        // Discord wants numeric component codes, not botkit's own tags.
        assert_eq!(payload["components"][0]["type"], 1);
        assert_eq!(payload["components"][0]["components"][0]["type"], 2);
        assert_eq!(payload["components"][0]["components"][0]["custom_id"], "id");
    }

    #[test]
    fn embedless_responses_omit_the_keys_entirely() {
        let payload = message_payload(&Response::text("hi")).unwrap();
        assert!(payload.get("embeds").is_none());
        assert!(payload.get("components").is_none());
    }

    #[test]
    fn interaction_response_codes_match_discords_wire_values() {
        // An empty response still has to be acknowledged, and which code does
        // that silently depends on the interaction kind.
        assert_eq!(INTERACTION_PONG, 1);
        assert_eq!(CHANNEL_MESSAGE_WITH_SOURCE, 4);
        assert_eq!(DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE, 5);
        assert_eq!(DEFERRED_UPDATE_MESSAGE, 6);
    }

    #[test]
    fn ephemeral_is_a_message_flag_not_a_payload_field() {
        // `message_payload` is shared with the channel endpoint, which has no
        // ephemeral concept; the flag is added by the interaction path.
        let response = Response::text("hi").ephemeral();
        let payload = message_payload(&response).unwrap();
        assert!(payload.get("flags").is_none());
        assert!(response.is_ephemeral());
        assert_eq!(EPHEMERAL_FLAG, 64);
    }
}
