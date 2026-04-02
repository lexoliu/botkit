use std::time::Duration;

use botkit_core::action::{ChatAction, ChatActionFuture, ChatActionSender};

use crate::client::TelegramClient;

/// Telegram chat action sender
///
/// Sends chat actions to Telegram with appropriate action strings.
#[derive(Clone)]
pub struct TelegramActionSender {
    client: TelegramClient,
    chat_id: i64,
}

impl TelegramActionSender {
    /// Create a new Telegram action sender
    pub fn new(client: TelegramClient, chat_id: i64) -> Self {
        Self { client, chat_id }
    }

    /// Map unified ChatAction to Telegram API action string
    fn action_string(action: ChatAction) -> &'static str {
        match action {
            ChatAction::Typing => "typing",
            ChatAction::UploadPhoto => "upload_photo",
            ChatAction::RecordVideo => "record_video",
            ChatAction::UploadVideo => "upload_video",
            ChatAction::RecordVoice => "record_voice",
            ChatAction::UploadVoice => "upload_voice",
            ChatAction::UploadDocument => "upload_document",
            ChatAction::ChooseSticker => "choose_sticker",
            ChatAction::FindLocation => "find_location",
            ChatAction::RecordVideoNote => "record_video_note",
            ChatAction::UploadVideoNote => "upload_video_note",
        }
    }
}

impl ChatActionSender for TelegramActionSender {
    fn send_action(&self, action: ChatAction) -> ChatActionFuture<'_> {
        Box::pin(async move {
            self.client
                .send_chat_action(self.chat_id, Self::action_string(action))
                .await
        })
    }

    fn action_expiry(&self) -> Duration {
        // Telegram typing expires after 5 seconds
        Duration::from_secs(5)
    }

    fn clone_boxed(&self) -> Box<dyn ChatActionSender> {
        Box::new(self.clone())
    }
}
