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
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> core::ImportResult<R>) -> core::ImportResult<R> {
        let mut conn = self.db.lock().map_err(|_| "database: lock poisoned")?;
        f(&mut conn)
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
