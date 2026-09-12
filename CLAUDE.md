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
cargo run -p botkit-examples --bin discord
cargo run -p botkit-examples --bin matrix
cargo run -p botkit-examples --bin telegram_polling
cargo run -p botkit-examples --bin telegram_typing_and_files

# The skyzen (HTTP webhook) examples are their own workspace
cargo run --manifest-path examples/webhook/Cargo.toml --bin telegram_webhook
cargo run --manifest-path examples/webhook/Cargo.toml --bin unified

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
- **examples/**: Runnable examples per platform
- **examples/webhook/**: skyzen-based HTTP webhook examples. A **separate workspace**, excluded from the root one — see "Dependency constraints" below

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
- Public trait methods return `impl Future`; never expose `Pin<Box<dyn Future>>` or boxed-future aliases in public signatures.
- When a trait must be stored dynamically, define a private object-safe twin `[Trait]Impl` plus a public wrapper `Any[Trait]` (e.g. `AnyChatActionSender`, `AnyHandler`, `AnyContainerExec`; `Tools` plays that role for `Tool`). Boxing lives inside the wrapper — callers only see the concrete API.
- `anyhow` is banned: every error is a `thiserror` enum so callers can match on failure kinds. `Box<dyn Error>` is acceptable only at a binary's outermost boundary.
- Serialization via `serde`/`serde_json`
- Async patterns with `futures-lite`
- CI gates on `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and the WASM build; run all three before pushing
- Platform wire formats live in the platform crate, never in `botkit-core`

## Dependency constraints

**skyzen and matrix-sdk cannot share a workspace.** skyzen depends
(non-optionally) on `skyzen-services` -> `sqlx` -> `libsqlite3-sys`, while
matrix-sdk's state store pulls `rusqlite` -> a *different* `libsqlite3-sys`.
Two packages declaring `links = "sqlite3"` cannot appear in one resolve graph,
and cargo enforces that across an entire workspace rather than per crate.

That is why `examples/webhook/` carries its own `[workspace]` table and is listed
under `exclude` in the root manifest. Do not add `botkit-telegram`'s webhook
examples back into the root workspace, and do not add a skyzen dependency to any
root workspace member. The library crates are unaffected: `botkit-telegram`
integrates over `http-kit`'s `Endpoint`, not skyzen.

**rustls needs an explicitly installed crypto provider.** Discord and Telegram
reach TLS through zenwave (`ring`); Matrix reaches it through reqwest
(`aws-lc-rs`). A bot using Matrix *and* one of the others compiles both into
`rustls`, which then refuses to auto-detect and panics on the first handshake.
Each adapter therefore calls `install_crypto_provider()` when its client is
built — whichever runs first wins and the rest are no-ops. Any new networking
entry point must do the same.

**getrandom on wasm32** needs both the `wasm_js` feature and
`--cfg getrandom_backend="wasm_js"`. The cfg lives in `.cargo/config.toml`, and
the `getrandom` version in `matrix/Cargo.toml` must track whatever matrix-sdk
pulls, or the feature lands on the wrong copy of the crate.

`Cargo.lock` is gitignored, so every CI run resolves fresh — a version
combination that only works locally will fail there.
