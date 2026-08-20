use std::collections::HashMap;
use std::future::Future;

use crate::BotError;
use crate::handler::{BoxedHandler, IntoHandler};
use crate::shutdown::Shutdown;

// Handlers and futures are only thread-safe off wasm32, where there are no
// threads to be safe across; the bounds follow the rest of the crate.
#[cfg(not(target_arch = "wasm32"))]
pub trait BotBounds: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + ?Sized> BotBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait BotBounds {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> BotBounds for T {}

#[cfg(not(target_arch = "wasm32"))]
pub trait BotFutureBounds: Future + Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Future + Send + ?Sized> BotFutureBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait BotFutureBounds: Future {}
#[cfg(target_arch = "wasm32")]
impl<T: Future + ?Sized> BotFutureBounds for T {}

/// Unified Bot trait that hides connection mode differences
///
/// Every adapter — Discord's gateway, Telegram's long polling, and Matrix's
/// sync loop — exposes the same entry point: run until told to stop.
///
/// # Example
/// ```ignore
/// use botkit_core::{Bot, Shutdown};
///
/// // Run until the process is killed.
/// bot.run().await?;
///
/// // Or run until something else signals shutdown.
/// let (signal, shutdown) = Shutdown::channel();
/// let task = spawn(bot.run_until(shutdown));
/// signal.shutdown();
/// task.await?;
/// ```
pub trait Bot: Sized + BotBounds {
    /// Run the bot until `shutdown` fires or a fatal error occurs
    ///
    /// Returns `Ok(())` when stopped via the shutdown signal.
    fn run_until(self, shutdown: Shutdown) -> impl BotFutureBounds<Output = Result<(), BotError>>;

    /// Run the bot until a fatal error occurs
    ///
    /// Equivalent to [`Bot::run_until`] with a signal that never fires.
    fn run(self) -> impl BotFutureBounds<Output = Result<(), BotError>> {
        self.run_until(Shutdown::never())
    }
}

/// An incoming event, resolved to the shape the router dispatches on
///
/// Adapters translate their platform payload into one of these and hand it to
/// [`BotBuilder::route`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event<'a> {
    /// A command invocation, by command name (without any prefix or slash)
    Command(&'a str),
    /// A button press / callback, by its custom id
    Button(&'a str),
    /// A plain message that is not a command
    Message,
}

/// How a handler was registered
///
/// Mostly useful for introspection; routing itself goes through
/// [`BotBuilder::route`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandlerPattern {
    /// Matches a command by exact name
    Command(String),
    /// Matches a button id exactly, or by prefix when it ends with `*`
    Button(String),
    /// Matches any non-command message
    Message,
}

impl HandlerPattern {
    /// Check whether this pattern matches an event
    pub fn matches(&self, event: Event<'_>) -> bool {
        match (self, event) {
            (Self::Command(name), Event::Command(value)) => name == value,
            (Self::Button(pattern), Event::Button(value)) => match pattern.strip_suffix('*') {
                Some(prefix) => value.starts_with(prefix),
                None => pattern == value,
            },
            (Self::Message, Event::Message) => true,
            _ => false,
        }
    }
}

/// A registered command and the description shown in platform command menus
#[derive(Debug, Clone, Copy)]
pub struct CommandInfo<'a> {
    /// Command name, without any prefix or slash
    pub name: &'a str,
    /// Description, or an empty string when none was given
    pub description: &'a str,
}

/// Builder for constructing bots with handlers
///
/// Handlers are indexed as they are registered, so dispatch is a hash lookup
/// rather than a scan. When two handlers claim the same command or button id,
/// the first one registered wins.
#[derive(Default)]
pub struct BotBuilder {
    /// Command name -> handler. Also keeps registration order for `commands()`.
    commands: HashMap<String, CommandEntry>,
    /// Button ids without a wildcard, matched by equality.
    buttons: HashMap<String, BoxedHandler>,
    /// Button patterns ending in `*`, matched by prefix in registration order.
    button_prefixes: Vec<(String, BoxedHandler)>,
    /// Catch-all message handler.
    message: Option<BoxedHandler>,
}

struct CommandEntry {
    handler: BoxedHandler,
    description: Option<String>,
    /// Registration index, so `commands()` can yield a stable order.
    order: usize,
}

impl BotBuilder {
    /// Create a new bot builder
    pub fn new() -> Self {
        Self::default()
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
    pub fn command<H, Args>(self, name: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.insert_command(name.into(), None, handler.into_handler())
    }

    /// Register a command handler with a description
    ///
    /// The description is used for slash command menus (e.g., Telegram's /command list).
    pub fn command_with_description<H, Args>(
        self,
        name: impl Into<String>,
        description: impl Into<String>,
        handler: H,
    ) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.insert_command(
            name.into(),
            Some(description.into()),
            handler.into_handler(),
        )
    }

    fn insert_command(
        mut self,
        name: String,
        description: Option<String>,
        handler: BoxedHandler,
    ) -> Self {
        let order = self.commands.len();
        self.commands.entry(name).or_insert(CommandEntry {
            handler,
            description,
            order,
        });
        self
    }

    /// Register a button handler with pattern matching
    ///
    /// Pattern can end with `*` for prefix matching (e.g., "confirm_*").
    /// Exact ids take priority over prefix patterns.
    pub fn button<H, Args>(mut self, pattern: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        let pattern = pattern.into();
        let handler = handler.into_handler();

        match pattern.strip_suffix('*') {
            Some(prefix) => self.button_prefixes.push((prefix.to_string(), handler)),
            None => {
                self.buttons.entry(pattern).or_insert(handler);
            }
        }
        self
    }

    /// Register a catch-all message handler
    ///
    /// Only one message handler is used; later registrations are ignored.
    pub fn message<H, Args>(mut self, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.message.get_or_insert_with(|| handler.into_handler());
        self
    }

    /// Get all registered commands with their descriptions, in registration order
    pub fn commands(&self) -> impl Iterator<Item = CommandInfo<'_>> {
        let mut entries: Vec<_> = self.commands.iter().collect();
        entries.sort_by_key(|(_, entry)| entry.order);
        entries.into_iter().map(|(name, entry)| CommandInfo {
            name: name.as_str(),
            description: entry.description.as_deref().unwrap_or(""),
        })
    }

    /// Whether any command handler is registered
    pub fn has_commands(&self) -> bool {
        !self.commands.is_empty()
    }

    /// Find the handler that should serve an event
    ///
    /// Commands and exact button ids resolve with a single hash lookup; only
    /// wildcard button patterns fall back to a scan.
    pub fn route(&self, event: Event<'_>) -> Option<&BoxedHandler> {
        match event {
            Event::Command(name) => self.commands.get(name).map(|entry| &entry.handler),
            Event::Button(id) => self.buttons.get(id).or_else(|| {
                self.button_prefixes
                    .iter()
                    .find(|(prefix, _)| id.starts_with(prefix.as_str()))
                    .map(|(_, handler)| handler)
            }),
            Event::Message => self.message.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Context;

    async fn reply() -> &'static str {
        "reply"
    }

    fn builder() -> BotBuilder {
        BotBuilder::new()
            .command("ping", reply)
            .command_with_description("help", "Show help", reply)
            .button("exact", reply)
            .button("confirm_*", reply)
            .message(reply)
    }

    #[test]
    fn routes_commands_by_name() {
        let builder = builder();
        assert!(builder.route(Event::Command("ping")).is_some());
        assert!(builder.route(Event::Command("help")).is_some());
        assert!(builder.route(Event::Command("missing")).is_none());
    }

    #[test]
    fn routes_buttons_exactly_then_by_prefix() {
        let builder = builder();
        assert!(builder.route(Event::Button("exact")).is_some());
        assert!(builder.route(Event::Button("confirm_yes")).is_some());
        assert!(builder.route(Event::Button("confirm_")).is_some());
        assert!(builder.route(Event::Button("cancel")).is_none());
    }

    #[test]
    fn command_and_button_namespaces_do_not_collide() {
        let builder = builder();
        assert!(builder.route(Event::Button("ping")).is_none());
        assert!(builder.route(Event::Command("exact")).is_none());
    }

    #[test]
    fn message_handler_is_a_catch_all() {
        assert!(builder().route(Event::Message).is_some());
        assert!(BotBuilder::new().route(Event::Message).is_none());
    }

    #[test]
    fn first_registration_wins() {
        async fn first() -> &'static str {
            "first"
        }
        async fn second() -> &'static str {
            "second"
        }

        let builder = BotBuilder::new()
            .command("dup", first)
            .command("dup", second);
        let handler = builder.route(Event::Command("dup")).unwrap().clone();
        let response =
            futures_lite::future::block_on(handler.call(Context::new(crate::test_util::StubData)));
        assert_eq!(response.content(), Some("first"));
    }

    #[test]
    fn commands_keep_registration_order_and_descriptions() {
        let commands: Vec<_> = builder()
            .commands()
            .map(|c| (c.name.to_string(), c.description.to_string()))
            .collect();
        assert_eq!(
            commands,
            vec![
                ("ping".to_string(), String::new()),
                ("help".to_string(), "Show help".to_string()),
            ]
        );
    }

    #[test]
    fn pattern_matching_mirrors_routing() {
        assert!(HandlerPattern::Command("ping".into()).matches(Event::Command("ping")));
        assert!(!HandlerPattern::Command("ping".into()).matches(Event::Button("ping")));
        assert!(HandlerPattern::Button("confirm_*".into()).matches(Event::Button("confirm_yes")));
        assert!(!HandlerPattern::Button("confirm_*".into()).matches(Event::Button("deny")));
        assert!(HandlerPattern::Message.matches(Event::Message));
    }
}
