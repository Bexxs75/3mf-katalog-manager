//! Blocking import work; catalog locks only cover database access.
use super::*;

fn scan_candidates(
    job: &ImportJob,
    roots: &[PathBuf],
    limit: usize,
) -> Vec<(PathBuf, Option<PathBuf>)> {
    scan_candidates_observed(job, roots, limit, || {})
}

fn scan_candidates_observed(
    job: &ImportJob,
    roots: &[PathBuf],
    limit: usize,
    mut observe: impl FnMut(),
) -> Vec<(PathBuf, Option<PathBuf>)> {
    fn visit(
        job: &ImportJob,
        path: &Path,
        root: Option<&Path>,
        out: &mut Vec<(PathBuf, Option<PathBuf>)>,
        limit: usize,
        observe: &mut impl FnMut(),
    ) -> bool {
        if job.cancel.load(Ordering::Acquire) || out.len() >= limit {
            return false;
        }
        if path.is_symlink() {
            return true;
        }
        if path.is_dir() {
            job.data.lock().unwrap().current = Some(path.to_string_lossy().into_owned());
            observe();
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    if !visit(job, &entry.path(), root, out, limit, observe) {
                        return false;
                    }
                }
            }
        } else if root.is_none() || files::is_supported_extension(path) {
            out.push((path.to_path_buf(), root.map(Path::to_path_buf)));
            let mut data = job.data.lock().unwrap();
            data.known = out.len();
            data.current = path.parent().map(|p| p.to_string_lossy().into_owned());
            drop(data);
            observe();
        }
        true
    }
    let mut out = Vec::new();
    let mut complete = true;
    for root in roots {
        if !visit(
            job,
            root,
            root.is_dir().then_some(root.as_path()),
            &mut out,
            limit,
            &mut observe,
        ) {
            complete = false;
            break;
        }
    }
    let mut data = job.data.lock().unwrap();
    data.known = out.len();
    data.candidates = out
        .iter()
        .map(|(path, _)| path.to_string_lossy().into_owned())
        .collect();
    data.scan_complete = complete;
    data.current = None;
    if !complete {
        job.cancel.store(true, Ordering::Release);
    }
    out
}
fn skip(job: &ImportJob, path: &Path, reason: &str) {
    job.data.lock().unwrap().groups.skipped.push(SkippedEntry {
        path: path.to_string_lossy().into_owned(),
        reason: reason.into(),
    });
}
fn flush_models(
    job: &ImportJob,
    database: &mut impl files::ImportDb,
    batch: &mut Vec<files::PendingImport>,
    imported: &mut Vec<ModelFileDto>,
) -> CmdResult<()> {
    let mut local = files::empty_import_result();
    let mut duplicates = Vec::new();
    let result = files::store_batch_detailed(
        database,
        batch,
        &mut local,
        files::ImportMode::Batched,
        &mut duplicates,
    );
    let mut data = job.data.lock().unwrap();
    for file in &local.imported {
        if job.placement_required {
            data.groups.imported_not_placed.push(NotPlacedEntry {
                path: file.path.clone(),
                file_id: file.id.clone(),
                reason: "jobFailed".into(),
            });
        } else {
            data.groups.imported.push(ImportedEntry {
                path: file.path.clone(),
                file_id: file.id.clone(),
                folder_id: (!file.folder_id.is_empty()).then(|| file.folder_id.clone()),
            });
        }
    }
    for file in &local.skipped {
        data.groups.skipped.push(SkippedEntry {
            path: file.path.clone(),
            reason: "failed".into(),
        });
    }
    data.groups.duplicate.extend(duplicates);
    data.in_flight = 0;
    imported.extend(local.imported);
    result
}
pub(super) fn run_models(
    service: &ImportJobs,
    job: &ImportJob,
    database: &mut impl files::ImportDb,
    roots: &[PathBuf],
    sensitive: &[PathBuf],
) -> CmdResult<()> {
    let candidates = scan_candidates(job, roots, service.scan_limit);
    job.transition(ImportState::Importing);
    let mut batch = Vec::new();
    let mut imported = Vec::new();
    let mut error = None;
    for (path, root) in &candidates {
        if job.cancel.load(Ordering::Acquire) || error.is_some() {
            skip(job, path, "notStarted");
            continue;
        }
        if path.is_file() && crate::archive::is_archive_path(path) && root.is_none() {
            let grant_id = format!(
                "{}-{}",
                job.id,
                job.data.lock().unwrap().groups.archive.len()
            );
            let grant_id = service
                .archives
                .lock()
                .unwrap()
                .entry((job.id.clone(), path.clone()))
                .or_insert_with(|| ArchiveGrant {
                    parent: job.id.clone(),
                    id: grant_id,
                    expires: None,
                })
                .id
                .clone();
            job.data.lock().unwrap().groups.archive.push(serde_json::json!({"path":path.to_string_lossy(),"state":"pending","grantId":grant_id}));
            continue;
        }
        if !files::is_supported_extension(path) {
            skip(job, path, "unsupported");
            continue;
        }
        let path_string = path.to_string_lossy().into_owned();
        match database.with_conn(|conn| {
            db::file_exists_by_path(conn, &path_string).map_err(|e| format!("database: {e}").into())
        }) {
            Ok(true) => {
                job.data
                    .lock()
                    .unwrap()
                    .groups
                    .duplicate
                    .push(DuplicateEntry {
                        path: path_string,
                        kind: "path".into(),
                        existing_file_id: None,
                    });
                continue;
            }
            Err(e) => {
                skip(job, path, "notStarted");
                error = Some(e);
                continue;
            }
            _ => {}
        }
        if std::fs::metadata(path).is_ok_and(|m| m.len() == 0) {
            skip(job, path, "empty");
            continue;
        }
        if job.cancel.load(Ordering::Acquire) {
            skip(job, path, "notStarted");
            continue;
        }
        job.data.lock().unwrap().current = Some(path_string);
        match service.reader.read(path) {
            Ok(new_file) => {
                batch.push(files::PendingImport {
                    path: path.clone(),
                    folder_root: root.clone(),
                    new_file,
                });
                job.data.lock().unwrap().in_flight = batch.len();
            }
            Err(_) => skip(job, path, if path.exists() { "invalid" } else { "failed" }),
        }
        job.data.lock().unwrap().current = None;
        if batch.len() >= 25 {
            if let Err(e) = flush_models(job, database, &mut batch, &mut imported) {
                error = Some(e);
            }
        }
    }
    if error.is_none() {
        if let Err(e) = flush_models(job, database, &mut batch, &mut imported) {
            error = Some(e);
        }
    }
    if error.is_some() {
        let mut data = job.data.lock().unwrap();
        for item in &batch {
            let path = item.path.to_string_lossy().into_owned();
            if !data.groups.imported.iter().any(|f| f.path == path)
                && !data
                    .groups
                    .imported_not_placed
                    .iter()
                    .any(|f| f.path == path)
                && !data.groups.skipped.iter().any(|f| f.path == path)
                && !data.groups.duplicate.iter().any(|f| f.path == path)
            {
                data.groups.skipped.push(SkippedEntry {
                    path,
                    reason: "notStarted".into(),
                });
            }
        }
    }
    if job.placement_required && error.is_none() {
        job.transition(ImportState::Placing);
        for file in &mut imported {
            let reason = if job.cancel.load(Ordering::Acquire) {
                Some("cancelled")
            } else {
                let placed = database.with_conn(|conn| {
                    let target = job
                        .target_folder_id
                        .as_ref()
                        .and_then(|s| s.parse::<i64>().ok())
                        .ok_or_else(|| CmdError::expected("targetMissing"))?;
                    let folders = db::list_folders(conn).map_err(|e| format!("database: {e}"))?;
                    let folder = folders
                        .iter()
                        .find(|f| f.id == target)
                        .ok_or_else(|| CmdError::expected("targetMissing"))?;
                    if !service.target.exists(Path::new(&folder.path)) {
                        return Err(CmdError::expected("targetMissing"));
                    }
                    reject_if_sensitive_path(Path::new(&folder.path), sensitive)
                        .map_err(|_| CmdError::expected("protected"))?;
                    files::move_file_to_folder_with_conn(
                        conn,
                        file.id.parse().unwrap(),
                        Some(target),
                        sensitive,
                    )?;
                    let row = db::get_file(conn, file.id.parse().unwrap())
                        .map_err(|e| format!("database: {e}"))?
                        .ok_or_else(|| CmdError::from("file missing"))?;
                    file.path = row.path;
                    file.folder_id = target.to_string();
                    Ok(())
                });
                match placed {
                    Ok(()) => None,
                    Err(e) if e.message == "targetMissing" => Some("targetMissing"),
                    Err(e) if e.message == "protected" => Some("protected"),
                    Err(e) if e.message.starts_with("database:") => {
                        error = Some(e);
                        break;
                    }
                    Err(_) => Some("moveFailed"),
                }
            };
            let mut data = job.data.lock().unwrap();
            if let Some(reason) = reason {
                if let Some(entry) = data
                    .groups
                    .imported_not_placed
                    .iter_mut()
                    .find(|e| e.file_id == file.id)
                {
                    entry.reason = reason.into();
                }
            } else {
                let original = data
                    .groups
                    .imported_not_placed
                    .iter()
                    .find(|e| e.file_id == file.id)
                    .map(|e| e.path.clone())
                    .unwrap_or_else(|| file.path.clone());
                data.groups
                    .imported_not_placed
                    .retain(|e| e.file_id != file.id);
                data.groups.imported.push(ImportedEntry {
                    path: original,
                    file_id: file.id.clone(),
                    folder_id: Some(file.folder_id.clone()),
                });
            }
        }
    }
    let data = job.data.lock().unwrap();
    *job.legacy.lock().unwrap() = Some(ImportResultDto {
        duplicate_entries: data.groups.duplicate.clone(),
        imported,
        duplicate_count: data.groups.duplicate.len() as i64,
        pending_archives: data
            .groups
            .archive
            .iter()
            .filter_map(|a| a["path"].as_str().map(String::from))
            .collect(),
        skipped: data
            .groups
            .skipped
            .iter()
            .map(|s| files::SkippedFileDto {
                path: s.path.clone(),
                reason: match s.reason.as_str() {
                    "empty" => files::SkipReason::Empty,
                    "invalid" => files::SkipReason::Invalid,
                    _ => files::SkipReason::Failed,
                },
            })
            .collect(),
    });
    if let Some(e) = error {
        Err(e)
    } else {
        Ok(())
    }
}

pub(super) fn heartbeat(job: Arc<ImportJob>) {
    heartbeat_with_wait(job, || std::thread::sleep(Duration::from_millis(250)));
}

fn heartbeat_with_wait(job: Arc<ImportJob>, mut wait: impl FnMut()) {
    while !job.progress().state.terminal() {
        wait();
        if job.progress().state.terminal() {
            break;
        }
        job.event();
    }
}
pub(super) fn run_archives(
    app: &tauri::AppHandle,
    state: &AppState,
    job: &ImportJob,
    target: &Path,
    requests: Vec<ArchiveRequest>,
    delete: bool,
) -> CmdResult<()> {
    use tauri::Emitter;
    {
        let mut data = job.data.lock().unwrap();
        data.known = requests.len();
        data.candidates = requests
            .iter()
            .map(|request| request.path.clone())
            .collect();
        data.scan_complete = true;
    }
    job.transition(ImportState::Importing);
    let mut legacy = ArchiveImportResultDto {
        imported: Vec::new(),
        duplicate_count: 0,
        archives: Vec::new(),
        skipped: Vec::new(),
    };
    let mut database_error = None;
    for request in requests {
        if job.cancel.load(Ordering::Acquire) || database_error.is_some() {
            skip(job, Path::new(&request.path), "notStarted");
            legacy.archives.push(ArchiveOutcomeDto {
                path: request.path.clone(),
                error: Some("notStarted".into()),
                ..Default::default()
            });
            continue;
        }
        let archive_path = request.path.clone();
        job.data.lock().unwrap().current = Some(archive_path.clone());
        let result = archives::extract_archives_controlled(
            &state.sensitive_dirs,
            target,
            vec![request],
            delete,
            |dir, created| {
                let mut database = ArchiveJobDb {
                    db: &state.db,
                    job,
                    reader: state.import_jobs.reader.as_ref(),
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
            job.data.lock().unwrap().groups.archive.push(value);
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
        let mut data = job.data.lock().unwrap();
        data.in_flight = 0;
        data.current = None;
    }
    *job.archive_legacy.lock().unwrap() = Some(legacy);
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
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> CmdResult<R>) -> CmdResult<R> {
        let mut conn = self.db.lock().map_err(|_| "database: lock poisoned")?;
        f(&mut conn)
    }
    fn read_file(&mut self, path: &Path) -> CmdResult<NewFile> {
        self.job.data.lock().unwrap().current = Some(path.to_string_lossy().into_owned());
        let result = self.reader.read(path);
        self.job.data.lock().unwrap().current = None;
        result
    }
    fn report_in_flight(&self, count: usize) {
        self.job.data.lock().unwrap().in_flight = count;
    }
}

#[cfg(test)]
mod scan_tests {
    use super::*;
    use std::sync::mpsc;

    #[derive(Default)]
    struct Clock(AtomicU64);
    impl ImportClock for Clock {
        fn now_ms(&self) -> u64 {
            self.0.load(Ordering::Relaxed)
        }
    }
    #[derive(Default)]
    struct Events(Mutex<Vec<serde_json::Value>>);
    impl ImportEvents for Events {
        fn emit(&self, name: &str, payload: serde_json::Value) {
            if name == "import://progress" {
                self.0.lock().unwrap().push(payload);
            }
        }
    }

    #[test]
    fn scanning_heartbeat_reports_growing_found_with_injected_clock_and_limit() {
        for limit in [2, usize::MAX] {
            let dir =
                std::env::temp_dir().join(format!("scan_found_{}_{}", std::process::id(), limit));
            std::fs::create_dir_all(&dir).unwrap();
            for i in 0..3 {
                std::fs::write(dir.join(format!("{i}.stl")), b"stl").unwrap();
            }
            std::fs::write(dir.join("ignored.txt"), b"text").unwrap();
            let clock = Arc::new(Clock::default());
            let events = Arc::new(Events::default());
            let service = ImportJobs {
                clock: clock.clone(),
                events: events.clone(),
                scan_limit: limit,
                ..ImportJobs::default()
            };
            let job = service
                .enqueue(ImportSource::Folder, None, false, None)
                .unwrap();
            service.next().unwrap();
            let (visited_tx, visited_rx) = mpsc::channel();
            let (resume_tx, resume_rx) = mpsc::channel();
            std::thread::scope(|scope| {
                let scan_job = job.clone();
                let scan_dir = dir.clone();
                let scan_limit = service.scan_limit;
                scope.spawn(move || {
                    let candidates =
                        scan_candidates_observed(&scan_job, &[scan_dir], scan_limit, || {
                            visited_tx.send(()).unwrap();
                            resume_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        });
                    assert_eq!(candidates.len(), limit.min(3));
                    scan_job.finish(None);
                    drop(visited_tx);
                });
                let mut paused = false;
                heartbeat_with_wait(job.clone(), || {
                    if paused {
                        resume_tx.send(()).unwrap();
                    }
                    paused = visited_rx.recv_timeout(Duration::from_secs(5)).is_ok();
                    clock.0.fetch_add(250, Ordering::Relaxed);
                });
            });
            let recorded = events.0.lock().unwrap();
            let scanning: Vec<_> = recorded
                .iter()
                .filter(|p| p["state"] == "scanning" && p["elapsedMs"] != 0)
                .collect();
            let found: Vec<_> = scanning
                .iter()
                .map(|p| p["found"].as_u64().unwrap())
                .collect();
            assert_eq!(found, (0..=limit.min(3) as u64).collect::<Vec<_>>());
            for (i, payload) in scanning.iter().enumerate() {
                assert!(payload["total"].is_null());
                assert_eq!(payload["done"], 0);
                assert_eq!(payload["current"], dir.to_string_lossy().as_ref());
                assert_eq!(payload["elapsedMs"], (i as u64 + 1) * 250);
            }
            assert_eq!(job.progress().found, limit.min(3));
            assert_eq!(job.progress().scan_complete, limit == usize::MAX);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
