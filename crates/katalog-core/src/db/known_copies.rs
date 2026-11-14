//! Copies of cataloged models that the disk sync offered and the user did not take
//! over. They are remembered by path, size and content hash so the next sync neither
//! hashes nor offers them again. The table never decides anything about the files
//! themselves: the sync only reads it to skip work.

use rusqlite::{params, Connection};

use super::error::DbError;

/// Upper bound for the table; the oldest rows go first. It only stops the table from growing without end.
pub const MAX_KNOWN_COPIES: usize = 20_000;
/// Longest path a remembered copy may carry (characters), like every stored path.
pub const MAX_KNOWN_COPY_PATH_CHARS: usize = 4096;

/// A remembered copy: the file at `path` had `size_bytes` and `content_hash` when it was declined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownCopyRecord {
    pub id: i64,
    pub path: String,
    pub size_bytes: i64,
    pub content_hash: String,
    pub noted_at: String,
}

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<KnownCopyRecord> {
    Ok(KnownCopyRecord { id: r.get(0)?, path: r.get(1)?, size_bytes: r.get(2)?, content_hash: r.get(3)?, noted_at: r.get(4)? })
}

pub fn list_known_copies(conn: &Connection) -> Result<Vec<KnownCopyRecord>, DbError> {
    let mut stmt = conn.prepare("SELECT id, path, size_bytes, content_hash, noted_at FROM disk_sync_known_copies ORDER BY id")?;
    let rows = stmt.query_map([], from_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The first `limit` rows (oldest first) for the list in the settings.
pub fn list_known_copies_limited(conn: &Connection, limit: usize) -> Result<Vec<KnownCopyRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, path, size_bytes, content_hash, noted_at FROM disk_sync_known_copies ORDER BY id LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], from_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn count_known_copies(conn: &Connection) -> Result<i64, DbError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM disk_sync_known_copies", [], |r| r.get(0))?)
}

/// Remembers a declined copy. A second decline of the same path replaces size and hash.
pub fn note_known_copy(conn: &Connection, path: &str, size_bytes: i64, content_hash: &str, noted_at: &str) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO disk_sync_known_copies (path, size_bytes, content_hash, noted_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(path) DO UPDATE SET size_bytes = excluded.size_bytes,
             content_hash = excluded.content_hash, noted_at = excluded.noted_at",
        params![path, size_bytes, content_hash, noted_at],
    )?;
    Ok(())
}

/// Forgets single rows (stale or taken over); returns how many were removed.
pub fn forget_known_copies(conn: &Connection, ids: &[i64]) -> Result<usize, DbError> {
    let mut stmt = conn.prepare("DELETE FROM disk_sync_known_copies WHERE id = ?1")?;
    let mut removed = 0;
    for id in ids {
        removed += stmt.execute(params![id])?;
    }
    Ok(removed)
}

/// Forgets every remembered copy, so they are offered again. Returns how many were forgotten.
pub fn clear_known_copies(conn: &Connection) -> Result<i64, DbError> {
    Ok(conn.execute("DELETE FROM disk_sync_known_copies", [])? as i64)
}

/// Drops the oldest rows above [`MAX_KNOWN_COPIES`]; returns how many went.
pub fn trim_known_copies(conn: &Connection) -> Result<usize, DbError> {
    Ok(conn.execute(
        "DELETE FROM disk_sync_known_copies WHERE id IN (
             SELECT id FROM disk_sync_known_copies ORDER BY noted_at DESC, id DESC LIMIT -1 OFFSET ?1)",
        params![MAX_KNOWN_COPIES as i64],
    )?)
}

/// What a row of a foreign backup must look like before the app accepts it: a plain path
/// (no control characters, no `..` component, not endless), a size that is not negative and
/// the 64 hex digits of the content hash. Returns the reason in plain words.
pub fn validate_stored_fields(path: &str, size_bytes: i64, content_hash: &str) -> Result<(), &'static str> {
    if path.trim().is_empty() {
        return Err("empty path");
    }
    if path.chars().take(MAX_KNOWN_COPY_PATH_CHARS + 1).count() > MAX_KNOWN_COPY_PATH_CHARS {
        return Err("path too long");
    }
    if path.chars().any(char::is_control) {
        return Err("control character in path");
    }
    if path.split(['/', '\\']).any(|part| part == "..") {
        return Err("parent component in path");
    }
    if size_bytes < 0 {
        return Err("negative size");
    }
    if content_hash.len() != 64 || !content_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("content hash is not 64 hex digits");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_in_memory;

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn notes_lists_counts_and_clears() {
        let conn = connect_in_memory().unwrap();
        assert_eq!(count_known_copies(&conn).unwrap(), 0);
        note_known_copy(&conn, "/m/a.3mf", 10, HASH, "2026-11-01T10:00:00Z").unwrap();
        note_known_copy(&conn, "/m/b.3mf", 20, HASH, "2026-11-01T10:00:01Z").unwrap();
        let rows = list_known_copies(&conn).unwrap();
        assert_eq!(rows.iter().map(|r| (r.path.as_str(), r.size_bytes)).collect::<Vec<_>>(), vec![("/m/a.3mf", 10), ("/m/b.3mf", 20)]);
        assert_eq!(count_known_copies(&conn).unwrap(), 2);
        assert_eq!(list_known_copies_limited(&conn, 1).unwrap().len(), 1);
        assert_eq!(clear_known_copies(&conn).unwrap(), 2);
        assert_eq!(count_known_copies(&conn).unwrap(), 0);
    }

    #[test]
    fn a_second_decline_of_a_path_replaces_the_row() {
        let conn = connect_in_memory().unwrap();
        note_known_copy(&conn, "/m/a.3mf", 10, HASH, "t1").unwrap();
        let other = "f".repeat(64);
        note_known_copy(&conn, "/m/a.3mf", 11, &other, "t2").unwrap();
        let rows = list_known_copies(&conn).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].size_bytes, rows[0].content_hash.as_str(), rows[0].noted_at.as_str()), (11, other.as_str(), "t2"));
    }

    #[test]
    fn single_rows_can_be_forgotten() {
        let conn = connect_in_memory().unwrap();
        note_known_copy(&conn, "/m/a.3mf", 1, HASH, "t").unwrap();
        note_known_copy(&conn, "/m/b.3mf", 1, HASH, "t").unwrap();
        let first = list_known_copies(&conn).unwrap()[0].id;
        assert_eq!(forget_known_copies(&conn, &[first, 9999]).unwrap(), 1);
        assert_eq!(list_known_copies(&conn).unwrap()[0].path, "/m/b.3mf");
    }

    #[test]
    fn trimming_keeps_the_newest_rows_up_to_the_limit() {
        let conn = connect_in_memory().unwrap();
        conn.execute_batch("BEGIN").unwrap();
        for i in 0..MAX_KNOWN_COPIES + 5 {
            note_known_copy(&conn, &format!("/m/{i}.3mf"), 1, HASH, &format!("2026-11-01T10:00:00.{i:06}Z")).unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
        assert_eq!(trim_known_copies(&conn).unwrap(), 5);
        assert_eq!(count_known_copies(&conn).unwrap(), MAX_KNOWN_COPIES as i64);
        let exists = |p: &str| -> bool {
            conn.query_row("SELECT COUNT(*) FROM disk_sync_known_copies WHERE path = ?1", [p], |r| r.get::<_, i64>(0)).unwrap() == 1
        };
        assert!(!exists("/m/0.3mf") && !exists("/m/4.3mf"), "the oldest five are gone");
        assert!(exists("/m/5.3mf") && exists(&format!("/m/{}.3mf", MAX_KNOWN_COPIES + 4)));
        assert_eq!(trim_known_copies(&conn).unwrap(), 0, "nothing to trim at the limit");
    }

    #[test]
    fn stored_fields_are_checked() {
        assert!(validate_stored_fields("/m/a.3mf", 0, HASH).is_ok());
        assert!(validate_stored_fields("C:\\m\\a.3mf", 5, &HASH.to_uppercase()).is_ok());
        for (why, path, size, hash) in [
            ("empty", " ", 1, HASH),
            ("control", "/m/a\u{7}.3mf", 1, HASH),
            ("dotdot", "/m/../etc/a.3mf", 1, HASH),
            ("dotdot windows", "C:\\m\\..\\a.3mf", 1, HASH),
            ("negative", "/m/a.3mf", -1, HASH),
            ("short hash", "/m/a.3mf", 1, "abcd"),
            ("non-hex hash", "/m/a.3mf", 1, &"g".repeat(64)),
        ] {
            assert!(validate_stored_fields(path, size, hash).is_err(), "{why}");
        }
        let long = format!("/{}", "x".repeat(MAX_KNOWN_COPY_PATH_CHARS));
        assert!(validate_stored_fields(&long, 1, HASH).is_err(), "too long");
    }
}
