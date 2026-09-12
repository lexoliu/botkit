use std::time::Duration;

use botkit_core::BotError;
use botkit_core::action::{ChatAction, ChatActionFutureBounds, ChatActionSender};

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
    ) -> impl ChatActionFutureBounds<Output = Result<(), BotError>> + '_ {
        // Discord only supports typing, ignore the action type
        async move { self.client.trigger_typing(&self.channel_id).await }
    }

    fn action_expiry(&self) -> Duration {
        // Discord typing expires after ~10 seconds
        Duration::from_secs(10)
    }
}
