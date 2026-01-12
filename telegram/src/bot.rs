use std::sync::Arc;

use botkit_core::{BotBuilder, BotError, Context, ContextData, IntoHandler, Response};
use executor_core::spawn;
use http_kit::{Body, Endpoint, HttpError, Request, Response as HttpResponse, StatusCode};

use crate::client::TelegramClient;
use crate::event::TelegramContextData;
use crate::types::{
    BotCommand, InlineKeyboardButton, InlineKeyboardMarkup, ReplyMarkup, Update, UpdateKind,
};

/// Error type for the Telegram webhook endpoint
#[derive(Debug)]
pub struct WebhookError(BotError);

impl std::fmt::Display for WebhookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for WebhookError {}

impl HttpError for WebhookError {
    fn status(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// Telegram bot builder
///
/// Create a bot with command and button handlers, then call `build()` to get
/// a webhook handler that implements skyzen's `Endpoint` trait.
///
/// # Example
/// ```ignore
/// use botkit_core::User;
/// use botkit_telegram::{TelegramBot, TelegramWebhook};
///
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
/// // Return TelegramWebhook directly - it implements Endpoint
/// #[skyzen::main]
/// fn main() -> TelegramWebhook {
///     TelegramBot::new(token)
///         .command("ping", ping)
///         .command("greet", greet)
///         .build()
/// }
/// ```
pub struct TelegramBot {
    token: String,
    builder: BotBuilder,
}

impl TelegramBot {
    /// Create a new Telegram bot
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            builder: BotBuilder::new(),
        }
    }

    /// Register a command handler (e.g., /start, /help)
    pub fn command<H, Args>(mut self, name: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.command(name, handler);
        self
    }

    /// Register a command handler with description
    ///
    /// The description appears in Telegram's command menu (slash command suggestions).
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

    /// Register a button handler (callback query data pattern)
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
    pub fn message<H, Args>(mut self, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.message(handler);
        self
    }

    /// Build the webhook handler (for use with skyzen's Endpoint)
    pub fn build(self) -> TelegramWebhook {
        TelegramWebhook {
            client: TelegramClient::new(&self.token),
            builder: Arc::new(self.builder),
        }
    }

    /// Run the bot using long-polling (no server needed)
    ///
    /// This method polls Telegram's API for updates and processes them
    /// using the registered handlers. It runs forever until interrupted.
    ///
    /// # Example
    /// ```ignore
    /// use botkit_telegram::TelegramBot;
    ///
    /// async fn ping() -> &'static str { "Pong!" }
    ///
    /// TelegramBot::new(token)
    ///     .command("ping", ping)
    ///     .run_polling()
    ///     .await;
    /// ```
    pub async fn run_polling(self) -> Result<(), BotError> {
        let client = TelegramClient::new(&self.token);

        // Register commands with Telegram's Bot API for slash command menu
        let commands: Vec<BotCommand> = self
            .builder
            .commands()
            .map(|(name, desc)| BotCommand::new(name, if desc.is_empty() { name } else { desc }))
            .collect();

        if !commands.is_empty() {
            if let Err(e) = client.set_my_commands(&commands).await {
                eprintln!("Warning: Failed to register commands: {}", e);
            }
        }

        let builder = Arc::new(self.builder);

        // Delete any existing webhook to enable polling
        client.delete_webhook().await?;

        let mut offset: Option<i64> = None;

        loop {
            match client.get_updates(offset, Some(30)).await {
                Ok(updates) => {
                    for update in updates {
                        offset = Some(update.update_id + 1);
                        // Process update directly (no spawn needed in polling mode)
                        if let Err(e) = process_update_sync(&client, &builder, update).await {
                            eprintln!("Error handling update: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error fetching updates: {}", e);
                }
            }
        }
    }
}

/// Telegram webhook handler
///
/// Handles incoming webhook updates from Telegram. Use with a skyzen router.
#[derive(Clone)]
pub struct TelegramWebhook {
    client: TelegramClient,
    builder: Arc<BotBuilder>,
}

impl TelegramWebhook {
    /// Get the client for making API calls
    pub fn client(&self) -> &TelegramClient {
        &self.client
    }

    /// Handle a webhook update
    ///
    /// This is the main entry point for processing Telegram updates.
    pub async fn handle(&self, update: Update) -> Result<(), BotError> {
        self.handle_update(update).await
    }

    /// Handle a webhook update (internal)
    async fn handle_update(&self, update: Update) -> Result<(), BotError> {
        // Determine event type and value for routing
        let (event_type, value) = match &update.kind {
            UpdateKind::Message(msg) => {
                // Check if it's a command
                let data = TelegramContextData::new(update.clone(), self.client.clone());
                if let Some(cmd_name) = data.command_name() {
                    ("command", cmd_name.to_string())
                } else {
                    ("message", msg.text.clone().unwrap_or_default())
                }
            }
            UpdateKind::CallbackQuery(cq) => ("button", cq.data.clone().unwrap_or_default()),
            _ => return Ok(()),
        };

        // Find matching handler
        let handler = self.builder.find_handler(event_type, &value);

        if let Some(handler) = handler {
            let data = TelegramContextData::new(update.clone(), self.client.clone());
            let ctx = Context::new(data);

            // Spawn handler as a separate task
            let client = self.client.clone();
            spawn(async move {
                let response = handler.call(ctx).await;
                if let Err(e) = send_response(&client, &update, response).await {
                    eprintln!("Telegram response error: {}", e);
                }
            })
            .detach();
        }

        Ok(())
    }
}

/// Process update synchronously (for polling mode - no spawn)
async fn process_update_sync(
    client: &TelegramClient,
    builder: &BotBuilder,
    update: Update,
) -> Result<(), BotError> {
    let (event_type, value) = match &update.kind {
        UpdateKind::Message(msg) => {
            let data = TelegramContextData::new(update.clone(), client.clone());
            if let Some(cmd_name) = data.command_name() {
                ("command", cmd_name.to_string())
            } else {
                ("message", msg.text.clone().unwrap_or_default())
            }
        }
        UpdateKind::CallbackQuery(cq) => ("button", cq.data.clone().unwrap_or_default()),
        _ => return Ok(()),
    };

    if let Some(handler) = builder.find_handler(event_type, &value) {
        let data = TelegramContextData::new(update.clone(), client.clone());
        let ctx = Context::new(data);
        let response = handler.call(ctx).await;
        send_response(client, &update, response).await?;
    }

    Ok(())
}

async fn send_response(
    client: &TelegramClient,
    update: &Update,
    mut response: Response,
) -> Result<(), BotError> {
    if response.is_empty() || response.is_acknowledge() {
        return Ok(());
    }

    // Get chat ID from update
    let chat_id = match &update.kind {
        UpdateKind::Message(m) | UpdateKind::EditedMessage(m) => m.chat.id,
        UpdateKind::CallbackQuery(cq) => cq.message.as_ref().map(|m| m.chat.id).unwrap_or(0),
        _ => return Ok(()),
    };

    if chat_id == 0 {
        return Ok(());
    }

    // Handle file response
    if response.is_file() {
        if let Some(file_response) = response.take_file() {
            // Show upload indicator
            let _ = client.send_chat_action(chat_id, "upload_document").await;

            return client
                .send_document(
                    chat_id,
                    file_response.file,
                    file_response.filename.as_deref(),
                    file_response.caption.as_deref(),
                )
                .await;
        }
    }

    // Handle text response
    let content = response.content().unwrap_or("");
    if content.is_empty() {
        return Ok(());
    }

    // Build reply markup from components
    let reply_markup = build_reply_markup(&response);

    client.send_message(chat_id, content, reply_markup).await
}

fn build_reply_markup(response: &Response) -> Option<ReplyMarkup> {
    use botkit_core::types::component::Component;

    let components = response.components();
    if components.is_empty() {
        return None;
    }

    // Convert components to Telegram inline keyboard
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    for component in components {
        match component {
            Component::ActionRow(action_row) => {
                let row: Vec<InlineKeyboardButton> = action_row
                    .components
                    .iter()
                    .filter_map(|c| match c {
                        Component::Button(btn) => {
                            if let Some(url) = &btn.url {
                                Some(InlineKeyboardButton::url(&btn.label, url))
                            } else if let Some(custom_id) = &btn.custom_id {
                                Some(InlineKeyboardButton::callback(&btn.label, custom_id))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .collect();

                if !row.is_empty() {
                    rows.push(row);
                }
            }
            Component::Button(btn) => {
                let button = if let Some(url) = &btn.url {
                    InlineKeyboardButton::url(&btn.label, url)
                } else if let Some(custom_id) = &btn.custom_id {
                    InlineKeyboardButton::callback(&btn.label, custom_id)
                } else {
                    continue;
                };
                rows.push(vec![button]);
            }
            _ => {}
        }
    }

    if rows.is_empty() {
        None
    } else {
        Some(ReplyMarkup::InlineKeyboard(InlineKeyboardMarkup {
            inline_keyboard: rows,
        }))
    }
}

/// Implement Endpoint for TelegramWebhook so it can be used directly with skyzen
///
/// This allows users to return TelegramWebhook from `#[skyzen::main]` without
/// manually building a router.
impl Endpoint for TelegramWebhook {
    type Error = WebhookError;

    async fn respond(&mut self, request: &mut Request) -> Result<HttpResponse, Self::Error> {
        // Parse the request body as a Telegram Update
        let update: Update = request
            .body_mut()
            .into_json()
            .await
            .map_err(|e| WebhookError(BotError::Other(e.to_string())))?;

        // Handle the update
        self.handle(update).await.map_err(WebhookError)?;

        // Return 200 OK
        Ok(HttpResponse::new(Body::from_bytes("OK")))
    }
}
