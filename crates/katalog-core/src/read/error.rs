use serde::Serialize;

use crate::error_code::ErrorCode;

/// Transport-neutral counterpart of the desktop error contract. Logging stays
/// in the desktop adapter; classification and wire codes travel with the error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadError {
    pub message: String,
    pub expected: bool,
    /// SQLite busy/locked classification, deliberately absent from the desktop wire contract.
    #[serde(skip)]
    pub busy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<GeometryErrorCode>,
    /// Values for the placeholders of the translated text (names, numbers).
    /// Never words of a language: the frontend owns all wording.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GeometryErrorCode {
    NotFound,
    Unreadable,
    TooLarge,
    Unsupported,
}
impl ReadError {
    pub fn expected(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            expected: true,
            code: None,
            busy: false,
            params: Vec::new(),
        }
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
    pub fn with_code(mut self, code: GeometryErrorCode) -> Self {
        self.code = Some(code);
        self
    }
}
impl From<String> for ReadError {
    fn from(message: String) -> Self {
        Self {
            message,
            expected: false,
            code: None,
            busy: false,
            params: Vec::new(),
        }
    }
}
impl From<&str> for ReadError {
    fn from(message: &str) -> Self {
        Self::from(message.to_owned())
    }
}
impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ReadError {}

impl From<crate::db::error::DbError> for ReadError {
    fn from(error: crate::db::error::DbError) -> Self {
        let busy = matches!(&error, crate::db::error::DbError::Sqlite(rusqlite::Error::SqliteFailure(code, _))
            if matches!(code.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked));
        match error {
            crate::db::error::DbError::Coded { code, params } => {
                Self::coded_with(code, params)
            }
            other => Self {
                message: other.to_string(),
                expected: false,
                code: None,
                busy,
                params: Vec::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sqlite_busy_and_locked_are_structured_but_wire_contract_is_unchanged() {
        for code in [rusqlite::ffi::SQLITE_BUSY, rusqlite::ffi::SQLITE_LOCKED] {
            let error = ReadError::from(crate::db::error::DbError::from(
                rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None),
            ));
            assert!(error.busy);
            assert_eq!(
                serde_json::to_value(&error).unwrap(),
                serde_json::json!({
                    "message": error.message, "expected": false
                })
            );
        }
        assert!(!ReadError::from("database is locked").busy);
    }

    #[test]
    fn coded_error_carries_code_and_params_on_the_wire() {
        let error = ReadError::coded_with(ErrorCode::MoveTooManyFiles, [501, 500]);
        assert!(error.expected && !error.busy && error.code.is_none());
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({"message": "moveTooManyFiles", "expected": true, "params": ["501", "500"]})
        );
        // Without params the wire form is the one clients have always seen.
        assert_eq!(
            serde_json::to_value(ReadError::coded(ErrorCode::FolderNameEmpty)).unwrap(),
            serde_json::json!({"message": "folderNameEmpty", "expected": true})
        );
        assert_eq!(
            serde_json::to_value(ReadError::expected("Freitext")).unwrap(),
            serde_json::json!({"message": "Freitext", "expected": true})
        );
    }

    #[test]
    fn a_coded_database_error_stays_a_code() {
        let error = ReadError::from(crate::db::error::DbError::coded_with(ErrorCode::InvalidCatalogId, ["x"]));
        assert_eq!(error, ReadError::coded_with(ErrorCode::InvalidCatalogId, ["x"]));
    }
}
