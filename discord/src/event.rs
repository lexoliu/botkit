use std::any::Any;

use botkit_core::action::AnyChatActionSender;
use botkit_core::{ContextData, OptionValue};

use crate::action::DiscordActionSender;
use crate::client::DiscordClient;
use crate::types::{Interaction, InteractionData, InteractionOption, Message};

/// Discord interaction context - implements ContextData for platform abstraction
pub struct DiscordContextData {
    interaction: Interaction,
    client: DiscordClient,
    // Values derived once at construction, so the accessors can hand out
    // borrows instead of rebuilding strings on every call.
    channel_id: String,
    user_id: String,
    user_name: String,
    options: Vec<(String, OptionValue)>,
}

impl DiscordContextData {
    pub fn new(interaction: Interaction, client: DiscordClient) -> Self {
        let channel_id = interaction.channel_id.clone().unwrap_or_default();

        let (user_id, user_name) = interaction
            .member
            .as_ref()
            .and_then(|m| m.user.as_ref())
            .or(interaction.user.as_ref())
            .map(|u| {
                // `global_name` is the modern display name; `username` is the
                // handle every account still has.
                let name = u.global_name.clone().unwrap_or_else(|| u.username.clone());
                (u.id.clone(), name)
            })
            .unwrap_or_default();

        let options = match &interaction.data {
            Some(InteractionData::ApplicationCommand { options, .. }) => {
                options.iter().map(option_pair).collect()
            }
            _ => Vec::new(),
        };

        Self {
            interaction,
            client,
            channel_id,
            user_id,
            user_name,
            options,
        }
    }

    /// The interaction that produced this context
    pub fn interaction(&self) -> &Interaction {
        &self.interaction
    }

    /// The channel this interaction came from, if Discord reported one
    pub fn channel_id_opt(&self) -> Option<&str> {
        self.interaction.channel_id.as_deref()
    }

    /// Get the client for making API calls
    pub fn client(&self) -> &DiscordClient {
        &self.client
    }
}

fn option_pair(option: &InteractionOption) -> (String, OptionValue) {
    let value = match &option.value {
        serde_json::Value::String(s) => OptionValue::String(s.clone()),
        serde_json::Value::Bool(b) => OptionValue::Boolean(*b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => OptionValue::Integer(i),
            None => OptionValue::Number(n.as_f64().unwrap_or(0.0)),
        },
        other => OptionValue::String(other.to_string()),
    };
    (option.name.clone(), value)
}

impl ContextData for DiscordContextData {
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
        match &self.interaction.data {
            Some(InteractionData::ApplicationCommand { name, .. }) => Some(name),
            _ => None,
        }
    }

    fn command_args(&self) -> Option<&str> {
        // Discord uses structured options, not string args; see `option`.
        None
    }

    fn option(&self, name: &str) -> Option<OptionValue> {
        self.options
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
    }

    fn button_id(&self) -> Option<&str> {
        match &self.interaction.data {
            Some(InteractionData::MessageComponent { custom_id, .. }) => Some(custom_id),
            Some(InteractionData::ModalSubmit { custom_id, .. }) => Some(custom_id),
            _ => None,
        }
    }

    fn message_content(&self) -> Option<&str> {
        self.interaction
            .message
            .as_ref()
            .map(|m| m.content.as_str())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn action_sender(&self) -> Option<AnyChatActionSender> {
        if self.channel_id.is_empty() {
            return None;
        }
        Some(AnyChatActionSender::new(DiscordActionSender::new(
            self.client.clone(),
            self.channel_id.clone(),
        )))
    }
}

/// Discord message context, for handlers registered with `DiscordBot::message`
pub struct MessageContextData {
    message: Message,
    client: DiscordClient,
    user_name: String,
    /// Whether this message is an edit (`MESSAGE_UPDATE`) rather than a new
    /// message (`MESSAGE_CREATE`). On edits `message` holds the post-edit
    /// state and may be partial — fields Discord didn't resend are at their
    /// defaults.
    pub edited: bool,
}

impl MessageContextData {
    pub fn new(message: Message, client: DiscordClient) -> Self {
        let user_name = message
            .author
            .global_name
            .clone()
            .unwrap_or_else(|| message.author.username.clone());

        Self {
            message,
            client,
            user_name,
            edited: false,
        }
    }

    /// The context for an edited message — same shape as `new`, with the
    /// `edited` flag set so handlers can tell a correction from a new post.
    pub fn new_edited(message: Message, client: DiscordClient) -> Self {
        Self {
            edited: true,
            ..Self::new(message, client)
        }
    }

    /// The message that produced this context
    pub fn message(&self) -> &Message {
        &self.message
    }

    /// Get the client for making API calls
    pub fn client(&self) -> &DiscordClient {
        &self.client
    }
}

impl ContextData for MessageContextData {
    fn channel_id(&self) -> &str {
        &self.message.channel_id
    }

    fn user_id(&self) -> &str {
        &self.message.author.id
    }

    fn user_name(&self) -> &str {
        &self.user_name
    }

    fn command_name(&self) -> Option<&str> {
        None
    }

    fn command_args(&self) -> Option<&str> {
        None
    }

    fn option(&self, _name: &str) -> Option<OptionValue> {
        None
    }

    fn button_id(&self) -> Option<&str> {
        None
    }

    fn message_content(&self) -> Option<&str> {
        Some(&self.message.content)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn action_sender(&self) -> Option<AnyChatActionSender> {
        Some(AnyChatActionSender::new(DiscordActionSender::new(
            self.client.clone(),
            self.message.channel_id.clone(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use botkit_core::OptionValue;

    fn client() -> DiscordClient {
        DiscordClient::new("token", "app")
    }

    fn interaction(json: serde_json::Value) -> Interaction {
        serde_json::from_value(json).expect("interaction parses")
    }

    fn slash_command() -> Interaction {
        interaction(serde_json::json!({
            "id": "i1",
            "application_id": "app",
            "type": 2,
            "token": "tok",
            "version": 1,
            "channel_id": "chan",
            "member": {
                "roles": [],
                "user": {
                    "id": "u1",
                    "username": "ada",
                    "global_name": "Ada Lovelace"
                }
            },
            "data": {
                "id": "c1",
                "name": "greet",
                "options": [
                    { "name": "who", "type": 3, "value": "world" },
                    { "name": "count", "type": 4, "value": 3 },
                    { "name": "ratio", "type": 10, "value": 1.5 },
                    { "name": "loud", "type": 5, "value": true }
                ]
            }
        }))
    }

    #[test]
    fn slash_commands_expose_name_user_and_options() {
        let data = DiscordContextData::new(slash_command(), client());

        assert_eq!(data.command_name(), Some("greet"));
        assert_eq!(data.channel_id(), "chan");
        assert_eq!(data.user_id(), "u1");
        // `global_name` is the display name Discord shows.
        assert_eq!(data.user_name(), "Ada Lovelace");
        assert_eq!(data.button_id(), None);

        assert_eq!(
            data.option("who")
                .and_then(|v| v.as_str().map(String::from))
                .as_deref(),
            Some("world")
        );
        assert_eq!(data.option("count").and_then(|v| v.as_i64()), Some(3));
        assert_eq!(data.option("ratio").and_then(|v| v.as_f64()), Some(1.5));
        assert_eq!(data.option("loud").and_then(|v| v.as_bool()), Some(true));
        assert!(data.option("missing").is_none());
    }

    #[test]
    fn users_without_a_global_name_fall_back_to_the_handle() {
        let data = DiscordContextData::new(
            interaction(serde_json::json!({
                "id": "i1",
                "application_id": "app",
                "type": 2,
                "token": "tok",
                "version": 1,
                "user": { "id": "u2", "username": "legacy" },
                "data": { "id": "c1", "name": "ping" }
            })),
            client(),
        );

        assert_eq!(data.user_name(), "legacy");
        // No channel means no typing indicator to send.
        assert_eq!(data.channel_id(), "");
        assert!(data.action_sender().is_none());
    }

    #[test]
    fn component_interactions_expose_their_custom_id() {
        let data = DiscordContextData::new(
            interaction(serde_json::json!({
                "id": "i2",
                "application_id": "app",
                "type": 3,
                "token": "tok",
                "version": 1,
                "channel_id": "chan",
                "user": { "id": "u1", "username": "ada" },
                "data": { "custom_id": "confirm_yes", "component_type": 2 },
                "message": {
                    "id": "m1",
                    "channel_id": "chan",
                    "author": { "id": "b1", "username": "bot" },
                    "content": "Are you sure?",
                    "timestamp": "2024-01-01T00:00:00Z",
                    "tts": false,
                    "mention_everyone": false,
                    "mentions": [],
                    "type": 0
                }
            })),
            client(),
        );

        assert_eq!(data.button_id(), Some("confirm_yes"));
        assert_eq!(data.command_name(), None);
        assert_eq!(data.message_content(), Some("Are you sure?"));
        assert!(data.action_sender().is_some());
    }

    #[test]
    fn option_values_keep_their_json_type() {
        let data = DiscordContextData::new(slash_command(), client());
        assert!(matches!(data.option("who"), Some(OptionValue::String(_))));
        assert!(matches!(
            data.option("count"),
            Some(OptionValue::Integer(_))
        ));
        assert!(matches!(data.option("ratio"), Some(OptionValue::Number(_))));
        assert!(matches!(data.option("loud"), Some(OptionValue::Boolean(_))));
    }

    #[test]
    fn message_contexts_report_the_author_and_channel() {
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "m1",
            "channel_id": "chan",
            "author": { "id": "u1", "username": "ada", "global_name": "Ada" },
            "content": "hello",
            "timestamp": "2024-01-01T00:00:00Z",
            "tts": false,
            "mention_everyone": false,
            "mentions": [],
            "type": 0
        }))
        .expect("message parses");

        let data = MessageContextData::new(message, client());
        assert_eq!(data.channel_id(), "chan");
        assert_eq!(data.user_id(), "u1");
        assert_eq!(data.user_name(), "Ada");
        assert_eq!(data.message_content(), Some("hello"));
        assert_eq!(data.command_name(), None);
        assert_eq!(data.button_id(), None);
        assert!(data.action_sender().is_some());
    }
}
