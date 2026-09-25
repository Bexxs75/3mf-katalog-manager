use super::*;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDto {
    pub id: String,
    pub name: String,
    pub model_count: i64,
}
fn to_collection_dto(record: db::models::CollectionRecord) -> CollectionDto {
    CollectionDto {
        id: record.id.to_string(),
        name: record.name,
        model_count: record.model_count,
    }
}
#[tauri::command]
pub fn list_collections(state: State<AppState>) -> CmdResult<Vec<CollectionDto>> {
    let conn = lock_db(&state)?;
    let collections = db::list_collections(&conn).map_err(|e| e.to_string())?;
    Ok(collections.into_iter().map(to_collection_dto).collect())
}
#[tauri::command]
pub fn create_collection(state: State<AppState>, name: String) -> CmdResult<CollectionDto> {
    let conn = lock_db(&state)?;
    let created_at = chrono::Utc::now().to_rfc3339();
    let id = db::create_collection(&conn, &name, &created_at).map_err(|e| e.to_string())?;
    Ok(CollectionDto { id: id.to_string(), name, model_count: 0 })
}
#[tauri::command]
pub fn rename_collection(state: State<AppState>, collection_id: String, name: String) -> CmdResult<()> {
    let id: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    db::rename_collection(&conn, id, &name).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn delete_collection(state: State<AppState>, collection_id: String) -> CmdResult<()> {
    let id: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    db::delete_collection(&conn, id).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn add_files_to_collection(state: State<AppState>, collection_id: String, file_ids: Vec<String>) -> CmdResult<()> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    let start = db::max_collection_position(&conn, cid).map_err(|e| e.to_string())?.unwrap_or(-1) + 1;
    for (next, file_id) in (start..).zip(file_ids) {
        let fid: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
        db::add_file_to_collection(&conn, cid, fid, next).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
pub fn remove_file_from_collection(state: State<AppState>, collection_id: String, file_id: String) -> CmdResult<()> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let fid: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::remove_file_from_collection(&conn, cid, fid).map_err(|e| e.to_string())
}
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionPositionUpdate {
    pub file_id: String,
    pub position: i64,
}
#[tauri::command]
pub fn reorder_collection(state: State<AppState>, collection_id: String, updates: Vec<CollectionPositionUpdate>) -> CmdResult<()> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let mut conn = lock_db(&state)?;
    reorder_collection_with_conn(&mut conn, cid, updates)
}
/// Kernlogik von `reorder_collection`, ohne `State`, damit testbar. Alle ids
/// werden vorab geprueft (existieren und sind Mitglied), alle Updates laufen in
/// einer Transaktion (siehe `reorder_queue_with_conn`).
fn reorder_collection_with_conn(
    conn: &mut Connection,
    collection_id: i64,
    updates: Vec<CollectionPositionUpdate>,
) -> CmdResult<()> {
    let member_ids = db::list_collection_file_ids(conn, collection_id).map_err(|e| e.to_string())?;

    let mut parsed = Vec::with_capacity(updates.len());
    for update in updates {
        let fid: i64 = update.file_id.parse().map_err(|_| "invalid file id".to_string())?;
        if !member_ids.contains(&fid) {
            return Err(format!(
                "Datei mit id {fid} ist nicht Teil der Collection {collection_id} - Batch wird nicht angewendet"
            ));
        }
        parsed.push((fid, update.position));
    }

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for (fid, position) in parsed {
        db::set_collection_position(&tx, collection_id, fid, position).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub fn list_collection_files(state: State<AppState>, collection_id: String) -> CmdResult<Vec<ModelFileDto>> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    let ids = db::list_collection_file_ids(&conn, cid).map_err(|e| e.to_string())?;
    let files = db::list_files_by_ids(&conn, &ids).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_reorder_batch_is_all_or_nothing_on_a_mid_batch_failure() {
        let mut conn = db::connect_in_memory().unwrap();
        let id1 = db::test_insert_minimal_file(&conn, "/tmp/1.3mf", None).unwrap();
        let id2 = db::test_insert_minimal_file(&conn, "/tmp/2.3mf", None).unwrap();
        let created_at = chrono::Utc::now().to_rfc3339();
        let cid = db::create_collection(&conn, "Testset", &created_at).unwrap();
        db::add_file_to_collection(&conn, cid, id1, 0).unwrap();
        db::add_file_to_collection(&conn, cid, id2, 1).unwrap();
        // id 999999 ist kein Mitglied dieser Collection - erzwingt einen
        // Fehler "in der Mitte" des Batches.
        let updates = vec![
            CollectionPositionUpdate { file_id: id1.to_string(), position: 5 },
            CollectionPositionUpdate { file_id: "999999".to_string(), position: 6 },
            CollectionPositionUpdate { file_id: id2.to_string(), position: 7 },
        ];

        let result = reorder_collection_with_conn(&mut conn, cid, updates);

        assert!(result.is_err());
        let ids = db::list_collection_file_ids(&conn, cid).unwrap();
        // Positionen unveraendert (0 und 1), keines der gueltigen Updates
        // (5 und 7) darf committed sein.
        assert_eq!(ids, vec![id1, id2], "Reihenfolge/Positionen duerfen nach fehlgeschlagenem Batch unveraendert sein");
    }
    #[test]
    fn a_fully_valid_collection_reorder_batch_remains_functionally_identical() {
        let mut conn = db::connect_in_memory().unwrap();
        let id1 = db::test_insert_minimal_file(&conn, "/tmp/1.3mf", None).unwrap();
        let id2 = db::test_insert_minimal_file(&conn, "/tmp/2.3mf", None).unwrap();
        let created_at = chrono::Utc::now().to_rfc3339();
        let cid = db::create_collection(&conn, "Testset", &created_at).unwrap();
        db::add_file_to_collection(&conn, cid, id1, 0).unwrap();
        db::add_file_to_collection(&conn, cid, id2, 1).unwrap();
        let updates = vec![
            CollectionPositionUpdate { file_id: id1.to_string(), position: 1 },
            CollectionPositionUpdate { file_id: id2.to_string(), position: 0 },
        ];

        reorder_collection_with_conn(&mut conn, cid, updates).unwrap();

        let ids = db::list_collection_file_ids(&conn, cid).unwrap();
        assert_eq!(ids, vec![id2, id1], "nach Swap muss id2 (Position 0) vor id1 (Position 1) stehen");
    }
    #[test]
    fn collection_reorder_batch_rolls_back_an_already_applied_earlier_update_when_a_later_one_fails_inside_the_transaction() {
        // Fehler per Trigger innerhalb der Transaktion, nach bestandener Vorabpruefung.
        let mut conn = db::connect_in_memory().unwrap();
        let id1 = db::test_insert_minimal_file(&conn, "/tmp/1.3mf", None).unwrap();
        let id2 = db::test_insert_minimal_file(&conn, "/tmp/2.3mf", None).unwrap();
        let id3 = db::test_insert_minimal_file(&conn, "/tmp/3.3mf", None).unwrap();
        let created_at = chrono::Utc::now().to_rfc3339();
        let cid = db::create_collection(&conn, "Testset", &created_at).unwrap();
        db::add_file_to_collection(&conn, cid, id1, 0).unwrap();
        db::add_file_to_collection(&conn, cid, id2, 1).unwrap();
        db::add_file_to_collection(&conn, cid, id3, 2).unwrap();

        conn.execute_batch(
            "CREATE TRIGGER block_second_collection_update BEFORE UPDATE ON collection_files
             WHEN NEW.position = 21
             BEGIN SELECT RAISE(ABORT, 'simulierter Fehler beim zweiten Collection-Update'); END;",
        )
        .unwrap();

        let updates = vec![
            CollectionPositionUpdate { file_id: id1.to_string(), position: 10 },
            CollectionPositionUpdate { file_id: id2.to_string(), position: 21 },
            CollectionPositionUpdate { file_id: id3.to_string(), position: 30 },
        ];

        let result = reorder_collection_with_conn(&mut conn, cid, updates);

        assert!(result.is_err(), "must surface the trigger-raised failure on the second update");
        let position1: i64 = conn
            .query_row(
                "SELECT position FROM collection_files WHERE collection_id = ?1 AND file_id = ?2",
                rusqlite::params![cid, id1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            position1, 0,
            "the first update must be rolled back even though it succeeded inside the transaction before the second one failed"
        );
    }
}
