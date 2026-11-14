use std::path::PathBuf;

/// Device names reserved on Windows - they apply there with an extension too
/// (`CON.txt`). Applied on all platforms, so a catalog stays valid when moved to
/// Windows.
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "COM0", "LPT0",
    "CONIN$", "CONOUT$",
    // Windows also treats the superscript digits as device names.
    "COM\u{b9}", "COM\u{b2}", "COM\u{b3}", "LPT\u{b9}", "LPT\u{b2}", "LPT\u{b3}",
];

/// Invisible Unicode format characters (zero-width, bidi controls like U+202E
/// "Right-to-Left Override"). They can disguise an extension
/// ("invoice\u{202E}lts.exe" is displayed as "invoiceexe.stl") -
/// `char::is_control` doesn't catch them.
fn is_invisible_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
    )
}

/// Sanitizes ONE path component: control characters and `<>:"|?*/\` become `_`,
/// trailing dots/spaces are dropped (Windows silently discards them), reserved
/// device names get a leading `_`. Can return an empty string (e.g. for "..." or " ").
pub fn sanitize_component(part: &str) -> String {
    let mut clean: String = part
        .chars()
        .map(|c| {
            if c.is_control()
                || is_invisible_format_char(c)
                || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\')
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    while clean.ends_with('.') || clean.ends_with(' ') {
        clean.pop();
    }
    // Windows ignores trailing spaces/dots of the stem ("CON .txt" = CON).
    let stem = clean
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    if RESERVED_NAMES.contains(&stem.as_str()) {
        clean.insert(0, '_');
    }
    clean
}

/// Name for an archive's target folder: like `sanitize_component`, but without
/// leading dots. Otherwise "`.local.zip`" merged into the home directory would
/// become `~/.local` - and an entry `share/applications/x.desktop` would end up in
/// the application menu.
pub fn safe_folder_name(name: &str) -> String {
    sanitize_component(name).trim_start_matches('.').to_string()
}

/// Turns an (untrusted) entry name from an archive into a safe RELATIVE path.
/// `None` means: skip the entry (zip slip attempt, absolute path, drive letter,
/// UNC path, empty name).
pub fn safe_relative_path(name: &str) -> Option<PathBuf> {
    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/') {
        return None;
    }
    let mut out = PathBuf::new();
    for (index, part) in normalized.split('/').enumerate() {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return None;
        }
        let bytes = part.as_bytes();
        if index == 0 && bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return None;
        }
        let clean = sanitize_component(part);
        if clean.is_empty() {
            return None;
        }
        out.push(clean);
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}
