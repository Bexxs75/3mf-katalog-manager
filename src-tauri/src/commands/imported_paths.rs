use std::path::Path;

pub(super) fn folder_paths(
    conn: &rusqlite::Connection,
) -> Result<Vec<String>, crate::db::error::DbError> {
    let mut stmt = conn.prepare("SELECT path FROM folders")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// Parse Windows prefixes as text so this policy is testable on every host. In
// particular, a verbatim drive prefix must be recognized before the UNC prefix.
fn windows_path(path: &str) -> Result<Option<(bool, Vec<String>)>, String> {
    let normalized = path.replace('/', "\\");
    let (unc, tail) = if let Some(tail) = normalized.strip_prefix(r"\\?\") {
        if tail
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("UNC\\"))
        {
            (true, &tail[4..])
        } else if is_drive(tail) {
            (false, tail)
        } else {
            return Err("Nicht erlaubtes Windows-Präfix".into());
        }
    } else if normalized.starts_with(r"\\.\") {
        return Err("Gerätepfade sind nicht erlaubt".into());
    } else if let Some(tail) = normalized.strip_prefix(r"\\") {
        (true, tail)
    } else if is_drive(&normalized) {
        (false, normalized.as_str())
    } else {
        return Ok(None);
    };
    let parts: Vec<String> = tail
        .split('\\')
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect();
    if (unc && parts.len() < 2)
        || parts.iter().enumerate().any(|(i, part)| {
            matches!(part.trim_end_matches(' '), "." | "..")
                || part.chars().any(char::is_control)
                || (part.contains(':') && (unc || i != 0))
        })
    {
        return Err("Unsicherer Windows-Pfad".into());
    }
    Ok(Some((unc, parts)))
}

fn is_drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\'
}

/// Pure lexical policy: unknown network paths must never reach filesystem calls.
pub(super) fn validate_path(path: &str, known_folders: &[String]) -> Result<(), String> {
    if let Some((unc, parts)) = windows_path(path)? {
        if !unc || known_folders.iter().any(|folder| {
            matches!(windows_path(folder), Ok(Some((true, root))) if parts.starts_with(&root))
        }) {
            return Ok(());
        }
        return Err("Netzwerkpfad liegt nicht in einem bekannten Katalogordner".into());
    }
    // On Windows a rooted path without a drive still depends on process state.
    if cfg!(windows) || path.starts_with('\\') || !Path::new(path).is_absolute() {
        return Err("Importierter Pfad muss absolut sein".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_prefixes_and_trusted_unc_roots_are_checked_without_io() {
        let known = vec![r"\\nas\share".into(), r"\\h\s".into()];
        for path in [r"C:\x", r"\\?\C:\x"] {
            assert!(validate_path(path, &[]).is_ok(), "{path}");
        }
        for path in [
            r"\\nas\share",
            r"\\NAS\share\x",
            r"\\?\UNC\h\s\x",
            "//nas/share/x",
        ] {
            assert!(validate_path(path, &[]).is_err(), "{path}");
            assert!(validate_path(path, &known).is_ok(), "{path}");
        }
        for path in [
            r"\\192.0.2.10\probe\x",
            r"\\nas\share-other\x",
            r"\\nas\share\..\secret",
            r"\\nas\share\.. \secret",
            r"\\?\GLOBALROOT\Device\x",
            r"\\.\C:\x",
            r"C:x",
        ] {
            assert!(validate_path(path, &known).is_err(), "{path}");
        }
        assert!(validate_path(r"\\h\s\x", &[r"\\?\UNC\h\s".into()]).is_ok());
    }
}
