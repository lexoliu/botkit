use botkit_core::types::component::{ActionRow, Button, Component};
use botkit_core::{Bot, Response, User};
use botkit_discord::{DiscordBot, GatewayIntents};
use botkit_matrix::{MatrixBot, MatrixConfig};
use botkit_telegram::{TelegramBot, TelegramWebhook, Update};
use executor_core::spawn;
use skyzen::routing::{CreateRouteNode, Route, Router};
use skyzen::utils::Json;

// Simple handlers - no Context needed!
async fn ping() -> &'static str {
    "Pong!"
}

async fn help() -> &'static str {
    "Available commands:\n\
     /ping - Check if bot is alive\n\
     /help - Show this message\n\
     /greet - Get a personalized greeting\n\
     /buttons - Show interactive buttons"
}

// Using extractors - get user info automatically
async fn greet(user: User) -> String {
    format!("Hello, {}!", user.name)
}

// Return Response for full control (components, embeds, etc.)
async fn buttons() -> Response {
    Response::text("Click a button:").with_components(vec![Component::ActionRow(
        ActionRow::buttons(vec![
            Button::primary("btn_hello", "Say Hello"),
            Button::secondary("btn_info", "Get Info"),
            Button::danger("btn_cancel", "Cancel"),
        ]),
    )])
}

// Button handlers
async fn button_hello() -> Response {
    Response::text("Hello!").ephemeral()
}

async fn button_info() -> Response {
    Response::text("This is a unified bot framework supporting Discord, Telegram, and Matrix!")
        .ephemeral()
}

async fn button_cancel() -> Response {
    Response::text("Operation cancelled.").ephemeral()
}

// Discord bot setup - clean API with extractors
fn setup_discord_bot(token: &str, app_id: &str) -> DiscordBot {
    DiscordBot::new(
        token,
        app_id,
        GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES,
    )
    .command("ping", ping)
    .command("help", help)
    .command("greet", greet)
    .command("buttons", buttons)
    .button("btn_hello", button_hello)
    .button("btn_info", button_info)
    .button("btn_cancel", button_cancel)
}

// Telegram bot setup - same handlers work seamlessly
fn setup_telegram_bot(token: &str) -> TelegramWebhook {
    TelegramBot::new(token)
        .command("ping", ping)
        .command("help", help)
        .command("start", help) // Telegram convention
        .command("greet", greet)
        .command("buttons", buttons)
        .button("btn_hello", button_hello)
        .button("btn_info", button_info)
        .button("btn_cancel", button_cancel)
        .build()
}

// Matrix bot setup - same handlers, uses ! prefix for commands
fn setup_matrix_bot(config: MatrixConfig) -> MatrixBot {
    MatrixBot::new(config)
        .command("ping", ping)
        .command("help", help)
        .command("greet", greet)
        .command("buttons", buttons)
        // Matrix uses reactions instead of buttons
        .reaction("👋", button_hello)
        .reaction("ℹ️", button_info)
        .reaction("❌", button_cancel)
}

// Create skyzen router with Telegram webhook endpoint
fn create_router(telegram: TelegramWebhook) -> Router {
    Route::new((
        // Health check endpoint
        "/health".at(|| async { "OK" }),
        // Telegram webhook endpoint
        "/telegram/webhook".post(move |Json(update): Json<Update>| {
            let tg = telegram.clone();
            async move {
                if let Err(e) = tg.handle(update).await {
                    eprintln!("Telegram error: {}", e);
                }
                "OK"
            }
        }),
    ))
    .build()
}

#[skyzen::main]
fn main() -> Router {
    // Read tokens from environment
    let discord_token = std::env::var("DISCORD_TOKEN").unwrap_or_default();
    let discord_app_id = std::env::var("DISCORD_APP_ID").unwrap_or_default();
    let telegram_token = std::env::var("TELEGRAM_TOKEN").unwrap_or_default();
    let matrix_homeserver = std::env::var("MATRIX_HOMESERVER").unwrap_or_default();
    let matrix_user = std::env::var("MATRIX_USER").unwrap_or_default();
    let matrix_password = std::env::var("MATRIX_PASSWORD").unwrap_or_default();

    // Setup Telegram webhook handler
    let telegram = setup_telegram_bot(&telegram_token);

    // Setup and spawn Discord bot (runs via WebSocket in background)
    if !discord_token.is_empty() && !discord_app_id.is_empty() {
        let discord = setup_discord_bot(&discord_token, &discord_app_id);

        spawn(async move {
            if let Err(e) = discord.run().await {
                eprintln!("Discord bot error: {}", e);
            }
        })
        .detach();
    }

    // Setup and spawn Matrix bot (runs via sync loop in background)
    if !matrix_homeserver.is_empty() && !matrix_user.is_empty() && !matrix_password.is_empty() {
        let config = MatrixConfig::new(&matrix_homeserver)
            .password_auth(&matrix_user, &matrix_password)
            .command_prefix("!")
            .device_name("BotKit Unified Example")
            .auto_join_rooms(true);

        let matrix = setup_matrix_bot(config);

        spawn(async move {
            if let Err(e) = matrix.run().await {
                eprintln!("Matrix bot error: {}", e);
            }
        })
        .detach();
    }

    println!("Starting unified bot server...");
    println!("- Telegram webhook: POST /telegram/webhook");
    println!("- Discord: WebSocket connection (if configured)");
    println!("- Matrix: Sync loop (if configured)");
    println!("- Health check: GET /health");

    // Return skyzen router for HTTP server
    create_router(telegram)
}
