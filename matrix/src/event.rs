use std::any::Any;

use botkit_core::action::ChatActionSender;
use botkit_core::{ContextData, OptionValue};
use matrix_sdk::Room;
use matrix_sdk::ruma::events::reaction::OriginalSyncReactionEvent;
use matrix_sdk::ruma::events::room::message::{MessageType, OriginalSyncRoomMessageEvent};

use crate::action::MatrixActionSender;
use crate::client::MatrixClient;

/// The button id a reaction is routed under.
///
/// Reactions share the button table so one handler can serve a Discord button
/// and a Matrix reaction; keeping the id in one place stops registration and
/// dispatch from drifting apart.
pub fn reaction_button_id(emoji: &str) -> String {
    format!("reaction:{emoji}")
}

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

        let button_id = Some(reaction_button_id(&event.content.relates_to.key));

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

    /// Split `<prefix><name> <args>` out of a message body.
    ///
    /// An empty prefix would make every message a command, and a bare prefix
    /// with no name is not a command either.
    fn parse_command(text: Option<&str>, prefix: &str) -> (Option<String>, Option<String>) {
        if prefix.is_empty() {
            return (None, None);
        }

        let Some(rest) = text.and_then(|t| t.strip_prefix(prefix)) else {
            return (None, None);
        };

        let mut parts = rest.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or_default();

        if name.is_empty() {
            return (None, None);
        }

        let args = parts.next().map(str::trim).filter(|a| !a.is_empty());

        (Some(name.to_string()), args.map(str::to_string))
    }

    /// Get the Matrix Room for advanced operations
    pub fn room(&self) -> &Room {
        &self.room
    }

    /// Get the client for making API calls
    pub fn client(&self) -> &MatrixClient {
        &self.client
    }

    /// The command this message invokes, if any
    ///
    /// Parsed once at construction, so routing and the `CommandName` extractor
    /// always agree.
    pub fn command(&self) -> Option<&str> {
        self.command_name.as_deref()
    }

    /// The message body, absent for non-text messages and reactions
    pub fn message_text(&self) -> Option<&str> {
        self.message_content.as_deref()
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

#[cfg(test)]
mod tests {
    use super::MatrixContextData;

    fn parse(text: &str, prefix: &str) -> (Option<String>, Option<String>) {
        MatrixContextData::parse_command(Some(text), prefix)
    }

    #[test]
    fn parses_a_command_with_arguments() {
        let (name, args) = parse("!greet world and beyond", "!");
        assert_eq!(name.as_deref(), Some("greet"));
        assert_eq!(args.as_deref(), Some("world and beyond"));
    }

    #[test]
    fn parses_a_bare_command() {
        assert_eq!(parse("!ping", "!"), (Some("ping".into()), None));
    }

    #[test]
    fn trailing_whitespace_is_not_an_argument() {
        assert_eq!(parse("!ping   ", "!"), (Some("ping".into()), None));
    }

    #[test]
    fn multi_character_prefixes_work() {
        assert_eq!(
            parse(">>ping now", ">>"),
            (Some("ping".into()), Some("now".into()))
        );
    }

    #[test]
    fn messages_without_the_prefix_are_not_commands() {
        assert_eq!(parse("ping", "!"), (None, None));
        assert_eq!(parse("hey !ping", "!"), (None, None));
    }

    #[test]
    fn a_bare_prefix_is_not_a_command() {
        assert_eq!(parse("!", "!"), (None, None));
        assert_eq!(parse("! ping", "!"), (None, None));
    }

    #[test]
    fn an_empty_prefix_does_not_make_everything_a_command() {
        assert_eq!(parse("hello", ""), (None, None));
    }

    #[test]
    fn non_ascii_prefixes_split_on_character_boundaries() {
        assert_eq!(
            parse("🤖ping now", "🤖"),
            (Some("ping".into()), Some("now".into()))
        );
    }

    #[test]
    fn absent_text_is_not_a_command() {
        assert_eq!(MatrixContextData::parse_command(None, "!"), (None, None));
    }
}
