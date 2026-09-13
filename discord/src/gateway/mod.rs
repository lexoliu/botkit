mod intents;

pub use intents::GatewayIntents;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use async_io::Timer;
use executor_core::spawn;
use serde::Deserialize;
use serde_json::Value;
use zenwave::websocket::{WebSocketMessage, WebSocketReceiver, WebSocketSender};

#[cfg(test)]
use crate::types::InteractionType;
use crate::types::{Interaction, Message};

const GATEWAY_QUERY: &str = "?v=10&encoding=json";
const DEFAULT_GATEWAY_URL: &str = "wss://gateway.discord.gg";

/// Sentinel for "no event sequence received yet".
const NO_SEQUENCE: u64 = u64::MAX;

/// Gateway opcodes we act on.
mod op {
    pub const DISPATCH: u8 = 0;
    pub const HEARTBEAT: u8 = 1;
    pub const RECONNECT: u8 = 7;
    pub const INVALID_SESSION: u8 = 9;
    pub const HELLO: u8 = 10;
    pub const HEARTBEAT_ACK: u8 = 11;
}

/// Discord Gateway connection manager
pub struct Gateway {
    token: String,
    intents: GatewayIntents,
}

impl Gateway {
    /// Create a new gateway connection manager
    pub fn new(token: impl Into<String>, intents: GatewayIntents) -> Self {
        Self {
            token: token.into(),
            intents,
        }
    }

    /// Open a fresh gateway connection and identify as a new session
    pub async fn connect(&self) -> Result<GatewayConnection, GatewayError> {
        self.open(format!("{DEFAULT_GATEWAY_URL}/{GATEWAY_QUERY}"), None)
            .await
    }

    /// Reopen a connection and replay missed events for an existing session
    ///
    /// Discord replays every event after `session.sequence`, so no dispatches
    /// are lost across a reconnect.
    pub async fn resume(&self, session: &Session) -> Result<GatewayConnection, GatewayError> {
        let url = format!("{}/{GATEWAY_QUERY}", session.resume_gateway_url);
        self.open(url, Some(session.clone())).await
    }

    async fn open(
        &self,
        url: String,
        resume: Option<Session>,
    ) -> Result<GatewayConnection, GatewayError> {
        let ws = zenwave::websocket::connect(&url)
            .await
            .map_err(|e| GatewayError::Connection(e.to_string()))?;
        let (sender, receiver) = ws.split();

        let sequence = Arc::new(AtomicU64::new(
            resume.as_ref().map_or(NO_SEQUENCE, |s| s.sequence),
        ));

        Ok(GatewayConnection {
            sender,
            receiver,
            token: self.token.clone(),
            intents: self.intents,
            session: resume,
            sequence,
            heartbeat: None,
        })
    }
}

/// Everything needed to resume a dropped session
#[derive(Debug, Clone)]
pub struct Session {
    /// Session id issued in the `READY` payload
    pub id: String,
    /// Session-specific gateway URL Discord asks us to resume against
    pub resume_gateway_url: String,
    /// Last event sequence seen on this session
    pub sequence: u64,
}

/// Active gateway connection
pub struct GatewayConnection {
    sender: WebSocketSender,
    receiver: WebSocketReceiver,
    token: String,
    intents: GatewayIntents,
    session: Option<Session>,
    sequence: Arc<AtomicU64>,
    heartbeat: Option<HeartbeatLoop>,
}

impl GatewayConnection {
    /// The session to hand to [`Gateway::resume`], if this connection has one
    pub fn session(&self) -> Option<Session> {
        let mut session = self.session.clone()?;
        session.sequence = self.sequence.load(Ordering::Acquire);
        Some(session)
    }

    /// Receive the next gateway event
    ///
    /// Heartbeats, hellos, and acks are handled internally; this only returns
    /// once something the bot cares about arrives.
    pub async fn recv(&mut self) -> Result<GatewayEvent, GatewayError> {
        loop {
            // A zombie connection (heartbeats stopped being acked) is closed by
            // the heartbeat task, so this resolves instead of hanging forever.
            let msg = self
                .receiver
                .recv()
                .await
                .map_err(|e| GatewayError::Connection(e.to_string()))?;

            let Some(msg) = msg else {
                return Err(GatewayError::Closed);
            };

            let text = match msg {
                WebSocketMessage::Text(t) => t.to_string(),
                WebSocketMessage::Binary(b) => String::from_utf8(b.to_vec())
                    .map_err(|e| GatewayError::Protocol(e.to_string()))?,
                WebSocketMessage::Close => return Err(GatewayError::Closed),
                _ => continue,
            };

            let payload: GatewayPayload =
                serde_json::from_str(&text).map_err(|e| GatewayError::Protocol(e.to_string()))?;

            if let Some(s) = payload.s {
                self.sequence.store(s, Ordering::Release);
            }

            match payload.op {
                op::DISPATCH => {
                    if let Some(event) = self.handle_dispatch(payload.t.as_deref(), payload.d)? {
                        return Ok(event);
                    }
                }
                op::HEARTBEAT => self.send_heartbeat().await?,
                op::RECONNECT => return Ok(GatewayEvent::Reconnect),
                // `d` is true when the session may still be resumed.
                op::INVALID_SESSION => {
                    let resumable = payload.d.and_then(|d| d.as_bool()).unwrap_or(false);
                    if !resumable {
                        self.session = None;
                    }
                    return Ok(GatewayEvent::InvalidSession { resumable });
                }
                op::HELLO => {
                    if let Some(interval) = payload
                        .d
                        .as_ref()
                        .and_then(|d| d.get("heartbeat_interval"))
                        .and_then(Value::as_u64)
                    {
                        self.start_heartbeating(Duration::from_millis(interval));
                    }

                    // Resuming replays missed events; identifying starts fresh.
                    match self.session.clone() {
                        Some(session) => self.send_resume(&session).await?,
                        None => self.identify().await?,
                    }
                }
                op::HEARTBEAT_ACK => {
                    if let Some(heartbeat) = &self.heartbeat {
                        heartbeat.acknowledge();
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_dispatch(
        &mut self,
        event_name: Option<&str>,
        data: Option<Value>,
    ) -> Result<Option<GatewayEvent>, GatewayError> {
        let (Some(name), Some(data)) = (event_name, data) else {
            return Ok(None);
        };

        // A payload we can't parse is one bad event, not a broken connection:
        // Discord adds fields over time, and dropping the connection over an
        // unfamiliar one would take the bot down with it.
        match name {
            "READY" => {
                let Some(ready) = parse::<Ready>(name, data) else {
                    return Ok(None);
                };
                self.session = Some(Session {
                    id: ready.session_id,
                    resume_gateway_url: ready
                        .resume_gateway_url
                        .unwrap_or_else(|| DEFAULT_GATEWAY_URL.to_string()),
                    sequence: self.sequence.load(Ordering::Acquire),
                });
                Ok(Some(GatewayEvent::Ready))
            }
            "RESUMED" => Ok(Some(GatewayEvent::Resumed)),
            "INTERACTION_CREATE" => Ok(parse::<Interaction>(name, data)
                .map(|interaction| GatewayEvent::InteractionCreate(Box::new(interaction)))),
            "MESSAGE_CREATE" => Ok(parse::<Message>(name, data)
                // Bot and webhook chatter would otherwise loop back into the
                // bot's own message handlers.
                .filter(|message| !message.author.bot.unwrap_or(false))
                .map(|message| GatewayEvent::MessageCreate(Box::new(message)))),
            "MESSAGE_UPDATE" => Ok(parse::<Message>(name, data)
                .filter(|message| !message.author.bot.unwrap_or(false))
                .map(|message| GatewayEvent::MessageUpdate(Box::new(message)))),
            _ => Ok(None),
        }
    }

    async fn identify(&mut self) -> Result<(), GatewayError> {
        let identify = serde_json::json!({
            "op": 2,
            "d": {
                "token": self.token,
                "intents": self.intents.bits(),
                "properties": {
                    "os": std::env::consts::OS,
                    "browser": "botkit",
                    "device": "botkit"
                }
            }
        });

        self.send(&identify).await
    }

    async fn send_resume(&mut self, session: &Session) -> Result<(), GatewayError> {
        let resume = serde_json::json!({
            "op": 6,
            "d": {
                "token": self.token,
                "session_id": session.id,
                "seq": last_sequence(&self.sequence),
            }
        });

        self.send(&resume).await
    }

    async fn send(&self, payload: &Value) -> Result<(), GatewayError> {
        self.sender
            .send_text(payload.to_string())
            .await
            .map_err(|e| GatewayError::Connection(e.to_string()))
    }

    /// Send a heartbeat
    pub async fn send_heartbeat(&mut self) -> Result<(), GatewayError> {
        send_heartbeat_frame(&self.sender, &self.sequence).await
    }

    fn start_heartbeating(&mut self, interval: Duration) {
        if self.heartbeat.is_some() {
            return;
        }

        self.heartbeat = Some(HeartbeatLoop::spawn(
            self.sender.clone(),
            Arc::clone(&self.sequence),
            interval,
        ));
    }

    /// Close the connection (consumes self)
    pub async fn close(mut self) -> Result<(), GatewayError> {
        self.heartbeat.take();
        self.sender
            .close()
            .await
            .map_err(|e| GatewayError::Connection(e.to_string()))
    }
}

/// Deserialize a dispatch payload, logging and dropping ones we can't read.
fn parse<T: serde::de::DeserializeOwned>(event: &str, data: Value) -> Option<T> {
    match serde_json::from_value(data) {
        Ok(value) => Some(value),
        Err(e) => {
            tracing::warn!("Ignoring malformed {event} payload: {e}");
            None
        }
    }
}

fn last_sequence(sequence: &AtomicU64) -> Option<u64> {
    match sequence.load(Ordering::Acquire) {
        NO_SEQUENCE => None,
        value => Some(value),
    }
}

async fn send_heartbeat_frame(
    sender: &WebSocketSender,
    sequence: &AtomicU64,
) -> Result<(), GatewayError> {
    let heartbeat = serde_json::json!({ "op": 1, "d": last_sequence(sequence) });

    sender
        .send_text(heartbeat.to_string())
        .await
        .map_err(|e| GatewayError::Connection(e.to_string()))
}

/// Gateway payload structure
#[derive(Debug, Deserialize)]
struct GatewayPayload {
    op: u8,
    d: Option<Value>,
    s: Option<u64>,
    t: Option<String>,
}

/// The parts of `READY` needed to resume later.
#[derive(Debug, Deserialize)]
struct Ready {
    session_id: String,
    resume_gateway_url: Option<String>,
}

/// Gateway events
#[derive(Debug)]
pub enum GatewayEvent {
    /// A new session was established
    Ready,
    /// A previous session was resumed and its missed events replayed
    Resumed,
    /// A slash command, button press, or other interaction
    InteractionCreate(Box<Interaction>),
    /// A message from a non-bot author
    MessageCreate(Box<Message>),
    /// A message was edited — the payload carries the post-edit state
    MessageUpdate(Box<Message>),
    /// Discord asked us to reconnect, or the connection went silent
    Reconnect,
    /// The session is gone; `resumable` says whether a resume may still work
    InvalidSession { resumable: bool },
}

/// Gateway errors
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("connection error: {0}")]
    Connection(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("connection closed")]
    Closed,
}

/// Background heartbeat sender.
///
/// Also watches whether Discord is still acking. An unacknowledged heartbeat
/// means the socket is a zombie — open, but no longer carrying events — so the
/// task closes it, which surfaces to `recv` as a closed connection and lets the
/// bot resume the session on a fresh socket.
struct HeartbeatLoop {
    stop: async_channel::Sender<()>,
    /// Set when a heartbeat is sent, cleared when Discord acks it.
    awaiting_ack: Arc<AtomicBool>,
}

impl HeartbeatLoop {
    fn spawn(sender: WebSocketSender, sequence: Arc<AtomicU64>, interval: Duration) -> Self {
        let (stop, stopped) = async_channel::bounded::<()>(1);
        let awaiting_ack = Arc::new(AtomicBool::new(false));
        let task_awaiting_ack = Arc::clone(&awaiting_ack);

        spawn(async move {
            loop {
                // Wake on whichever comes first: the next heartbeat deadline,
                // or the connection being dropped.
                let due = futures_lite::future::or(
                    async {
                        Timer::after(interval).await;
                        true
                    },
                    async {
                        while stopped.recv().await.is_ok() {}
                        false
                    },
                )
                .await;

                if !due {
                    break;
                }

                // The previous heartbeat was never acked: give up on this socket.
                if task_awaiting_ack.swap(true, Ordering::AcqRel) {
                    let _ = sender.close().await;
                    break;
                }

                if send_heartbeat_frame(&sender, &sequence).await.is_err() {
                    break;
                }
            }
        })
        .detach();

        Self { stop, awaiting_ack }
    }

    fn acknowledge(&self) {
        self.awaiting_ack.store(false, Ordering::Release);
    }
}

impl Drop for HeartbeatLoop {
    fn drop(&mut self) {
        self.stop.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection_state() -> (Option<Session>, Arc<AtomicU64>) {
        (None, Arc::new(AtomicU64::new(NO_SEQUENCE)))
    }

    #[test]
    fn heartbeats_omit_the_sequence_until_one_arrives() {
        let (_, sequence) = connection_state();
        assert_eq!(last_sequence(&sequence), None);

        sequence.store(42, Ordering::Release);
        assert_eq!(last_sequence(&sequence), Some(42));
    }

    #[test]
    fn malformed_payloads_are_dropped_not_fatal() {
        // A dispatch we cannot read must not take the connection down with it.
        let parsed: Option<Message> = parse("MESSAGE_CREATE", serde_json::json!({ "junk": true }));
        assert!(parsed.is_none());
    }

    #[test]
    fn well_formed_payloads_parse() {
        let message: Option<Message> = parse(
            "MESSAGE_CREATE",
            serde_json::json!({
                "id": "m1",
                "channel_id": "c1",
                "author": { "id": "u1", "username": "ada" },
                "content": "hi",
                "timestamp": "2024-01-01T00:00:00Z",
                "tts": false,
                "mention_everyone": false,
                "mentions": [],
                "type": 0
            }),
        );
        assert_eq!(message.expect("parsed").content, "hi");
    }

    #[test]
    fn interaction_types_round_trip_through_their_wire_numbers() {
        for (number, kind) in [
            (1u8, InteractionType::Ping),
            (2, InteractionType::ApplicationCommand),
            (3, InteractionType::MessageComponent),
            (4, InteractionType::ApplicationCommandAutocomplete),
            (5, InteractionType::ModalSubmit),
            // Discord may add types; they must not fail to parse.
            (99, InteractionType::Unknown(99)),
        ] {
            assert_eq!(InteractionType::from(number), kind);
            assert_eq!(u8::from(kind), number);
        }
    }
}
