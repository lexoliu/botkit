use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use botkit_core::BotError;
use botkit_core::action::{ChatAction, ChatActionSender};
use matrix_sdk::Room;

/// Matrix chat action sender
///
/// Sends typing notifications to Matrix rooms.
#[derive(Clone)]
pub struct MatrixActionSender {
    room: Room,
}

impl MatrixActionSender {
    /// Create a new Matrix action sender for the given room
    pub fn new(room: Room) -> Self {
        Self { room }
    }
}

impl ChatActionSender for MatrixActionSender {
    fn send_action(
        &self,
        action: ChatAction,
    ) -> Pin<Box<dyn Future<Output = Result<(), BotError>> + Send + '_>> {
        Box::pin(async move {
            // Matrix only supports typing indicators
            if action == ChatAction::Typing {
                self.room
                    .typing_notice(true)
                    .await
                    .map_err(|e| BotError::Api(e.to_string()))?;
            }
            // Other actions are silently ignored - Matrix doesn't support them
            Ok(())
        })
    }

    fn action_expiry(&self) -> Duration {
        // Matrix typing notifications last about 30 seconds
        // Renew more frequently for responsiveness
        Duration::from_secs(20)
    }

    fn clone_boxed(&self) -> Box<dyn ChatActionSender> {
        Box::new(self.clone())
    }
}
