use super::*;
use std::io::Write;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "import_handoff_contract_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn stl(&self, i: u32) -> PathBuf {
        let path = self.0.join(format!("part_{i}.stl"));
        let mut bytes = vec![0; 80];
        bytes.extend_from_slice(&1u32.to_le_bytes());
        let s = i as f32;
        for n in [
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
            bytes.extend_from_slice(&n.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        path
    }
    fn zip(&self) -> PathBuf {
        let path = self.0.join("model.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file("model.stl", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&std::fs::read(self.stl(100)).unwrap())
            .unwrap();
        zip.finish().unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[derive(Default)]
struct FakeClock(AtomicU64);
impl ImportClock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}
fn request(path: &Path) -> ArchiveRequest {
    let info = archives::inspect_one(path);
    ArchiveRequest {
        path: info.path,
        folder_name: info.suggested_folder_name,
        on_conflict: archives::ConflictMode::New,
        expected_size: info.file_size,
        expected_modified_unix_ms: info.modified_unix_ms,
    }
}

struct DatabaseLostAfterBatchOne {
    conn: Connection,
    injected: bool,
}
impl files::ImportDb for DatabaseLostAfterBatchOne {
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> CmdResult<R>) -> CmdResult<R> {
        let result = f(&mut self.conn);
        if !self.injected
            && self.conn.is_autocommit()
            && db::list_files(&self.conn).unwrap().len() == 25
        {
            self.conn.execute_batch("PRAGMA query_only = ON;").unwrap();
            self.injected = true;
        }
        result
    }
}
#[test]
fn contract_04_database_failure_batch_two_preserves_committed_prefix() {
    let fixture = Fixture::new();
    let roots: Vec<_> = (0..55).map(|i| fixture.stl(i)).collect();
    let service = ImportJobs::default();
    let job = service
        .enqueue_work(
            ImportSource::Files,
            None,
            false,
            None,
            JobInput::Models(roots.clone()),
        )
        .unwrap();
    service.next().unwrap();
    let mut database = DatabaseLostAfterBatchOne {
        conn: db::connect_in_memory().unwrap(),
        injected: false,
    };
    let error = run_models(&service, &job, &mut database, &roots, &[]).unwrap_err();
    assert!(error.message.starts_with("database:"));
    service.finish(
        &job,
        Some(ImportJobError {
            kind: "database".into(),
            message: error.message,
        }),
    );
    let result = job.result().unwrap();
    assert_eq!(result.state, ImportState::Failed);
    assert_eq!(result.job_error.unwrap().kind, "database");
    assert_eq!(db::list_files(&database.conn).unwrap().len(), 25);
    assert_eq!(result.groups.imported.len(), 25);
    assert_eq!(result.groups.skipped.len(), 30);
    assert!(result
        .groups
        .skipped
        .iter()
        .all(|file| file.reason == "notStarted"));
    assert_eq!(result.counts.known, 55);
    assert_eq!(result.counts.imported + result.counts.skipped, 55);
}

#[test]
fn contract_16_mixed_drop_grant_survives_forty_minute_queue_and_is_parent_bound() {
    let fixture = Fixture::new();
    let archive = fixture.zip();
    let roots: Vec<_> = (0..3)
        .map(|i| fixture.stl(i))
        .chain(std::iter::once(archive.clone()))
        .collect();
    let clock = Arc::new(FakeClock::default());
    let service = ImportJobs {
        clock: clock.clone(),
        ..ImportJobs::default()
    };
    let blocker = service
        .enqueue(ImportSource::Files, None, false, None)
        .unwrap();
    service.next().unwrap();
    let job = service
        .enqueue_work(
            ImportSource::Dropped,
            None,
            false,
            None,
            JobInput::Models(roots.clone()),
        )
        .unwrap();
    assert!(service
        .archives
        .lock()
        .unwrap()
        .get(&(job.id.clone(), archive.clone()))
        .unwrap()
        .expires
        .is_none());
    assert!(
        service
            .claim_archives(&[request(&archive)], Some(&job.id))
            .is_none(),
        "queued input is not yet handed off"
    );
    clock.0.store(40 * 60_000, Ordering::Relaxed);
    service.finish(&blocker, None);
    assert_eq!(service.next().unwrap().id, job.id);
    let mut conn = db::connect_in_memory().unwrap();
    run_models(&service, &job, &mut &mut conn, &roots, &[]).unwrap();
    service.finish(&job, None);
    let result = job.result().unwrap();
    assert_eq!(result.state, ImportState::Finished);
    assert_eq!(result.groups.imported.len(), 3);
    assert_eq!(result.groups.archive.len(), 1);
    assert_eq!(result.groups.archive[0]["state"], "pending");
    assert_eq!(result.counts.known, 4);
    assert_eq!(result.counts.imported + result.counts.archive, 4);
    assert_eq!(
        job.legacy
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .pending_archives,
        vec![archive.to_string_lossy().into_owned()]
    );
    assert!(service
        .claim_archives(&[request(&archive)], Some("different-parent"))
        .is_none());
    assert_eq!(
        service.claim_archives(&[request(&archive)], Some(&job.id)),
        Some(Some(job.id.clone()))
    );
    assert!(
        service
            .claim_archives(&[request(&archive)], Some(&job.id))
            .is_none(),
        "archive grant is single use"
    );
    let child = service
        .enqueue_work(
            ImportSource::Archive,
            None,
            false,
            Some(job.id.clone()),
            JobInput::Archives {
                target: fixture.0.clone(),
                requests: vec![request(&archive)],
                delete: false,
            },
        )
        .unwrap();
    assert_eq!(child.parent_job_id.as_deref(), Some(job.id.as_str()));
}

#[test]
fn contract_16_archive_grant_expires_thirty_one_minutes_after_pending_publication() {
    let fixture = Fixture::new();
    let archive = fixture.zip();
    let clock = Arc::new(FakeClock::default());
    let service = ImportJobs {
        clock: clock.clone(),
        ..ImportJobs::default()
    };
    let roots = vec![archive.clone()];
    let job = service
        .enqueue_work(
            ImportSource::Dropped,
            None,
            false,
            None,
            JobInput::Models(roots.clone()),
        )
        .unwrap();
    clock.0.store(40 * 60_000, Ordering::Relaxed);
    service.next().unwrap();
    let mut conn = db::connect_in_memory().unwrap();
    run_models(&service, &job, &mut &mut conn, &roots, &[]).unwrap();
    service.finish(&job, None);
    assert_eq!(
        service
            .archives
            .lock()
            .unwrap()
            .get(&(job.id.clone(), archive.clone()))
            .unwrap()
            .expires,
        Some(70 * 60_000)
    );
    clock.0.store(71 * 60_000, Ordering::Relaxed);
    assert!(service
        .claim_archives(&[request(&archive)], Some(&job.id))
        .is_none());
    let retry = service
        .enqueue_work(
            ImportSource::Dropped,
            None,
            false,
            None,
            JobInput::Models(roots.clone()),
        )
        .unwrap();
    service.next().unwrap();
    run_models(&service, &retry, &mut &mut conn, &roots, &[]).unwrap();
    service.finish(&retry, None);
    assert_eq!(
        service.claim_archives(&[request(&archive)], Some(&retry.id)),
        Some(Some(retry.id.clone()))
    );
    assert!(service
        .claim_archives(&[request(&archive)], Some(&job.id))
        .is_none());
}

#[test]
fn queued_archive_cancellation_discards_unpublished_grants() {
    let fixture = Fixture::new();
    let archive = fixture.zip();
    let service = ImportJobs::default();
    let job = service
        .enqueue_work(
            ImportSource::Dropped,
            None,
            false,
            None,
            JobInput::Models(vec![archive]),
        )
        .unwrap();
    assert_eq!(service.archives.lock().unwrap().len(), 1);
    assert_eq!(service.cancel(&job.id), Some(ImportState::Cancelled));
    assert!(service.archives.lock().unwrap().is_empty());
    assert_eq!(job.result().unwrap().counts.known, 0);
}

#[test]
fn terminal_results_expire_after_ten_minutes() {
    let clock = Arc::new(FakeClock::default());
    let service = ImportJobs {
        clock: clock.clone(),
        ..ImportJobs::default()
    };
    let job = service
        .enqueue(ImportSource::Dropped, None, false, None)
        .unwrap();
    service.cancel(&job.id);
    clock.0.store(599_999, Ordering::Relaxed);
    assert!(service.get(&job.id).unwrap().result().is_some());
    clock.0.store(600_000, Ordering::Relaxed);
    assert!(service.get(&job.id).is_none());
}

#[test]
fn contract_16_merged_dialog_is_partitioned_without_losing_parent_or_order() {
    let first = Fixture::new();
    let second = Fixture::new();
    let a = first.zip();
    let b = second.zip();
    let service = ImportJobs::default();
    let parent_a = service.test_handoff(vec![a.clone()]);
    let parent_b = service.test_handoff(vec![b.clone()]);
    assert!(service
        .claim_archives(&[request(&a), request(&b)], None)
        .is_none());
    let partition = service.partition_archives(vec![request(&a), request(&b)]);
    assert_eq!(partition.len(), 2);
    assert_eq!(partition[0].0.as_deref(), Some(parent_a.as_str()));
    assert_eq!(partition[1].0.as_deref(), Some(parent_b.as_str()));
    assert_eq!(partition[0].1[0].path, a.to_string_lossy());
    assert_eq!(partition[1].1[0].path, b.to_string_lossy());
    for (parent, requests) in partition {
        assert_eq!(
            service.claim_archives(&requests, parent.as_deref()),
            Some(parent)
        );
    }
}
