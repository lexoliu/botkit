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
    /// Message type code; `0` is a regular message.
    #[serde(rename = "type", default)]
    pub message_type: u8,
}
