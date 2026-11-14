use super::*;

#[tauri::command]
pub fn list_collections(state: State<AppState>) -> CmdResult<Vec<CollectionDto>> {
    let conn = lock_db(&state)?;
    katalog_core::read::list_collections(&conn).map_err(Into::into)
}
#[tauri::command]
pub fn create_collection(state: State<AppState>, name: String) -> CmdResult<CollectionDto> {
    let conn = lock_db(&state)?;
    katalog_core::write::create_collection(&conn, name).map_err(Into::into)
}
#[tauri::command]
pub fn rename_collection(state: State<AppState>, collection_id: String, name: String) -> CmdResult<()> {
    let _: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    katalog_core::write::rename_collection(&conn, collection_id, name).map_err(Into::into)
}
#[tauri::command]
pub fn delete_collection(state: State<AppState>, collection_id: String) -> CmdResult<()> {
    let _: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    katalog_core::write::delete_collection(&conn, collection_id).map_err(Into::into)
}
#[tauri::command]
pub fn add_files_to_collection(state: State<AppState>, collection_id: String, file_ids: Vec<String>) -> CmdResult<()> {
    let _: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    katalog_core::write::add_files_to_collection(&conn, collection_id, file_ids).map_err(Into::into)
}
#[tauri::command]
pub fn remove_file_from_collection(state: State<AppState>, collection_id: String, file_id: String) -> CmdResult<()> {
    let _: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let _: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    katalog_core::write::remove_file_from_collection(&conn, collection_id, file_id).map_err(Into::into)
}
pub use katalog_core::write::CollectionPositionUpdate;
#[tauri::command]
pub fn reorder_collection(state: State<AppState>, collection_id: String, updates: Vec<CollectionPositionUpdate>) -> CmdResult<()> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let mut conn = lock_db(&state)?;
    reorder_collection_with_conn(&mut conn, cid, updates)
}
/// Core logic of `reorder_collection`, without `State`, so it's testable. All ids
/// are checked up front (exist and are members), all updates run in one
/// transaction (see `reorder_queue_with_conn`).
fn reorder_collection_with_conn(
    conn: &mut Connection,
    collection_id: i64,
    updates: Vec<CollectionPositionUpdate>,
) -> CmdResult<()> {
    katalog_core::write::reorder_collection_with_conn(conn, collection_id, updates).map_err(Into::into)
}
#[tauri::command]
pub fn list_collection_files(state: State<AppState>, collection_id: String) -> CmdResult<Vec<ModelFileDto>> {
    let conn = lock_db(&state)?;
    katalog_core::read::list_collection_files(&conn, collection_id).map_err(Into::into)
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
        // id 999999 is not a member of this collection - forces an error "in the middle" of the batch.
        let updates = vec![
            CollectionPositionUpdate { file_id: id1.to_string(), position: 5 },
            CollectionPositionUpdate { file_id: "999999".to_string(), position: 6 },
            CollectionPositionUpdate { file_id: id2.to_string(), position: 7 },
        ];

        let result = reorder_collection_with_conn(&mut conn, cid, updates);

        assert!(result.is_err());
        let ids = db::list_collection_file_ids(&conn, cid).unwrap();
        // Positions unchanged (0 and 1), none of the valid updates (5 and 7) may be committed.
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
        // Error via trigger inside the transaction, after the up-front check passed.
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
