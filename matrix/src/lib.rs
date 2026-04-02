//! Matrix implementation for botkit
//!
//! This crate provides Matrix platform support for the botkit unified bot framework.
//! It uses the `matrix-sdk` crate for protocol handling, including E2E encryption.
//!
//! # Example
//! ```ignore
//! use botkit_matrix::{MatrixBot, MatrixConfig};
//! use botkit_core::extractor::User;
//!
//! async fn ping() -> &'static str { "Pong!" }
//! async fn greet(user: User) -> String { format!("Hello, {}!", user.name) }
//!
//! let config = MatrixConfig::new("https://matrix.org")
//!     .password_auth("@bot:matrix.org", "password")
//!     .command_prefix("!");
//!
//! MatrixBot::new(config)
//!     .command("ping", ping)
//!     .command("greet", greet)
//!     .reaction("👍", on_thumbsup)
//!     .run()
//!     .await
//!     .unwrap();
//! ```

#[cfg(target_arch = "wasm32")]
use getrandom as _;

pub mod action;
mod bot;
mod client;
mod config;
mod event;

pub use bot::MatrixBot;
pub use client::MatrixClient;
pub use config::{MatrixAuth, MatrixConfig};
pub use event::MatrixContextData;

// Re-export commonly used matrix-sdk types for convenience
pub use matrix_sdk::ruma::{OwnedDeviceId, OwnedEventId, OwnedRoomId, OwnedUserId};
pub use matrix_sdk::{Room, RoomState};
