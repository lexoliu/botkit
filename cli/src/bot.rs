//! The `CliBot` adapter: routes injected events through `BotBuilder` and
//! turns handler `Response`s into outbound wire actions.

use async_channel::Receiver;
use botkit_core::FileSource;
use botkit_core::types::component::Component;
use botkit_core::{Bot, BotBuilder, BotError, Context, Event, IntoHandler, Response, Shutdown};

use crate::context::CliContextData;
use crate::hub::CliHub;
use crate::transport::{self, Transport};
use crate::wire::{Inbound, Outbound, OutboundFile, OutboundMessage, WireButton};

/// A bot whose "platform" is the CLI wire protocol.
///
/// Build it like any other adapter, take the [`CliHub`] handle if you need
/// to drive it out-of-band (acpbot-style platform senders, tests), then
/// `run()`.
pub struct CliBot {
    builder: BotBuilder,
    transport: Transport,
    hub: CliHub,
    inbound: Receiver<Inbound>,
}

impl CliBot {
    /// A bot on the given transport.
    pub fn new(transport: Transport) -> Self {
        let (hub, inbound) = CliHub::new();
        Self {
            builder: BotBuilder::new(),
            transport,
            hub,
            inbound,
        }
    }

    /// The shared hub handle: inject events, subscribe to outbound lines,
    /// or build platform senders on it.
    pub fn hub(&self) -> CliHub {
        self.hub.clone()
    }

    /// Register a command handler.
    pub fn command<H, Args>(mut self, name: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.command(name, handler);
        self
    }

    /// Register a command handler with a menu description.
    pub fn command_with_description<H, Args>(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        handler: H,
    ) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self
            .builder
            .command_with_description(name, description, handler);
        self
    }

    /// Register a button handler (`*` suffix matches by prefix).
    pub fn button<H, Args>(mut self, pattern: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.button(pattern, handler);
        self
    }

    /// Register the catch-all message handler.
    pub fn message<H, Args>(mut self, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.message(handler);
        self
    }

    /// Register the handler for events no route claims.
    pub fn fallback<H, Args>(mut self, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.fallback(handler);
        self
    }

    /// Dispatch one inbound event: route it, run the handler, translate the
    /// response into outbound actions.
    async fn dispatch(&self, inbound: Inbound) {
        let event = match &inbound {
            Inbound::Command(command) => Event::Command(command.name.as_str()),
            Inbound::Button(button) => Event::Button(button.data.as_str()),
            _ => Event::Message,
        };
        let Some(handler) = self.builder.route(event).cloned() else {
            return;
        };
        let chat = inbound.chat().to_string();
        let thread_id = inbound.thread_id();
        let data = CliContextData::new(inbound, self.hub.clone());
        let response = handler.call(Context::new(data)).await;
        self.send_response(&chat, thread_id, response).await;
    }

    /// Serialize a handler `Response` into outbound wire actions addressed
    /// to the event's chat and topic.
    async fn send_response(&self, chat: &str, thread_id: Option<i64>, mut response: Response) {
        if let Some(text) = response.content() {
            self.hub.emit(Outbound::Message(OutboundMessage {
                chat: chat.to_string(),
                message_id: self.hub.next_message_id(),
                text: text.to_string(),
                buttons: components_to_buttons(response.components()),
                reply_to: None,
                thread_id,
                extras: embeds_extra(&response),
            }));
            return;
        }
        if let Some(file) = response.take_file() {
            let (path, data) = match file.file {
                FileSource::Path(path) => (Some(path.display().to_string()), None),
                other => {
                    let bytes = other.read().await.unwrap_or_default();
                    use base64::Engine;
                    (
                        None,
                        Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
                    )
                }
            };
            let kind = file
                .filename
                .as_deref()
                .or(path.as_deref())
                .map(media_kind)
                .unwrap_or("document")
                .to_string();
            self.hub.emit(Outbound::File(OutboundFile {
                chat: chat.to_string(),
                message_id: self.hub.next_message_id(),
                kind,
                path,
                data,
                filename: file.filename,
                caption: file.caption,
                thread_id,
            }));
        }
        // Empty and Acknowledge responses send nothing.
    }
}

impl Bot for CliBot {
    async fn run_until(self, shutdown: Shutdown) -> Result<(), BotError> {
        transport::start(&self.transport, &self.hub);

        enum Step {
            Event(Box<Inbound>),
            Stop,
        }

        loop {
            let step = futures_lite::future::or(
                async {
                    match self.inbound.recv().await {
                        Ok(event) => Step::Event(Box::new(event)),
                        Err(_) => Step::Stop,
                    }
                },
                async {
                    shutdown.wait().await;
                    Step::Stop
                },
            )
            .await;

            match step {
                Step::Event(event) => self.dispatch(*event).await,
                Step::Stop => return Ok(()),
            }
        }
    }
}

/// Flatten botkit components into CLI keyboard rows; non-button components
/// are skipped (they surface through `extras` instead).
fn components_to_buttons(components: &[Component]) -> Vec<Vec<WireButton>> {
    components
        .iter()
        .filter_map(|component| match component {
            Component::ActionRow(row) => Some(
                row.components
                    .iter()
                    .filter_map(|component| match component {
                        Component::Button(button) => Some(WireButton {
                            text: button.label.clone(),
                            data: button.custom_id.clone(),
                            url: button.url.clone(),
                        }),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .filter(|row: &Vec<WireButton>| !row.is_empty())
        .collect()
}

/// Embeds and select menus have no CLI-native shape; carry them as raw JSON.
fn embeds_extra(response: &Response) -> Option<serde_json::Value> {
    let embeds = response.embeds();
    if embeds.is_empty() {
        return None;
    }
    serde_json::to_value(embeds).ok()
}

/// MIME-derived media kind for a filename or path.
fn media_kind(name: &str) -> &'static str {
    let mime = mime_guess::from_path(name).first_or_octet_stream();
    match (mime.type_().as_str(), mime.subtype().as_str()) {
        ("image", "gif") => "animation",
        ("image", _) => "photo",
        ("video", _) => "video",
        ("audio", "ogg") => "voice",
        ("audio", _) => "audio",
        _ => "document",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{InboundButton, InboundCommand, InboundMessage, InboundReaction, WireUser};

    /// Install and drive a global executor once for the whole test binary:
    /// `Bot::run_until` and `ChatActionGuard` spawn through executor-core.
    fn ensure_executor() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let executor: &'static async_executor::Executor<'static> =
                Box::leak(Box::new(async_executor::Executor::new()));
            executor_core::init_global_executor(executor);
            std::thread::spawn(move || {
                futures_lite::future::block_on(executor.run(futures_lite::future::pending::<()>()));
            });
        });
    }

    fn user() -> WireUser {
        WireUser {
            id: "u1".to_string(),
            name: "alice".to_string(),
        }
    }

    fn message(text: &str) -> Inbound {
        Inbound::Message(InboundMessage {
            chat: "c1".to_string(),
            user: user(),
            message_id: None,
            text: Some(text.to_string()),
            caption: None,
            thread_id: None,
            reply_to: None,
            files: vec![],
            sticker: None,
            ambient: false,
        })
    }

    fn outbound_lines(rx: &Receiver<String>) -> Vec<Outbound> {
        let mut lines = Vec::new();
        while let Ok(line) = rx.try_recv() {
            lines.push(serde_json::from_str(&line).unwrap());
        }
        lines
    }

    /// An injected message reaches the message handler and its `Response`
    /// becomes an outbound `message` line addressed to the event's chat.
    #[test]
    fn message_round_trip() {
        ensure_executor();
        futures_lite::future::block_on(async {
            let bot = CliBot::new(Transport::Manual).message(|ctx: Context| async move {
                format!("echo: {}", ctx.message_content().unwrap_or("?"))
            });
            let hub = bot.hub();
            let sink = hub.subscribe();
            let (signal, shutdown) = botkit_core::Shutdown::channel();
            let task = executor_core::spawn(bot.run_until(shutdown));

            hub.inject(message("hi"));
            let ack_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            let lines = loop {
                let lines = outbound_lines(&sink);
                if lines.iter().any(|o| matches!(o, Outbound::Message(_))) {
                    break lines;
                }
                assert!(
                    std::time::Instant::now() < ack_deadline,
                    "no outbound message"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            let outbound = lines
                .into_iter()
                .find_map(|o| match o {
                    Outbound::Message(m) => Some(m),
                    _ => None,
                })
                .unwrap();
            assert_eq!(outbound.chat, "c1");
            assert_eq!(outbound.text, "echo: hi");

            signal.shutdown();
            task.await.unwrap();
        });
    }

    /// Commands route to their handler; button presses route to the button
    /// handler; reactions land on the catch-all.
    #[test]
    fn command_button_reaction_routing() {
        ensure_executor();
        futures_lite::future::block_on(async {
            let bot = CliBot::new(Transport::Manual)
                .command("ping", || async { "pong" })
                .button("yes", || async { "pressed" })
                .message(|| async { "got it" });
            let hub = bot.hub();
            let sink = hub.subscribe();
            let (signal, shutdown) = botkit_core::Shutdown::channel();
            let task = executor_core::spawn(bot.run_until(shutdown));

            hub.inject(Inbound::Command(InboundCommand {
                chat: "c1".to_string(),
                user: user(),
                name: "ping".to_string(),
                args: String::new(),
                message_id: None,
                thread_id: None,
            }));
            hub.inject(Inbound::Button(InboundButton {
                chat: "c1".to_string(),
                user: user(),
                data: "yes".to_string(),
                message_id: Some(3),
                message_text: None,
                thread_id: None,
            }));
            hub.inject(Inbound::Reaction(InboundReaction {
                chat: "c1".to_string(),
                user: user(),
                message_id: 4,
                added: vec!["👍".to_string()],
                removed: vec![],
            }));

            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            let texts = loop {
                let texts: Vec<String> = outbound_lines(&sink)
                    .into_iter()
                    .filter_map(|o| match o {
                        Outbound::Message(m) => Some(m.text),
                        _ => None,
                    })
                    .collect();
                if texts.len() >= 3 {
                    break texts;
                }
                assert!(std::time::Instant::now() < deadline, "only got {texts:?}");
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            assert_eq!(texts, ["pong", "pressed", "got it"]);

            signal.shutdown();
            task.await.unwrap();
        });
    }

    /// `ctx.typing()` produces an outbound `action` line on this platform.
    #[test]
    fn typing_action_surfaces() {
        ensure_executor();
        futures_lite::future::block_on(async {
            let bot = CliBot::new(Transport::Manual).message(|ctx: Context| async move {
                let _typing = ctx.typing();
                "done"
            });
            let hub = bot.hub();
            let sink = hub.subscribe();
            let (signal, shutdown) = botkit_core::Shutdown::channel();
            let task = executor_core::spawn(bot.run_until(shutdown));

            hub.inject(message("x"));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            let lines = loop {
                let lines = outbound_lines(&sink);
                if lines.iter().any(|o| matches!(o, Outbound::Message(_))) {
                    break lines;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            assert!(lines.iter().any(|o| matches!(
                o,
                Outbound::Action(a) if a.action == "typing" && a.chat == "c1"
            )));

            signal.shutdown();
            task.await.unwrap();
        });
    }
}
