//! Rules for companion files that the desktop app and the web file operations
//! share: which files are accepted, how the folder next to a model is named, and
//! the limits. One implementation, so both sides agree on which files belong to
//! which model.

use std::path::{Path, PathBuf};

/// Largest single companion file.
pub const MAX_COMPANION_BYTES: u64 = 50 * 1024 * 1024;
/// Most companion files one model can have (trashed ones do not count).
pub const MAX_COMPANIONS_PER_MODEL: i64 = 100;
/// How much of a file is inspected to decide whether its content matches its type.
pub const CONTENT_CHECK_BYTES: usize = 64 * 1024;
/// Appended to the model name (without extension) to name the companion folder.
pub const EXTRAS_SUFFIX: &str = "_extras";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompanionKind {
    Image,
    Pdf,
    Text,
}

impl CompanionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Pdf => "pdf",
            Self::Text => "text",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "image" => Some(Self::Image),
            "pdf" => Some(Self::Pdf),
            "text" => Some(Self::Text),
            _ => None,
        }
    }
}

/// Why a file is not accepted as a companion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The extension is not on the allowlist (or there is none).
    Type,
    /// The content does not match the extension, or is not what the type promises.
    Content,
}

pub fn lowercase_extension(name: &str) -> Option<String> {
    Path::new(name).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase)
}

pub fn kind_for_extension(extension: &str) -> Option<CompanionKind> {
    match extension {
        "png" | "jpg" | "jpeg" | "gif" | "webp" => Some(CompanionKind::Image),
        "pdf" => Some(CompanionKind::Pdf),
        "txt" | "md" => Some(CompanionKind::Text),
        _ => None,
    }
}

/// Text must be free of NUL bytes and valid UTF-8. A multi-byte character cut at the
/// end of an inspected block is not an error.
fn text_content_ok(head: &[u8]) -> bool {
    if head.contains(&0) {
        return false;
    }
    match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(error) => error.error_len().is_none() && head.len() >= CONTENT_CHECK_BYTES,
    }
}

/// The extension decides which signature the content must carry. A PNG named
/// `.jpg` is refused too: the types must agree.
fn content_matches(extension: &str, head: &[u8]) -> bool {
    match extension {
        "png" => head.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => head.starts_with(b"\xff\xd8\xff"),
        "gif" => head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a"),
        "webp" => head.starts_with(b"RIFF") && head.get(8..12) == Some(&b"WEBP"[..]),
        "pdf" => head.starts_with(b"%PDF-"),
        "txt" | "md" => text_content_ok(head),
        _ => false,
    }
}

/// Decides by extension and by the first bytes of the content. `head` is the start of
/// the file, at most [`CONTENT_CHECK_BYTES`] long.
pub fn classify(name: &str, head: &[u8]) -> Result<CompanionKind, Refusal> {
    let extension = lowercase_extension(name).ok_or(Refusal::Type)?;
    let kind = kind_for_extension(&extension).ok_or(Refusal::Type)?;
    if content_matches(&extension, head) {
        Ok(kind)
    } else {
        Err(Refusal::Content)
    }
}

/// The folder next to a model that holds its companions. It is derived from the
/// file stem, so `Foo.3mf` and `Foo.stl` share `Foo_extras`.
pub fn extras_dir_for(model_path: &Path) -> Option<PathBuf> {
    let mut name = model_path.file_stem()?.to_os_string();
    name.push(EXTRAS_SUFFIX);
    Some(model_path.parent()?.join(name))
}

/// True for a path that is the folder of some model's companions (by name only).
pub fn is_extras_dir_name(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.len() > EXTRAS_SUFFIX.len() && n.ends_with(EXTRAS_SUFFIX))
}

pub fn has_parent_component(path: &Path) -> bool {
    path.components().any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Structural check of a stored row. A restored backup could name any path; a companion
/// file must sit in a `_extras` folder, carry the name and allowed kind it claims, and
/// not be reached through `..`.
pub fn validate_stored_fields(name: &str, path: &str, kind: &str) -> Result<CompanionKind, &'static str> {
    let path = Path::new(path);
    let kind = CompanionKind::parse(kind).ok_or("kind")?;
    let ext_kind = lowercase_extension(name).and_then(|e| kind_for_extension(&e));
    if ext_kind != Some(kind) {
        return Err("type");
    }
    if path.file_name().and_then(|n| n.to_str()) != Some(name) {
        return Err("name");
    }
    if !path.parent().is_some_and(is_extras_dir_name) || !path.is_absolute() || has_parent_component(path) {
        return Err("location");
    }
    Ok(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_with_the_same_stem_share_one_extras_folder() {
        let a = extras_dir_for(Path::new("/c/Foo.3mf")).unwrap();
        let b = extras_dir_for(Path::new("/c/Foo.stl")).unwrap();
        assert_eq!(a, Path::new("/c/Foo_extras"));
        assert_eq!(a, b);
        assert_eq!(extras_dir_for(Path::new("/c/Foo.gcode.3mf")).unwrap(), Path::new("/c/Foo.gcode_extras"));
        assert!(extras_dir_for(Path::new("/")).is_none());
    }

    #[test]
    fn extras_folder_names_are_recognised_by_suffix_only() {
        assert!(is_extras_dir_name(Path::new("/c/Foo_extras")));
        assert!(!is_extras_dir_name(Path::new("/c/_extras")), "a bare suffix names no model");
        assert!(!is_extras_dir_name(Path::new("/c/Foo")));
    }

    #[test]
    fn stored_fields_must_describe_a_real_companion() {
        assert_eq!(validate_stored_fields("a.png", "/c/Foo_extras/a.png", "image"), Ok(CompanionKind::Image));
        assert_eq!(validate_stored_fields("a.png", "/c/Foo_extras/a.png", "pdf"), Err("type"));
        assert_eq!(validate_stored_fields("a.png", "/c/Foo_extras/b.png", "image"), Err("name"));
        assert_eq!(validate_stored_fields("a.png", "/c/Foo/a.png", "image"), Err("location"));
        assert_eq!(validate_stored_fields("a.png", "/c/x_extras/../a.png", "image"), Err("location"));
    }

    #[test]
    fn content_must_agree_with_the_extension() {
        assert_eq!(classify("a.png", b"\x89PNG\r\n\x1a\n"), Ok(CompanionKind::Image));
        assert_eq!(classify("a.jpg", b"\x89PNG\r\n\x1a\n"), Err(Refusal::Content));
        assert_eq!(classify("a.exe", b"MZ"), Err(Refusal::Type));
    }
}
