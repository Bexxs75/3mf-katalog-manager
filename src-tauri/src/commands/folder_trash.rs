use super::catalog_removal::{parse_id, SUBTREE};
use super::*;
use tauri::Manager;

/// Upper bound for the directory entries inspected to count non-catalog files; a
/// huge tree (e.g. a whole drive) must not block the dialog.
const OTHER_FILES_SCAN_LIMIT: usize = 10_000;

const ROOT_ERROR: &str = "Das ist der Speicherort des Katalogs oder ein übergeordneter Ordner davon und kann nicht gelöscht werden.";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderTrashSummary {
    name: String,
    subfolder_count: i64,
    /// Active catalog files in the folder and all its subfolders.
    model_count: i64,
    /// Files on disk below the folder that the catalog does not know. Only counted,
    /// never opened or changed.
    other_file_count: i64,
    /// The scan stopped at its limit, so `other_file_count` is "at least".
    other_file_count_truncated: bool,
    /// The folder is the catalog base folder or one of its ancestors; deleting is refused.
    is_catalog_root_or_ancestor: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderTrashFailure {
    file_id: String,
    path: String,
    reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderTrashResult {
    trashed_model_count: i64,
    /// Models that could not be moved to the app trash; they stay where they are.
    failed: Vec<FolderTrashFailure>,
    removed_folder_count: i64,
    /// Folders of the subtree that still exist on disk (not empty, symlink or not removable).
    kept_on_disk: Vec<String>,
}

/// The folder and all its descendants, parents before children.
fn subtree_folders(all: &[db::models::FolderRecord], root_id: i64) -> Vec<db::models::FolderRecord> {
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

fn is_catalog_root_or_ancestor(conn: &Connection, folders: &[db::models::FolderRecord]) -> bool {
    // A path that cannot be resolved counts as protected: refusing is the safe answer.
    folders.iter().any(|f| folders::reject_catalog_root_relocation(conn, Path::new(&f.path)).is_err())
}

/// Active catalog files below the given folders as `(id, path)`.
fn active_files_in_subtree(conn: &Connection, root_id: i64) -> CmdResult<Vec<(i64, String)>> {
    let mut stmt = conn
        .prepare(&format!(
            "{SUBTREE} SELECT id, path FROM files WHERE folder_id IN (SELECT id FROM subtree) AND deleted_at IS NULL ORDER BY id"
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
fn count_other_files(root: &Path, known: &HashSet<PathBuf>) -> (i64, bool) {
    if std::fs::symlink_metadata(root).map_or(true, |m| !m.is_dir()) {
        return (0, false);
    }
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0usize;
    let mut count = 0i64;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if visited >= OTHER_FILES_SCAN_LIMIT {
                return (count, true);
            }
            visited += 1;
            // DirEntry::file_type does not follow symlinks.
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                stack.push(entry.path());
            } else if !known.contains(&folders::comparison_path(&entry.path())) {
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
fn refusal_reason(file_path: &str, root: &Path, expanded_sensitive: &[PathBuf]) -> Option<&'static str> {
    let path = Path::new(file_path);
    // `..` can keep the textual folder prefix and still leave the folder.
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Some("outside_folder");
    }
    // Nothing on disk to move: the delete only soft-deletes the row.
    let Ok(meta) = std::fs::symlink_metadata(path) else { return None };
    // The folder itself must be a reachable real directory, and the file's parent
    // (symlinked directories resolved) must lie inside it, compared per component so
    // `A2` is not inside `A`.
    let Some(parent) = path.parent() else { return Some("outside_folder") };
    let (Ok(resolved_parent), true) = (parent.canonicalize(), root.is_dir()) else { return Some("outside_folder") };
    if !folders::comparison_path(&resolved_parent).starts_with(folders::comparison_path(root)) {
        return Some("outside_folder");
    }
    if archives::check_managed_location(&resolved_parent, expanded_sensitive).is_err() {
        return Some("outside_folder");
    }
    if !is_supported_extension(path) {
        return Some("not_a_model");
    }
    if meta.file_type().is_symlink() {
        return Some("symlink");
    }
    None
}

/// Resolved path of the folder row. An unresolvable folder yields a path that is not a
/// directory, which makes `refusal_reason` refuse every file that exists on disk.
fn resolved_root(folder: &db::models::FolderRecord) -> PathBuf {
    Path::new(&folder.path).canonicalize().unwrap_or_else(|_| PathBuf::from("/nonexistent-unresolvable-folder"))
}

fn folder_trash_summary_with_conn(conn: &Connection, folder_id: &str, sensitive_dirs: &[PathBuf]) -> CmdResult<FolderTrashSummary> {
    let id = parse_id(folder_id)?;
    let all = db::list_folders(conn)?;
    let subtree = subtree_folders(&all, id);
    let root = subtree.first().ok_or_else(|| CmdError::expected("Ordner nicht gefunden."))?;
    // Same rule as the delete: rows it would refuse are not models of this folder.
    let expanded = expand_sensitive_dirs(sensitive_dirs);
    let resolved = resolved_root(root);
    let files: Vec<(i64, String)> = active_files_in_subtree(conn, id)?
        .into_iter()
        .filter(|(_, p)| refusal_reason(p, &resolved, &expanded).is_none())
        .collect();
    // Companion files go to the trash with their model, so they are not "other" files.
    let companion_paths = db::companions::list_active_companion_paths(conn)?;
    let known: HashSet<PathBuf> = files
        .iter()
        .map(|(_, p)| p)
        .chain(companion_paths.iter())
        .map(|p| folders::comparison_path(Path::new(p)))
        .collect();
    let (other_file_count, other_file_count_truncated) = count_other_files(Path::new(&root.path), &known);
    Ok(FolderTrashSummary {
        name: root.name.clone(),
        subfolder_count: subtree.len() as i64 - 1,
        model_count: files.len() as i64,
        other_file_count,
        other_file_count_truncated,
        is_catalog_root_or_ancestor: is_catalog_root_or_ancestor(conn, &subtree),
    })
}

/// Moves every model of the folder tree into the app trash, drops the catalog rows of
/// folders that no longer hold models, and removes only EMPTY directories from disk.
/// Files the catalog does not know are never touched: a directory holding one simply
/// cannot be removed by `remove_dir` and stays.
fn trash_folder_with_conn(
    conn: &Connection,
    folder_id: &str,
    trash_dir: &Path,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<FolderTrashResult> {
    let id = parse_id(folder_id)?;
    let all = db::list_folders(conn)?;
    let subtree = subtree_folders(&all, id);
    if subtree.is_empty() {
        return Err(CmdError::expected("Ordner nicht gefunden."));
    }
    if is_catalog_root_or_ancestor(conn, &subtree) {
        return Err(CmdError::expected(ROOT_ERROR));
    }
    // A crafted backup can point any folder row anywhere, so every row is checked.
    let expanded = expand_sensitive_dirs(sensitive_dirs);
    for folder in &subtree {
        archives::check_managed_location(Path::new(&folder.path), &expanded)?;
    }

    // One file at a time through the same code as `delete_file`, so its move-back
    // compensation applies; a failure must not stop the others.
    let mut trashed = 0i64;
    let mut failed = Vec::new();
    let resolved = resolved_root(&subtree[0]);
    for (file_id, path) in active_files_in_subtree(conn, id)? {
        if let Some(code) = refusal_reason(&path, &resolved, &expanded) {
            log::warn!(target: "cleanup", "Ordner löschen, abgelehnt ({code}): {path}");
            failed.push(FolderTrashFailure { file_id: file_id.to_string(), path, reason: code.to_string() });
            continue;
        }
        let outcome = match db::get_file(conn, file_id) {
            Ok(Some(file)) => trash::delete_file_with_conn(conn, &file, file_id, trash_dir),
            Ok(None) => continue,
            Err(e) => Err(CmdError::from(e.to_string())),
        };
        match outcome {
            Ok(()) => {
                trashed += 1;
                // Remember that the folder went with it, so a restore may recreate its rows.
                if let Err(e) = conn.execute("INSERT OR IGNORE INTO folder_trash_origin (file_id) VALUES (?1)", [file_id]) {
                    log::warn!(target: "cleanup", "Ordner löschen, Herkunft nicht gemerkt: {path} ({e})");
                }
            }
            Err(e) => {
                log::warn!(target: "cleanup", "Ordner löschen, übersprungen: {path} ({e})");
                failed.push(FolderTrashFailure { file_id: file_id.to_string(), path, reason: e.to_string() });
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
                "{SUBTREE} SELECT DISTINCT folder_id FROM files WHERE folder_id IN (SELECT id FROM subtree) AND deleted_at IS NULL"
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
                current = subtree.iter().find(|f| f.id == cur).and_then(|f| f.parent_id).filter(|p| subtree.iter().any(|f| f.id == *p));
            }
        }
    }
    let removable: Vec<&db::models::FolderRecord> = subtree.iter().filter(|f| !blocked.contains(&f.id)).collect();
    let tx = conn.unchecked_transaction().map_err(DbError::from)?;
    for folder in &removable {
        tx.execute("DELETE FROM folders WHERE id = ?1", [folder.id]).map_err(DbError::from)?;
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
        match std::fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => kept_on_disk.push(folder.path.clone()),
            // A symlink is never removed as a folder (on Windows `remove_dir` would
            // drop a directory link), and never followed.
            Ok(meta) if !meta.is_dir() => kept_on_disk.push(folder.path.clone()),
            Ok(_) if blocked.contains(&folder.id) => kept_on_disk.push(folder.path.clone()),
            // A folder row can name any path; only a directory that really lies inside
            // the chosen folder (links resolved) may be removed.
            Ok(_) if !path.canonicalize().is_ok_and(|real| folders::comparison_path(&real).starts_with(folders::comparison_path(&resolved))) => {
                kept_on_disk.push(folder.path.clone())
            }
            // Never `remove_dir_all`: a non-empty directory must fail here.
            Ok(_) => {
                if std::fs::remove_dir(path).is_err() {
                    kept_on_disk.push(folder.path.clone());
                }
            }
        }
    }
    kept_on_disk.sort();

    log::info!(
        target: "catalog",
        "Ordner in den Papierkorb: {trashed} Modelle, {} Fehler, {} Ordnereinträge entfernt, {} Ordner auf der Festplatte behalten",
        failed.len(),
        removable.len(),
        kept_on_disk.len()
    );
    Ok(FolderTrashResult { trashed_model_count: trashed, failed, removed_folder_count: removable.len() as i64, kept_on_disk })
}

/// Test helper for the restore tests: trashes a folder tree and returns how many models moved.
#[cfg(test)]
pub(super) fn trash_folder_for_test(conn: &Connection, folder_id: i64, trash_dir: &Path) -> i64 {
    trash_folder_with_conn(conn, &folder_id.to_string(), trash_dir, &[]).unwrap().trashed_model_count
}

#[tauri::command]
pub async fn folder_trash_summary(app: tauri::AppHandle, folder_id: String) -> CmdResult<FolderTrashSummary> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = lock_db(&state)?;
        folder_trash_summary_with_conn(&conn, &folder_id, &state.sensitive_dirs)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn trash_folder(app: tauri::AppHandle, folder_id: String) -> CmdResult<FolderTrashResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _catalog_share = state.import_jobs.gate.exclusive()?;
        let conn = lock_db(&state)?;
        trash_folder_with_conn(&conn, &folder_id, &state.trash_dir, &state.sensitive_dirs)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        conn: Connection,
        root: PathBuf,
        trash: PathBuf,
        a: i64,
        sensitive: Vec<PathBuf>,
    }

    /// catalog/ (base) -> A -> {B, C}; models: A/a.3mf, B/b.3mf, C/c.3mf, plus the base folder row.
    fn fixture(name: &str) -> Fixture {
        let tmp = unique_test_dir(name);
        let root = tmp.join("catalog");
        let trash = tmp.join("trash");
        for dir in [root.join("A").join("B"), root.join("A").join("C"), trash.clone()] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let conn = db::connect_in_memory().unwrap();
        db::printer_link::set_setting(&conn, folders::SETTING_CATALOG_BASE_DIR, &root.to_string_lossy()).unwrap();
        let root_id = db::insert_folder_with_parent(&conn, "catalog", None, &root.to_string_lossy()).unwrap();
        let mk = |parent: i64, rel: &str| {
            let path = root.join(rel);
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            db::insert_folder_with_parent(&conn, &name, Some(parent), &path.to_string_lossy()).unwrap()
        };
        let a = mk(root_id, "A");
        let b = mk(a, "A/B");
        let c = mk(a, "A/C");
        for (folder, rel) in [(a, "A/a.3mf"), (b, "A/B/b.3mf"), (c, "A/C/c.3mf")] {
            let path = root.join(rel);
            std::fs::write(&path, rel.as_bytes()).unwrap();
            db::test_insert_minimal_file(&conn, &path.to_string_lossy(), Some(folder)).unwrap();
        }
        Fixture { conn, root, trash, a, sensitive: vec![tmp.join("sensitive")] }
    }

    fn folder_rows(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0)).unwrap()
    }

    fn active_files(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM files WHERE deleted_at IS NULL", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn trashes_models_drops_rows_and_removes_empty_directories() {
        let f = fixture("folder_trash_full");
        let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
        assert_eq!(result.trashed_model_count, 3);
        assert!(result.failed.is_empty() && result.kept_on_disk.is_empty(), "{result:?}");
        assert_eq!(result.removed_folder_count, 3);
        assert_eq!(active_files(&f.conn), 0);
        assert_eq!(db::list_trash(&f.conn).unwrap().len(), 3);
        // Only the base folder row is left.
        assert_eq!(folder_rows(&f.conn), 1);
        assert!(!f.root.join("A").exists(), "empty directories must be removed");
        assert!(f.root.exists(), "the catalog base folder must stay");
        assert_eq!(std::fs::read_dir(&f.trash).unwrap().count(), 3, "models must sit in the app trash");
    }

    #[test]
    fn folder_with_a_non_model_file_stays_and_the_file_is_untouched() {
        let f = fixture("folder_trash_other_file");
        let note = f.root.join("A").join("B").join("notes.txt");
        std::fs::write(&note, b"keep me").unwrap();
        let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
        assert_eq!(result.trashed_model_count, 3);
        assert_eq!(std::fs::read(&note).unwrap(), b"keep me");
        let kept: Vec<_> = result.kept_on_disk.iter().map(PathBuf::from).collect();
        assert_eq!(kept, vec![f.root.join("A"), f.root.join("A").join("B")]);
        assert!(!f.root.join("A").join("C").exists(), "the sibling that is really empty goes");
        // Catalog rows are gone because no models are left, even though the directory stays.
        assert_eq!(folder_rows(&f.conn), 1);
    }

    #[test]
    fn an_unknown_empty_subdirectory_is_kept_and_reported() {
        let f = fixture("folder_trash_unknown_dir");
        let unknown = f.root.join("A").join("B").join("not-in-catalog");
        std::fs::create_dir(&unknown).unwrap();
        let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
        assert!(unknown.is_dir());
        assert!(result.kept_on_disk.iter().any(|p| Path::new(p) == f.root.join("A").join("B")));
    }

    #[test]
    fn an_empty_directory_outside_the_chosen_folder_is_never_removed() {
        let f = fixture("folder_trash_outside_dir");
        let outside = f.root.parent().unwrap().join("elsewhere");
        std::fs::create_dir(&outside).unwrap();
        db::insert_folder_with_parent(&f.conn, "elsewhere", Some(f.a), &outside.to_string_lossy()).unwrap();
        let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
        assert!(outside.is_dir(), "a directory outside the folder stays");
        assert!(result.kept_on_disk.iter().any(|p| Path::new(p) == outside));
        assert!(!f.root.join("A").exists(), "counter-check: the folder's own empty directories still go");
    }

    #[test]
    fn catalog_root_and_its_ancestors_are_refused() {
        let f = fixture("folder_trash_root");
        let root_id: i64 = f.conn.query_row("SELECT id FROM folders WHERE parent_id IS NULL", [], |r| r.get(0)).unwrap();
        let err = trash_folder_with_conn(&f.conn, &root_id.to_string(), &f.trash, &f.sensitive).unwrap_err();
        assert_eq!(err.to_string(), ROOT_ERROR);
        assert_eq!(active_files(&f.conn), 3);
        assert!(f.root.join("A").join("a.3mf").exists());
        assert!(folder_summary(&f, root_id).is_catalog_root_or_ancestor);
        assert!(!folder_summary(&f, f.a).is_catalog_root_or_ancestor);

        // An ancestor folder of the base (base lives one level deeper).
        let parent = f.root.parent().unwrap();
        let parent_id = db::insert_folder_with_parent(&f.conn, "tmp", None, &parent.to_string_lossy()).unwrap();
        assert!(folder_summary(&f, parent_id).is_catalog_root_or_ancestor);
        assert!(trash_folder_with_conn(&f.conn, &parent_id.to_string(), &f.trash, &f.sensitive).is_err());
        assert_eq!(active_files(&f.conn), 3);
    }

    fn folder_summary(f: &Fixture, id: i64) -> FolderTrashSummary {
        folder_trash_summary_with_conn(&f.conn, &id.to_string(), &f.sensitive).unwrap()
    }

    #[test]
    fn sensitive_paths_are_refused_without_touching_anything() {
        let f = fixture("folder_trash_sensitive");
        let sensitive = vec![f.root.join("A").join("B")];
        let err = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &sensitive);
        assert!(err.is_err(), "a sensitive subfolder must stop the whole operation");
        assert_eq!(active_files(&f.conn), 3);
        assert!(f.root.join("A").join("B").join("b.3mf").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_neither_followed_nor_removed() {
        let f = fixture("folder_trash_symlink");
        let outside = unique_test_dir("folder_trash_symlink_outside");
        std::fs::write(outside.join("precious.txt"), b"outside").unwrap();
        let link = f.root.join("A").join("C").join("link");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(outside.join("precious.txt")).unwrap(), b"outside");
        assert!(result.kept_on_disk.iter().any(|p| Path::new(p) == f.root.join("A").join("C")));
        // The summary counts the link as one other file and does not enter it.
        let f2 = fixture("folder_trash_symlink_summary");
        std::os::unix::fs::symlink(&outside, f2.root.join("A").join("link")).unwrap();
        let summary = folder_summary(&f2, f2.a);
        assert_eq!(summary.other_file_count, 1);
        assert!(!summary.other_file_count_truncated);
    }

    #[test]
    fn one_unmovable_file_does_not_stop_the_rest_and_keeps_its_folder() {
        let f = fixture("folder_trash_partial");
        // The trash target already exists, so exactly this file cannot be moved.
        let blocked_id: i64 = f
            .conn
            .query_row("SELECT id FROM files WHERE name = 'b.3mf'", [], |r| r.get(0))
            .unwrap();
        std::fs::write(f.trash.join(format!("{blocked_id}-b.3mf")), b"squatter").unwrap();
        let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
        assert_eq!(result.trashed_model_count, 2);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].file_id, blocked_id.to_string());
        assert!(!result.failed[0].reason.is_empty());
        assert!(f.root.join("A").join("B").join("b.3mf").exists());
        assert_eq!(active_files(&f.conn), 1);
        // B and its ancestor A keep their rows and directories; C is gone.
        assert_eq!(folder_rows(&f.conn), 3);
        assert!(f.root.join("A").join("B").is_dir() && f.root.join("A").is_dir());
        assert!(!f.root.join("A").join("C").exists());
        assert!(result.kept_on_disk.iter().any(|p| Path::new(p) == f.root.join("A").join("B")));
    }

    /// Adds catalog rows to folder A that a crafted backup could contain, and returns
    /// `(row path, expected reason, content)`. Everything that must survive sits outside
    /// A or is not a model.
    fn plant_manipulated_rows(f: &Fixture) -> Vec<(PathBuf, &'static str, Vec<u8>)> {
        let outside = unique_test_dir("folder_trash_manip_outside");
        let mut planted = Vec::new();
        let mut add_row = |row: PathBuf, disk: PathBuf, reason: &'static str, content: &[u8]| {
            std::fs::write(&disk, content).unwrap();
            db::test_insert_minimal_file(&f.conn, &row.to_string_lossy(), Some(f.a)).unwrap();
            planted.push((disk, reason, content.to_vec()));
        };
        let p = outside.join("Rechnung.pdf");
        add_row(p.clone(), p, "outside_folder", b"invoice");
        let p = outside.join("other.3mf");
        add_row(p.clone(), p, "outside_folder", b"outside model");
        // `..` keeps the textual prefix of the folder but leaves it.
        add_row(f.root.join("A").join("..").join("escaped.3mf"), f.root.join("escaped.3mf"), "outside_folder", b"escaped");
        // Inside the folder, but not a model.
        let p = f.root.join("A").join("notes.txt");
        add_row(p.clone(), p, "not_a_model", b"notes");
        // Sibling folder whose name starts like the folder name.
        std::fs::create_dir_all(f.root.join("A2")).unwrap();
        let p = f.root.join("A2").join("x.3mf");
        add_row(p.clone(), p, "outside_folder", b"sibling");
        #[cfg(unix)]
        {
            let target = outside.join("linked.3mf");
            std::fs::write(&target, b"link target").unwrap();
            let link = f.root.join("A").join("link.3mf");
            std::os::unix::fs::symlink(&target, &link).unwrap();
            db::test_insert_minimal_file(&f.conn, &link.to_string_lossy(), Some(f.a)).unwrap();
            planted.push((link, "symlink", Vec::new()));
        }
        planted
    }

    #[test]
    fn manipulated_rows_never_move_files_outside_the_folder_or_non_models() {
        let f = fixture("folder_trash_manipulated");
        let planted = plant_manipulated_rows(&f);
        let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
        assert_eq!(result.trashed_model_count, 3, "only the three real models move: {result:?}");
        for (path, reason, content) in &planted {
            assert!(std::fs::symlink_metadata(path).is_ok(), "{} must stay", path.display());
            if *reason != "symlink" {
                assert_eq!(&std::fs::read(path).unwrap(), content, "{} must be unchanged", path.display());
            } else {
                assert_eq!(std::fs::read(path).unwrap(), b"link target");
            }
            let failure = result.failed.iter().find(|x| Path::new(&x.path) == path.as_path() || (path.ends_with("escaped.3mf") && x.path.ends_with("escaped.3mf")));
            assert_eq!(failure.map(|x| x.reason.as_str()), Some(*reason), "{}", path.display());
        }
        assert_eq!(std::fs::read_dir(&f.trash).unwrap().count(), 3, "nothing but the models in the trash");
        // The folder that still holds refused rows keeps its row and directory.
        assert!(f.root.join("A").is_dir());
        let rows: i64 = f.conn.query_row("SELECT COUNT(*) FROM folders WHERE id = ?1", [f.a], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn summary_counts_only_files_the_delete_would_really_move() {
        let f = fixture("folder_trash_manipulated_summary");
        plant_manipulated_rows(&f);
        let summary = folder_summary(&f, f.a);
        assert_eq!(summary.model_count, 3);
    }

    #[test]
    fn summary_counts_subfolders_models_and_other_files() {
        let f = fixture("folder_trash_summary");
        std::fs::write(f.root.join("A").join("readme.txt"), b"x").unwrap();
        std::fs::create_dir(f.root.join("A").join("B").join("unknown")).unwrap();
        std::fs::write(f.root.join("A").join("B").join("unknown").join("y.bin"), b"y").unwrap();
        // A trashed model is not a model any more.
        let extra = f.root.join("A").join("gone.3mf");
        std::fs::write(&extra, b"z").unwrap();
        let gone = db::test_insert_minimal_file(&f.conn, &extra.to_string_lossy(), Some(f.a)).unwrap();
        db::soft_delete_file(&f.conn, gone, Some("/t"), "now").unwrap();
        let summary = folder_summary(&f, f.a);
        assert_eq!(summary.name, "A");
        assert_eq!(summary.subfolder_count, 2);
        assert_eq!(summary.model_count, 3);
        // readme.txt, y.bin and gone.3mf (soft-deleted rows do not count as known).
        assert_eq!(summary.other_file_count, 3);
        assert!(!summary.other_file_count_truncated);
        assert!(!summary.is_catalog_root_or_ancestor);
    }

    #[test]
    fn summary_scan_is_capped() {
        let tmp = unique_test_dir("folder_trash_cap");
        for i in 0..OTHER_FILES_SCAN_LIMIT + 5 {
            std::fs::write(tmp.join(format!("f{i}")), b"").unwrap();
        }
        let (count, truncated) = count_other_files(&tmp, &HashSet::new());
        assert_eq!(count, OTHER_FILES_SCAN_LIMIT as i64);
        assert!(truncated);
        let _ = std::fs::remove_dir_all(tmp);
    }
}
