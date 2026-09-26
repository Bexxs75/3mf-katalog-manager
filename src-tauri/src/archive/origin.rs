//! Carries the origin mark ("from the internet") from the archive over to the
//! extracted files. Browsers mark downloads - Windows with the alternate data
//! stream `Zone.Identifier` (Mark of the Web), macOS with the
//! `com.apple.quarantine` attribute. Without carrying it over, the extracted files
//! lose this mark, and protections like SmartScreen, Office macro protection or
//! Gatekeeper no longer apply. Linux has no comparable mark.

use std::path::Path;

/// Reads the archive's mark; `None` if there is none.
#[cfg(windows)]
pub(super) fn read(archive: &Path) -> Option<Vec<u8>> {
    let mut stream = archive.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    std::fs::read(std::path::PathBuf::from(stream)).ok()
}

/// Writes the mark onto an extracted file. Errors are deliberately ignored
/// (e.g. a FAT32 target without alternate data streams).
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
