use botkit_telegram::TelegramBot;
use futures_lite::future::block_on;
use std::env;

async fn start() -> &'static str {
    println!("Received /start command");
    "Hello from debug bot!"
}

async fn echo() -> &'static str {
    println!("Received message");
    "I received your message"
}

fn main() {
    let token = env::var("BOT_TOKEN")
        .or_else(|_| env::var("TG_BOT_CODE"))
        .expect("BOT_TOKEN or TG_BOT_CODE env var required");

    println!("Starting debug polling bot...");
    println!("Token: {}...", &token[..10]);

    block_on(
        TelegramBot::new(token)
            .command("start", start)
            .message(echo)
            .run_polling(),
    )
    .unwrap();
}
