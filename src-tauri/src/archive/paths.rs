use std::path::PathBuf;

/// Unter Windows reservierte Geraetenamen - gelten dort auch mit Endung
/// (`CON.txt`). Wird auf allen Plattformen angewendet, damit ein Katalog
/// beim Umzug auf Windows gueltig bleibt.
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    // Windows behandelt auch die hochgestellten Ziffern als Geraetenamen.
    "COM\u{b9}", "COM\u{b2}", "COM\u{b3}", "LPT\u{b9}", "LPT\u{b2}", "LPT\u{b3}",
];

/// Unsichtbare Unicode-Formatzeichen (Zero-Width, Bidi-Steuerzeichen wie
/// U+202E "Right-to-Left Override"). Damit laesst sich eine Endung
/// verschleiern ("rechnung\u{202E}lts.exe" wird als "rechnungexe.stl"
/// angezeigt) - `char::is_control` erfasst diese Zeichen nicht.
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

/// Bereinigt EINE Pfadkomponente: Steuerzeichen und `<>:"|?*/\` werden zu
/// `_`, abschliessende Punkte/Leerzeichen entfallen (Windows verwirft sie
/// stillschweigend), reservierte Geraetenamen bekommen ein `_` vorangestellt.
/// Kann einen leeren String liefern (z.B. fuer "..." oder " ").
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
    let stem = clean.split('.').next().unwrap_or("").to_ascii_uppercase();
    if RESERVED_NAMES.contains(&stem.as_str()) {
        clean.insert(0, '_');
    }
    clean
}

/// Name fuer den Zielordner eines Archivs: wie `sanitize_component`, aber
/// ohne fuehrende Punkte. Sonst wuerde "`.local.zip`" beim Zusammenfuehren
/// im Home-Verzeichnis zu `~/.local` - und ein Eintrag
/// `share/applications/x.desktop` landete im Anwendungsmenue.
pub fn safe_folder_name(name: &str) -> String {
    sanitize_component(name).trim_start_matches('.').to_string()
}

/// Wandelt einen (nicht vertrauenswuerdigen) Eintragsnamen aus einem Archiv
/// in einen sicheren RELATIVEN Pfad um. `None` bedeutet: Eintrag
/// ueberspringen (Zip-Slip-Versuch, absoluter Pfad, Laufwerksbuchstabe,
/// UNC-Pfad, leerer Name).
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
