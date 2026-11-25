use super::*;
use katalog_core::folder_trash::{self as core, ModelTrash, StdFolderTrashFs};
use tauri::Manager;

#[cfg(test)]
const ROOT_ERROR: ErrorCode = ErrorCode::CatalogRootNotDeletable;

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

// Single-model moves stay shared with the desktop delete command, including
// companion handling and move-back compensation.
struct DesktopModelTrash<'a>(&'a Path);

impl ModelTrash for DesktopModelTrash<'_> {
    fn trash_model(
        &self,
        conn: &Connection,
        file: &db::models::FileRecord,
    ) -> katalog_core::write::WriteResult<()> {
        trash::delete_file_with_conn(conn, file, file.id, self.0)
            .map_err(|error| error.to_string().into())
    }
}

fn folder_trash_summary_with_conn(
    conn: &Connection,
    folder_id: &str,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<FolderTrashSummary> {
    let summary =
        core::folder_trash_summary_with_conn(&StdFolderTrashFs, conn, folder_id, sensitive_dirs)?;
    Ok(FolderTrashSummary {
        name: summary.name,
        subfolder_count: summary.subfolder_count,
        model_count: summary.model_count,
        other_file_count: summary.other_file_count,
        other_file_count_truncated: summary.other_file_count_truncated,
        is_catalog_root_or_ancestor: summary.is_catalog_root_or_ancestor,
    })
}

fn trash_folder_with_conn(
    conn: &Connection,
    folder_id: &str,
    trash_dir: &Path,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<FolderTrashResult> {
    let result = core::trash_folder_with_conn(
        &StdFolderTrashFs,
        &DesktopModelTrash(trash_dir),
        conn,
        folder_id,
        sensitive_dirs,
    )?;
    Ok(FolderTrashResult {
        trashed_model_count: result.trashed_model_count,
        failed: result
            .failed
            .into_iter()
            .map(|failure| FolderTrashFailure {
                file_id: failure.file_id,
                path: failure.path,
                reason: failure.reason,
            })
            .collect(),
        removed_folder_count: result.removed_folder_count,
        kept_on_disk: result.kept_on_disk,
    })
}

/// Test helper for the restore tests: trashes a folder tree and returns how many models moved.
#[cfg(test)]
pub(super) fn trash_folder_for_test(conn: &Connection, folder_id: i64, trash_dir: &Path) -> i64 {
    trash_folder_with_conn(conn, &folder_id.to_string(), trash_dir, &[]).unwrap().trashed_model_count
}

/// Test helper for the golden scenarios: the whole result of the folder trash as JSON.
#[cfg(test)]
pub(super) fn trash_folder_result_for_test(conn: &Connection, folder_id: i64, trash_dir: &Path, sensitive_dirs: &[PathBuf]) -> CmdResult<serde_json::Value> {
    trash_folder_with_conn(conn, &folder_id.to_string(), trash_dir, sensitive_dirs).map(|result| serde_json::to_value(result).unwrap())
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
        assert!(err.is(ROOT_ERROR, &[]), "{err:?}");
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
}
