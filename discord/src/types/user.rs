use serde::{Deserialize, Serialize};

/// Discord user
///
/// `Default` exists for partial payloads — a `MESSAGE_UPDATE` dispatch may
/// omit the author entirely, leaving an empty user rather than failing to
/// parse the message.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub discriminator: Option<String>,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
    pub bot: Option<bool>,
}

/// Discord guild member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub user: Option<User>,
    pub nick: Option<String>,
    pub roles: Vec<String>,
    pub joined_at: Option<String>,
    pub premium_since: Option<String>,
    pub deaf: Option<bool>,
    pub mute: Option<bool>,
    pub pending: Option<bool>,
    pub permissions: Option<String>,
}
