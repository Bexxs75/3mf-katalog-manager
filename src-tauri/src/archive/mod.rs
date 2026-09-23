//! Entpacken gepackter Modell-Downloads (zip, 7z, rar, tar + gz/bz2/xz/zst).
//!
//! Reine Dateisystem-Logik ohne Tauri- oder Datenbank-Bezug, damit alles
//! direkt unit-testbar ist. Die Tauri-Befehle dazu liegen in
//! `commands/archives.rs`.
//!
//! Aufbau:
//! - `formats`: pro Format "Eintraege auflisten" und "Eintraege streamen"
//! - `extract`: `Extractor`, die EINZIGE Stelle, die auf die Platte schreibt
//!   (Zip-Slip-Schutz, Symlink-Verbot, Byte-Budget, Aufraeumen)
//! - `paths`: Pfad- und Dateinamen-Bereinigung

mod extract;
mod formats;
mod origin;
mod paths;

pub use extract::{extract_archive, Extraction, ExtractStats};
pub use paths::{safe_folder_name, safe_relative_path, sanitize_component};

use std::fmt;
use std::path::Path;

/// Obergrenze fuer die entpackte Gesamtgroesse EINES Archivs. Wird in
/// `inspect` gegen die Header-Angaben und beim Schreiben zusaetzlich gegen
/// die tatsaechlich geschriebenen Bytes geprueft (Header koennen luegen).
pub const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Obergrenze fuer die Anzahl der Eintraege EINES Archivs.
pub const MAX_ENTRIES: usize = 10_000;

/// Dateitypen, die NIE entpackt werden, obwohl sonst "alles" entpackt wird:
/// ausfuehrbare Dateien und Skripte, Verknuepfungen, die schon beim
/// Anzeigen des Ordners eine Netzwerkanfrage ausloesen koennen
/// (`.lnk`/`.url`/`.scf`/`.library-ms`/`.searchconnector-ms` mit
/// UNC-Icon-Pfad -> NTLM-Hash-Abfluss), Datentraeger-Images und
/// Linux-/macOS-Starter. Modell-Downloads brauchen keine davon.
const BLOCKED_EXTENSIONS: &[&str] = &[
    "exe", "dll", "com", "scr", "pif", "cpl", "msi", "msp", "msc", "bat", "cmd", "ps1", "psm1",
    "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta", "reg", "inf", "lnk", "url", "scf",
    "library-ms", "searchconnector-ms", "appref-ms", "application", "xll", "jar", "iso", "img",
    "vhd", "vhdx", "sh", "bash", "zsh", "fish", "command", "desktop", "app",
];
/// Dateien, die der Datei-Manager beim blossen Oeffnen des Ordners
/// auswertet (Icon-/Autostart-Angaben).
const BLOCKED_FILE_NAMES: &[&str] = &["desktop.ini", "autorun.inf", ".directory"];

/// `true`, wenn ein (bereits bereinigter) relativer Pfad nicht entpackt
/// werden darf. Prueft JEDE Komponente, damit auch Inhalte eines
/// `Foo.app/`-Bundles gesperrt sind.
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

/// Mehrteilige Endungen stehen vor einteiligen, damit `.tar.gz` nicht nur
/// als `.gz` (nicht unterstuetzt) erkannt wird.
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

/// Endungen fuer den Filter des Datei-Auswahldialogs (nur die letzte
/// Endung, so wie ihn die Dialog-Plugins erwarten).
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

/// Vorgeschlagener Zielordner-Name: Dateiname ohne Archiv-Endung,
/// bereinigt. Faellt auf "Archiv" zurueck, wenn nichts uebrig bleibt.
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
    /// Symlink, Hardlink, Geraetedatei, Anti-Item … - wird nie angelegt.
    Other,
}

#[derive(Debug, Clone)]
pub struct EntryMeta {
    /// Roher Name aus dem Archiv - NICHT vertrauenswuerdig.
    pub name: String,
    /// Groesse laut Header - NICHT vertrauenswuerdig.
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

/// Listet die Eintraege (hoechstens `MAX_ENTRIES + 1`, damit ein
/// Archiv mit Millionen Mini-Eintraegen nicht den Speicher fuellt).
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

/// Prueft ein Archiv, OHNE etwas zu schreiben. `is_model` entscheidet, welche
/// Eintraege als katalogisierbare Modelle zaehlen (vom Aufrufer uebergeben,
/// damit dieses Modul die Import-Regeln nicht kennen muss).
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
