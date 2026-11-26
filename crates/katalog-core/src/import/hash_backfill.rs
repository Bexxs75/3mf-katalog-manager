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
    /// Admitted files that could not be read, or rows that kept changing under the run.
    pub failed: usize,
    /// Rows whose path the host did not admit.
    pub refused: usize,
    /// Every row without a hash was dealt with; `false` after `stop`.
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

/// Rounds in which rows that changed under the run are read again. Bounded, so a row
/// that never comes to rest cannot keep a run (and what waits for it) alive for ever.
const RETRY_ROUNDS: usize = 3;

/// A row the first look could not settle: it is looked at again with its current path.
struct Open {
    id: i64,
    /// The path that was tried.
    path: String,
    /// Why the file could not be read; `None` when the row changed before the hash was stored.
    error: Option<String>,
}

enum Outcome {
    Settled,
    Open(Open),
    Stopped,
}

/// Hashes one row and stores the result unless the row changed meanwhile.
fn visit(
    host: &mut impl HashBackfill,
    summary: &mut HashBackfillSummary,
    id: i64,
    path: String,
) -> Result<Outcome, DbError> {
    if host.stop() {
        return Ok(Outcome::Stopped);
    }
    let outcome = match host.hash(&path) {
        None => {
            summary.refused += 1;
            Outcome::Settled
        }
        Some(Ok(hash)) => {
            let mut stored = 0;
            host.store(&mut |conn| {
                // A rescan or rename during the read must keep its newer data.
                stored = conn.execute(
                    "UPDATE files SET content_hash = ?1 WHERE id = ?2 AND path = ?3 AND content_hash IS NULL",
                    rusqlite::params![hash, id, path],
                )?;
                Ok(())
            })?;
            if stored == 0 {
                // Renamed, moved or rescanned while the file was read.
                Outcome::Open(Open { id, path, error: None })
            } else {
                summary.hashed += 1;
                Outcome::Settled
            }
        }
        // A read the stop itself ended says nothing about the file.
        Some(Err(_)) if host.stop() => return Ok(Outcome::Stopped),
        // The file may just have been renamed away: decided on the second look.
        Some(Err(error)) => Outcome::Open(Open { id, path, error: Some(error) }),
    };
    host.progress(summary);
    Ok(outcome)
}

/// Visits every row without a hash, in id order. A row keeps its newer data when a
/// rescan changed it while its file was read; a row that was renamed or moved meanwhile
/// is read again at its new path.
pub fn backfill_content_hashes(
    host: &mut impl HashBackfill,
) -> Result<HashBackfillSummary, DbError> {
    let mut summary = HashBackfillSummary::default();
    let mut open: Vec<Open> = Vec::new();
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
        let Some(last) = batch.last() else { break };
        cursor = last.0;
        for (id, path) in batch {
            match visit(host, &mut summary, id, path)? {
                Outcome::Settled => (),
                Outcome::Open(row) => open.push(row),
                Outcome::Stopped => return Ok(summary),
            }
        }
    }
    for _ in 0..RETRY_ROUNDS {
        if open.is_empty() {
            break;
        }
        for rows in std::mem::take(&mut open).chunks(BATCH) {
            // The current path of each row; `None` when it has its hash by now or is gone.
            let mut current: Vec<Option<String>> = Vec::new();
            host.read_batch(&mut |conn| {
                let mut stmt = conn.prepare("SELECT path FROM files WHERE id = ?1 AND content_hash IS NULL")?;
                current.clear();
                for row in rows {
                    let mut found = stmt.query_map([row.id], |r| r.get(0))?;
                    current.push(found.next().transpose()?);
                }
                Ok(())
            })?;
            for (row, path) in rows.iter().zip(current) {
                let Some(path) = path else { continue };
                if let (Some(error), true) = (&row.error, path == row.path) {
                    // Still the same place: the file itself is the problem.
                    summary.failed += 1;
                    host.failed(&path, error);
                    host.progress(&summary);
                    continue;
                }
                match visit(host, &mut summary, row.id, path)? {
                    Outcome::Settled => (),
                    Outcome::Open(row) => open.push(row),
                    Outcome::Stopped => return Ok(summary),
                }
            }
        }
    }
    // Rows that changed in every round count as not hashed; the run still ends.
    for row in open {
        summary.failed += 1;
        host.failed(&row.path, row.error.as_deref().unwrap_or("changed while it was read"));
    }
    // A stop that arrived with the last row must not pass for a finished run.
    summary.complete = !host.stop();
    Ok(summary)
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
        /// Runs before the default answer of `hash`; `Some` replaces that answer.
        script: Option<Script>,
    }
    type Script = Box<dyn FnMut(&mut Connection, &str, &AtomicBool) -> Option<Result<String, String>>>;
    impl HashBackfill for Host<'_> {
        fn read_batch(&mut self, work: CatalogWork<'_>) -> Result<(), DbError> {
            work(&mut self.conn)
        }
        fn store(&mut self, work: CatalogWork<'_>) -> Result<(), DbError> {
            work(&mut self.conn)
        }
        fn hash(&mut self, path: &str) -> Option<Result<String, String>> {
            self.seen.push(path.into());
            if let Some(script) = &mut self.script {
                if let Some(answer) = script(&mut self.conn, path, self.stop) {
                    return Some(answer);
                }
            }
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
            script: None,
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
            script: None,
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
            script: None,
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

    fn scripted<'a>(conn: Connection, stop: &'a AtomicBool, script: Script) -> Host<'a> {
        Host { conn, stop, stop_after: None, seen: Vec::new(), failures: Vec::new(), script: Some(script) }
    }

    // N75-001: the read of the last waiting file is cut off by the stop itself.
    #[test]
    fn a_read_ended_by_the_stop_leaves_the_run_incomplete_and_the_next_run_finishes_it() {
        let stop = AtomicBool::new(false);
        let mut host = scripted(
            catalog(&[("/models/only.stl", None)]),
            &stop,
            Box::new(|_, _, stop| {
                if stop.swap(true, Ordering::SeqCst) {
                    return None;
                }
                Some(Err("cancelled".into()))
            }),
        );
        let summary = backfill_content_hashes(&mut host).unwrap();
        assert_eq!(summary, HashBackfillSummary::default());
        assert!(host.failures.is_empty());
        assert!(has_missing_content_hash(&host.conn).unwrap());

        stop.store(false, Ordering::SeqCst);
        host.script = None;
        let summary = backfill_content_hashes(&mut host).unwrap();
        assert_eq!(summary, HashBackfillSummary { hashed: 1, failed: 0, refused: 0, complete: true });
        assert_eq!(hashes(&host.conn), [Some("hash-of-/models/only.stl".into())]);
    }

    #[test]
    fn a_stop_during_the_last_successful_read_does_not_pass_for_a_finished_run() {
        let stop = AtomicBool::new(false);
        let mut host = scripted(
            catalog(&[("/models/only.stl", None)]),
            &stop,
            Box::new(|_, _, stop| {
                stop.store(true, Ordering::SeqCst);
                None
            }),
        );
        let summary = backfill_content_hashes(&mut host).unwrap();
        assert_eq!(summary, HashBackfillSummary { hashed: 1, failed: 0, refused: 0, complete: false });
    }

    // N75-002: renamed between reading the batch and storing the hash.
    #[test]
    fn a_row_renamed_while_its_file_was_read_gets_the_hash_of_its_new_path() {
        let stop = AtomicBool::new(false);
        let mut host = scripted(
            catalog(&[("/models/old.stl", None), ("/models/b.stl", None)]),
            &stop,
            Box::new(|conn, path, _| {
                if path == "/models/old.stl" {
                    conn.execute("UPDATE files SET path = '/models/new.stl' WHERE path = ?1", [path]).unwrap();
                }
                None
            }),
        );
        let summary = backfill_content_hashes(&mut host).unwrap();
        assert_eq!(summary, HashBackfillSummary { hashed: 2, failed: 0, refused: 0, complete: true });
        assert_eq!(
            hashes(&host.conn),
            [Some("hash-of-/models/new.stl".into()), Some("hash-of-/models/b.stl".into())]
        );
        assert_eq!(host.seen, ["/models/old.stl", "/models/b.stl", "/models/new.stl"]);
        assert!(host.failures.is_empty());
    }

    #[test]
    fn a_file_that_vanished_because_its_row_moved_is_read_at_the_new_path_and_is_no_failure() {
        let stop = AtomicBool::new(false);
        let mut host = scripted(
            catalog(&[("/models/old.stl", None)]),
            &stop,
            Box::new(|conn, path, _| {
                (path == "/models/old.stl").then(|| {
                    conn.execute("UPDATE files SET path = '/models/moved.stl'", []).unwrap();
                    Err("not found".into())
                })
            }),
        );
        let summary = backfill_content_hashes(&mut host).unwrap();
        assert_eq!(summary, HashBackfillSummary { hashed: 1, failed: 0, refused: 0, complete: true });
        assert_eq!(hashes(&host.conn), [Some("hash-of-/models/moved.stl".into())]);
        assert!(host.failures.is_empty());
    }

    #[test]
    fn a_row_filled_in_by_someone_else_meanwhile_keeps_that_hash_and_is_not_counted() {
        let stop = AtomicBool::new(false);
        let mut host = scripted(
            catalog(&[("/models/a.stl", None)]),
            &stop,
            Box::new(|conn, _, _| {
                conn.execute("UPDATE files SET content_hash = 'rescan'", []).unwrap();
                None
            }),
        );
        let summary = backfill_content_hashes(&mut host).unwrap();
        assert_eq!(summary, HashBackfillSummary { hashed: 0, failed: 0, refused: 0, complete: true });
        assert_eq!(hashes(&host.conn), [Some("rescan".into())]);
    }

    #[test]
    fn a_row_that_never_comes_to_rest_ends_the_run_after_a_few_rounds_as_a_failure() {
        let stop = AtomicBool::new(false);
        let mut host = scripted(
            catalog(&[("/models/0.stl", None)]),
            &stop,
            Box::new(|conn, _, _| {
                conn.execute("UPDATE files SET path = path || 'x'", []).unwrap();
                None
            }),
        );
        let summary = backfill_content_hashes(&mut host).unwrap();
        assert_eq!(summary, HashBackfillSummary { hashed: 0, failed: 1, refused: 0, complete: true });
        assert_eq!(host.seen.len(), 1 + RETRY_ROUNDS);
        assert_eq!(hashes(&host.conn), [None]);
        assert_eq!(host.failures.len(), 1);
    }
}
