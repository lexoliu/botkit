//! Telegram bot over long polling — no public HTTP endpoint needed.
//!
//! ```sh
//! BOT_TOKEN=... cargo run -p botkit-examples --bin telegram_polling
//! ```

use botkit_core::{Bot, CommandArgs, Shutdown, User};
use botkit_telegram::TelegramBot;

async fn ping() -> &'static str {
    "Pong!"
}

async fn start() -> &'static str {
    "Welcome! Try /ping, /greet, or /echo <text>."
}

async fn greet(user: User) -> String {
    format!("Hello, {}!", user.name)
}

async fn echo(args: CommandArgs) -> String {
    if args.0.is_empty() {
        "Give me something to echo: /echo hello".to_string()
    } else {
        args.0
    }
}

fn main() {
    let token = std::env::var("BOT_TOKEN").expect("BOT_TOKEN env var required");

    // Handlers can spawn background work (the typing indicator, for one), so
    // the bot needs an executor to spawn onto.
    let executor: &'static async_executor::Executor<'static> =
        Box::leak(Box::new(async_executor::Executor::new()));
    executor_core::init_global_executor(executor);

    // Stop cleanly on Ctrl-C instead of leaving the poll mid-flight.
    let (signal, shutdown) = Shutdown::channel();
    ctrlc::set_handler(move || signal.shutdown()).expect("install Ctrl-C handler");

    println!("Polling for updates. Press Ctrl+C to stop.");

    let bot = TelegramBot::new(token)
        .command_with_description("start", "Show available commands", start)
        .command_with_description("ping", "Check that the bot is alive", ping)
        .command_with_description("greet", "Get a personalized greeting", greet)
        .command_with_description("echo", "Repeat what you type", echo);

    futures_lite::future::block_on(executor.run(bot.run_until(shutdown))).expect("bot failed");

    println!("Stopped.");
}
