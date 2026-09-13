use serde::{Deserialize, Serialize};

use super::User;

/// Discord message
///
/// Fields Discord omits from partial payloads default rather than failing the
/// whole message: a message that will not deserialize is a message the bot
/// never sees.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    /// Empty on partial `MESSAGE_UPDATE` payloads that omit the author.
    #[serde(default)]
    pub author: User,
    /// Empty unless the bot has the `MESSAGE_CONTENT` intent.
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: String,
    pub edited_timestamp: Option<String>,
    #[serde(default)]
    pub tts: bool,
    #[serde(default)]
    pub mention_everyone: bool,
    #[serde(default)]
    pub mentions: Vec<User>,
    #[serde(default)]
    pub pinned: bool,
    /// Guild the message was sent in; absent on direct messages.
    #[serde(default)]
    pub guild_id: Option<String>,
    /// The message a reply points at, when Discord resolves it.
    #[serde(default)]
    pub referenced_message: Option<Box<Message>>,
    /// Files attached to the message.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// Message type code; `0` is a regular message, `19` a reply.
    #[serde(rename = "type", default)]
    pub message_type: u8,
}

/// A file attached to a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub filename: String,
    /// CDN URL the bytes are served from.
    pub url: String,
    /// The attachment's media type, when Discord knows it.
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub size: u64,
}
