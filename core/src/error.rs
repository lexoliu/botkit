use thiserror::Error;

/// Main error type for bot operations
#[derive(Debug, Error)]
pub enum BotError {
    #[error("connection failed: {0}")]
    Connection(String),

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("API request failed: {0}")]
    Api(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("handler error: {0}")]
    Handler(String),

    #[error("shutdown requested")]
    Shutdown,

    #[error("{0}")]
    Other(String),
}
