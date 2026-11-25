//! Rename and move of a catalog folder (issue #20, package P6): checks (name, protected paths,
//! catalog base folder, cycles, the real locations), the folder on disk, the catalog update in
//! one transaction and both roll-back branches. The desktop command keeps the gate and the
//! lock; this module has no `fsops`/`upload_fs` dependency, so it builds on Windows and macOS.
//!
//! All disk access goes through [`FolderMoveFs`], which the desktop implements on top of its
//! file-system boundary (`Fs`, P0a). Tests use a fake that fails in step n.
//!
//! Known weakness, kept as it was: the checks and the rename are separate steps, so another
//! program can still win the race in between (Forgejo #81).
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::db::{self, error::DbError, models::FolderRecord};
use crate::error_code::ErrorCode;
use crate::file_rename::{self, FileRenameFs, StdFileRenameFs};
use crate::import::file_names::SETTING_CATALOG_BASE_DIR;
use crate::import::placement::{
    comparison_path, expand_sensitive_dirs, reject_if_sensitive_path_expanded, resolve_path_for_sensitivity_check,
};
use crate::import::{ImportError as CmdError, ImportResult as CmdResult};

/// The disk operations of the folder flow: the rename helpers of the file rename (collision
/// check, case-only rename through an intermediate name) plus two lookups.
pub trait FolderMoveFs: FileRenameFs {
    /// Same as `Path::is_dir` (follows links, any error is `false`).
    fn is_dir(&self, path: &Path) -> bool;
    /// Same as `Path::exists`.
    fn exists(&self, path: &Path) -> bool;
}

/// `std::fs`, 1:1.
pub struct StdFolderMoveFs;

impl FileRenameFs for StdFolderMoveFs {
    fn symlink_metadata(&self, path: &Path) -> io::Result<std::fs::Metadata> {
        StdFileRenameFs.symlink_metadata(path)
    }
    fn read_dir_names(&self, dir: &Path) -> io::Result<Vec<std::ffi::OsString>> {
        StdFileRenameFs.read_dir_names(dir)
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        StdFileRenameFs.canonicalize(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        StdFileRenameFs.rename(from, to)
    }
    fn move_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        StdFileRenameFs.move_file(from, to)
    }
}

impl FolderMoveFs for StdFolderMoveFs {
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

/// `name` ends up in `with_file_name` and must be a single harmless path component.
fn validate_folder_name(name: &str) -> CmdResult<()> {
    crate::folder_create::validate_folder_name(name)
}

/// Relocating an ancestor of the catalog base folder also invalidates the persisted import
/// destination.
pub(crate) fn reject_catalog_root_relocation(conn: &Connection, path: &Path) -> CmdResult<()> {
    let Some(base) = db::printer_link::get_setting(conn, SETTING_CATALOG_BASE_DIR)? else {
        return Ok(());
    };
    let base = resolve_path_for_sensitivity_check(Path::new(&base))?;
    let source = resolve_path_for_sensitivity_check(path)?;
    if comparison_path(&base).starts_with(comparison_path(&source)) {
        return Err(CmdError::coded(ErrorCode::CatalogRootProtected));
    }
    Ok(())
}

/// A managed path must neither lie in nor contain a protected directory. Otherwise e.g. the
/// home directory (ancestor of ~/.ssh) or `/` could be a target. Same checks and texts as the
/// desktop's `archives::check_managed_location`.
pub fn check_managed_location(path: &Path, expanded_sensitive: &[PathBuf]) -> CmdResult<()> {
    reject_if_sensitive_path_expanded(path, expanded_sensitive)?;
    let resolved = resolve_path_for_sensitivity_check(path)?;
    match expanded_sensitive.iter().find(|dir| **dir != resolved && dir.starts_with(&resolved)) {
        Some(dir) => Err(format!(
            "Zielpfad enthaelt ein geschuetztes Verzeichnis ({}) und wird abgelehnt",
            dir.display()
        )
        .into()),
        None => Ok(()),
    }
}

fn find_folder(folders: &[FolderRecord], id: i64) -> CmdResult<FolderRecord> {
    folders.iter().find(|f| f.id == id).cloned().ok_or_else(|| CmdError::from("folder not found"))
}

/// Renames a real folder on disk and updates `folders.name` plus, recursively,
/// `folders.path`/`files.path`/`companion_files.path` for the folder and everything below it.
/// A failed catalog update puts the folder back; if that fails too, both errors are named.
pub fn rename_folder(conn: &Connection, fs: &dyn FolderMoveFs, id: i64, name: String, sensitive_dirs: &[PathBuf]) -> CmdResult<()> {
    validate_folder_name(&name)?;
    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let folder = find_folder(&folders, id)?;

    let old_path = PathBuf::from(&folder.path);
    reject_catalog_root_relocation(conn, &old_path)?;
    let new_path = old_path.with_file_name(&name);
    let expanded = expand_sensitive_dirs(sensitive_dirs);
    check_managed_location(&old_path, &expanded)?;
    check_managed_location(&new_path, &expanded)?;

    if new_path == old_path {
        return Ok(());
    }
    if file_rename::target_collides(fs, &old_path, &new_path).map_err(|e| e.to_string())? {
        return Err(CmdError::coded_with(ErrorCode::RenameTargetFolderExists, [new_path.display()]));
    }
    file_rename::rename_on_disk(fs, &old_path, &new_path).map_err(|e| e.to_string())?;

    let db_result = (|| -> Result<(), DbError> {
        let tx = conn.unchecked_transaction()?;
        db::rename_folder_name(&tx, id, &name)?;
        db::update_paths_under_folder(&tx, id, &folder.path, &new_path.to_string_lossy())?;
        tx.commit().map_err(DbError::from)
    })();

    if let Err(db_err) = db_result {
        if let Err(rollback_err) = file_rename::rename_on_disk(fs, &new_path, &old_path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Ordner-Umbenennung fehlgeschlagen ({rollback_err}) - Ordner heisst jetzt {}, DB verweist teilweise noch auf {}",
                new_path.display(),
                old_path.display()
            )
            .into());
        }
        return Err(db_err.to_string().into());
    }
    Ok(())
}

/// Whether `candidate_id` is a (direct or indirect) descendant of `ancestor_id`.
pub fn is_descendant(folders: &[FolderRecord], candidate_id: i64, ancestor_id: i64) -> bool {
    let mut current = candidate_id;
    while let Some(f) = folders.iter().find(|f| f.id == current) {
        match f.parent_id {
            Some(pid) if pid == ancestor_id => return true,
            Some(pid) => current = pid,
            None => return false,
        }
    }
    false
}

/// Moves a folder on disk below another parent folder and updates `parent_id` and all paths
/// below. Rejects cycles (by id and by the real locations) and a missing target folder.
pub fn move_folder(conn: &Connection, fs: &dyn FolderMoveFs, id: i64, target: Option<i64>, sensitive_dirs: &[PathBuf]) -> CmdResult<()> {
    let Some(target) = target else {
        // There's no "move to root": the disk would stay unchanged, but the DB would
        // no longer see a parent.
        return Err("Ein Ordner kann nicht ohne Zielordner verschoben werden".into());
    };

    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let folder = find_folder(&folders, id)?;

    if target == id {
        return Err(CmdError::coded(ErrorCode::FolderMoveIntoSelf));
    }
    if is_descendant(&folders, target, id) {
        return Err(CmdError::coded(ErrorCode::FolderMoveIntoDescendant));
    }

    let new_parent_path = folders
        .iter()
        .find(|f| f.id == target)
        .map(|f| f.path.clone())
        .ok_or_else(|| CmdError::from("target folder not found"))?;
    let target = Some(target);

    let old_path = PathBuf::from(&folder.path);
    reject_catalog_root_relocation(conn, &old_path)?;
    let name = old_path.file_name().ok_or_else(|| CmdError::from("invalid folder path"))?;
    let new_path = PathBuf::from(&new_parent_path).join(name);
    let expanded = expand_sensitive_dirs(sensitive_dirs);
    check_managed_location(&old_path, &expanded)?;
    check_managed_location(&new_path, &expanded)?;

    // Without these checks a vanished folder would surface as a raw OS error.
    if !fs.is_dir(&old_path) {
        return Err(CmdError::coded_with(ErrorCode::FolderMissingOnDisk, [old_path.display()]));
    }
    let new_parent = Path::new(&new_parent_path);
    if !fs.is_dir(new_parent) {
        return Err(CmdError::coded_with(ErrorCode::TargetFolderMissingOnDisk, [new_parent.display()]));
    }
    // The ids can disagree with the disk (outdated catalog, symlinks, differing
    // case on case-insensitive volumes), so the real locations are compared too.
    if let (Ok(source), Ok(parent)) = (fs.canonicalize(&old_path), fs.canonicalize(new_parent)) {
        if comparison_path(&parent).starts_with(comparison_path(&source)) {
            return Err(CmdError::coded(ErrorCode::FolderMoveIntoSelfOrDescendant));
        }
        if old_path
            .parent()
            .and_then(|p| fs.canonicalize(p).ok())
            .is_some_and(|current| comparison_path(&current) == comparison_path(&parent))
        {
            return Err(CmdError::coded(ErrorCode::FolderMoveAlreadyThere));
        }
    }
    if fs.exists(&new_path) {
        return Err(CmdError::coded_with(ErrorCode::FolderMoveNameTaken, [new_path.display()]));
    }
    fs.rename(&old_path, &new_path).map_err(|e| e.to_string())?;

    // Parent and paths in one transaction, so the DB is never half-updated while
    // the disk has already moved. unchecked_transaction because we only have
    // `&Connection`.
    let db_result = (|| -> Result<(), DbError> {
        let tx = conn.unchecked_transaction()?;
        db::set_folder_parent(&tx, id, target)?;
        db::update_paths_under_folder(&tx, id, &folder.path, &new_path.to_string_lossy())?;
        tx.commit().map_err(DbError::from)
    })();

    if let Err(db_err) = db_result {
        if let Err(rollback_err) = fs.rename(&new_path, &old_path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback des Ordner-Moves fehlgeschlagen ({rollback_err}) - Ordner liegt jetzt unter {}, DB verweist teilweise noch auf {}",
                new_path.display(),
                old_path.display()
            )
            .into());
        }
        return Err(db_err.to_string().into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
