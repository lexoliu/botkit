use std::any::Any;
use std::sync::Arc;

/// Context for handling bot events
///
/// Provides access to event data and platform client. Platform-specific
/// details are abstracted away - handlers work with a unified API.
#[derive(Clone)]
pub struct Context {
    inner: Arc<dyn ContextData>,
}

impl Context {
    /// Create a new context (called by platform implementations)
    pub fn new<T: ContextData>(data: T) -> Self {
        Self {
            inner: Arc::new(data),
        }
    }

    /// Get the channel/chat ID
    pub fn channel_id(&self) -> &str {
        self.inner.channel_id()
    }

    /// Get the user ID who triggered the event
    pub fn user_id(&self) -> &str {
        self.inner.user_id()
    }

    /// Get the user's display name
    pub fn user_name(&self) -> &str {
        self.inner.user_name()
    }

    /// Get the command name if this is a command event
    pub fn command_name(&self) -> Option<&str> {
        self.inner.command_name()
    }

    /// Get command arguments as a string (for Telegram-style commands)
    pub fn command_args(&self) -> Option<&str> {
        self.inner.command_args()
    }

    /// Get a command option value by name
    pub fn option(&self, name: &str) -> Option<OptionValue> {
        self.inner.option(name)
    }

    /// Get the button/callback custom_id if this is a button event
    pub fn button_id(&self) -> Option<&str> {
        self.inner.button_id()
    }

    /// Get the message content if available
    pub fn message_content(&self) -> Option<&str> {
        self.inner.message_content()
    }

    /// Access platform-specific raw data (for advanced use cases)
    ///
    /// Returns `Some(&T)` if the underlying platform data is of type `T`.
    pub fn platform<T: 'static>(&self) -> Option<&T> {
        self.inner.as_any().downcast_ref::<T>()
    }

    /// Get the inner context data for platform implementations
    pub fn data(&self) -> &dyn ContextData {
        &*self.inner
    }
}

/// Command option value
#[derive(Debug, Clone)]
pub enum OptionValue {
    String(String),
    Integer(i64),
    Boolean(bool),
    Number(f64),
}

impl OptionValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }
}

/// Trait implemented by platform-specific context data
///
/// Platform implementations provide this to expose event data
/// through the unified Context API.
pub trait ContextData: Send + Sync + 'static {
    fn channel_id(&self) -> &str;
    fn user_id(&self) -> &str;
    fn user_name(&self) -> &str;
    fn command_name(&self) -> Option<&str>;
    fn command_args(&self) -> Option<&str>;
    fn option(&self, name: &str) -> Option<OptionValue>;
    fn button_id(&self) -> Option<&str>;
    fn message_content(&self) -> Option<&str>;
    fn as_any(&self) -> &dyn Any;
}
