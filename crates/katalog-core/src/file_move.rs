//! Moving model files into another catalog folder (issue #20, package P5): the single move,
//! the batch (at most [`MAX_BATCH_MOVE_FILES`] files per call) and every roll-back branch.
//! The desktop commands keep the gate, the lock and the id parsing; this module has no
//! `fsops` / `upload_fs` dependency, so it builds on Windows and macOS.
//!
//! Disk access goes through [`FileMoveFs`], which the desktop implements on top of its
//! file-system boundary (`Fs`, P0a). The move itself is the primitive
//! `import::placement::move_file` (hard link first, copy as fallback across drives, never
//! replaces a target); a move across drives therefore stays what it always was: copy, then
//! remove the source, and a failure after the copy removes the copy again. Tests use a fake
//! that fails in step n.
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use crate::companion_actions::{self, CompanionFs, StdCompanionFs};
use crate::db;
use crate::error_code::ErrorCode;
use crate::import::placement::{comparison_path, reject_if_sensitive_path};
use crate::import::{ImportError as CmdError, ImportResult as CmdResult};

/// Upper bound per batch call; keeps one call from holding the catalog gate for an
/// unbounded time.
pub const MAX_BATCH_MOVE_FILES: usize = 5000;

/// The disk operations a move uses: the companion operations (which include the move of a
/// file itself) plus the two lookups that tell "catalog out of date" from a real fault.
pub trait FileMoveFs: CompanionFs {
    /// Same as `Path::is_dir` (follows links, any error is `false`).
    fn is_dir(&self, path: &Path) -> bool;
    /// Same as `Path::is_file`.
    fn is_file(&self, path: &Path) -> bool;
}

/// `std::fs`, 1:1.
pub struct StdFileMoveFs;

impl CompanionFs for StdFileMoveFs {
    fn symlink_metadata(&self, path: &Path) -> io::Result<std::fs::Metadata> {
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

impl FileMoveFs for StdFileMoveFs {
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
}

/// What [`move_file`] did for one file.
#[derive(Debug, PartialEq, Eq)]
pub enum MoveOutcome {
    /// Moved (or, for a file already lying in the folder's directory, only
    /// re-assigned in the catalog). Carries the old place for an undo.
    Moved { from_folder_id: Option<i64>, from_path: String },
    /// Already in the target folder, nothing to do.
    AlreadyThere,
}

/// A refused move: a stable machine code for the batch result plus the
/// message the single-file command has always shown.
#[derive(Debug)]
pub struct MoveRefusal {
    pub code: &'static str,
    pub error: CmdError,
    /// The companion error that made the move undo itself, when the undo failed too and
    /// `error` is the composed message instead. The caller converts it first (so the
    /// desktop log keeps the origin with its parameters, as before the move went into the
    /// core), then `error`. `None` when `error` is itself the origin.
    pub origin: Option<CmdError>,
}

impl MoveRefusal {
    fn new(code: &'static str, error: impl Into<CmdError>) -> Self {
        Self { code, error: error.into(), origin: None }
    }
}

/// Moves one file into the folder `folder_id` (`None`: keeps it where it is). Shared by the
/// single and the batch move, so both keep the same safety checks, never overwrite and roll
/// back on a DB failure. The refusal code is what lets the batch report a name clash or a
/// missing source without parsing messages. `case_insensitive` is how the companions' folder
/// names are compared (macOS, Windows).
pub fn move_file(
    fs: &dyn FileMoveFs,
    conn: &Connection,
    file_id: i64,
    folder_id: Option<i64>,
    sensitive_dirs: &[PathBuf],
    case_insensitive: bool,
) -> Result<MoveOutcome, MoveRefusal> {
    let file = db::get_file(conn, file_id)
        .map_err(|e| MoveRefusal::new("error", e.to_string()))?
        .ok_or_else(|| MoveRefusal::new("not_found", "file not found".to_string()))?;
    if file.deleted_at.is_some() {
        return Err(MoveRefusal::new("in_trash", CmdError::coded(ErrorCode::FileInTrash)));
    }

    let target_dir: PathBuf = match folder_id {
        Some(fid) => {
            let folders = db::list_folders(conn).map_err(|e| MoveRefusal::new("error", e.to_string()))?;
            let folder = folders
                .iter()
                .find(|f| f.id == fid)
                .ok_or_else(|| MoveRefusal::new("folder_not_found", "folder not found".to_string()))?;
            PathBuf::from(&folder.path)
        }
        None => Path::new(&file.path)
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| MoveRefusal::new("error", "invalid current path".to_string()))?,
    };
    let sensitive = |path: &Path| reject_if_sensitive_path(path, sensitive_dirs).map_err(|e| MoveRefusal::new("sensitive_path", e));
    sensitive(&target_dir)?;

    let old_path = PathBuf::from(&file.path);
    let name = old_path
        .file_name()
        .ok_or_else(|| MoveRefusal::new("error", "invalid file path".to_string()))?;
    let new_path = target_dir.join(name);
    sensitive(&old_path)?;
    sensitive(&new_path)?;
    if comparison_path(&old_path) == comparison_path(&new_path) {
        if folder_id.is_some() && file.folder_id == folder_id {
            return Ok(MoveOutcome::AlreadyThere);
        }
        // Importing the target's own contents only needs a catalog assignment.
        db::update_file_folder(conn, file_id, folder_id, &new_path.to_string_lossy())
            .map_err(|e| MoveRefusal::new("error", e.to_string()))?;
        return Ok(MoveOutcome::Moved { from_folder_id: file.folder_id, from_path: file.path });
    }
    // Without these checks the OS error ("No such file or directory") would reach
    // the user as an unexpected fault, although the catalog is merely out of date.
    if !fs.is_dir(&target_dir) {
        return Err(MoveRefusal::new(
            "target_missing",
            CmdError::coded_with(ErrorCode::TargetFolderMissingOnDisk, [target_dir.display()]),
        ));
    }
    if !fs.is_file(&old_path) {
        return Err(MoveRefusal::new(
            "source_missing",
            CmdError::coded_with(ErrorCode::FileMissingOnDisk, [old_path.display()]),
        ));
    }
    if let Err(e) = fs.move_file(&old_path, &new_path) {
        let code = if e.kind() == io::ErrorKind::AlreadyExists { "name_exists" } else { "error" };
        return Err(MoveRefusal::new(code, e.to_string()));
    }
    // The model's companion files travel with it; a clash there undoes the model move.
    let carried = match companion_actions::carry(fs, conn, file_id, &old_path, &new_path, case_insensitive) {
        Ok(carried) => carried,
        Err(error) => {
            if let Err(rollback_err) = fs.move_file(&new_path, &old_path) {
                let composed = format!(
                    "Begleitdateien konnten nicht mitgenommen werden ({error}) UND Rollback der Dateiverschiebung fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}",
                    new_path.display()
                );
                return Err(MoveRefusal { origin: Some(error), ..MoveRefusal::new("error", composed) });
            }
            return Err(MoveRefusal::new("error", error));
        }
    };

    if let Err(db_err) = db::update_file_folder(conn, file_id, folder_id, &new_path.to_string_lossy()) {
        carried.undo(fs, conn);
        // Undo the physical move so file system and DB don't drift apart. move_file
        // never overwrites anything.
        if let Err(rollback_err) = fs.move_file(&new_path, &old_path) {
            return Err(MoveRefusal::new(
                "error",
                format!(
                    "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Dateiverschiebung fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}, DB verweist weiter auf {}",
                    new_path.display(),
                    old_path.display()
                ),
            ));
        }
        // files.path is UNIQUE: a catalog row already owning the target path
        // (e.g. a file missing on disk) is a name clash, not a fault.
        let path_taken = conn
            .query_row(
                "SELECT 1 FROM files WHERE path = ?1 AND id != ?2",
                rusqlite::params![new_path.to_string_lossy(), file_id],
                |_| Ok(()),
            )
            .is_ok();
        let code = if path_taken { "name_exists" } else { "error" };
        return Err(MoveRefusal::new(code, db_err.to_string()));
    }
    carried.finish(fs);
    log::debug!(target: "datei", "verschoben: {} -> {}", old_path.display(), new_path.display());
    Ok(MoveOutcome::Moved { from_folder_id: file.folder_id, from_path: file.path })
}

/// Per-file result of a batch move. `status` is "moved", "skipped" or "failed"; `reason` is a
/// stable code. `from_folder_id`/`from_path` are only set for "moved" and are exactly what an
/// undo call needs.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MoveFileResult {
    pub id: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_folder_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_path: Option<String>,
}

impl MoveFileResult {
    fn with_reason(id: &str, status: &'static str, reason: &str) -> Self {
        Self { id: id.to_string(), status, reason: Some(reason.to_string()), from_folder_id: None, from_path: None }
    }
}

/// The target folder of a batch: the same for every file, so a wrong one fails the whole call
/// instead of producing thousands of identical per-file failures.
fn check_batch_target(fs: &dyn FileMoveFs, conn: &Connection, folder_id: i64, sensitive_dirs: &[PathBuf]) -> CmdResult<()> {
    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let folder = folders.iter().find(|f| f.id == folder_id).ok_or_else(|| "folder not found".to_string())?;
    let target_dir = PathBuf::from(&folder.path);
    reject_if_sensitive_path(&target_dir, sensitive_dirs)?;
    if !fs.is_dir(&target_dir) {
        return Err(CmdError::coded_with(ErrorCode::TargetFolderMissingOnDisk, [target_dir.display()]));
    }
    Ok(())
}

/// Moves several files into one catalog folder. `lock` hands out the DB connection per file,
/// so the lock is not held across the whole batch (moves across drives can be slow) and never
/// nested; its error type `E` is the caller's (the desktop's `CmdError`). Every file is moved
/// on its own (same checks and rollback as [`move_file`]); a refusal or failure for one file
/// never stops the rest.
pub fn move_files<G, C, E>(
    fs: &dyn FileMoveFs,
    mut lock: G,
    file_ids: &[String],
    folder_id: i64,
    sensitive_dirs: &[PathBuf],
    case_insensitive: bool,
) -> Result<Vec<MoveFileResult>, E>
where
    G: FnMut() -> Result<C, E>,
    C: std::ops::Deref<Target = Connection>,
    E: From<CmdError>,
{
    if file_ids.len() > MAX_BATCH_MOVE_FILES {
        return Err(CmdError::coded_with(ErrorCode::MoveTooManyFiles, [file_ids.len(), MAX_BATCH_MOVE_FILES]).into());
    }
    {
        let conn = lock()?;
        check_batch_target(fs, &conn, folder_id, sensitive_dirs).map_err(E::from)?;
    }

    let mut results = Vec::with_capacity(file_ids.len());
    for id_str in file_ids {
        let Ok(id) = id_str.parse::<i64>() else {
            results.push(MoveFileResult::with_reason(id_str, "failed", "invalid_id"));
            continue;
        };
        let conn = lock()?;
        results.push(match move_file(fs, &conn, id, Some(folder_id), sensitive_dirs, case_insensitive) {
            Ok(MoveOutcome::Moved { from_folder_id, from_path }) => MoveFileResult {
                id: id_str.clone(),
                status: "moved",
                reason: None,
                from_folder_id: from_folder_id.map(|f| f.to_string()),
                from_path: Some(from_path),
            },
            Ok(MoveOutcome::AlreadyThere) => MoveFileResult::with_reason(id_str, "skipped", "already_there"),
            Err(refusal) => {
                // The caller's error type is built (and so logged) for a refused file, as it
                // always was, although the batch reports only the code.
                if let Some(origin) = refusal.origin {
                    drop(E::from(origin));
                }
                drop(E::from(refusal.error));
                let status = if refusal.code == "name_exists" { "skipped" } else { "failed" };
                MoveFileResult::with_reason(id_str, status, refusal.code)
            }
        });
    }
    Ok(results)
}

#[cfg(test)]
#[path = "file_move_tests.rs"]
mod tests;
