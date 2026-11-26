//! The model trash family of the desktop catalog (issue #20, package P7): delete a model (single
//! and batch), restore it, delete it for good, empty the trash and the clean-up of entries older
//! than 7 days at startup. The desktop commands keep the locks, the `State` and the dialogs; this
//! module has no `fsops` / `upload_fs` dependency, so it builds on Windows and macOS.
//!
//! The trash format of the desktop does not change: a model goes to `<trash dir>/<id>-<name>`,
//! the row keeps `deleted_at` and `trash_path` (see `docs/plans/issue20-p7-zuschnitt.md`, plan §3.1),
//! and the web's reader `fsops::desktop_trash` keeps reading it. Companion files follow the model
//! through [`crate::companion_actions`].
//!
//! All disk access goes through [`ModelTrashFs`], which the desktop implements on top of its
//! file-system boundary (`Fs`, P0a). Tests use a fake that fails in step n.
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::companion_actions::{self as actions, CompanionFs, StdCompanionFs};
use crate::db::{self, error::DbError};
use crate::error_code::ErrorCode;
use crate::folder_create::find_catalog_parent;
use crate::import::file_names::SETTING_CATALOG_BASE_DIR;
use crate::import::placement::{comparison_path, expand_sensitive_dirs, reject_if_sensitive_path_expanded, resolve_path_for_sensitivity_check};
use crate::import::{ImportError as CmdError, ImportResult as CmdResult};

/// Entries older than this many days are removed by the startup clean-up.
pub const TRASH_RETENTION_DAYS: i64 = 7;

/// The disk operations the model trash uses.
pub trait ModelTrashFs {
    /// Same as `std::fs::metadata`.
    fn metadata(&self, path: &Path) -> io::Result<Metadata>;
    /// Same as `Path::exists` (follows links, any error is `false`).
    fn exists(&self, path: &Path) -> bool;
    /// Same as `std::fs::create_dir_all`.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// Same as `std::fs::remove_dir` (only ever removes an empty directory).
    fn remove_dir(&self, path: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// A move that never replaces its target (hard link first, copy as fallback), see
    /// `import::placement::move_file`.
    fn move_file(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// The disk access of the companion files that follow the model.
    fn companions(&self) -> &dyn CompanionFs;
}

/// `std::fs`, 1:1.
pub struct StdModelTrashFs;

impl ModelTrashFs for StdModelTrashFs {
    fn metadata(&self, path: &Path) -> io::Result<Metadata> {
        std::fs::metadata(path)
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }
    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir(path)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
    fn move_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        crate::import::placement::move_file(from, to)
    }
    fn companions(&self) -> &dyn CompanionFs {
        &StdCompanionFs
    }
}

/// Soft-deletes a model whose file is unreachable (nothing to move).
pub fn delete_missing_file(conn: &Connection, id: i64) -> CmdResult<()> {
    let deleted_at = chrono::Utc::now().to_rfc3339();
    db::soft_delete_file(conn, id, None, &deleted_at).map_err(|e| e.to_string().into())
}

/// Moves a model with its companions into the trash. `file` is the already loaded record.
pub fn delete_file(fs: &dyn ModelTrashFs, conn: &Connection, file: &db::models::FileRecord, id: i64, trash_dir: &Path) -> CmdResult<()> {
    if file.deleted_at.is_some() {
        return Err(CmdError::coded(ErrorCode::FileInTrash));
    }
    match fs.metadata(Path::new(&file.path)) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // Path unreachable (renamed folder, unmounted drive): nothing to move, but
            // still only soft-delete, because the file is usually not really gone. Other
            // errors like PermissionDenied go through the error path below.
            let deleted_at = chrono::Utc::now().to_rfc3339();
            db::soft_delete_file(conn, id, None, &deleted_at).map_err(|e| CmdError::from(e.to_string()))?;
            actions::trash_with_model(fs.companions(), conn, id, trash_dir, &deleted_at);
            return Ok(());
        }
        Err(e) => return Err(e.to_string().into()),
        Ok(_) => {}
    }

    let trash_path = trash_dir.join(format!("{id}-{}", file.name));
    fs.move_file(Path::new(&file.path), &trash_path).map_err(|e| e.to_string())?;

    let deleted_at = chrono::Utc::now().to_rfc3339();
    if let Err(db_err) = db::soft_delete_file(conn, id, Some(&trash_path.to_string_lossy()), &deleted_at) {
        // Move the file back out of the trash if the DB couldn't mark it as deleted;
        // otherwise it would vanish for the user.
        if let Err(rollback_err) = fs.move_file(&trash_path, Path::new(&file.path)) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback aus dem Papierkorb fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}, DB fuehrt sie weiterhin als aktiv unter {}",
                trash_path.display(),
                file.path
            )
            .into());
        }
        return Err(db_err.to_string().into());
    }
    // The companions follow the model into the trash under the same timestamp, so
    // restoring the model finds exactly these. They cannot make the removal fail.
    actions::trash_with_model(fs.companions(), conn, id, trash_dir, &deleted_at);
    log::debug!(target: "datei", "in den Papierkorb: {}", file.path);
    Ok(())
}

/// The cleanup batch: single errors are logged and the batch goes on, so the frontend can
/// reload cleanly afterwards.
pub fn delete_files(fs: &dyn ModelTrashFs, conn: &Connection, trash_dir: &Path, file_ids: Vec<String>) -> CmdResult<()> {
    for file_id in file_ids {
        let id: i64 = match file_id.parse() {
            Ok(id) => id,
            Err(_) => {
                log::warn!(target: "cleanup", "skipped invalid file ID: {file_id}");
                continue;
            }
        };
        // Cleanup batch: a file deleted in the meantime (e.g. twice in the selection)
        // is skipped instead of aborting the whole batch.
        let file = match db::get_file(conn, id) {
            Ok(Some(file)) => file,
            Ok(None) => continue,
            Err(e) => {
                log::error!(target: "cleanup", "could not load file ID {id}: {e}");
                continue;
            }
        };
        // Same function as for single deletes, so the move-back compensation applies here too.
        if let Err(e) = delete_file(fs, conn, &file, id, trash_dir) {
            log::warn!(target: "cleanup", "übersprungen: {} ({e})", file.path);
        }
    }
    Ok(())
}

/// Brings a model back, and the companions that went to the trash together with it.
pub fn restore_file(fs: &dyn ModelTrashFs, conn: &Connection, file: &db::models::FileRecord, id: i64, sensitive_dirs: &[PathBuf]) -> CmdResult<()> {
    restore_model(fs, conn, file, id, sensitive_dirs)?;
    // Companions that went to the trash together with the model come back with it.
    if let Some(deleted_at) = &file.deleted_at {
        actions::restore_with_model(fs.companions(), conn, id, deleted_at, sensitive_dirs);
    }
    Ok(())
}

fn restore_model(fs: &dyn ModelTrashFs, conn: &Connection, file: &db::models::FileRecord, id: i64, sensitive_dirs: &[PathBuf]) -> CmdResult<()> {
    if file.deleted_at.is_none() {
        return Err("file is not in trash".into());
    }

    let Some(trash_path) = file.trash_path.clone() else {
        // Without trash_path the file was already unreachable when deleted: just make
        // the entry visible again.
        return db::restore_file(conn, id, None).map_err(|e| e.to_string().into());
    };

    let original = Path::new(&file.path);
    let target_path = if fs.exists(original) {
        let stem = original.file_stem().and_then(|s| s.to_str()).unwrap_or("datei");
        let ext = original.extension().and_then(|s| s.to_str());
        let parent = original.parent().unwrap_or_else(|| Path::new("."));
        let new_name = match ext {
            Some(ext) => format!("{stem} (wiederhergestellt).{ext}"),
            None => format!("{stem} (wiederhergestellt)"),
        };
        parent.join(new_name)
    } else {
        original.to_path_buf()
    };

    // A folder moved to the trash takes its directories with it, so the target
    // folder may be gone. Recreate it (only after the sensitive-path check, since
    // the stored path can come from an imported backup) and remember which
    // directories are new so a failed restore can undo them.
    let created_dirs = missing_parent_dirs(fs, &target_path);
    if !created_dirs.is_empty() {
        let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
        check_managed_location(parent, &expand_sensitive_dirs(sensitive_dirs))?;
        fs.create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let undo_dirs = || {
        // Deepest first; remove_dir only ever removes an empty directory.
        for dir in created_dirs.iter().rev() {
            let _ = fs.remove_dir(dir);
        }
    };

    if let Err(e) = fs.move_file(Path::new(&trash_path), &target_path) {
        undo_dirs();
        return Err(e.to_string().into());
    }

    let from_folder_trash = conn
        .query_row("SELECT 1 FROM folder_trash_origin WHERE file_id = ?1", [id], |_| Ok(()))
        .is_ok();
    let new_path_str = target_path.to_string_lossy().to_string();
    let new_path_arg = if new_path_str == file.path { None } else { Some(new_path_str.as_str()) };
    let db_result = (|| -> Result<(), DbError> {
        let tx = conn.unchecked_transaction()?;
        db::restore_file(&tx, id, new_path_arg)?;
        relink_restored_folder(&tx, id, target_path.parent(), created_dirs.first().map(|d| d.as_path()), from_folder_trash)?;
        tx.execute("DELETE FROM folder_trash_origin WHERE file_id = ?1", [id])?;
        tx.commit().map_err(DbError::from)
    })();
    if let Err(db_err) = db_result {
        // Back into the trash if the DB couldn't record the restore.
        let moved_back = fs.move_file(&target_path, Path::new(&trash_path));
        if moved_back.is_ok() {
            undo_dirs();
        }
        if let Err(rollback_err) = moved_back {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback in den Papierkorb fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}, DB fuehrt sie weiterhin als geloescht",
                target_path.display()
            )
            .into());
        }
        return Err(db_err.to_string().into());
    }
    log::debug!(target: "datei", "wiederhergestellt: {} -> {}", trash_path, target_path.display());
    Ok(())
}

/// Same checks as the desktop's `archives::check_managed_location` (text included): the
/// target must neither lie in nor contain a protected directory.
fn check_managed_location(target_dir: &Path, expanded_sensitive: &[PathBuf]) -> CmdResult<()> {
    reject_if_sensitive_path_expanded(target_dir, expanded_sensitive)?;
    let resolved = resolve_path_for_sensitivity_check(target_dir)?;
    match expanded_sensitive.iter().find(|dir| **dir != resolved && dir.starts_with(&resolved)) {
        Some(dir) => Err(format!("Zielpfad enthaelt ein geschuetztes Verzeichnis ({}) und wird abgelehnt", dir.display()).into()),
        None => Ok(()),
    }
}

/// Directories above `target` that do not exist yet, outermost first.
fn missing_parent_dirs(fs: &dyn ModelTrashFs, target: &Path) -> Vec<PathBuf> {
    let mut missing = Vec::new();
    let mut cursor = target.parent();
    while let Some(dir) = cursor {
        if dir.as_os_str().is_empty() || fs.exists(dir) {
            break;
        }
        missing.push(dir.to_path_buf());
        cursor = dir.parent();
    }
    missing.reverse();
    missing
}

/// Puts a restored file back into the catalog folder tree. When the directory was
/// recreated (`created_root` is the outermost new directory), the nearest cataloged ancestor (or the catalog base folder)
/// is the anchor and the missing folder rows are inserted below it - the same way the
/// disk sync attaches new folders. When the directory already existed, only a folder
/// row for exactly that directory is reused: a row that is missing then may have been
/// removed from the catalog on purpose, and a restore must not bring it back. A file
/// whose folder is not found stays as it is. When the recreated directory has no cataloged
/// ancestor and lies outside the catalog base folder (a top-level catalog folder that was
/// trashed), the outermost recreated directory becomes a new top-level folder row.
/// A model that was trashed together with its folder (`from_folder_trash`) also gets
/// its folder rows back when the directory stayed on disk because of a foreign file:
/// there the row was removed by the folder trash, not by the user. Only catalog rows
/// are written, below a cataloged ancestor or the catalog base folder.
fn relink_restored_folder(
    conn: &Connection,
    id: i64,
    dir: Option<&Path>,
    created_root: Option<&Path>,
    from_folder_trash: bool,
) -> Result<(), DbError> {
    let Some(dir) = dir else { return Ok(()) };
    let folders = db::list_folders(conn)?;
    let existing = folders
        .iter()
        .find(|f| comparison_path(Path::new(&f.path)) == comparison_path(dir))
        .map(|f| f.id);
    let folder_id = match existing {
        Some(folder_id) => folder_id,
        None if created_root.is_none() && !from_folder_trash => return Ok(()),
        None => {
            let anchor = match find_catalog_parent(dir, &folders) {
                Some(parent) => folders.iter().find(|f| f.id == parent).map(|f| PathBuf::from(&f.path)),
                None => db::printer_link::get_setting(conn, SETTING_CATALOG_BASE_DIR)?
                    .map(PathBuf::from)
                    .filter(|base| comparison_path(dir).starts_with(comparison_path(base))),
            };
            let Some(anchor) = anchor.or_else(|| created_root.map(PathBuf::from)) else { return Ok(()) };
            db::ensure_folder_path(conn, &anchor, dir)?
        }
    };
    conn.execute(
        "UPDATE files SET folder_id = ?1 WHERE id = ?2 AND folder_id IS NOT ?1",
        rusqlite::params![folder_id, id],
    )?;
    Ok(())
}

/// Removes the trash file of an entry (a file that is already gone is fine). An error means the
/// entry must stay.
fn remove_trash_file(fs: &dyn ModelTrashFs, trash_path: &str) -> io::Result<()> {
    match fs.remove_file(Path::new(trash_path)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Deletes a model in the trash for good: its file, the trash files of its companions and the row.
pub fn delete_file_permanently(fs: &dyn ModelTrashFs, conn: &Connection, id: i64, trash_dir: &Path) -> CmdResult<()> {
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    if file.deleted_at.is_none() {
        return Err("file is not in trash".into());
    }
    if let Some(trash_path) = &file.trash_path {
        remove_trash_file(fs, trash_path).map_err(|e| e.to_string())?;
    }
    // Its companions in the trash go with it; the rows follow by cascade.
    actions::purge_trash_files_of_model(fs.companions(), conn, id, trash_dir)?;
    db::delete_file(conn, id).map_err(|e| e.to_string().into())
}

/// Removes every entry of the trash. Single errors are logged and the entry stays.
pub fn empty_trash(fs: &dyn ModelTrashFs, conn: &Connection, trash_dir: &Path) -> CmdResult<()> {
    let files = db::list_trash(conn).map_err(|e| e.to_string())?;
    for file in files {
        if let Some(trash_path) = &file.trash_path {
            if let Err(e) = remove_trash_file(fs, trash_path) {
                log::error!(target: "trash", "removing failed for file ID {}: {e}", file.id);
                continue;
            }
        }
        if let Err(e) = actions::purge_trash_files_of_model(fs.companions(), conn, file.id, trash_dir) {
            log::error!(target: "trash", "removing companions failed for file ID {}: {e}", file.id);
            continue;
        }
        if let Err(e) = db::delete_file(conn, file.id) {
            log::error!(target: "trash", "could not delete DB row for file ID {}: {e}", file.id);
        }
    }
    // Companions removed on their own from a model that is still in the catalog.
    actions::purge_all_trashed(fs.companions(), conn, trash_dir);
    Ok(())
}

/// At startup: removes trash entries older than [`TRASH_RETENTION_DAYS`] days before `now`.
/// Single errors are logged and skipped.
pub fn purge_expired_trash(fs: &dyn ModelTrashFs, conn: &Connection, trash_dir: &Path, now: chrono::DateTime<chrono::Utc>) {
    let cutoff = (now - chrono::Duration::days(TRASH_RETENTION_DAYS)).to_rfc3339();
    let expired = match db::purge_expired_trash(conn, &cutoff) {
        Ok(files) => files,
        Err(e) => {
            log::error!(target: "startup", "trash cleanup: query failed: {e}");
            return;
        }
    };
    for file in expired {
        if let Some(trash_path) = &file.trash_path {
            if let Err(e) = remove_trash_file(fs, trash_path) {
                log::error!(target: "startup", "trash cleanup: file failed for ID {}: {e}", file.id);
                continue;
            }
        }
        if let Err(e) = actions::purge_trash_files_of_model(fs.companions(), conn, file.id, trash_dir) {
            log::error!(target: "startup", "trash cleanup: companions failed for ID {}: {e}", file.id);
            continue;
        }
        if let Err(e) = db::delete_file(conn, file.id) {
            log::error!(target: "startup", "trash cleanup: DB row failed for ID {}: {e}", file.id);
        }
    }
    actions::purge_expired_trashed(fs.companions(), conn, trash_dir, &cutoff);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unique_test_dir;
    use std::cell::{Cell, RefCell};
    use std::io::ErrorKind::{NotFound, PermissionDenied, Unsupported};

    struct Fault {
        op: &'static str,
        nth: usize,
        kind: io::ErrorKind,
        seen: Cell<usize>,
    }

    /// Real files in a temp dir; the n-th call of one operation fails.
    #[derive(Default)]
    struct FaultFs {
        faults: Vec<Fault>,
        calls: RefCell<Vec<(&'static str, PathBuf)>>,
    }

    impl FaultFs {
        fn fail(mut self, op: &'static str, nth: usize, kind: io::ErrorKind) -> Self {
            self.faults.push(Fault { op, nth, kind, seen: Cell::new(0) });
            self
        }
        fn step(&self, op: &'static str, path: &Path) -> io::Result<()> {
            self.calls.borrow_mut().push((op, path.to_path_buf()));
            let mut result = Ok(());
            for fault in self.faults.iter().filter(|f| f.op == op) {
                fault.seen.set(fault.seen.get() + 1);
                if fault.seen.get() == fault.nth && result.is_ok() {
                    result = Err(io::Error::new(fault.kind, format!("injected {op} #{}", fault.nth)));
                }
            }
            result
        }
        fn count(&self, op: &str) -> usize {
            self.calls.borrow().iter().filter(|c| c.0 == op).count()
        }
    }

    impl CompanionFs for FaultFs {
        fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
            StdCompanionFs.symlink_metadata(path)
        }
        fn exists(&self, path: &Path) -> bool {
            StdCompanionFs.exists(path)
        }
        fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
            StdCompanionFs.canonicalize(path)
        }
        fn create_dir(&self, path: &Path) -> io::Result<()> {
            StdCompanionFs.create_dir(path)
        }
        fn remove_dir(&self, path: &Path) -> io::Result<()> {
            StdCompanionFs.remove_dir(path)
        }
        fn remove_file(&self, path: &Path) -> io::Result<()> {
            StdCompanionFs.remove_file(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            StdCompanionFs.rename(from, to)
        }
        fn create_new(&self, path: &Path) -> io::Result<std::fs::File> {
            StdCompanionFs.create_new(path)
        }
        fn move_file(&self, from: &Path, to: &Path) -> io::Result<()> {
            StdCompanionFs.move_file(from, to)
        }
    }

    impl ModelTrashFs for FaultFs {
        fn metadata(&self, path: &Path) -> io::Result<Metadata> {
            self.step("metadata", path)?;
            StdModelTrashFs.metadata(path)
        }
        fn exists(&self, path: &Path) -> bool {
            ModelTrashFs::exists(&StdModelTrashFs, path)
        }
        fn create_dir_all(&self, path: &Path) -> io::Result<()> {
            self.step("create_dir_all", path)?;
            StdModelTrashFs.create_dir_all(path)
        }
        fn remove_dir(&self, path: &Path) -> io::Result<()> {
            self.step("remove_dir", path)?;
            ModelTrashFs::remove_dir(&StdModelTrashFs, path)
        }
        fn remove_file(&self, path: &Path) -> io::Result<()> {
            self.step("remove_file", path)?;
            ModelTrashFs::remove_file(&StdModelTrashFs, path)
        }
        fn move_file(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.step("move_file", from)?;
            ModelTrashFs::move_file(&StdModelTrashFs, from, to)
        }
        fn companions(&self) -> &dyn CompanionFs {
            self
        }
    }

    struct Fx {
        base: PathBuf,
        trash: PathBuf,
        conn: Connection,
    }

    impl Drop for Fx {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    impl Fx {
        fn new(tag: &str) -> Self {
            let base = unique_test_dir(&format!("p7_core_{tag}"));
            let base = base.canonicalize().unwrap_or(base);
            let trash = base.join("trash");
            std::fs::create_dir_all(&trash).unwrap();
            Self { base, trash, conn: db::connect_in_memory().unwrap() }
        }
        fn model(&self, rel: &str) -> (i64, PathBuf) {
            let path = self.base.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"model").unwrap();
            (db::test_insert_minimal_file(&self.conn, &path.to_string_lossy(), None).unwrap(), path)
        }
        fn file(&self, id: i64) -> db::models::FileRecord {
            db::get_file(&self.conn, id).unwrap().unwrap()
        }
        /// Puts a model into the trash with the real file system.
        fn trashed(&self, rel: &str) -> (i64, PathBuf) {
            let (id, path) = self.model(rel);
            delete_file(&StdModelTrashFs, &self.conn, &self.file(id), id, &self.trash).unwrap();
            (id, path)
        }
        fn exists(&self, id: i64) -> bool {
            db::get_file(&self.conn, id).unwrap().is_some()
        }
    }

    #[test]
    fn a_metadata_permission_error_changes_nothing() {
        let f = Fx::new("meta");
        let (id, path) = f.model("m.3mf");
        let fs = FaultFs::default().fail("metadata", 1, PermissionDenied);
        assert!(delete_file(&fs, &f.conn, &f.file(id), id, &f.trash).is_err());
        assert!(path.exists());
        assert!(f.file(id).deleted_at.is_none());
        assert_eq!(fs.count("move_file"), 0);
    }

    #[test]
    fn an_unreachable_path_is_only_soft_deleted() {
        let f = Fx::new("soft");
        let (id, path) = f.model("m.3mf");
        std::fs::remove_file(path).unwrap();
        delete_file(&StdModelTrashFs, &f.conn, &f.file(id), id, &f.trash).unwrap();
        let file = f.file(id);
        assert!(file.deleted_at.is_some() && file.trash_path.is_none());
    }

    #[test]
    fn a_failed_move_leaves_the_model_active() {
        let f = Fx::new("move");
        let (id, path) = f.model("m.3mf");
        let fs = FaultFs::default().fail("move_file", 1, Unsupported);
        let error = delete_file(&fs, &f.conn, &f.file(id), id, &f.trash).unwrap_err();
        assert!(error.message.contains("injected move_file #1"));
        assert!(path.exists() && f.file(id).deleted_at.is_none());
    }

    #[test]
    fn a_db_failure_and_a_failed_move_back_are_both_reported() {
        let f = Fx::new("rollback");
        let (id, path) = f.model("m.3mf");
        f.conn
            .execute_batch(&format!(
                "CREATE TRIGGER block BEFORE UPDATE ON files WHEN NEW.id = {id} AND NEW.deleted_at IS NOT NULL
                 BEGIN SELECT RAISE(ABORT, 'blocked'); END;"
            ))
            .unwrap();
        let fs = FaultFs::default().fail("move_file", 2, PermissionDenied);
        let error = delete_file(&fs, &f.conn, &f.file(id), id, &f.trash).unwrap_err();
        assert!(error.message.contains("UND Rollback aus dem Papierkorb fehlgeschlagen"), "{error}");
        assert!(!path.exists());
        assert!(f.trash.join(format!("{id}-m.3mf")).exists(), "the file stays in the trash and the text says where");
        assert!(error.message.contains(&format!("{id}-m.3mf")));
    }

    #[test]
    fn a_restore_that_cannot_create_the_folder_moves_nothing() {
        let f = Fx::new("mkdir");
        let (id, path) = f.trashed("a/b/m.3mf");
        std::fs::remove_dir_all(f.base.join("a")).unwrap();
        let fs = FaultFs::default().fail("create_dir_all", 1, PermissionDenied);
        assert!(restore_file(&fs, &f.conn, &f.file(id), id, &[]).is_err());
        assert_eq!(fs.count("move_file"), 0);
        assert!(!path.exists() && f.file(id).deleted_at.is_some());
    }

    #[test]
    fn a_failed_restore_move_removes_the_directories_it_created_deepest_first() {
        let f = Fx::new("undo");
        let (id, path) = f.trashed("a/b/m.3mf");
        std::fs::remove_dir_all(f.base.join("a")).unwrap();
        let fs = FaultFs::default().fail("move_file", 1, PermissionDenied);
        assert!(restore_file(&fs, &f.conn, &f.file(id), id, &[]).is_err());
        let removed: Vec<PathBuf> = fs.calls.borrow().iter().filter(|c| c.0 == "remove_dir").map(|c| c.1.clone()).collect();
        assert_eq!(removed, vec![f.base.join("a/b"), f.base.join("a")]);
        assert!(!f.base.join("a").exists() && !path.exists());
    }

    #[test]
    fn a_failed_move_back_after_a_db_failure_keeps_the_new_directories() {
        let f = Fx::new("noundo");
        let (id, path) = f.trashed("a/m.3mf");
        std::fs::remove_dir_all(f.base.join("a")).unwrap();
        f.conn
            .execute_batch(
                "CREATE TRIGGER block BEFORE UPDATE ON files WHEN NEW.deleted_at IS NULL AND OLD.deleted_at IS NOT NULL
                 BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
            )
            .unwrap();
        let fs = FaultFs::default().fail("move_file", 2, PermissionDenied);
        let error = restore_file(&fs, &f.conn, &f.file(id), id, &[]).unwrap_err();
        assert!(error.message.contains("UND Rollback in den Papierkorb fehlgeschlagen"), "{error}");
        assert_eq!(fs.count("remove_dir"), 0);
        assert!(path.exists(), "the file is where the text says");
    }

    #[test]
    fn a_taken_name_gets_the_restored_suffix() {
        let f = Fx::new("name");
        let (id, path) = f.trashed("m.3mf");
        std::fs::write(&path, b"occupant").unwrap();
        restore_file(&StdModelTrashFs, &f.conn, &f.file(id), id, &[]).unwrap();
        assert!(f.base.join("m (wiederhergestellt).3mf").exists());
        assert_eq!(f.file(id).path, f.base.join("m (wiederhergestellt).3mf").to_string_lossy());
    }

    #[test]
    fn delete_for_good_keeps_the_entry_when_the_file_cannot_be_removed_and_ignores_a_missing_one() {
        let f = Fx::new("perm");
        let (id, _) = f.trashed("m.3mf");
        let fs = FaultFs::default().fail("remove_file", 1, PermissionDenied);
        assert!(delete_file_permanently(&fs, &f.conn, id, &f.trash).is_err());
        assert!(f.exists(id));
        let fs = FaultFs::default().fail("remove_file", 1, NotFound);
        delete_file_permanently(&fs, &f.conn, id, &f.trash).unwrap();
        assert!(!f.exists(id));
    }

    #[test]
    fn emptying_skips_an_entry_that_cannot_be_removed_and_removes_the_rest() {
        let f = Fx::new("empty");
        let (a, _) = f.trashed("a.3mf");
        let (b, _) = f.trashed("b.3mf");
        let fs = FaultFs::default().fail("remove_file", 1, PermissionDenied);
        empty_trash(&fs, &f.conn, &f.trash).unwrap();
        let left: Vec<bool> = [a, b].iter().map(|id| f.exists(*id)).collect();
        assert_eq!(left.iter().filter(|x| **x).count(), 1, "exactly the failing entry stays");
    }

    #[test]
    fn the_startup_clean_up_uses_the_seven_day_limit_of_the_given_time() {
        let f = Fx::new("cutoff");
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-15T12:00:00+00:00").unwrap().with_timezone(&chrono::Utc);
        let stamps = [
            ("2026-06-08T11:59:59+00:00", false),
            ("2026-06-08T12:00:00+00:00", true),
            ("2026-06-08T12:00:01+00:00", true),
            ("2026-06-14T00:00:00+00:00", true),
        ];
        let mut ids = Vec::new();
        for (i, (stamp, _)) in stamps.iter().enumerate() {
            let (id, _) = f.trashed(&format!("m{i}.3mf"));
            f.conn.execute("UPDATE files SET deleted_at = ?1 WHERE id = ?2", rusqlite::params![stamp, id]).unwrap();
            ids.push(id);
        }
        purge_expired_trash(&StdModelTrashFs, &f.conn, &f.trash, now);
        for (id, (stamp, stays)) in ids.iter().zip(stamps) {
            assert_eq!(f.exists(*id), stays, "{stamp}");
        }
    }

    #[test]
    fn the_startup_clean_up_goes_on_after_a_failing_entry() {
        let f = Fx::new("startup_fault");
        let (a, _) = f.trashed("a.3mf");
        let (b, _) = f.trashed("b.3mf");
        f.conn.execute("UPDATE files SET deleted_at = '2020-01-01T00:00:00+00:00'", []).unwrap();
        let fs = FaultFs::default().fail("remove_file", 1, PermissionDenied);
        purge_expired_trash(&fs, &f.conn, &f.trash, chrono::Utc::now());
        assert_eq!([a, b].iter().filter(|id| f.exists(**id)).count(), 1);
    }

    #[test]
    fn the_batch_skips_invalid_unknown_and_failing_entries() {
        let f = Fx::new("batch");
        let (a, _) = f.model("a.3mf");
        let (b, _) = f.model("b.3mf");
        let fs = FaultFs::default().fail("move_file", 1, PermissionDenied);
        delete_files(&fs, &f.conn, &f.trash, vec!["x".into(), "9999".into(), a.to_string(), b.to_string()]).unwrap();
        assert!(f.file(a).deleted_at.is_none(), "the failing one stays active");
        assert!(f.file(b).deleted_at.is_some(), "the batch goes on");
    }
}
