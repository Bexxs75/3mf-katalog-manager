use std::fmt;

#[derive(Debug)]
pub enum DbError {
    Sqlite(rusqlite::Error),
    Other(String),
    /// Rejected user input (e.g. an empty name): normal feedback, not a fault.
    Invalid(String),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Sqlite(e) => write!(f, "database error: {e}"),
            DbError::Other(msg) => write!(f, "{msg}"),
            DbError::Invalid(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Sqlite(e)
    }
}

/// Count UTF-16 units to match HTML input maxLength, including non-BMP text.
pub fn validate_text_length(value: &str, max: usize, label: &str) -> Result<(), DbError> {
    if value.encode_utf16().take(max + 1).count() > max {
        return Err(DbError::Invalid(format!("{label} darf höchstens {max} Zeichen enthalten")));
    }
    Ok(())
}
