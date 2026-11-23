//! Folder trash orchestration with desktop-compatible validation and SQL.
//! Hosts must hold their exclusive catalog gate throughout the call. Adapters must
//! enforce their own filesystem confinement; preflight checks cannot prevent races.
//! Single-model trash (including compensation and companions) remains a host operation.
use crate::import::sources::{SourceEntries, SourceKind};
use crate::import::{file_names::SETTING_CATALOG_BASE_DIR, placement::comparison_path};
use crate::write::{WriteError as CmdError, WriteResult as CmdResult};
use crate::{
    db::{self, error::DbError, FOLDER_SUBTREE_CTE},
    error_code::ErrorCode,
};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
};

mod std_fs;
pub use std_fs::StdFolderTrashFs;

/// Reads must preserve the requested symlink semantics. Listing must be lazy and
/// yield immediate children with kinds obtained without following symlinks.
pub trait FolderTrashFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    fn kind(&self, path: &Path, follow_links: bool) -> io::Result<SourceKind>;
    fn list(&self, path: &Path) -> io::Result<SourceEntries<'_>>;
    /// Remove only an empty directory, never recurse or remove a directory link.
    fn remove_empty_dir(&self, path: &Path) -> io::Result<()>;
}

/// Execute one admitted model's trash operation. Success means the model row has
/// been soft-deleted. On failure, attempt move-back compensation and report any
/// rollback failure, as for single-file trash. Hosts also handle missing files,
/// trash naming, and companions.
/// Folder-origin SQL and folder-row cleanup are owned by this module.
pub trait ModelTrash {
    fn trash_model(&self, conn: &Connection, file: &db::models::FileRecord) -> CmdResult<()>;
}

/// A per-model decision, without filesystem or database mutations.
#[derive(Debug, PartialEq, Eq)]
pub struct ModelTrashPlan {
    pub file_id: i64,
    pub path: String,
    pub refusal_reason: Option<&'static str>,
}

/// Plan against the resolved chosen folder and sensitive paths expanded with their
/// canonical spellings. A missing file is admitted for the host's soft-delete path.
/// This is a snapshot, not an authorization token; hosts still enforce confinement.
pub fn plan_model_trash(
    fs: &dyn FolderTrashFs,
    file_id: i64,
    path: String,
    resolved_root: &Path,
    expanded_sensitive: &[PathBuf],
) -> ModelTrashPlan {
    let refusal_reason = refusal_reason(fs, &path, resolved_root, expanded_sensitive);
    ModelTrashPlan {
        file_id,
        path,
        refusal_reason,
    }
}

fn parse_id(id: &str) -> CmdResult<i64> {
    id.parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| CmdError::coded(ErrorCode::InvalidCatalogId))
}

fn expand_sensitive_dirs(fs: &dyn FolderTrashFs, dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut expanded = Vec::new();
    for dir in dirs {
        if let Ok(canonical) = fs.canonicalize(dir) {
            if canonical != *dir {
                expanded.push(canonical);
            }
        }
        expanded.push(dir.clone());
    }
    expanded
}

// Missing tails are reconstructed after resolving the nearest existing ancestor,
// matching desktop checks for unplugged drives and missing directories.
fn resolve_path(fs: &dyn FolderTrashFs, path: &Path) -> CmdResult<PathBuf> {
    if let Ok(canonical) = fs.canonicalize(path) {
        return Ok(canonical);
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Pfad enthaelt \"..\"-Komponenten und wird abgelehnt"
            .to_string()
            .into());
    }
    let mut trailing = Vec::new();
    let mut cursor = path;
    while let (Some(name), Some(parent)) = (cursor.file_name(), cursor.parent()) {
        trailing.push(name.to_os_string());
        if let Ok(mut resolved) = fs.canonicalize(parent) {
            for component in trailing.iter().rev() {
                resolved.push(component);
            }
            return Ok(resolved);
        }
        cursor = parent;
    }
    Ok(path.to_path_buf())
}

fn check_managed_location(fs: &dyn FolderTrashFs, path: &Path, dirs: &[PathBuf]) -> CmdResult<()> {
    let resolved = resolve_path(fs, path)?;
    for dir in dirs {
        if resolved.starts_with(dir) {
            return Err(format!(
                "Zielpfad liegt in einem geschuetzten Systemverzeichnis ({}) und wird abgelehnt",
                dir.display()
            )
            .into());
        }
    }
    if let Some(dir) = dirs
        .iter()
        .find(|dir| **dir != resolved && dir.starts_with(&resolved))
    {
        return Err(format!(
            "Zielpfad enthaelt ein geschuetztes Verzeichnis ({}) und wird abgelehnt",
            dir.display()
        )
        .into());
    }
    Ok(())
}

fn reject_catalog_root_relocation(
    fs: &dyn FolderTrashFs,
    conn: &Connection,
    path: &Path,
) -> CmdResult<()> {
    let Some(base) = db::printer_link::get_setting(conn, SETTING_CATALOG_BASE_DIR)? else {
        return Ok(());
    };
    let base = resolve_path(fs, Path::new(&base))?;
    let source = resolve_path(fs, path)?;
    if comparison_path(&base).starts_with(comparison_path(&source)) {
        return Err(CmdError::coded(ErrorCode::CatalogRootProtected));
    }
    Ok(())
}

/// Upper bound for the directory entries inspected to count non-catalog files; a
/// huge tree (e.g. a whole drive) must not block the dialog.
pub const OTHER_FILES_SCAN_LIMIT: usize = 10_000;

const ROOT_ERROR: ErrorCode = ErrorCode::CatalogRootNotDeletable;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderTrashSummary {
    pub name: String,
    pub subfolder_count: i64,
    /// Active catalog files in the folder and all its subfolders.
    pub model_count: i64,
    /// Files on disk below the folder that the catalog does not know. Only counted,
    /// never opened or changed.
    pub other_file_count: i64,
    /// The scan stopped at its limit, so `other_file_count` is "at least".
    pub other_file_count_truncated: bool,
    /// The folder is the catalog base folder or one of its ancestors; deleting is refused.
    pub is_catalog_root_or_ancestor: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderTrashFailure {
    pub file_id: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderTrashResult {
    pub trashed_model_count: i64,
    /// Models that could not be moved to the app trash; they stay where they are.
    pub failed: Vec<FolderTrashFailure>,
    pub removed_folder_count: i64,
    /// Folders of the subtree that still exist on disk (not empty, symlink or not removable).
    pub kept_on_disk: Vec<String>,
}

/// The folder and all its descendants, parents before children.
fn subtree_folders(
    all: &[db::models::FolderRecord],
    root_id: i64,
) -> Vec<db::models::FolderRecord> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = std::collections::VecDeque::from([root_id]);
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(folder) = all.iter().find(|f| f.id == id) {
            result.push(folder.clone());
            queue.extend(all.iter().filter(|f| f.parent_id == Some(id)).map(|f| f.id));
        }
    }
    result
}

fn is_catalog_root_or_ancestor(
    fs: &dyn FolderTrashFs,
    conn: &Connection,
    folders: &[db::models::FolderRecord],
) -> bool {
    // A path that cannot be resolved counts as protected: refusing is the safe answer.
    folders
        .iter()
        .any(|f| reject_catalog_root_relocation(fs, conn, Path::new(&f.path)).is_err())
}

/// Active catalog files below the given folders as `(id, path)`.
fn active_files_in_subtree(conn: &Connection, root_id: i64) -> CmdResult<Vec<(i64, String)>> {
    let mut stmt = conn
        .prepare(&format!(
            "{FOLDER_SUBTREE_CTE} SELECT id, path FROM files WHERE folder_id IN (SELECT id FROM subtree) AND deleted_at IS NULL ORDER BY id"
        ))
        .map_err(DbError::from)?;
    let rows = stmt
        .query_map([root_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(DbError::from)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DbError::from)?;
    Ok(rows)
}

/// Counts entries below `root` that are not in `known`. Symlinks are never entered
/// (a linked folder could lead anywhere) and count as one file each.
fn count_other_files(fs: &dyn FolderTrashFs, root: &Path, known: &HashSet<PathBuf>) -> (i64, bool) {
    if fs
        .kind(root, false)
        .map_or(true, |m| m != SourceKind::Directory)
    {
        return (0, false);
    }
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0usize;
    let mut count = 0i64;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs.list(&dir) else { continue };
        for entry in entries.flatten() {
            if visited >= OTHER_FILES_SCAN_LIMIT {
                return (count, true);
            }
            visited += 1;
            // DirEntry::file_type does not follow symlinks.
            let Ok(kind) = entry.kind else { continue };
            if kind == SourceKind::Directory {
                stack.push(entry.path);
            } else if !known.contains(&comparison_path(&entry.path)) {
                count += 1;
            }
        }
    }
    (count, false)
}

/// Why a catalog row must not be moved when its folder tree is deleted, or `None`.
/// A row only has to agree with the folder by id, and a restored backup can set any
/// path, so before anything is touched the file itself must prove that it is a model
/// inside the folder being deleted. `root` is the resolved path of that folder.
fn refusal_reason(
    fs: &dyn FolderTrashFs,
    file_path: &str,
    root: &Path,
    expanded_sensitive: &[PathBuf],
) -> Option<&'static str> {
    let path = Path::new(file_path);
    // `..` can keep the textual folder prefix and still leave the folder.
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Some("outside_folder");
    }
    // Nothing on disk to move: the delete only soft-deletes the row.
    let Ok(meta) = fs.kind(path, false) else {
        return None;
    };
    // The folder itself must be a reachable real directory, and the file's parent
    // (symlinked directories resolved) must lie inside it, compared per component so
    // `A2` is not inside `A`.
    let Some(parent) = path.parent() else {
        return Some("outside_folder");
    };
    let (Ok(resolved_parent), true) = (
        fs.canonicalize(parent),
        fs.kind(root, true)
            .is_ok_and(|k| k == SourceKind::Directory),
    ) else {
        return Some("outside_folder");
    };
    if !comparison_path(&resolved_parent).starts_with(comparison_path(root)) {
        return Some("outside_folder");
    }
    if check_managed_location(fs, &resolved_parent, expanded_sensitive).is_err() {
        return Some("outside_folder");
    }
    // Entries of the catalog, so sliced files the user added one by one go to the trash as well.
    if !crate::import::files::is_catalog_extension(path) {
        return Some("not_a_model");
    }
    if meta == SourceKind::Symlink {
        return Some("symlink");
    }
    None
}

/// Resolved path of the folder row. An unresolvable folder yields a path that is not a
/// directory, which makes `refusal_reason` refuse every file that exists on disk.
fn resolved_root(fs: &dyn FolderTrashFs, folder: &db::models::FolderRecord) -> PathBuf {
    fs.canonicalize(Path::new(&folder.path))
        .unwrap_or_else(|_| PathBuf::from("/nonexistent-unresolvable-folder"))
}

pub fn folder_trash_summary_with_conn(
    fs: &dyn FolderTrashFs,
    conn: &Connection,
    folder_id: &str,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<FolderTrashSummary> {
    let id = parse_id(folder_id)?;
    let all = db::list_folders(conn)?;
    let subtree = subtree_folders(&all, id);
    let root = subtree
        .first()
        .ok_or_else(|| CmdError::coded(ErrorCode::FolderNotFound))?;
    // Same rule as the delete: rows it would refuse are not models of this folder.
    let expanded = expand_sensitive_dirs(fs, sensitive_dirs);
    let resolved = resolved_root(fs, root);
    let files: Vec<(i64, String)> = active_files_in_subtree(conn, id)?
        .into_iter()
        .filter(|(_, p)| refusal_reason(fs, p, &resolved, &expanded).is_none())
        .collect();
    // Companion files go to the trash with their model, so they are not "other" files.
    let companion_paths = db::companions::list_active_companion_paths(conn)?;
    let known: HashSet<PathBuf> = files
        .iter()
        .map(|(_, p)| p)
        .chain(companion_paths.iter())
        .map(|p| comparison_path(Path::new(p)))
        .collect();
    let (other_file_count, other_file_count_truncated) =
        count_other_files(fs, Path::new(&root.path), &known);
    Ok(FolderTrashSummary {
        name: root.name.clone(),
        subfolder_count: subtree.len() as i64 - 1,
        model_count: files.len() as i64,
        other_file_count,
        other_file_count_truncated,
        is_catalog_root_or_ancestor: is_catalog_root_or_ancestor(fs, conn, &subtree),
    })
}

/// Moves every model of the folder tree into the app trash, drops the catalog rows of
/// folders that no longer hold models, and removes only EMPTY directories from disk.
/// Files the catalog does not know are never touched: a directory holding one simply
/// cannot be removed by `remove_dir` and stays.
pub fn trash_folder_with_conn(
    fs: &dyn FolderTrashFs,
    trash: &dyn ModelTrash,
    conn: &Connection,
    folder_id: &str,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<FolderTrashResult> {
    let id = parse_id(folder_id)?;
    let all = db::list_folders(conn)?;
    let subtree = subtree_folders(&all, id);
    if subtree.is_empty() {
        return Err(CmdError::coded(ErrorCode::FolderNotFound));
    }
    if is_catalog_root_or_ancestor(fs, conn, &subtree) {
        return Err(CmdError::coded(ROOT_ERROR));
    }
    // A crafted backup can point any folder row anywhere, so every row is checked.
    let expanded = expand_sensitive_dirs(fs, sensitive_dirs);
    for folder in &subtree {
        check_managed_location(fs, Path::new(&folder.path), &expanded)?;
    }

    // One file at a time through the same code as `delete_file`, so its move-back
    // compensation applies; a failure must not stop the others.
    let mut trashed = 0i64;
    let mut failed = Vec::new();
    let resolved = resolved_root(fs, &subtree[0]);
    for (file_id, path) in active_files_in_subtree(conn, id)? {
        let ModelTrashPlan {
            file_id,
            path,
            refusal_reason,
        } = plan_model_trash(fs, file_id, path, &resolved, &expanded);
        if let Some(code) = refusal_reason {
            log::warn!(target: "cleanup", "Folder trash refused ({code}): {path}");
            failed.push(FolderTrashFailure {
                file_id: file_id.to_string(),
                path,
                reason: code.to_string(),
            });
            continue;
        }
        let outcome = match db::get_file(conn, file_id) {
            Ok(Some(file)) => trash.trash_model(conn, &file),
            Ok(None) => continue,
            Err(e) => Err(CmdError::from(e.to_string())),
        };
        match outcome {
            Ok(()) => {
                trashed += 1;
                // Remember that the folder went with it, so a restore may recreate its rows.
                if let Err(e) = conn.execute(
                    "INSERT OR IGNORE INTO folder_trash_origin (file_id) VALUES (?1)",
                    [file_id],
                ) {
                    log::warn!(target: "cleanup", "Folder trash origin not recorded: {path} ({e})");
                }
            }
            Err(e) => {
                log::warn!(target: "cleanup", "Folder trash skipped: {path} ({e})");
                failed.push(FolderTrashFailure {
                    file_id: file_id.to_string(),
                    path,
                    reason: e.to_string(),
                });
            }
        }
    }

    // A folder row may only go when nothing active is left below it. Folders with a
    // leftover model, and their ancestors up to the root, stay in the catalog.
    let remaining: Vec<(i64, String)> = active_files_in_subtree(conn, id)?;
    let remaining_ids: HashSet<i64> = remaining.iter().map(|(fid, _)| *fid).collect();
    let mut blocked: HashSet<i64> = HashSet::new();
    if !remaining_ids.is_empty() {
        let mut stmt = conn
            .prepare(&format!(
                "{FOLDER_SUBTREE_CTE} SELECT DISTINCT folder_id FROM files WHERE folder_id IN (SELECT id FROM subtree) AND deleted_at IS NULL"
            ))
            .map_err(DbError::from)?;
        let holders = stmt
            .query_map([id], |r| r.get::<_, i64>(0))
            .map_err(DbError::from)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(DbError::from)?;
        for holder in holders {
            let mut current = Some(holder);
            while let Some(cur) = current {
                if !blocked.insert(cur) {
                    break;
                }
                current = subtree
                    .iter()
                    .find(|f| f.id == cur)
                    .and_then(|f| f.parent_id)
                    .filter(|p| subtree.iter().any(|f| f.id == *p));
            }
        }
    }
    let removable: Vec<&db::models::FolderRecord> = subtree
        .iter()
        .filter(|f| !blocked.contains(&f.id))
        .collect();
    let tx = conn.unchecked_transaction().map_err(DbError::from)?;
    for folder in &removable {
        tx.execute("DELETE FROM folders WHERE id = ?1", [folder.id])
            .map_err(DbError::from)?;
    }
    tx.commit().map_err(DbError::from)?;

    // Deepest first, so a parent becomes empty once its child directories are gone.
    let mut by_depth: Vec<&db::models::FolderRecord> = subtree.iter().collect();
    by_depth.sort_by_key(|f| std::cmp::Reverse(Path::new(&f.path).components().count()));
    let mut kept_on_disk = Vec::new();
    for folder in by_depth {
        let path = Path::new(&folder.path);
        if folder.path.is_empty() {
            continue;
        }
        match fs.kind(path, false) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => kept_on_disk.push(folder.path.clone()),
            // A symlink is never removed as a folder (on Windows `remove_dir` would
            // drop a directory link), and never followed.
            Ok(meta) if meta != SourceKind::Directory => kept_on_disk.push(folder.path.clone()),
            Ok(_) if blocked.contains(&folder.id) => kept_on_disk.push(folder.path.clone()),
            // A folder row can name any path; only a directory that really lies inside
            // the chosen folder (links resolved) may be removed.
            Ok(_)
                if !fs.canonicalize(path).is_ok_and(|real| {
                    comparison_path(&real).starts_with(comparison_path(&resolved))
                }) =>
            {
                kept_on_disk.push(folder.path.clone())
            }
            // Never `remove_dir_all`: a non-empty directory must fail here.
            Ok(_) => {
                if fs.remove_empty_dir(path).is_err() {
                    kept_on_disk.push(folder.path.clone());
                }
            }
        }
    }
    kept_on_disk.sort();

    log::info!(
        target: "catalog",
        "Folder trash: {trashed} models, {} failures, {} folder rows removed, {} directories kept",
        failed.len(),
        removable.len(),
        kept_on_disk.len()
    );
    Ok(FolderTrashResult {
        trashed_model_count: trashed,
        failed,
        removed_folder_count: removable.len() as i64,
        kept_on_disk,
    })
}

#[cfg(test)]
mod tests;
