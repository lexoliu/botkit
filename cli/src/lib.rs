//! botkit-cli: an IM platform implemented as an agent-friendly CLI.
//!
//! Instead of talking to Discord/Telegram/Matrix, a `CliBot` speaks a JSONL
//! wire protocol: drivers inject [`Inbound`] events (messages, commands,
//! button presses, reactions, edits — with media, stickers, threads, and
//! replies) and observe every [`Outbound`] action the bot performs
//! (messages, files, reactions, edits, deletes, pins, typing indicators).
//!
//! Two transports:
//!
//! - [`Transport::Stdio`] — stdin/stdout, for `cargo run` debugging.
//! - [`Transport::Unix`] — a unix socket the `botkit-cli` driver binary
//!   connects to, for driving a long-running bot process.
//!
//! [`Transport::Manual`] attaches nothing: the owner injects and subscribes
//! through the [`CliHub`] handle directly (tests, in-process embedding).
//!
//! ```ignore
//! use botkit_cli::{CliBot, Transport};
//! use botkit_core::Bot;
//!
//! let bot = CliBot::new(Transport::Stdio)
//!     .command("ping", || async { "pong" })
//!     .message(|ctx: botkit_core::Context| async move {
//!         format!("echo: {}", ctx.message_content().unwrap_or(""))
//!     });
//! bot.run().await?;
//! ```

mod bot;
mod context;
mod hub;
mod transport;
pub mod wire;

pub use bot::CliBot;
pub use context::{CliActionSender, CliContextData};
pub use hub::CliHub;
pub use transport::Transport;
pub use wire::{Inbound, Outbound};
