use std::any::Any;

use botkit_core::{ContextData, OptionValue};

use crate::types::{EntityType, Update, UpdateKind};

/// Telegram context data - implements ContextData for platform abstraction
pub struct TelegramContextData {
    pub update: Update,
    // Cached values
    channel_id: String,
    user_id: String,
    user_name: String,
    command_name: Option<String>,
    command_args: Option<String>,
    button_id: Option<String>,
    message_content: Option<String>,
}

impl TelegramContextData {
    pub fn new(update: Update) -> Self {
        let (channel_id, user_id, user_name, message_content) = match &update.kind {
            UpdateKind::Message(m) | UpdateKind::EditedMessage(m) => {
                let user = m.from.as_ref();
                (
                    m.chat.id.to_string(),
                    user.map(|u| u.id.to_string()).unwrap_or_default(),
                    user.map(|u| u.first_name.clone()).unwrap_or_default(),
                    m.text.clone(),
                )
            }
            UpdateKind::CallbackQuery(cq) => {
                let chat_id = cq
                    .message
                    .as_ref()
                    .map(|m| m.chat.id.to_string())
                    .unwrap_or_default();
                (
                    chat_id,
                    cq.from.id.to_string(),
                    cq.from.first_name.clone(),
                    cq.message.as_ref().and_then(|m| m.text.clone()),
                )
            }
            _ => (String::new(), String::new(), String::new(), None),
        };

        let (command_name, command_args) = Self::extract_command(&update);

        let button_id = match &update.kind {
            UpdateKind::CallbackQuery(cq) => cq.data.clone(),
            _ => None,
        };

        Self {
            update,
            channel_id,
            user_id,
            user_name,
            command_name,
            command_args,
            button_id,
            message_content,
        }
    }

    fn extract_command(update: &Update) -> (Option<String>, Option<String>) {
        let message = match &update.kind {
            UpdateKind::Message(m) => m,
            _ => return (None, None),
        };

        let text = match &message.text {
            Some(t) => t,
            None => return (None, None),
        };

        let entities = match &message.entities {
            Some(e) => e,
            None => return (None, None),
        };

        // Find bot_command entity at offset 0
        let cmd_entity = entities
            .iter()
            .find(|e| matches!(e.entity_type, EntityType::BotCommand) && e.offset == 0);

        let cmd_entity = match cmd_entity {
            Some(e) => e,
            None => return (None, None),
        };

        let cmd_text = &text[..cmd_entity.length as usize];
        // Remove leading '/' and any @bot_name suffix
        let name = cmd_text
            .trim_start_matches('/')
            .split('@')
            .next()
            .unwrap_or("")
            .to_string();

        let args = text[cmd_entity.length as usize..].trim().to_string();
        let args = if args.is_empty() { None } else { Some(args) };

        (Some(name), args)
    }
}

impl ContextData for TelegramContextData {
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
        self.command_args.as_deref()
    }

    fn option(&self, _name: &str) -> Option<OptionValue> {
        // Telegram doesn't have structured options like Discord
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
}
