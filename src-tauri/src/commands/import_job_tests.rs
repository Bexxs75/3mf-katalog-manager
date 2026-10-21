//! Contract-level tests of the import worker with real SQLite and bounded fakes.
use super::*;
use std::sync::mpsc;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "import_job_contract_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn stl(&self, seed: u32) -> PathBuf {
        let path = self.0.join(format!("model_{seed}.stl"));
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

fn finish_models(service: &ImportJobs, job: &ImportJob, conn: &mut Connection, paths: &[PathBuf]) {
    let error = run_models(service, job, &mut &mut *conn, paths, &[])
        .err()
        .map(|error| ImportJobError {
            kind: "database".into(),
            message: error.message,
        });
    job.finish(error);
}
fn assert_partition(result: &ImportJobResult, known: usize) {
    let counts = &result.counts;
    assert_eq!(counts.known, known);
    assert_eq!(
        counts.imported
            + counts.imported_not_placed
            + counts.duplicate
            + counts.skipped
            + counts.archive,
        known
    );
    assert_eq!(
        result.groups.imported.len()
            + result.groups.imported_not_placed.len()
            + result.groups.duplicate.len()
            + result.groups.skipped.len()
            + result.groups.archive.len(),
        known
    );
}

#[test]
fn contract_01_scan_limit_keeps_terminal_partition_of_known_candidates() {
    let fixture = Fixture::new();
    let paths: Vec<_> = (0..4).map(|i| fixture.stl(i)).collect();
    let service = ImportJobs {
        scan_limit: 2,
        ..ImportJobs::default()
    };
    let job = service
        .enqueue(ImportSource::Files, None, false, None)
        .unwrap();
    service.next().unwrap();
    let mut conn = db::connect_in_memory().unwrap();
    finish_models(&service, &job, &mut conn, &paths);
    let result = job.result().unwrap();
    assert_eq!(result.state, ImportState::Cancelled);
    assert!(!result.scan_complete);
    assert_partition(&result, 2);
    assert_eq!(result.groups.skipped.len(), 2);
    assert!(result
        .groups
        .skipped
        .iter()
        .all(|s| s.reason == "notStarted"));
    assert!(db::list_files(&conn).unwrap().is_empty());
}

struct BlockingReader {
    calls: AtomicU64,
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl ImportReader for BlockingReader {
    fn read(&self, path: &Path) -> CmdResult<NewFile> {
        if self.calls.fetch_add(1, Ordering::Relaxed) == 1 {
            self.entered.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .expect("test releases blocked reader");
        }
        FileReader.read(path)
    }
}

#[test]
fn contract_05_cancel_is_immediate_during_read_and_commits_read_batch() {
    let fixture = Fixture::new();
    let paths: Vec<_> = (0..3).map(|i| fixture.stl(i)).collect();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let reader = Arc::new(BlockingReader {
        calls: AtomicU64::new(0),
        entered: entered_tx,
        release: Mutex::new(release_rx),
    });
    let service = ImportJobs {
        reader: reader.clone(),
        ..ImportJobs::default()
    };
    let job = service
        .enqueue(ImportSource::Files, None, false, None)
        .unwrap();
    service.next().unwrap();
    let conn = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let mut conn = db::connect_in_memory().unwrap();
            finish_models(&service, &job, &mut conn, &paths);
            conn
        });
        entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("worker reaches second read");
        let before = job.progress();
        let start = Instant::now();
        let response = service.cancel(&job.id);
        let elapsed = start.elapsed();
        release_tx.send(()).unwrap();
        let conn = worker.join().unwrap();
        assert_eq!(
            before.in_flight, 1,
            "first parsed file is pending its commit"
        );
        assert_eq!(before.done, 0, "uncommitted work must not be published");
        assert_eq!(response, Some(ImportState::Cancelling));
        assert!(
            elapsed <= Duration::from_millis(200),
            "control path took {elapsed:?}"
        );
        conn
    });
    let result = job.result().unwrap();
    assert_eq!(result.state, ImportState::Cancelled);
    assert_eq!(
        reader.calls.load(Ordering::Relaxed),
        2,
        "third file must never be read"
    );
    assert_eq!(result.groups.imported.len(), 2);
    assert_eq!(db::list_files(&conn).unwrap().len(), 2);
    assert_eq!(result.groups.skipped.len(), 1);
    assert_eq!(result.groups.skipped[0].path, paths[2].to_string_lossy());
    assert_eq!(result.groups.skipped[0].reason, "notStarted");
    assert_partition(&result, 3);
}

struct MissingTarget;
impl TargetChecker for MissingTarget {
    fn exists(&self, _: &Path) -> bool {
        false
    }
}
#[test]
fn contract_06_missing_target_keeps_import_at_original_path() {
    let fixture = Fixture::new();
    let path = fixture.stl(1);
    let target = fixture.0.join("target");
    std::fs::create_dir(&target).unwrap();
    let mut conn = db::connect_in_memory().unwrap();
    let target_id =
        db::insert_folder_with_parent(&conn, "target", None, &target.to_string_lossy()).unwrap();
    let service = ImportJobs {
        target: Arc::new(MissingTarget),
        ..ImportJobs::default()
    };
    let job = service
        .enqueue(ImportSource::Files, Some(target_id.to_string()), true, None)
        .unwrap();
    service.next().unwrap();
    finish_models(&service, &job, &mut conn, std::slice::from_ref(&path));
    let result = job.result().unwrap();
    assert_eq!(result.state, ImportState::Finished);
    assert_eq!(result.groups.imported_not_placed.len(), 1);
    assert_eq!(result.groups.imported_not_placed[0].reason, "targetMissing");
    assert_eq!(
        result.groups.imported_not_placed[0].path,
        path.to_string_lossy()
    );
    assert!(path.exists());
    assert!(!target.join(path.file_name().unwrap()).exists());
    let rows = db::list_files(&conn).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, path.to_string_lossy());
    assert_eq!(rows[0].folder_id, None);
    assert_partition(&result, 1);
}

#[test]
fn contract_13_files_without_catalog_root_import_without_placement() {
    let fixture = Fixture::new();
    let path = fixture.stl(1);
    let service = ImportJobs::default();
    let job = service
        .enqueue(ImportSource::Files, None, false, None)
        .unwrap();
    service.next().unwrap();
    let mut conn = db::connect_in_memory().unwrap();
    finish_models(&service, &job, &mut conn, std::slice::from_ref(&path));
    let result = job.result().unwrap();
    assert_eq!(result.state, ImportState::Finished);
    assert!(!result.placement_required);
    assert_eq!(result.groups.imported.len(), 1);
    assert_eq!(result.groups.imported[0].folder_id, None);
    assert_eq!(result.groups.imported[0].path, path.to_string_lossy());
    assert!(result.groups.imported_not_placed.is_empty());
    assert_eq!(db::list_files(&conn).unwrap()[0].folder_id, None);
    assert_partition(&result, 1);
}

struct CancelAfterRead(Arc<ImportJob>);
impl ImportReader for CancelAfterRead {
    fn read(&self, path: &Path) -> CmdResult<NewFile> {
        let file = FileReader.read(path)?;
        self.0.cancel.store(true, Ordering::Release);
        Ok(file)
    }
}
#[test]
fn contract_05_database_failure_on_cancellation_commit_takes_priority() {
    let fixture = Fixture::new();
    let paths = vec![fixture.stl(0), fixture.stl(1)];
    let mut service = ImportJobs::default();
    let job = service
        .enqueue(ImportSource::Files, None, false, None)
        .unwrap();
    service.reader = Arc::new(CancelAfterRead(job.clone()));
    service.next().unwrap();
    let mut conn = db::connect_in_memory().unwrap();
    conn.execute_batch("PRAGMA query_only = ON;").unwrap();
    finish_models(&service, &job, &mut conn, &paths);
    let result = job.result().unwrap();
    assert_eq!(result.state, ImportState::Failed);
    assert_eq!(result.job_error.as_ref().unwrap().kind, "database");
    assert!(db::list_files(&conn).unwrap().is_empty());
    assert_partition(&result, 2);
    assert_eq!(result.groups.skipped.len(), 2);
}

#[derive(Default)]
struct TestClock(AtomicU64);
impl ImportClock for TestClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}
#[derive(Default)]
struct RecordedEvents(Mutex<Vec<(Instant, String, serde_json::Value)>>);
impl ImportEvents for RecordedEvents {
    fn emit(&self, name: &str, payload: serde_json::Value) {
        self.0
            .lock()
            .unwrap()
            .push((Instant::now(), name.to_string(), payload));
    }
}

#[test]
fn contract_08_heartbeat_continues_during_five_second_blocked_read() {
    let fixture = Fixture::new();
    let paths = vec![fixture.stl(0), fixture.stl(1)];
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let events = Arc::new(RecordedEvents::default());
    let clock = Arc::new(TestClock::default());
    let service = ImportJobs {
        reader: Arc::new(BlockingReader {
            calls: AtomicU64::new(0),
            entered: entered_tx,
            release: Mutex::new(release_rx),
        }),
        events: events.clone(),
        clock: clock.clone(),
        ..ImportJobs::default()
    };
    let job = service
        .enqueue(ImportSource::Files, None, false, None)
        .unwrap();
    service.next().unwrap();
    let (blocked_at, released_at) = std::thread::scope(|scope| {
        let heart = scope.spawn(|| heartbeat(job.clone()));
        let worker = scope.spawn(|| {
            let mut conn = db::connect_in_memory().unwrap();
            finish_models(&service, &job, &mut conn, &paths);
        });
        entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("second read blocked");
        clock.0.store(5000, Ordering::Relaxed);
        let blocked_at = Instant::now();
        std::thread::sleep(Duration::from_secs(5));
        let released_at = Instant::now();
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        heart.join().unwrap();
        (blocked_at, released_at)
    });
    let events = events.0.lock().unwrap();
    let progress: Vec<_> = events
        .iter()
        .filter(|(time, name, _)| {
            name == "import://progress" && *time >= blocked_at && *time <= released_at
        })
        .collect();
    assert!(
        progress.len() >= 18,
        "heartbeat stopped while read was blocked: {} events",
        progress.len()
    );
    assert!(
        progress.len() <= 20,
        "heartbeat exceeds four events per second: {} events",
        progress.len()
    );
    let mut previous = blocked_at;
    for (at, _, payload) in progress {
        assert!(*at - previous <= Duration::from_millis(500));
        previous = *at;
        assert_eq!(payload["state"], "importing");
        assert_eq!(payload["inFlight"], 1);
        assert_eq!(payload["done"], 0);
        assert_eq!(payload["counts"]["imported"], 0);
        assert_eq!(payload["elapsedMs"], 5000);
        assert_eq!(payload["current"], paths[1].to_string_lossy().as_ref());
        assert!(payload.get("percent").is_none());
        assert!(payload.get("percentage").is_none());
    }
    assert!(released_at - previous <= Duration::from_millis(500));
    assert_eq!(
        events
            .iter()
            .filter(|(_, name, _)| name == "import://finished")
            .count(),
        1
    );
}

#[test]
fn contract_08_thousand_small_files_do_not_emit_per_batch_progress() {
    let fixture = Fixture::new();
    let paths: Vec<_> = (0..1000).map(|i| fixture.stl(i)).collect();
    let events = Arc::new(RecordedEvents::default());
    let service = ImportJobs {
        events: events.clone(),
        ..ImportJobs::default()
    };
    let job = service
        .enqueue(ImportSource::Files, None, false, None)
        .unwrap();
    service.next().unwrap();
    let start = Instant::now();
    std::thread::scope(|scope| {
        let heart = scope.spawn(|| heartbeat(job.clone()));
        let mut conn = db::connect_in_memory().unwrap();
        finish_models(&service, &job, &mut conn, &paths);
        assert_eq!(db::list_files(&conn).unwrap().len(), 1000);
        heart.join().unwrap();
    });
    let elapsed = start.elapsed();
    let events = events.0.lock().unwrap();
    let progress_count = events
        .iter()
        .filter(|(_, name, _)| name == "import://progress")
        .count();
    // queued/scanning/importing/finished each emit once; only the independent
    // timer may add progress, never the forty individual batch commits.
    assert!(
        progress_count <= 4 + (elapsed.as_secs_f64() * 4.0).ceil() as usize,
        "{progress_count} progress events during {elapsed:?}"
    );
    assert_partition(&job.result().unwrap(), 1000);
}

#[test]
fn contract_11_grants_reject_forged_expired_and_reused_paths() {
    let grants = PathGrants::default();
    let real = PathBuf::from("/observed/model.stl");
    let forged = PathBuf::from("/forged/model.stl");
    grants.register(std::slice::from_ref(&real), 1000, 60_000);
    assert!(
        !grants.claim(&[real.clone(), forged], 1001),
        "partially forged requests must fail atomically"
    );
    assert!(grants.claim(std::slice::from_ref(&real), 1001));
    assert!(!grants.claim(std::slice::from_ref(&real), 1002));
    grants.register(std::slice::from_ref(&real), 1000, 60_000);
    assert!(!grants.claim(std::slice::from_ref(&real), 61_000));
    grants.register(std::slice::from_ref(&real), 1000, 600_000);
    assert!(grants.claim(std::slice::from_ref(&real), 600_999));
    grants.register(std::slice::from_ref(&real), 1000, 600_000);
    assert!(!grants.claim(std::slice::from_ref(&real), 601_000));
}

#[test]
fn contract_12_queue_admission_waits_for_running_exclusive_command() {
    let service = Arc::new(ImportJobs::default());
    let exclusive = service.gate.exclusive().unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (admitted_tx, admitted_rx) = mpsc::channel();
    let worker_service = service.clone();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let job = worker_service
            .enqueue(ImportSource::Files, None, false, None)
            .unwrap();
        admitted_tx.send(job).unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let premature = admitted_rx.recv_timeout(Duration::from_millis(100));
    drop(exclusive);
    assert!(
        matches!(premature, Err(mpsc::RecvTimeoutError::Timeout)),
        "queue admitted import during an exclusive command"
    );
    let job = admitted_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();
    assert_eq!(
        service.gate.exclusive().unwrap_err().message,
        "importActive"
    );
    service.cancel(&job.id);
    assert!(service.gate.exclusive().is_ok());
}

#[test]
fn contract_12_write_gate_remains_closed_through_cancelling() {
    let service = ImportJobs::default();
    let job = service
        .enqueue(ImportSource::Files, None, false, None)
        .unwrap();
    service.next().unwrap();
    job.transition(ImportState::Importing);
    let error = service.gate.exclusive().unwrap_err();
    assert_eq!(error.message, "importActive");
    assert!(error.expected);
    assert_eq!(service.cancel(&job.id), Some(ImportState::Cancelling));
    let error = service.gate.exclusive().unwrap_err();
    assert_eq!(error.message, "importActive");
    assert!(error.expected);
    job.finish(None);
    assert!(service.gate.exclusive().is_ok());
}

#[test]
fn contract_05_control_response_does_not_wait_for_worker_snapshot_lock() {
    let service = ImportJobs::default();
    let job = service
        .enqueue(ImportSource::Dropped, None, false, None)
        .unwrap();
    service.next().unwrap();
    job.transition(ImportState::Importing);
    let snapshot = job.data.lock().unwrap();
    let started = Instant::now();
    assert_eq!(service.cancel(&job.id), Some(ImportState::Cancelling));
    assert!(started.elapsed() < Duration::from_millis(200));
    assert!(job.cancel.load(Ordering::Acquire));
    drop(snapshot);
    service.finish(&job, None);
}

#[test]
fn contract_11_rejected_job_never_enters_the_worker_queue() {
    let service = ImportJobs::default();
    let job = service
        .enqueue_checked(
            ImportSource::Dropped,
            None,
            false,
            None,
            JobInput::Models(vec![PathBuf::from("/unauthorized.stl")]),
            Some(ImportJobError {
                kind: "unauthorized".into(),
                message: "unauthorized".into(),
            }),
        )
        .unwrap();
    assert!(service.next().is_none());
    let result = job.result().unwrap();
    assert_eq!(result.state, ImportState::Failed);
    assert_eq!(result.job_error.unwrap().kind, "unauthorized");
    assert_eq!(result.counts.known, 0);
    assert!(service.archives.lock().unwrap().is_empty());
    assert!(service.gate.exclusive().is_ok());
}

#[test]
fn folder_contents_moves_nested_files_flat_and_keeps_source_directory() {
    let fixture = Fixture::new();
    let target = fixture.0.join("catalog");
    let source = target.join("Eingang");
    let nested = source.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    let first = source.join("first.stl");
    let second = nested.join("second.stl");
    std::fs::rename(fixture.stl(1), &first).unwrap();
    std::fs::rename(fixture.stl(2), &second).unwrap();
    let mut conn = db::connect_in_memory().unwrap();
    let target_id = db::insert_folder_with_parent(&conn, "catalog", None, &target.to_string_lossy()).unwrap();
    let service = ImportJobs::default();
    let job = service.enqueue(ImportSource::FolderContents, Some(target_id.to_string()), true, None).unwrap();
    service.next().unwrap();
    finish_models(&service, &job, &mut conn, std::slice::from_ref(&source));
    let result = job.result().unwrap();
    assert_eq!(result.counts.imported, 2);
    assert_eq!(db::list_folders(&conn).unwrap().len(), 1);
    for file in db::list_files(&conn).unwrap() {
        assert_eq!(file.folder_id, Some(target_id));
        assert_eq!(Path::new(&file.path).parent(), Some(target.as_path()));
        assert!(Path::new(&file.path).exists());
    }
    assert!(source.is_dir());
    assert!(nested.is_dir());
    assert!(!first.exists());
    assert!(!second.exists());
}

#[test]
fn folder_contents_without_placement_does_not_mirror_or_duplicate_existing_paths() {
    let fixture = Fixture::new();
    fixture.stl(1);
    let mut conn = db::connect_in_memory().unwrap();
    let service = ImportJobs::default();
    for expected_duplicates in [0, 1] {
        let job = service.enqueue(ImportSource::FolderContents, None, false, None).unwrap();
        service.next().unwrap();
        finish_models(&service, &job, &mut conn, std::slice::from_ref(&fixture.0));
        assert_eq!(job.result().unwrap().counts.duplicate, expected_duplicates);
        assert_eq!(db::list_files(&conn).unwrap().len(), 1);
        assert!(db::list_folders(&conn).unwrap().is_empty());
    }
}

#[test]
fn folder_contents_rejects_sensitive_roots() {
    let fixture = Fixture::new();
    let service = ImportJobs::default();
    assert!(service.authorize_models(ImportSource::FolderContents, std::slice::from_ref(&fixture.0), false, std::slice::from_ref(&fixture.0)).is_err());
}

#[test]
fn folder_contents_accepts_ignored_names_for_selected_roots_only() {
    let fixture = Fixture::new();
    for name in [".Eingang", "x.part"] {
        let source = fixture.0.join(name);
        std::fs::create_dir_all(source.join(".hidden")).unwrap();
        std::fs::create_dir_all(source.join("nested.part")).unwrap();
        std::fs::rename(fixture.stl(1), source.join("visible.stl")).unwrap();
        std::fs::rename(fixture.stl(2), source.join(".hidden/model.stl")).unwrap();
        std::fs::rename(fixture.stl(3), source.join("nested.part/model.stl")).unwrap();
        let mut conn = db::connect_in_memory().unwrap();
        let service = ImportJobs::default();
        let job = service.enqueue(ImportSource::FolderContents, None, false, None).unwrap();
        service.next().unwrap();
        finish_models(&service, &job, &mut conn, std::slice::from_ref(&source));
        assert_eq!(job.result().unwrap().counts.imported, 1, "{name}");
        let files = db::list_files(&conn).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(Path::new(&files[0].path), source.join("visible.stl"));
    }
}

#[test]
fn folder_contents_name_collision_preserves_both_files_and_catalog_paths() {
    let fixture = Fixture::new();
    let source = fixture.0.join("source");
    let target = fixture.0.join("catalog");
    std::fs::create_dir_all(&target).unwrap();
    let mut originals = Vec::new();
    for seed in [1, 2] {
        let folder = source.join(format!("sub{seed}"));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("model.stl");
        std::fs::rename(fixture.stl(seed), &path).unwrap();
        originals.push((path.clone(), std::fs::read(path).unwrap()));
    }
    let mut conn = db::connect_in_memory().unwrap();
    let target_id = db::insert_folder_with_parent(&conn, "catalog", None, &target.to_string_lossy()).unwrap();
    let service = ImportJobs::default();
    let job = service.enqueue(ImportSource::FolderContents, Some(target_id.to_string()), true, None).unwrap();
    service.next().unwrap();
    finish_models(&service, &job, &mut conn, &[source]);
    let result = job.result().unwrap();
    assert_partition(&result, 2);
    assert_eq!(result.counts.imported, 1);
    assert_eq!(result.counts.imported_not_placed, 1);
    let failed = &result.groups.imported_not_placed[0];
    assert_eq!(failed.reason, "moveFailed");
    let files = db::list_files(&conn).unwrap();
    assert_eq!(files.len(), 2);
    let retained = files.iter().find(|file| file.id.to_string() == failed.file_id).unwrap();
    assert_eq!(retained.path, failed.path);
    let destination = target.join("model.stl");
    for (path, bytes) in originals {
        if path == Path::new(&failed.path) {
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        } else {
            assert!(!path.exists());
            assert_eq!(std::fs::read(&destination).unwrap(), bytes);
            assert!(files.iter().any(|file| Path::new(&file.path) == destination));
        }
    }
}

#[test]
fn folder_scans_report_archives_without_importing_them() {
    for source in [ImportSource::FolderContents, ImportSource::Folder, ImportSource::Dropped, ImportSource::SetupAdopt] {
        for with_model in [false, true] {
            let fixture = Fixture::new();
            std::fs::write(fixture.0.join("models.zip"), b"archive").unwrap();
            if with_model { fixture.stl(1); }
            let service = ImportJobs::default();
            let job = service.enqueue(source, None, false, None).unwrap();
            let mut conn = db::connect_in_memory().unwrap();
            finish_models(&service, &job, &mut conn, std::slice::from_ref(&fixture.0));
            let result = job.result().unwrap();
            assert_eq!(result.state, ImportState::Finished);
            assert_eq!(result.counts.imported, usize::from(with_model));
            assert_eq!(serde_json::to_value(&result).unwrap()["archivesSkipped"], 1);
            assert_eq!(serde_json::to_value(job.progress()).unwrap()["archivesSkipped"], 1);
            assert_partition(&result, usize::from(with_model));
            assert!(fixture.0.join("models.zip").is_file());
        }
    }
}

#[test]
fn import_rename_setting_preserves_placement_and_only_changes_new_models() {
    for rename in [false, true] {
        for placement in [false, true] {
            let fixture = Fixture::new();
            let catalog = fixture.0.join("catalog");
            std::fs::create_dir(&catalog).unwrap();
            let paths = vec![fixture.stl(1), fixture.stl(2)];
            let originals: Vec<_> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();
            let mut conn = db::connect_in_memory().unwrap();
            let folder =
                db::insert_folder_with_parent(&conn, "catalog", None, catalog.to_str().unwrap())
                    .unwrap();
            db::printer_link::set_setting(
                &conn,
                folders::SETTING_CATALOG_BASE_DIR,
                catalog.to_str().unwrap(),
            )
            .unwrap();
            db::printer_link::set_setting(
                &conn,
                "rename_files_on_import",
                if rename { "1" } else { "0" },
            )
            .unwrap();
            let existing = catalog.join("existing.stl");
            std::fs::rename(fixture.stl(3), &existing).unwrap();
            files::import_many_with_conn(&mut conn, vec![existing.clone()]).unwrap();
            let service = ImportJobs::default();
            let job = service
                .enqueue(
                    ImportSource::Files,
                    placement.then(|| folder.to_string()),
                    placement,
                    None,
                )
                .unwrap();
            service.next().unwrap();
            let before = chrono::Local::now().format("%Y-%m-%d_%H-%M_").to_string();
            let mut candidates = paths.clone();
            candidates.push(existing.clone());
            finish_models(&service, &job, &mut conn, &candidates);
            let after = chrono::Local::now().format("%Y-%m-%d_%H-%M_").to_string();
            let result = job.result().unwrap();
            assert_partition(&result, 3);
            assert_eq!(result.counts.imported, 2);
            assert_eq!(result.counts.duplicate, 1);
            assert!(existing.exists());
            let legacy = job.legacy.lock().unwrap();
            let imported = &legacy.as_ref().unwrap().imported;
            for (index, dto) in imported.iter().enumerate() {
                if placement {
                    assert!(!paths[index].exists());
                } else {
                    assert_eq!(std::fs::read(&paths[index]).unwrap(), originals[index]);
                }
                assert_eq!(std::fs::read(&dto.path).unwrap(), originals[index]);
                assert_eq!(
                    Path::new(&dto.path).file_name().unwrap().to_string_lossy(),
                    dto.name
                );
                if rename && placement {
                    assert!(dto.name.starts_with(&before) || dto.name.starts_with(&after));
                    assert_eq!(&dto.name[..17], &imported[0].name[..17]);
                } else {
                    assert_eq!(
                        dto.name,
                        paths[index].file_name().unwrap().to_string_lossy()
                    );
                }
                if placement {
                    assert_eq!(dto.folder_id, folder.to_string());
                }
            }
        }
    }
}

#[test]
fn archive_named_directory_is_scanned() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.0.join("x.zip")).unwrap();
    std::fs::rename(fixture.stl(1), fixture.0.join("x.zip/model.stl")).unwrap();
    let service = ImportJobs::default();
    let job = service.enqueue(ImportSource::FolderContents, None, false, None).unwrap();
    let mut conn = db::connect_in_memory().unwrap();
    finish_models(&service, &job, &mut conn, std::slice::from_ref(&fixture.0));
    let result = job.result().unwrap();
    assert_eq!(result.counts.imported, 1);
    assert_eq!(serde_json::to_value(result).unwrap()["archivesSkipped"], 0);
}

#[test]
fn rename_external_folder_and_drop_imports_keep_layout_with_or_without_catalog_root() {
    for source in [ImportSource::Folder, ImportSource::Dropped] {
        for configured in [false, true] {
            let fixture = Fixture::new();
            let external = fixture.0.join("external");
            let nested = external.join("nested");
            let catalog = fixture.0.join("catalog");
            std::fs::create_dir_all(&nested).unwrap();
            std::fs::create_dir(&catalog).unwrap();
            let path = nested.join("model.stl");
            std::fs::rename(fixture.stl(1), &path).unwrap();
            let original = std::fs::read(&path).unwrap();
            let mut conn = db::connect_in_memory().unwrap();
            db::printer_link::set_setting(&conn, "rename_files_on_import", "1").unwrap();
            if configured {
                db::printer_link::set_setting(
                    &conn,
                    folders::SETTING_CATALOG_BASE_DIR,
                    catalog.to_str().unwrap(),
                )
                .unwrap();
            }
            let service = ImportJobs::default();
            let job = service.enqueue(source, None, false, None).unwrap();
            service.next().unwrap();
            finish_models(&service, &job, &mut conn, &[external]);
            let result = job.result().unwrap();
            assert_partition(&result, 1);
            assert!(result.job_error.is_none());
            assert_eq!(result.counts.imported, 1);
            let rows = db::list_files(&conn).unwrap();
            assert_eq!(rows[0].path, path.to_str().unwrap());
            assert_eq!(rows[0].name, "model.stl");
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert_eq!(std::fs::read_dir(&catalog).unwrap().count(), 0);
            assert_eq!(std::fs::read_dir(&nested).unwrap().count(), 1);
            // Every imported folder remains an ancestor of its model's folder.
            let folders = db::list_folders(&conn).unwrap();
            assert!(!folders.is_empty());
            assert!(folders
                .iter()
                .all(|folder| Path::new(&rows[0].path).starts_with(&folder.path)));
        }
    }
}

#[test]
fn rename_internal_in_place_preserves_folder_and_reports_protected_root() {
    for protected in [false, true] {
        let fixture = Fixture::new();
        let path = fixture.stl(1);
        let mut conn = db::connect_in_memory().unwrap();
        db::printer_link::set_setting(&conn, "rename_files_on_import", "1").unwrap();
        db::printer_link::set_setting(
            &conn,
            folders::SETTING_CATALOG_BASE_DIR,
            fixture.0.to_str().unwrap(),
        )
        .unwrap();
        let service = ImportJobs::default();
        let job = service
            .enqueue(ImportSource::Files, None, false, None)
            .unwrap();
        service.next().unwrap();
        let sensitive = if protected {
            vec![fixture.0.clone()]
        } else {
            vec![]
        };
        run_models(&service, &job, &mut &mut conn, std::slice::from_ref(&path), &sensitive).unwrap();
        job.finish(None);
        let result = job.result().unwrap();
        assert_partition(&result, 1);
        if protected {
            assert_eq!(result.groups.imported_not_placed[0].reason, "protected");
            assert!(path.exists());
        } else {
            assert_eq!(result.counts.imported, 1);
            assert!(!path.exists());
            let rows = db::list_files(&conn).unwrap();
            assert!(rows[0].name.ends_with("_model_1.stl"));
            assert_eq!(Path::new(&rows[0].path).parent(), Some(fixture.0.as_path()));
        }
    }
}

#[cfg(unix)]
#[test]
fn archive_symlink_is_not_counted() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("target.txt"), b"archive").unwrap();
    std::os::unix::fs::symlink(fixture.0.join("target.txt"), fixture.0.join("linked.zip")).unwrap();
    let service = ImportJobs::default();
    let job = service.enqueue(ImportSource::FolderContents, None, false, None).unwrap();
    let mut conn = db::connect_in_memory().unwrap();
    finish_models(&service, &job, &mut conn, std::slice::from_ref(&fixture.0));
    let result = job.result().unwrap();
    assert_eq!(result.counts.known, 0);
    assert_eq!(serde_json::to_value(result).unwrap()["archivesSkipped"], 0);
}

#[test]
fn imports_files_already_in_placement_target_without_moving_them() {
    for source in [ImportSource::Files, ImportSource::FolderContents] {
        let fixture = Fixture::new();
        let path = fixture.stl(1);
        let bytes = std::fs::read(&path).unwrap();
        let mut conn = db::connect_in_memory().unwrap();
        let target = db::insert_folder_with_parent(&conn, "catalog", None, fixture.0.to_str().unwrap()).unwrap();
        let service = ImportJobs::default();
        let input = if matches!(source, ImportSource::Files) { path.clone() } else { fixture.0.clone() };
        let job = service.enqueue(source, Some(target.to_string()), true, None).unwrap();
        service.next().unwrap();
        finish_models(&service, &job, &mut conn, &[input]);
        let result = job.result().unwrap();
        assert_eq!(result.counts.imported, 1);
        assert_eq!(result.counts.imported_not_placed, 0);
        let files = db::list_files(&conn).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].folder_id, Some(target));
        assert_eq!(Path::new(&files[0].path), path);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_import_names_are_never_renamed_lossily() {
    use std::os::unix::ffi::OsStringExt;
    for enabled in [false, true] {
        let fixture = Fixture::new();
        let path = fixture
            .0
            .join(std::ffi::OsString::from_vec(b"model_\xff.stl".to_vec()));
        std::fs::rename(fixture.stl(1), &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mut conn = db::connect_in_memory().unwrap();
        db::printer_link::set_setting(
            &conn,
            "rename_files_on_import",
            if enabled { "1" } else { "0" },
        )
        .unwrap();
        db::printer_link::set_setting(
            &conn,
            folders::SETTING_CATALOG_BASE_DIR,
            fixture.0.to_str().unwrap(),
        )
        .unwrap();
        let service = ImportJobs::default();
        let job = service
            .enqueue(ImportSource::Folder, None, false, None)
            .unwrap();
        service.next().unwrap();
        finish_models(&service, &job, &mut conn, std::slice::from_ref(&fixture.0));
        let result = job.result().unwrap();
        assert_partition(&result, 1);
        assert!(result.job_error.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 1);
        assert!(!Path::new(&path.to_string_lossy().into_owned()).exists());
    }
}

#[test]
fn rename_database_failure_rolls_back_and_fails_job() {
    let fixture = Fixture::new();
    let path = fixture.stl(1);
    let mut conn = db::connect_in_memory().unwrap();
    db::printer_link::set_setting(&conn, "rename_files_on_import", "1").unwrap();
    db::printer_link::set_setting(
        &conn,
        folders::SETTING_CATALOG_BASE_DIR,
        fixture.0.to_str().unwrap(),
    )
    .unwrap();
    conn.execute_batch("CREATE TRIGGER reject_import_name BEFORE UPDATE OF name ON files BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    let service = ImportJobs::default();
    let job = service
        .enqueue(ImportSource::Files, None, false, None)
        .unwrap();
    service.next().unwrap();
    finish_models(&service, &job, &mut conn, std::slice::from_ref(&path));
    let result = job.result().unwrap();
    assert_partition(&result, 1);
    assert!(result
        .job_error
        .as_ref()
        .unwrap()
        .message
        .starts_with("database:"));
    assert_eq!(result.groups.imported_not_placed[0].reason, "jobFailed");
    assert!(path.exists());
    assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 1);
    assert_eq!(
        db::list_files(&conn).unwrap()[0].path,
        path.to_str().unwrap()
    );
}

#[test]
fn source_folder_files_are_moved_into_the_catalog_folder_like_a_normal_import() {
    let fixture = Fixture::new();
    let catalog = fixture.0.join("catalog");
    std::fs::create_dir(&catalog).unwrap();
    let downloads = fixture.0.join("downloads");
    std::fs::create_dir_all(downloads.join("sub")).unwrap();
    let a = fixture.stl(1);
    let b = fixture.stl(2);
    let left = fixture.stl(3);
    for (file, target) in [(&a, "a.stl"), (&b, "sub/b.stl"), (&left, "left.stl")] {
        std::fs::rename(file, downloads.join(target)).unwrap();
    }
    let (a, b, left) = (downloads.join("a.stl"), downloads.join("sub/b.stl"), downloads.join("left.stl"));

    let conn = db::connect_in_memory().unwrap();
    let folder = db::insert_folder_with_parent(&conn, "catalog", None, catalog.to_str().unwrap()).unwrap();
    db::printer_link::set_setting(&conn, folders::SETTING_CATALOG_BASE_DIR, catalog.to_str().unwrap()).unwrap();
    let state = AppState {
        import_jobs: ImportJobs::default(),
        db: Mutex::new(conn),
        trash_dir: fixture.0.join("trash"),
        db_path: fixture.0.join("catalog.db"),
        sensitive_dirs: Vec::new(),
    };
    source_folders::add_source_folder_with_conn(&state.db.lock().unwrap(), &downloads, &[]).unwrap();
    let chosen = vec![a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned()];
    let paths = source_folders::accepted_import_paths(&state, &chosen, 5000).unwrap();
    let originals: Vec<_> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();

    // What `import_source_files` hands to the queue: the same job a normal import with a
    // catalog folder runs, placing the files into that folder.
    let service = ImportJobs::default();
    let job = service.enqueue(ImportSource::Files, Some(folder.to_string()), true, None).unwrap();
    service.next().unwrap();
    finish_models(&service, &job, &mut state.db.lock().unwrap(), &paths);

    let result = job.result().unwrap();
    assert_eq!(result.counts.imported, 2);
    for (path, original) in paths.iter().zip(&originals) {
        assert!(!path.exists(), "moved out of the source folder: {}", path.display());
        assert_eq!(&std::fs::read(catalog.join(path.file_name().unwrap())).unwrap(), original);
    }
    assert!(left.is_file(), "files that were not chosen stay");
    let stored: Vec<String> = db::list_files(&state.db.lock().unwrap()).unwrap().into_iter().map(|file| file.path).collect();
    assert!(stored.iter().all(|p| Path::new(p).starts_with(&catalog)), "{stored:?}");
    let again = source_folders::scan_with_state(&state, 5000).unwrap();
    let names: Vec<_> = again[0].files.iter().map(|f| f.relative_path.clone()).collect();
    assert_eq!(names, vec!["left.stl"], "what was taken in is no longer new");
}
