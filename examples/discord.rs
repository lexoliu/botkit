//! Discord bot over the gateway.
//!
//! Slash commands are registered on startup, so they appear in Discord's UI a
//! few moments after the bot connects.
//!
//! ```sh
//! DISCORD_TOKEN=... DISCORD_APP_ID=... \
//!   cargo run -p botkit-examples --bin discord
//! ```

use botkit_core::types::{ActionRow, Button, Component};
use botkit_core::{Bot, Response, Shutdown, User};
use botkit_discord::{DiscordBot, GatewayIntents};

async fn ping() -> &'static str {
    "Pong!"
}

async fn greet(user: User) -> String {
    format!("Hello, {}!", user.name)
}

/// Returning a `Response` gives access to embeds, components, and flags.
async fn buttons() -> Response {
    Response::text("Pick one:").with_components(vec![Component::ActionRow(ActionRow::buttons(
        vec![
            Button::primary("confirm_yes", "Yes"),
            Button::danger("confirm_no", "No"),
            Button::link("https://github.com/lexoliu/botkit", "Docs"),
        ],
    ))])
}

/// A `*` suffix matches by prefix, so one handler serves every `confirm_` button.
async fn on_confirm(id: botkit_core::ButtonId) -> Response {
    let choice = id.0.strip_prefix("confirm_").unwrap_or(&id.0);
    Response::text(format!("You chose: {choice}")).ephemeral()
}

fn main() {
    let token = std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN env var required");
    let app_id = std::env::var("DISCORD_APP_ID").expect("DISCORD_APP_ID env var required");

    tracing_subscriber::fmt::init();

    // Interactions are handled on spawned tasks, so the bot needs an executor.
    let executor: &'static async_executor::Executor<'static> =
        Box::leak(Box::new(async_executor::Executor::new()));
    executor_core::init_global_executor(executor);

    let (signal, shutdown) = Shutdown::channel();
    ctrlc::set_handler(move || signal.shutdown()).expect("install Ctrl-C handler");

    let bot = DiscordBot::new(
        token,
        app_id,
        // MESSAGE_CONTENT is a privileged intent; enable it in the Discord
        // developer portal, or drop it along with the `message` handler.
        GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES,
    )
    .command_with_description("ping", "Check that the bot is alive", ping)
    .command_with_description("greet", "Get a personalized greeting", greet)
    .command_with_description("buttons", "Show interactive buttons", buttons)
    .button("confirm_*", on_confirm);

    println!("Connecting to Discord. Press Ctrl+C to stop.");

    futures_lite::future::block_on(executor.run(bot.run_until(shutdown))).expect("bot failed");

    println!("Stopped.");
}
