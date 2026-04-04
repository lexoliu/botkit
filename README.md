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

- `Bot` and `BotBuilder`
- extractor-based handlers such as `User`, `CommandArgs`, and `MessageContent`
- unified `Response` and file response types

### `botkit-discord`

Discord support built around gateway events and command/button handlers.

### `botkit-telegram`

Telegram support for webhook-style handling and bot client operations.

### `botkit-matrix`

Matrix support built on top of `matrix-sdk`, including encrypted room support.

## Status

This project is a library workspace under active development. The API surface is still evolving.
