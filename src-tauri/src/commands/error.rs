use serde::Serialize;

use crate::db::error::DbError;

use super::error_code::ErrorCode;

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
    /// Values for the placeholders of the translated text (names, paths, numbers).
    /// Never words of a language: the frontend owns all wording.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
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
        Self { message, expected: false, code: None, params: Vec::new() }
    }

    pub fn with_code(mut self, code: GeometryErrorCode) -> Self {
        self.code = Some(code);
        self
    }

    /// An expected error identified by a registered code; the frontend turns it into
    /// text in the chosen language.
    #[track_caller]
    pub fn coded(code: ErrorCode) -> Self {
        Self::coded_with(code, [] as [String; 0])
    }

    /// Like `coded`, with the values the translated text needs. `params` hold data
    /// only (names, paths, numbers), never wording.
    #[track_caller]
    pub fn coded_with<I, S>(code: ErrorCode, params: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: ToString,
    {
        let params: Vec<String> = params.into_iter().map(|p| p.to_string()).collect();
        let at = std::panic::Location::caller();
        log::info!(target: "cmd", "{} ({}:{})", Self::coded_log_text(code, &params), at.file(), at.line());
        Self { message: code.as_str().to_string(), expected: true, code: None, params }
    }

    /// Log text of a coded error. The params are written as they are, never with
    /// `{:?}`: Debug doubles Windows backslashes, and the log anonymizer then no
    /// longer recognizes the home and catalog paths in them.
    fn coded_log_text(code: ErrorCode, params: &[String]) -> String {
        if params.is_empty() { code.as_str().to_string() } else { format!("{}: {}", code.as_str(), params.join(" | ")) }
    }

    /// Test helper: whether this is exactly `code` with exactly these params.
    #[cfg(test)]
    pub(crate) fn is(&self, code: ErrorCode, params: &[&str]) -> bool {
        self.expected && self.message == code.as_str() && self.params == params
    }

    #[track_caller]
    pub fn expected(message: impl Into<String>) -> Self {
        let message = message.into();
        let at = std::panic::Location::caller();
        log::info!(target: "cmd", "{message} ({}:{})", at.file(), at.line());
        Self { message, expected: true, code: None, params: Vec::new() }
    }
}

impl From<String> for CmdError {
    #[track_caller]
    fn from(message: String) -> Self {
        let at = std::panic::Location::caller();
        log::log!(target: "cmd", Self::input_log_level(false), "{message} ({}:{})", at.file(), at.line());
        Self { message, expected: false, code: None, params: Vec::new() }
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
        assert_eq!(e, CmdError { message: "boom".into(), expected: false, code: None, params: Vec::new() });
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
    fn coded_error_carries_code_and_params() {
        let error = CmdError::coded_with(ErrorCode::FolderNameForbiddenChar, [":"]);
        assert!(error.is(ErrorCode::FolderNameForbiddenChar, &[":"]));
        assert_eq!(serde_json::to_value(&error).unwrap(), serde_json::json!({
            "message": "folderNameForbiddenChar", "expected": true, "params": [":"]
        }));
        assert_eq!(serde_json::to_string(&CmdError::coded(ErrorCode::FolderNameEmpty)).unwrap(),
            r#"{"message":"folderNameEmpty","expected":true}"#);
    }

    #[test]
    fn coded_log_text_keeps_paths_recognizable_for_the_anonymizer() {
        use crate::diagnostics::anonymize::{anonymize, to_text, Context};
        let params = vec![
            r"C:\Users\Anna\Models\a.3mf".to_string(),
            r"C:\PrivateClient\Catalog\Deko".to_string(),
            r"\\?\D:\PrivateClient\Katalog\Deko".to_string(),
        ];
        let line = CmdError::coded_log_text(ErrorCode::FolderExists, &params);
        assert!(line.contains(r"C:\PrivateClient\Catalog"), "{line}");
        let ctx = Context {
            home: Some(r"C:\Users\Anna".into()),
            user: Some("Anna".into()),
            catalog_roots: vec![r"C:\PrivateClient\Catalog".into(), r"D:\PrivateClient\Katalog".into()],
            ..Context::default()
        };
        let out = to_text(&anonymize(&line, &ctx).unwrap());
        assert!(!out.contains("PrivateClient"), "{out}");
        assert!(!out.contains("Anna"), "{out}");
        // The Debug form is what broke this: the doubled backslashes defeat the match.
        let debug = format!("{params:?}");
        assert!(to_text(&anonymize(&debug, &ctx).unwrap()).contains("PrivateClient"), "premise: Debug output is not anonymized");
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
