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

// Tuples of extractors extract each element in declaration order.
macro_rules! impl_from_context_tuple {
    () => {
        impl FromContext for () {
            async fn from_context(_ctx: &Context) -> Self {}
        }
    };
    ($($ty:ident),+) => {
        impl<$($ty: FromContext,)+> FromContext for ($($ty,)+) {
            async fn from_context(ctx: &Context) -> Self {
                ($($ty::from_context(ctx).await,)+)
            }
        }
    };
}

impl_from_context_tuple!();
impl_from_context_tuple!(T1);
impl_from_context_tuple!(T1, T2);
impl_from_context_tuple!(T1, T2, T3);
impl_from_context_tuple!(T1, T2, T3, T4);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{EmptyData, StubData};
    use futures_lite::future::block_on;

    fn stub() -> Context {
        Context::new(StubData)
    }

    fn empty() -> Context {
        Context::new(EmptyData)
    }

    #[test]
    fn extracts_user_and_channel() {
        let user = block_on(User::from_context(&stub()));
        assert_eq!(
            (user.id.as_str(), user.name.as_str()),
            ("stub-user", "stub-user")
        );
        assert_eq!(block_on(Channel::from_context(&stub())).id, "stub-channel");
    }

    #[test]
    fn extracts_command_parts() {
        assert_eq!(block_on(CommandName::from_context(&stub())).0, "cmd");
        assert_eq!(block_on(CommandArgs::from_context(&stub())).0, "args");
        assert_eq!(block_on(ButtonId::from_context(&stub())).0, "stub-button");
        assert_eq!(
            block_on(MessageContent::from_context(&stub())).0,
            "stub message"
        );
    }

    #[test]
    fn absent_fields_extract_as_empty_strings() {
        assert_eq!(block_on(CommandName::from_context(&empty())).0, "");
        assert_eq!(block_on(CommandArgs::from_context(&empty())).0, "");
        assert_eq!(block_on(ButtonId::from_context(&empty())).0, "");
        assert_eq!(block_on(MessageContent::from_context(&empty())).0, "");
    }

    #[test]
    fn tuples_extract_every_element() {
        let (name, args, user) =
            block_on(<(CommandName, CommandArgs, User)>::from_context(&stub()));
        assert_eq!((name.0.as_str(), args.0.as_str()), ("cmd", "args"));
        assert_eq!(user.id, "stub-user");
    }

    #[test]
    fn context_extracts_itself() {
        let ctx = block_on(Context::from_context(&stub()));
        assert_eq!(ctx.user_id(), "stub-user");
    }

    #[test]
    fn typing_is_absent_without_platform_support() {
        // `StubData` does not override `action_sender`.
        assert!(block_on(Typing::from_context(&stub())).0.is_none());
    }
}
