# botkit

`botkit` is a Rust library for building chat bots with one handler model across multiple platforms.

The `botkit` crate is the facade entrypoint for the workspace. It re-exports `botkit-core` by
default and exposes platform adapters behind explicit feature flags.

## Installation

Use the facade crate when you want one dependency with opt-in platforms:

```toml
[dependencies]
botkit = { version = "0.1.0", features = ["telegram"] }
```

Available facade features:

- `discord` enables `botkit::discord` and the `DiscordBot` re-exports.
- `telegram` enables `botkit::telegram` and the `TelegramBot` re-exports.
- `matrix` enables `botkit::matrix` and the `MatrixBot` re-exports.
- `full` enables all platform adapters.

You can also depend on the smaller crates directly:

- `botkit-core` provides the shared bot abstractions, extractors, handlers, and response types.
- `botkit-discord` provides the Discord integration.
- `botkit-telegram` provides the Telegram integration.
- `botkit-matrix` provides the Matrix integration.

## Quick start

```rust
# // Requires the `telegram` feature; the rest of the API is platform-agnostic.
# #[cfg(feature = "telegram")]
# mod demo {
use botkit::prelude::*;

async fn ping() -> &'static str {
    "Pong!"
}

async fn greet(user: User) -> String {
    format!("Hello, {}!", user.name)
}

# pub async fn run(token: String) -> Result<(), BotError> {
botkit::TelegramBot::new(token)
    .command_with_description("ping", "Check the bot is alive", ping)
    .command_with_description("greet", "Say hello", greet)
    .run()
    .await
# }
# }
```

Handlers are plain async functions. Their parameters are extractors
(`User`, `Channel`, `CommandArgs`, `ButtonId`, `MessageContent`, `Typing`,
`Context`), and their return type is anything that converts into a `Response`.
The same handler works on every platform.

To stop a bot on demand, use a shutdown signal instead of `run`:

```rust
# use botkit::prelude::*;
# fn spawn<F: std::future::Future>(_: F) {}
# fn demo(bot: impl Bot) {
let (signal, shutdown) = Shutdown::channel();
spawn(bot.run_until(shutdown));

// Later, from anywhere:
signal.shutdown();
# }
```

## Design goals

- Reuse the same handler style across platforms.
- Keep the core abstractions transport-agnostic.
- Support async-first bot execution.
- Keep platform adapters opt-in instead of forcing heavy dependencies by default.
- Expose platform crates separately instead of hiding everything behind type erasure.

## Crates

### `botkit`

Facade crate that re-exports the core API and feature-gated platform integrations.

### `botkit-core`

Shared building blocks:

- `Bot`, `BotBuilder`, and the `Shutdown` / `ShutdownSignal` pair
- extractor-based handlers such as `User`, `CommandArgs`, and `MessageContent`
- unified `Response`, embeds, components, and file responses

### `botkit-discord`

Discord support built around gateway events and command, button, and message
handlers. Registers slash commands on startup and reconnects (resuming the
session, so no events are lost) on its own.

### `botkit-telegram`

Telegram support for both long polling and webhook-style handling, plus bot
client operations.

### `botkit-matrix`

Matrix support built on top of `matrix-sdk`, including encrypted room support.
Reactions route through the same handler table as buttons.

## Feature support by platform

| | Discord | Telegram | Matrix |
| --- | --- | --- | --- |
| Commands | slash commands | `/command` | prefixed text (`!command`) |
| Command descriptions | registered on startup | `setMyCommands` | not applicable |
| Buttons | message components | inline keyboards | emoji reactions |
| Structured command options | yes | no (string args) | no (string args) |
| Messages | needs `MESSAGE_CONTENT` intent | yes | yes |
| Embeds | native | rendered as text | rendered as HTML |
| Files | attachments | documents | attachments |
| Ephemeral replies | yes | ignored | ignored |
| Typing indicator | yes | yes | yes |

## Status

This project is a library workspace under active development. The API surface is still evolving.
