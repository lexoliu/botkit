#![doc = include_str!("../README.md")]

pub use botkit_core as core;
pub use botkit_core::*;

#[cfg(feature = "discord")]
pub use botkit_discord as discord;
#[cfg(feature = "discord")]
pub use botkit_discord::{
    DiscordBot, DiscordClient, DiscordContextData, Gateway, GatewayIntents, MessageContextData,
};

#[cfg(feature = "telegram")]
pub use botkit_telegram as telegram;
#[cfg(feature = "telegram")]
pub use botkit_telegram::{TelegramBot, TelegramClient, TelegramContextData, TelegramWebhook};

#[cfg(feature = "matrix")]
pub use botkit_matrix as matrix;
#[cfg(feature = "matrix")]
pub use botkit_matrix::{MatrixAuth, MatrixBot, MatrixClient, MatrixConfig, MatrixContextData};

pub mod prelude {
    pub use botkit_core::types::{
        ActionRow, Button, ButtonStyle, Component, Embed, SelectMenu, SelectOption,
    };
    pub use botkit_core::{
        AnyChatActionSender, Bot, BotBuilder, BotError, ButtonId, Channel, ChatAction,
        ChatActionGuard, ChatActionSender, CommandArgs, CommandInfo, CommandName, Context,
        ContextData, Event, FileResponse, FileSource, FromContext, Handler, IntoHandler,
        IntoResponse, MessageContent, OptionValue, Response, Shutdown, ShutdownSignal, Typing,
        User,
    };

    #[cfg(feature = "discord")]
    pub use botkit_discord::DiscordBot;

    #[cfg(feature = "telegram")]
    pub use botkit_telegram::{TelegramBot, TelegramWebhook};

    #[cfg(feature = "matrix")]
    pub use botkit_matrix::{MatrixBot, MatrixConfig};
}
