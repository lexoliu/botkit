use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use botkit_core::action::{ChatAction, ChatActionSender};
use botkit_core::BotError;

use crate::client::DiscordClient;

/// Discord chat action sender
///
/// Discord only supports typing indicators, so all actions map to typing.
#[derive(Clone)]
pub struct DiscordActionSender {
    client: DiscordClient,
    channel_id: String,
}

impl DiscordActionSender {
    /// Create a new Discord action sender
    pub fn new(client: DiscordClient, channel_id: String) -> Self {
        Self { client, channel_id }
    }
}

impl ChatActionSender for DiscordActionSender {
    fn send_action(
        &self,
        _action: ChatAction,
    ) -> Pin<Box<dyn Future<Output = Result<(), BotError>> + Send + '_>> {
        // Discord only supports typing, ignore the action type
        Box::pin(async move { self.client.trigger_typing(&self.channel_id).await })
    }

    fn action_expiry(&self) -> Duration {
        // Discord typing expires after ~10 seconds
        Duration::from_secs(10)
    }

    fn clone_boxed(&self) -> Box<dyn ChatActionSender> {
        Box::new(self.clone())
    }
}
