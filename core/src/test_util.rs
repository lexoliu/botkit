//! Shared test fixtures for this crate's unit tests.

use std::any::Any;

use crate::{ContextData, OptionValue};

/// A `ContextData` with fixed values, so extractor and routing tests don't need
/// a platform adapter.
pub(crate) struct StubData;

impl ContextData for StubData {
    fn channel_id(&self) -> &str {
        "stub-channel"
    }

    fn user_id(&self) -> &str {
        "stub-user"
    }

    fn user_name(&self) -> &str {
        "stub-user"
    }

    fn command_name(&self) -> Option<&str> {
        Some("cmd")
    }

    fn command_args(&self) -> Option<&str> {
        Some("args")
    }

    fn option(&self, name: &str) -> Option<OptionValue> {
        match name {
            "count" => Some(OptionValue::Integer(7)),
            "label" => Some(OptionValue::String("hi".into())),
            _ => None,
        }
    }

    fn button_id(&self) -> Option<&str> {
        Some("stub-button")
    }

    fn message_content(&self) -> Option<&str> {
        Some("stub message")
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A `ContextData` where every optional field is absent, for exercising the
/// `unwrap_or_default` paths in extractors.
pub(crate) struct EmptyData;

impl ContextData for EmptyData {
    fn channel_id(&self) -> &str {
        ""
    }

    fn user_id(&self) -> &str {
        ""
    }

    fn user_name(&self) -> &str {
        ""
    }

    fn command_name(&self) -> Option<&str> {
        None
    }

    fn command_args(&self) -> Option<&str> {
        None
    }

    fn option(&self, _name: &str) -> Option<OptionValue> {
        None
    }

    fn button_id(&self) -> Option<&str> {
        None
    }

    fn message_content(&self) -> Option<&str> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
