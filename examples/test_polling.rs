//! Test botkit with real Telegram messages using long polling
//!
//! No server needed - we poll Telegram's API directly.

use botkit_core::User;
use botkit_telegram::TelegramBot;
use futures_lite::future::block_on;

async fn ping() -> &'static str {
    "Pong!"
}

async fn greet(user: User) -> String {
    format!("Hello, {}!", user.name)
}

async fn start() -> &'static str {
    "Welcome to the bot! Try /ping or /greet"
}

fn main() {
    let token = std::env::var("BOT_TOKEN")
        .or_else(|_| std::env::var("TG_BOT_CODE"))
        .expect("BOT_TOKEN or TG_BOT_CODE env var required");

    println!("Starting polling bot...");
    println!("Commands: /ping, /start, /greet");
    println!("Press Ctrl+C to stop.\n");

    // That's it! Just register handlers and run
    block_on(
        TelegramBot::new(token)
            .command("ping", ping)
            .command("start", start)
            .command("greet", greet)
            .run_polling(),
    )
    .unwrap();
}
