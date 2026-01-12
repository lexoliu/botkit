use std::future::Future;

use crate::BotError;
use crate::handler::{BoxedHandler, IntoHandler};

/// Unified Bot trait that hides connection mode differences
///
/// Both Discord (WebSocket) and Telegram (HTTP webhook) bots implement
/// this trait, providing a consistent API regardless of the underlying
/// transport mechanism.
pub trait Bot: Sized + Send {
    /// Run the bot and start processing events
    ///
    /// Returns a handle that can be used to control the bot (shutdown, etc.)
    fn run(self) -> impl Future<Output = Result<BotHandle, BotError>> + Send;
}

/// Handle to control a running bot
pub struct BotHandle {
    shutdown_tx: async_channel::Sender<()>,
}

impl BotHandle {
    /// Create a new bot handle with a shutdown channel
    pub fn new(shutdown_tx: async_channel::Sender<()>) -> Self {
        Self { shutdown_tx }
    }

    /// Signal the bot to shut down gracefully
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(()).await;
    }

    /// Create a shutdown channel pair
    pub fn channel() -> (Self, async_channel::Receiver<()>) {
        let (tx, rx) = async_channel::bounded(1);
        (Self::new(tx), rx)
    }
}

/// Builder for constructing bots with handlers
pub struct BotBuilder {
    handlers: Vec<HandlerEntry>,
}

struct HandlerEntry {
    pattern: HandlerPattern,
    handler: BoxedHandler,
    description: Option<String>,
}

#[derive(Clone)]
pub enum HandlerPattern {
    Command(String),
    Button(String),
    Message,
}

impl HandlerPattern {
    pub fn matches(&self, event_type: &str, value: &str) -> bool {
        match self {
            Self::Command(name) => event_type == "command" && name == value,
            Self::Button(pattern) => {
                if event_type != "button" {
                    return false;
                }
                if pattern.ends_with('*') {
                    value.starts_with(&pattern[..pattern.len() - 1])
                } else {
                    pattern == value
                }
            }
            Self::Message => event_type == "message",
        }
    }
}

impl BotBuilder {
    /// Create a new bot builder
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Register a command handler
    ///
    /// Handlers use the extractor/responder pattern:
    /// ```ignore
    /// // Simple handler
    /// async fn ping() -> &'static str {
    ///     "Pong!"
    /// }
    ///
    /// // With extractors
    /// async fn greet(user: User) -> String {
    ///     format!("Hello, {}!", user.name)
    /// }
    ///
    /// bot.command("ping", ping)
    ///    .command("greet", greet)
    /// ```
    pub fn command<H, Args>(mut self, name: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.handlers.push(HandlerEntry {
            pattern: HandlerPattern::Command(name.into()),
            handler: handler.into_handler(),
            description: None,
        });
        self
    }

    /// Register a command handler with a description
    ///
    /// The description is used for slash command menus (e.g., Telegram's /command list).
    pub fn command_with_description<H, Args>(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        handler: H,
    ) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.handlers.push(HandlerEntry {
            pattern: HandlerPattern::Command(name.into()),
            handler: handler.into_handler(),
            description: Some(description.into()),
        });
        self
    }

    /// Register a button handler with pattern matching
    ///
    /// Pattern can end with `*` for prefix matching (e.g., "confirm_*")
    pub fn button<H, Args>(mut self, pattern: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.handlers.push(HandlerEntry {
            pattern: HandlerPattern::Button(pattern.into()),
            handler: handler.into_handler(),
            description: None,
        });
        self
    }

    /// Register a catch-all message handler
    pub fn message<H, Args>(mut self, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.handlers.push(HandlerEntry {
            pattern: HandlerPattern::Message,
            handler: handler.into_handler(),
            description: None,
        });
        self
    }

    /// Get all registered commands with their descriptions
    ///
    /// Returns an iterator of (name, description) pairs.
    /// Commands without descriptions are included with an empty string.
    pub fn commands(&self) -> impl Iterator<Item = (&str, &str)> {
        self.handlers.iter().filter_map(|entry| {
            if let HandlerPattern::Command(name) = &entry.pattern {
                let desc = entry.description.as_deref().unwrap_or("");
                Some((name.as_str(), desc))
            } else {
                None
            }
        })
    }

    /// Find a handler matching the event type and value
    pub fn find_handler(&self, event_type: &str, value: &str) -> Option<BoxedHandler> {
        self.handlers
            .iter()
            .find(|entry| entry.pattern.matches(event_type, value))
            .map(|entry| entry.handler.clone())
    }
}

impl Default for BotBuilder {
    fn default() -> Self {
        Self::new()
    }
}
