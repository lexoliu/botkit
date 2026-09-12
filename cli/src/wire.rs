//! The JSONL wire protocol shared by the bot-side transport and the
//! `botkit-cli` driver binary.
//!
//! Inbound lines (driver -> bot) are [`Inbound`] events. Outbound lines
//! (bot -> driver) are [`Outbound`] actions. One JSON object per line, on
//! stdin/stdout or a unix-socket connection.

use serde::{Deserialize, Serialize};

/// A user attached to an inbound event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireUser {
    /// Platform user id.
    pub id: String,
    /// Display name.
    pub name: String,
}

/// A file attached to an inbound message.
///
/// The CLI platform has no remote file store: `path` is a local file the
/// bot may read directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireFile {
    /// Media kind: `photo`, `video`, `audio`, `voice`, `animation`,
    /// `document`, or `sticker`.
    pub kind: String,
    /// Local filesystem path.
    pub path: String,
    /// MIME type; inferred from `path` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    /// Stable identifier for resend-by-id flows; defaults to `path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
}

/// The message an inbound message replies to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReplyRef {
    /// Replied-to message id.
    pub message_id: i64,
    /// Replied-to author display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Replied-to text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// A sticker attached to an inbound message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireSticker {
    /// Sticker identifier for resend-by-id.
    pub file_id: String,
    /// Associated emoji.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    /// Owning set name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_name: Option<String>,
    /// `static`, `animated`, or `video`.
    #[serde(default = "default_static")]
    pub format: String,
    /// Local file backing the sticker, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

fn default_static() -> String {
    "static".to_string()
}

/// A plain or media message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InboundMessage {
    /// Chat/channel id.
    pub chat: String,
    /// Author.
    pub user: WireUser,
    /// Message id; assigned by the hub when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<i64>,
    /// Message text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Media caption, when the message carries files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Forum topic the message belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<i64>,
    /// The message this one replies to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<WireReplyRef>,
    /// Attached files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<WireFile>,
    /// Attached sticker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sticker: Option<WireSticker>,
    /// Marks the message as room chatter not aimed at the bot — the way a
    /// group message without a reply/mention looks. Consumers may use it to
    /// flag the event `ambient` rather than `direct`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ambient: bool,
}

/// A `/name args` invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InboundCommand {
    /// Chat/channel id.
    pub chat: String,
    /// Invoking user.
    pub user: WireUser,
    /// Command name, without prefix.
    pub name: String,
    /// Raw argument string.
    #[serde(default)]
    pub args: String,
    /// Message id of the invocation; assigned by the hub when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<i64>,
    /// Forum topic the command was sent in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<i64>,
}

/// An inline-button press.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InboundButton {
    /// Chat/channel id.
    pub chat: String,
    /// Pressing user.
    pub user: WireUser,
    /// The button's callback data.
    pub data: String,
    /// Message the button was attached to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<i64>,
    /// Text of the message the button was attached to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_text: Option<String>,
    /// Forum topic of the button's message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<i64>,
}

/// Reactions added and removed on a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InboundReaction {
    /// Chat/channel id.
    pub chat: String,
    /// Reacting user.
    pub user: WireUser,
    /// Message being reacted to.
    pub message_id: i64,
    /// Reactions that appeared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    /// Reactions that disappeared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
}

/// A message whose text was edited.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InboundEdited {
    /// Chat/channel id.
    pub chat: String,
    /// Editing user.
    pub user: WireUser,
    /// Edited message id.
    pub message_id: i64,
    /// New text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Forum topic of the message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<i64>,
}

/// An event a driver injects into the bot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Inbound {
    /// Register this connection as an outbound stream. Only meaningful on
    /// the socket transport; on stdio the single stream is implicit.
    Subscribe,
    /// A plain or media message.
    Message(Box<InboundMessage>),
    /// A command invocation.
    Command(InboundCommand),
    /// An inline-button press.
    Button(InboundButton),
    /// A reaction update.
    Reaction(InboundReaction),
    /// A message edit.
    Edited(InboundEdited),
}

impl Inbound {
    /// The chat this event targets.
    pub fn chat(&self) -> &str {
        match self {
            Self::Message(e) => &e.chat,
            Self::Command(e) => &e.chat,
            Self::Button(e) => &e.chat,
            Self::Reaction(e) => &e.chat,
            Self::Edited(e) => &e.chat,
            Self::Subscribe => "",
        }
    }

    /// The acting user.
    pub fn user(&self) -> Option<&WireUser> {
        match self {
            Self::Message(e) => Some(&e.user),
            Self::Command(e) => Some(&e.user),
            Self::Button(e) => Some(&e.user),
            Self::Reaction(e) => Some(&e.user),
            Self::Edited(e) => Some(&e.user),
            Self::Subscribe => None,
        }
    }

    /// The message id attached to this event, if any.
    pub fn message_id(&self) -> Option<i64> {
        match self {
            Self::Message(e) => e.message_id,
            Self::Command(e) => e.message_id,
            Self::Button(e) => e.message_id,
            Self::Reaction(e) => Some(e.message_id),
            Self::Edited(e) => Some(e.message_id),
            Self::Subscribe => None,
        }
    }

    /// The forum topic this event belongs to.
    pub fn thread_id(&self) -> Option<i64> {
        match self {
            Self::Message(e) => e.thread_id,
            Self::Command(e) => e.thread_id,
            Self::Button(e) => e.thread_id,
            Self::Edited(e) => e.thread_id,
            Self::Reaction(_) | Self::Subscribe => None,
        }
    }

    /// Assign a message id where the driver left it out.
    pub(crate) fn ensure_message_id(&mut self, next: impl Fn() -> i64) {
        let slot = match self {
            Self::Message(e) => &mut e.message_id,
            Self::Command(e) => &mut e.message_id,
            Self::Button(e) => &mut e.message_id,
            _ => return,
        };
        if slot.is_none() {
            *slot = Some(next());
        }
    }
}

/// One inline-keyboard button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireButton {
    /// Button label.
    pub text: String,
    /// Callback data (mutually exclusive with `url`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// Link target (mutually exclusive with `data`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A text message the bot sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundMessage {
    /// Chat/channel id.
    pub chat: String,
    /// Assigned message id.
    pub message_id: i64,
    /// Message text.
    pub text: String,
    /// Inline keyboard rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buttons: Vec<Vec<WireButton>>,
    /// Message this one replies to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<i64>,
    /// Forum topic the message was sent to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<i64>,
    /// Platform extras that have no CLI-native shape (embeds, select menus).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// A file the bot sent. `path` stays a local reference — the platform does
/// not copy bytes — while in-memory payloads travel base64'd in `data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundFile {
    /// Chat/channel id.
    pub chat: String,
    /// Assigned message id.
    pub message_id: i64,
    /// Media kind.
    pub kind: String,
    /// Local path, when the payload lives on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Base64 payload, when the bytes were in memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// Suggested filename.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Caption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Forum topic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<i64>,
}

/// Reaction(s) the bot set on a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundReaction {
    /// Chat/channel id.
    pub chat: String,
    /// Target message.
    pub message_id: i64,
    /// Emojis now on the message.
    pub emojis: Vec<String>,
    /// Animated/big variant.
    #[serde(default)]
    pub is_big: bool,
}

/// A message the bot edited.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundEdit {
    /// Chat/channel id.
    pub chat: String,
    /// Edited message.
    pub message_id: i64,
    /// New text.
    pub text: String,
}

/// A message the bot deleted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundDelete {
    /// Chat/channel id.
    pub chat: String,
    /// Deleted message.
    pub message_id: i64,
}

/// A message the bot pinned or unpinned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundPin {
    /// Chat/channel id.
    pub chat: String,
    /// Target message.
    pub message_id: i64,
    /// `true` for unpin.
    #[serde(default)]
    pub unpin: bool,
    /// Whether the pin notified the chat.
    #[serde(default)]
    pub notify: bool,
}

/// A chat action indicator (typing and friends).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundAction {
    /// Chat/channel id.
    pub chat: String,
    /// Action name, e.g. `typing`.
    pub action: String,
    /// `true` when the indicator was cleared early.
    #[serde(default)]
    pub clear: bool,
    /// Forum topic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<i64>,
}

/// An injected event was accepted for dispatch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundAck {
    /// The message id the event carries (assigned if the driver omitted it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<i64>,
}

/// An inbound line failed to parse or an event could not be delivered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundError {
    /// What went wrong.
    pub message: String,
}

/// An action the bot performed, streamed to subscribers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Outbound {
    /// A text message.
    Message(OutboundMessage),
    /// A file/media message.
    File(OutboundFile),
    /// Reaction(s) set on a message.
    Reaction(OutboundReaction),
    /// A message edit.
    Edit(OutboundEdit),
    /// A message deletion.
    Delete(OutboundDelete),
    /// A pin or unpin.
    Pin(OutboundPin),
    /// A chat action indicator.
    Action(OutboundAction),
    /// An injected event was accepted.
    Ack(OutboundAck),
    /// A driver-facing error.
    Error(OutboundError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> WireUser {
        WireUser {
            id: "u1".to_string(),
            name: "alice".to_string(),
        }
    }

    #[test]
    fn inbound_message_round_trips() {
        let event = Inbound::Message(Box::new(InboundMessage {
            chat: "c1".to_string(),
            user: user(),
            message_id: Some(7),
            text: Some("hi".to_string()),
            caption: None,
            thread_id: Some(3),
            reply_to: None,
            files: vec![WireFile {
                kind: "photo".to_string(),
                path: "/tmp/x.png".to_string(),
                mime: None,
                file_id: None,
            }],
            sticker: None,
            ambient: false,
        }));
        let line = serde_json::to_string(&event).unwrap();
        let parsed: Inbound = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed, event);
        assert_eq!(parsed.chat(), "c1");
        assert_eq!(parsed.message_id(), Some(7));
        assert_eq!(parsed.thread_id(), Some(3));
    }

    #[test]
    fn inbound_tagged_variants_parse() {
        let command: Inbound = serde_json::from_str(
            r#"{"type":"command","chat":"c1","user":{"id":"u","name":"n"},"name":"ping","args":"a b"}"#,
        )
        .unwrap();
        assert!(matches!(command, Inbound::Command(_)));

        let button: Inbound = serde_json::from_str(
            r#"{"type":"button","chat":"c1","user":{"id":"u","name":"n"},"data":"yes"}"#,
        )
        .unwrap();
        assert!(matches!(button, Inbound::Button(_)));

        let reaction: Inbound = serde_json::from_str(
            r#"{"type":"reaction","chat":"c1","user":{"id":"u","name":"n"},"message_id":9,"added":["👍"]}"#,
        )
        .unwrap();
        match reaction {
            Inbound::Reaction(r) => assert_eq!(r.added, ["👍"]),
            other => panic!("expected reaction, got {other:?}"),
        }
    }

    #[test]
    fn outbound_message_omits_empty_fields() {
        let outbound = Outbound::Message(OutboundMessage {
            chat: "c1".to_string(),
            message_id: 1,
            text: "hi".to_string(),
            buttons: vec![],
            reply_to: None,
            thread_id: None,
            extras: None,
        });
        let json = serde_json::to_value(&outbound).unwrap();
        assert_eq!(json["type"], "message");
        assert!(json.get("buttons").is_none());
        assert!(json.get("thread_id").is_none());
    }

    #[test]
    fn ensure_message_id_fills_missing() {
        let mut event = Inbound::Message(Box::new(InboundMessage {
            chat: "c".to_string(),
            user: user(),
            message_id: None,
            text: None,
            caption: None,
            thread_id: None,
            reply_to: None,
            files: vec![],
            sticker: None,
            ambient: false,
        }));
        event.ensure_message_id(|| 42);
        assert_eq!(event.message_id(), Some(42));

        let mut fixed = Inbound::Edited(InboundEdited {
            chat: "c".to_string(),
            user: user(),
            message_id: 5,
            text: None,
            thread_id: None,
        });
        fixed.ensure_message_id(|| 99);
        assert_eq!(fixed.message_id(), Some(5));
    }
}
