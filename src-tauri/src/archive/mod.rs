//! Extracting packed model downloads (zip, 7z, rar, tar + gz/bz2/xz/zst).
//!
//! Pure file system logic without Tauri or database dependencies, so everything
//! is directly unit-testable. The Tauri commands live in `commands/archives.rs`.
//!
//! Structure:
//! - `formats`: per format "list entries" and "stream entries"
//! - `extract`: `Extractor`, the ONLY place that writes to disk
//!   (zip-slip protection, no symlinks, byte budget, cleanup)
//! - `paths`: path and file name sanitizing

mod extract;
mod formats;
mod origin;
mod paths;

pub use extract::extract_archive;
pub use paths::{safe_folder_name, safe_relative_path};

use std::fmt;
use std::path::Path;

/// Limit for the total unpacked size of ONE archive. Checked in `inspect` against
/// the header values and again while writing against the bytes actually written
/// (headers can lie).
pub const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Limit for the number of entries of ONE archive.
pub const MAX_ENTRIES: usize = 10_000;

/// File types that are NEVER extracted, although otherwise "everything" is:
/// executables and scripts, shortcuts that can trigger a network request just by
/// displaying the folder (`.lnk`/`.url`/`.scf`/`.library-ms`/`.searchconnector-ms`
/// with a UNC icon path -> NTLM hash leak), disk images and Linux/macOS
/// launchers. Model downloads need none of these.
const BLOCKED_EXTENSIONS: &[&str] = &[
    "exe", "dll", "com", "scr", "pif", "cpl", "msi", "msp", "msc", "bat", "cmd", "ps1", "psm1",
    "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta", "reg", "inf", "lnk", "url", "scf",
    "library-ms", "searchconnector-ms", "appref-ms", "application", "xll", "jar", "iso", "img",
    "vhd", "vhdx", "sh", "bash", "zsh", "fish", "command", "desktop", "app",
];
/// Files the file manager evaluates just by opening the folder (icon/autostart settings).
const BLOCKED_FILE_NAMES: &[&str] = &["desktop.ini", "autorun.inf", ".directory"];

/// `true` if an (already sanitized) relative path must not be extracted. Checks
/// EVERY component, so the contents of a `Foo.app/` bundle are blocked too.
pub fn is_blocked_path(rel: &Path) -> bool {
    rel.components().any(|component| {
        let name = component.as_os_str().to_string_lossy().to_lowercase();
        if BLOCKED_FILE_NAMES.contains(&name.as_str()) {
            return true;
        }
        Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| BLOCKED_EXTENSIONS.contains(&e))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
    Rar,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
    TarZst,
}

/// Multi-part extensions come before single ones, so `.tar.gz` isn't just recognized as `.gz` (unsupported).
const SUFFIXES: &[(&str, ArchiveFormat)] = &[
    (".tar.gz", ArchiveFormat::TarGz),
    (".tar.bz2", ArchiveFormat::TarBz2),
    (".tar.xz", ArchiveFormat::TarXz),
    (".tar.zst", ArchiveFormat::TarZst),
    (".tgz", ArchiveFormat::TarGz),
    (".tbz2", ArchiveFormat::TarBz2),
    (".txz", ArchiveFormat::TarXz),
    (".tzst", ArchiveFormat::TarZst),
    (".zip", ArchiveFormat::Zip),
    (".7z", ArchiveFormat::SevenZ),
    (".rar", ArchiveFormat::Rar),
    (".tar", ArchiveFormat::Tar),
];

/// Extensions for the file picker filter (only the last extension, as the dialog plugins expect).
pub const DIALOG_EXTENSIONS: &[&str] = &[
    "zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz", "zst", "tzst",
];

fn lower_file_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_lowercase())
}

pub fn detect_format(path: &Path) -> Option<ArchiveFormat> {
    let name = lower_file_name(path)?;
    SUFFIXES
        .iter()
        .find(|(suffix, _)| name.len() > suffix.len() && name.ends_with(suffix))
        .map(|(_, format)| *format)
}

pub fn is_archive_path(path: &Path) -> bool {
    detect_format(path).is_some()
}

/// Suggested target folder name: file name without the archive extension,
/// sanitized. Falls back to "Archiv" if nothing is left.
pub fn folder_name_for(path: &Path) -> String {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let lower = file_name.to_lowercase();
    let stem = SUFFIXES
        .iter()
        .find(|(suffix, _)| lower.len() > suffix.len() && lower.ends_with(suffix))
        .map(|(suffix, _)| &file_name[..file_name.len() - suffix.len()])
        .unwrap_or(&file_name);
    let clean = safe_folder_name(stem);
    if clean.is_empty() {
        "Archiv".to_string()
    } else {
        clean
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    /// Symlink, hardlink, device file, anti-item ... - never created.
    Other,
}

#[derive(Debug, Clone)]
pub struct EntryMeta {
    /// Raw name from the archive - NOT trustworthy.
    pub name: String,
    /// Size from the header - NOT trustworthy.
    pub size: u64,
    pub kind: EntryKind,
    pub encrypted: bool,
}

#[derive(Debug)]
pub enum ArchiveError {
    Encrypted,
    Unsupported(String),
    Unreadable(String),
    LimitExceeded,
    TooManyEntries,
    Io(std::io::Error),
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArchiveError::Encrypted => write!(f, "Archiv ist verschluesselt"),
            ArchiveError::Unsupported(msg) => write!(f, "Archiv wird nicht unterstuetzt: {msg}"),
            ArchiveError::Unreadable(msg) => write!(f, "Archiv ist beschaedigt oder unlesbar: {msg}"),
            ArchiveError::LimitExceeded => write!(
                f,
                "Archiv ueberschreitet die Grenze von {} GB entpackt",
                MAX_UNPACKED_BYTES / (1024 * 1024 * 1024)
            ),
            ArchiveError::TooManyEntries => {
                write!(f, "Archiv hat mehr als {MAX_ENTRIES} Eintraege")
            }
            ArchiveError::Io(e) => write!(f, "Schreibfehler: {e}"),
        }
    }
}

impl From<std::io::Error> for ArchiveError {
    fn from(e: std::io::Error) -> Self {
        ArchiveError::Io(e)
    }
}

/// Lists the entries (at most `MAX_ENTRIES + 1`, so an archive with millions of
/// tiny entries doesn't fill up memory).
pub fn list_entries(path: &Path, format: ArchiveFormat) -> Result<Vec<EntryMeta>, ArchiveError> {
    formats::list(path, format)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectStatus {
    Ok,
    NoModels,
    TooLarge,
    Encrypted,
    Unreadable,
    Unsupported,
}

#[derive(Debug, Clone)]
pub struct InspectSummary {
    pub status: InspectStatus,
    pub model_count: u32,
    pub entry_count: u32,
    pub unpacked_size: u64,
}

/// Checks an archive WITHOUT writing anything. `is_model` decides which entries
/// count as catalogable models (passed in by the caller, so this module doesn't
/// need to know the import rules).
pub fn inspect(path: &Path, is_model: impl Fn(&Path) -> bool) -> InspectSummary {
    let empty = |status| InspectSummary {
        status,
        model_count: 0,
        entry_count: 0,
        unpacked_size: 0,
    };
    let Some(format) = detect_format(path) else {
        return empty(InspectStatus::Unsupported);
    };
    let entries = match list_entries(path, format) {
        Ok(entries) => entries,
        Err(ArchiveError::Encrypted) => return empty(InspectStatus::Encrypted),
        Err(ArchiveError::Unsupported(_)) => return empty(InspectStatus::Unsupported),
        Err(ArchiveError::LimitExceeded | ArchiveError::TooManyEntries) => {
            return empty(InspectStatus::TooLarge)
        }
        Err(_) => return empty(InspectStatus::Unreadable),
    };

    let files: Vec<&EntryMeta> = entries.iter().filter(|e| e.kind == EntryKind::File).collect();
    let unpacked_size = files.iter().fold(0u64, |sum, e| sum.saturating_add(e.size));
    let model_count = files
        .iter()
        .filter(|e| safe_relative_path(&e.name).is_some_and(|p| is_model(&p)))
        .count() as u32;
    let summary = |status| InspectSummary {
        status,
        model_count,
        entry_count: entries.len() as u32,
        unpacked_size,
    };

    if files.iter().any(|e| e.encrypted) {
        summary(InspectStatus::Encrypted)
    } else if entries.len() > MAX_ENTRIES || unpacked_size > MAX_UNPACKED_BYTES {
        summary(InspectStatus::TooLarge)
    } else if model_count == 0 {
        summary(InspectStatus::NoModels)
    } else {
        summary(InspectStatus::Ok)
    }
}

#[cfg(test)]
mod tests;
