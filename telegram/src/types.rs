use serde::{Deserialize, Serialize};

/// Telegram Update object received via webhook
///
/// Deserialized field by field rather than with a flattened enum: Telegram adds
/// new update kinds regularly, and a bot must keep accepting them (as
/// [`UpdateKind::Unknown`]) instead of failing the whole payload — a rejected
/// update is redelivered forever.
#[derive(Debug, Clone)]
pub struct Update {
    pub update_id: i64,
    pub kind: UpdateKind,
}

/// The kind of update received
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum UpdateKind {
    /// A new message in a chat
    Message(Message),
    /// An edit to a message already delivered
    EditedMessage(Message),
    /// An inline keyboard button press
    CallbackQuery(CallbackQuery),
    /// An update this version does not model
    Unknown,
}

impl<'de> Deserialize<'de> for Update {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// Every update kind botkit understands, plus the id shared by all.
        #[derive(Deserialize)]
        struct RawUpdate {
            update_id: i64,
            message: Option<Message>,
            edited_message: Option<Message>,
            callback_query: Option<CallbackQuery>,
        }

        let raw = RawUpdate::deserialize(deserializer)?;

        let kind = if let Some(message) = raw.message {
            UpdateKind::Message(message)
        } else if let Some(message) = raw.edited_message {
            UpdateKind::EditedMessage(message)
        } else if let Some(callback_query) = raw.callback_query {
            UpdateKind::CallbackQuery(callback_query)
        } else {
            UpdateKind::Unknown
        };

        Ok(Self {
            update_id: raw.update_id,
            kind,
        })
    }
}

/// Telegram Message object
#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub from: Option<User>,
    pub chat: Chat,
    pub date: i64,
    pub text: Option<String>,
    pub entities: Option<Vec<MessageEntity>>,
    pub reply_to_message: Option<Box<Message>>,
}

/// Telegram User object
#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: i64,
    pub is_bot: bool,
    pub first_name: String,
    pub last_name: Option<String>,
    pub username: Option<String>,
    pub language_code: Option<String>,
}

/// Telegram Chat object
#[derive(Debug, Clone, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub chat_type: ChatType,
    pub title: Option<String>,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

/// Chat type
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatType {
    Private,
    Group,
    Supergroup,
    Channel,
}

/// Message entity (commands, mentions, etc.)
#[derive(Debug, Clone, Deserialize)]
pub struct MessageEntity {
    #[serde(rename = "type")]
    pub entity_type: EntityType,
    pub offset: i64,
    pub length: i64,
}

/// Entity type
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Mention,
    Hashtag,
    Cashtag,
    BotCommand,
    Url,
    Email,
    PhoneNumber,
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Spoiler,
    Code,
    Pre,
    TextLink,
    TextMention,
    CustomEmoji,
    #[serde(other)]
    Unknown,
}

/// Callback query (button press)
#[derive(Debug, Clone, Deserialize)]
pub struct CallbackQuery {
    pub id: String,
    pub from: User,
    pub message: Option<Message>,
    pub inline_message_id: Option<String>,
    pub chat_instance: String,
    pub data: Option<String>,
}

/// Inline keyboard markup
#[derive(Debug, Clone, Serialize)]
pub struct InlineKeyboardMarkup {
    pub inline_keyboard: Vec<Vec<InlineKeyboardButton>>,
}

/// Inline keyboard button
#[derive(Debug, Clone, Serialize)]
pub struct InlineKeyboardButton {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_data: Option<String>,
}

impl InlineKeyboardButton {
    pub fn callback(text: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            url: None,
            callback_data: Some(data.into()),
        }
    }

    pub fn url(text: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            url: Some(url.into()),
            callback_data: None,
        }
    }
}

/// Reply markup options
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ReplyMarkup {
    InlineKeyboard(InlineKeyboardMarkup),
}

/// Bot command for setMyCommands API
#[derive(Debug, Clone, Serialize)]
pub struct BotCommand {
    /// Command name without the leading slash
    pub command: String,
    /// Description shown in the command menu
    pub description: String,
}

impl BotCommand {
    /// Create a new bot command
    pub fn new(command: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            description: description.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: serde_json::Value) -> Update {
        serde_json::from_value(json).expect("update parses")
    }

    #[test]
    fn parses_a_message_update() {
        let update = parse(serde_json::json!({
            "update_id": 1,
            "message": {
                "message_id": 5,
                "date": 0,
                "chat": { "id": 42, "type": "supergroup", "title": "Room" },
                "text": "hi"
            }
        }));

        assert_eq!(update.update_id, 1);
        let UpdateKind::Message(message) = update.kind else {
            panic!("expected a message");
        };
        assert_eq!(message.message_id, 5);
        assert_eq!(message.text.as_deref(), Some("hi"));
    }

    #[test]
    fn parses_an_edited_message_update() {
        let update = parse(serde_json::json!({
            "update_id": 2,
            "edited_message": {
                "message_id": 5,
                "date": 0,
                "chat": { "id": 42, "type": "private" },
                "text": "fixed"
            }
        }));
        assert!(matches!(update.kind, UpdateKind::EditedMessage(_)));
    }

    #[test]
    fn unmodelled_update_kinds_still_parse() {
        // Telegram keeps adding update kinds; rejecting one would make it
        // redeliver the same update forever.
        for kind in ["poll", "my_chat_member", "inline_query", "channel_post"] {
            let update = parse(serde_json::json!({
                "update_id": 3,
                kind: { "id": "whatever", "unexpected": [1, 2, 3] }
            }));
            assert!(matches!(update.kind, UpdateKind::Unknown), "{kind}");
            assert_eq!(update.update_id, 3);
        }
    }

    #[test]
    fn unknown_fields_on_known_kinds_are_ignored() {
        let update = parse(serde_json::json!({
            "update_id": 4,
            "message": {
                "message_id": 5,
                "date": 0,
                "chat": { "id": 42, "type": "private" },
                "text": "hi",
                "some_future_field": { "nested": true }
            }
        }));
        assert!(matches!(update.kind, UpdateKind::Message(_)));
    }

    #[test]
    fn inline_keyboards_serialize_the_way_telegram_expects() {
        let markup = ReplyMarkup::InlineKeyboard(InlineKeyboardMarkup {
            inline_keyboard: vec![vec![
                InlineKeyboardButton::callback("Yes", "yes"),
                InlineKeyboardButton::url("Docs", "https://example.com"),
            ]],
        });

        let json = serde_json::to_value(&markup).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "inline_keyboard": [[
                    { "text": "Yes", "callback_data": "yes" },
                    { "text": "Docs", "url": "https://example.com" }
                ]]
            })
        );
    }
}
