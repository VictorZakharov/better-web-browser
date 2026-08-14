//! Fetch failures. HTTP status codes intentionally do not appear here.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchErrorKind {
    InvalidRequest,
    Network,
    Aborted,
    Cors,
    Redirect,
    BodyTooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchError {
    kind: FetchErrorKind,
    message: String,
}

impl FetchError {
    pub fn new(kind: FetchErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> FetchErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn network(message: impl Into<String>) -> Self {
        Self::new(FetchErrorKind::Network, message)
    }

    pub(crate) fn aborted() -> Self {
        Self::new(FetchErrorKind::Aborted, "request was aborted")
    }

    pub(crate) fn body_too_large(limit: usize) -> Self {
        Self::new(
            FetchErrorKind::BodyTooLarge,
            format!("response body exceeds its {limit}-byte budget"),
        )
    }
}

impl fmt::Display for FetchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for FetchError {}
