use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use async_io::Timer;
#[cfg(target_arch = "wasm32")]
use gloo_timers::future::sleep;

use crate::BotError;

#[cfg(not(target_arch = "wasm32"))]
pub type ChatActionFuture<'a> = Pin<Box<dyn Future<Output = Result<(), BotError>> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type ChatActionFuture<'a> = Pin<Box<dyn Future<Output = Result<(), BotError>> + 'a>>;

#[cfg(not(target_arch = "wasm32"))]
pub trait ChatActionSenderBounds: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> ChatActionSenderBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait ChatActionSenderBounds {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> ChatActionSenderBounds for T {}

/// Chat action types for platform indicators
///
/// Used by the framework to show the appropriate indicator (typing, uploading,
/// and so on). Platforms that only support typing map every variant onto it.
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
pub trait ChatActionSender: ChatActionSenderBounds + 'static {
    /// Send a chat action to the specified channel
    fn send_action(&self, action: ChatAction) -> ChatActionFuture<'_>;

    /// Duration after which the action indicator expires
    ///
    /// Used for auto-renewal: renew at 80% of this duration.
    fn action_expiry(&self) -> Duration;

    /// Clear the indicator early
    ///
    /// Called when the guard drops. Platforms that expire indicators on a timer
    /// (Discord, Telegram) need nothing here; Matrix uses it to retract the
    /// typing notice immediately.
    fn clear_action(&self) -> ChatActionFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}

/// RAII guard that keeps a chat action active until dropped
///
/// When created, immediately sends the action and starts auto-renewal.
/// When dropped, the renewal task stops promptly and clears the indicator.
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
    /// Closed on drop; the renewal task selects on it so it wakes immediately
    /// instead of sleeping out the rest of its interval.
    stop: async_channel::Sender<()>,
}

impl ChatActionGuard {
    /// Create and start a chat action indicator
    ///
    /// The action is sent immediately and renewed automatically until
    /// the guard is dropped.
    pub fn start(sender: Box<dyn ChatActionSender>, action: ChatAction) -> Self {
        let (stop, stopped) = async_channel::bounded::<()>(1);

        // Renew ahead of expiry so the indicator never visibly flickers. A
        // sender that reports no expiry still gets one initial send.
        let renewal_interval = sender.action_expiry().mul_f32(0.8);

        spawn_renewal(async move {
            let _ = sender.send_action(action).await;

            while !renewal_interval.is_zero() {
                // Whichever comes first: the renewal deadline, or the guard
                // dropping and closing the channel.
                let on_stop = async {
                    // Resolves once the guard drops and closes the channel.
                    while stopped.recv().await.is_ok() {}
                };

                if race(sleep_for(renewal_interval), on_stop).await.is_err() {
                    break;
                }

                if sender.send_action(action).await.is_err() {
                    return;
                }
            }

            let _ = sender.clear_action().await;
        });

        Self { stop }
    }
}

impl Drop for ChatActionGuard {
    fn drop(&mut self) {
        self.stop.close();
    }
}

/// Resolve to `Ok` if `left` finishes first, `Err` if `right` does.
async fn race<L: Future<Output = ()>, R: Future<Output = ()>>(left: L, right: R) -> Result<(), ()> {
    futures_lite::future::or(
        async {
            left.await;
            Ok(())
        },
        async {
            right.await;
            Err(())
        },
    )
    .await
}

#[cfg(not(target_arch = "wasm32"))]
async fn sleep_for(duration: Duration) {
    Timer::after(duration).await;
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_renewal(task: impl Future<Output = ()> + Send + 'static) {
    executor_core::spawn(task).detach();
}

#[cfg(target_arch = "wasm32")]
async fn sleep_for(duration: Duration) {
    sleep(duration).await;
}

#[cfg(target_arch = "wasm32")]
fn spawn_renewal(task: impl Future<Output = ()> + 'static) {
    executor_core::spawn_local(task).detach();
}
