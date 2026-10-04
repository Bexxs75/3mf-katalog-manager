use super::*;
use rusqlite::{params, OptionalExtension};

/// Run on a worker thread after AppState is installed, never during setup.
pub(crate) fn backfill_file_modified_at(state: &AppState) {
    match backfill_with_reader(state, files::disk_modified_at) {
        Ok(Some((changed, unreadable))) if changed + unreadable > 0 => {
            log::info!(target: "startup", "Änderungsdatum nachgetragen: {changed} Dateien, {unreadable} nicht lesbar");
        }
        Ok(_) => {}
        Err(error) => {
            log::error!(target: "startup", "Änderungsdatum konnte nicht nachgetragen werden: {error}")
        }
    }
}

fn with_catalog<T>(
    state: &AppState,
    work: impl FnOnce(&mut Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    // Do not queue an exclusive writer behind an import: that would prevent
    // further queue admissions. Only short database operations hold this share.
    let share = loop {
        if let Some(share) = state.import_jobs.gate.try_background_exclusive()? {
            break share;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let mut conn = state
        .db
        .lock()
        .map_err(|_| DbError::Other("Katalogsperre beschädigt".into()))?;
    let result = work(&mut conn);
    drop(conn);
    drop(share);
    result
}

fn backfill_with_reader(
    state: &AppState,
    mut modified: impl FnMut(&Path) -> Option<String>,
) -> Result<Option<(usize, usize)>, DbError> {
    let done = with_catalog(state, |conn| {
        let marker: Option<String> = conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'file_modified_backfill_done'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(marker.as_deref() == Some("1"))
    })?;
    if done {
        return Ok(None);
    }
    let (mut cursor, mut changed, mut unreadable) = (0_i64, 0, 0);
    loop {
        let batch: Vec<(i64, String)> = with_catalog(state, |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, path FROM files WHERE file_modified_at IS NULL AND id > ?1 ORDER BY id LIMIT 200",
            )?;
            let rows = stmt.query_map([cursor], |row| Ok((row.get(0)?, row.get(1)?)))?;
            Ok(rows.collect::<Result<_, _>>()?)
        })?;
        if batch.is_empty() {
            with_catalog(state, |conn| {
                let tx = conn.transaction()?;
                tx.execute("INSERT OR REPLACE INTO app_settings (key, value) VALUES ('file_modified_backfill_done', '1')", [])?;
                tx.commit()?;
                Ok(())
            })?;
            return Ok(Some((changed, unreadable)));
        }
        cursor = batch.last().unwrap().0;
        // Network filesystem calls may block; no application lock spans them.
        // Keyset pagination visits unreadable rows once instead of retrying NULLs.
        let values: Vec<_> = batch
            .into_iter()
            .map(|(id, path)| {
                let time = modified(Path::new(&path));
                (id, path, time)
            })
            .collect();
        let (written, missing) = with_catalog(state, |conn| {
            let tx = conn.transaction()?;
            let (mut written, mut missing) = (0, 0);
            for (id, path, time) in &values {
                if let Some(time) = time {
                    written += tx.execute(
                        "UPDATE files SET file_modified_at = ?1 WHERE id = ?2 AND path = ?3 AND file_modified_at IS NULL",
                        params![time, id, path],
                    )?;
                } else {
                    missing += 1;
                }
            }
            tx.commit()?;
            Ok((written, missing))
        })?;
        changed += written;
        unreadable += missing;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> AppState {
        AppState {
            import_jobs: ImportJobs::default(),
            db: Mutex::new(db::connect_in_memory().unwrap()),
            trash_dir: PathBuf::new(),
            db_path: PathBuf::new(),
            sensitive_dirs: Vec::new(),
        }
    }

    #[test]
    fn backfill_batches_null_rows_preserves_values_and_runs_once() {
        let state = state();
        let dir = unique_test_dir("modified_backfill");
        let path = dir.join("existing.stp");
        std::fs::write(&path, b"ISO-10303-21;").unwrap();
        let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(time)
            .unwrap();
        let expected = chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339();
        {
            let conn = state.db.lock().unwrap();
            for id in 1..=401 {
                conn.execute("INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('old', ?1, 'stp', 1, '')",
                    [if id == 401 { dir.join("missing.stp").to_string_lossy().into_owned() }
                     else { dir.join(format!("existing-{id}.stp")).to_string_lossy().into_owned() }]).unwrap();
            }
            conn.execute(
                "UPDATE files SET path = ?1 WHERE id = 1",
                [path.to_string_lossy().as_ref()],
            )
            .unwrap();
            conn.execute(
                "UPDATE files SET file_modified_at = 'keep' WHERE id = 2",
                [],
            )
            .unwrap();
        }
        // An injected reader avoids needing hundreds of physical files and proves
        // that neither catalog nor database locks span filesystem work.
        let mut reads = 0;
        let result = backfill_with_reader(&state, |p| {
            reads += 1;
            assert!(state.db.try_lock().is_ok());
            let import = state.import_jobs.gate.import();
            assert_eq!(
                state.import_jobs.gate.exclusive().unwrap_err().message,
                "importActive"
            );
            drop(import);
            if p.ends_with("missing.stp") {
                None
            } else {
                files::disk_modified_at(&path)
            }
        })
        .unwrap();
        assert_eq!(result, Some((399, 1)));
        assert_eq!(reads, 400);
        let conn = state.db.lock().unwrap();
        assert_eq!(
            db::get_file(&conn, 1)
                .unwrap()
                .unwrap()
                .file_modified_at
                .as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(
            db::get_file(&conn, 2)
                .unwrap()
                .unwrap()
                .file_modified_at
                .as_deref(),
            Some("keep")
        );
        assert_eq!(
            db::get_file(&conn, 401).unwrap().unwrap().file_modified_at,
            None
        );
        assert_eq!(
            conn.query_row(
                "SELECT value FROM app_settings WHERE key = 'file_modified_backfill_done'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "1"
        );
        drop(conn);
        assert_eq!(
            backfill_with_reader(&state, |_| panic!("second start must not read files")).unwrap(),
            None
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn backfill_does_not_overwrite_concurrent_rescan() {
        let state = state();
        let id = db::test_insert_minimal_file(&state.db.lock().unwrap(), "/tmp/backfill.stl", None)
            .unwrap();
        assert_eq!(
            backfill_with_reader(&state, |_| {
                state
                    .db
                    .lock()
                    .unwrap()
                    .execute(
                        "UPDATE files SET file_modified_at = 'newer' WHERE id = ?1",
                        [id],
                    )
                    .unwrap();
                Some("older".into())
            })
            .unwrap(),
            Some((0, 0))
        );
        assert_eq!(
            db::get_file(&state.db.lock().unwrap(), id)
                .unwrap()
                .unwrap()
                .file_modified_at
                .as_deref(),
            Some("newer")
        );
    }

    #[test]
    fn backfill_real_reader_leaves_missing_file_null() {
        let state = state();
        let dir = unique_test_dir("backfill_real");
        let path = dir.join("existing.stl");
        std::fs::write(&path, b"file contents").unwrap();
        let (present, missing) = {
            let conn = state.db.lock().unwrap();
            (
                db::test_insert_minimal_file(&conn, path.to_str().unwrap(), None).unwrap(),
                db::test_insert_minimal_file(
                    &conn,
                    dir.join("missing.stl").to_str().unwrap(),
                    None,
                )
                .unwrap(),
            )
        };
        assert_eq!(
            backfill_with_reader(&state, files::disk_modified_at).unwrap(),
            Some((1, 1))
        );
        let conn = state.db.lock().unwrap();
        assert_eq!(
            db::get_file(&conn, present)
                .unwrap()
                .unwrap()
                .file_modified_at,
            files::disk_modified_at(&path)
        );
        assert_eq!(
            db::get_file(&conn, missing)
                .unwrap()
                .unwrap()
                .file_modified_at,
            None
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn backfill_yields_to_import_and_does_not_write_a_changed_path() {
        let state = state();
        let id = db::test_insert_minimal_file(&state.db.lock().unwrap(), "/tmp/before.stl", None)
            .unwrap();
        let share = state.import_jobs.gate.import();
        let (sent, received) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                backfill_with_reader(&state, |_| {
                    sent.send(()).unwrap();
                    let import = state.import_jobs.gate.import();
                    state
                        .db
                        .lock()
                        .unwrap()
                        .execute(
                            "UPDATE files SET path = '/tmp/after.stl' WHERE id = ?1",
                            [id],
                        )
                        .unwrap();
                    drop(import);
                    Some("date".into())
                })
            });
            assert!(matches!(
                received.recv_timeout(std::time::Duration::from_millis(100)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ));
            // New imports can still enter while maintenance is waiting.
            drop(state.import_jobs.gate.import());
            drop(share);
            assert_eq!(worker.join().unwrap().unwrap(), Some((0, 0)));
        });
        assert_eq!(
            db::get_file(&state.db.lock().unwrap(), id)
                .unwrap()
                .unwrap()
                .file_modified_at,
            None
        );
    }

    #[test]
    fn backfill_failed_transaction_does_not_set_marker() {
        let state = state();
        {
            let conn = state.db.lock().unwrap();
            db::test_insert_minimal_file(&conn, "/tmp/fail.stl", None).unwrap();
            conn.execute_batch("CREATE TRIGGER reject_modified BEFORE UPDATE OF file_modified_at ON files BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
        }
        assert!(backfill_with_reader(&state, |_| Some("date".into())).is_err());
        let conn = state.db.lock().unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM app_settings WHERE key = 'file_modified_backfill_done'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db::get_file(&conn, 1).unwrap().unwrap().file_modified_at,
            None
        );
    }
}
