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
    /// A reaction on a message changed. Only delivered when `message_reaction`
    /// appears in `allowed_updates`.
    MessageReaction(MessageReactionUpdated),
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
            message_reaction: Option<MessageReactionUpdated>,
        }

        let raw = RawUpdate::deserialize(deserializer)?;

        let kind = if let Some(message) = raw.message {
            UpdateKind::Message(message)
        } else if let Some(message) = raw.edited_message {
            UpdateKind::EditedMessage(message)
        } else if let Some(callback_query) = raw.callback_query {
            UpdateKind::CallbackQuery(callback_query)
        } else if let Some(reaction) = raw.message_reaction {
            UpdateKind::MessageReaction(reaction)
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
    /// Edit timestamp for an edited message.
    pub edit_date: Option<i64>,
    pub text: Option<String>,
    /// Caption under attached media — media messages carry their text here
    /// instead of `text`.
    pub caption: Option<String>,
    pub entities: Option<Vec<MessageEntity>>,
    pub reply_to_message: Option<Box<Message>>,
    /// A sticker attached to the message.
    pub sticker: Option<Sticker>,
    /// Attached photo, largest variant last.
    pub photo: Option<Vec<PhotoSize>>,
    /// Attached video.
    pub video: Option<MediaFile>,
    /// Attached audio track.
    pub audio: Option<MediaFile>,
    /// Attached voice note.
    pub voice: Option<MediaFile>,
    /// Attached document.
    pub document: Option<MediaFile>,
    /// Attached animation (GIF).
    pub animation: Option<MediaFile>,
    /// Forum topic the message was posted to, if the chat has topics.
    pub message_thread_id: Option<i64>,
}

/// A sticker attached to a message or contained in a sticker set.
#[derive(Debug, Clone, Deserialize)]
pub struct Sticker {
    /// Identifier usable with `sendSticker` to resend it.
    pub file_id: String,
    /// Persistent identifier invariant across bots and re-uploads.
    pub file_unique_id: String,
    /// `regular`, `mask`, or `custom_emoji`.
    #[serde(rename = "type")]
    pub sticker_type: String,
    /// The emoji associated with the sticker.
    pub emoji: Option<String>,
    /// Name of the sticker set it belongs to, if any.
    pub set_name: Option<String>,
    /// True for `.tgs` animated stickers.
    #[serde(default)]
    pub is_animated: bool,
    /// True for `.webm` video stickers.
    #[serde(default)]
    pub is_video: bool,
}

/// One size of a photo attached to a message.
#[derive(Debug, Clone, Deserialize)]
pub struct PhotoSize {
    /// Identifier usable to re-send or fetch the file.
    pub file_id: String,
    /// Persistent identifier invariant across bots and re-uploads.
    pub file_unique_id: String,
    /// Photo width in pixels.
    pub width: i64,
    /// Photo height in pixels.
    pub height: i64,
}

/// A file attached to a message (video, audio, voice note, document, …).
#[derive(Debug, Clone, Deserialize)]
pub struct MediaFile {
    /// Identifier usable to re-send or fetch the file.
    pub file_id: String,
    /// Persistent identifier invariant across bots and re-uploads.
    pub file_unique_id: String,
    /// MIME type reported by the sender.
    pub mime_type: Option<String>,
    /// Original file name (documents only).
    pub file_name: Option<String>,
}

/// A file as returned by `getFile` — `file_path` is the download path under
/// `https://api.telegram.org/file/bot<token>/`.
#[derive(Debug, Clone, Deserialize)]
pub struct File {
    /// Identifier usable with `sendX`/`getFile`.
    pub file_id: String,
    /// Persistent identifier invariant across bots and re-uploads.
    pub file_unique_id: String,
    /// File size in bytes, if known.
    pub file_size: Option<i64>,
    /// Server-side download path. Files up to 20MB are downloadable; the path
    /// stays valid for at least an hour.
    pub file_path: Option<String>,
}

/// A sticker set as returned by `getStickerSet`.
#[derive(Debug, Clone, Deserialize)]
pub struct StickerSet {
    /// The set's short name (the `t.me/addstickers/<name>` slug).
    pub name: String,
    /// Human-readable title.
    pub title: String,
    /// `regular`, `mask`, or `custom_emoji`.
    pub sticker_type: String,
    /// Every sticker in the set, in order.
    pub stickers: Vec<Sticker>,
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
    /// `getMe` only: whether group privacy mode is off, i.e. the bot
    /// receives every group message rather than only commands, replies,
    /// and mentions.
    #[serde(default)]
    pub can_read_all_group_messages: Option<bool>,
}

/// Telegram Chat object. `getChat` returns `ChatFullInfo`, a superset —
/// the extra fields deserialize here and stay `None` on `message.chat`.
#[derive(Debug, Clone, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub chat_type: ChatType,
    pub title: Option<String>,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    /// `ChatFullInfo`: the chat's public description.
    #[serde(default)]
    pub description: Option<String>,
    /// `ChatFullInfo`: a private chat's bio line.
    #[serde(default)]
    pub bio: Option<String>,
    /// `ChatFullInfo`: primary invite link for groups/channels.
    #[serde(default)]
    pub invite_link: Option<String>,
    /// `ChatFullInfo`: the discussion group a channel is linked to.
    #[serde(default)]
    pub linked_chat_id: Option<i64>,
    /// `ChatFullInfo`: whether new members see the chat's history.
    #[serde(default)]
    pub has_visible_history: Option<bool>,
}

/// Telegram ChatMember object — the status a user holds in a chat. The
/// per-status extra fields (`can_post_messages`, `until_date`, …) are left
/// unparsed; `status` discriminates them.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatMember {
    /// `creator`, `administrator`, `member`, `restricted`, `left`, or
    /// `kicked`.
    pub status: String,
    /// The user the status describes.
    pub user: User,
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
    /// `text_mention` entities carry the mentioned user inline.
    #[serde(default)]
    pub user: Option<User>,
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

/// Telegram `MessageReactionUpdated` — a user's reaction set on a message
/// changed.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageReactionUpdated {
    pub chat: Chat,
    /// The message the reactions apply to.
    pub message_id: i64,
    /// When the reaction changed.
    pub date: Option<i64>,
    /// The user who changed their reaction (absent for anonymous admins).
    pub user: Option<User>,
    /// The acting chat, when a channel reacted anonymously.
    pub actor_chat: Option<Chat>,
    /// Reactions before the change.
    pub old_reaction: Vec<ReactionType>,
    /// Reactions after the change.
    pub new_reaction: Vec<ReactionType>,
}

/// Telegram `ReactionType` — a standard emoji, a custom emoji, or a paid
/// reaction.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReactionType {
    /// A standard emoji (`👍`, `❤`, …).
    Emoji {
        /// The emoji character.
        emoji: String,
    },
    /// A custom emoji sticker.
    CustomEmoji {
        /// The custom emoji's id.
        custom_emoji_id: String,
    },
    /// Telegram's paid reaction.
    Paid,
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
