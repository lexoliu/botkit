use std::any::Any;

use botkit_core::action::ChatActionSender;
use botkit_core::{ContextData, OptionValue};

use crate::action::DiscordActionSender;
use crate::client::DiscordClient;
use crate::types::{Interaction, InteractionData};

/// Discord context data - implements ContextData for platform abstraction
pub struct DiscordContextData {
    pub interaction: Interaction,
    // Client for API calls
    client: DiscordClient,
    // Cached values
    channel_id: String,
    user_id: String,
    user_name: String,
    command_name: Option<String>,
    button_id: Option<String>,
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
            .map(|u| (u.id.clone(), u.username.clone()))
            .unwrap_or_default();

        let (command_name, button_id, options) = match &interaction.data {
            Some(InteractionData::ApplicationCommand { name, options, .. }) => {
                let opts = options
                    .iter()
                    .map(|opt| {
                        let value = match &opt.value {
                            serde_json::Value::String(s) => OptionValue::String(s.clone()),
                            serde_json::Value::Number(n) => {
                                if let Some(i) = n.as_i64() {
                                    OptionValue::Integer(i)
                                } else {
                                    OptionValue::Number(n.as_f64().unwrap_or(0.0))
                                }
                            }
                            serde_json::Value::Bool(b) => OptionValue::Boolean(*b),
                            _ => OptionValue::String(opt.value.to_string()),
                        };
                        (opt.name.clone(), value)
                    })
                    .collect();
                (Some(name.clone()), None, opts)
            }
            Some(InteractionData::MessageComponent { custom_id, .. }) => {
                (None, Some(custom_id.clone()), Vec::new())
            }
            _ => (None, None, Vec::new()),
        };

        Self {
            interaction,
            client,
            channel_id,
            user_id,
            user_name,
            command_name,
            button_id,
            options,
        }
    }

    /// Get the client for making API calls
    pub fn client(&self) -> &DiscordClient {
        &self.client
    }
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
        self.command_name.as_deref()
    }

    fn command_args(&self) -> Option<&str> {
        // Discord uses structured options, not string args
        None
    }

    fn option(&self, name: &str) -> Option<OptionValue> {
        self.options
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
    }

    fn button_id(&self) -> Option<&str> {
        self.button_id.as_deref()
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

    fn action_sender(&self) -> Option<Box<dyn ChatActionSender>> {
        if self.channel_id.is_empty() {
            return None;
        }
        Some(Box::new(DiscordActionSender::new(
            self.client.clone(),
            self.channel_id.clone(),
        )))
    }
}
