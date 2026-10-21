//! Repository functions for source folders (folders checked for new model files)
//! and for the files the user declined from them.

use rusqlite::{params, Connection, OptionalExtension};

use super::error::DbError;
use super::models::{RejectedSourceRecord, SourceFolderRecord};
use super::printer_link::{get_setting, set_setting};

/// `app_settings` key of the "check the source folders at startup" switch.
pub const SETTING_SOURCE_CHECK_ON_START: &str = "source_check_on_start";

pub fn list_source_folders(conn: &Connection) -> Result<Vec<SourceFolderRecord>, DbError> {
    let mut stmt = conn.prepare("SELECT id, path, added_at FROM source_folders ORDER BY id")?;
    let rows = stmt
        .query_map([], |r| Ok(SourceFolderRecord { id: r.get(0)?, path: r.get(1)?, added_at: r.get(2)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_source_folder(conn: &Connection, id: i64) -> Result<Option<SourceFolderRecord>, DbError> {
    Ok(conn
        .query_row("SELECT id, path, added_at FROM source_folders WHERE id = ?1", params![id], |r| {
            Ok(SourceFolderRecord { id: r.get(0)?, path: r.get(1)?, added_at: r.get(2)? })
        })
        .optional()?)
}

pub fn insert_source_folder(conn: &Connection, path: &str, added_at: &str) -> Result<i64, DbError> {
    conn.execute("INSERT INTO source_folders (path, added_at) VALUES (?1, ?2)", params![path, added_at])?;
    Ok(conn.last_insert_rowid())
}

/// Removes only the remembered folder; its files are never touched.
pub fn delete_source_folder(conn: &Connection, id: i64) -> Result<bool, DbError> {
    Ok(conn.execute("DELETE FROM source_folders WHERE id = ?1", params![id])? > 0)
}

/// Remembers a declined file. A second decline of the same path updates the size.
pub fn upsert_rejected_source(conn: &Connection, path: &str, size_bytes: i64, rejected_at: &str) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO source_rejected (path, size_bytes, rejected_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(path) DO UPDATE SET size_bytes = excluded.size_bytes, rejected_at = excluded.rejected_at",
        params![path, size_bytes, rejected_at],
    )?;
    Ok(())
}

pub fn list_rejected_sources(conn: &Connection) -> Result<Vec<RejectedSourceRecord>, DbError> {
    let mut stmt = conn.prepare("SELECT id, path, size_bytes, rejected_at FROM source_rejected ORDER BY id")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(RejectedSourceRecord { id: r.get(0)?, path: r.get(1)?, size_bytes: r.get(2)?, rejected_at: r.get(3)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn count_rejected_sources(conn: &Connection) -> Result<i64, DbError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM source_rejected", [], |r| r.get(0))?)
}

/// Forgets every declined file; returns how many were forgotten.
pub fn clear_rejected_sources(conn: &Connection) -> Result<i64, DbError> {
    Ok(conn.execute("DELETE FROM source_rejected", [])? as i64)
}

pub fn source_check_on_start(conn: &Connection) -> Result<bool, DbError> {
    Ok(get_setting(conn, SETTING_SOURCE_CHECK_ON_START)?.as_deref() == Some("1"))
}

pub fn set_source_check_on_start(conn: &Connection, enabled: bool) -> Result<(), DbError> {
    set_setting(conn, SETTING_SOURCE_CHECK_ON_START, if enabled { "1" } else { "0" })
}
