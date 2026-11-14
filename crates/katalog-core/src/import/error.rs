use serde::Serialize;

use crate::db::error::DbError;
use crate::error_code::ErrorCode;

/// Transport-neutral import error. Desktop diagnostics live in its adapter. `expected` separates normal feedback
/// ("a file with this name already exists") from real faults, so the UI only
/// offers "Report problem" for the latter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportError {
    pub message: String,
    pub expected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<GeometryErrorCode>,
    /// Values for the placeholders of the translated text (names, numbers).
    /// Never words of a language: the frontend owns all wording.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
}

pub use crate::read::GeometryErrorCode;

impl ImportError {
    pub fn with_code(mut self, code: GeometryErrorCode) -> Self {
        self.code = Some(code);
        self
    }

    pub fn expected(message: impl Into<String>) -> Self {
        Self { message: message.into(), expected: true, code: None, params: Vec::new() }
    }

    /// An expected error identified by a registered code; the frontend turns it
    /// into text in the chosen language.
    pub fn coded(code: ErrorCode) -> Self {
        Self::expected(code.as_str())
    }

    /// Like `coded`, with the values the translated text needs. `params` hold data
    /// only (names, numbers), never wording and never server paths.
    pub fn coded_with<I, S>(code: ErrorCode, params: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: ToString,
    {
        let mut error = Self::coded(code);
        error.params = params.into_iter().map(|p| p.to_string()).collect();
        error
    }
}

impl From<String> for ImportError {
    #[track_caller]
    fn from(message: String) -> Self {
        Self { message, expected: false, code: None, params: Vec::new() }
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
            DbError::Coded { code, params } => Self::coded_with(code, params),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coded_error_carries_code_and_params_on_the_wire() {
        assert_eq!(
            serde_json::to_value(ImportError::coded_with(ErrorCode::FileNameForbiddenChar, [":"])).unwrap(),
            serde_json::json!({"message": "fileNameForbiddenChar", "expected": true, "params": [":"]})
        );
        assert_eq!(
            serde_json::to_value(ImportError::coded(ErrorCode::FileNameEmpty)).unwrap(),
            serde_json::json!({"message": "fileNameEmpty", "expected": true})
        );
        assert_eq!(
            serde_json::to_value(ImportError::expected("Freitext")).unwrap(),
            serde_json::json!({"message": "Freitext", "expected": true})
        );
    }

    #[test]
    fn a_coded_database_error_stays_a_code() {
        let error = ImportError::from(DbError::coded_with(ErrorCode::InvalidCatalogId, ["x"]));
        assert_eq!(error, ImportError::coded_with(ErrorCode::InvalidCatalogId, ["x"]));
        assert!(error.expected);
    }
}
