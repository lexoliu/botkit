use thiserror::Error;

/// Main error type for bot operations
///
/// A clean shutdown is not an error: [`crate::Bot::run_until`] returns `Ok(())`
/// when its signal fires.
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

    #[error("{0}")]
    Other(String),
}
