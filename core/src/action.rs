use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_io::Timer;

use crate::BotError;

/// Chat action types for platform indicators
///
/// Internal enum - not exported publicly. Used by framework
/// to show appropriate indicators (typing, uploading, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatAction {
    /// User is typing a message
    Typing,
    /// User is uploading a photo
    UploadPhoto,
    /// User is recording a video
    RecordVideo,
    /// User is uploading a video
    UploadVideo,
    /// User is recording audio/voice
    RecordVoice,
    /// User is uploading audio/voice
    UploadVoice,
    /// User is uploading a document
    UploadDocument,
    /// User is choosing a sticker
    ChooseSticker,
    /// User is finding a location
    FindLocation,
    /// User is recording a video note
    RecordVideoNote,
    /// User is uploading a video note
    UploadVideoNote,
}

/// Trait for sending chat actions to a channel
///
/// Platform implementations define how to send actions and their expiration times.
pub trait ChatActionSender: Send + Sync + 'static {
    /// Send a chat action to the specified channel
    fn send_action(
        &self,
        action: ChatAction,
    ) -> Pin<Box<dyn Future<Output = Result<(), BotError>> + Send + '_>>;

    /// Duration after which the action indicator expires
    ///
    /// Used for auto-renewal: renew at 80% of this duration.
    fn action_expiry(&self) -> Duration;

    /// Clone this sender into a boxed trait object
    fn clone_boxed(&self) -> Box<dyn ChatActionSender>;
}

/// RAII guard that keeps a chat action active until dropped
///
/// When created, immediately sends the action and starts auto-renewal.
/// When dropped, signals the background task to stop.
///
/// # Example
/// ```ignore
/// async fn slow_command(ctx: Context) -> String {
///     let _typing = ctx.typing();  // Starts typing indicator
///     expensive_work().await;
///     "Done!"
/// }  // Typing stops when _typing is dropped
/// ```
pub struct ChatActionGuard {
    stop_flag: Arc<AtomicBool>,
}

impl ChatActionGuard {
    /// Create and start a chat action indicator
    ///
    /// The action is sent immediately and renewed automatically until
    /// the guard is dropped.
    pub fn start(sender: Box<dyn ChatActionSender>, action: ChatAction) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&stop_flag);

        // Calculate renewal interval (80% of expiry time)
        let expiry = sender.action_expiry();
        let renewal_interval = Duration::from_millis((expiry.as_millis() as u64 * 80) / 100);

        executor_core::spawn(async move {
            // Send initial action
            let _ = sender.send_action(action).await;

            loop {
                // Sleep for renewal interval
                Timer::after(renewal_interval).await;

                // Check if we should stop
                if flag_clone.load(Ordering::Acquire) {
                    break;
                }

                // Renew the action
                if sender.send_action(action).await.is_err() {
                    break;
                }
            }
        })
        .detach();

        Self { stop_flag }
    }
}

impl Drop for ChatActionGuard {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
    }
}
