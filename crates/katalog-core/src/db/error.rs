use std::fmt;

use crate::error_code::ErrorCode;

#[derive(Debug)]
pub enum DbError {
    Sqlite(rusqlite::Error),
    Other(String),
    /// The catalog was written by a newer app version; opening it with this one could lose data.
    NewerSchema { found: i64, supported: i64 },
    /// Rejected user input (e.g. an empty name): normal feedback, not a fault.
    Invalid(String),
    /// Rejected user input identified by a registered code; the interface turns it
    /// into text. `params` hold data only, never wording.
    Coded { code: ErrorCode, params: Vec<String> },
}

impl DbError {
    pub fn coded(code: ErrorCode) -> Self {
        DbError::Coded { code, params: Vec::new() }
    }

    pub fn coded_with<I, S>(code: ErrorCode, params: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: ToString,
    {
        DbError::Coded { code, params: params.into_iter().map(|p| p.to_string()).collect() }
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Sqlite(e) => write!(f, "database error: {e}"),
            DbError::Other(msg) => write!(f, "{msg}"),
            DbError::NewerSchema { found, supported } => write!(
                f,
                "Datenbank-Schemaversion {found} ist neuer als die von dieser App-Version unterstuetzte Version {supported}"
            ),
            DbError::Invalid(msg) => write!(f, "{msg}"),
            // The code is the best text a log or a fallback can show.
            DbError::Coded { code, .. } => f.write_str(code.as_str()),
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
