use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;

use crate::context::Context;
use crate::extractor::FromContext;
use crate::responder::IntoResponse;
use crate::response::Response;

#[cfg(not(target_arch = "wasm32"))]
pub type HandlerCallFuture<'a> = Pin<Box<dyn Future<Output = Response> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type HandlerCallFuture<'a> = Pin<Box<dyn Future<Output = Response> + 'a>>;

#[cfg(not(target_arch = "wasm32"))]
pub trait HandlerBounds: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> HandlerBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait HandlerBounds {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> HandlerBounds for T {}

#[cfg(not(target_arch = "wasm32"))]
pub trait HandlerFnBounds: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> HandlerFnBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait HandlerFnBounds {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> HandlerFnBounds for T {}

#[cfg(not(target_arch = "wasm32"))]
pub trait HandlerFutureBounds: Future + Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Future + Send + ?Sized> HandlerFutureBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait HandlerFutureBounds: Future {}
#[cfg(target_arch = "wasm32")]
impl<T: Future + ?Sized> HandlerFutureBounds for T {}

/// Trait for bot event handlers
///
/// Handlers use the extractor/responder pattern - they extract typed
/// data from context and return types that implement IntoResponse.
///
/// # Example
/// ```ignore
/// // No parameters
/// async fn ping() -> &'static str {
///     "Pong!"
/// }
///
/// // With extractors
/// async fn greet(user: User) -> String {
///     format!("Hello, {}!", user.name)
/// }
///
/// // Multiple extractors
/// async fn info(user: User, channel: Channel) -> String {
///     format!("User {} in channel {}", user.name, channel.id)
/// }
///
/// // Full context access when needed
/// async fn advanced(ctx: Context) -> Response {
///     // ... complex logic
///     Response::text("Done")
/// }
/// ```
pub trait Handler: HandlerBounds + 'static {
    /// Handle the event and produce a response
    fn call(&self, ctx: Context) -> HandlerCallFuture<'_>;
}

/// Boxed handler for storage
pub type BoxedHandler = Arc<dyn Handler>;

/// Trait to convert functions into handlers
pub trait IntoHandler<Args> {
    /// Convert this function into a boxed handler
    fn into_handler(self) -> BoxedHandler;
}

/// Adapter that pairs a handler function with the extractors it asks for.
///
/// `PhantomData<fn() -> Args>` is covariant in `Args` and unconditionally
/// `Send + Sync`, so the auto trait impls fall out of `F` alone — the extractor
/// types never have to be thread-safe themselves.
struct FnHandler<F, Args>(F, PhantomData<fn() -> Args>);

impl<F, Args> FnHandler<F, Args> {
    fn new(f: F) -> Self {
        Self(f, PhantomData)
    }
}

/// Generate an `IntoHandler` impl for a handler function of the given arity.
macro_rules! impl_into_handler {
    ($($ty:ident $arg:ident),*) => {
        impl<F, Fut, R, $($ty,)*> IntoHandler<($($ty,)*)> for F
        where
            F: Fn($($ty,)*) -> Fut + HandlerFnBounds + 'static,
            Fut: HandlerFutureBounds<Output = R> + 'static,
            R: IntoResponse + 'static,
            $($ty: FromContext + 'static,)*
        {
            fn into_handler(self) -> BoxedHandler {
                Arc::new(FnHandler::new(self))
            }
        }

        impl<F, Fut, R, $($ty,)*> Handler for FnHandler<F, ($($ty,)*)>
        where
            F: Fn($($ty,)*) -> Fut + HandlerFnBounds + 'static,
            Fut: HandlerFutureBounds<Output = R> + 'static,
            R: IntoResponse + 'static,
            $($ty: FromContext + 'static,)*
        {
            fn call(&self, ctx: Context) -> HandlerCallFuture<'_> {
                Box::pin(async move {
                    // `ctx` is unused at arity zero.
                    let _ = &ctx;
                    $(let $arg = $ty::from_context(&ctx).await;)*
                    (self.0)($($arg,)*).await.into_response()
                })
            }
        }
    };
}

impl_into_handler!();
impl_into_handler!(T1 t1);
impl_into_handler!(T1 t1, T2 t2);
impl_into_handler!(T1 t1, T2 t2, T3 t3);
impl_into_handler!(T1 t1, T2 t2, T3 t3, T4 t4);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::{Channel, CommandArgs, CommandName, User};
    use crate::test_util::StubData;
    use futures_lite::future::block_on;

    fn call<H: IntoHandler<Args>, Args>(handler: H) -> Response {
        let handler = handler.into_handler();
        block_on(handler.call(Context::new(StubData)))
    }

    #[test]
    fn zero_arg_handler() {
        async fn ping() -> &'static str {
            "Pong!"
        }
        assert_eq!(call(ping).content(), Some("Pong!"));
    }

    #[test]
    fn extractors_are_applied_in_order() {
        async fn four(a: CommandName, b: CommandArgs, c: User, d: Channel) -> String {
            format!("{} {} {} {}", a.0, b.0, c.name, d.id)
        }
        assert_eq!(
            call(four).content(),
            Some("cmd args stub-user stub-channel")
        );
    }

    #[test]
    fn closures_are_handlers_too() {
        let handler = || async { String::from("closure") };
        assert_eq!(call(handler).content(), Some("closure"));
    }

    #[test]
    fn handlers_may_capture_non_thread_safe_extractors() {
        // `Context` is not `Sync`-bound by the extractor itself; this compiles
        // only because `FnHandler`'s auto traits depend on `F` alone.
        async fn with_ctx(ctx: Context) -> String {
            ctx.user_id().to_string()
        }
        assert_eq!(call(with_ctx).content(), Some("stub-user"));
    }
}
