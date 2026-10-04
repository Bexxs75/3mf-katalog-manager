use super::*;
use rusqlite::OptionalExtension;
use tauri::Manager;

const SUBTREE: &str = "WITH RECURSIVE subtree(id) AS (SELECT id FROM folders WHERE id = ?1 UNION SELECT f.id FROM folders f JOIN subtree s ON f.parent_id = s.id)";
const BASE_DIR_ERROR: &str = "Das ist der Speicherort des Katalogs. Zum Leeren „Katalog zurücksetzen“ in den Einstellungen verwenden.";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRemovalSummary {
    name: String,
    subfolder_count: i64,
    model_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRemovalCount {
    model_count: i64,
    folder_count: i64,
}

fn parse_id(id: &str) -> CmdResult<i64> {
    id.parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| CmdError::expected("Ungültige Katalog-ID."))
}

fn remove_files_with_conn(conn: &Connection, file_ids: Vec<String>) -> CmdResult<usize> {
    let ids = file_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<CmdResult<HashSet<_>>>()?;
    let tx = conn.unchecked_transaction().map_err(DbError::from)?;
    for id in &ids {
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM files WHERE id = ?1 AND deleted_at IS NULL)",
                [id],
                |r| r.get(0),
            )
            .map_err(DbError::from)?;
        if !exists {
            return Err(CmdError::expected(
                "Modell nicht gefunden oder bereits im Papierkorb.",
            ));
        }
    }
    for id in &ids {
        tx.execute("DELETE FROM files WHERE id = ?1", [id])
            .map_err(DbError::from)?;
    }
    tx.commit().map_err(DbError::from)?;
    log::info!(target: "catalog", "{} Modelle aus dem Katalog entfernt", ids.len());
    Ok(ids.len())
}

fn folder_summary_with_conn(conn: &Connection, folder_id: &str) -> CmdResult<FolderRemovalSummary> {
    let id = parse_id(folder_id)?;
    let name = conn
        .query_row("SELECT name FROM folders WHERE id = ?1", [id], |r| r.get(0))
        .optional()
        .map_err(DbError::from)?
        .ok_or_else(|| CmdError::expected("Ordner nicht gefunden."))?;
    let subfolder_count = conn
        .query_row(
            &format!("{SUBTREE} SELECT COUNT(*) - 1 FROM subtree"),
            [id],
            |r| r.get(0),
        )
        .map_err(DbError::from)?;
    let model_count = conn.query_row(&format!("{SUBTREE} SELECT COUNT(*) FROM files WHERE folder_id IN (SELECT id FROM subtree) AND deleted_at IS NULL"), [id], |r| r.get(0)).map_err(DbError::from)?;
    Ok(FolderRemovalSummary {
        name,
        subfolder_count,
        model_count,
    })
}

pub(super) fn remove_folder_with_conn(
    conn: &Connection,
    folder_id: &str,
) -> CmdResult<CatalogRemovalCount> {
    let id = parse_id(folder_id)?;
    let tx = conn.unchecked_transaction().map_err(DbError::from)?;
    let summary = folder_summary_with_conn(&tx, folder_id)?;
    // Protect ancestors too: cascading their removal must not bypass the base-directory guard.
    let contains_base: bool = tx.query_row(&format!("{SUBTREE} SELECT EXISTS(SELECT 1 FROM folders WHERE id IN (SELECT id FROM subtree) AND path = (SELECT value FROM app_settings WHERE key = ?2))"), rusqlite::params![id, folders::SETTING_CATALOG_BASE_DIR], |r| r.get(0)).map_err(DbError::from)?;
    if contains_base {
        return Err(CmdError::expected(BASE_DIR_ERROR));
    }
    tx.execute(&format!("{SUBTREE} DELETE FROM files WHERE folder_id IN (SELECT id FROM subtree) AND deleted_at IS NULL"), [id]).map_err(DbError::from)?;
    tx.execute("DELETE FROM folders WHERE id = ?1", [id])
        .map_err(DbError::from)?;
    tx.commit().map_err(DbError::from)?;
    let result = CatalogRemovalCount {
        model_count: summary.model_count,
        folder_count: summary.subfolder_count + 1,
    };
    log::info!(target: "catalog", "{} Modelle und {} Ordner aus dem Katalog entfernt", result.model_count, result.folder_count);
    Ok(result)
}

fn reset_with_conn(conn: &Connection) -> CmdResult<CatalogRemovalCount> {
    let tx = conn.unchecked_transaction().map_err(DbError::from)?;
    let folder_count = tx
        .query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0))
        .map_err(DbError::from)?;
    let model_count = tx
        .execute("DELETE FROM files WHERE deleted_at IS NULL", [])
        .map_err(DbError::from)? as i64;
    // Reset detaches the retained job history from the catalog, including trash entries.
    tx.execute("UPDATE printer_jobs SET booked_file_id = NULL", [])
        .map_err(DbError::from)?;
    tx.execute("DELETE FROM folders", [])
        .map_err(DbError::from)?;
    tx.execute("DELETE FROM tags", []).map_err(DbError::from)?;
    tx.execute("DELETE FROM collections", [])
        .map_err(DbError::from)?;
    tx.execute(
        "DELETE FROM app_settings WHERE key = ?1",
        [folders::SETTING_CATALOG_BASE_DIR],
    )
    .map_err(DbError::from)?;
    tx.commit().map_err(DbError::from)?;
    log::info!(target: "catalog", "Katalog zurückgesetzt: {model_count} Modelle, {folder_count} Ordner");
    Ok(CatalogRemovalCount {
        model_count,
        folder_count,
    })
}

#[tauri::command]
pub async fn remove_files_from_catalog(
    app: tauri::AppHandle,
    file_ids: Vec<String>,
) -> CmdResult<usize> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _catalog_share = state.import_jobs.gate.exclusive()?;
        let conn = lock_db(&state)?;
        remove_files_with_conn(&conn, file_ids)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn folder_removal_summary(
    app: tauri::AppHandle,
    folder_id: String,
) -> CmdResult<FolderRemovalSummary> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = lock_db(&state)?;
        folder_summary_with_conn(&conn, &folder_id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remove_folder_from_catalog(
    app: tauri::AppHandle,
    folder_id: String,
) -> CmdResult<CatalogRemovalCount> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _catalog_share = state.import_jobs.gate.exclusive()?;
        let conn = lock_db(&state)?;
        remove_folder_with_conn(&conn, &folder_id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn reset_catalog(app: tauri::AppHandle) -> CmdResult<CatalogRemovalCount> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _catalog_share = state.import_jobs.gate.exclusive()?;
        let conn = lock_db(&state)?;
        let result = reset_with_conn(&conn)?;
        app.state::<ApprovedTargets>().clear();
        app.state::<ApprovedCatalogParents>().clear();
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Connection, i64, i64, i64, i64) {
        let conn = db::connect_in_memory().unwrap();
        assert!(conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get::<_, bool>(0))
            .unwrap());
        let root = db::insert_folder(&conn, "root").unwrap();
        let child =
            db::insert_folder_with_parent(&conn, "child", Some(root), "/catalog/child").unwrap();
        let active =
            db::test_insert_minimal_file(&conn, "/catalog/child/a.stl", Some(child)).unwrap();
        let trash =
            db::test_insert_minimal_file(&conn, "/catalog/child/b.stl", Some(child)).unwrap();
        db::soft_delete_file(&conn, trash, Some("/trash/b.stl"), "today").unwrap();
        (conn, root, child, active, trash)
    }

    fn related_rows(conn: &Connection, active: i64, trash: i64) {
        conn.execute_batch("INSERT INTO tags VALUES (1, 'tag', 100);
            INSERT INTO collections VALUES (1, 'collection', 'today');
            INSERT INTO printers (id, name) VALUES (1, 'printer');
            INSERT INTO filament_spools (id, material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                VALUES (1, 'PLA', 1.75, 1000, 900, 'today');
            INSERT INTO printer_connections (printer_id, kind, address, connected_since) VALUES (1, 'moonraker', 'localhost', 1);
            INSERT INTO registered_slicers (name, executable_path, is_auto_detected) VALUES ('slicer', '/slicer', 0);").unwrap();
        for id in [active, trash] {
            conn.execute("INSERT INTO file_tags VALUES (?1, 1)", [id])
                .unwrap();
            conn.execute("INSERT INTO collection_files VALUES (1, ?1, 0)", [id])
                .unwrap();
            conn.execute(
                "INSERT INTO file_metadata VALUES (?1, 'key', 'value')",
                [id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO file_materials (file_id, name) VALUES (?1, 'PLA')",
                [id],
            )
            .unwrap();
            conn.execute("INSERT INTO print_log (file_id, printed_at, created_at) VALUES (?1, 'today', 'today')", [id]).unwrap();
        }
        conn.execute(
            "UPDATE files SET queue_position = 1 WHERE id = ?1",
            [active],
        )
        .unwrap();
        conn.execute("INSERT INTO printer_jobs (printer_id, remote_id, file_name, outcome, raw_status, ended_at, print_duration_s, used_mm, booked_file_id, booked_spool_id) VALUES (1, 'remote', 'a.stl', 'completed', 'complete', 1, 2, 3, ?1, 1)", [active]).unwrap();
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn all_removals_preserve_disk_bytes_and_related_data_consistency() {
        for mode in ["files", "folder", "reset"] {
            let (conn, root, child, active, trash) = fixture();
            related_rows(&conn, active, trash);
            let dir = unique_test_dir(&format!("catalog_remove_{mode}"));
            let subdir = dir.join("child");
            std::fs::create_dir_all(&subdir).unwrap();
            let disk_file = subdir.join("a.stl");
            let unrelated = subdir.join("not-imported.txt");
            let trash_file = dir.join("trashed.stl");
            for path in [&disk_file, &unrelated, &trash_file] {
                std::fs::write(path, b"unchanged bytes").unwrap();
            }
            conn.execute(
                "UPDATE folders SET path = ?1 WHERE id = ?2",
                rusqlite::params![subdir.to_str().unwrap(), child],
            )
            .unwrap();
            conn.execute(
                "UPDATE files SET path = ?1 WHERE id = ?2",
                rusqlite::params![disk_file.to_str().unwrap(), active],
            )
            .unwrap();
            conn.execute(
                "UPDATE files SET trash_path = ?1 WHERE id = ?2",
                rusqlite::params![trash_file.to_str().unwrap(), trash],
            )
            .unwrap();
            match mode {
                "files" => {
                    remove_files_with_conn(&conn, vec![active.to_string()]).unwrap();
                }
                "folder" => {
                    remove_folder_with_conn(&conn, &root.to_string()).unwrap();
                }
                _ => {
                    reset_with_conn(&conn).unwrap();
                }
            }
            assert!(subdir.is_dir());
            for path in [&disk_file, &unrelated, &trash_file] {
                assert_eq!(std::fs::read(path).unwrap(), b"unchanged bytes");
            }
            for table in ["file_metadata", "file_materials", "print_log"] {
                assert_eq!(count(&conn, table), 1, "{mode}: {table}");
            }
            for table in ["tags", "collections", "file_tags", "collection_files"] {
                assert_eq!(
                    count(&conn, table),
                    if mode == "reset" { 0 } else { 1 },
                    "{mode}: {table}"
                );
            }
            for table in [
                "filament_spools",
                "printers",
                "printer_connections",
                "printer_jobs",
                "registered_slicers",
            ] {
                assert_eq!(count(&conn, table), 1, "{mode}: {table}");
            }
            assert_eq!(
                conn.query_row("SELECT booked_file_id FROM printer_jobs", [], |r| r
                    .get::<_, Option<i64>>(0))
                    .unwrap(),
                None
            );
            assert_eq!(
                conn.query_row("SELECT booked_spool_id FROM printer_jobs", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert_eq!(conn.query_row("SELECT COUNT(*) FROM files WHERE deleted_at IS NULL AND queue_position IS NOT NULL", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
            assert_eq!(
                conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
                0
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn database_failure_rolls_back_folder_removal_and_reset() {
        for reset in [false, true] {
            let (conn, root, _, active, _) = fixture();
            conn.execute_batch("CREATE TRIGGER reject_folder_delete BEFORE DELETE ON folders BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
            let result = if reset {
                reset_with_conn(&conn)
            } else {
                remove_folder_with_conn(&conn, &root.to_string())
            };
            assert!(result.is_err());
            assert!(db::get_file(&conn, active).unwrap().is_some());
            assert_eq!(count(&conn, "folders"), 2);
        }
    }

    #[test]
    fn cleared_approvals_no_longer_allow_old_targets() {
        let targets = ApprovedTargets::default();
        let conn = db::connect_in_memory().unwrap();
        let path = Path::new("/old-target");
        targets.approve(path);
        assert!(target_is_approved(&conn, &targets, path));
        targets.clear();
        assert!(!target_is_approved(&conn, &targets, path));
    }

    #[test]
    fn removal_is_atomic_and_rejects_unknown_invalid_and_trashed_ids() {
        let (conn, _, _, active, trash) = fixture();
        for bad in ["invalid".into(), "9999".into(), trash.to_string()] {
            let error = remove_files_with_conn(&conn, vec![active.to_string(), bad]).unwrap_err();
            assert!(error.expected);
            assert!(db::get_file(&conn, active).unwrap().is_some());
        }
        assert_eq!(
            remove_files_with_conn(&conn, vec![active.to_string(), active.to_string()]).unwrap(),
            1
        );
        assert!(db::get_file(&conn, active).unwrap().is_none());
        assert!(db::get_file(&conn, trash).unwrap().is_some());
    }

    #[test]
    fn recursive_summary_and_removal_preserve_trash_and_original_path() {
        let (conn, root, _, active, trash) = fixture();
        let summary = folder_summary_with_conn(&conn, &root.to_string()).unwrap();
        assert_eq!((summary.subfolder_count, summary.model_count), (1, 1));
        assert_eq!(summary.name, "root");
        remove_folder_with_conn(&conn, &root.to_string()).unwrap();
        assert!(db::get_file(&conn, active).unwrap().is_none());
        let file = db::get_file(&conn, trash).unwrap().unwrap();
        assert_eq!(file.folder_id, None);
        assert_eq!(file.path, "/catalog/child/b.stl");
        assert_eq!(file.trash_path.as_deref(), Some("/trash/b.stl"));
        db::restore_file(&conn, trash, None).unwrap();
        assert!(db::get_file(&conn, trash)
            .unwrap()
            .unwrap()
            .deleted_at
            .is_none());
        assert!(db::list_folders(&conn).unwrap().is_empty());
    }

    #[test]
    fn storage_location_and_its_ancestors_cannot_be_removed() {
        let (conn, root, child, active, _) = fixture();
        db::printer_link::set_setting(&conn, "catalog_base_dir", "/catalog/child").unwrap();
        for id in [root, child] {
            assert!(
                remove_folder_with_conn(&conn, &id.to_string())
                    .unwrap_err()
                    .expected
            );
        }
        assert!(db::get_file(&conn, active).unwrap().is_some());
    }

    #[test]
    fn reset_detaches_jobs_even_when_the_booked_model_is_in_trash() {
        let (conn, _, _, active, trash) = fixture();
        related_rows(&conn, active, trash);
        conn.execute("UPDATE printer_jobs SET booked_file_id = ?1", [trash])
            .unwrap();
        reset_with_conn(&conn).unwrap();
        assert_eq!(
            conn.query_row("SELECT booked_file_id FROM printer_jobs", [], |r| r
                .get::<_, Option<i64>>(0))
                .unwrap(),
            None
        );
        assert_eq!(count(&conn, "printer_jobs"), 1);
        assert!(db::get_file(&conn, trash)
            .unwrap()
            .unwrap()
            .deleted_at
            .is_some());
    }

    #[test]
    fn reset_removes_catalog_but_keeps_trash_and_other_settings() {
        let (conn, _, _, active, trash) = fixture();
        db::printer_link::set_setting(&conn, "catalog_base_dir", "/catalog").unwrap();
        db::printer_link::set_setting(&conn, "keep", "yes").unwrap();
        let result = reset_with_conn(&conn).unwrap();
        assert_eq!((result.model_count, result.folder_count), (1, 2));
        assert!(db::get_file(&conn, active).unwrap().is_none());
        assert_eq!(db::get_file(&conn, trash).unwrap().unwrap().folder_id, None);
        assert_eq!(
            db::printer_link::get_setting(&conn, "catalog_base_dir").unwrap(),
            None
        );
        assert_eq!(
            db::printer_link::get_setting(&conn, "keep")
                .unwrap()
                .as_deref(),
            Some("yes")
        );
    }
}
