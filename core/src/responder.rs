use crate::response::Response;

#[cfg(not(target_arch = "wasm32"))]
pub trait IntoResponseBounds: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + ?Sized> IntoResponseBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait IntoResponseBounds {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> IntoResponseBounds for T {}

/// Trait for converting types into bot responses
///
/// Similar to skyzen's `Responder` trait, this allows handlers to
/// return various types that get converted to responses.
///
/// # Example
/// ```ignore
/// // Return a string directly
/// async fn ping() -> &'static str {
///     "Pong!"
/// }
///
/// // Return a formatted string
/// async fn greet(user: User) -> String {
///     format!("Hello, {}!", user.name)
/// }
///
/// // Return Response for full control
/// async fn buttons() -> Response {
///     Response::text("Click a button:")
///         .with_components(vec![...])
/// }
/// ```
pub trait IntoResponse: IntoResponseBounds {
    /// Convert this type into a bot response
    fn into_response(self) -> Response;
}

// String types
impl IntoResponse for String {
    fn into_response(self) -> Response {
        Response::text(self)
    }
}

impl IntoResponse for &'static str {
    fn into_response(self) -> Response {
        Response::text(self)
    }
}

impl IntoResponse for std::borrow::Cow<'static, str> {
    fn into_response(self) -> Response {
        Response::text(self)
    }
}

// Response passes through unchanged
impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

// Unit type returns empty response
impl IntoResponse for () {
    fn into_response(self) -> Response {
        Response::empty()
    }
}

// Option<T> - None returns empty response
impl<T: IntoResponse> IntoResponse for Option<T> {
    fn into_response(self) -> Response {
        match self {
            Some(value) => value.into_response(),
            None => Response::empty(),
        }
    }
}

/// `Err` is rendered to the user as `Error: {e}`.
///
/// Return `Response` directly when a handler needs to control what a failure
/// looks like, or keep the error type's `Display` user-facing.
impl<T, E> IntoResponse for Result<T, E>
where
    T: IntoResponse,
    E: std::fmt::Display + IntoResponseBounds,
{
    fn into_response(self) -> Response {
        match self {
            Ok(value) => value.into_response(),
            Err(e) => Response::text(format!("Error: {e}")),
        }
    }
}

// File responses
impl IntoResponse for async_fs::File {
    fn into_response(self) -> Response {
        Response::file(self)
    }
}

impl IntoResponse for std::path::PathBuf {
    fn into_response(self) -> Response {
        Response::path(self)
    }
}

/// A file paired with a caption.
///
/// Only file-shaped payloads get this impl: `with_caption` is meaningless for a
/// text response, so `("text", "caption")` is a compile error rather than a
/// silently dropped caption.
macro_rules! impl_captioned_file {
    ($ty:ty) => {
        impl<C: Into<String> + IntoResponseBounds> IntoResponse for ($ty, C) {
            fn into_response(self) -> Response {
                let (file, caption) = self;
                file.into_response().with_caption(caption)
            }
        }
    };
}

impl_captioned_file!(async_fs::File);
impl_captioned_file!(std::path::PathBuf);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_become_text_responses() {
        assert_eq!("hi".into_response().content(), Some("hi"));
        assert_eq!(String::from("hi").into_response().content(), Some("hi"));
        assert_eq!(
            std::borrow::Cow::Borrowed("hi").into_response().content(),
            Some("hi")
        );
    }

    #[test]
    fn unit_and_none_are_empty() {
        assert!(().into_response().is_empty());
        assert!(Option::<String>::None.into_response().is_empty());
        assert_eq!(Some("hi").into_response().content(), Some("hi"));
    }

    #[test]
    fn errors_render_their_display() {
        let result: Result<&str, std::fmt::Error> = Err(std::fmt::Error);
        assert_eq!(
            result.into_response().content(),
            Some("Error: an error occurred when formatting an argument")
        );
    }

    #[test]
    fn ok_passes_the_inner_value_through() {
        let result: Result<&str, std::fmt::Error> = Ok("fine");
        assert_eq!(result.into_response().content(), Some("fine"));
    }

    #[test]
    fn captions_attach_to_file_responses() {
        let mut response = (std::path::PathBuf::from("/tmp/a.txt"), "caption").into_response();
        let file = response.take_file().unwrap();
        assert_eq!(file.caption.as_deref(), Some("caption"));
        assert_eq!(file.filename.as_deref(), Some("a.txt"));
    }
}
