# botkit

`botkit` is a Rust bot framework for building chat bots with one handler model across multiple platforms.

The workspace is split into small crates:

- `botkit-core` provides the shared bot abstractions, extractors, handlers, and response types.
- `botkit-discord` provides the Discord integration.
- `botkit-telegram` provides the Telegram integration.
- `botkit-matrix` provides the Matrix integration.

## Design goals

- Reuse the same handler style across platforms.
- Keep the core abstractions transport-agnostic.
- Support async-first bot execution.
- Expose platform crates separately instead of hiding everything behind type erasure.

## Crates

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
