//! Name rules for files and folders the desktop app creates or renames on disk.
use std::path::Path;

use super::{ImportError as CmdError, ImportResult as CmdResult};

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

pub fn validate_name(name: &str, limit_length: bool, kind: &str) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Err(CmdError::expected(format!("Der {kind} darf nicht leer sein")));
    }
    if let Some(c) = name.chars().find(|c| FORBIDDEN_DIR_NAME_CHARS.contains(c) || c.is_control()) {
        let shown = if c.is_control() { format!("U+{:04X}", c as u32) } else { c.to_string() };
        return Err(CmdError::expected(format!("Dieses Zeichen ist in {kind}n nicht erlaubt: {shown}")));
    }
    if limit_length && name.len() > MAX_DIR_NAME_BYTES {
        return Err(CmdError::expected(format!("Der {kind} ist zu lang")));
    }
    if name == "." || name == ".." {
        return Err(CmdError::expected(format!("Dieser {kind} ist nicht erlaubt")));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(CmdError::expected(format!("{kind}n dürfen nicht mit einem Punkt oder Leerzeichen enden")));
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_uppercase();
    if WINDOWS_RESERVED_NAMES.contains(&stem.as_str()) {
        return Err(CmdError::expected(format!("Dieser {kind} ist unter Windows reserviert")));
    }
    // Defence in depth: whatever slipped through above must still be a single
    // plain component, never a root, prefix or parent reference.
    let mut components = Path::new(name).components();
    if !matches!((components.next(), components.next()), (Some(std::path::Component::Normal(_)), None)) {
        return Err(CmdError::expected(format!("Dieser {kind} ist nicht erlaubt")));
    }
    Ok(())
}
