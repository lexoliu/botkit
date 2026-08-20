//! Platform-agnostic building blocks for the botkit bot framework.
//!
//! Handlers are plain async functions. They declare what they need as
//! parameters ([`FromContext`] extractors) and return anything that converts
//! into a [`Response`] ([`IntoResponse`]). Adapters translate their platform's
//! payloads into a [`Context`] and route them through a [`BotBuilder`].

pub mod action;
mod bot;
mod context;
mod error;
mod extractor;
mod handler;
mod responder;
mod response;
mod shutdown;
#[cfg(test)]
mod test_util;
pub mod types;

pub use action::{ChatAction, ChatActionGuard, ChatActionSender};
pub use bot::{Bot, BotBuilder, CommandInfo, Event};
pub use context::{Context, ContextData, OptionValue};
pub use error::BotError;
pub use extractor::{
    ButtonId, Channel, CommandArgs, CommandName, FromContext, MessageContent, Typing, User,
};
pub use handler::{BoxedHandler, Handler, IntoHandler};
pub use responder::IntoResponse;
pub use response::{FileResponse, FileSource, Response};
pub use shutdown::{Shutdown, ShutdownSignal};
