use std::any::Any;

use botkit_core::action::AnyChatActionSender;
use botkit_core::{ContextData, OptionValue};

use crate::action::TelegramActionSender;
use crate::client::TelegramClient;
use crate::types::{EntityType, Update, UpdateKind};

/// Telegram context data - implements ContextData for platform abstraction
pub struct TelegramContextData {
    pub update: Update,
    // Client for API calls
    client: TelegramClient,
    // Values derived once at construction, so the accessors can hand out
    // borrows instead of rebuilding strings on every call.
    channel_id: String,
    chat_id: Option<i64>,
    user_id: String,
    user_name: String,
    command_name: Option<String>,
    command_args: Option<String>,
}

impl TelegramContextData {
    pub fn new(update: Update, client: TelegramClient) -> Self {
        let (chat_id, user, actor_chat) = match &update.kind {
            UpdateKind::Message(m) | UpdateKind::EditedMessage(m) => {
                (Some(m.chat.id), m.from.as_ref(), None)
            }
            // An inline-message callback has no chat to reply in.
            UpdateKind::CallbackQuery(cq) => {
                (cq.message.as_ref().map(|m| m.chat.id), Some(&cq.from), None)
            }
            UpdateKind::MessageReaction(r) => {
                (Some(r.chat.id), r.user.as_ref(), r.actor_chat.as_ref())
            }
            UpdateKind::Unknown => (None, None, None),
        };

        // Anonymous reactions come from a channel acting as itself; surface
        // the acting chat as the sender instead of an empty identity.
        let (user_id, user_name) = user
            .map(|u| (u.id.to_string(), display_name(u)))
            .or_else(|| {
                actor_chat.map(|c| {
                    (
                        c.id.to_string(),
                        c.title
                            .clone()
                            .or_else(|| c.username.clone())
                            .unwrap_or_default(),
                    )
                })
            })
            .unwrap_or_default();

        let (command_name, command_args) = extract_command(&update);

        Self {
            channel_id: chat_id.map(|id| id.to_string()).unwrap_or_default(),
            chat_id,
            user_id,
            user_name,
            command_name,
            command_args,
            update,
            client,
        }
    }

    /// Get the client for making API calls
    pub fn client(&self) -> &TelegramClient {
        &self.client
    }

    /// The numeric chat ID, absent for updates with no chat to reply in
    pub fn chat_id(&self) -> Option<i64> {
        self.chat_id
    }

    /// The forum topic this update belongs to, when the chat has topics.
    pub fn thread_id(&self) -> Option<i64> {
        match &self.update.kind {
            UpdateKind::Message(m) | UpdateKind::EditedMessage(m) => m.message_thread_id,
            UpdateKind::CallbackQuery(cq) => cq.message.as_ref()?.message_thread_id,
            UpdateKind::MessageReaction(_) | UpdateKind::Unknown => None,
        }
    }
}

/// Telegram only guarantees `first_name`; append the surname when present.
fn display_name(user: &crate::types::User) -> String {
    match &user.last_name {
        Some(last) => format!("{} {last}", user.first_name),
        None => user.first_name.clone(),
    }
}

/// Pull `/command@bot args` out of a message.
///
/// Entity offsets and lengths come straight off the wire and are counted in
/// UTF-16 code units, so they are resolved against the text's UTF-16 view and
/// every index is checked - a malformed update must not panic the webhook.
fn extract_command(update: &Update) -> (Option<String>, Option<String>) {
    let UpdateKind::Message(message) = &update.kind else {
        return (None, None);
    };

    let (Some(text), Some(entities)) = (&message.text, &message.entities) else {
        return (None, None);
    };

    // Telegram only treats a command as an invocation when it opens the message.
    let entity = entities
        .iter()
        .find(|e| matches!(e.entity_type, EntityType::BotCommand) && e.offset == 0);

    let Some(entity) = entity.filter(|e| e.length > 0) else {
        return (None, None);
    };

    let split = usize::try_from(entity.length)
        .ok()
        .and_then(|length| utf16_offset_to_byte_index(text, length));

    let Some(split) = split else {
        return (None, None);
    };

    let (command, rest) = text.split_at(split);

    // Strip the leading slash and any `@bot_name` suffix.
    let name = command
        .trim_start_matches('/')
        .split('@')
        .next()
        .unwrap_or_default();

    if name.is_empty() {
        return (None, None);
    }

    let args = rest.trim();
    (
        Some(name.to_string()),
        (!args.is_empty()).then(|| args.to_string()),
    )
}

/// Convert a UTF-16 code-unit offset into a byte index, or `None` if it runs
/// past the end of the string or lands mid-character.
fn utf16_offset_to_byte_index(text: &str, offset: usize) -> Option<usize> {
    if offset == 0 {
        return Some(0);
    }

    let mut units = 0;
    for (index, ch) in text.char_indices() {
        if units == offset {
            return Some(index);
        }
        units += ch.len_utf16();
    }

    (units == offset).then_some(text.len())
}

impl ContextData for TelegramContextData {
    fn channel_id(&self) -> &str {
        &self.channel_id
    }

    fn user_id(&self) -> &str {
        &self.user_id
    }

    fn user_name(&self) -> &str {
        &self.user_name
    }

    fn command_name(&self) -> Option<&str> {
        self.command_name.as_deref()
    }

    fn command_args(&self) -> Option<&str> {
        self.command_args.as_deref()
    }

    fn option(&self, _name: &str) -> Option<OptionValue> {
        // Telegram doesn't have structured options like Discord
        None
    }

    fn button_id(&self) -> Option<&str> {
        match &self.update.kind {
            UpdateKind::CallbackQuery(cq) => cq.data.as_deref(),
            _ => None,
        }
    }

    fn message_content(&self) -> Option<&str> {
        match &self.update.kind {
            UpdateKind::Message(m) | UpdateKind::EditedMessage(m) => {
                m.text.as_deref().or(m.caption.as_deref())
            }
            UpdateKind::CallbackQuery(cq) => cq.message.as_ref()?.text.as_deref().or(cq
                .message
                .as_ref()?
                .caption
                .as_deref()),
            UpdateKind::MessageReaction(_) | UpdateKind::Unknown => None,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn action_sender(&self) -> Option<AnyChatActionSender> {
        Some(AnyChatActionSender::new(TelegramActionSender::new(
            self.client.clone(),
            self.chat_id?,
            self.thread_id(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(json: serde_json::Value) -> Update {
        serde_json::from_value(json).expect("valid update")
    }

    fn message(text: &str, entities: serde_json::Value) -> Update {
        update(serde_json::json!({
            "update_id": 1,
            "message": {
                "message_id": 1,
                "date": 0,
                "chat": { "id": 42, "type": "private" },
                "from": { "id": 7, "is_bot": false, "first_name": "Ada", "last_name": "Lovelace" },
                "text": text,
                "entities": entities,
            }
        }))
    }

    fn command_entity(length: i64) -> serde_json::Value {
        serde_json::json!([{ "type": "bot_command", "offset": 0, "length": length }])
    }

    #[test]
    fn parses_a_command_with_arguments() {
        let update = message("/greet world and beyond", command_entity(6));
        let (name, args) = extract_command(&update);
        assert_eq!(name.as_deref(), Some("greet"));
        assert_eq!(args.as_deref(), Some("world and beyond"));
    }

    #[test]
    fn parses_a_bare_command() {
        let (name, args) = extract_command(&message("/ping", command_entity(5)));
        assert_eq!(name.as_deref(), Some("ping"));
        assert_eq!(args, None);
    }

    #[test]
    fn strips_the_bot_mention_suffix() {
        let (name, args) = extract_command(&message("/ping@my_bot now", command_entity(12)));
        assert_eq!(name.as_deref(), Some("ping"));
        assert_eq!(args.as_deref(), Some("now"));
    }

    #[test]
    fn ignores_commands_that_do_not_open_the_message() {
        let update = message(
            "see /ping",
            serde_json::json!([{ "type": "bot_command", "offset": 4, "length": 5 }]),
        );
        assert_eq!(extract_command(&update), (None, None));
    }

    #[test]
    fn ignores_messages_without_a_command_entity() {
        let update = message("just chatting", serde_json::json!([]));
        assert_eq!(extract_command(&update), (None, None));
    }

    #[test]
    fn entity_lengths_are_counted_in_utf16_code_units() {
        // The emoji is one char but two UTF-16 units, so the arguments start at
        // byte 9 even though the command is 7 chars long.
        let update = message("/wave🎉 hi", command_entity(7));
        let (name, args) = extract_command(&update);
        assert_eq!(name.as_deref(), Some("wave🎉"));
        assert_eq!(args.as_deref(), Some("hi"));
    }

    #[test]
    fn out_of_range_entity_lengths_do_not_panic() {
        for length in [-1, 0, 6, 9999] {
            let update = message("/ping", command_entity(length));
            assert_eq!(extract_command(&update), (None, None), "length {length}");
        }
    }

    #[test]
    fn entity_lengths_landing_mid_character_do_not_panic() {
        // Length 1 splits the 2-unit emoji in half.
        let update = message("🎉x", command_entity(1));
        assert_eq!(extract_command(&update), (None, None));
    }

    #[test]
    fn callback_queries_expose_their_button_and_chat() {
        let update = update(serde_json::json!({
            "update_id": 2,
            "callback_query": {
                "id": "cb1",
                "from": { "id": 7, "is_bot": false, "first_name": "Ada" },
                "chat_instance": "x",
                "data": "confirm_yes",
                "message": {
                    "message_id": 1,
                    "date": 0,
                    "chat": { "id": 42, "type": "private" },
                    "text": "Are you sure?"
                }
            }
        }));

        let data = TelegramContextData::new(update, TelegramClient::new("token"));
        assert_eq!(data.chat_id(), Some(42));
        assert_eq!(data.button_id(), Some("confirm_yes"));
        assert_eq!(data.command_name(), None);
        assert_eq!(data.message_content(), Some("Are you sure?"));
        assert_eq!(data.user_name(), "Ada");
    }

    #[test]
    fn inline_callback_queries_have_no_chat_to_reply_in() {
        let update = update(serde_json::json!({
            "update_id": 3,
            "callback_query": {
                "id": "cb2",
                "from": { "id": 7, "is_bot": false, "first_name": "Ada" },
                "chat_instance": "x",
                "inline_message_id": "inline-1",
                "data": "x"
            }
        }));

        let data = TelegramContextData::new(update, TelegramClient::new("token"));
        assert_eq!(data.chat_id(), None);
        assert_eq!(data.channel_id(), "");
        assert!(data.action_sender().is_none());
    }

    #[test]
    fn messages_expose_the_sender_and_chat() {
        let data = TelegramContextData::new(
            message("/greet you", command_entity(6)),
            TelegramClient::new("token"),
        );
        assert_eq!(data.chat_id(), Some(42));
        assert_eq!(data.channel_id(), "42");
        assert_eq!(data.user_id(), "7");
        assert_eq!(data.user_name(), "Ada Lovelace");
        assert_eq!(data.command_name(), Some("greet"));
        assert_eq!(data.command_args(), Some("you"));
        assert!(data.action_sender().is_some());
    }

    #[test]
    fn media_caption_serves_as_message_content() {
        // A photo/document message has no `text`; its caption is the text.
        let update = update(serde_json::json!({
            "update_id": 5,
            "message": {
                "message_id": 9,
                "date": 0,
                "chat": { "id": 42, "type": "private" },
                "from": { "id": 7, "is_bot": false, "first_name": "Ada" },
                "caption": "look at this",
                "photo": [
                    { "file_id": "p1", "file_unique_id": "u1", "width": 90, "height": 90 },
                    { "file_id": "p2", "file_unique_id": "u2", "width": 800, "height": 600 }
                ]
            }
        }));

        let data = TelegramContextData::new(update, TelegramClient::new("token"));
        assert_eq!(data.message_content(), Some("look at this"));
        assert_eq!(data.chat_id(), Some(42));
    }

    #[test]
    fn unsupported_update_kinds_are_inert() {
        let update = update(serde_json::json!({
            "update_id": 4,
            "poll": { "id": "p1" }
        }));

        let data = TelegramContextData::new(update, TelegramClient::new("token"));
        assert_eq!(data.chat_id(), None);
        assert_eq!(data.user_id(), "");
        assert_eq!(data.command_name(), None);
        assert_eq!(data.button_id(), None);
        assert_eq!(data.message_content(), None);
    }
}
