use serde::{Deserialize, Serialize};

use super::User;

/// Discord message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    pub author: User,
    pub content: String,
    pub timestamp: String,
    pub edited_timestamp: Option<String>,
    pub tts: bool,
    pub mention_everyone: bool,
    pub mentions: Vec<User>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(rename = "type")]
    pub message_type: u8,
}
