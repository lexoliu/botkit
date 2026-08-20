use std::sync::Arc;

use botkit_core::{Bot, BotBuilder, BotError, Context, Event, IntoHandler, Response, Shutdown};
use matrix_sdk::config::SyncSettings;
use matrix_sdk::ruma::api::client::session::get_login_types::v3::LoginType;
use matrix_sdk::ruma::events::reaction::OriginalSyncReactionEvent;
use matrix_sdk::ruma::events::room::member::StrippedRoomMemberEvent;
use matrix_sdk::ruma::events::room::message::OriginalSyncRoomMessageEvent;
use matrix_sdk::{Client, LoopCtrl, Room, RoomState};
use tracing::{error, info, warn};

use crate::client::MatrixClient;
use crate::config::{MatrixAuth, MatrixConfig};
use crate::event::{MatrixContextData, reaction_button_id};

/// Matrix bot builder
///
/// Create a bot with command and reaction handlers, then call `run()` to start.
///
/// # Example
/// ```ignore
/// use botkit_matrix::{MatrixBot, MatrixConfig};
/// use botkit_core::{Bot, User};
///
/// async fn ping() -> &'static str { "Pong!" }
/// async fn greet(user: User) -> String { format!("Hello, {}!", user.name) }
///
/// let config = MatrixConfig::new("https://matrix.org")
///     .password_auth("@bot:matrix.org", "password")
///     .command_prefix("!");
///
/// MatrixBot::new(config)
///     .command("ping", ping)
///     .command("greet", greet)
///     .run()
///     .await
///     .unwrap();
/// ```
pub struct MatrixBot {
    config: MatrixConfig,
    builder: BotBuilder,
}

impl MatrixBot {
    /// Create a new Matrix bot with the given configuration
    pub fn new(config: MatrixConfig) -> Self {
        Self {
            config,
            builder: BotBuilder::new(),
        }
    }

    /// Register a command handler (e.g., !ping, !help)
    ///
    /// Commands are parsed from message text using the configured prefix.
    pub fn command<H, Args>(mut self, name: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.command(name, handler);
        self
    }

    /// Register a command handler with description
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

    /// Register a reaction handler
    ///
    /// Reactions are routed through the same table as buttons, under the id
    /// `reaction:<emoji>`, so a single handler can serve a Discord button and a
    /// Matrix reaction.
    pub fn reaction<H, Args>(mut self, emoji: impl AsRef<str>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self
            .builder
            .button(reaction_button_id(emoji.as_ref()), handler);
        self
    }

    /// Register a message handler (for non-command messages)
    pub fn message<H, Args>(mut self, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.message(handler);
        self
    }

    /// Build and connect the Matrix client
    async fn build_client(&self) -> Result<Client, BotError> {
        #[cfg(not(target_arch = "wasm32"))]
        let client_builder = {
            let client_builder = Client::builder().homeserver_url(&self.config.homeserver_url);

            match &self.config.state_store_path {
                Some(path) => client_builder.sqlite_store(path, None),
                None => client_builder,
            }
        };

        #[cfg(target_arch = "wasm32")]
        let client_builder = Client::builder().homeserver_url(&self.config.homeserver_url);

        let client = client_builder
            .build()
            .await
            .map_err(|e| BotError::Connection(e.to_string()))?;

        match &self.config.auth {
            MatrixAuth::Password { user_id, password } => {
                if user_id.is_empty() {
                    return Err(BotError::Auth(
                        "no credentials configured; call password_auth or access_token_auth"
                            .to_string(),
                    ));
                }

                let login_types = client
                    .matrix_auth()
                    .get_login_types()
                    .await
                    .map_err(|e| BotError::Auth(e.to_string()))?;

                if !login_types
                    .flows
                    .iter()
                    .any(|f| matches!(f, LoginType::Password(_)))
                {
                    return Err(BotError::Auth(
                        "Homeserver does not support password login".to_string(),
                    ));
                }

                let mut login = client.matrix_auth().login_username(user_id, password);
                if let Some(device_name) = &self.config.device_name {
                    login = login.initial_device_display_name(device_name);
                }

                login.await.map_err(|e| BotError::Auth(e.to_string()))?;
                info!("Logged in as {user_id}");
            }
            MatrixAuth::AccessToken {
                user_id,
                access_token,
                device_id,
            } => {
                use matrix_sdk::authentication::matrix::MatrixSession;
                use matrix_sdk::{SessionMeta, SessionTokens};

                let session = MatrixSession {
                    meta: SessionMeta {
                        user_id: user_id.clone(),
                        device_id: device_id.clone(),
                    },
                    tokens: SessionTokens {
                        access_token: access_token.clone(),
                        refresh_token: None,
                    },
                };

                client
                    .restore_session(session)
                    .await
                    .map_err(|e| BotError::Auth(e.to_string()))?;

                info!("Restored session for {user_id}");
            }
        }

        Ok(client)
    }
}

impl Bot for MatrixBot {
    async fn run_until(self, shutdown: Shutdown) -> Result<(), BotError> {
        let client = self.build_client().await?;
        let matrix_client = MatrixClient::new(client.clone());

        let bot = Arc::new(BotState {
            builder: self.builder,
            client: matrix_client,
            command_prefix: self.config.command_prefix.clone(),
        });

        register_handlers(&client, &bot, self.config.auto_join_rooms);

        // The first sync catches up on room state without replaying the entire
        // backlog through the handlers registered above.
        info!("Starting initial sync...");
        client
            .sync_once(SyncSettings::default())
            .await
            .map_err(|e| BotError::Connection(e.to_string()))?;

        info!("Matrix bot connected and syncing");

        // `sync_with_callback` gives us a checkpoint between batches, which is
        // where a shutdown request can take effect.
        client
            .sync_with_callback(SyncSettings::default(), |_| {
                let shutdown = shutdown.clone();
                async move {
                    if shutdown.is_shutdown() {
                        LoopCtrl::Break
                    } else {
                        LoopCtrl::Continue
                    }
                }
            })
            .await
            .map_err(|e| BotError::Connection(e.to_string()))?;

        Ok(())
    }
}

fn register_handlers(client: &Client, bot: &Arc<BotState>, auto_join_rooms: bool) {
    let message_bot = Arc::clone(bot);
    client.add_event_handler(move |event: OriginalSyncRoomMessageEvent, room: Room| {
        let bot = Arc::clone(&message_bot);
        async move {
            if !is_actionable(&room, &event.sender) {
                return;
            }
            if let Err(e) = handle_message(&bot, &event, room).await {
                error!("Error handling message: {e}");
            }
        }
    });

    let reaction_bot = Arc::clone(bot);
    client.add_event_handler(move |event: OriginalSyncReactionEvent, room: Room| {
        let bot = Arc::clone(&reaction_bot);
        async move {
            if !is_actionable(&room, &event.sender) {
                return;
            }
            if let Err(e) = handle_reaction(&bot, &event, room).await {
                error!("Error handling reaction: {e}");
            }
        }
    });

    if auto_join_rooms {
        client.add_event_handler(
            |event: StrippedRoomMemberEvent, room: Room, client: Client| async move {
                // Only act on invites addressed to this bot.
                if client.user_id() != Some(&event.state_key) || room.state() != RoomState::Invited
                {
                    return;
                }

                info!("Joining room {}", room.room_id());
                if let Err(e) = room.join().await {
                    warn!("Failed to join room {}: {e}", room.room_id());
                }
            },
        );
    }
}

/// Events from rooms we haven't joined, or from the bot itself, are noise.
fn is_actionable(room: &Room, sender: &matrix_sdk::ruma::UserId) -> bool {
    room.state() == RoomState::Joined && room.client().user_id() != Some(sender)
}

/// Shared, immutable state each dispatched event needs.
struct BotState {
    builder: BotBuilder,
    client: MatrixClient,
    command_prefix: String,
}

async fn handle_message(
    bot: &BotState,
    event: &OriginalSyncRoomMessageEvent,
    room: Room,
) -> Result<(), BotError> {
    // The context owns the command parsing, so routing and the `CommandName`
    // extractor can never disagree about where a command ends.
    let data = MatrixContextData::from_message(
        event,
        room.clone(),
        bot.client.clone(),
        &bot.command_prefix,
    );

    // Not a text message at all.
    if data.message_text().is_none() {
        return Ok(());
    }

    let event = match data.command() {
        Some(command) => Event::Command(command),
        None => Event::Message,
    };

    let Some(handler) = bot.builder.route(event).cloned() else {
        return Ok(());
    };

    let response = handler.call(Context::new(data)).await;
    send_response(&bot.client, &room, response).await
}

async fn handle_reaction(
    bot: &BotState,
    event: &OriginalSyncReactionEvent,
    room: Room,
) -> Result<(), BotError> {
    let button_id = reaction_button_id(&event.content.relates_to.key);

    let Some(handler) = bot.builder.route(Event::Button(&button_id)).cloned() else {
        return Ok(());
    };

    let data = MatrixContextData::from_reaction(event, room.clone(), bot.client.clone());
    let response = handler.call(Context::new(data)).await;
    send_response(&bot.client, &room, response).await
}

async fn send_response(
    client: &MatrixClient,
    room: &Room,
    mut response: Response,
) -> Result<(), BotError> {
    if response.is_empty() || response.is_acknowledge() {
        return Ok(());
    }

    if let Some(file) = response.take_file() {
        let filename = file.filename.as_deref().unwrap_or("file").to_owned();
        let bytes = file
            .file
            .read()
            .await
            .map_err(|e| BotError::Other(format!("failed to read attachment: {e}")))?;

        client.send_file(room, &filename, bytes).await?;

        // Matrix carries no caption on an attachment, so send it as its own
        // message rather than dropping it.
        if let Some(caption) = file.caption {
            client.send_message(room, &caption).await?;
        }

        return Ok(());
    }

    let content = response.content().unwrap_or("");
    let embeds = response.embeds();
    let components = response.components();

    // Matrix has no interactive components; render buttons as links so a
    // response built for Discord still says something useful here.
    let html = render_html(content, embeds, components);

    match html {
        Some(html) => client.send_formatted_message(room, content, &html).await?,
        None if !content.is_empty() => client.send_message(room, content).await?,
        None => return Ok(()),
    };

    Ok(())
}

/// Render the parts of a response Matrix can only express as formatted text.
///
/// Returns `None` when plain text says everything the response carries.
fn render_html(
    text: &str,
    embeds: &[botkit_core::types::Embed],
    components: &[botkit_core::types::Component],
) -> Option<String> {
    let links = component_links(components);
    if embeds.is_empty() && links.is_empty() {
        return None;
    }

    let mut html = String::new();

    if !text.is_empty() {
        html.push_str("<p>");
        escape_html_into(&mut html, text);
        html.push_str("</p>");
    }

    for embed in embeds {
        html.push_str("<blockquote>");

        if let Some(title) = &embed.title {
            html.push_str("<strong>");
            escape_html_into(&mut html, title);
            html.push_str("</strong><br/>");
        }

        if let Some(description) = &embed.description {
            escape_html_into(&mut html, description);
            html.push_str("<br/>");
        }

        for field in &embed.fields {
            html.push_str("<em>");
            escape_html_into(&mut html, &field.name);
            html.push_str(":</em> ");
            escape_html_into(&mut html, &field.value);
            html.push_str("<br/>");
        }

        if let Some(footer) = &embed.footer {
            html.push_str("<small>");
            escape_html_into(&mut html, &footer.text);
            html.push_str("</small>");
        }

        html.push_str("</blockquote>");
    }

    if !links.is_empty() {
        html.push_str("<ul>");
        for (label, url) in links {
            html.push_str("<li><a href=\"");
            escape_html_into(&mut html, url);
            html.push_str("\">");
            escape_html_into(&mut html, label);
            html.push_str("</a></li>");
        }
        html.push_str("</ul>");
    }

    Some(html)
}

/// Collect the link buttons out of a component tree; the rest have no Matrix
/// equivalent and are dropped.
fn component_links(components: &[botkit_core::types::Component]) -> Vec<(&str, &str)> {
    use botkit_core::types::Component;

    fn collect<'a>(components: &'a [Component], out: &mut Vec<(&'a str, &'a str)>) {
        for component in components {
            match component {
                Component::ActionRow(row) => collect(&row.components, out),
                Component::Button(button) => {
                    if let Some(url) = &button.url {
                        out.push((button.label.as_str(), url.as_str()));
                    }
                }
                Component::SelectMenu(_) => {}
            }
        }
    }

    let mut links = Vec::new();
    collect(components, &mut links);
    links
}

/// Append `s` to `out` with HTML metacharacters escaped, in a single pass.
fn escape_html_into(out: &mut String, s: &str) {
    out.reserve(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use botkit_core::types::{ActionRow, Button, Component, Embed};

    fn escape(s: &str) -> String {
        let mut out = String::new();
        escape_html_into(&mut out, s);
        out
    }

    #[test]
    fn escaping_covers_every_metacharacter() {
        assert_eq!(
            escape(r#"<a href="x">&'</a>"#),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;"
        );
    }

    #[test]
    fn escaping_leaves_ordinary_text_alone() {
        assert_eq!(escape("hello 🎉 world"), "hello 🎉 world");
    }

    #[test]
    fn plain_text_needs_no_html() {
        assert_eq!(render_html("hi", &[], &[]), None);
    }

    #[test]
    fn embeds_render_as_blockquotes() {
        let embed = Embed::new()
            .title("Title")
            .description("Body")
            .field("Key", "Value", false)
            .footer("Footer");

        let html = render_html("Intro", std::slice::from_ref(&embed), &[]).expect("html");
        assert_eq!(
            html,
            "<p>Intro</p><blockquote><strong>Title</strong><br/>Body<br/>\
             <em>Key:</em> Value<br/><small>Footer</small></blockquote>"
        );
    }

    #[test]
    fn embed_content_is_escaped() {
        let embed = Embed::new().description("<script>alert(1)</script>");
        let html = render_html("", std::slice::from_ref(&embed), &[]).expect("html");
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn link_buttons_become_a_list() {
        let components = vec![Component::ActionRow(ActionRow::buttons(vec![
            Button::link("https://example.com", "Docs"),
            // Callback buttons have nothing to point at on Matrix.
            Button::primary("noop", "Press"),
        ]))];

        let html = render_html("See:", &[], &components).expect("html");
        assert_eq!(
            html,
            "<p>See:</p><ul><li><a href=\"https://example.com\">Docs</a></li></ul>"
        );
    }

    #[test]
    fn callback_only_components_need_no_html() {
        let components = vec![Component::Button(Button::primary("noop", "Press"))];
        assert_eq!(render_html("hi", &[], &components), None);
    }

    #[test]
    fn reaction_ids_match_button_registration() {
        assert_eq!(reaction_button_id("👍"), "reaction:👍");
    }
}
