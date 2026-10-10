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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<GeometryErrorCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GeometryErrorCode { NotFound, Unreadable, TooLarge, Unsupported }

impl CmdError {
    fn input_log_level(import: bool) -> log::Level {
        if import { log::Level::Warn } else { log::Level::Error }
    }

    fn import_log_message(message: &str, verbose: bool) -> String {
        if verbose { return message.into(); }
        use crate::diagnostics::anonymize::{anonymize, to_text, Context};
        anonymize(message, &Context { replace_file_names: true, ..Context::default() })
            .map(|segments| to_text(&segments))
            .unwrap_or_else(|_| "Importfehler (Anonymisierung fehlgeschlagen)".into())
    }

    pub fn import_input(message: impl Into<String>) -> Self {
        let message = message.into();
        // Parser errors can include archive entry names; normal logs follow the
        // same filename replacement rules as the diagnostics preview.
        let logged = Self::import_log_message(&message, log::max_level() >= log::LevelFilter::Debug);
        log::log!(target: "cmd", Self::input_log_level(true), "{logged}");
        Self { message, expected: false, code: None }
    }

    pub fn with_code(mut self, code: GeometryErrorCode) -> Self {
        self.code = Some(code);
        self
    }

    #[track_caller]
    pub fn expected(message: impl Into<String>) -> Self {
        let message = message.into();
        let at = std::panic::Location::caller();
        log::info!(target: "cmd", "{message} ({}:{})", at.file(), at.line());
        Self { message, expected: true, code: None }
    }
}

impl From<String> for CmdError {
    #[track_caller]
    fn from(message: String) -> Self {
        let at = std::panic::Location::caller();
        log::log!(target: "cmd", Self::input_log_level(false), "{message} ({}:{})", at.file(), at.line());
        Self { message, expected: false, code: None }
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
        assert_eq!(e, CmdError { message: "boom".into(), expected: false, code: None });
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
    fn optional_geometry_code_preserves_message_and_expected() {
        let error = CmdError::expected("missing").with_code(GeometryErrorCode::NotFound);
        assert_eq!(serde_json::to_value(error).unwrap(), serde_json::json!({
            "message": "missing", "expected": true, "code": "notFound"
        }));
    }

    #[test]
    fn compares_and_derefs_like_a_string() {
        let e: CmdError = "Datei fehlt".into();
        assert_eq!(e, "Datei fehlt");
        assert!(e.contains("fehlt"));
        assert_eq!(format!("{e}"), "Datei fehlt");
    }
}

#[cfg(test)]
#[test]
fn import_parse_errors_only_change_the_log_level() {
    assert_eq!(CmdError::input_log_level(true), log::Level::Warn);
    assert_eq!(CmdError::input_log_level(false), log::Level::Error);
    let message = "OBJ parse error: invalid face";
    let error = CmdError::import_input(message);
    assert_eq!(error.message, message);
    assert!(!error.expected);
}

#[cfg(test)]
#[test]
fn import_parser_details_replace_filenames_only_in_normal_logs() {
    let message = "3MF parse error: missing secret.model.3mf";
    assert_eq!(CmdError::import_log_message(message, true), message);
    assert!(!CmdError::import_log_message(message, false).contains("secret.model.3mf"));
}
