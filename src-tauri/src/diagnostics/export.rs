//! Picks the part of the log files that goes into a bug report.
use std::path::{Path, PathBuf};

use crate::diagnostics::HEADER_MARK;

/// Keeps the export small enough to sit next to a screenshot in one e-mail.
pub const EXPORT_LIMIT: usize = 1_000_000;

pub fn collect_tail(logs_oldest_first: &[String], limit: usize) -> String {
    let all = logs_oldest_first.concat();
    if all.len() <= limit {
        return all;
    }
    let mut cut = all.len() - limit;
    while !all.is_char_boundary(cut) {
        cut += 1;
    }
    let start = all[cut..].find('\n').map(|i| cut + i + 1).unwrap_or(all.len());
    let tail = &all[start..];
    if tail.contains(HEADER_MARK) {
        return tail.to_string();
    }
    match all[..start].lines().rfind(|l| l.contains(HEADER_MARK)) {
        Some(header) => format!("{header}\n…\n{tail}"),
        None => tail.to_string(),
    }
}

pub fn contains_debug(text: &str) -> bool {
    text.lines().any(|l| l.get(20..25) == Some("DEBUG"))
}

/// Log files of the app, oldest first (rotated files keep a date in their name,
/// the modification time orders them reliably).
pub fn read_logs(dir: &Path) -> Vec<String> {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "log"))
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|(_, p)| std::fs::read(&p).ok())
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .collect()
}

pub fn free_file_name(dir: &Path, date: &str) -> PathBuf {
    let first = dir.join(format!("3mf-katalog-log-{date}.txt"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("3mf-katalog-log-{date}-{n}.txt")))
        .find(|p| !p.exists())
        .expect("unbounded range always yields a free name")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::HEADER_MARK;

    fn head() -> String {
        format!("2026-10-03 10:00:00 INFO  {HEADER_MARK} 0.15.0 (ohne STEP) · Linux · Schema 40 · 5 Modelle\n")
    }

    #[test]
    fn small_logs_are_returned_whole_and_in_order() {
        let logs = vec!["a\n".to_string(), head(), "b\n".to_string()];
        assert_eq!(collect_tail(&logs, 1000), format!("a\n{}b\n", head()));
    }

    #[test]
    fn long_logs_start_at_a_line_and_keep_the_last_header() {
        let body: String = (0..200).map(|i| format!("line {i:03}\n")).collect();
        let logs = vec![head(), body];
        let out = collect_tail(&logs, 100);
        assert!(out.starts_with(&head()), "{out}");
        assert!(out.contains("…\n"));
        assert!(out.ends_with("line 199\n"));
        assert!(!out.contains("line 000"));
    }

    #[test]
    fn header_is_not_duplicated_when_already_in_the_tail() {
        let logs = vec!["x\n".repeat(100), head(), "y\n".to_string()];
        let out = collect_tail(&logs, head().len() + 10);
        assert_eq!(out.matches(HEADER_MARK).count(), 1);
    }

    #[test]
    fn debug_detection() {
        assert!(contains_debug("2026-10-03 10:00:00 DEBUG [datei] x"));
        assert!(!contains_debug("2026-10-03 10:00:00 INFO  [datei] debug x"));
    }

    #[test]
    fn free_file_name_counts_up() {
        let dir = std::env::temp_dir().join(format!("3mf-free-name-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = free_file_name(&dir, "2026-10-03");
        assert_eq!(first.file_name().unwrap(), "3mf-katalog-log-2026-10-03.txt");
        std::fs::write(&first, "x").unwrap();
        assert_eq!(free_file_name(&dir, "2026-10-03").file_name().unwrap(), "3mf-katalog-log-2026-10-03-2.txt");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
