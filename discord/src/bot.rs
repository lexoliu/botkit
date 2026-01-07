use std::sync::Arc;

use botkit_core::{Bot, BotBuilder, BotError, BotHandle, Context, IntoHandler, Response};
use executor_core::spawn;

use crate::client::DiscordClient;
use crate::event::DiscordContextData;
use crate::gateway::{Gateway, GatewayEvent, GatewayIntents};
use crate::types::{Interaction, InteractionData, InteractionType};

/// Discord bot builder
///
/// Create a bot with command and button handlers, then call `run()` to start.
///
/// # Example
/// ```ignore
/// // Simple handler - no Context needed!
/// async fn ping() -> &'static str {
///     "Pong!"
/// }
///
/// // With extractors
/// async fn greet(user: User) -> String {
///     format!("Hello, {}!", user.name)
/// }
///
/// let bot = DiscordBot::new(token, app_id, GatewayIntents::GUILDS)
///     .command("ping", ping)
///     .command("greet", greet);
///
/// bot.run().await?;
/// ```
pub struct DiscordBot {
    token: String,
    application_id: String,
    intents: GatewayIntents,
    builder: BotBuilder,
}

impl DiscordBot {
    /// Create a new Discord bot
    pub fn new(
        token: impl Into<String>,
        application_id: impl Into<String>,
        intents: GatewayIntents,
    ) -> Self {
        Self {
            token: token.into(),
            application_id: application_id.into(),
            intents,
            builder: BotBuilder::new(),
        }
    }

    /// Register a command handler
    pub fn command<H, Args>(mut self, name: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.command(name, handler);
        self
    }

    /// Register a button handler with pattern matching
    ///
    /// Pattern can end with `*` for prefix matching (e.g., "confirm_*")
    pub fn button<H, Args>(mut self, pattern: impl Into<String>, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.button(pattern, handler);
        self
    }

    /// Register a message handler
    pub fn message<H, Args>(mut self, handler: H) -> Self
    where
        H: IntoHandler<Args>,
    {
        self.builder = self.builder.message(handler);
        self
    }

    async fn handle_interaction(
        self: &Arc<Self>,
        client: &DiscordClient,
        interaction: Interaction,
    ) -> Result<(), BotError> {
        // Handle ping
        if interaction.interaction_type == InteractionType::Ping {
            client
                .respond_interaction(&interaction.id, &interaction.token, 1, serde_json::json!({}))
                .await?;
            return Ok(());
        }

        // Determine event type and value for routing
        let (event_type, value) = match interaction.interaction_type {
            InteractionType::ApplicationCommand => {
                let name = match &interaction.data {
                    Some(InteractionData::ApplicationCommand { name, .. }) => name.clone(),
                    _ => return Ok(()),
                };
                ("command", name)
            }
            InteractionType::MessageComponent => {
                let custom_id = interaction
                    .data
                    .as_ref()
                    .and_then(|d| match d {
                        InteractionData::MessageComponent { custom_id, .. } => {
                            Some(custom_id.clone())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                ("button", custom_id)
            }
            _ => return Ok(()),
        };

        // Find matching handler
        let handler = self.builder.find_handler(event_type, &value);

        if let Some(handler) = handler {
            let data = DiscordContextData::new(interaction.clone());
            let ctx = Context::new(data);

            let response = handler.call(ctx).await;

            // Send response
            self.send_response(client, &interaction, response).await?;
        }

        Ok(())
    }

    async fn send_response(
        &self,
        client: &DiscordClient,
        interaction: &Interaction,
        response: Response,
    ) -> Result<(), BotError> {
        if response.is_empty() {
            return Ok(());
        }

        if response.is_acknowledge() {
            client
                .respond_interaction(
                    &interaction.id,
                    &interaction.token,
                    5, // DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE
                    serde_json::json!({}),
                )
                .await?;
            return Ok(());
        }

        // Build response data
        let mut data = serde_json::json!({
            "content": response.content().unwrap_or("")
        });

        let embeds = response.embeds();
        if !embeds.is_empty() {
            data["embeds"] = serde_json::to_value(embeds).unwrap_or_default();
        }

        let components = response.components();
        if !components.is_empty() {
            data["components"] = serde_json::to_value(components).unwrap_or_default();
        }

        if response.is_ephemeral() {
            data["flags"] = serde_json::json!(64);
        }

        client
            .respond_interaction(
                &interaction.id,
                &interaction.token,
                4, // CHANNEL_MESSAGE_WITH_SOURCE
                data,
            )
            .await
    }
}

impl Bot for DiscordBot {
    async fn run(self) -> Result<BotHandle, BotError> {
        let (handle, shutdown_rx) = BotHandle::channel();

        let client = DiscordClient::new(&self.token, &self.application_id);
        let gateway = Gateway::new(&self.token, self.intents);

        let bot = Arc::new(self);

        // Spawn the event loop
        futures_lite::future::race(
            async {
                let mut conn = gateway
                    .connect()
                    .await
                    .map_err(|e| BotError::Connection(e.to_string()))?;

                loop {
                    match conn.recv().await {
                        Ok(GatewayEvent::Ready) => {
                            // Bot is ready
                        }
                        Ok(GatewayEvent::InteractionCreate(interaction)) => {
                            let bot = Arc::clone(&bot);
                            let client = client.clone();
                            // Spawn interaction handler as a separate task
                            spawn(async move {
                                if let Err(e) = bot.handle_interaction(&client, interaction).await {
                                    eprintln!("Interaction error: {}", e);
                                }
                            })
                            .detach();
                        }
                        Ok(GatewayEvent::Reconnect) => {
                            // Reconnect logic would go here
                            break;
                        }
                        Ok(GatewayEvent::InvalidSession) => {
                            return Err(BotError::Auth("Invalid session".to_string()));
                        }
                        Err(e) => {
                            return Err(BotError::Connection(e.to_string()));
                        }
                    }
                }

                Ok::<_, BotError>(())
            },
            async {
                let _ = shutdown_rx.recv().await;
                Err(BotError::Shutdown)
            },
        )
        .await?;

        Ok(handle)
    }
}
