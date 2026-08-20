# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Build all crates
cargo build

# Build specific crate
cargo build -p botkit-core
cargo build -p botkit-discord
cargo build -p botkit-telegram
cargo build -p botkit-matrix

# Run examples
cargo run -p botkit-examples --bin unified
cargo run -p botkit-examples --bin telegram_webhook
cargo run -p botkit-examples --bin telegram_polling
cargo run -p botkit-examples --bin telegram_typing_and_files
cargo run -p botkit-examples --bin matrix

# Check without building
cargo check

# Run tests
cargo test --workspace --all-features

# Verify the WASM target still builds (CI does this)
cargo build -p botkit-matrix --target wasm32-unknown-unknown

# Format code
cargo fmt

# Run clippy
cargo clippy
```

## Architecture

This is a unified bot framework supporting Discord, Telegram, and Matrix platforms through a common abstraction layer.

### Workspace Structure

- **core/** (`botkit-core`): Platform-agnostic abstractions - traits, extractors, responders, and shared types
- **discord/** (`botkit-discord`): Discord implementation using WebSocket Gateway
- **telegram/** (`botkit-telegram`): Telegram implementation supporting both webhooks (via skyzen HTTP) and long polling
- **matrix/** (`botkit-matrix`): Matrix implementation using matrix-sdk with E2EE support
- **examples/**: Runnable examples demonstrating unified and platform-specific usage

### Key Abstractions (in `botkit-core`)

The framework uses an **extractor/responder pattern** similar to Axum:

1. **Handler System** (`handler.rs`): Functions become handlers via `IntoHandler` trait. Supports 0-4 arguments with automatic extraction.

2. **Extractors** (`extractor.rs`): Types implementing `FromContext` are automatically extracted from event context:
   - `User` - user ID and name
   - `Channel` - channel ID
   - `CommandName`, `CommandArgs` - command data
   - `ButtonId` - callback button identifier
   - `MessageContent` - message text
   - `Context` - full context access

3. **Responders** (`responder.rs`): Return types implementing `IntoResponse` are converted to platform responses. Supports `&str`, `String`, `Cow<str>`, `Response`, `()`, `Option<T>`, `Result<T, E>`, `async_fs::File`, `PathBuf`, and `(file, caption)` tuples.

4. **Context** (`context.rs`): Unified interface wrapping platform-specific `ContextData` implementations. Provides consistent access to user, channel, command, and message data across platforms.

5. **Response** (`response.rs`): Rich response type supporting text, embeds, components (buttons/action rows), file attachments (`FileSource`: open handle, in-memory bytes, or a path read at send time), and flags (ephemeral).

6. **Routing** (`bot.rs`): Adapters translate a platform payload into an `Event` (`Command`, `Button`, or `Message`) and call `BotBuilder::route`. Commands and exact button ids resolve through a hash map; only wildcard button patterns (`"confirm_*"`) fall back to a scan. First registration wins on a duplicate.

7. **Lifecycle** (`bot.rs`, `shutdown.rs`): Every adapter implements `Bot`. `run()` blocks until a fatal error; `run_until(shutdown)` also stops when a `ShutdownSignal` fires. `Shutdown::channel()` creates the pair.

### Handler Registration Pattern

```rust
// Handlers are plain async functions
async fn ping() -> &'static str { "Pong!" }
async fn greet(user: User) -> String { format!("Hello, {}!", user.name) }

// Register with builder pattern
DiscordBot::new(token, app_id, intents)
    .command("ping", ping)
    .command_with_description("greet", "Say hello", greet)
    .button("btn_id", button_handler)
    .button("confirm_*", prefix_handler)  // `*` suffix matches by prefix
    .message(catch_all)
    .run()      // from the `Bot` trait; blocks until stopped
    .await?;
```

Graceful shutdown works the same on every platform:

```rust
let (signal, shutdown) = Shutdown::channel();
spawn(bot.run_until(shutdown));
signal.shutdown();   // returns immediately; the bot stops at its next await
```

### Platform Implementations

**Discord** (`discord/`):
- Connects via WebSocket to Discord Gateway
- Handles slash commands, button interactions, and messages (needs the `MESSAGE_CONTENT` intent)
- Registers the declared slash commands on startup; opt out with `skip_command_registration()`
- Reconnects and RESUMEs on its own, with exponential backoff and zombie-connection detection
- REST calls authenticate with Discord's `Bot <token>` scheme, not `Bearer`
- Unified `Component`s are rendered to Discord's numeric wire format in `types/component.rs` — core's serde representation is botkit's own and is not what the API accepts

**Telegram** (`telegram/`):
- **Webhook mode**: `bot.build()` returns `TelegramWebhook` for skyzen HTTP integration. It routes and returns immediately, running the handler in the background so Telegram is not kept waiting
- **Polling mode**: `bot.run()` long-polls, with exponential backoff on API errors
- Both modes share one `Dispatcher`, so they behave identically
- Handles `/commands` and inline keyboard callbacks
- `Update` is deserialized field by field: Telegram keeps adding update kinds, and rejecting one would make it redeliver forever
- Entity offsets are UTF-16 code units and come from untrusted input, so command extraction resolves them against the text's UTF-16 view with every index checked

**Matrix** (`matrix/`):
- Uses `matrix-sdk` crate for protocol handling (including E2EE)
- **Sync loop mode**: `bot.run()` connects and syncs with homeserver
- Commands parsed from messages with configurable prefix (default `!`)
- Reactions mapped to button handlers via `reaction:emoji` pattern (see `event::reaction_button_id`)
- Supports password auth or access token auth
- Embeds and link buttons are rendered to HTML, since Matrix has no interactive components
- File responses upload as attachments; captions follow as a separate message
- WASM compatible via `js` feature flag — core's `*Bounds` traits drop the `Send`/`Sync` requirements there, so keep new public traits cfg-gated the same way

```rust
// Matrix bot example
let config = MatrixConfig::new("https://matrix.org")
    .password_auth("@bot:matrix.org", "password")
    .command_prefix("!")
    .auto_join_rooms(true);

MatrixBot::new(config)
    .command("ping", ping)
    .command("greet", greet)
    .reaction("👍", on_thumbsup)
    .run()
    .await?;
```

### HTTP Stack

Uses custom HTTP crates (not tokio ecosystem):
- `skyzen` - HTTP server framework (similar to Axum)
- `zenwave` - WebSocket client
- `http-kit` - HTTP primitives
- `executor-core` - Async runtime primitives
- `matrix-sdk` - Matrix protocol (uses tokio internally but WASM-compatible)

## Code Conventions

- Uses Rust 2024 edition
- Error handling via `thiserror`
- Serialization via `serde`/`serde_json`
- Async patterns with `futures-lite`
- CI gates on `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and the WASM build; run all three before pushing
- Platform wire formats live in the platform crate, never in `botkit-core`

## Dependency pins

`skyzen`, `skyzen-core`, and `skyzen-macros` are pinned to exactly `0.1.0` in
the workspace manifest. They only compile as a matched set, and `skyzen 0.1.1`
pulls in `sqlx` -> `libsqlite3-sys 0.28`, which cannot coexist with
`matrix-sdk`'s `rusqlite`. `Cargo.lock` is gitignored, so unpinning these
breaks a fresh resolve (including in CI).
