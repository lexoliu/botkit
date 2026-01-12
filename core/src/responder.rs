use crate::response::Response;

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
pub trait IntoResponse: Send {
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

// Result<T, E> - Ok returns T, Err returns error message
impl<T: IntoResponse, E: std::fmt::Display + Send> IntoResponse for Result<T, E> {
    fn into_response(self) -> Response {
        match self {
            Ok(value) => value.into_response(),
            Err(e) => Response::text(format!("Error: {}", e)),
        }
    }
}

// File response
impl IntoResponse for async_fs::File {
    fn into_response(self) -> Response {
        Response::file(self)
    }
}

// File with caption (static str)
impl IntoResponse for (async_fs::File, &'static str) {
    fn into_response(self) -> Response {
        Response::file(self.0).with_caption(self.1)
    }
}

// File with caption (String)
impl IntoResponse for (async_fs::File, String) {
    fn into_response(self) -> Response {
        Response::file(self.0).with_caption(self.1)
    }
}
