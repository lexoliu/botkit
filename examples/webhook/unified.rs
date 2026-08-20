//! One set of handlers serving two platforms at once.
//!
//! Discord runs on its gateway in a background task while Telegram is served
//! over an HTTP webhook, both dispatching the same handler functions.
//!
//! Matrix works identically (see `botkit-examples --bin matrix`) but cannot
//! share a binary with skyzen: skyzen and matrix-sdk each depend on a package
//! that links the system `sqlite3` library, and only one may exist per build.
//!
//! ```sh
//! DISCORD_TOKEN=... DISCORD_APP_ID=... TELEGRAM_TOKEN=... \
//!   cargo run --bin unified
//! ```

use botkit_core::types::{ActionRow, Button, Component};
use botkit_core::{Bot, ButtonId, Response, User};
use botkit_discord::{DiscordBot, GatewayIntents};
use botkit_telegram::{TelegramBot, TelegramWebhook, Update};
use executor_core::spawn;
use skyzen::routing::{CreateRouteNode, Route, Router};
use skyzen::utils::Json;

// Handlers are plain async functions, shared verbatim across platforms.

async fn ping() -> &'static str {
    "Pong!"
}

async fn help() -> &'static str {
    "Commands:\n\
     ping - Check if the bot is alive\n\
     help - Show this message\n\
     greet - Get a personalized greeting\n\
     buttons - Show interactive buttons"
}

async fn greet(user: User) -> String {
    format!("Hello, {}!", user.name)
}

/// Discord renders these as message components; Telegram as an inline keyboard.
async fn buttons() -> Response {
    Response::text("Click a button:").with_components(vec![Component::ActionRow(
        ActionRow::buttons(vec![
            Button::primary("btn_hello", "Say Hello"),
            Button::secondary("btn_info", "Get Info"),
            Button::danger("btn_cancel", "Cancel"),
        ]),
    )])
}

/// A `*` suffix matches by prefix, so one handler covers every `btn_` id.
async fn on_button(id: ButtonId) -> Response {
    let text = match id.0.as_str() {
        "btn_hello" => "Hello!",
        "btn_info" => "A unified bot framework for Discord, Telegram, and Matrix.",
        "btn_cancel" => "Operation cancelled.",
        _ => "Unknown button.",
    };
    // Ephemeral on Discord; ignored by platforms without the concept.
    Response::text(text).ephemeral()
}

fn discord_bot(token: &str, app_id: &str) -> DiscordBot {
    DiscordBot::new(
        token,
        app_id,
        GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES,
    )
    .command_with_description("ping", "Check if the bot is alive", ping)
    .command_with_description("help", "Show available commands", help)
    .command_with_description("greet", "Get a personalized greeting", greet)
    .command_with_description("buttons", "Show interactive buttons", buttons)
    .button("btn_*", on_button)
}

fn telegram_webhook(token: &str) -> TelegramWebhook {
    TelegramBot::new(token)
        .command_with_description("ping", "Check if the bot is alive", ping)
        .command_with_description("help", "Show available commands", help)
        .command_with_description("greet", "Get a personalized greeting", greet)
        .command_with_description("buttons", "Show interactive buttons", buttons)
        .button("btn_*", on_button)
        .build()
}

fn router(telegram: TelegramWebhook) -> Router {
    Route::new((
        "/health".at(|| async { "OK" }),
        "/telegram/webhook".post(move |Json(update): Json<Update>| {
            let telegram = telegram.clone();
            async move {
                // Routing returns immediately; the handler runs in the
                // background so Telegram is not kept waiting.
                if let Err(e) = telegram.handle(update).await {
                    eprintln!("Telegram error: {e}");
                }
                "OK"
            }
        }),
    ))
    .build()
}

#[skyzen::main]
fn main() -> Router {
    let telegram_token = std::env::var("TELEGRAM_TOKEN").unwrap_or_default();
    let discord_token = std::env::var("DISCORD_TOKEN").unwrap_or_default();
    let discord_app_id = std::env::var("DISCORD_APP_ID").unwrap_or_default();

    if !discord_token.is_empty() && !discord_app_id.is_empty() {
        let discord = discord_bot(&discord_token, &discord_app_id);
        spawn(async move {
            if let Err(e) = discord.run().await {
                eprintln!("Discord bot stopped: {e}");
            }
        })
        .detach();
        println!("- Discord: gateway connection");
    } else {
        println!("- Discord: skipped (set DISCORD_TOKEN and DISCORD_APP_ID)");
    }

    println!("- Telegram webhook: POST /telegram/webhook");
    println!("- Health check: GET /health");

    router(telegram_webhook(&telegram_token))
}
