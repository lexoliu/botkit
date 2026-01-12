//! Matrix bot example
//!
//! Run with:
//! ```bash
//! MATRIX_HOMESERVER=https://matrix.org \
//! MATRIX_USER=@bot:matrix.org \
//! MATRIX_PASSWORD=yourpassword \
//! cargo run -p botkit-examples --bin test_matrix
//! ```

use botkit_core::{CommandArgs, User};
use botkit_matrix::{MatrixBot, MatrixConfig};
use futures_lite::future::block_on;

async fn ping() -> &'static str {
    "Pong!"
}

async fn greet(user: User) -> String {
    format!("Hello, {}!", user.name)
}

async fn echo(args: CommandArgs) -> String {
    if args.0.is_empty() {
        "Usage: !echo <message>".to_string()
    } else {
        format!("You said: {}", args.0)
    }
}

async fn on_thumbsup(user: User) -> String {
    format!("{} gave a thumbs up!", user.name)
}

fn main() {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    // Read configuration from environment
    let homeserver =
        std::env::var("MATRIX_HOMESERVER").expect("MATRIX_HOMESERVER environment variable not set");
    let user_id = std::env::var("MATRIX_USER").expect("MATRIX_USER environment variable not set");
    let password =
        std::env::var("MATRIX_PASSWORD").expect("MATRIX_PASSWORD environment variable not set");

    let config = MatrixConfig::new(homeserver)
        .password_auth(&user_id, &password)
        .command_prefix("!")
        .device_name("BotKit Matrix Example")
        .auto_join_rooms(true);

    // Run the bot
    block_on(async {
        MatrixBot::new(config)
            .command("ping", ping)
            .command("greet", greet)
            .command("echo", echo)
            .reaction("👍", on_thumbsup)
            .run()
            .await
            .expect("Bot failed");
    });
}
