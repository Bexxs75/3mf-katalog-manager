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

/// Rows of a model that are not in the trash, in display order, at most `limit`.
pub fn list_active_companions_limited(conn: &Connection, file_id: i64, limit: i64) -> Result<Vec<CompanionRecord>, DbError> {
    collect(
        conn,
        &format!(
            "SELECT {COLUMNS} FROM companion_files WHERE file_id = ?1 AND deleted_at IS NULL ORDER BY name COLLATE NOCASE, id LIMIT ?2"
        ),
        params![file_id, limit],
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

/// Like [`restore_companion`], but the file came back under a different name. All of it
/// is one statement, so the row never shows a name that disagrees with its path.
pub fn restore_companion_as(conn: &Connection, id: i64, name: &str, path: &str) -> Result<(), DbError> {
    let affected = conn.execute(
        "UPDATE companion_files SET deleted_at = NULL, trash_path = NULL, name = ?1, path = ?2 WHERE id = ?3",
        params![name, path, id],
    )?;
    if affected == 0 {
        return Err(DbError::Other(format!("keine Begleitdatei mit id {id} gefunden")));
    }
    Ok(())
}

/// True if a row other than `except` already holds `path` (trashed rows included: they
/// keep their old path, which stays unique).
pub fn path_held_by_other_row(conn: &Connection, path: &str, except: Option<i64>) -> Result<bool, DbError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM companion_files WHERE path = ?1 AND id IS NOT ?2)",
        params![path, except],
        |r| r.get(0),
    )?)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connect_in_memory, insert_folder, test_insert_minimal_file, update_paths_under_folder};

    fn model(conn: &Connection, path: &str) -> i64 {
        test_insert_minimal_file(conn, path, None).unwrap()
    }

    #[test]
    fn rows_are_stored_listed_and_removed_with_their_model() {
        let conn = connect_in_memory().unwrap();
        let (a, b) = (model(&conn, "/m/a.3mf"), model(&conn, "/m/b.3mf"));
        let first = insert_companion(&conn, a, "z.txt", "/m/a_extras/z.txt", "text", Some(5), "t1").unwrap();
        insert_companion(&conn, a, "A.png", "/m/a_extras/A.png", "image", None, "t2").unwrap();
        insert_companion(&conn, b, "b.pdf", "/m/b_extras/b.pdf", "pdf", Some(1), "t3").unwrap();
        let names: Vec<_> = list_companions(&conn, a).unwrap().into_iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["A.png", "z.txt"], "sorted by name, ignoring case");
        assert_eq!(get_companion(&conn, first).unwrap().unwrap().size_bytes, Some(5));
        assert!(insert_companion(&conn, a, "z.txt", "/m/a_extras/z.txt", "text", None, "t").is_err(), "paths are unique");
        assert!(insert_companion(&conn, a, "x.exe", "/m/a_extras/x.exe", "exe", None, "t").is_err(), "kinds are checked");
        assert!(insert_companion(&conn, 999, "n.txt", "/m/n.txt", "text", None, "t").is_err(), "a model must exist");
        conn.execute("DELETE FROM files WHERE id = ?1", [a]).unwrap();
        assert!(list_companions(&conn, a).unwrap().is_empty());
        assert_eq!(list_companions(&conn, b).unwrap().len(), 1);
    }

    #[test]
    fn trash_state_is_tracked_per_row() {
        let conn = connect_in_memory().unwrap();
        let a = model(&conn, "/m/a.3mf");
        let one = insert_companion(&conn, a, "1.txt", "/m/a_extras/1.txt", "text", None, "t").unwrap();
        let two = insert_companion(&conn, a, "2.txt", "/m/a_extras/2.txt", "text", None, "t").unwrap();
        soft_delete_companion(&conn, one, Some("/trash/companion-1-1.txt"), "T1").unwrap();
        soft_delete_companion(&conn, two, None, "T2").unwrap();
        assert_eq!(count_active_companions(&conn, a).unwrap(), 0);
        assert_eq!(list_trashed_with_model(&conn, a, "T1").unwrap().len(), 1);
        assert_eq!(list_trashed_files_of_model(&conn, a).unwrap().len(), 2);
        assert_eq!(list_expired_trashed_companions(&conn, "T2").unwrap().len(), 1, "only T1 is older than T2");
        restore_companion(&conn, one, "/m/a_extras/1.txt").unwrap();
        assert_eq!(list_active_companions(&conn, a).unwrap().len(), 1);
        assert!(soft_delete_companion(&conn, 999, None, "T").is_err());
    }

    #[test]
    fn a_folder_rename_rewrites_companion_paths_below_it_and_nothing_else() {
        let conn = connect_in_memory().unwrap();
        let folder = insert_folder(&conn, "/a/Deko").unwrap();
        let inside = model(&conn, "/a/Deko/x.3mf");
        let sibling = model(&conn, "/a/Deko2/y.3mf");
        insert_companion(&conn, inside, "p.png", "/a/Deko/x_extras/p.png", "image", None, "t").unwrap();
        insert_companion(&conn, sibling, "p.png", "/a/Deko2/y_extras/p.png", "image", None, "t").unwrap();
        let trashed = insert_companion(&conn, inside, "q.png", "/a/Deko/x_extras/q.png", "image", None, "t").unwrap();
        soft_delete_companion(&conn, trashed, Some("/trash/companion-3-q.png"), "T").unwrap();

        update_paths_under_folder(&conn, folder, "/a/Deko", "/a/Decor").unwrap();

        let path = |model: i64, name: &str| list_companions(&conn, model).unwrap().into_iter().find(|c| c.name == name).unwrap();
        assert_eq!(path(inside, "p.png").path, "/a/Decor/x_extras/p.png");
        assert_eq!(path(sibling, "p.png").path, "/a/Deko2/y_extras/p.png", "a folder with a similar name stays");
        let moved_trashed = path(inside, "q.png");
        assert_eq!(moved_trashed.path, "/a/Decor/x_extras/q.png", "where a restore will put it");
        assert_eq!(moved_trashed.trash_path.as_deref(), Some("/trash/companion-3-q.png"), "the trash file itself is not touched");
    }
}
