//! Fills in the content hash of catalog rows that have none.
//!
//! A restored backup arrives without hashes (duplicate decisions need hashes of the
//! local files), and old catalogs predate the column. The host decides which locks a
//! catalog access takes and which paths may be read; file I/O runs outside the catalog.
use crate::db::error::DbError;
use rusqlite::Connection;

const BATCH: usize = 200;

/// One short catalog access, run by the host under its own locks.
pub type CatalogWork<'a> = &'a mut dyn FnMut(&mut Connection) -> Result<(), DbError>;

/// What the host contributes to one run.
pub trait HashBackfill {
    /// Runs the access that reads the next rows. The host may read, in the same
    /// access, whatever `hash` needs to judge the paths of that batch.
    fn read_batch(&mut self, work: CatalogWork<'_>) -> Result<(), DbError>;
    /// Runs the access that stores one hash.
    fn store(&mut self, work: CatalogWork<'_>) -> Result<(), DbError>;
    /// Hash of the file at `path`. `None` when the host does not admit the path:
    /// the row is left alone and nothing is read.
    fn hash(&mut self, path: &str) -> Option<Result<String, String>>;
    /// A file that was admitted but could not be hashed; the run goes on.
    fn failed(&mut self, _path: &str, _error: &str) {}
    /// Called after every row, for progress reports.
    fn progress(&mut self, _summary: &HashBackfillSummary) {}
    /// Checked before every row; `true` ends the run early.
    fn stop(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HashBackfillSummary {
    /// Rows that received their hash.
    pub hashed: usize,
    /// Admitted files that could not be read.
    pub failed: usize,
    /// Rows whose path the host did not admit.
    pub refused: usize,
    /// Every row without a hash was visited once; `false` after `stop`.
    pub complete: bool,
}

/// Whether an active model still waits for its hash.
pub fn has_missing_content_hash(conn: &Connection) -> Result<bool, DbError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM files WHERE content_hash IS NULL AND deleted_at IS NULL)",
        [],
        |row| row.get(0),
    )?)
}

/// Visits every row without a hash once, in id order. A row keeps its newer data when
/// a rescan or rename changed it while its file was read.
pub fn backfill_content_hashes(
    host: &mut impl HashBackfill,
) -> Result<HashBackfillSummary, DbError> {
    let mut summary = HashBackfillSummary::default();
    let mut cursor = 0_i64;
    loop {
        let mut batch: Vec<(i64, String)> = Vec::new();
        host.read_batch(&mut |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT id, path FROM files WHERE content_hash IS NULL AND id > ?1 ORDER BY id LIMIT {BATCH}"
            ))?;
            let rows = stmt.query_map([cursor], |row| Ok((row.get(0)?, row.get(1)?)))?;
            batch = rows.collect::<Result<Vec<_>, _>>()?;
            Ok(())
        })?;
        let Some(last) = batch.last() else {
            summary.complete = true;
            return Ok(summary);
        };
        cursor = last.0;
        for (id, path) in batch {
            if host.stop() {
                return Ok(summary);
            }
            match host.hash(&path) {
                None => summary.refused += 1,
                Some(Ok(hash)) => {
                    host.store(&mut |conn| {
                        // A rescan or rename during the read must keep its newer data.
                        conn.execute(
                            "UPDATE files SET content_hash = ?1 WHERE id = ?2 AND path = ?3 AND content_hash IS NULL",
                            rusqlite::params![hash, id, path],
                        )?;
                        Ok(())
                    })?;
                    summary.hashed += 1;
                }
                Some(Err(error)) => {
                    summary.failed += 1;
                    host.failed(&path, &error);
                }
            }
            host.progress(&summary);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Host<'a> {
        conn: Connection,
        stop: &'a AtomicBool,
        stop_after: Option<usize>,
        seen: Vec<String>,
        failures: Vec<String>,
    }
    impl HashBackfill for Host<'_> {
        fn read_batch(&mut self, work: CatalogWork<'_>) -> Result<(), DbError> {
            work(&mut self.conn)
        }
        fn store(&mut self, work: CatalogWork<'_>) -> Result<(), DbError> {
            work(&mut self.conn)
        }
        fn hash(&mut self, path: &str) -> Option<Result<String, String>> {
            self.seen.push(path.into());
            match path {
                p if p.starts_with("/outside/") => None,
                p if p.ends_with("gone.stl") => Some(Err("missing".into())),
                p => Some(Ok(format!("hash-of-{p}"))),
            }
        }
        fn failed(&mut self, path: &str, _: &str) {
            self.failures.push(path.into());
        }
        fn progress(&mut self, summary: &HashBackfillSummary) {
            if self.stop_after == Some(summary.hashed) {
                self.stop.store(true, Ordering::SeqCst);
            }
        }
        fn stop(&self) -> bool {
            self.stop.load(Ordering::SeqCst)
        }
    }

    fn catalog(paths: &[(&str, Option<&str>)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT, content_hash TEXT, deleted_at TEXT)",
        )
        .unwrap();
        for (path, hash) in paths {
            conn.execute("INSERT INTO files (path, content_hash) VALUES (?1, ?2)", rusqlite::params![path, hash])
                .unwrap();
        }
        conn
    }
    fn hashes(conn: &Connection) -> Vec<Option<String>> {
        conn.prepare("SELECT content_hash FROM files ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn fills_missing_hashes_and_leaves_refused_failed_and_known_rows_alone() {
        let stop = AtomicBool::new(false);
        let mut host = Host {
            conn: catalog(&[
                ("/models/a.stl", None),
                ("/models/b.stl", Some("kept")),
                ("/outside/c.stl", None),
                ("/models/gone.stl", None),
                ("/models/d.stl", None),
            ]),
            stop: &stop,
            stop_after: None,
            seen: Vec::new(),
            failures: Vec::new(),
        };
        assert!(has_missing_content_hash(&host.conn).unwrap());
        let summary = backfill_content_hashes(&mut host).unwrap();
        assert_eq!(summary, HashBackfillSummary { hashed: 2, failed: 1, refused: 1, complete: true });
        assert_eq!(
            hashes(&host.conn),
            [Some("hash-of-/models/a.stl".into()), Some("kept".into()), None, None, Some("hash-of-/models/d.stl".into())]
        );
        // A row that already has its hash is never offered to the host.
        assert!(!host.seen.contains(&"/models/b.stl".to_string()));
        assert_eq!(host.failures, ["/models/gone.stl"]);
    }

    #[test]
    fn more_rows_than_one_batch_are_all_visited_once() {
        let rows: Vec<String> = (0..BATCH * 2 + 3).map(|i| format!("/models/{i}.stl")).collect();
        let stop = AtomicBool::new(false);
        let mut host = Host {
            conn: catalog(&rows.iter().map(|p| (p.as_str(), None)).collect::<Vec<_>>()),
            stop: &stop,
            stop_after: None,
            seen: Vec::new(),
            failures: Vec::new(),
        };
        let summary = backfill_content_hashes(&mut host).unwrap();
        assert_eq!((summary.hashed, summary.complete), (rows.len(), true));
        assert_eq!(host.seen, rows);
        assert!(!has_missing_content_hash(&host.conn).unwrap());
    }

    #[test]
    fn a_stop_ends_the_run_as_incomplete_and_keeps_what_was_stored() {
        let stop = AtomicBool::new(false);
        let mut host = Host {
            conn: catalog(&[("/models/a.stl", None), ("/models/b.stl", None), ("/models/c.stl", None)]),
            stop: &stop,
            stop_after: Some(1),
            seen: Vec::new(),
            failures: Vec::new(),
        };
        let summary = backfill_content_hashes(&mut host).unwrap();
        assert_eq!(summary, HashBackfillSummary { hashed: 1, failed: 0, refused: 0, complete: false });
        assert_eq!(hashes(&host.conn), [Some("hash-of-/models/a.stl".into()), None, None]);
    }

    #[test]
    fn a_row_in_the_trash_does_not_count_as_waiting() {
        let conn = catalog(&[("/models/a.stl", None)]);
        conn.execute("UPDATE files SET deleted_at = 'x'", []).unwrap();
        assert!(!has_missing_content_hash(&conn).unwrap());
    }
}
