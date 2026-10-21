//! Repository functions for companion files. The files themselves live on disk in
//! a `<model>_extras` folder; these rows only record them.

use rusqlite::{params, Connection, OptionalExtension};

use super::error::DbError;
use super::models::CompanionRecord;

const COLUMNS: &str = "id, file_id, name, path, kind, size_bytes, added_at, deleted_at, trash_path";

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<CompanionRecord> {
    Ok(CompanionRecord {
        id: row.get(0)?,
        file_id: row.get(1)?,
        name: row.get(2)?,
        path: row.get(3)?,
        kind: row.get(4)?,
        size_bytes: row.get(5)?,
        added_at: row.get(6)?,
        deleted_at: row.get(7)?,
        trash_path: row.get(8)?,
    })
}

fn collect(conn: &Connection, sql: &str, args: impl rusqlite::Params) -> Result<Vec<CompanionRecord>, DbError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(args, from_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn insert_companion(
    conn: &Connection,
    file_id: i64,
    name: &str,
    path: &str,
    kind: &str,
    size_bytes: Option<i64>,
    added_at: &str,
) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO companion_files (file_id, name, path, kind, size_bytes, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![file_id, name, path, kind, size_bytes, added_at],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_companion(conn: &Connection, id: i64) -> Result<Option<CompanionRecord>, DbError> {
    Ok(conn
        .query_row(&format!("SELECT {COLUMNS} FROM companion_files WHERE id = ?1"), params![id], from_row)
        .optional()?)
}

/// Every row of a model, trashed ones included, in display order.
pub fn list_companions(conn: &Connection, file_id: i64) -> Result<Vec<CompanionRecord>, DbError> {
    collect(
        conn,
        &format!("SELECT {COLUMNS} FROM companion_files WHERE file_id = ?1 ORDER BY name COLLATE NOCASE, id"),
        params![file_id],
    )
}

/// Rows of a model that are not in the trash.
pub fn list_active_companions(conn: &Connection, file_id: i64) -> Result<Vec<CompanionRecord>, DbError> {
    collect(
        conn,
        &format!(
            "SELECT {COLUMNS} FROM companion_files WHERE file_id = ?1 AND deleted_at IS NULL ORDER BY id"
        ),
        params![file_id],
    )
}

pub fn count_active_companions(conn: &Connection, file_id: i64) -> Result<i64, DbError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM companion_files WHERE file_id = ?1 AND deleted_at IS NULL",
        params![file_id],
        |r| r.get(0),
    )?)
}

/// Rows that went into the trash together with their model (same `deleted_at`).
pub fn list_trashed_with_model(conn: &Connection, file_id: i64, deleted_at: &str) -> Result<Vec<CompanionRecord>, DbError> {
    collect(
        conn,
        &format!("SELECT {COLUMNS} FROM companion_files WHERE file_id = ?1 AND deleted_at = ?2 ORDER BY id"),
        params![file_id, deleted_at],
    )
}

/// Rows of a model whose file sits in the app trash.
pub fn list_trashed_files_of_model(conn: &Connection, file_id: i64) -> Result<Vec<CompanionRecord>, DbError> {
    collect(
        conn,
        &format!("SELECT {COLUMNS} FROM companion_files WHERE file_id = ?1 AND deleted_at IS NOT NULL ORDER BY id"),
        params![file_id],
    )
}

pub fn list_all_trashed_companions(conn: &Connection) -> Result<Vec<CompanionRecord>, DbError> {
    collect(
        conn,
        &format!("SELECT {COLUMNS} FROM companion_files WHERE deleted_at IS NOT NULL ORDER BY id"),
        [],
    )
}

pub fn list_expired_trashed_companions(conn: &Connection, older_than: &str) -> Result<Vec<CompanionRecord>, DbError> {
    collect(
        conn,
        &format!("SELECT {COLUMNS} FROM companion_files WHERE deleted_at IS NOT NULL AND deleted_at < ?1 ORDER BY id"),
        params![older_than],
    )
}

pub fn soft_delete_companion(conn: &Connection, id: i64, trash_path: Option<&str>, deleted_at: &str) -> Result<(), DbError> {
    let affected = conn.execute(
        "UPDATE companion_files SET deleted_at = ?1, trash_path = ?2 WHERE id = ?3",
        params![deleted_at, trash_path, id],
    )?;
    if affected == 0 {
        return Err(DbError::Other(format!("keine Begleitdatei mit id {id} gefunden")));
    }
    Ok(())
}

/// Clears the trash state and points the row at the place the file was restored to.
pub fn restore_companion(conn: &Connection, id: i64, path: &str) -> Result<(), DbError> {
    let affected = conn.execute(
        "UPDATE companion_files SET deleted_at = NULL, trash_path = NULL, path = ?1 WHERE id = ?2",
        params![path, id],
    )?;
    if affected == 0 {
        return Err(DbError::Other(format!("keine Begleitdatei mit id {id} gefunden")));
    }
    Ok(())
}

pub fn update_companion_path(conn: &Connection, id: i64, path: &str) -> Result<(), DbError> {
    let affected = conn.execute("UPDATE companion_files SET path = ?1 WHERE id = ?2", params![path, id])?;
    if affected == 0 {
        return Err(DbError::Other(format!("keine Begleitdatei mit id {id} gefunden")));
    }
    Ok(())
}

pub fn delete_companion(conn: &Connection, id: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM companion_files WHERE id = ?1", params![id])?;
    Ok(())
}

/// Paths of every row that is not in the trash, for the disk comparison.
pub fn list_active_companion_paths(conn: &Connection) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare("SELECT path FROM companion_files WHERE deleted_at IS NULL")?;
    let rows = stmt.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    Ok(rows)
}
