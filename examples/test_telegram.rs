use botkit_core::User;
use botkit_telegram::{TelegramBot, TelegramWebhook};

async fn ping() -> &'static str {
    "Pong!"
}

async fn greet(user: User) -> String {
    format!("Hello, {}!", user.name)
}

async fn start() -> &'static str {
    "Welcome to the bot! Try /ping or /greet"
}

#[skyzen::main]
fn main() -> TelegramWebhook {
    // Support both BOT_TOKEN and TG_BOT_CODE env vars
    let token = std::env::var("BOT_TOKEN")
        .or_else(|_| std::env::var("TG_BOT_CODE"))
        .expect("BOT_TOKEN or TG_BOT_CODE env var required");

    println!("Testing Telegram bot with token...");

    // Build the webhook handler - it implements Endpoint so can be returned directly
    let webhook = TelegramBot::new(&token)
        .command("ping", ping)
        .command("start", start)
        .command("greet", greet)
        .build();

    println!("Bot configured!");
    println!("Commands: /ping, /start, /greet");
    println!("\nStarting server on http://127.0.0.1:3000");
    println!("Telegram webhook endpoint: POST /");
    println!("\nTo test locally, use ngrok or similar to expose the endpoint.");

    webhook
}
