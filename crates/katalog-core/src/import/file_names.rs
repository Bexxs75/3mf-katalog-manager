//! Name rules for files and folders the desktop app creates or renames on disk.
use std::path::Path;

use super::{ImportError as CmdError, ImportResult as CmdResult};
use crate::error_code::ErrorCode;

/// Settings key of the catalog base folder.
pub const SETTING_CATALOG_BASE_DIR: &str = "catalog_base_dir";

// Windows forbids these in file names. They are rejected on every OS so a
// catalog set up on Linux or macOS keeps working after a move to Windows.
pub const FORBIDDEN_DIR_NAME_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

// Device names Windows refuses as file or folder names, also with an extension.
// Windows also treats the superscript digits ¹²³ like 1, 2, 3 here.
pub const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "COM¹", "COM²", "COM³", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "LPT¹",
    "LPT²", "LPT³",
];

// Most file systems allow at most 255 bytes per name component. Checked up front
// so the user gets a clear message instead of an OS error.
pub const MAX_DIR_NAME_BYTES: usize = 255;

/// Whether a name is meant for a folder or a file; the wording differs per kind,
/// so each kind has its own codes (the same ones the desktop folder checks use).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    Folder,
    File,
}

pub fn validate_name(name: &str, limit_length: bool, kind: NameKind) -> CmdResult<()> {
    let code = |folder: ErrorCode, file: ErrorCode| match kind {
        NameKind::Folder => folder,
        NameKind::File => file,
    };
    if name.trim().is_empty() {
        return Err(CmdError::coded(code(ErrorCode::FolderNameEmpty, ErrorCode::FileNameEmpty)));
    }
    if let Some(c) = name.chars().find(|c| FORBIDDEN_DIR_NAME_CHARS.contains(c) || c.is_control()) {
        let shown = if c.is_control() { format!("U+{:04X}", c as u32) } else { c.to_string() };
        return Err(CmdError::coded_with(code(ErrorCode::FolderNameForbiddenChar, ErrorCode::FileNameForbiddenChar), [shown]));
    }
    if limit_length && name.len() > MAX_DIR_NAME_BYTES {
        return Err(CmdError::coded(code(ErrorCode::FolderNameTooLong, ErrorCode::FileNameTooLong)));
    }
    let not_allowed = || CmdError::coded(code(ErrorCode::FolderNameNotAllowed, ErrorCode::FileNameNotAllowed));
    if name == "." || name == ".." {
        return Err(not_allowed());
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(CmdError::coded(code(ErrorCode::FolderNameEndsWithDotOrSpace, ErrorCode::FileNameEndsWithDotOrSpace)));
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_uppercase();
    if WINDOWS_RESERVED_NAMES.contains(&stem.as_str()) {
        return Err(CmdError::coded(code(ErrorCode::FolderNameWindowsReserved, ErrorCode::FileNameWindowsReserved)));
    }
    // Defence in depth: whatever slipped through above must still be a single
    // plain component, never a root, prefix or parent reference.
    let mut components = Path::new(name).components();
    if !matches!((components.next(), components.next()), (Some(std::path::Component::Normal(_)), None)) {
        return Err(not_allowed());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(name: &str, kind: NameKind) -> (String, Vec<String>) {
        let e = validate_name(name, true, kind).unwrap_err();
        assert!(e.expected, "{name:?}");
        (e.message, e.params)
    }

    #[test]
    fn every_refusal_names_the_code_of_its_kind_and_no_wording() {
        let long = "a".repeat(MAX_DIR_NAME_BYTES + 1);
        for (name, folder, file, params) in [
            ("  ", "folderNameEmpty", "fileNameEmpty", vec![]),
            ("a/b", "folderNameForbiddenChar", "fileNameForbiddenChar", vec!["/".to_string()]),
            ("a\u{7}b", "folderNameForbiddenChar", "fileNameForbiddenChar", vec!["U+0007".to_string()]),
            (long.as_str(), "folderNameTooLong", "fileNameTooLong", vec![]),
            ("..", "folderNameNotAllowed", "fileNameNotAllowed", vec![]),
            ("a.", "folderNameEndsWithDotOrSpace", "fileNameEndsWithDotOrSpace", vec![]),
            ("con.txt", "folderNameWindowsReserved", "fileNameWindowsReserved", vec![]),
        ] {
            assert_eq!(refused(name, NameKind::Folder), (folder.to_string(), params.clone()), "{name:?}");
            assert_eq!(refused(name, NameKind::File), (file.to_string(), params), "{name:?}");
        }
        assert!(validate_name("Deckel v2.stl", true, NameKind::File).is_ok());
    }
}
