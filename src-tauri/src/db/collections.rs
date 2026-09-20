// Repository-Funktionen fuer Sammlungen: viele-zu-viele wie Tags, aber mit
// einer position-Spalte pro Zuordnung fuer eine manuell festlegbare
// Reihenfolge - der eigentliche Mehrwert gegenueber einem Tag.

use rusqlite::{params, Connection};

use super::error::DbError;
use super::models::CollectionRecord;

pub fn create_collection(conn: &Connection, name: &str, created_at: &str) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO collections (name, created_at) VALUES (?1, ?2)",
        params![name, created_at],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_collections(conn: &Connection) -> Result<Vec<CollectionRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name,
                (SELECT COUNT(*) FROM collection_files cf
                 JOIN files f ON f.id = cf.file_id
                 WHERE cf.collection_id = c.id AND f.deleted_at IS NULL)
         FROM collections c
         ORDER BY c.created_at",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CollectionRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                model_count: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn rename_collection(conn: &Connection, id: i64, name: &str) -> Result<(), DbError> {
    conn.execute("UPDATE collections SET name = ?1 WHERE id = ?2", params![name, id])?;
    Ok(())
}

pub fn delete_collection(conn: &Connection, id: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM collections WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn max_collection_position(conn: &Connection, collection_id: i64) -> Result<Option<i64>, DbError> {
    Ok(conn.query_row(
        "SELECT MAX(position) FROM collection_files WHERE collection_id = ?1",
        params![collection_id],
        |row| row.get(0),
    )?)
}

/// Fuegt eine Datei ans Ende der Sammlung an. Ist die Datei bereits
/// zugeordnet, passiert nichts (kein Duplikat, keine Neu-Positionierung) -
/// "INSERT OR IGNORE" nutzt dafuer den UNIQUE-Constraint auf
/// (collection_id, file_id).
pub fn add_file_to_collection(
    conn: &Connection,
    collection_id: i64,
    file_id: i64,
    position: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR IGNORE INTO collection_files (collection_id, file_id, position) VALUES (?1, ?2, ?3)",
        params![collection_id, file_id, position],
    )?;
    Ok(())
}

pub fn remove_file_from_collection(conn: &Connection, collection_id: i64, file_id: i64) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM collection_files WHERE collection_id = ?1 AND file_id = ?2",
        params![collection_id, file_id],
    )?;
    Ok(())
}

pub fn set_collection_position(
    conn: &Connection,
    collection_id: i64,
    file_id: i64,
    position: i64,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE collection_files SET position = ?1 WHERE collection_id = ?2 AND file_id = ?3",
        params![position, collection_id, file_id],
    )?;
    Ok(())
}

pub fn list_collection_file_ids(conn: &Connection, collection_id: i64) -> Result<Vec<i64>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT cf.file_id FROM collection_files cf
         JOIN files f ON f.id = cf.file_id
         WHERE cf.collection_id = ?1 AND f.deleted_at IS NULL
         ORDER BY cf.position",
    )?;
    let rows = stmt
        .query_map(params![collection_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repository::connect_in_memory;
    use crate::db::{insert_file, models::{FileType, NewFile}};
    use std::collections::BTreeMap;

    fn sample_file(path: &str) -> NewFile {
        NewFile {
            name: path.to_string(),
            path: path.to_string(),
            file_type: FileType::ThreeMf,
            folder_id: None,
            origin: "local".to_string(),
            cloud_id: None,
            sync_status: "local-only".to_string(),
            file_size_bytes: 100,
            dimensions_mm: None,
            volume_cm3: None,
            object_count: None,
            thumbnail_png: None,
            imported_at: "2026-09-12T10:00:00Z".to_string(),
            file_modified_at: None,
            materials: vec![],
            metadata: BTreeMap::new(),
            tags: vec![],
            print_status: "not_printed".to_string(),
            last_viewed_at: None,
            creator: None,
            content_hash: None,
            render_snapshot_png: None,
            custom_image_png: None,
            source_url: None,
            queue_position: None,
            favorite: false,
            plate_count: None,
            slice_info_json: None,
        }
    }

    #[test]
    fn add_file_to_collection_appends_at_given_position_and_ignores_duplicates() {
        let mut conn = connect_in_memory().unwrap();
        let file_id = insert_file(&mut conn, &sample_file("/tmp/a.3mf")).unwrap();
        let collection_id = create_collection(&conn, "Bauvorhaben X", "2026-09-12T10:00:00Z").unwrap();

        add_file_to_collection(&conn, collection_id, file_id, 0).unwrap();
        add_file_to_collection(&conn, collection_id, file_id, 5).unwrap(); // Duplikat, wird ignoriert

        let ids = list_collection_file_ids(&conn, collection_id).unwrap();
        assert_eq!(ids, vec![file_id]);
    }

    #[test]
    fn set_collection_position_reorders_correctly() {
        let mut conn = connect_in_memory().unwrap();
        let a = insert_file(&mut conn, &sample_file("/tmp/a.3mf")).unwrap();
        let b = insert_file(&mut conn, &sample_file("/tmp/b.3mf")).unwrap();
        let collection_id = create_collection(&conn, "Bauvorhaben X", "2026-09-12T10:00:00Z").unwrap();
        add_file_to_collection(&conn, collection_id, a, 0).unwrap();
        add_file_to_collection(&conn, collection_id, b, 1).unwrap();

        set_collection_position(&conn, collection_id, a, 5).unwrap();
        set_collection_position(&conn, collection_id, b, 0).unwrap();

        let ids = list_collection_file_ids(&conn, collection_id).unwrap();
        assert_eq!(ids, vec![b, a]);
    }

    #[test]
    fn delete_collection_removes_only_the_mapping_not_the_file() {
        let mut conn = connect_in_memory().unwrap();
        let file_id = insert_file(&mut conn, &sample_file("/tmp/a.3mf")).unwrap();
        let collection_id = create_collection(&conn, "Bauvorhaben X", "2026-09-12T10:00:00Z").unwrap();
        add_file_to_collection(&conn, collection_id, file_id, 0).unwrap();

        delete_collection(&conn, collection_id).unwrap();

        let ids = list_collection_file_ids(&conn, collection_id).unwrap();
        assert!(ids.is_empty());
        let file_still_exists: i64 = conn
            .query_row("SELECT COUNT(*) FROM files WHERE id = ?1", params![file_id], |row| row.get(0))
            .unwrap();
        assert_eq!(file_still_exists, 1);
    }

    #[test]
    fn list_collections_returns_correct_model_count() {
        let mut conn = connect_in_memory().unwrap();
        let a = insert_file(&mut conn, &sample_file("/tmp/a.3mf")).unwrap();
        let b = insert_file(&mut conn, &sample_file("/tmp/b.3mf")).unwrap();
        let collection_id = create_collection(&conn, "Bauvorhaben X", "2026-09-12T10:00:00Z").unwrap();
        add_file_to_collection(&conn, collection_id, a, 0).unwrap();
        add_file_to_collection(&conn, collection_id, b, 1).unwrap();

        let collections = list_collections(&conn).unwrap();
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].model_count, 2);
    }

    #[test]
    fn list_collection_file_ids_excludes_soft_deleted_files() {
        let mut conn = connect_in_memory().unwrap();
        let a = insert_file(&mut conn, &sample_file("/tmp/a.3mf")).unwrap();
        let b = insert_file(&mut conn, &sample_file("/tmp/b.3mf")).unwrap();
        let collection_id = create_collection(&conn, "Bauvorhaben X", "2026-09-12T10:00:00Z").unwrap();
        add_file_to_collection(&conn, collection_id, a, 0).unwrap();
        add_file_to_collection(&conn, collection_id, b, 1).unwrap();

        conn.execute(
            "UPDATE files SET deleted_at = ?1 WHERE id = ?2",
            params!["2026-09-12T12:00:00Z", a],
        )
        .unwrap();

        let ids = list_collection_file_ids(&conn, collection_id).unwrap();
        assert_eq!(ids, vec![b]);

        let collections = list_collections(&conn).unwrap();
        assert_eq!(collections[0].model_count, 1);
    }
}
