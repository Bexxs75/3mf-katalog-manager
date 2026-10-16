//! Desktop archive execution adapter.
use super::*;
pub(super) fn run_archives(
    app: &tauri::AppHandle,
    state: &AppState,
    job: &ImportJob,
    target: &Path,
    requests: Vec<ArchiveRequest>,
    delete: bool,
) -> CmdResult<()> {
    use tauri::Emitter;
    job.begin_archive_candidates(requests.iter().map(|r| r.path.clone()).collect());
    let mut legacy = ArchiveImportResultDto {
        imported: Vec::new(),
        duplicate_count: 0,
        archives: Vec::new(),
        skipped: Vec::new(),
    };
    let mut database_error = None;
    for request in requests {
        if job.cancel.load(Ordering::Acquire) || database_error.is_some() {
            job.skip(Path::new(&request.path), "notStarted");
            legacy.archives.push(ArchiveOutcomeDto {
                path: request.path.clone(),
                error: Some("notStarted".into()),
                ..Default::default()
            });
            continue;
        }
        let archive_path = request.path.clone();
        job.set_current(Some(archive_path.clone()));
        let result = archives::extract_archives_controlled(
            &state.sensitive_dirs,
            target,
            vec![request],
            delete,
            |dir, created| {
                let mut database = ArchiveJobDb {
                    db: &state.db,
                    job,
                    reader: state.import_jobs.reader(),
                };
                let result = files::import_many_selected(
                    &mut database,
                    vec![dir.to_path_buf()],
                    files::ImportMode::Atomic,
                    Some(&job.cancel),
                    Some(created),
                )?;
                // Import has committed. Cosmetic folder attachment must never
                // send an error back to the extraction rollback path.
                if !result.imported.is_empty() {
                    match state.db.lock() {
                        Ok(conn) => {
                            if let Err(error) = db::attach_folder_to_parent_by_path(&conn, dir) {
                                log::warn!(target: "archive", "folder attachment failed: {error}");
                            }
                        }
                        Err(error) => {
                            log::warn!(target: "archive", "folder attachment lock failed: {error}")
                        }
                    }
                }
                Ok(result)
            },
            |path, stage| {
                let _ = app.emit(
                    "archive-progress",
                    serde_json::json!({"path":path,"state":stage}),
                );
            },
            Some(&job.cancel),
        );
        let result = match result {
            Ok(result) => result,
            Err(error) => ArchiveImportResultDto {
                imported: Vec::new(),
                duplicate_count: 0,
                skipped: Vec::new(),
                archives: vec![ArchiveOutcomeDto {
                    path: archive_path,
                    error: Some(error.message),
                    ..Default::default()
                }],
            },
        };
        for outcome in &result.archives {
            let mut value = serde_json::to_value(outcome).unwrap();
            value["models"] = serde_json::to_value(&outcome.models).unwrap();
            value["state"] = serde_json::json!(if outcome.error.is_some() {
                "failed"
            } else {
                "finished"
            });
            job.push_archive(value);
            if outcome
                .error
                .as_ref()
                .is_some_and(|e| e.starts_with("database:"))
            {
                database_error = outcome.error.clone();
            }
        }
        legacy.imported.extend(result.imported);
        legacy.duplicate_count += result.duplicate_count;
        legacy.skipped.extend(result.skipped);
        legacy.archives.extend(result.archives);
        job.set_in_flight(0);
        job.set_current(None);
    }
    job.set_archive_result(legacy);
    if let Some(error) = database_error {
        Err(error.into())
    } else {
        Ok(())
    }
}

struct ArchiveJobDb<'a> {
    db: &'a Mutex<Connection>,
    job: &'a ImportJob,
    reader: &'a dyn ImportReader,
}
impl files::ImportDb for ArchiveJobDb<'_> {
    fn hash(&mut self, path: &Path) -> core::ImportResult<String> {
        files::compute_content_hash(path).map_err(Into::into)
    }
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> core::ImportResult<R>) -> core::ImportResult<R> {
        let result = self.db.lock()
            .map_err(|_| core::ImportError::from("database: lock poisoned"))
            .and_then(|mut conn| f(&mut conn));
        if let Err(error) = &result {
            // Failed commits may be classified as expected for recovery, but
            // still need error-level diagnostics in the desktop archive path.
            if !error.expected || error.message.starts_with("database:") {
                log::error!(target: "import", "{error}");
            }
        }
        result
    }
    fn read_file(&mut self, path: &Path) -> core::ImportResult<NewFile> {
        self.job.set_current(Some(path.to_string_lossy().into_owned()));
        let result = self.reader.read(path);
        self.job.set_current(None);
        result
    }
    fn report_in_flight(&self, count: usize) {
        self.job.set_in_flight(count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use files::ImportDb;
    use std::cell::RefCell;

    thread_local! {
        static ERRORS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }
    struct Logger;
    impl log::Log for Logger {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool { true }
        fn log(&self, record: &log::Record<'_>) {
            if record.level() == log::Level::Error {
                ERRORS.with(|errors| errors.borrow_mut().push(record.args().to_string()));
            }
        }
        fn flush(&self) {}
    }
    fn capture(f: impl FnOnce()) -> Vec<String> {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            log::set_logger(&Logger).unwrap();
            log::set_max_level(log::LevelFilter::Trace);
        });
        ERRORS.with(|errors| errors.borrow_mut().clear());
        f();
        ERRORS.with(|errors| std::mem::take(&mut *errors.borrow_mut()))
    }
    fn with_database(f: impl FnOnce(&mut ArchiveJobDb<'_>)) {
        let db = Mutex::new(db::connect_in_memory().unwrap());
        let service = <ImportJobs as DesktopJobs>::default();
        let job = service.enqueue(ImportSource::Files, None, false, None).unwrap();
        f(&mut ArchiveJobDb { db: &db, job: &job, reader: service.reader() });
    }
    #[test]
    fn archive_hash_failure_logs_error_and_remains_invalid() {
        let missing = std::env::temp_dir().join(format!("archive_missing_{}.stl", std::process::id()));
        assert!(!missing.exists());
        let errors = capture(|| with_database(|database| {
            let result = files::import_many_selected(database, vec![missing], files::ImportMode::Atomic, None, None).unwrap();
            assert!(result.imported.is_empty());
            assert_eq!(result.skipped.len(), 1);
            assert_eq!(result.skipped[0].reason, files::SkipReason::Invalid);
        }));
        assert!(!errors.is_empty(), "hash failure must reach error-level diagnostics");
    }
    #[test]
    fn archive_database_failure_logs_error_without_changing_result() {
        let errors = capture(|| with_database(|database| {
            let error = database.with_conn(|conn| {
                conn.execute_batch("INSERT INTO missing_table VALUES (1)")
                    .map_err(|e| core::ImportError::from(format!("database: {e}")))
            }).unwrap_err();
            assert!(!error.expected);
            assert!(error.message.contains("missing_table"));
        }));
        assert!(errors.iter().any(|e| e.contains("missing_table")));
    }
    #[test]
    fn archive_commit_failure_logs_error_even_when_expected() {
        let errors = capture(|| with_database(|database| {
            let error = database.with_conn(|conn| {
                conn.execute_batch("CREATE TABLE parent_test(id INTEGER PRIMARY KEY); CREATE TABLE child_test(id INTEGER REFERENCES parent_test(id) DEFERRABLE INITIALLY DEFERRED)").unwrap();
                let tx = conn.transaction().unwrap();
                tx.execute("INSERT INTO child_test VALUES (1)", []).unwrap();
                tx.commit().map_err(|e| core::ImportError::expected(format!("database: {e}")))
            }).unwrap_err();
            assert!(error.expected);
            assert!(error.message.contains("FOREIGN KEY"));
        }));
        assert!(errors.iter().any(|e| e.contains("FOREIGN KEY")));
    }
}
