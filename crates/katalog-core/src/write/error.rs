use crate::db::error::DbError;
use crate::error_code::ErrorCode;

/// How the server classifies a coded write error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodedKind {
    Invalid,
    NotFound,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    Invalid { message: String },
    NotFound { message: String },
    Busy { message: String },
    Other { message: String, kind: &'static str },
    /// A rejection identified by a registered code; the interface turns it into
    /// text. `params` hold data only (names, numbers), never wording and never
    /// server paths.
    Coded { kind: CodedKind, code: ErrorCode, params: Vec<String> },
}
impl WriteError {
    pub fn expected(message: impl Into<String>) -> Self {
        Self::Invalid {
            message: message.into(),
        }
    }
    /// Rejected input identified by a registered code (HTTP 422 on the server).
    pub fn coded(code: ErrorCode) -> Self {
        Self::coded_with(code, [] as [String; 0])
    }
    pub fn coded_with<I, S>(code: ErrorCode, params: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: ToString,
    {
        Self::Coded {
            kind: CodedKind::Invalid,
            code,
            params: params.into_iter().map(|p| p.to_string()).collect(),
        }
    }
    /// A missing entry identified by a registered code (HTTP 404 on the server).
    pub fn coded_not_found(code: ErrorCode) -> Self {
        Self::Coded { kind: CodedKind::NotFound, code, params: Vec::new() }
    }
}
impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid { message }
            | Self::NotFound { message }
            | Self::Busy { message }
            | Self::Other { message, .. } => f.write_str(message),
            // The code is the best text a log or a fallback can show.
            Self::Coded { code, .. } => f.write_str(code.as_str()),
        }
    }
}
impl std::error::Error for WriteError {}
impl From<String> for WriteError {
    fn from(message: String) -> Self {
        Self::Other {
            message,
            kind: "other",
        }
    }
}
impl From<rusqlite::Error> for WriteError {
    fn from(e: rusqlite::Error) -> Self {
        let message = e.to_string();
        if matches!(&e, rusqlite::Error::SqliteFailure(code, _) if matches!(code.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked))
        {
            Self::Busy { message }
        } else {
            let kind = match e {
                rusqlite::Error::SqliteFailure(code, _) => match code.code {
                    rusqlite::ErrorCode::ConstraintViolation => "sqlite_constraint",
                    rusqlite::ErrorCode::DatabaseCorrupt => "sqlite_corrupt",
                    rusqlite::ErrorCode::SystemIoFailure => "sqlite_io",
                    _ => "sqlite_failure",
                },
                _ => "sqlite_conversion",
            };
            Self::Other { message, kind }
        }
    }
}
impl From<DbError> for WriteError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::Invalid(message) => Self::Invalid { message },
            DbError::Coded { code, params } => Self::coded_with(code, params),
            DbError::Sqlite(e) => {
                let mut error = Self::from(e);
                match &mut error {
                    Self::Busy { message } | Self::Other { message, .. } => {
                        *message = format!("database error: {message}")
                    }
                    _ => {}
                }
                error
            }
            other => Self::from(other.to_string()),
        }
    }
}
