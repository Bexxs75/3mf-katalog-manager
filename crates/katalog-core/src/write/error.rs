use crate::db::error::DbError;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    Invalid { message: String },
    NotFound { message: String },
    Busy { message: String },
    Other { message: String, kind: &'static str },
}
impl WriteError {
    pub fn expected(message: impl Into<String>) -> Self {
        Self::Invalid {
            message: message.into(),
        }
    }
}
impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (Self::Invalid { message }
        | Self::NotFound { message }
        | Self::Busy { message }
        | Self::Other { message, .. }) = self;
        f.write_str(message)
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
