use std::path::PathBuf;

use matrix_sdk::ruma::{OwnedDeviceId, OwnedUserId};

/// Matrix bot configuration
///
/// Configure the homeserver, authentication, command prefix, and other options.
///
/// # Example
/// ```ignore
/// let config = MatrixConfig::new("https://matrix.org")
///     .password_auth("@bot:matrix.org", "password")
///     .command_prefix("!")
///     .auto_join_rooms(true);
/// ```
pub struct MatrixConfig {
    /// Homeserver URL (e.g., "https://matrix.org")
    pub(crate) homeserver_url: String,

    /// Authentication method
    pub(crate) auth: MatrixAuth,

    /// Command prefix for parsing commands from messages (default: "!")
    pub(crate) command_prefix: String,

    /// Device display name for this bot session
    pub(crate) device_name: Option<String>,

    /// State store path for persistence (enables session restore and E2EE key storage)
    pub(crate) state_store_path: Option<PathBuf>,

    /// Whether to auto-join rooms when invited
    pub(crate) auto_join_rooms: bool,
}

/// Authentication options for Matrix
pub enum MatrixAuth {
    /// Username and password login
    Password {
        /// Matrix user ID (e.g., "@bot:matrix.org")
        user_id: String,
        /// Password
        password: String,
    },
    /// Access token (for pre-authenticated sessions)
    AccessToken {
        /// Matrix user ID
        user_id: OwnedUserId,
        /// Access token
        access_token: String,
        /// Device ID (required for E2EE)
        device_id: OwnedDeviceId,
    },
}

impl MatrixConfig {
    /// Create a new Matrix configuration with the given homeserver URL
    ///
    /// Authentication must be configured before building the bot.
    pub fn new(homeserver_url: impl Into<String>) -> Self {
        Self {
            homeserver_url: homeserver_url.into(),
            auth: MatrixAuth::Password {
                user_id: String::new(),
                password: String::new(),
            },
            command_prefix: "!".to_string(),
            device_name: None,
            state_store_path: None,
            auto_join_rooms: false,
        }
    }

    /// Set password authentication
    ///
    /// # Arguments
    /// * `user_id` - Matrix user ID (e.g., "@bot:matrix.org")
    /// * `password` - Password for the account
    pub fn password_auth(
        mut self,
        user_id: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.auth = MatrixAuth::Password {
            user_id: user_id.into(),
            password: password.into(),
        };
        self
    }

    /// Set access token authentication
    ///
    /// Use this for pre-authenticated sessions. Requires the device ID for E2EE.
    pub fn access_token_auth(
        mut self,
        user_id: OwnedUserId,
        access_token: impl Into<String>,
        device_id: OwnedDeviceId,
    ) -> Self {
        self.auth = MatrixAuth::AccessToken {
            user_id,
            access_token: access_token.into(),
            device_id,
        };
        self
    }

    /// Set the command prefix (default: "!")
    ///
    /// Messages starting with this prefix will be parsed as commands.
    pub fn command_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.command_prefix = prefix.into();
        self
    }

    /// Set the device display name
    ///
    /// This appears in the user's device list.
    pub fn device_name(mut self, name: impl Into<String>) -> Self {
        self.device_name = Some(name.into());
        self
    }

    /// Set the state store path for persistence
    ///
    /// Enables session persistence and E2EE key storage.
    /// Without this, E2EE keys are lost on restart.
    pub fn state_store_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.state_store_path = Some(path.into());
        self
    }

    /// Enable or disable auto-joining rooms when invited
    pub fn auto_join_rooms(mut self, enabled: bool) -> Self {
        self.auto_join_rooms = enabled;
        self
    }
}
