use std::any::Any;

use botkit_core::action::ChatActionSender;
use botkit_core::{ContextData, OptionValue};
use matrix_sdk::Room;
use matrix_sdk::ruma::events::reaction::OriginalSyncReactionEvent;
use matrix_sdk::ruma::events::room::message::{MessageType, OriginalSyncRoomMessageEvent};

use crate::action::MatrixActionSender;
use crate::client::MatrixClient;

/// Matrix context data - implements ContextData for platform abstraction
pub struct MatrixContextData {
    /// Room where the event occurred
    room: Room,
    /// Client for API calls
    client: MatrixClient,
    // Cached values
    room_id: String,
    user_id: String,
    user_name: String,
    command_name: Option<String>,
    command_args: Option<String>,
    /// For reactions mapped to buttons
    button_id: Option<String>,
    message_content: Option<String>,
}

impl MatrixContextData {
    /// Create context from a room message event
    pub fn from_message(
        event: &OriginalSyncRoomMessageEvent,
        room: Room,
        client: MatrixClient,
        command_prefix: &str,
    ) -> Self {
        let room_id = room.room_id().to_string();
        let user_id = event.sender.to_string();

        // Get display name (fallback to user_id localpart)
        let user_name = event.sender.localpart().to_string();

        // Extract message content
        let message_content = match &event.content.msgtype {
            MessageType::Text(text) => Some(text.body.clone()),
            _ => None,
        };

        // Parse command from message
        let (command_name, command_args) =
            Self::parse_command(message_content.as_deref(), command_prefix);

        Self {
            room,
            client,
            room_id,
            user_id,
            user_name,
            command_name,
            command_args,
            button_id: None,
            message_content,
        }
    }

    /// Create context from a reaction event
    pub fn from_reaction(
        event: &OriginalSyncReactionEvent,
        room: Room,
        client: MatrixClient,
    ) -> Self {
        let room_id = room.room_id().to_string();
        let user_id = event.sender.to_string();
        let user_name = event.sender.localpart().to_string();

        // Map reaction emoji to button_id
        let emoji = &event.content.relates_to.key;
        let button_id = Some(format!("reaction:{}", emoji));

        Self {
            room,
            client,
            room_id,
            user_id,
            user_name,
            command_name: None,
            command_args: None,
            button_id,
            message_content: None,
        }
    }

    fn parse_command(text: Option<&str>, prefix: &str) -> (Option<String>, Option<String>) {
        let text = match text {
            Some(t) if t.starts_with(prefix) => t,
            _ => return (None, None),
        };

        let without_prefix = &text[prefix.len()..];
        let mut parts = without_prefix.splitn(2, char::is_whitespace);

        let command_name = parts.next().map(|s| s.to_string());
        let command_args = parts.next().map(|s| s.trim().to_string());

        (command_name, command_args)
    }

    /// Get the Matrix Room for advanced operations
    pub fn room(&self) -> &Room {
        &self.room
    }

    /// Get the client for making API calls
    pub fn client(&self) -> &MatrixClient {
        &self.client
    }
}

impl ContextData for MatrixContextData {
    fn channel_id(&self) -> &str {
        &self.room_id
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
        // Matrix doesn't have structured options like Discord
        None
    }

    fn button_id(&self) -> Option<&str> {
        self.button_id.as_deref()
    }

    fn message_content(&self) -> Option<&str> {
        self.message_content.as_deref()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn action_sender(&self) -> Option<Box<dyn ChatActionSender>> {
        Some(Box::new(MatrixActionSender::new(self.room.clone())))
    }
}
