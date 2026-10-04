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
