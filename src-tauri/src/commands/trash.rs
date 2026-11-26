//! Desktop side of the model trash (issue #20, package P7): the Tauri commands, the locks and
//! thin wrappers with the old signatures. The actions themselves live in
//! `katalog_core::model_trash`.
use super::*;
use katalog_core::companion_actions::CompanionFs;
use katalog_core::model_trash::{self as core, ModelTrashFs};

/// The model trash's disk access (core trait) on top of the desktop's `Fs` boundary. Every
/// call goes to the same `Fs` method the code used before the actions moved into the core,
/// so the call order seen by `FaultFs` is unchanged; `create_dir_all` and the `exists` of a
/// restore never had an `Fs` argument and still go to `std::fs` directly.
pub(super) struct ModelTrashFsAdapter<'a> {
    fs: &'a dyn Fs,
    companions: companions::CompanionFsAdapter<'a>,
}

impl<'a> ModelTrashFsAdapter<'a> {
    pub(super) fn new(fs: &'a dyn Fs) -> Self {
        Self { fs, companions: companions::CompanionFsAdapter(fs) }
    }
}

impl ModelTrashFs for ModelTrashFsAdapter<'_> {
    fn metadata(&self, path: &Path) -> std::io::Result<std::fs::Metadata> {
        self.fs.metadata(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.fs.exists(path)
    }
    fn create_dir_all(&self, path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path)
    }
    fn remove_dir(&self, path: &Path) -> std::io::Result<()> {
        self.fs.remove_dir(path)
    }
    fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        self.fs.remove_file(path)
    }
    fn move_file(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        move_file_with(self.fs, from, to)
    }
    fn companions(&self) -> &dyn CompanionFs {
        &self.companions
    }
}

pub(super) fn delete_missing_file_with_conn(conn: &Connection, id: i64) -> CmdResult<()> {
    core::delete_missing_file(conn, id).map_err(Into::into)
}

/// Core logic of `delete_file` with an already loaded `FileRecord`, without `State`, so it's testable.
pub(super) fn delete_file_with_conn(
    conn: &Connection,
    file: &db::models::FileRecord,
    id: i64,
    trash_dir: &std::path::Path,
) -> CmdResult<()> {
    delete_file_with_conn_fs(&RealFs, conn, file, id, trash_dir)
}

/// `delete_file_with_conn` on an injected file system (tests).
pub(super) fn delete_file_with_conn_fs(
    fs: &dyn Fs,
    conn: &Connection,
    file: &db::models::FileRecord,
    id: i64,
    trash_dir: &std::path::Path,
) -> CmdResult<()> {
    core::delete_file(&ModelTrashFsAdapter::new(fs), conn, file, id, trash_dir).map_err(Into::into)
}
#[tauri::command]
pub fn delete_file(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    delete_file_with_conn(&conn, &file, id, &state.trash_dir)
}
#[tauri::command]
pub fn delete_files(state: State<AppState>, file_ids: Vec<String>) -> CmdResult<()> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let conn = lock_db(&state)?;
    delete_files_with_conn(&conn, &state.trash_dir, file_ids)
}
/// Core logic of `delete_files`, without `State`, so it's testable.
pub(super) fn delete_files_with_conn(conn: &Connection, trash_dir: &std::path::Path, file_ids: Vec<String>) -> CmdResult<()> {
    core::delete_files(&ModelTrashFsAdapter::new(&RealFs), conn, trash_dir, file_ids).map_err(Into::into)
}
#[tauri::command]
pub fn list_trash(state: State<AppState>) -> CmdResult<Vec<ModelFileDto>> {
    let conn = lock_db(&state)?;
    katalog_core::read::list_trash(&conn).map_err(Into::into)
}
/// Core logic of `restore_file` with an already loaded `FileRecord`, without `State`, so it's testable.
pub(super) fn restore_file_with_conn(
    conn: &Connection,
    file: &db::models::FileRecord,
    id: i64,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<()> {
    core::restore_file(&ModelTrashFsAdapter::new(&RealFs), conn, file, id, sensitive_dirs).map_err(Into::into)
}
#[tauri::command]
pub fn restore_file(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    restore_file_with_conn(&conn, &file, id, &state.sensitive_dirs)
}
#[tauri::command]
pub fn delete_file_permanently(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    delete_file_permanently_with_conn(&conn, id, &state.trash_dir)
}
/// Core logic of `delete_file_permanently`, without `State`, so it's testable.
pub(super) fn delete_file_permanently_with_conn(conn: &Connection, id: i64, trash_dir: &std::path::Path) -> CmdResult<()> {
    core::delete_file_permanently(&ModelTrashFsAdapter::new(&RealFs), conn, id, trash_dir).map_err(Into::into)
}
#[tauri::command]
pub fn empty_trash(state: State<AppState>) -> CmdResult<()> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let conn = lock_db(&state)?;
    empty_trash_with_conn(&conn, &state.trash_dir)
}
/// Core logic of `empty_trash`, without `State`, so it's testable.
pub(super) fn empty_trash_with_conn(conn: &Connection, trash_dir: &std::path::Path) -> CmdResult<()> {
    core::empty_trash(&ModelTrashFsAdapter::new(&RealFs), conn, trash_dir).map_err(Into::into)
}
/// At startup: removes trash entries older than 7 days. Single errors are
/// logged and skipped.
pub fn purge_expired_trash_on_startup(conn: &Connection, trash_dir: &std::path::Path) {
    core::purge_expired_trash(&ModelTrashFsAdapter::new(&RealFs), conn, trash_dir, chrono::Utc::now());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_delete_preserves_the_existing_trash_reference() {
        let conn = db::connect_in_memory().unwrap();
        let id = db::test_insert_minimal_file(&conn, "missing.stl", None).unwrap();
        db::soft_delete_file(&conn, id, Some("existing-trash"), "now").unwrap();
        let file = db::get_file(&conn, id).unwrap().unwrap();
        assert!(delete_file_with_conn(&conn, &file, id, Path::new("unused")).is_err());
        assert_eq!(db::get_file(&conn, id).unwrap().unwrap().trash_path.as_deref(), Some("existing-trash"));
    }

    #[test]
    fn delete_file_compensates_when_soft_delete_fails() {
        // The compensation must kick in when soft_delete_file fails AFTER the move.
        let dir = unique_test_dir("delete_file_compensation");
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("model.3mf");
        std::fs::write(&src_path, b"CONTENT").unwrap();

        let conn = db::connect_in_memory().unwrap();
        let file_id = db::test_insert_minimal_file(&conn, &src_path.to_string_lossy(), None).unwrap();
        // Hard-delete the row first: the UPDATE hits 0 rows, but the FileRecord is
        // already loaded.
        let file = db::get_file(&conn, file_id).unwrap().unwrap();
        db::delete_file(&conn, file_id).unwrap();

        let trash_dir = unique_test_dir("delete_file_compensation_trash");
        std::fs::create_dir_all(&trash_dir).unwrap();
        let result = delete_file_with_conn(&conn, &file, file_id, &trash_dir);

        assert!(result.is_err(), "must surface the soft_delete_file failure (0 rows affected)");
        assert!(src_path.exists(), "source file must be moved back from trash after soft_delete_file failed");
    }
    #[test]
    fn bulk_delete_compensates_for_one_failing_file_without_stranding_it_in_trash() {
        // Multi-select: the UPDATE fails for file 2. File 1 ends up in the trash,
        // file 2 back at its original place, and the call still succeeds.
        let dir = unique_test_dir("bulk_delete_compensation");
        std::fs::create_dir_all(&dir).unwrap();
        let trash_dir = unique_test_dir("bulk_delete_compensation_trash");
        std::fs::create_dir_all(&trash_dir).unwrap();

        let path1 = dir.join("model1.3mf");
        let path2 = dir.join("model2.3mf");
        std::fs::write(&path1, b"CONTENT1").unwrap();
        std::fs::write(&path2, b"CONTENT2").unwrap();

        let conn = db::connect_in_memory().unwrap();
        let id1 = db::test_insert_minimal_file(&conn, &path1.to_string_lossy(), None).unwrap();
        let id2 = db::test_insert_minimal_file(&conn, &path2.to_string_lossy(), None).unwrap();

        // A trigger makes only the UPDATE for id2 fail after the file was moved
        // (a hard delete wouldn't work, the file would be skipped earlier).
        conn.execute_batch(&format!(
            "CREATE TRIGGER block_soft_delete_id2 BEFORE UPDATE ON files
             WHEN NEW.id = {id2} AND NEW.deleted_at IS NOT NULL
             BEGIN SELECT RAISE(ABORT, 'simulierter Fehler bei soft_delete_file'); END;"
        ))
        .unwrap();

        let result = delete_files_with_conn(&conn, &trash_dir, vec![id1.to_string(), id2.to_string()]);

        assert!(result.is_ok(), "bulk delete must not fail the whole batch on one item's error: {result:?}");

        // id1: landed in the trash as usual.
        let file1 = db::get_file(&conn, id1).unwrap().unwrap();
        assert!(file1.deleted_at.is_some(), "file 1 must be soft-deleted");
        assert!(!path1.exists(), "file 1 must have been moved into the trash");

        // id2 must be back at its original place, not orphaned in the trash.
        assert!(path2.exists(), "file 2 must be moved back out of trash after the compensating rollback");
        let trash_entry = trash_dir.join(format!("{id2}-model2.3mf"));
        assert!(!trash_entry.exists(), "file 2 must not remain stranded in the trash directory");
    }

    #[test]
    fn restore_file_compensates_when_db_update_fails() {
        let dir = unique_test_dir("restore_file_compensation");
        std::fs::create_dir_all(&dir).unwrap();
        let trash_dir = unique_test_dir("restore_file_compensation_trash");
        std::fs::create_dir_all(&trash_dir).unwrap();

        let original_path = dir.join("model.3mf");
        let trash_path = trash_dir.join("model.3mf");
        std::fs::write(&trash_path, b"CONTENT").unwrap();

        let conn = db::connect_in_memory().unwrap();
        let file_id = db::test_insert_minimal_file(&conn, &original_path.to_string_lossy(), None).unwrap();
        let deleted_at = chrono::Utc::now().to_rfc3339();
        db::soft_delete_file(&conn, file_id, Some(&trash_path.to_string_lossy()), &deleted_at).unwrap();

        // The trigger fails exactly the change from deleted to restored, after the
        // file was already moved back.
        conn.execute_batch(
            "CREATE TRIGGER block_restore BEFORE UPDATE ON files
             WHEN NEW.deleted_at IS NULL AND OLD.deleted_at IS NOT NULL
             BEGIN SELECT RAISE(ABORT, 'simulierter Fehler bei restore_file'); END;",
        )
        .unwrap();

        let file = db::get_file(&conn, file_id).unwrap().unwrap();
        let result = restore_file_with_conn(&conn, &file, file_id, &[]);

        assert!(result.is_err(), "must surface the db::restore_file failure raised by the trigger");
        assert!(trash_path.exists(), "file must be moved back into the trash after db::restore_file failed");
        assert!(!original_path.exists(), "destination must not exist after the rollback");
        let still_deleted: Option<String> = conn
            .query_row("SELECT deleted_at FROM files WHERE id = ?1", [file_id], |r| r.get(0))
            .unwrap();
        assert!(still_deleted.is_some(), "db must still show the file as deleted after the failed restore");
    }
}

#[cfg(test)]
mod catalog_removal_restore_tests {
    use super::*;

    #[test]
    fn restore_from_trash_after_removing_its_catalog_folder() {
        let conn = db::connect_in_memory().unwrap();
        let dir = unique_test_dir("restore_after_catalog_removal");
        let models = dir.join("models");
        let trash = dir.join("trash");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::create_dir_all(&trash).unwrap();
        let path = models.join("model.stl");
        std::fs::write(&path, b"original model bytes").unwrap();
        let folder = db::insert_folder_with_parent(&conn, "models", None, models.to_str().unwrap()).unwrap();
        let id = db::test_insert_minimal_file(&conn, path.to_str().unwrap(), Some(folder)).unwrap();
        delete_file_with_conn(&conn, &db::get_file(&conn, id).unwrap().unwrap(), id, &trash).unwrap();
        super::super::catalog_removal::remove_folder_with_conn(&conn, &folder.to_string()).unwrap();
        let trashed = db::get_file(&conn, id).unwrap().unwrap();
        assert_eq!(trashed.folder_id, None);
        assert!(trashed.deleted_at.is_some());
        restore_file_with_conn(&conn, &trashed, id, &[]).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"original model bytes");
        assert!(db::get_file(&conn, id).unwrap().unwrap().deleted_at.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod restore_into_missing_folder_tests {
    use super::*;

    /// Catalog base with folder rows base -> A -> B and one model in B, trashed together with its folders.
    fn trashed_with_folders(name: &str) -> (Connection, PathBuf, PathBuf, i64, i64) {
        let tmp = unique_test_dir(name);
        let base = tmp.join("catalog");
        let trash = tmp.join("trash");
        let dir_b = base.join("A").join("B");
        std::fs::create_dir_all(&dir_b).unwrap();
        std::fs::create_dir_all(&trash).unwrap();
        let conn = db::connect_in_memory().unwrap();
        db::printer_link::set_setting(&conn, folders::SETTING_CATALOG_BASE_DIR, &base.to_string_lossy()).unwrap();
        let base_id = db::insert_folder_with_parent(&conn, "catalog", None, &base.to_string_lossy()).unwrap();
        let a = db::insert_folder_with_parent(&conn, "A", Some(base_id), &base.join("A").to_string_lossy()).unwrap();
        let b = db::insert_folder_with_parent(&conn, "B", Some(a), &dir_b.to_string_lossy()).unwrap();
        let path = dir_b.join("m.3mf");
        std::fs::write(&path, b"bytes").unwrap();
        let id = db::test_insert_minimal_file(&conn, &path.to_string_lossy(), Some(b)).unwrap();
        let result = super::super::folder_trash::trash_folder_for_test(&conn, a, &trash);
        assert_eq!(result, 1);
        assert!(!base.join("A").exists());
        (conn, base, trash, id, a)
    }

    #[test]
    fn restore_recreates_the_folder_on_disk_and_in_the_catalog() {
        let (conn, base, _trash, id, _a) = trashed_with_folders("restore_missing_folder");
        let file = db::get_file(&conn, id).unwrap().unwrap();
        assert_eq!(file.folder_id, None);
        restore_file_with_conn(&conn, &file, id, &[]).unwrap();
        let path = base.join("A").join("B").join("m.3mf");
        assert_eq!(std::fs::read(&path).unwrap(), b"bytes");
        let restored = db::get_file(&conn, id).unwrap().unwrap();
        assert!(restored.deleted_at.is_none());
        let folder: (String, Option<i64>) = conn
            .query_row(
                "SELECT f.path, p.parent_id FROM folders f LEFT JOIN folders p ON p.id = f.parent_id WHERE f.id = ?1",
                [restored.folder_id.expect("file must be assigned to a folder again")],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(PathBuf::from(folder.0), base.join("A").join("B"));
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 3, "base, A and B again, without duplicates");
    }

    /// Two models in A, trashed together with their folder.
    fn two_models_trashed(name: &str) -> (Connection, PathBuf, [i64; 2]) {
        let tmp = unique_test_dir(name);
        let base = tmp.join("catalog");
        let trash = tmp.join("trash");
        std::fs::create_dir_all(base.join("A")).unwrap();
        std::fs::create_dir_all(&trash).unwrap();
        let conn = db::connect_in_memory().unwrap();
        db::printer_link::set_setting(&conn, folders::SETTING_CATALOG_BASE_DIR, &base.to_string_lossy()).unwrap();
        let base_id = db::insert_folder_with_parent(&conn, "catalog", None, &base.to_string_lossy()).unwrap();
        let a = db::insert_folder_with_parent(&conn, "A", Some(base_id), &base.join("A").to_string_lossy()).unwrap();
        let ids = ["one.3mf", "two.3mf"].map(|n| {
            let path = base.join("A").join(n);
            std::fs::write(&path, n.as_bytes()).unwrap();
            db::test_insert_minimal_file(&conn, &path.to_string_lossy(), Some(a)).unwrap()
        });
        assert_eq!(super::super::folder_trash::trash_folder_for_test(&conn, a, &trash), 2);
        (conn, base, ids)
    }

    fn restore(conn: &Connection, id: i64) {
        let file = db::get_file(conn, id).unwrap().unwrap();
        restore_file_with_conn(conn, &file, id, &[]).unwrap();
    }

    fn folder_of(conn: &Connection, id: i64) -> Option<i64> {
        db::get_file(conn, id).unwrap().unwrap().folder_id
    }

    #[test]
    fn every_restored_model_of_a_deleted_folder_gets_its_folder_back() {
        let (conn, base, [one, two]) = two_models_trashed("restore_two_models");
        restore(&conn, one);
        restore(&conn, two);
        let first = folder_of(&conn, one).expect("first model is assigned");
        assert_eq!(folder_of(&conn, two), Some(first), "the second model must join the recreated folder");
        let path: String = conn.query_row("SELECT path FROM folders WHERE id = ?1", [first], |r| r.get(0)).unwrap();
        assert_eq!(PathBuf::from(path), base.join("A"));
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 2, "base and A, no duplicates");
    }

    #[test]
    fn restore_fixes_a_folder_assignment_that_points_to_another_folder() {
        let (conn, base, [one, two]) = two_models_trashed("restore_stale_assignment");
        restore(&conn, one);
        let a = folder_of(&conn, one).unwrap();
        let other = db::insert_folder_with_parent(&conn, "Other", None, &base.join("Other").to_string_lossy()).unwrap();
        conn.execute("UPDATE files SET folder_id = ?1 WHERE id = ?2", [other, two]).unwrap();
        conn.execute("UPDATE files SET deleted_at = '2026-01-01T00:00:00Z' WHERE id = ?1", [two]).unwrap();
        restore(&conn, two);
        assert_eq!(folder_of(&conn, two), Some(a));
    }

    #[test]
    fn restore_recreates_a_trashed_top_level_folder_without_a_catalog_base() {
        let tmp = unique_test_dir("restore_top_level_folder");
        let trash = tmp.join("trash");
        let technik = tmp.join("Technik2");
        let sub = technik.join("Sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(&trash).unwrap();
        let conn = db::connect_in_memory().unwrap();
        // The trashed folder is a catalog root of its own; no base folder setting exists.
        let t = db::insert_folder_with_parent(&conn, "Technik2", None, &technik.to_string_lossy()).unwrap();
        let s = db::insert_folder_with_parent(&conn, "Sub", Some(t), &sub.to_string_lossy()).unwrap();
        let path = sub.join("m.3mf");
        std::fs::write(&path, b"bytes").unwrap();
        let id = db::test_insert_minimal_file(&conn, &path.to_string_lossy(), Some(s)).unwrap();
        // The folder is the catalog root of the file; make it deletable by trashing the model with its rows.
        super::super::folder_trash::trash_folder_for_test(&conn, t, &trash);
        assert!(!technik.exists());
        let file = db::get_file(&conn, id).unwrap().unwrap();
        restore_file_with_conn(&conn, &file, id, &[]).unwrap();
        assert!(path.exists());
        let folder_id = db::get_file(&conn, id).unwrap().unwrap().folder_id.expect("assigned to a folder again");
        let (name, parent): (String, Option<i64>) = conn
            .query_row("SELECT name, parent_id FROM folders WHERE id = ?1", [folder_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(name, "Sub");
        let parent = parent.expect("Sub hangs below the recreated Technik2");
        let top: (String, Option<i64>) = conn
            .query_row("SELECT name, parent_id FROM folders WHERE id = ?1", [parent], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(top, ("Technik2".to_string(), None));
    }

    #[test]
    fn restore_never_recreates_a_folder_row_the_user_removed_while_the_directory_exists() {
        // Single models trashed one by one (no folder trash), then the user removes the
        // folder from the catalog; the directory still exists.
        let tmp = unique_test_dir("restore_removed_row");
        let base = tmp.join("catalog");
        let trash = tmp.join("trash");
        std::fs::create_dir_all(base.join("A")).unwrap();
        std::fs::create_dir_all(&trash).unwrap();
        let conn = db::connect_in_memory().unwrap();
        db::printer_link::set_setting(&conn, folders::SETTING_CATALOG_BASE_DIR, &base.to_string_lossy()).unwrap();
        let base_id = db::insert_folder_with_parent(&conn, "catalog", None, &base.to_string_lossy()).unwrap();
        let a = db::insert_folder_with_parent(&conn, "A", Some(base_id), &base.join("A").to_string_lossy()).unwrap();
        let ids = ["one.3mf", "two.3mf"].map(|n| {
            let path = base.join("A").join(n);
            std::fs::write(&path, n.as_bytes()).unwrap();
            let id = db::test_insert_minimal_file(&conn, &path.to_string_lossy(), Some(a)).unwrap();
            delete_file_with_conn(&conn, &db::get_file(&conn, id).unwrap().unwrap(), id, &trash).unwrap();
            id
        });
        super::super::catalog_removal::remove_folder_with_conn(&conn, &a.to_string()).unwrap();
        let rows_before: i64 = conn.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0)).unwrap();
        for id in ids {
            restore(&conn, id);
        }
        let rows_after: i64 = conn.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0)).unwrap();
        assert_eq!(rows_after, rows_before);
        assert_eq!(folder_of(&conn, ids[0]), None);
        assert_eq!(folder_of(&conn, ids[1]), None);
        assert!(base.join("A").join("one.3mf").exists() && base.join("A").join("two.3mf").exists());
    }

    #[test]
    fn restore_after_folder_trash_with_a_foreign_file_recreates_the_folder_row() {
        let tmp = unique_test_dir("restore_foreign_file_stays");
        let base = tmp.join("catalog");
        let trash = tmp.join("trash");
        let dir = base.join("Werkstatt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&trash).unwrap();
        let conn = db::connect_in_memory().unwrap();
        db::printer_link::set_setting(&conn, folders::SETTING_CATALOG_BASE_DIR, &base.to_string_lossy()).unwrap();
        let base_id = db::insert_folder_with_parent(&conn, "catalog", None, &base.to_string_lossy()).unwrap();
        let w = db::insert_folder_with_parent(&conn, "Werkstatt", Some(base_id), &dir.to_string_lossy()).unwrap();
        std::fs::write(dir.join("notiz.txt"), b"foreign").unwrap();
        let path = dir.join("Prisma.stl");
        std::fs::write(&path, b"bytes").unwrap();
        let id = db::test_insert_minimal_file(&conn, &path.to_string_lossy(), Some(w)).unwrap();
        assert_eq!(super::super::folder_trash::trash_folder_for_test(&conn, w, &trash), 1);
        assert!(dir.join("notiz.txt").exists(), "the directory stays because of the foreign file");
        restore(&conn, id);
        let folder_id = folder_of(&conn, id).expect("model must hang at a folder row again");
        let (path_db, parent): (String, Option<i64>) = conn
            .query_row("SELECT path, parent_id FROM folders WHERE id = ?1", [folder_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(PathBuf::from(path_db), dir);
        assert_eq!(parent, Some(base_id));
        assert_eq!(std::fs::read(dir.join("notiz.txt")).unwrap(), b"foreign");
        let marks: i64 = conn.query_row("SELECT COUNT(*) FROM folder_trash_origin", [], |r| r.get(0)).unwrap();
        assert_eq!(marks, 0, "the mark is used up by the restore");
    }

    #[test]
    fn restore_into_a_sensitive_missing_folder_is_refused() {
        let (conn, base, trash, id, _a) = trashed_with_folders("restore_missing_sensitive");
        let file = db::get_file(&conn, id).unwrap().unwrap();
        let result = restore_file_with_conn(&conn, &file, id, std::slice::from_ref(&base));
        assert!(result.is_err());
        assert!(!base.join("A").exists(), "no directory may be created in a sensitive location");
        assert!(db::get_file(&conn, id).unwrap().unwrap().deleted_at.is_some());
        assert_eq!(std::fs::read_dir(trash).unwrap().count(), 1, "file stays in the trash");
    }

    #[test]
    fn failed_restore_removes_the_directories_it_created() {
        let (conn, base, trash, id, _a) = trashed_with_folders("restore_missing_rollback");
        conn.execute_batch(
            "CREATE TRIGGER block_restore BEFORE UPDATE ON files
             WHEN NEW.deleted_at IS NULL AND OLD.deleted_at IS NOT NULL
             BEGIN SELECT RAISE(ABORT, 'simulated restore failure'); END;",
        )
        .unwrap();
        let file = db::get_file(&conn, id).unwrap().unwrap();
        assert!(restore_file_with_conn(&conn, &file, id, &[]).is_err());
        assert!(!base.join("A").exists(), "recreated directories must be undone");
        assert_eq!(std::fs::read_dir(trash).unwrap().count(), 1);
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1, "only the base row remains");
    }
}
