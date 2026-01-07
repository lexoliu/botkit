mod intents;

pub use intents::GatewayIntents;

use serde::Deserialize;
use serde_json::Value;
use zenwave::websocket::WebSocketMessage;

use crate::types::Interaction;

const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";

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

    /// Connect to the Discord Gateway
    pub async fn connect(&self) -> Result<GatewayConnection, GatewayError> {
        let ws = zenwave::websocket::connect(GATEWAY_URL)
            .await
            .map_err(|e| GatewayError::Connection(e.to_string()))?;

        Ok(GatewayConnection {
            ws,
            token: self.token.clone(),
            intents: self.intents,
            session_id: None,
            sequence: None,
            heartbeat_interval: None,
        })
    }
}

/// Active gateway connection
pub struct GatewayConnection {
    ws: zenwave::websocket::WebSocket,
    token: String,
    intents: GatewayIntents,
    session_id: Option<String>,
    sequence: Option<u64>,
    heartbeat_interval: Option<u64>,
}

impl GatewayConnection {
    /// Receive the next gateway event
    pub async fn recv(&mut self) -> Result<GatewayEvent, GatewayError> {
        loop {
            let msg = self
                .ws
                .recv()
                .await
                .map_err(|e| GatewayError::Connection(e.to_string()))?;

            let Some(msg) = msg else {
                return Err(GatewayError::Closed);
            };

            let text = match msg {
                WebSocketMessage::Text(t) => t.to_string(),
                WebSocketMessage::Binary(b) => {
                    String::from_utf8(b.to_vec())
                        .map_err(|e| GatewayError::Protocol(e.to_string()))?
                }
                WebSocketMessage::Close => {
                    return Err(GatewayError::Closed);
                }
                _ => continue,
            };

            let payload: GatewayPayload =
                serde_json::from_str(&text).map_err(|e| GatewayError::Protocol(e.to_string()))?;

            // Update sequence number
            if let Some(s) = payload.s {
                self.sequence = Some(s);
            }

            match payload.op {
                // Dispatch
                0 => {
                    if let Some(event) = self.handle_dispatch(payload.t.as_deref(), payload.d)? {
                        return Ok(event);
                    }
                }
                // Heartbeat request
                1 => {
                    self.send_heartbeat().await?;
                }
                // Reconnect
                7 => {
                    return Ok(GatewayEvent::Reconnect);
                }
                // Invalid session
                9 => {
                    return Ok(GatewayEvent::InvalidSession);
                }
                // Hello
                10 => {
                    if let Some(d) = payload.d {
                        if let Some(interval) = d.get("heartbeat_interval").and_then(|v| v.as_u64())
                        {
                            self.heartbeat_interval = Some(interval);
                        }
                    }
                    self.identify().await?;
                }
                // Heartbeat ACK
                11 => {
                    // Heartbeat acknowledged
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
        let Some(name) = event_name else {
            return Ok(None);
        };
        let Some(data) = data else {
            return Ok(None);
        };

        match name {
            "READY" => {
                if let Some(session_id) = data.get("session_id").and_then(|v| v.as_str()) {
                    self.session_id = Some(session_id.to_string());
                }
                Ok(Some(GatewayEvent::Ready))
            }
            "INTERACTION_CREATE" => {
                let interaction: Interaction = serde_json::from_value(data)
                    .map_err(|e| GatewayError::Protocol(e.to_string()))?;
                Ok(Some(GatewayEvent::InteractionCreate(interaction)))
            }
            "MESSAGE_CREATE" => {
                // Could parse message here
                Ok(None)
            }
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

        self.ws
            .send_text(serde_json::to_string(&identify).unwrap())
            .await
            .map_err(|e| GatewayError::Connection(e.to_string()))?;

        Ok(())
    }

    /// Send a heartbeat
    pub async fn send_heartbeat(&mut self) -> Result<(), GatewayError> {
        let heartbeat = serde_json::json!({
            "op": 1,
            "d": self.sequence
        });

        self.ws
            .send_text(serde_json::to_string(&heartbeat).unwrap())
            .await
            .map_err(|e| GatewayError::Connection(e.to_string()))?;

        Ok(())
    }

    /// Get the heartbeat interval in milliseconds
    pub fn heartbeat_interval(&self) -> Option<u64> {
        self.heartbeat_interval
    }

    /// Close the connection (consumes self)
    pub async fn close(self) -> Result<(), GatewayError> {
        self.ws
            .close()
            .await
            .map_err(|e| GatewayError::Connection(e.to_string()))
    }
}

/// Gateway payload structure
#[derive(Debug, Deserialize)]
struct GatewayPayload {
    op: u8,
    d: Option<Value>,
    s: Option<u64>,
    t: Option<String>,
}

/// Gateway events
#[derive(Debug)]
pub enum GatewayEvent {
    Ready,
    InteractionCreate(Interaction),
    Reconnect,
    InvalidSession,
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
