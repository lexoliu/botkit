use std::future::Future;

use crate::action::ChatActionGuard;
use crate::context::Context;

#[cfg(not(target_arch = "wasm32"))]
pub trait FromContextBounds: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> FromContextBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait FromContextBounds {}
#[cfg(target_arch = "wasm32")]
impl<T> FromContextBounds for T {}

#[cfg(not(target_arch = "wasm32"))]
pub trait ContextFutureBounds: Future + Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Future + Send + ?Sized> ContextFutureBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait ContextFutureBounds: Future {}
#[cfg(target_arch = "wasm32")]
impl<T: Future + ?Sized> ContextFutureBounds for T {}

/// Trait for extracting typed data from bot context
///
/// Similar to skyzen's `Extractor` trait, this allows handlers to
/// declare what data they need as function parameters.
///
/// # Example
/// ```ignore
/// // Built-in extractors
/// async fn greet(user: User) -> String {
///     format!("Hello, {}!", user.name)
/// }
///
/// async fn echo(args: CommandArgs) -> String {
///     args.0
/// }
/// ```
pub trait FromContext: Sized + FromContextBounds {
    /// Extract data from the context
    fn from_context(ctx: &Context) -> impl ContextFutureBounds<Output = Self>;
}

/// Extract the full context (for advanced use cases)
impl FromContext for Context {
    async fn from_context(ctx: &Context) -> Self {
        ctx.clone()
    }
}

/// User information extractor
#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub name: String,
}

impl FromContext for User {
    async fn from_context(ctx: &Context) -> Self {
        Self {
            id: ctx.user_id().to_string(),
            name: ctx.user_name().to_string(),
        }
    }
}

/// Channel information extractor
#[derive(Debug, Clone)]
pub struct Channel {
    pub id: String,
}

impl FromContext for Channel {
    async fn from_context(ctx: &Context) -> Self {
        Self {
            id: ctx.channel_id().to_string(),
        }
    }
}

/// Command name extractor
#[derive(Debug, Clone)]
pub struct CommandName(pub String);

impl FromContext for CommandName {
    async fn from_context(ctx: &Context) -> Self {
        Self(ctx.command_name().unwrap_or_default().to_string())
    }
}

/// Command arguments extractor (Telegram-style string args)
#[derive(Debug, Clone)]
pub struct CommandArgs(pub String);

impl FromContext for CommandArgs {
    async fn from_context(ctx: &Context) -> Self {
        Self(ctx.command_args().unwrap_or_default().to_string())
    }
}

/// Button/callback ID extractor
#[derive(Debug, Clone)]
pub struct ButtonId(pub String);

impl FromContext for ButtonId {
    async fn from_context(ctx: &Context) -> Self {
        Self(ctx.button_id().unwrap_or_default().to_string())
    }
}

/// Message content extractor
#[derive(Debug, Clone)]
pub struct MessageContent(pub String);

impl FromContext for MessageContent {
    async fn from_context(ctx: &Context) -> Self {
        Self(ctx.message_content().unwrap_or_default().to_string())
    }
}

// Implement FromContext for tuples (for multiple extractors)
impl FromContext for () {
    async fn from_context(_ctx: &Context) -> Self {}
}

impl<T1: FromContext> FromContext for (T1,) {
    async fn from_context(ctx: &Context) -> Self {
        (T1::from_context(ctx).await,)
    }
}

impl<T1: FromContext, T2: FromContext> FromContext for (T1, T2) {
    async fn from_context(ctx: &Context) -> Self {
        let t1 = T1::from_context(ctx).await;
        let t2 = T2::from_context(ctx).await;
        (t1, t2)
    }
}

impl<T1: FromContext, T2: FromContext, T3: FromContext> FromContext for (T1, T2, T3) {
    async fn from_context(ctx: &Context) -> Self {
        let t1 = T1::from_context(ctx).await;
        let t2 = T2::from_context(ctx).await;
        let t3 = T3::from_context(ctx).await;
        (t1, t2, t3)
    }
}

impl<T1: FromContext, T2: FromContext, T3: FromContext, T4: FromContext> FromContext
    for (T1, T2, T3, T4)
{
    async fn from_context(ctx: &Context) -> Self {
        let t1 = T1::from_context(ctx).await;
        let t2 = T2::from_context(ctx).await;
        let t3 = T3::from_context(ctx).await;
        let t4 = T4::from_context(ctx).await;
        (t1, t2, t3, t4)
    }
}

/// Typing indicator extractor
///
/// Automatically starts a typing indicator when extracted.
/// The indicator stops when the handler completes (guard is dropped).
///
/// # Example
/// ```ignore
/// async fn slow_handler(_typing: Typing) -> String {
///     expensive_work().await;
///     "Done!"
/// }
/// ```
pub struct Typing(pub Option<ChatActionGuard>);

impl FromContext for Typing {
    async fn from_context(ctx: &Context) -> Self {
        Self(ctx.typing())
    }
}
