//! Importvertrag §9: real SQLite faults against the batched import path.
use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "import_contract_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn stl(&self, name: &str, seed: u32) -> PathBuf {
        let path = self.0.join(name);
        let mut bytes = vec![0u8; 80];
        bytes.extend_from_slice(&1u32.to_le_bytes());
        let s = seed as f32;
        for value in [
            0.0f32,
            0.0,
            1.0,
            0.0,
            0.0,
            s,
            10.0,
            0.0,
            s,
            0.0,
            10.0,
            s + 1.0,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Remove the fault only after SQLite has actually rejected a COMMIT. This
/// leaves production responsible for retaining and retrying all batch inputs.
struct OneCommitFault {
    conn: Connection,
    rows_after_failure: Option<usize>,
}
impl ImportDb for OneCommitFault {
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> CmdResult<R>) -> CmdResult<R> {
        let outcome = f(&mut self.conn);
        if outcome
            .as_ref()
            .is_err_and(|error| error.to_string().contains("FOREIGN KEY constraint failed"))
        {
            assert!(
                self.rows_after_failure.is_none(),
                "batch retried more than once without clearing the fault"
            );
            assert!(
                self.conn.is_autocommit(),
                "failed batch must be rolled back"
            );
            self.rows_after_failure = Some(db::list_files(&self.conn).unwrap().len());
            self.conn
                .execute_batch("DROP TRIGGER fail_second_commit;")
                .unwrap();
        }
        outcome
    }
}

#[test]
fn contract_02_batch_two_commit_failure_preserves_inputs_and_batch_one() {
    let fixture = Fixture::new();
    let paths = (0..50)
        .map(|i| fixture.stl(&format!("part_{i}.stl"), i))
        .collect();
    let conn = db::connect_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE deferred_fault (file_id INTEGER REFERENCES files(id) DEFERRABLE INITIALLY DEFERRED);
         CREATE TRIGGER fail_second_commit AFTER INSERT ON files
         WHEN (SELECT COUNT(*) FROM files) = 27
         BEGIN INSERT INTO deferred_fault VALUES (-1); END;"
    ).unwrap();
    let mut database = OneCommitFault {
        conn,
        rows_after_failure: None,
    };
    let result = import_many_in_batches(&mut database, paths, ImportMode::Batched)
        .expect("second batch must be recovered through individual transactions");
    assert_eq!(
        database.rows_after_failure,
        Some(25),
        "batch one survives, no rows from the failed batch survive"
    );
    assert_eq!(db::list_files(&database.conn).unwrap().len(), 50);
    assert_eq!(
        result.imported.len(),
        50,
        "publish each committed file exactly once"
    );
    assert!(result.skipped.is_empty());
    assert_eq!(result.duplicate_count, 0);
}

#[test]
fn contract_03_statement_after_file_insert_rolls_back_only_failed_file() {
    let fixture = Fixture::new();
    let bad = fixture.stl("bad.stl", 1);
    let good = fixture.stl("good.stl", 2);
    let mut conn = db::connect_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TRIGGER fail_metadata BEFORE INSERT ON file_metadata
         WHEN NEW.value = 'reject_this_file'
         BEGIN SELECT RAISE(ABORT, 'injected metadata failure'); END;",
    )
    .unwrap();
    let mut bad_file = read_model_file(&bad, None).unwrap();
    bad_file
        .metadata
        .insert("test".to_string(), "reject_this_file".to_string());
    let good_file = read_model_file(&good, None).unwrap();
    let mut batch = vec![
        PendingImport {
            path: bad.clone(),
            folder_root: None,
            new_file: bad_file,
        },
        PendingImport {
            path: good.clone(),
            folder_root: None,
            new_file: good_file,
        },
    ];
    let mut result = ImportResultDto {
        duplicate_entries: Vec::new(),
        imported: vec![],
        duplicate_count: 0,
        pending_archives: vec![],
        skipped: vec![],
    };
    store_batch(&mut &mut conn, &mut batch, &mut result, ImportMode::Batched).unwrap();
    let rows = db::list_files(&conn).unwrap();
    assert_eq!(
        rows.len(),
        1,
        "failed metadata insert must not leave an orphan files row"
    );
    assert_eq!(rows[0].path, good.to_string_lossy());
    assert_eq!(result.imported.len(), 1);
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].path, bad.to_string_lossy());
    assert_eq!(result.skipped[0].reason, SkipReason::Failed);
}

#[test]
fn contract_09_failed_hash_representative_allows_next_candidate() {
    let fixture = Fixture::new();
    let bad = fixture.stl("bad.stl", 1);
    let good = fixture.stl("good.stl", 1);
    let mut conn = db::connect_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TRIGGER fail_representative BEFORE INSERT ON files
         WHEN NEW.name = 'bad.stl'
         BEGIN SELECT RAISE(ABORT, 'path-dependent insert failure'); END;",
    )
    .unwrap();
    let result = import_many_with_conn(&mut conn, vec![bad.clone(), good.clone()]).unwrap();
    assert_eq!(
        result.imported.len(),
        1,
        "next hash candidate must replace the failed representative"
    );
    assert_eq!(result.imported[0].path, good.to_string_lossy());
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].path, bad.to_string_lossy());
    assert_eq!(result.skipped[0].reason, SkipReason::Failed);
    assert_eq!(
        result.duplicate_count, 0,
        "uncommitted representative is not a duplicate"
    );
    assert_eq!(db::list_files(&conn).unwrap().len(), 1);
}

struct FailedReleaseDb(Connection);
impl ImportDb for FailedReleaseDb {
    fn savepoint_control(&self) -> fn(&Connection, &str) -> rusqlite::Result<()> {
        |conn, sql| {
            if sql.starts_with("RELEASE") {
                Err(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_IOERR),
                    Some("injected RELEASE failure".into()),
                ))
            } else {
                conn.execute_batch(sql)
            }
        }
    }
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> CmdResult<R>) -> CmdResult<R> {
        f(&mut self.0)
    }
}
#[test]
fn contract_03_release_failure_is_database_error_and_publishes_no_rows() {
    let fixture = Fixture::new();
    let path = fixture.stl("release.stl", 1);
    let mut database = FailedReleaseDb(db::connect_in_memory().unwrap());
    let mut batch = vec![PendingImport {
        path: path.clone(),
        folder_root: None,
        new_file: read_model_file(&path, None).unwrap(),
    }];
    let mut result = empty_import_result();
    let error =
        store_batch(&mut database, &mut batch, &mut result, ImportMode::Batched).unwrap_err();
    assert!(error.message.starts_with("database:"));
    assert!(error.message.contains("injected RELEASE failure"));
    assert_eq!(
        batch.len(),
        1,
        "failed inputs remain available to classify the job"
    );
    assert!(result.imported.is_empty());
    assert!(
        result.skipped.is_empty(),
        "RELEASE failure must not be classified as a file error"
    );
    assert!(db::list_files(&database.0).unwrap().is_empty());
    assert!(database.0.is_autocommit());
}

#[test]
fn contract_02_file_related_single_commit_failure_skips_only_that_input() {
    let fixture = Fixture::new();
    let bad = fixture.stl("bad.stl", 1);
    let good = fixture.stl("good.stl", 2);
    let mut conn = db::connect_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE deferred_fault (file_id INTEGER REFERENCES files(id) DEFERRABLE INITIALLY DEFERRED);
        CREATE TRIGGER file_commit_fault AFTER INSERT ON files WHEN NEW.name = 'bad.stl'
        BEGIN INSERT INTO deferred_fault VALUES (-1); END;").unwrap();
    let result = import_many_with_conn(&mut conn, vec![bad.clone(), good.clone()]).unwrap();
    assert_eq!(result.imported.len(), 1);
    assert_eq!(result.imported[0].path, good.to_string_lossy());
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].path, bad.to_string_lossy());
    assert_eq!(result.skipped[0].reason, SkipReason::Failed);
    assert_eq!(db::list_files(&conn).unwrap().len(), 1);
}
