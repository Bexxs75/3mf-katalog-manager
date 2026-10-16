mod disk_sync;
pub use disk_sync::*;
mod runtime_environment;
pub use runtime_environment::*;
mod catalog_removal;
pub use catalog_removal::*;
// Shared types and helpers of the Tauri commands; the commands themselves live
// in the submodules by topic and are re-exported here.
mod archives;
mod backup;
mod imported_paths;
mod collections;
mod diagnostics;
mod dropped_image;
mod error;
mod external_env;
mod files;
mod file_modified_backfill;
pub(crate) use file_modified_backfill::backfill_file_modified_at;
mod import_jobs;
pub use import_jobs::*;
mod filament;
mod folders;
mod rename;
mod printer_link;
mod printers;
mod security;
mod slicers;
mod trash;
mod updater;

pub use archives::*;
pub use backup::*;
pub use collections::*;
pub use diagnostics::*;
pub use dropped_image::*;
pub use error::*;
pub use files::*;
pub use filament::*;
pub use folders::*;
pub use printer_link::*;
pub use printers::*;
pub use security::*;
pub use slicers::*;
pub use trash::*;
pub use updater::*;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::db::error::DbError;
use crate::db::models::{FileType, MaterialRecord, NewFile, ScannedMetadataUpdate};
use crate::db::{self, models::FileRecord};
use crate::geometry::RenderMesh;
use crate::slicers::detect_slicers;
use crate::tagging::{self, TaggingContext};
use crate::{obj, stl, threemf};

pub struct AppState {
    pub import_jobs: ImportJobs,
    pub db: Mutex<Connection>,
    pub trash_dir: std::path::PathBuf,
    pub db_path: std::path::PathBuf,
    /// Directories that must never be written to (computed once at startup). An
    /// imported backup can point `folders.path`/`files.path` anywhere, e.g. to
    /// autostart folders.
    pub sensitive_dirs: Vec<std::path::PathBuf>,
}
pub(crate) type CmdResult<T> = Result<T, CmdError>;
#[allow(unused_imports)] // Preserve the shared DTO API for desktop callers.
pub use katalog_core::read::{ModelFileDto, MaterialDto, FilamentUsageDto, PlateFilamentUsageDto, SliceInfoDto, CostEstimateDto, FileSummaryDto, FileImagesDto, TagCountDto, PrintLogEntryDto, FolderDto, CollectionDto};
use katalog_core::read::to_dto;
pub(crate) fn lock_db<'a>(state: &'a State<AppState>) -> CmdResult<std::sync::MutexGuard<'a, Connection>> {
    state.db.lock().map_err(|_| "database lock poisoned".into())
}
/// Moves a file without ever overwriting an existing target.
///
/// Always copies with `OpenOptions::create_new(true)` (O_EXCL/CREATE_NEW):
/// `std::fs::rename` replaces an existing target on POSIX and Windows, a prior
/// `exists()` check would be a TOCTOU window, and rename-no-replace only exists
/// as three platform-specific syscalls. Not atomic as a whole: after a crash,
/// source and target can briefly both exist.
pub(crate) fn move_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    // Keep moved models private even when the source allowed broader access.
    let mut dst = crate::safe_file::create_new_private(to)?;

    let copy_result = (|| -> std::io::Result<()> {
        let mut src = std::fs::File::open(from)?;
        std::io::copy(&mut src, &mut dst)?;
        dst.sync_all()?;
        Ok(())
    })();

    if let Err(e) = copy_result {
        // A partially written target is always cleaned up.
        drop(dst);
        let _ = std::fs::remove_file(to);
        return Err(e);
    }
    drop(dst);

    if let Err(remove_err) = std::fs::remove_file(from) {
        let _ = std::fs::remove_file(to);
        return Err(remove_err);
    }
    Ok(())
}
/// Second barrier next to `validate_folder_name`: checks the resolved target
/// path against sensitive system directories (config, autostart, SSH/GPG, system
/// roots). Applies wherever a path from the DB goes to
/// `fs::rename`/`fs::create_dir`, because an imported backup can inject e.g.
/// `path = ~/.config/autostart`.
///
/// The RESOLVED path is compared (`resolve_path_for_sensitivity_check`),
/// otherwise `..` or symlinks could bypass the check.
pub(crate) fn reject_if_sensitive_path(path: &Path, sensitive_dirs: &[PathBuf]) -> CmdResult<()> {
    reject_if_sensitive_path_expanded(path, &expand_sensitive_dirs(sensitive_dirs))
}
/// Adds the canonical spelling of each protected folder (e.g. /bin -> /usr/bin).
/// Compute once when many paths are checked.
fn expand_sensitive_dirs(sensitive_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut expanded = Vec::with_capacity(sensitive_dirs.len());
    for dir in sensitive_dirs {
        if let Ok(canonical) = dir.canonicalize() {
            if canonical != *dir {
                expanded.push(canonical);
            }
        }
        expanded.push(dir.clone());
    }
    expanded
}
fn reject_if_sensitive_path_expanded(path: &Path, expanded_dirs: &[PathBuf]) -> CmdResult<()> {
    let resolved = resolve_path_for_sensitivity_check(path)?;
    for dir in expanded_dirs {
        if resolved == *dir || resolved.starts_with(dir) {
            return Err(format!(
                "Zielpfad liegt in einem geschuetzten Systemverzeichnis ({}) und wird abgelehnt",
                dir.display()
            )
            .into());
        }
    }
    Ok(())
}
/// Containment instead of a denylist: `path` must lie inside the trash. Otherwise
/// a backup could set `trash_path = ~/Documents/important.pdf` with an old
/// `deleted_at`, and `purge_expired_trash_on_startup` would delete the file on
/// the next start. Both sides are resolved (`..`, symlinks).
fn reject_if_outside_trash_dir(path: &Path, resolved_trash_dir: &Path) -> CmdResult<()> {
    let resolved = resolve_path_for_sensitivity_check(path)?;
    if resolved == resolved_trash_dir || !resolved.starts_with(resolved_trash_dir) {
        return Err(format!(
            "Papierkorb-Pfad liegt ausserhalb des Papierkorb-Verzeichnisses ({}) und wird abgelehnt",
            resolved_trash_dir.display()
        )
        .into());
    }
    Ok(())
}
/// Resolves a path far enough that the prefix comparison in
/// `reject_if_sensitive_path` can't be undermined by `..` components or symlinks.
/// The path need NOT exist - at many call sites it's a target still to be
/// created (new folder, move target, freshly chosen catalog base directory):
/// - if the path exists, `canonicalize` decides (resolves symlinks AND `..`);
/// - if not, every literal `..` component is rejected (it could lead out of the
///   checked subtree) and the longest existing ancestor is resolved instead,
///   with the remaining components appended literally.
fn resolve_path_for_sensitivity_check(path: &Path) -> CmdResult<PathBuf> {
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical);
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(
            "Pfad enthaelt \"..\"-Komponenten und wird abgelehnt".into(),
        );
    }

    let mut trailing: Vec<std::ffi::OsString> = Vec::new();
    let mut cursor = path;
    while let (Some(name), Some(parent)) = (cursor.file_name(), cursor.parent()) {
        trailing.push(name.to_os_string());
        if let Ok(canonical) = parent.canonicalize() {
            let mut resolved = canonical;
            for component in trailing.iter().rev() {
                resolved.push(component);
            }
            return Ok(resolved);
        }
        cursor = parent;
    }
    Ok(path.to_path_buf())
}
// Only allow http(s) links: the frontend renders the URL unchanged as <a href>,
// and a "javascript:"/"data:" value would be executed on click instead of
// navigated (close to CWE-79).
fn validate_source_url(url: Option<String>) -> CmdResult<Option<String>> {
    let Some(trimmed) = url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty()) else {
        return Ok(None);
    };
    if is_http_url(&trimmed) {
        Ok(Some(trimmed))
    } else {
        Err(CmdError::expected("source URL must start with http:// or https://"))
    }
}
fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

// Von Tests in mehreren Untermodulen gebraucht.
#[cfg(test)]
pub(crate) fn unique_test_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "{name}_{}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create test dir");
    dir
}

/// Minimal `FileRecord` for tests, remaining fields with placeholders.
#[cfg(test)]
pub(crate) fn sample_file_record(id: i64, content_hash: Option<&str>, imported_at: &str) -> FileRecord {
    FileRecord {
        id,
        name: format!("file-{id}.3mf"),
        path: format!("/tmp/file-{id}.3mf"),
        file_type: FileType::ThreeMf,
        folder_id: None,
        origin: "local".to_string(),
        sync_status: "local-only".to_string(),
        cloud_id: None,
        file_size_bytes: 1024,
        dimensions_mm: None,
        volume_cm3: None,
        object_count: None,
        thumbnail_png: None,
        imported_at: imported_at.to_string(),
        file_modified_at: None,
        materials: Vec::new(),
        metadata: BTreeMap::new(),
        tags: Vec::new(),
        print_status: "not_printed".to_string(),
        last_viewed_at: None,
        creator: None,
        content_hash: content_hash.map(|s| s.to_string()),
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
        license: None,
        queue_position: None,
        favorite: false,
        plate_count: None,
        slice_info_json: None,
        deleted_at: None,
        trash_path: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn moved_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = unique_test_dir("move_private");
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("source");
        let to = dir.join("target");
        std::fs::write(&from, b"private model").unwrap();
        std::fs::set_permissions(&from, std::fs::Permissions::from_mode(0o644)).unwrap();
        move_file(&from, &to).unwrap();
        assert_eq!(std::fs::metadata(&to).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read(&to).unwrap(), b"private model");
        assert!(!from.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn validate_source_url_accepts_http_and_https() {
        assert_eq!(
            validate_source_url(Some("https://example.com/model".to_string())).unwrap(),
            Some("https://example.com/model".to_string())
        );
        assert_eq!(
            validate_source_url(Some("http://example.com".to_string())).unwrap(),
            Some("http://example.com".to_string())
        );
    }
    #[test]
    fn validate_source_url_rejects_non_http_schemes() {
        assert!(validate_source_url(Some("javascript:alert(1)".to_string())).is_err());
        assert!(validate_source_url(Some("data:text/html,<script>".to_string())).is_err());
    }
    #[test]
    fn validate_source_url_treats_none_and_blank_as_clear() {
        assert_eq!(validate_source_url(None).unwrap(), None);
        assert_eq!(validate_source_url(Some("   ".to_string())).unwrap(), None);
    }

    #[test]
    fn reject_if_sensitive_path_rejects_exact_match_and_descendants_but_not_siblings() {
        // Deliberately no "/home/..." prefix: on macOS "/home" is an automounter
        // symlink and would be resolved while canonicalizing.
        let sensitive = vec![PathBuf::from("/nonexistent-3mf-test-root/.config")];
        assert!(reject_if_sensitive_path(Path::new("/nonexistent-3mf-test-root/.config"), &sensitive).is_err());
        assert!(reject_if_sensitive_path(Path::new("/nonexistent-3mf-test-root/.config/autostart"), &sensitive).is_err());
        assert!(reject_if_sensitive_path(Path::new("/nonexistent-3mf-test-root/.config-backup"), &sensitive).is_ok());
        assert!(reject_if_sensitive_path(Path::new("/nonexistent-3mf-test-root/3D-Drucke"), &sensitive).is_ok());
    }
    #[test]
    fn reject_if_sensitive_path_rejects_parent_dir_traversal_into_sensitive_dir() {
        // Without resolving, the prefix comparison would say "no match" although the
        // path really lands in the protected directory.
        let base = unique_test_dir("reject_sensitive_traversal");
        let sensitive = base.join(".config");
        std::fs::create_dir_all(&sensitive).unwrap();
        let catalog = base.join("Modelle");
        std::fs::create_dir_all(&catalog).unwrap();

        let escaping = catalog.join("../.config/autostart");
        let result = reject_if_sensitive_path(&escaping, std::slice::from_ref(&sensitive));
        let inside = reject_if_sensitive_path(&catalog.join("Unterordner"), &[sensitive]);

        let _ = std::fs::remove_dir_all(&base);
        assert!(result.is_err(), "a path traversing into a sensitive dir must be rejected");
        assert!(inside.is_ok(), "a regular new subfolder of the catalog must stay allowed: {inside:?}");
    }
    #[test]
    fn reject_if_sensitive_path_rejects_symlink_into_sensitive_dir() {
        #[cfg(unix)]
        {
            let base = unique_test_dir("reject_sensitive_symlink");
            let sensitive = base.join(".config");
            std::fs::create_dir_all(&sensitive).unwrap();
            let link = base.join("harmlos");
            std::os::unix::fs::symlink(&sensitive, &link).unwrap();

            let result = reject_if_sensitive_path(&link, &[sensitive]);

            let _ = std::fs::remove_dir_all(&base);
            assert!(result.is_err(), "a symlink pointing into a sensitive dir must be rejected");
        }
    }
    #[test]
    fn reject_if_sensitive_path_allows_a_not_yet_existing_fresh_catalog_dir() {
        // Most important non-regression case of canonicalization: the catalog base
        // directory chosen during setup doesn't exist yet.
        let base = unique_test_dir("reject_sensitive_fresh");
        let fresh = base.join("Neuer Katalog").join("Unterordner");
        let result = reject_if_sensitive_path(&fresh, &[std::env::temp_dir().join("3mf-nichts-davon")]);
        let _ = std::fs::remove_dir_all(&base);
        assert!(result.is_ok(), "a fresh, not yet created catalog dir must remain a valid choice: {result:?}");
    }

    #[test]
    fn move_file_refuses_to_overwrite_an_existing_destination() {
        let dir = unique_test_dir("move_file_no_clobber");
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("source.3mf");
        let to = dir.join("dest.3mf");
        std::fs::write(&from, b"SOURCE-CONTENT").unwrap();
        std::fs::write(&to, b"EXISTING-DEST-CONTENT").unwrap();

        let result = move_file(&from, &to);

        assert!(result.is_err(), "move_file must refuse to overwrite an existing destination");
        assert_eq!(
            std::fs::read(&to).unwrap(),
            b"EXISTING-DEST-CONTENT",
            "destination content must be unchanged after a refused move"
        );
        assert!(from.exists(), "source must still exist after a refused move");
    }
    #[test]
    fn move_file_cleans_up_a_partially_written_destination_on_copy_failure() {
        // A partial copy that failed during io::copy()/sync_all() must always be cleaned up.
        let dir = unique_test_dir("move_file_partial_copy_cleanup");
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("source.3mf");
        std::fs::write(&from, vec![0u8; 10 * 1024 * 1024]).unwrap(); // 10 MB
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&from, std::fs::Permissions::from_mode(0o000)).unwrap();
            let to = dir.join("dest.3mf");
            let result = move_file(&from, &to);
            // Reset permissions so the unique_test_dir cleanup (if any) isn't blocked.
            std::fs::set_permissions(&from, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(result.is_err());
            assert!(!to.exists(), "partially written destination must be cleaned up on copy failure");
        }
    }

}
