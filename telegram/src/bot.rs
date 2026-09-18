use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use botkit_core::{
    Bot, BotBuilder, BotError, Context, ContextData, Event, IntoHandler, Response, Shutdown,
};
use executor_core::spawn;
use http_kit::{Body, Endpoint, HttpError, Request, Response as HttpResponse, StatusCode};
use tracing::{error, warn};

use crate::client::TelegramClient;
use crate::event::TelegramContextData;
use crate::types::{
    BotCommand, Formatted, InlineKeyboardButton, InlineKeyboardMarkup, ReplyMarkup, Update,
    UpdateKind,
};

/// How long Telegram holds a long-poll open before returning empty.
const POLL_TIMEOUT_SECS: u32 = 30;
/// First wait after a failed `getUpdates`; doubles up to the max.
const INITIAL_POLL_BACKOFF: Duration = Duration::from_secs(1);
/// Longest wait between failed `getUpdates` attempts.
const MAX_POLL_BACKOFF: Duration = Duration::from_secs(60);

/// Error type for the Telegram webhook endpoint
#[derive(Debug)]
pub struct WebhookError(BotError);

impl WebhookError {
    /// The underlying bot error
    pub fn into_inner(self) -> BotError {
        self.0
    }
}

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
/// Create a bot with command and button handlers, then either call `build()`
/// for a webhook endpoint or `run()` for long polling.
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
    register_commands: bool,
}

impl TelegramBot {
    /// Create a new Telegram bot
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            builder: BotBuilder::new(),
            register_commands: true,
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

    /// Register a handler for events nothing else claimed
    ///
    /// Unregistered commands and unmatched callbacks reach the fallback
    /// rather than being dropped. See [`BotBuilder::fallback`].
    pub fn fallback<H, Args>(mut self, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.fallback(handler);
        self
    }

    /// Stop publishing the registered commands to Telegram on startup
    ///
    /// On by default for [`TelegramBot::run`]; webhook mode never registers
    /// commands, since `build()` does no I/O.
    pub fn skip_command_registration(mut self) -> Self {
        self.register_commands = false;
        self
    }

    /// Build the webhook handler (for use with skyzen's Endpoint)
    ///
    /// Registering the command menu is a network call, so webhook deployments
    /// should call [`TelegramClient::set_my_commands`] themselves at startup;
    /// [`TelegramBot::commands`] renders the list to pass it.
    pub fn build(self) -> TelegramWebhook {
        TelegramWebhook {
            dispatcher: Arc::new(Dispatcher {
                client: TelegramClient::new(&self.token),
                builder: self.builder,
            }),
        }
    }

    /// The command menu entries implied by the registered handlers
    pub fn commands(&self) -> Vec<BotCommand> {
        self.builder
            .commands()
            .map(|command| {
                // Telegram rejects an empty description, so fall back to the name.
                let description = if command.description.is_empty() {
                    command.name
                } else {
                    command.description
                };
                BotCommand::new(command.name, description)
            })
            .collect()
    }
}

impl Bot for TelegramBot {
    async fn run_until(self, shutdown: Shutdown) -> Result<(), BotError> {
        let client = TelegramClient::new(&self.token);

        if self.register_commands {
            let commands = self.commands();
            if !commands.is_empty()
                && let Err(e) = client.set_my_commands(&commands).await
            {
                warn!("Failed to register commands: {e}");
            }
        }

        // Long polling and webhooks are mutually exclusive on Telegram's side.
        client.delete_webhook().await?;

        let dispatcher = Arc::new(Dispatcher {
            client,
            builder: self.builder,
        });

        poll_updates(dispatcher, shutdown).await
    }
}

async fn poll_updates(dispatcher: Arc<Dispatcher>, shutdown: Shutdown) -> Result<(), BotError> {
    let mut offset: Option<i64> = None;
    let mut backoff = INITIAL_POLL_BACKOFF;

    loop {
        if shutdown.is_shutdown() {
            return Ok(());
        }

        let poll = dispatcher
            .client
            .get_updates(offset, Some(POLL_TIMEOUT_SECS));

        let updates = match race_shutdown(poll, &shutdown).await {
            None => return Ok(()),
            Some(Ok(updates)) => {
                backoff = INITIAL_POLL_BACKOFF;
                updates
            }
            Some(Err(e)) => {
                // Without a backoff a persistent failure (revoked token,
                // network outage) becomes a hot loop against Telegram's API.
                error!("Error fetching updates: {e}");
                if race_shutdown(sleep(backoff), &shutdown).await.is_none() {
                    return Ok(());
                }
                backoff = (backoff * 2).min(MAX_POLL_BACKOFF);
                continue;
            }
        };

        for update in updates {
            // Acknowledge before handling: an update that panics a handler must
            // not be redelivered forever.
            offset = Some(update.update_id + 1);

            if let Err(e) = dispatcher.dispatch(update).await {
                error!("Error handling update: {e}");
            }
        }
    }
}

/// Routes updates to handlers and sends whatever they return.
///
/// Shared by webhook and polling mode so both behave identically.
struct Dispatcher {
    client: TelegramClient,
    builder: BotBuilder,
}

impl Dispatcher {
    /// Handle one update to completion.
    async fn dispatch(&self, update: Update) -> Result<(), BotError> {
        let Some((data, handler)) = self.prepare(update).await? else {
            return Ok(());
        };

        let chat_id = data.chat_id();
        let thread_id = data.thread_id();
        let response = handler.call(Context::new(data)).await;
        send_response(&self.client, chat_id, thread_id, response).await
    }

    /// Handle one update in the background, so a webhook can reply immediately.
    async fn dispatch_detached(self: &Arc<Self>, update: Update) -> Result<(), BotError> {
        let Some((data, handler)) = self.prepare(update).await? else {
            return Ok(());
        };

        let this = Arc::clone(self);
        spawn(async move {
            let chat_id = data.chat_id();
            let thread_id = data.thread_id();
            let response = handler.call(Context::new(data)).await;
            if let Err(e) = send_response(&this.client, chat_id, thread_id, response).await {
                error!("Telegram response error: {e}");
            }
        })
        .detach();

        Ok(())
    }

    /// Acknowledge the update and resolve it to a handler, building the context
    /// exactly once.
    async fn prepare(
        &self,
        update: Update,
    ) -> Result<Option<(TelegramContextData, botkit_core::AnyHandler)>, BotError> {
        // Telegram spins the button until the query is answered, so do it
        // before the handler runs rather than after. A failure here is cosmetic
        // - it must not cost the user their button press.
        if let UpdateKind::CallbackQuery(callback_query) = &update.kind
            && let Err(e) = self
                .client
                .answer_callback_query(&callback_query.id, None, false)
                .await
        {
            warn!("Failed to answer callback query: {e}");
        }

        // Edits and reactions route as messages: they only arrive when the
        // bot opted into them via `allowed_updates`, and the message handler
        // is where a bot would observe them. The handler can tell them apart
        // through `UpdateKind`.
        if !matches!(
            update.kind,
            UpdateKind::Message(_)
                | UpdateKind::EditedMessage(_)
                | UpdateKind::CallbackQuery(_)
                | UpdateKind::MessageReaction(_)
        ) {
            return Ok(None);
        }

        let is_callback = matches!(update.kind, UpdateKind::CallbackQuery(_));
        let data = TelegramContextData::new(update, self.client.clone());

        let event = match (is_callback, data.command_name(), data.button_id()) {
            // A callback carries a button id or nothing we can route on; it is
            // never a message, so don't let it fall through to the catch-all.
            (true, _, Some(button)) => Event::Button(button),
            (true, _, None) => return Ok(None),
            (false, Some(command), _) => Event::Command(command),
            (false, None, _) => Event::Message,
        };

        Ok(self
            .builder
            .route(event)
            .cloned()
            .map(|handler| (data, handler)))
    }
}

/// Telegram webhook handler
///
/// Handles incoming webhook updates from Telegram. Use with a skyzen router.
#[derive(Clone)]
pub struct TelegramWebhook {
    dispatcher: Arc<Dispatcher>,
}

impl TelegramWebhook {
    /// Get the client for making API calls
    pub fn client(&self) -> &TelegramClient {
        &self.dispatcher.client
    }

    /// Handle a webhook update
    ///
    /// Returns as soon as the update is routed; the handler runs in the
    /// background so Telegram isn't kept waiting on slow work.
    pub async fn handle(&self, update: Update) -> Result<(), BotError> {
        self.dispatcher.dispatch_detached(update).await
    }
}

impl Endpoint for TelegramWebhook {
    type Error = WebhookError;

    async fn respond(&mut self, request: &mut Request) -> Result<HttpResponse, Self::Error> {
        let update: Update = request
            .body_mut()
            .into_json()
            .await
            .map_err(|e| WebhookError(BotError::Other(e.to_string())))?;

        self.handle(update).await.map_err(WebhookError)?;

        Ok(HttpResponse::new(Body::from_bytes("OK")))
    }
}

async fn send_response(
    client: &TelegramClient,
    chat_id: Option<i64>,
    thread_id: Option<i64>,
    mut response: Response,
) -> Result<(), BotError> {
    if response.is_empty() || response.is_acknowledge() {
        return Ok(());
    }

    // Callback queries from inline messages carry no chat to reply in.
    let Some(chat_id) = chat_id else {
        return Ok(());
    };

    if let Some(file) = response.take_file() {
        let _ = client
            .send_chat_action(chat_id, "upload_document", thread_id)
            .await;

        return client
            .send_document(
                chat_id,
                file.file,
                file.filename.as_deref(),
                file.caption.as_deref().map(Formatted::from),
                thread_id,
            )
            .await
            .map(|_| ());
    }

    let content = response.content().unwrap_or("");
    if content.is_empty() {
        return Ok(());
    }

    client
        .send_message(chat_id, content, thread_id, build_reply_markup(&response))
        .await?;
    Ok(())
}

/// Flatten unified components into Telegram's inline keyboard rows.
///
/// A bare button becomes a row of its own; select menus have no Telegram
/// equivalent and are skipped.
fn build_reply_markup(response: &Response) -> Option<ReplyMarkup> {
    use botkit_core::types::component::{Button, Component};

    fn to_button(button: &Button) -> Option<InlineKeyboardButton> {
        match (&button.url, &button.custom_id) {
            (Some(url), _) => Some(InlineKeyboardButton::url(&button.label, url)),
            (None, Some(custom_id)) => {
                Some(InlineKeyboardButton::callback(&button.label, custom_id))
            }
            (None, None) => None,
        }
    }

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    for component in response.components() {
        match component {
            Component::ActionRow(action_row) => {
                let row: Vec<_> = action_row
                    .components
                    .iter()
                    .filter_map(|c| match c {
                        Component::Button(button) => to_button(button),
                        _ => None,
                    })
                    .collect();

                if !row.is_empty() {
                    rows.push(row);
                }
            }
            Component::Button(button) => rows.extend(to_button(button).map(|b| vec![b])),
            Component::SelectMenu(_) => {}
        }
    }

    (!rows.is_empty()).then_some(ReplyMarkup::InlineKeyboard(InlineKeyboardMarkup {
        inline_keyboard: rows,
    }))
}

/// Await `future`, giving up early if shutdown is requested.
async fn race_shutdown<F: Future>(future: F, shutdown: &Shutdown) -> Option<F::Output> {
    futures_lite::future::or(async { Some(future.await) }, async {
        shutdown.wait().await;
        None
    })
    .await
}

async fn sleep(duration: Duration) {
    async_io::Timer::after(duration).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use botkit_core::types::component::{ActionRow, Button, Component, SelectMenu, SelectOption};

    /// The event an update routes to, without performing any network calls.
    fn routed_event(update: &Update) -> Option<&'static str> {
        if !matches!(
            update.kind,
            UpdateKind::Message(_)
                | UpdateKind::EditedMessage(_)
                | UpdateKind::CallbackQuery(_)
                | UpdateKind::MessageReaction(_)
        ) {
            return None;
        }

        let is_callback = matches!(update.kind, UpdateKind::CallbackQuery(_));
        let data = TelegramContextData::new(update.clone(), TelegramClient::new("t"));

        match (is_callback, data.command_name(), data.button_id()) {
            (true, _, Some(_)) => Some("button"),
            (true, _, None) => None,
            (false, Some(_), _) => Some("command"),
            (false, None, _) => Some("message"),
        }
    }

    fn update(json: serde_json::Value) -> Update {
        serde_json::from_value(json).expect("update parses")
    }

    #[test]
    fn commands_messages_and_buttons_route_to_distinct_events() {
        let command = update(serde_json::json!({
            "update_id": 1,
            "message": {
                "message_id": 1, "date": 0,
                "chat": { "id": 1, "type": "private" },
                "text": "/ping",
                "entities": [{ "type": "bot_command", "offset": 0, "length": 5 }]
            }
        }));
        assert_eq!(routed_event(&command), Some("command"));

        let message = update(serde_json::json!({
            "update_id": 2,
            "message": {
                "message_id": 1, "date": 0,
                "chat": { "id": 1, "type": "private" },
                "text": "just chatting"
            }
        }));
        assert_eq!(routed_event(&message), Some("message"));

        let button = update(serde_json::json!({
            "update_id": 3,
            "callback_query": {
                "id": "cb", "chat_instance": "x", "data": "confirm",
                "from": { "id": 1, "is_bot": false, "first_name": "Ada" }
            }
        }));
        assert_eq!(routed_event(&button), Some("button"));
    }

    #[test]
    fn reaction_updates_route_to_the_message_handler() {
        let reaction = update(serde_json::json!({
            "update_id": 7,
            "message_reaction": {
                "message_id": 9,
                "chat": { "id": 42, "type": "private" },
                "user": { "id": 1, "is_bot": false, "first_name": "Ada" },
                "date": 0,
                "old_reaction": [],
                "new_reaction": [{ "type": "emoji", "emoji": "👍" }]
            }
        }));
        assert_eq!(routed_event(&reaction), Some("message"));

        let data = TelegramContextData::new(reaction, TelegramClient::new("t"));
        assert_eq!(data.chat_id(), Some(42));
        assert_eq!(data.user_name(), "Ada");
        let UpdateKind::MessageReaction(r) = &data.update.kind else {
            panic!("expected a reaction update");
        };
        assert_eq!(r.message_id, 9);
        assert!(matches!(
            r.new_reaction.as_slice(),
            [crate::types::ReactionType::Emoji { .. }]
        ));
    }

    #[test]
    fn a_callback_without_data_does_not_fall_through_to_the_message_handler() {
        let update = update(serde_json::json!({
            "update_id": 4,
            "callback_query": {
                "id": "cb", "chat_instance": "x",
                "from": { "id": 1, "is_bot": false, "first_name": "Ada" }
            }
        }));
        assert_eq!(routed_event(&update), None);
    }

    #[test]
    fn edits_route_to_the_message_handler_but_unmodelled_kinds_do_not() {
        let edit = update(serde_json::json!({
            "update_id": 5,
            "edited_message": {
                "message_id": 1, "date": 0,
                "chat": { "id": 1, "type": "private" },
                "text": "reworded"
            }
        }));
        assert_eq!(routed_event(&edit), Some("message"));

        let poll = update(serde_json::json!({ "update_id": 6, "poll": { "id": "p" } }));
        assert_eq!(routed_event(&poll), None);
    }

    fn markup(response: &Response) -> Option<Vec<Vec<InlineKeyboardButton>>> {
        build_reply_markup(response).map(|ReplyMarkup::InlineKeyboard(m)| m.inline_keyboard)
    }

    #[test]
    fn no_components_means_no_markup() {
        assert!(markup(&Response::text("hi")).is_none());
    }

    #[test]
    fn action_rows_become_keyboard_rows() {
        let response = Response::text("hi").with_components(vec![Component::ActionRow(
            ActionRow::buttons(vec![
                Button::primary("a", "A"),
                Button::link("https://example.com", "Link"),
            ]),
        )]);

        let rows = markup(&response).expect("one row");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0].callback_data.as_deref(), Some("a"));
        assert_eq!(rows[0][1].url.as_deref(), Some("https://example.com"));
        assert!(rows[0][1].callback_data.is_none());
    }

    #[test]
    fn bare_buttons_get_their_own_row() {
        let response = Response::text("hi").with_components(vec![
            Component::Button(Button::primary("a", "A")),
            Component::Button(Button::secondary("b", "B")),
        ]);

        let rows = markup(&response).expect("two rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0].text, "A");
        assert_eq!(rows[1][0].text, "B");
    }

    #[test]
    fn components_without_a_telegram_equivalent_are_skipped() {
        let response = Response::text("hi").with_components(vec![
            Component::SelectMenu(SelectMenu::new("menu", vec![SelectOption::new("l", "v")])),
            Component::ActionRow(ActionRow::new(vec![])),
        ]);
        assert!(markup(&response).is_none());
    }
}
