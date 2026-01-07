use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Member, Message, User};

/// Discord interaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "u8", into = "u8")]
pub enum InteractionType {
    Ping,
    ApplicationCommand,
    MessageComponent,
    ApplicationCommandAutocomplete,
    ModalSubmit,
    Unknown(u8),
}

impl From<u8> for InteractionType {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Ping,
            2 => Self::ApplicationCommand,
            3 => Self::MessageComponent,
            4 => Self::ApplicationCommandAutocomplete,
            5 => Self::ModalSubmit,
            n => Self::Unknown(n),
        }
    }
}

impl From<InteractionType> for u8 {
    fn from(value: InteractionType) -> Self {
        match value {
            InteractionType::Ping => 1,
            InteractionType::ApplicationCommand => 2,
            InteractionType::MessageComponent => 3,
            InteractionType::ApplicationCommandAutocomplete => 4,
            InteractionType::ModalSubmit => 5,
            InteractionType::Unknown(n) => n,
        }
    }
}

/// Discord interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interaction {
    pub id: String,
    pub application_id: String,
    #[serde(rename = "type")]
    pub interaction_type: InteractionType,
    pub data: Option<InteractionData>,
    pub guild_id: Option<String>,
    pub channel_id: Option<String>,
    pub member: Option<Member>,
    pub user: Option<User>,
    pub token: String,
    pub version: u8,
    pub message: Option<Message>,
    pub app_permissions: Option<String>,
    pub locale: Option<String>,
    pub guild_locale: Option<String>,
}

/// Interaction data variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InteractionData {
    ApplicationCommand {
        id: String,
        name: String,
        #[serde(rename = "type")]
        command_type: Option<u8>,
        #[serde(default)]
        options: Vec<InteractionOption>,
        resolved: Option<Value>,
    },
    MessageComponent {
        custom_id: String,
        component_type: u8,
        #[serde(default)]
        values: Vec<String>,
    },
    ModalSubmit {
        custom_id: String,
        components: Vec<Value>,
    },
}

/// Command option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionOption {
    pub name: String,
    #[serde(rename = "type")]
    pub option_type: u8,
    #[serde(default)]
    pub value: Value,
    #[serde(default)]
    pub options: Vec<InteractionOption>,
    pub focused: Option<bool>,
}
