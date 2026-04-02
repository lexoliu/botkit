use std::sync::Arc;

use botkit_core::{BotBuilder, BotError, Context, IntoHandler, Response};
use matrix_sdk::config::SyncSettings;
use matrix_sdk::ruma::api::client::session::get_login_types::v3::LoginType;
use matrix_sdk::ruma::events::reaction::OriginalSyncReactionEvent;
use matrix_sdk::ruma::events::room::member::StrippedRoomMemberEvent;
use matrix_sdk::ruma::events::room::message::{MessageType, OriginalSyncRoomMessageEvent};
use matrix_sdk::{Client, Room, RoomState};

use crate::client::MatrixClient;
use crate::config::{MatrixAuth, MatrixConfig};
use crate::event::MatrixContextData;

/// Matrix bot builder
///
/// Create a bot with command and reaction handlers, then call `run()` to start.
///
/// # Example
/// ```ignore
/// use botkit_matrix::{MatrixBot, MatrixConfig};
/// use botkit_core::extractor::User;
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
    /// Maps emoji reactions to button-like callbacks.
    /// The key can be Unicode emoji (e.g., "👍") or the emoji will be matched directly.
    pub fn reaction<H, Args>(mut self, emoji: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        // Map reaction to button handler for unified pattern
        let button_id = format!("reaction:{}", emoji.into());
        self.builder = self.builder.button(button_id, handler);
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

    /// Run the bot using Matrix sync loop
    ///
    /// This connects to the homeserver, authenticates, and processes events
    /// using the registered handlers. It runs forever until interrupted.
    pub async fn run(self) -> Result<(), BotError> {
        let client = self.build_client().await?;
        let matrix_client = MatrixClient::new(client.clone());

        // Wrap self in Arc for sharing across handlers
        let bot = Arc::new(BotState {
            builder: self.builder,
            command_prefix: self.config.command_prefix.clone(),
        });

        // Register message event handler
        let bot_clone = Arc::clone(&bot);
        let matrix_client_clone = matrix_client.clone();
        client.add_event_handler(move |event: OriginalSyncRoomMessageEvent, room: Room| {
            let bot = Arc::clone(&bot_clone);
            let matrix_client = matrix_client_clone.clone();
            async move {
                if room.state() != RoomState::Joined {
                    return;
                }

                // Ignore messages from ourselves
                let sdk_client = room.client();
                if sdk_client.user_id().is_some_and(|u| u == event.sender) {
                    return;
                }

                if let Err(e) = handle_message(&bot, &matrix_client, &event, room).await {
                    tracing::error!("Error handling message: {}", e);
                }
            }
        });

        // Register reaction event handler
        let bot_clone = Arc::clone(&bot);
        let matrix_client_clone = matrix_client.clone();
        client.add_event_handler(move |event: OriginalSyncReactionEvent, room: Room| {
            let bot = Arc::clone(&bot_clone);
            let matrix_client = matrix_client_clone.clone();
            async move {
                if room.state() != RoomState::Joined {
                    return;
                }

                // Ignore reactions from ourselves
                let sdk_client = room.client();
                if sdk_client.user_id().is_some_and(|u| u == event.sender) {
                    return;
                }

                if let Err(e) = handle_reaction(&bot, &matrix_client, &event, room).await {
                    tracing::error!("Error handling reaction: {}", e);
                }
            }
        });

        // Auto-join rooms if configured
        if self.config.auto_join_rooms {
            client.add_event_handler(
                |event: StrippedRoomMemberEvent, room: Room, client: Client| async move {
                    // Only handle invites for ourselves
                    if event.state_key != client.user_id().expect("logged in") {
                        return;
                    }

                    if room.state() == RoomState::Invited {
                        tracing::info!("Joining room {}", room.room_id());
                        if let Err(e) = room.join().await {
                            tracing::error!("Failed to join room {}: {}", room.room_id(), e);
                        }
                    }
                },
            );
        }

        // Initial sync to get room state
        tracing::info!("Starting initial sync...");
        client
            .sync_once(SyncSettings::default())
            .await
            .map_err(|e| BotError::Connection(e.to_string()))?;

        tracing::info!("Matrix bot connected and syncing");

        // Run sync loop forever
        client
            .sync(SyncSettings::default())
            .await
            .map_err(|e| BotError::Connection(e.to_string()))?;

        Ok(())
    }

    /// Build and connect the Matrix client
    async fn build_client(&self) -> Result<Client, BotError> {
        #[cfg(not(target_arch = "wasm32"))]
        let client_builder = {
            let client_builder = Client::builder().homeserver_url(&self.config.homeserver_url);

            if let Some(path) = &self.config.state_store_path {
                client_builder.sqlite_store(path, None)
            } else {
                client_builder
            }
        };

        #[cfg(target_arch = "wasm32")]
        let client_builder = Client::builder().homeserver_url(&self.config.homeserver_url);

        let client = client_builder
            .build()
            .await
            .map_err(|e| BotError::Connection(e.to_string()))?;

        // Authenticate based on config
        match &self.config.auth {
            MatrixAuth::Password { user_id, password } => {
                // Check if password login is supported
                let login_types = client
                    .matrix_auth()
                    .get_login_types()
                    .await
                    .map_err(|e| BotError::Auth(e.to_string()))?;

                let supports_password = login_types
                    .flows
                    .iter()
                    .any(|f| matches!(f, LoginType::Password(_)));

                if !supports_password {
                    return Err(BotError::Auth(
                        "Homeserver does not support password login".to_string(),
                    ));
                }

                let mut login_builder = client.matrix_auth().login_username(user_id, password);

                if let Some(device_name) = &self.config.device_name {
                    login_builder = login_builder.initial_device_display_name(device_name);
                }

                login_builder
                    .await
                    .map_err(|e| BotError::Auth(e.to_string()))?;

                tracing::info!("Logged in as {}", user_id);
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

                tracing::info!("Restored session for {}", user_id);
            }
        }

        Ok(client)
    }
}

/// Internal state shared across event handlers
struct BotState {
    builder: BotBuilder,
    command_prefix: String,
}

async fn handle_message(
    bot: &BotState,
    client: &MatrixClient,
    event: &OriginalSyncRoomMessageEvent,
    room: Room,
) -> Result<(), BotError> {
    let MessageType::Text(text_content) = &event.content.msgtype else {
        return Ok(());
    };

    let body = &text_content.body;

    // Determine event type and value for routing
    let (event_type, value) = if body.starts_with(&bot.command_prefix) {
        let without_prefix = &body[bot.command_prefix.len()..];
        let mut parts = without_prefix.splitn(2, char::is_whitespace);
        let cmd_name = parts.next().unwrap_or("");
        ("command", cmd_name.to_string())
    } else {
        ("message", body.clone())
    };

    // Find matching handler
    if let Some(handler) = bot.builder.find_handler(event_type, &value) {
        let data = MatrixContextData::from_message(
            event,
            room.clone(),
            client.clone(),
            &bot.command_prefix,
        );
        let ctx = Context::new(data);

        let response = handler.call(ctx).await;
        send_response(client, &room, response).await?;
    }

    Ok(())
}

async fn handle_reaction(
    bot: &BotState,
    client: &MatrixClient,
    event: &OriginalSyncReactionEvent,
    room: Room,
) -> Result<(), BotError> {
    // Extract reaction key (emoji)
    let emoji = &event.content.relates_to.key;
    let button_id = format!("reaction:{}", emoji);

    if let Some(handler) = bot.builder.find_handler("button", &button_id) {
        let data = MatrixContextData::from_reaction(event, room.clone(), client.clone());
        let ctx = Context::new(data);

        let response = handler.call(ctx).await;
        send_response(client, &room, response).await?;
    }

    Ok(())
}

async fn send_response(
    client: &MatrixClient,
    room: &Room,
    response: Response,
) -> Result<(), BotError> {
    if response.is_empty() || response.is_acknowledge() {
        return Ok(());
    }

    // Get content
    let content = match response.content() {
        Some(c) if !c.is_empty() => c,
        _ => return Ok(()),
    };

    // Check for embeds - convert to formatted message
    let embeds = response.embeds();

    if embeds.is_empty() {
        // Plain text message
        client.send_message(room, content).await?;
    } else {
        // Build HTML from content and embeds
        let html = embeds_to_html(content, embeds);
        client.send_formatted_message(room, content, &html).await?;
    }

    Ok(())
}

fn embeds_to_html(text: &str, embeds: &[botkit_core::types::Embed]) -> String {
    let mut html = String::new();

    if !text.is_empty() {
        html.push_str(&format!("<p>{}</p>", escape_html(text)));
    }

    for embed in embeds {
        html.push_str("<blockquote>");

        if let Some(title) = &embed.title {
            html.push_str(&format!("<strong>{}</strong><br/>", escape_html(title)));
        }

        if let Some(desc) = &embed.description {
            html.push_str(&format!("{}<br/>", escape_html(desc)));
        }

        for field in &embed.fields {
            html.push_str(&format!(
                "<em>{}:</em> {}<br/>",
                escape_html(&field.name),
                escape_html(&field.value)
            ));
        }

        html.push_str("</blockquote>");
    }

    html
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
