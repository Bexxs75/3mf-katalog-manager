use super::*;

/// Core logic of `delete_file` with an already loaded `FileRecord`, without `State`, so it's testable.
fn delete_file_with_conn(
    conn: &Connection,
    file: &db::models::FileRecord,
    id: i64,
    trash_dir: &std::path::Path,
) -> CmdResult<()> {
    match std::fs::metadata(&file.path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Path unreachable (renamed folder, unmounted drive): nothing to move, but
            // still only soft-delete, because the file is usually not really gone. Other
            // errors like PermissionDenied go through the error path below.
            let deleted_at = chrono::Utc::now().to_rfc3339();
            return db::soft_delete_file(conn, id, None, &deleted_at).map_err(|e| e.to_string());
        }
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }

    let trash_path = trash_dir.join(format!("{id}-{}", file.name));
    move_file(std::path::Path::new(&file.path), &trash_path).map_err(|e| e.to_string())?;

    let deleted_at = chrono::Utc::now().to_rfc3339();
    if let Err(db_err) = db::soft_delete_file(conn, id, Some(&trash_path.to_string_lossy()), &deleted_at) {
        // Move the file back out of the trash if the DB couldn't mark it as deleted;
        // otherwise it would vanish for the user.
        if let Err(rollback_err) = move_file(&trash_path, std::path::Path::new(&file.path)) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback aus dem Papierkorb fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}, DB fuehrt sie weiterhin als aktiv unter {}",
                trash_path.display(),
                file.path
            ));
        }
        return Err(db_err.to_string());
    }
    Ok(())
}
#[tauri::command]
pub fn delete_file(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    delete_file_with_conn(&conn, &file, id, &state.trash_dir)
}
#[tauri::command]
pub fn delete_files(state: State<AppState>, file_ids: Vec<String>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    delete_files_with_conn(&conn, &state.trash_dir, file_ids)
}
/// Core logic of `delete_files`, without `State`, so it's testable.
fn delete_files_with_conn(conn: &Connection, trash_dir: &std::path::Path, file_ids: Vec<String>) -> CmdResult<()> {
    for file_id in file_ids {
        let id: i64 = match file_id.parse() {
            Ok(id) => id,
            Err(_) => {
                eprintln!("[cleanup] Ungueltige Datei-ID uebersprungen: {file_id}");
                continue;
            }
        };
        // Cleanup batch: a file deleted in the meantime (e.g. twice in the selection)
        // is skipped instead of aborting the whole batch.
        let file = match db::get_file(conn, id) {
            Ok(Some(file)) => file,
            Ok(None) => continue,
            Err(e) => {
                eprintln!("[cleanup] could not load file ID {id}: {e}");
                continue;
            }
        };
        // Log single errors and continue, so the frontend can reload cleanly
        // afterwards. Same function as for single deletes, so the move-back
        // compensation applies here too.
        if let Err(e) = delete_file_with_conn(conn, &file, id, trash_dir) {
            eprintln!("[cleanup] deleting failed for file ID {id}: {e}");
        }
    }
    Ok(())
}
#[tauri::command]
pub fn list_trash(state: State<AppState>) -> CmdResult<Vec<ModelFileDto>> {
    let conn = lock_db(&state)?;
    let files = db::list_trash(&conn).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
}
/// Core logic of `restore_file` with an already loaded `FileRecord`, without `State`, so it's testable.
fn restore_file_with_conn(
    conn: &Connection,
    file: &db::models::FileRecord,
    id: i64,
) -> CmdResult<()> {
    if file.deleted_at.is_none() {
        return Err("file is not in trash".to_string());
    }

    let Some(trash_path) = file.trash_path.clone() else {
        // Without trash_path the file was already unreachable when deleted: just make
        // the entry visible again.
        return db::restore_file(conn, id, None).map_err(|e| e.to_string());
    };

    let original = std::path::Path::new(&file.path);
    let target_path = if original.exists() {
        let stem = original.file_stem().and_then(|s| s.to_str()).unwrap_or("datei");
        let ext = original.extension().and_then(|s| s.to_str());
        let parent = original.parent().unwrap_or_else(|| std::path::Path::new("."));
        let new_name = match ext {
            Some(ext) => format!("{stem} (wiederhergestellt).{ext}"),
            None => format!("{stem} (wiederhergestellt)"),
        };
        parent.join(new_name)
    } else {
        original.to_path_buf()
    };

    move_file(std::path::Path::new(&trash_path), &target_path).map_err(|e| e.to_string())?;

    let new_path_str = target_path.to_string_lossy().to_string();
    let new_path_arg = if new_path_str == file.path { None } else { Some(new_path_str.as_str()) };
    if let Err(db_err) = db::restore_file(conn, id, new_path_arg) {
        // Back into the trash if the DB couldn't record the restore.
        if let Err(rollback_err) = move_file(&target_path, std::path::Path::new(&trash_path)) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback in den Papierkorb fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}, DB fuehrt sie weiterhin als geloescht",
                target_path.display()
            ));
        }
        return Err(db_err.to_string());
    }
    Ok(())
}
#[tauri::command]
pub fn restore_file(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    restore_file_with_conn(&conn, &file, id)
}
#[tauri::command]
pub fn delete_file_permanently(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    if file.deleted_at.is_none() {
        return Err("file is not in trash".to_string());
    }
    if let Some(trash_path) = &file.trash_path {
        if let Err(e) = std::fs::remove_file(trash_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.to_string());
            }
        }
    }
    db::delete_file(&conn, id).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn empty_trash(state: State<AppState>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    let files = db::list_trash(&conn).map_err(|e| e.to_string())?;
    for file in files {
        if let Some(trash_path) = &file.trash_path {
            if let Err(e) = std::fs::remove_file(trash_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("[trash] removing failed for file ID {}: {e}", file.id);
                    continue;
                }
            }
        }
        if let Err(e) = db::delete_file(&conn, file.id) {
            eprintln!("[trash] could not delete DB row for file ID {}: {e}", file.id);
        }
    }
    Ok(())
}
/// At startup: removes trash entries older than 7 days. Single errors are
/// logged and skipped.
pub fn purge_expired_trash_on_startup(conn: &Connection) {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let expired = match db::purge_expired_trash(conn, &cutoff) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("[startup] trash cleanup: query failed: {e}");
            return;
        }
    };
    for file in expired {
        if let Some(trash_path) = &file.trash_path {
            if let Err(e) = std::fs::remove_file(trash_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("[startup] trash cleanup: file failed for ID {}: {e}", file.id);
                    continue;
                }
            }
        }
        if let Err(e) = db::delete_file(conn, file.id) {
            eprintln!("[startup] trash cleanup: DB row failed for ID {}: {e}", file.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let result = restore_file_with_conn(&conn, &file, file_id);

        assert!(result.is_err(), "must surface the db::restore_file failure raised by the trigger");
        assert!(trash_path.exists(), "file must be moved back into the trash after db::restore_file failed");
        assert!(!original_path.exists(), "destination must not exist after the rollback");
        let still_deleted: Option<String> = conn
            .query_row("SELECT deleted_at FROM files WHERE id = ?1", [file_id], |r| r.get(0))
            .unwrap();
        assert!(still_deleted.is_some(), "db must still show the file as deleted after the failed restore");
    }
}
