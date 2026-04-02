#![allow(clippy::type_complexity)]
use std::future::Future;
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

// Zero args
impl<F, Fut, R> IntoHandler<()> for F
where
    F: Fn() -> Fut + HandlerFnBounds + 'static,
    Fut: HandlerFutureBounds<Output = R> + 'static,
    R: IntoResponse + 'static,
{
    fn into_handler(self) -> BoxedHandler {
        struct H<F>(F);

        impl<F, Fut, R> Handler for H<F>
        where
            F: Fn() -> Fut + HandlerFnBounds + 'static,
            Fut: HandlerFutureBounds<Output = R> + 'static,
            R: IntoResponse + 'static,
        {
            fn call(&self, _ctx: Context) -> HandlerCallFuture<'_> {
                let fut = (self.0)();
                Box::pin(async move { fut.await.into_response() })
            }
        }

        Arc::new(H(self))
    }
}

// One arg
impl<F, Fut, T1, R> IntoHandler<(T1,)> for F
where
    F: Fn(T1) -> Fut + HandlerFnBounds + 'static,
    Fut: HandlerFutureBounds<Output = R> + 'static,
    T1: FromContext + 'static,
    R: IntoResponse + 'static,
{
    fn into_handler(self) -> BoxedHandler {
        struct H<F, T1>(F, std::marker::PhantomData<fn() -> T1>);

        // Safety: PhantomData<fn() -> T1> is always Send+Sync
        unsafe impl<F: Send, T1> Send for H<F, T1> {}
        unsafe impl<F: Sync, T1> Sync for H<F, T1> {}

        impl<F, Fut, T1, R> Handler for H<F, T1>
        where
            F: Fn(T1) -> Fut + HandlerFnBounds + 'static,
            Fut: HandlerFutureBounds<Output = R> + 'static,
            T1: FromContext + 'static,
            R: IntoResponse + 'static,
        {
            fn call(&self, ctx: Context) -> HandlerCallFuture<'_> {
                Box::pin(async move {
                    let t1 = T1::from_context(&ctx).await;
                    (self.0)(t1).await.into_response()
                })
            }
        }

        Arc::new(H(self, std::marker::PhantomData))
    }
}

// Two args
impl<F, Fut, T1, T2, R> IntoHandler<(T1, T2)> for F
where
    F: Fn(T1, T2) -> Fut + HandlerFnBounds + 'static,
    Fut: HandlerFutureBounds<Output = R> + 'static,
    T1: FromContext + 'static,
    T2: FromContext + 'static,
    R: IntoResponse + 'static,
{
    fn into_handler(self) -> BoxedHandler {
        struct H<F, T1, T2>(F, std::marker::PhantomData<fn() -> (T1, T2)>);

        unsafe impl<F: Send, T1, T2> Send for H<F, T1, T2> {}
        unsafe impl<F: Sync, T1, T2> Sync for H<F, T1, T2> {}

        impl<F, Fut, T1, T2, R> Handler for H<F, T1, T2>
        where
            F: Fn(T1, T2) -> Fut + HandlerFnBounds + 'static,
            Fut: HandlerFutureBounds<Output = R> + 'static,
            T1: FromContext + 'static,
            T2: FromContext + 'static,
            R: IntoResponse + 'static,
        {
            fn call(&self, ctx: Context) -> HandlerCallFuture<'_> {
                Box::pin(async move {
                    let t1 = T1::from_context(&ctx).await;
                    let t2 = T2::from_context(&ctx).await;
                    (self.0)(t1, t2).await.into_response()
                })
            }
        }

        Arc::new(H(self, std::marker::PhantomData))
    }
}

// Three args
impl<F, Fut, T1, T2, T3, R> IntoHandler<(T1, T2, T3)> for F
where
    F: Fn(T1, T2, T3) -> Fut + HandlerFnBounds + 'static,
    Fut: HandlerFutureBounds<Output = R> + 'static,
    T1: FromContext + 'static,
    T2: FromContext + 'static,
    T3: FromContext + 'static,
    R: IntoResponse + 'static,
{
    fn into_handler(self) -> BoxedHandler {
        struct H<F, T1, T2, T3>(F, std::marker::PhantomData<fn() -> (T1, T2, T3)>);

        unsafe impl<F: Send, T1, T2, T3> Send for H<F, T1, T2, T3> {}
        unsafe impl<F: Sync, T1, T2, T3> Sync for H<F, T1, T2, T3> {}

        impl<F, Fut, T1, T2, T3, R> Handler for H<F, T1, T2, T3>
        where
            F: Fn(T1, T2, T3) -> Fut + HandlerFnBounds + 'static,
            Fut: HandlerFutureBounds<Output = R> + 'static,
            T1: FromContext + 'static,
            T2: FromContext + 'static,
            T3: FromContext + 'static,
            R: IntoResponse + 'static,
        {
            fn call(&self, ctx: Context) -> HandlerCallFuture<'_> {
                Box::pin(async move {
                    let t1 = T1::from_context(&ctx).await;
                    let t2 = T2::from_context(&ctx).await;
                    let t3 = T3::from_context(&ctx).await;
                    (self.0)(t1, t2, t3).await.into_response()
                })
            }
        }

        Arc::new(H(self, std::marker::PhantomData))
    }
}

// Four args
impl<F, Fut, T1, T2, T3, T4, R> IntoHandler<(T1, T2, T3, T4)> for F
where
    F: Fn(T1, T2, T3, T4) -> Fut + HandlerFnBounds + 'static,
    Fut: HandlerFutureBounds<Output = R> + 'static,
    T1: FromContext + 'static,
    T2: FromContext + 'static,
    T3: FromContext + 'static,
    T4: FromContext + 'static,
    R: IntoResponse + 'static,
{
    fn into_handler(self) -> BoxedHandler {
        struct H<F, T1, T2, T3, T4>(F, std::marker::PhantomData<fn() -> (T1, T2, T3, T4)>);

        unsafe impl<F: Send, T1, T2, T3, T4> Send for H<F, T1, T2, T3, T4> {}
        unsafe impl<F: Sync, T1, T2, T3, T4> Sync for H<F, T1, T2, T3, T4> {}

        impl<F, Fut, T1, T2, T3, T4, R> Handler for H<F, T1, T2, T3, T4>
        where
            F: Fn(T1, T2, T3, T4) -> Fut + HandlerFnBounds + 'static,
            Fut: HandlerFutureBounds<Output = R> + 'static,
            T1: FromContext + 'static,
            T2: FromContext + 'static,
            T3: FromContext + 'static,
            T4: FromContext + 'static,
            R: IntoResponse + 'static,
        {
            fn call(&self, ctx: Context) -> HandlerCallFuture<'_> {
                Box::pin(async move {
                    let t1 = T1::from_context(&ctx).await;
                    let t2 = T2::from_context(&ctx).await;
                    let t3 = T3::from_context(&ctx).await;
                    let t4 = T4::from_context(&ctx).await;
                    (self.0)(t1, t2, t3, t4).await.into_response()
                })
            }
        }

        Arc::new(H(self, std::marker::PhantomData))
    }
}
