//! Rules for companion files that the desktop app and the web file operations
//! share: which files are accepted, how the folder next to a model is named, and
//! the limits. One implementation, so both sides agree on which files belong to
//! which model.

use std::path::{Path, PathBuf};

/// Largest single companion file.
pub const MAX_COMPANION_BYTES: u64 = 50 * 1024 * 1024;
/// The largest limit a server may configure for a single companion upload (4 GiB).
pub const MAX_COMPANION_BYTES_CONFIGURABLE: u64 = 4096 * 1024 * 1024;
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
    /// Stored and handed back, never opened, shown or run: CAD sources and documents.
    Other,
}

impl CompanionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Pdf => "pdf",
            Self::Text => "text",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "image" => Some(Self::Image),
            "pdf" => Some(Self::Pdf),
            "text" => Some(Self::Text),
            "other" => Some(Self::Other),
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
        "fcstd" | "scad" | "dxf" | "csv" | "docx" | "odt" => Some(CompanionKind::Other),
        _ => None,
    }
}

/// Every extension that is accepted, in the order the lists in the interface show them. The
/// frontend keeps a copy; a test compares both against `companion-extensions.json`.
pub const ALLOWED_EXTENSIONS: &[&str] =
    &["png", "jpg", "jpeg", "gif", "webp", "pdf", "txt", "md", "fcstd", "scad", "dxf", "csv", "docx", "odt"];

/// Whether the content of a file with this extension must be valid UTF-8 text without NUL
/// bytes from its first to its last byte.
pub fn is_text_extension(extension: &str) -> bool {
    matches!(extension, "txt" | "md" | "scad" | "dxf" | "csv")
}

/// Whether a file name carries an extension whose content is checked as text.
pub fn is_text_name(name: &str) -> bool {
    lowercase_extension(name).is_some_and(|e| is_text_extension(&e))
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
        // A ZIP container. Only the local file header is checked: the files are never
        // unpacked or opened here.
        "fcstd" | "docx" | "odt" => head.starts_with(b"PK\x03\x04"),
        e if is_text_extension(e) => text_content_ok(head),
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

/// Like [`classify`], but for a file that is held completely in memory: a text file
/// is checked from its first to its last byte, because anything in it that is not
/// valid UTF-8 (or a NUL byte past the inspected head) shows up later when the file is
/// opened as text.
pub fn classify_full(name: &str, bytes: &[u8]) -> Result<CompanionKind, Refusal> {
    let kind = classify(name, &bytes[..bytes.len().min(CONTENT_CHECK_BYTES)])?;
    if is_text_name(name) && (bytes.contains(&0) || std::str::from_utf8(bytes).is_err()) {
        return Err(Refusal::Content);
    }
    Ok(kind)
}

/// Checks UTF-8 and NUL over a stream, chunk by chunk; a character cut at a chunk's end
/// is completed by the next chunk.
#[derive(Default)]
pub struct TextScan {
    pending: Vec<u8>,
}

impl TextScan {
    pub fn feed(&mut self, chunk: &[u8]) -> bool {
        if chunk.contains(&0) {
            return false;
        }
        let joined;
        let data: &[u8] = if self.pending.is_empty() {
            chunk
        } else {
            joined = [self.pending.as_slice(), chunk].concat();
            &joined
        };
        let (rest_from, ok) = match std::str::from_utf8(data) {
            Ok(_) => (data.len(), true),
            Err(e) if e.error_len().is_none() => (e.valid_up_to(), true),
            Err(_) => (0, false),
        };
        if !ok {
            return false;
        }
        self.pending = data[rest_from..].to_vec();
        true
    }

    /// False when the stream ended in the middle of a character.
    pub fn finish(&self) -> bool {
        self.pending.is_empty()
    }
}

/// A harmless single-component name for a user-chosen file: forbidden and control
/// characters become `_`, the extension is lowercased, dots and blanks around the stem
/// go, an empty stem becomes `datei`, and room is left for a collision counter. `None`
/// when the name has no extension or no valid form.
pub fn companion_file_name(original: &str) -> Option<String> {
    use crate::import::file_names::{validate_name, FORBIDDEN_DIR_NAME_CHARS, MAX_DIR_NAME_BYTES};
    let cleaned: String =
        original.chars().map(|c| if c.is_control() || FORBIDDEN_DIR_NAME_CHARS.contains(&c) { '_' } else { c }).collect();
    let extension = lowercase_extension(&cleaned)?;
    let stem = &cleaned[..cleaned.len() - extension.len() - 1];
    let mut stem = stem.trim_matches(|c: char| c == '.' || c == ' ').to_string();
    if stem.is_empty() {
        stem = "datei".to_string();
    }
    // Leave room for a collision counter and the folder's own limit.
    while stem.len() + extension.len() + 8 > MAX_DIR_NAME_BYTES {
        stem.pop();
    }
    let full = format!("{stem}.{extension}");
    if validate_name(&full, true, crate::import::file_names::NameKind::File).is_ok() {
        return Some(full);
    }
    let prefixed = format!("_{full}");
    validate_name(&prefixed, true, crate::import::file_names::NameKind::File).is_ok().then_some(prefixed)
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
    fn a_text_file_is_checked_to_its_last_byte() {
        let mut long = vec![b'a'; CONTENT_CHECK_BYTES + 10];
        assert_eq!(classify_full("n.txt", &long), Ok(CompanionKind::Text));
        long[CONTENT_CHECK_BYTES + 5] = 0;
        assert_eq!(classify("n.txt", &long[..CONTENT_CHECK_BYTES]), Ok(CompanionKind::Text), "the head alone looks fine");
        assert_eq!(classify_full("n.txt", &long), Err(Refusal::Content));
        let mut bad = vec![b'a'; CONTENT_CHECK_BYTES + 10];
        bad[CONTENT_CHECK_BYTES + 3] = 0xff;
        assert_eq!(classify_full("n.md", &bad), Err(Refusal::Content));
        assert_eq!(classify_full("p.png", b"\x89PNG\r\n\x1a\n"), Ok(CompanionKind::Image));
    }

    #[test]
    fn a_text_scan_spans_chunks() {
        let euro = "\u{20ac}".as_bytes();
        let mut scan = TextScan::default();
        assert!(scan.feed(&euro[..1]) && scan.feed(&euro[1..2]) && scan.feed(&euro[2..]) && scan.finish());
        let mut cut = TextScan::default();
        assert!(cut.feed(&euro[..2]) && !cut.finish(), "ended inside a character");
        let mut bad = TextScan::default();
        assert!(bad.feed(b"ok") && !bad.feed(&[0xff]));
        assert!(!TextScan::default().feed(b"a\0"));
    }

    #[test]
    fn companion_names_are_one_harmless_component() {
        assert_eq!(companion_file_name("Bild.PNG").as_deref(), Some("Bild.png"));
        assert_eq!(companion_file_name("a/b:c.txt").as_deref(), Some("a_b_c.txt"));
        assert_eq!(companion_file_name("..png.md").as_deref(), Some("png.md"));
        assert_eq!(companion_file_name(".md"), None, "a bare dot file has no extension");
        assert_eq!(companion_file_name("CON.txt").as_deref(), Some("_CON.txt"));
        assert_eq!(companion_file_name("noextension"), None);
        let long = format!("{}.png", "x".repeat(400));
        assert!(companion_file_name(&long).unwrap().len() <= 247 + 4);
    }

    #[test]
    fn content_must_agree_with_the_extension() {
        assert_eq!(classify("a.png", b"\x89PNG\r\n\x1a\n"), Ok(CompanionKind::Image));
        assert_eq!(classify("a.jpg", b"\x89PNG\r\n\x1a\n"), Err(Refusal::Content));
        assert_eq!(classify("a.exe", b"MZ"), Err(Refusal::Type));
    }

    const ZIP: &[u8] = b"PK\x03\x04\x14\x00\x00\x00";

    #[test]
    fn the_formats_for_other_files_need_their_own_signature() {
        for name in ["m.fcstd", "d.docx", "d.odt", "M.FCStd"] {
            assert_eq!(classify(name, ZIP), Ok(CompanionKind::Other), "{name}");
            assert_eq!(classify(name, b"\x89PNG\r\n\x1a\n"), Err(Refusal::Content), "{name} with a PNG inside");
            assert_eq!(classify(name, b"plain text"), Err(Refusal::Content), "{name} with text inside");
            assert_eq!(classify(name, b""), Err(Refusal::Content), "{name} empty");
        }
        for name in ["part.scad", "plan.dxf", "table.csv"] {
            assert_eq!(classify(name, b"cube(10);\n"), Ok(CompanionKind::Other), "{name}");
            assert_eq!(classify(name, b"a\0b"), Err(Refusal::Content), "{name} with a NUL byte");
            assert_eq!(classify(name, ZIP), Err(Refusal::Content), "{name} with a ZIP inside");
            assert_eq!(classify(name, &[0xff, 0xfe, b'a']), Err(Refusal::Content), "{name} that is not UTF-8");
        }
        assert_eq!(classify("a.fcstd.exe", ZIP), Err(Refusal::Type), "only the last extension counts");
    }

    #[test]
    fn a_text_like_other_file_is_checked_to_its_last_byte() {
        let mut long = vec![b'a'; CONTENT_CHECK_BYTES + 10];
        long[CONTENT_CHECK_BYTES + 5] = 0;
        assert_eq!(classify_full("p.scad", &long), Err(Refusal::Content));
        assert!(is_text_name("T.CSV") && is_text_name("n.md") && !is_text_name("m.fcstd") && !is_text_name("p.png"));
        // A ZIP is binary on purpose: the NUL bytes of its header are no reason to refuse it.
        let mut zip = ZIP.to_vec();
        zip.extend_from_slice(&[0, 1, 2, 0, 0]);
        assert_eq!(classify_full("m.fcstd", &zip), Ok(CompanionKind::Other));
    }

    #[test]
    fn no_extension_of_the_blocklist_is_ever_accepted() {
        for extension in crate::archive::BLOCKED_EXTENSIONS {
            assert_eq!(kind_for_extension(extension), None, "{extension}");
            // Whatever the content claims, the type decides first.
            assert_eq!(classify(&format!("x.{extension}"), ZIP), Err(Refusal::Type), "{extension}");
            assert_eq!(classify(&format!("x.{extension}"), b"hello"), Err(Refusal::Type), "{extension}");
        }
        for extension in ALLOWED_EXTENSIONS {
            assert!(!crate::archive::is_blocked_path(Path::new(&format!("x.{extension}"))), "{extension} is on the blocklist");
        }
        for extension in ["step", "stp", "stl", "3mf", "obj", "svg", "zip", "7z", "rar", "html", "docm", "xlsm", "json"] {
            assert_eq!(kind_for_extension(extension), None, "{extension} must not be a companion");
        }
    }

    #[test]
    fn the_extension_lists_agree_with_each_other_and_with_the_shared_gold_list() {
        let gold: serde_json::Value = serde_json::from_str(include_str!("../../../src/lib/companionExtensions.json")).unwrap();
        let gold: std::collections::BTreeMap<String, String> = gold["extensions"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(extension, kind)| (extension.clone(), kind.as_str().unwrap().to_string()))
            .collect();
        let by_function: std::collections::BTreeMap<String, String> = ALLOWED_EXTENSIONS
            .iter()
            .map(|e| (e.to_string(), kind_for_extension(e).expect("listed extensions have a kind").as_str().to_string()))
            .collect();
        assert_eq!(by_function, gold);
        assert_eq!(ALLOWED_EXTENSIONS.len(), gold.len(), "no extension is listed twice");
        // Nothing outside the list maps to a kind.
        for probe in ["bmp", "tif", "rtf", "doc", "xlsx", "odp", "ods", "stp", "exe", ""] {
            assert_eq!(kind_for_extension(probe), None, "{probe}");
        }
    }

    #[test]
    fn the_limits_stay_where_they_were() {
        assert_eq!(MAX_COMPANION_BYTES, 50 * 1024 * 1024);
        assert_eq!(MAX_COMPANIONS_PER_MODEL, 100);
    }

    #[test]
    fn a_stored_other_row_must_have_an_allowed_extension() {
        let row = |name: &str, kind: &str| validate_stored_fields(name, &format!("/c/Foo_extras/{name}"), kind);
        assert_eq!(row("m.fcstd", "other"), Ok(CompanionKind::Other));
        assert_eq!(row("m.exe", "other"), Err("type"), "a foreign catalog may claim anything");
        assert_eq!(row("m.js", "other"), Err("type"));
        assert_eq!(row("m.step", "other"), Err("type"));
        assert_eq!(row("m.fcstd", "text"), Err("type"), "the kind must be the one the extension gives");
        assert_eq!(row("a.png", "other"), Err("type"));
        assert_eq!(CompanionKind::parse("other"), Some(CompanionKind::Other));
        assert_eq!(CompanionKind::parse("exe"), None);
    }
}
