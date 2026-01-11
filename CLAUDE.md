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

# Run examples
cargo run -p botkit-examples --bin unified
cargo run -p botkit-examples --bin test_telegram
cargo run -p botkit-examples --bin test_polling

# Check without building
cargo check

# Format code
cargo fmt

# Run clippy
cargo clippy
```

## Architecture

This is a unified bot framework supporting Discord and Telegram platforms through a common abstraction layer.

### Workspace Structure

- **core/** (`botkit-core`): Platform-agnostic abstractions - traits, extractors, responders, and shared types
- **discord/** (`botkit-discord`): Discord implementation using WebSocket Gateway
- **telegram/** (`botkit-telegram`): Telegram implementation supporting both webhooks (via skyzen HTTP) and long polling
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

3. **Responders** (`responder.rs`): Return types implementing `IntoResponse` are converted to platform responses. Supports `&str`, `String`, `Response`.

4. **Context** (`context.rs`): Unified interface wrapping platform-specific `ContextData` implementations. Provides consistent access to user, channel, command, and message data across platforms.

5. **Response** (`response.rs`): Rich response type supporting text, embeds, components (buttons/action rows), and flags (ephemeral).

### Handler Registration Pattern

```rust
// Handlers are plain async functions
async fn ping() -> &'static str { "Pong!" }
async fn greet(user: User) -> String { format!("Hello, {}!", user.name) }

// Register with builder pattern
DiscordBot::new(token, app_id, intents)
    .command("ping", ping)
    .command("greet", greet)
    .button("btn_id", button_handler)
```

### Platform Implementations

**Discord** (`discord/`):
- Connects via WebSocket to Discord Gateway
- Handles slash commands and button interactions
- `bot.run()` starts the connection loop

**Telegram** (`telegram/`):
- **Webhook mode**: `bot.build()` returns `TelegramWebhook` for skyzen HTTP integration
- **Polling mode**: `bot.run_polling()` for development/testing
- Handles `/commands` and inline keyboard callbacks

### HTTP Stack

Uses custom HTTP crates (not tokio ecosystem):
- `skyzen` - HTTP server framework (similar to Axum)
- `zenwave` - WebSocket client
- `http-kit` - HTTP primitives
- `executor-core` - Async runtime primitives

## Code Conventions

- Uses Rust 2024 edition
- Error handling via `thiserror`
- Serialization via `serde`/`serde_json`
- Async patterns with `futures-lite`
