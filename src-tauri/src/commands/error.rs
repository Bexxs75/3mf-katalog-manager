use serde::Serialize;

use crate::db::error::DbError;

/// Error returned by a Tauri command. `expected` separates normal feedback
/// ("a file with this name already exists") from real faults, so the UI only
/// offers "Report problem" for the latter.
///
/// Every error logs itself when it is created: Tauri has no central hook for
/// command results, and the caller location is more precise than a command name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CmdError {
    pub message: String,
    pub expected: bool,
}

impl CmdError {
    #[track_caller]
    pub fn expected(message: impl Into<String>) -> Self {
        let message = message.into();
        let at = std::panic::Location::caller();
        log::info!(target: "cmd", "{message} ({}:{})", at.file(), at.line());
        Self { message, expected: true }
    }
}

impl From<String> for CmdError {
    #[track_caller]
    fn from(message: String) -> Self {
        let at = std::panic::Location::caller();
        log::error!(target: "cmd", "{message} ({}:{})", at.file(), at.line());
        Self { message, expected: false }
    }
}

impl From<&str> for CmdError {
    #[track_caller]
    fn from(message: &str) -> Self {
        Self::from(message.to_string())
    }
}

impl From<DbError> for CmdError {
    #[track_caller]
    fn from(e: DbError) -> Self {
        match e {
            DbError::Invalid(msg) => Self::expected(msg),
            other => Self::from(other.to_string()),
        }
    }
}

impl std::ops::Deref for CmdError {
    type Target = str;
    fn deref(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for CmdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl PartialEq<&str> for CmdError {
    fn eq(&self, other: &&str) -> bool {
        self.message == *other
    }
}

impl PartialEq<str> for CmdError {
    fn eq(&self, other: &str) -> bool {
        self.message == other
    }
}

impl PartialEq<String> for CmdError {
    fn eq(&self, other: &String) -> bool {
        &self.message == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_becomes_unexpected() {
        let e: CmdError = "boom".to_string().into();
        assert_eq!(e, CmdError { message: "boom".into(), expected: false });
    }

    #[test]
    fn expected_constructor_marks_expected() {
        assert!(CmdError::expected("Name existiert schon").expected);
    }

    #[test]
    fn db_invalid_is_expected_other_db_errors_are_not() {
        assert!(CmdError::from(DbError::Invalid("Name darf nicht leer sein".into())).expected);
        assert!(!CmdError::from(DbError::Other("kaputt".into())).expected);
    }

    #[test]
    fn serializes_message_and_expected() {
        let json = serde_json::to_string(&CmdError::expected("x")).unwrap();
        assert_eq!(json, r#"{"message":"x","expected":true}"#);
    }

    #[test]
    fn compares_and_derefs_like_a_string() {
        let e: CmdError = "Datei fehlt".into();
        assert_eq!(e, "Datei fehlt");
        assert!(e.contains("fehlt"));
        assert_eq!(format!("{e}"), "Datei fehlt");
    }
}
