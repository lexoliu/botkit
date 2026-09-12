//! `ContextData` for the CLI platform: the unified `Context` view plus the
//! full inbound payload for adapters that want it.

use std::any::Any;

use botkit_core::BotError;
use botkit_core::action::{
    AnyChatActionSender, ChatAction, ChatActionFutureBounds, ChatActionSender,
};
use botkit_core::{ContextData, OptionValue};

use crate::hub::CliHub;
use crate::wire::{Inbound, Outbound, OutboundAction};

/// Per-event context for the CLI platform.
///
/// `event` carries the full wire payload — media, stickers, reactions,
/// threads — for consumers that downcast via `Context::platform`.
pub struct CliContextData {
    /// The inbound event being dispatched.
    pub event: Inbound,
    hub: CliHub,
}

impl CliContextData {
    /// Build the context for one inbound event.
    pub(crate) fn new(event: Inbound, hub: CliHub) -> Self {
        Self { event, hub }
    }
}

impl ContextData for CliContextData {
    fn channel_id(&self) -> &str {
        self.event.chat()
    }

    fn user_id(&self) -> &str {
        self.event.user().map_or("", |u| u.id.as_str())
    }

    fn user_name(&self) -> &str {
        self.event.user().map_or("", |u| u.name.as_str())
    }

    fn command_name(&self) -> Option<&str> {
        match &self.event {
            Inbound::Command(command) => Some(command.name.as_str()),
            _ => None,
        }
    }

    fn command_args(&self) -> Option<&str> {
        match &self.event {
            Inbound::Command(command) => Some(command.args.as_str()),
            _ => None,
        }
    }

    fn option(&self, _name: &str) -> Option<OptionValue> {
        None
    }

    fn button_id(&self) -> Option<&str> {
        match &self.event {
            Inbound::Button(button) => Some(button.data.as_str()),
            _ => None,
        }
    }

    fn message_content(&self) -> Option<&str> {
        match &self.event {
            Inbound::Message(message) => message.text.as_deref().or(message.caption.as_deref()),
            Inbound::Button(button) => button.message_text.as_deref(),
            Inbound::Edited(edited) => edited.text.as_deref(),
            _ => None,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn action_sender(&self) -> Option<AnyChatActionSender> {
        Some(AnyChatActionSender::new(CliActionSender {
            hub: self.hub.clone(),
            chat: self.event.chat().to_string(),
            thread_id: self.event.thread_id(),
        }))
    }
}

/// Chat action sender that reports indicators as outbound `action` lines.
#[derive(Clone)]
pub struct CliActionSender {
    hub: CliHub,
    chat: String,
    thread_id: Option<i64>,
}

impl CliActionSender {
    /// A sender emitting action lines for `chat`, optionally in `thread_id`.
    pub fn new(hub: CliHub, chat: String, thread_id: Option<i64>) -> Self {
        Self {
            hub,
            chat,
            thread_id,
        }
    }

    fn emit(&self, action: &str, clear: bool) {
        self.hub.emit(Outbound::Action(OutboundAction {
            chat: self.chat.clone(),
            action: action.to_string(),
            clear,
            thread_id: self.thread_id,
        }));
    }
}

impl ChatActionSender for CliActionSender {
    fn send_action(
        &self,
        action: ChatAction,
    ) -> impl ChatActionFutureBounds<Output = Result<(), BotError>> + '_ {
        let name = match action {
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
        };
        self.emit(name, false);
        async { Ok(()) }
    }

    fn action_expiry(&self) -> std::time::Duration {
        // The driver renders action lines as they arrive; there is nothing
        // to renew.
        std::time::Duration::ZERO
    }

    fn clear_action(&self) -> impl ChatActionFutureBounds<Output = Result<(), BotError>> + '_ {
        self.emit("typing", true);
        async { Ok(()) }
    }
}
