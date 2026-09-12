//! Shared state between the transport, the dispatcher, and platform senders.
//!
//! The hub owns three things: the inbound queue transports push events onto,
//! the set of outbound sinks (subscribed socket connections, or stdout in
//! stdio mode), and the message-id counter for both directions.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use async_channel::{Receiver, Sender};

use crate::wire::{Inbound, Outbound};

/// Shared CLI-platform state.
///
/// Clone it freely: the dispatcher, `CliActionSender`s, acpbot-style
/// platform senders, and transports all hold their own handle.
#[derive(Clone)]
pub struct CliHub {
    inner: Arc<HubInner>,
}

struct HubInner {
    inbound: Sender<Inbound>,
    sinks: Mutex<Vec<Sender<String>>>,
    next_message_id: AtomicI64,
}

impl CliHub {
    /// Create a hub plus the receiver the dispatcher drains.
    pub(crate) fn new() -> (Self, Receiver<Inbound>) {
        let (inbound_tx, inbound_rx) = async_channel::unbounded();
        let hub = Self {
            inner: Arc::new(HubInner {
                inbound: inbound_tx,
                sinks: Mutex::new(Vec::new()),
                next_message_id: AtomicI64::new(1),
            }),
        };
        (hub, inbound_rx)
    }

    /// Assign the next platform message id.
    pub fn next_message_id(&self) -> i64 {
        self.inner.next_message_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Queue an inbound event for dispatch, assigning a message id where the
    /// driver omitted one. Returns the id the event carries.
    ///
    /// `Inbound::Subscribe` never reaches this function — transports turn it
    /// into a sink registration instead.
    pub(crate) fn inject(&self, mut event: Inbound) -> i64 {
        event.ensure_message_id(|| self.next_message_id());
        let id = event.message_id().unwrap_or(0);
        if self.inner.inbound.try_send(event).is_err() {
            self.emit(Outbound::Error(crate::wire::OutboundError {
                message: "dispatcher is gone; event dropped".to_string(),
            }));
        }
        id
    }

    /// Register a new outbound sink. The returned receiver yields one JSONL
    /// line per outbound action until unsubscribed (channel dropped).
    pub fn subscribe(&self) -> Receiver<String> {
        let (tx, rx) = async_channel::unbounded();
        self.inner.sinks.lock().expect("sinks poisoned").push(tx);
        rx
    }

    /// Serialize an outbound action and fan it out to every sink. Dead sinks
    /// are pruned.
    pub fn emit(&self, outbound: Outbound) {
        let Ok(line) = serde_json::to_string(&outbound) else {
            return;
        };
        self.emit_line(&line);
    }

    /// Push a pre-serialized line to every sink.
    pub fn emit_line(&self, line: &str) {
        let mut sinks = self.inner.sinks.lock().expect("sinks poisoned");
        sinks.retain(|sink| sink.try_send(line.to_string()).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{InboundMessage, OutboundMessage, WireUser};

    fn event() -> Inbound {
        Inbound::Message(Box::new(InboundMessage {
            chat: "c".to_string(),
            user: WireUser {
                id: "u".to_string(),
                name: "n".to_string(),
            },
            message_id: None,
            text: Some("hi".to_string()),
            caption: None,
            thread_id: None,
            reply_to: None,
            files: vec![],
            sticker: None,
            ambient: false,
        }))
    }

    #[test]
    fn inject_assigns_message_ids() {
        let (hub, rx) = CliHub::new();
        let first = hub.inject(event());
        let second = hub.inject(event());
        assert_eq!((first, second), (1, 2));
        assert_eq!(rx.try_recv().unwrap().message_id(), Some(1));
        assert_eq!(rx.try_recv().unwrap().message_id(), Some(2));
    }

    #[test]
    fn emit_reaches_subscribers_as_jsonl() {
        let (hub, _rx) = CliHub::new();
        let sink = hub.subscribe();
        hub.emit(Outbound::Message(OutboundMessage {
            chat: "c".to_string(),
            message_id: 1,
            text: "hello".to_string(),
            buttons: vec![],
            reply_to: None,
            thread_id: None,
            extras: None,
        }));
        let line = sink.try_recv().unwrap();
        let parsed: Outbound = serde_json::from_str(&line).unwrap();
        assert!(matches!(parsed, Outbound::Message(_)));
    }

    #[test]
    fn dead_sinks_are_pruned() {
        let (hub, _rx) = CliHub::new();
        let sink = hub.subscribe();
        drop(sink);
        hub.emit(Outbound::Ack(crate::wire::OutboundAck { message_id: None }));
        assert!(hub.inner.sinks.lock().unwrap().is_empty());
    }
}
