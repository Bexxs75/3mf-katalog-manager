use serde::Serialize;

use crate::db::error::DbError;

/// Transport-neutral import error. Desktop diagnostics live in its adapter. `expected` separates normal feedback
/// ("a file with this name already exists") from real faults, so the UI only
/// offers "Report problem" for the latter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportError {
    pub message: String,
    pub expected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<GeometryErrorCode>,
}

pub use crate::read::GeometryErrorCode;

impl ImportError {
    pub fn with_code(mut self, code: GeometryErrorCode) -> Self {
        self.code = Some(code);
        self
    }

    pub fn expected(message: impl Into<String>) -> Self {
        Self { message: message.into(), expected: true, code: None }
    }
}

impl From<String> for ImportError {
    #[track_caller]
    fn from(message: String) -> Self {
        Self { message, expected: false, code: None }
    }
}

impl From<&str> for ImportError {
    #[track_caller]
    fn from(message: &str) -> Self {
        Self::from(message.to_string())
    }
}

impl From<DbError> for ImportError {
    #[track_caller]
    fn from(e: DbError) -> Self {
        match e {
            DbError::Invalid(msg) => Self::expected(msg),
            other => Self::from(other.to_string()),
        }
    }
}

impl std::ops::Deref for ImportError {
    type Target = str;
    fn deref(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl PartialEq<&str> for ImportError {
    fn eq(&self, other: &&str) -> bool {
        self.message == *other
    }
}

impl PartialEq<str> for ImportError {
    fn eq(&self, other: &str) -> bool {
        self.message == other
    }
}

impl PartialEq<String> for ImportError {
    fn eq(&self, other: &String) -> bool {
        &self.message == other
    }
}


impl std::error::Error for ImportError {}
