//! Example demonstrating typing indicators and file uploads
//!
//! Shows how to:
//! - Use `Typing` extractor for automatic typing indicator during slow operations
//! - Return files directly from handlers using `async_fs::File`
//! - Return files with captions using tuple syntax

use async_fs::File;
use async_io::Timer;
use botkit_core::{Response, Typing, User};
use botkit_telegram::TelegramBot;
use std::time::Duration;

/// Slow command that shows typing indicator while "thinking"
///
/// The `Typing` extractor automatically starts the typing indicator
/// and keeps it active (with auto-renewal) until the handler returns.
async fn think(_typing: Typing) -> &'static str {
    // Simulate slow operation - typing indicator stays active
    Timer::after(Duration::from_secs(3)).await;
    "Done thinking!"
}

/// Send a file to the user
///
/// Simply return `async_fs::File` - the framework handles the upload
async fn get_file() -> File {
    File::open("Cargo.toml").await.expect("Failed to open file")
}

/// Send a file with a caption using tuple syntax
async fn get_file_with_caption() -> (File, &'static str) {
    let file = File::open("Cargo.toml").await.expect("Failed to open file");
    (file, "Here's the Cargo.toml file!")
}

/// Send a file with custom filename using Response builder
async fn download(user: User) -> Response {
    let file = File::open("Cargo.toml").await.expect("Failed to open file");
    Response::file(file)
        .with_filename("config.toml")
        .with_caption(format!("Here you go, {}!", user.name))
}

async fn start() -> &'static str {
    "Commands:\n\
     /think - Shows typing indicator for 3 seconds\n\
     /file - Sends Cargo.toml\n\
     /filewithcaption - Sends file with caption\n\
     /download - Sends file with custom name"
}

fn main() {
    let token = std::env::var("BOT_TOKEN")
        .or_else(|_| std::env::var("TG_BOT_CODE"))
        .expect("BOT_TOKEN or TG_BOT_CODE env var required");

    eprintln!("Starting typing & files demo bot...");
    eprintln!("Commands: /start, /think, /file, /filewithcaption, /download");
    eprintln!("Press Ctrl+C to stop.\n");

    // Create a static executor for spawning background tasks (e.g., typing indicator)
    let ex: &'static async_executor::Executor<'static> =
        Box::leak(Box::new(async_executor::Executor::new()));
    executor_core::init_global_executor(ex);

    // Run the bot with the executor driving spawned tasks
    futures_lite::future::block_on(ex.run(async {
        TelegramBot::new(token)
            .command_with_description("start", "Show available commands", start)
            .command_with_description("think", "Shows typing indicator for 3 seconds", think)
            .command_with_description("file", "Sends Cargo.toml as a document", get_file)
            .command_with_description("filewithcaption", "Sends file with caption", get_file_with_caption)
            .command_with_description("download", "Sends file with custom name", download)
            .run_polling()
            .await
    }))
    .unwrap();
}
