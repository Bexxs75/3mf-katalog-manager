//! Herkunftsmarkierung ("aus dem Internet") vom Archiv auf die entpackten
//! Dateien uebertragen. Browser markieren Downloads - Windows mit dem
//! Alternate Data Stream `Zone.Identifier` (Mark of the Web), macOS mit dem
//! Attribut `com.apple.quarantine`. Ohne Uebertragung verlieren die
//! entpackten Dateien diese Markierung, und Schutzmechanismen wie
//! SmartScreen, Office-Makroschutz oder Gatekeeper greifen nicht mehr.
//! Linux kennt keine vergleichbare Markierung.

use std::path::Path;

/// Liest die Markierung des Archivs; `None`, wenn keine vorhanden ist.
#[cfg(windows)]
pub(super) fn read(archive: &Path) -> Option<Vec<u8>> {
    let mut stream = archive.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    std::fs::read(std::path::PathBuf::from(stream)).ok()
}

/// Schreibt die Markierung auf eine entpackte Datei. Fehler werden bewusst
/// ignoriert (z.B. FAT32-Ziel ohne Alternate Data Streams).
#[cfg(windows)]
pub(super) fn apply(target: &Path, mark: &[u8]) {
    let mut stream = target.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let _ = std::fs::write(std::path::PathBuf::from(stream), mark);
}

#[cfg(target_os = "macos")]
const QUARANTINE_ATTR: &str = "com.apple.quarantine";

#[cfg(target_os = "macos")]
pub(super) fn read(archive: &Path) -> Option<Vec<u8>> {
    xattr::get(archive, QUARANTINE_ATTR).ok().flatten()
}

#[cfg(target_os = "macos")]
pub(super) fn apply(target: &Path, mark: &[u8]) {
    let _ = xattr::set(target, QUARANTINE_ATTR, mark);
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(super) fn read(_archive: &Path) -> Option<Vec<u8>> {
    None
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(super) fn apply(_target: &Path, _mark: &[u8]) {}
