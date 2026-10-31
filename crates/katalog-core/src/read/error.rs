use serde::Serialize;

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
        }
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
        Self {
            message: error.to_string(),
            expected: false,
            code: None,
            busy,
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
}
