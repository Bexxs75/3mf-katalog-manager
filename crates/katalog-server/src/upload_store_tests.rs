use super::*;
use std::path::Path;
struct Fixture {
    root: PathBuf,
    store: Arc<UploadStore>,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("upload-test-{}", random_id().unwrap()));
        for d in ["uploads", "db", "auth"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        let store = Self::open(&root, &HashSet::new());
        Self { root, store }
    }
    fn open(root: &Path, catalogued: &HashSet<PathBuf>) -> Arc<UploadStore> {
        UploadStore::open(
            root.join("uploads"),
            root.join("auth/uploads.json"),
            root.join("db"),
            1024 * 1024,
            2,
            &catalogued
                .iter()
                .map(|p| {
                    (
                        p.clone(),
                        katalog_core::import::files::compute_content_hash(p).ok(),
                    )
                })
                .collect(),
        )
        .unwrap()
    }
    fn upload(&self) -> Entry {
        let mut p = self.store.begin(4, "test.3mf").unwrap();
        p.write(b"PK\x03\x04").unwrap();
        p.finish().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
#[test]
fn name_boundary() {
    assert_eq!(sanitize_name("caf%65%CC%81.3mf").unwrap(), "café.3mf");
    assert_eq!(sanitize_name("a%E2%80%AE.3mf").unwrap(), "a.3mf");
    for s in [
        "",
        "..",
        "%FF.3mf",
        "%zz.3mf",
        "a/b.3mf",
        "a%5Cb.3mf",
        "CON.3mf",
        "a?.3mf",
    ] {
        assert_eq!(sanitize_name(s).unwrap_err().status, 400, "{s}");
    }
    assert_eq!(sanitize_name("x.3mf.exe").unwrap_err().status, 415);
    assert_eq!(sanitize_name("x.zip").unwrap(), "x.zip");
    assert_eq!(sanitize_name("X.7Z").unwrap(), "X.7Z");
    for unsupported in ["x.rar", "x.tar", "x.tar.gz", "x.tgz", "x.tar.xz"] {
        let e = sanitize_name(unsupported).unwrap_err();
        assert_eq!((e.status, e.kind), (415, "archive_format_unsupported"), "{unsupported}");
    }
    assert_eq!(sanitize_name("x.gz").unwrap_err().kind, "unsupported_type");
    assert_eq!(
        sanitize_name(&format!("{}.3mf", "é".repeat(126)))
            .unwrap_err()
            .status,
        400
    );
}
#[test]
fn cancellation_length_signature_cleanup() {
    let f = Fixture::new();
    {
        let mut p = f.store.begin(4, "a.3mf").unwrap();
        p.write(b"P").unwrap();
    }
    assert_eq!(
        std::fs::read_dir(f.root.join("uploads")).unwrap().count(),
        0
    );
    let mut p = f.store.begin(4, "a.3mf").unwrap();
    p.write(b"ABCD").unwrap();
    assert_eq!(p.finish().unwrap_err().kind, "invalid_signature");
    let p = f.store.begin(4, "a.3mf").unwrap();
    assert_eq!(p.finish().unwrap_err().kind, "length_mismatch");
    assert!(f.store.list().unwrap().is_empty());
}
#[test]
fn conflicts_never_replace_and_restart_recovers() {
    let f = Fixture::new();
    let a = f.upload();
    let b = f.upload();
    assert_ne!(a.name, b.name);
    f.store
        .begin_import(std::slice::from_ref(&a.id), "job")
        .unwrap();
    assert_eq!(f.store.discard(&a.id).unwrap_err().status, 409);
    let reopened = Fixture::open(&f.root, &HashSet::from([f.store.path(&b)]));
    let all = reopened.list().unwrap();
    assert_eq!(
        all.iter().find(|e| e.id == a.id).unwrap().state,
        UploadState::Uploaded
    );
    assert_eq!(
        all.iter().find(|e| e.id == b.id).unwrap().state,
        UploadState::Imported
    );
    assert_eq!(
        std::fs::metadata(f.root.join("auth/uploads.json"))
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );
}
#[test]
fn malicious_inode_hardlink_and_symlink_are_rejected() {
    let f = Fixture::new();
    let a = f.upload();
    std::fs::hard_link(f.store.path(&a), f.root.join("extra")).unwrap();
    assert!(f.store.checked_file(&a).is_err());
    assert_eq!(
        f.store
            .begin_import(std::slice::from_ref(&a.id), "job")
            .unwrap_err()
            .status,
        409
    );
    assert_eq!(f.store.list().unwrap()[0].state, UploadState::Invalid);
    f.store.discard(&a.id).unwrap();
    assert!(f.root.join("extra").is_file());
    assert!(!f.store.path(&a).exists());
    std::os::unix::fs::symlink(f.root.join("extra"), f.store.path(&a)).unwrap();
    assert!(f.store.checked_file(&a).is_err());
}
#[test]
fn results_survive_restart_and_invalid_can_discard() {
    let f = Fixture::new();
    let e = f.upload();
    f.store
        .begin_import(std::slice::from_ref(&e.id), "job")
        .unwrap();
    f.store
        .complete_batch(vec![(
            e.id.clone(),
            UploadState::Duplicate,
            serde_json::json!({"fileId":17}),
        )])
        .unwrap();
    f.store
        .save_job("job", serde_json::json!({"jobId":"job","state":"finished"}))
        .unwrap();
    let reopened = Fixture::open(&f.root, &HashSet::new());
    assert!(reopened.job_result("job").unwrap().is_some());
    reopened.discard(&e.id).unwrap();
    assert!(!f.store.path(&e).exists());
}

#[test]
fn active_slots_and_pending_quotas_release_on_drop() {
    let f = Fixture::new();
    let a = f.store.begin(4, "same.3mf").unwrap();
    let b = f.store.begin(4, "same.3mf").unwrap();
    assert!(matches!(
        f.store.begin(4, "same.3mf"),
        Err(StoreError { status: 503, .. })
    ));
    drop(a);
    drop(b);
    assert!(f.store.begin(4, "same.3mf").is_ok());
    assert!(matches!(
        f.store.begin(1024 * 1024 + 1, "same.3mf"),
        Err(StoreError { status: 413, .. })
    ));
    let mut e = f.upload();
    e.size = PENDING_BYTES;
    f.store
        .inner
        .lock()
        .unwrap()
        .journal
        .entries
        .insert(e.id.clone(), e);
    assert!(matches!(
        f.store.begin(4, "same.3mf"),
        Err(StoreError { status: 409, .. })
    ));
}
#[test]
fn simultaneously_reserved_same_name_gets_unique_files() {
    let f = Fixture::new();
    let mut a = f.store.begin(4, "same.3mf").unwrap();
    let mut b = f.store.begin(4, "same.3mf").unwrap();
    a.write(b"PK").unwrap();
    a.write(b"\x03\x04").unwrap();
    b.write(b"PK\x03\x04").unwrap();
    let a = std::thread::spawn(move || a.finish().unwrap());
    let b = std::thread::spawn(move || b.finish().unwrap());
    let (a, b) = (a.join().unwrap(), b.join().unwrap());
    assert_ne!(a.name, b.name);
    assert_ne!(a.ino, b.ino);
}
#[test]
fn directory_and_replaced_inode_never_import_and_missing_reconciles() {
    let f = Fixture::new();
    let e = f.upload();
    std::fs::rename(f.store.path(&e), f.root.join("old")).unwrap();
    std::fs::write(f.store.path(&e), b"PK\x03\x04").unwrap();
    assert!(f.store.checked_file(&e).is_err());
    assert!(Fixture::open(&f.root, &HashSet::new())
        .list()
        .unwrap()
        .is_empty());
    std::fs::remove_file(f.store.path(&e)).unwrap();
    std::fs::create_dir(f.store.path(&e)).unwrap();
    assert!(f.store.checked_file(&e).is_err());
    std::os::unix::fs::symlink(f.root.join("uploads"), f.root.join("alias")).unwrap();
    assert!(Directory::open(&f.root.join("alias")).is_err());
}

#[test]
fn missing_files_are_removed_and_only_old_owned_temporaries_cleaned() {
    use std::fs::FileTimes;
    let f = Fixture::new();
    let e = f.upload();
    std::fs::remove_file(f.store.path(&e)).unwrap();
    let old = format!(".upload-{}.part", random_id().unwrap());
    let fresh = format!(".upload-{}.part", random_id().unwrap());
    for name in [&old, &fresh, "unowned.part"] {
        std::fs::write(f.root.join("uploads").join(name), b"data").unwrap();
    }
    let file = File::open(f.root.join("uploads").join(&old)).unwrap();
    file.set_times(FileTimes::new().set_modified(UNIX_EPOCH))
        .unwrap();
    let reopened = Fixture::open(&f.root, &HashSet::new());
    assert!(reopened.list().unwrap().is_empty());
    assert!(!f.root.join("uploads").join(old).exists());
    assert!(f.root.join("uploads").join(fresh).exists());
    assert!(f.root.join("uploads/unowned.part").exists());
}
#[test]
fn per_parser_limit_uses_decoded_extension_and_duplicate_start_is_conflict() {
    let f = Fixture::new();
    assert!(matches!(
        f.store.begin_limited(5, "mesh.%73tl", 4),
        Err(StoreError { status: 413, .. })
    ));
    let e = f.upload();
    f.store
        .begin_import(std::slice::from_ref(&e.id), "job")
        .unwrap();
    assert_eq!(
        f.store
            .begin_import(std::slice::from_ref(&e.id), "job2")
            .unwrap_err()
            .status,
        409
    );
    assert_eq!(
        f.store
            .begin_import(&["missing".to_owned()], "job2")
            .unwrap_err()
            .status,
        404
    );
    f.store.reset_importing("job").unwrap();
    assert_eq!(f.store.list().unwrap()[0].state, UploadState::Uploaded);
}

#[test]
fn interrupted_job_reconstructs_committed_and_pending_files() {
    let f = Fixture::new();
    let a = f.upload();
    let b = f.upload();
    let c = f.upload();
    f.store
        .begin_import(&[a.id.clone(), b.id.clone(), c.id.clone()], "interrupted")
        .unwrap();
    assert_eq!(
        f.store.job_result("interrupted").unwrap().unwrap()["state"],
        "importing"
    );
    f.store
        .complete_batch(vec![(
            a.id.clone(),
            UploadState::Imported,
            serde_json::json!({"fileId":"42"}),
        )])
        .unwrap();
    // b was committed to the DB immediately before the crash, but its journal
    // update never happened; c never reached a commit.
    let reopened = Fixture::open(
        &f.root,
        &HashSet::from([f.store.path(&a), f.store.path(&b)]),
    );
    let job = reopened.job_result("interrupted").unwrap().unwrap();
    assert_eq!(job["state"], "failed");
    assert_eq!(job["kind"], "interrupted");
    assert_eq!(job["done"], 2);
    assert_eq!(job["total"], 3);
    let files = job["files"].as_array().unwrap();
    let committed = files.iter().find(|e| e["uploadId"] == a.id).unwrap();
    assert_eq!(committed["fileId"], "42");
    assert_eq!(
        files.iter().find(|e| e["uploadId"] == b.id).unwrap()["state"],
        "imported"
    );
    assert_eq!(
        files.iter().find(|e| e["uploadId"] == c.id).unwrap()["state"],
        "uploaded"
    );
    assert_eq!(
        reopened
            .list()
            .unwrap()
            .iter()
            .find(|e| e.id == c.id)
            .unwrap()
            .job_id
            .as_deref(),
        Some("interrupted")
    );
    // A subsequent restart must leave the now-terminal result unchanged.
    assert_eq!(
        Fixture::open(&f.root, &HashSet::new())
            .job_result("interrupted")
            .unwrap()
            .unwrap(),
        job
    );
}
#[test]
fn replaced_temporary_and_foreign_cleanup_target_are_never_deleted() {
    let f = Fixture::new();
    let mut pending = f.store.begin(4, "model.3mf").unwrap();
    pending.write(b"PK\x03\x04").unwrap();
    let path = f.root.join("uploads").join(&pending.temporary);
    std::fs::rename(&path, f.root.join("owned-temp")).unwrap();
    std::fs::write(&path, b"foreign").unwrap();
    let foreign = f.root.join("uploads/foreign.3mf");
    std::fs::write(&foreign, b"foreign").unwrap();
    assert_eq!(pending.unlink_owned("foreign.3mf").unwrap_err().status, 409);
    assert_eq!(pending.finish().unwrap_err().status, 409);
    assert_eq!(std::fs::read(&path).unwrap(), b"foreign");
    assert_eq!(std::fs::read(&foreign).unwrap(), b"foreign");
    assert!(!f.root.join("uploads/model.3mf").exists());
}

#[test]
fn reconcile_ids_repairs_entry_and_interrupted_job_but_keeps_history() {
    let f = Fixture::new();
    let e = f.upload();
    f.store
        .begin_import(std::slice::from_ref(&e.id), "recover")
        .unwrap();
    let history = serde_json::json!({"jobId":"historical","state":"completed","files":[{"uploadId":e.id,"state":"duplicate","existingFileId":"previous"}],"kind":null});
    f.store.save_job("historical", history.clone()).unwrap();
    let reopened = Fixture::open(&f.root, &HashSet::from([f.store.path(&e)]));
    reopened
        .reconcile_file_ids(&HashMap::from([(f.store.path(&e), "42".to_owned())]))
        .unwrap();
    let expected = serde_json::json!({"uploadId":e.id,"name":e.name,"state":"imported","fileId":"42","existingFileId":null,"kind":null});
    assert_eq!(reopened.list().unwrap()[0].result, Some(expected.clone()));
    assert_eq!(
        reopened.job_result("recover").unwrap().unwrap()["files"][0],
        expected
    );
    assert_eq!(reopened.job_result("historical").unwrap(), Some(history));
    let again = Fixture::open(&f.root, &HashSet::from([f.store.path(&e)]));
    assert_eq!(again.list().unwrap()[0].result, Some(expected.clone()));
    assert_eq!(
        again.job_result("recover").unwrap().unwrap()["files"][0],
        expected
    );
}

#[test]
fn pending_snapshot_excludes_imported_history() {
    let f = Fixture::new();
    let imported = f.upload();
    let pending = f.upload();
    f.store
        .complete_batch(vec![(
            imported.id,
            UploadState::Imported,
            serde_json::json!({}),
        )])
        .unwrap();
    assert_eq!(
        f.store
            .pending_list()
            .unwrap()
            .iter()
            .map(|e| e.id.clone())
            .collect::<Vec<_>>(),
        vec![pending.id]
    );
    assert_eq!(f.store.list().unwrap().len(), 2);
    assert!(f.store.job_results().unwrap().is_empty());
}

#[test]
fn reserve_rejects_impossible_capacity_and_checks_inside_one_frame() {
    let f = Fixture::new();
    assert_eq!(
        f.store
            .space(&f.store.lock().unwrap(), u64::MAX)
            .unwrap_err()
            .status,
        507
    );
    let mut pending = f.store.begin(8, "frame.stl").unwrap();
    // Move the next boundary close to avoid allocating a 64 MiB test frame.
    pending.next_check = 4;
    f.store.lock().unwrap().reserved = u64::MAX - RESERVE;
    assert_eq!(pending.write(b"abcdefgh").unwrap_err().status, 507);
    assert_eq!(pending.written, 4);
    assert_eq!(pending.file.metadata().unwrap().len(), 4);
}

#[test]
fn recent_jobs_keep_update_order_cap_and_full_lookup() {
    let f = Fixture::new();
    for n in 0..33 {
        f.store
            .save_job(
                &format!("job-{n:02}"),
                serde_json::json!({"jobId":format!("job-{n:02}"),"state":"completed","files":[]}),
            )
            .unwrap();
    }
    let recent = f.store.recent_job_results().unwrap();
    assert_eq!(recent.len(), 32);
    assert_eq!(recent[0]["jobId"], "job-32");
    assert_eq!(recent[31]["jobId"], "job-01");
    assert!(f.store.job_result("job-00").unwrap().is_some());
    f.store.save_job("job-00",serde_json::json!({"jobId":"job-00","state":"completed","files":vec![serde_json::json!({});201]})).unwrap();
    let reopened = Fixture::open(&f.root, &HashSet::new());
    let recent = reopened.recent_job_results().unwrap();
    assert_eq!(recent.len(), 32);
    assert_eq!(recent[0]["jobId"], "job-00");
    assert_eq!(recent[0]["files"].as_array().unwrap().len(), 200);
    assert_eq!(
        reopened.job_result("job-00").unwrap().unwrap()["files"]
            .as_array()
            .unwrap()
            .len(),
        201
    );
    reopened.inner.lock().unwrap().journal.job_order.clear();
    assert_eq!(reopened.recent_job_results().unwrap()[0]["jobId"], "job-32");
}

#[test]
fn poisoned_store_still_releases_reservation_on_drop() {
    let f = Fixture::new();
    let pending = f.store.begin(4, "poison.3mf").unwrap();
    let store = Arc::clone(&f.store);
    assert!(std::thread::spawn(move || {
        let _guard = store.inner.lock().unwrap();
        panic!("poison test");
    })
    .join()
    .is_err());
    drop(pending);
    let inner = f
        .store
        .inner
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(inner.active, 0);
    assert_eq!(inner.reserved, 0);
    drop(inner);
    assert!(f.store.list().is_err());
    assert_eq!(
        std::fs::read_dir(f.root.join("uploads")).unwrap().count(),
        0
    );
}

#[test]
fn round3_written_bytes_release_space_reservation() {
    let f = Fixture::new();
    let mut p = f.store.begin(8, "space.obj").unwrap();
    p.write(b"abcd").unwrap();
    assert_eq!(f.store.lock().unwrap().reserved, 4);
    drop(p);
    assert_eq!(f.store.lock().unwrap().reserved, 0);
}

#[test]
fn round3_terminal_jobs_are_bounded() {
    let f = Fixture::new();
    for n in 0..110 {
        f.store
            .save_job(
                &format!("job-{n}"),
                serde_json::json!({"state":"completed"}),
            )
            .unwrap();
    }
    assert_eq!(f.store.job_results().unwrap().len(), 100);
    assert!(f.store.job_result("job-0").unwrap().is_none());
    assert!(f.store.job_result("job-109").unwrap().is_some());
}

#[test]
fn round3_missing_or_replaced_invalid_upload_can_be_discarded() {
    for replaced in [false, true] {
        let f = Fixture::new();
        let e = f.upload();
        f.store
            .complete_batch(vec![(
                e.id.clone(),
                UploadState::Invalid,
                serde_json::json!({}),
            )])
            .unwrap();
        std::fs::rename(f.store.path(&e), f.root.join("original")).unwrap();
        if replaced {
            std::fs::write(f.store.path(&e), b"foreign").unwrap();
        }
        f.store.discard(&e.id).unwrap();
        assert!(f.store.list().unwrap().is_empty());
        if replaced {
            assert_eq!(std::fs::read(f.store.path(&e)).unwrap(), b"foreign");
        }
    }
}

#[test]
fn round3_old_journal_temporaries_cleaned_only() {
    let f = Fixture::new();
    let old = f
        .root
        .join(format!("auth/.journal-{}.part", random_id().unwrap()));
    let fresh = f
        .root
        .join(format!("auth/.journal-{}.part", random_id().unwrap()));
    for p in [&old, &fresh] {
        std::fs::write(p, b"partial").unwrap();
    }
    File::open(&old)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(UNIX_EPOCH))
        .unwrap();
    let _ = Fixture::open(&f.root, &HashSet::new());
    assert!(!old.exists());
    assert!(fresh.exists());
}

#[test]
fn round3_catalogued_missing_paths_get_suffix_and_hash_mismatch_stays_open() {
    let f = Fixture::new();
    let e = f.upload();
    let occupied = HashMap::from([(f.store.path(&e), Some("different hash".to_owned()))]);
    let reopened = UploadStore::open(
        f.root.join("uploads"),
        f.root.join("auth/uploads.json"),
        f.root.join("db"),
        1024,
        2,
        &occupied,
    )
    .unwrap();
    assert_eq!(reopened.list().unwrap()[0].state, UploadState::Uploaded);
    std::fs::remove_file(f.store.path(&e)).unwrap();
    let mut p = reopened.begin(4, &e.name).unwrap();
    p.write(b"PK\x03\x04").unwrap();
    assert_eq!(p.finish().unwrap().name, "test (2).3mf");
}

#[test]
fn round3_imported_history_and_open_list_are_bounded_without_deleting_files() {
    let f = Fixture::new();
    let e = f.upload();
    let before = f.store.persist_count.load(Ordering::Relaxed);
    for batch in 0..60 {
        let mut updates = Vec::new();
        {
            let mut inner = f.store.lock().unwrap();
            for n in 0..20 {
                let mut entry = e.clone();
                entry.id = format!("entry-{:05}", batch * 20 + n);
                entry.time = batch * 20 + n;
                updates.push((
                    entry.id.clone(),
                    UploadState::Imported,
                    serde_json::json!({}),
                ));
                inner.journal.entries.insert(entry.id.clone(), entry);
            }
        }
        f.store.complete_batch(updates).unwrap();
    }
    assert_eq!(f.store.persist_count.load(Ordering::Relaxed) - before, 60);
    assert_eq!(f.store.list().unwrap().len(), 1001);
    assert_eq!(f.store.pending_list().unwrap().len(), 1);
    assert!(f.store.path(&e).is_file());
    let disk: Journal =
        serde_json::from_slice(&std::fs::read(f.root.join("auth/uploads.json")).unwrap()).unwrap();
    assert_eq!(disk.entries.len(), 1001);
    assert!(!disk.entries.contains_key("entry-00000"));
    assert!(disk.entries.contains_key("entry-01199"));
    let mut inner = f.store.lock().unwrap();
    for n in 0..550 {
        let mut pending = e.clone();
        pending.id = format!("open-{n}");
        inner.journal.entries.insert(pending.id.clone(), pending);
    }
    drop(inner);
    assert_eq!(f.store.pending_list().unwrap().len(), 500);
}

#[test]
fn round3_disk_files_are_not_reserved_twice_and_active_quota_stays_full() {
    let f = Fixture::new();
    let mut e = f.upload();
    e.size = f.store.dir.available().unwrap();
    f.store
        .lock()
        .unwrap()
        .journal
        .entries
        .insert(e.id.clone(), e);
    assert!(f.store.space(&f.store.lock().unwrap(), 0).is_ok());
    let f = Fixture::new();
    let mut p = f.store.begin(8, "part.obj").unwrap();
    p.write(b"four").unwrap();
    assert_eq!(f.store.lock().unwrap().active_bytes, 8);
    drop(p);
    assert_eq!(f.store.lock().unwrap().active_bytes, 0);
}

#[test]
fn round3_hardlinked_temporaries_and_unknown_files_survive_cleanup() {
    let f = Fixture::new();
    let temp = f
        .root
        .join(format!("uploads/.upload-{}.part", random_id().unwrap()));
    std::fs::write(&temp, b"data").unwrap();
    File::open(&temp)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(UNIX_EPOCH))
        .unwrap();
    std::fs::hard_link(&temp, f.root.join("uploads/orphan.obj")).unwrap();
    let directory = f
        .root
        .join(format!("auth/.journal-{}.part", random_id().unwrap()));
    std::fs::create_dir(&directory).unwrap();
    let link = f
        .root
        .join(format!("auth/.journal-{}.part", random_id().unwrap()));
    std::os::unix::fs::symlink(&temp, &link).unwrap();
    let _ = Fixture::open(&f.root, &HashSet::new());
    assert!(temp.exists());
    assert!(f.root.join("uploads/orphan.obj").exists());
    assert!(directory.is_dir());
    assert!(link.is_symlink());
}

#[test]
fn round3_discard_keeps_replacement_symlink_or_directory() {
    for symlink in [false, true] {
        let f = Fixture::new();
        let e = f.upload();
        f.store
            .complete_batch(vec![(
                e.id.clone(),
                UploadState::Duplicate,
                serde_json::json!({}),
            )])
            .unwrap();
        std::fs::rename(f.store.path(&e), f.root.join("owned")).unwrap();
        if symlink {
            std::os::unix::fs::symlink(f.root.join("owned"), f.store.path(&e)).unwrap();
        } else {
            std::fs::create_dir(f.store.path(&e)).unwrap();
        }
        f.store.discard(&e.id).unwrap();
        assert!(f.store.list().unwrap().is_empty());
        assert!(std::fs::symlink_metadata(f.store.path(&e)).is_ok());
        assert_eq!(std::fs::read(f.root.join("owned")).unwrap(), b"PK\x03\x04");
    }
}

#[test]
fn catalog_removal_startup_clears_stale_result_and_drops_missing_file() {
    let f = Fixture::new();
    let kept = f.upload();
    let missing = f.upload();
    f.store
        .complete_batch(vec![
            (
                kept.id.clone(),
                UploadState::Imported,
                serde_json::json!({"fileId":"1"}),
            ),
            (
                missing.id.clone(),
                UploadState::Imported,
                serde_json::json!({"fileId":"2"}),
            ),
        ])
        .unwrap();
    std::fs::remove_file(f.store.path(&missing)).unwrap();
    let reopened = Fixture::open(&f.root, &HashSet::new());
    let entries = reopened.list().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, kept.id);
    assert_eq!(entries[0].state, UploadState::Uploaded);
    assert!(entries[0].result.is_none());
    assert!(entries[0].job_id.is_none());
}

#[test]
fn loading_for_fsops_recovery_does_not_reconcile_or_persist() {
    let f = Fixture::new();
    let entry = f.upload();
    std::fs::rename(
        f.root.join("uploads").join(&entry.name),
        f.root.join("uploads/moved.3mf"),
    )
    .unwrap();
    let before = std::fs::read(f.root.join("auth/uploads.json")).unwrap();
    let store = UploadStore::load(
        f.root.join("uploads"),
        f.root.join("auth/uploads.json"),
        f.root.join("db"),
        1024 * 1024,
        2,
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(store.list().unwrap().len(), 1);
    assert_eq!(
        std::fs::read(f.root.join("auth/uploads.json")).unwrap(),
        before
    );
}

#[test]
fn file_id_reconciliation_detaches_moved_row_without_rebinding_old_path() {
    let f = Fixture::new();
    let e = f.upload();
    f.store
        .complete_batch(vec![(
            e.id.clone(),
            UploadState::Imported,
            serde_json::json!({"fileId":"42"}),
        )])
        .unwrap();
    let ids = HashMap::from([
        (f.root.join("moved.3mf"), "42".into()),
        (f.store.path(&e), "99".into()),
    ]);
    let hash = katalog_core::import::files::compute_content_hash(&f.store.path(&e)).unwrap();
    f.store
        .reconcile_catalog(&ids, &HashMap::from([(f.store.path(&e), Some(hash))]))
        .unwrap();
    assert!(f.store.list().unwrap().is_empty());
    assert!(f.store.path(&e).exists());
}
#[test]
fn disabled_upload_ledger_durably_detaches_and_resets_without_touching_files() {
    let f = Fixture::new();
    let e = f.upload();
    f.store
        .complete_batch(vec![(
            e.id.clone(),
            UploadState::Imported,
            serde_json::json!({"fileId":"42"}),
        )])
        .unwrap();
    std::fs::copy(
        f.root.join("auth/uploads.json"),
        f.root.join("auth/web-uploads.json"),
    )
    .unwrap();
    let mut offline = OfflineLedger::load(&f.root.join("auth")).unwrap();
    offline.update(&[42], true).unwrap();
    assert_eq!(
        OfflineLedger::load(&f.root.join("auth")).unwrap().entries()[0].state,
        UploadState::Uploaded
    );
    std::fs::copy(
        f.root.join("auth/uploads.json"),
        f.root.join("auth/web-uploads.json"),
    )
    .unwrap();
    let mut offline = OfflineLedger::load(&f.root.join("auth")).unwrap();
    offline.update(&[42], false).unwrap();
    assert!(OfflineLedger::load(&f.root.join("auth"))
        .unwrap()
        .entries()
        .is_empty());
    assert!(f.store.path(&e).exists());
}

#[test]
fn known_file_id_survives_null_or_stale_hash_and_cannot_be_discarded() {
    for hash in [None, Some("stale-hash".to_owned())] {
        let f = Fixture::new();
        let e = f.upload();
        f.store
            .complete_batch(vec![(
                e.id.clone(),
                UploadState::Imported,
                serde_json::json!({"fileId":"42"}),
            )])
            .unwrap();
        f.store
            .reconcile_catalog(
                &HashMap::from([(f.store.path(&e), "42".into())]),
                &HashMap::from([(f.store.path(&e), hash)]),
            )
            .unwrap();
        let entries = f.store.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].state, UploadState::Imported);
        assert_eq!(entries[0].result.as_ref().unwrap()["fileId"], "42");
        assert_eq!(f.store.discard(&e.id).unwrap_err().kind, "upload_state");
        assert!(f.store.path(&e).exists());
    }
}
#[test]
fn known_file_id_with_replacement_inode_is_detached_even_matching_hash() {
    let f = Fixture::new();
    let e = f.upload();
    f.store
        .complete_batch(vec![(
            e.id.clone(),
            UploadState::Imported,
            serde_json::json!({"fileId":"42"}),
        )])
        .unwrap();
    let original = std::fs::File::open(f.store.path(&e)).unwrap();
    std::fs::remove_file(f.store.path(&e)).unwrap();
    std::fs::write(f.store.path(&e), b"PK\x03\x04").unwrap();
    let hash = katalog_core::import::files::compute_content_hash(&f.store.path(&e)).unwrap();
    f.store
        .reconcile_catalog(
            &HashMap::from([(f.store.path(&e), "42".into())]),
            &HashMap::from([(f.store.path(&e), Some(hash))]),
        )
        .unwrap();
    assert!(f.store.list().unwrap().is_empty());
    assert!(f.store.path(&e).exists());
    drop(original);
}

#[test]
fn journal_with_loose_mode_hard_link_or_excess_size_is_refused_on_load() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.upload();
    drop(f.store.clone());
    let journal = f.root.join("auth/uploads.json");
    let open = || {
        UploadStore::load(
            f.root.join("uploads"),
            journal.clone(),
            f.root.join("db"),
            1024 * 1024,
            2,
            &HashMap::new(),
        )
        .map(|_| ())
    };
    assert!(open().is_ok(), "intact journal loads");
    assert!(OfflineLedger::load(&f.root.join("auth")).is_ok());
    // Group/world-readable mode.
    std::fs::set_permissions(&journal, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(open().is_err());
    std::fs::set_permissions(&journal, std::fs::Permissions::from_mode(0o600)).unwrap();
    // Second hard link to the journal.
    let link = f.root.join("auth/uploads.link");
    std::fs::hard_link(&journal, &link).unwrap();
    assert!(open().is_err());
    std::fs::remove_file(&link).unwrap();
    assert!(open().is_ok());
    // Above the size limit, even with valid mode.
    File::options().write(true).open(&journal).unwrap().set_len(MAX_JOURNAL_BYTES + 1).unwrap();
    assert!(open().is_err());
}

fn kind_of<T>(result: Result<T, StoreError>) -> &'static str {
    result.err().expect("an error").kind
}
fn staging_name() -> String {
    format!("{STAGING_PREFIX}{}", random_id().unwrap())
}
fn age(path: &Path, seconds: u64) {
    let file = std::fs::File::open(path).unwrap();
    file.set_modified(SystemTime::now() - std::time::Duration::from_secs(seconds)).unwrap();
}

#[test]
fn leftover_staging_folders_are_removed_at_startup_but_nothing_else() {
    let f = Fixture::new();
    let uploads = f.root.join("uploads");
    // A crash in the middle of unpacking: real folder, pattern name, nested content.
    let stale = uploads.join(staging_name());
    std::fs::create_dir_all(stale.join("a/b")).unwrap();
    std::fs::write(stale.join("a/b/x.stl"), b"solid").unwrap();
    age(&stale, 3600);
    // Fresh ones go too: at startup no job of this process exists.
    let fresh = uploads.join(staging_name());
    std::fs::create_dir(&fresh).unwrap();
    // Look-alikes that must not be touched: wrong id length, a file, a symlink.
    let short = uploads.join(format!("{STAGING_PREFIX}abc"));
    std::fs::create_dir(&short).unwrap();
    age(&short, 3600);
    let file = uploads.join(staging_name());
    std::fs::write(&file, b"not a folder").unwrap();
    let target = f.root.join("precious");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), b"x").unwrap();
    let link = uploads.join(staging_name());
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let _store = Fixture::open(&f.root, &HashSet::new());
    assert!(!stale.exists());
    assert!(!fresh.exists());
    assert!(short.exists() && file.exists());
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"x");
}

#[test]
fn extraction_reservation_counts_against_the_pending_quota_until_released() {
    let f = Fixture::new();
    let reservation = f.store.begin_extraction("none", 1000, 200).unwrap();
    assert_eq!(kind_of(f.store.begin(4, "a.3mf")), "import_or_discard_first");
    // A second archive does not fit either, models would exceed 200 in total.
    assert_eq!(kind_of(f.store.begin_extraction("none", 1, 1)), "import_or_discard_first");
    drop(reservation);
    let mut p = f.store.begin(4, "a.3mf").unwrap();
    p.write(b"PK\x03\x04").unwrap();
    p.finish().unwrap();
}

#[test]
fn extraction_is_refused_before_unpacking_when_models_exceed_quota_or_free_space() {
    let f = Fixture::new();
    assert_eq!(kind_of(f.store.begin_extraction("none", 1, 201)), "import_or_discard_first");
    assert_eq!(kind_of(f.store.begin_extraction("none", 21 * 1024 * 1024 * 1024, 1)), "import_or_discard_first");
    // The 256 MiB reserve counts on top of the models: asking for all free space must fail.
    let available = Directory::open(&f.root.join("uploads")).unwrap().available().unwrap();
    if available < PENDING_BYTES {
        assert_eq!(kind_of(f.store.begin_extraction("none", available, 1)), "no_space");
    }
    // Open uploads already consume the quota: one at 1 MiB leaves room for 199 more models, not 200.
    let mut p = f.store.begin(4, "a.3mf").unwrap();
    p.write(b"PK\x03\x04").unwrap();
    p.finish().unwrap();
    assert_eq!(kind_of(f.store.begin_extraction("none", 10, 200)), "import_or_discard_first");
    assert!(f.store.begin_extraction("none", 10, 199).is_ok());
}

#[test]
fn adopting_models_links_them_without_overwriting_and_updates_the_journal_once() {
    let f = Fixture::new();
    let mut p = f.store.begin(4, "paket.zip").unwrap();
    p.write(b"PK\x03\x04").unwrap();
    let archive = p.finish().unwrap();
    let entry = f.store.begin_import(std::slice::from_ref(&archive.id), "job").unwrap().remove(0);
    let staging = f.root.join("uploads").join(staging_name());
    std::fs::create_dir_all(staging.join("x")).unwrap();
    std::fs::create_dir_all(staging.join("y")).unwrap();
    std::fs::write(staging.join("x/m.stl"), b"solid x").unwrap();
    std::fs::write(staging.join("y/m.stl"), b"solid y").unwrap();
    std::fs::write(f.root.join("uploads/m.stl"), b"vorhanden").unwrap();
    let staged = vec![
        StagedModel { dir: staging.join("x"), file: "m.stl".into(), name: "m.stl".into() },
        StagedModel { dir: staging.join("y"), file: "m.stl".into(), name: "m.stl".into() },
    ];
    let before = f.store.persist_count.load(Ordering::Relaxed);
    let children = f.store.adopt_extracted(&entry.id, "job", &staged, &HashSet::new(), serde_json::json!({"kind":"archive"})).unwrap();
    assert_eq!(f.store.persist_count.load(Ordering::Relaxed), before + 1);
    let names: Vec<_> = children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["m (2).stl", "m (3).stl"]);
    assert_eq!(std::fs::read(f.root.join("uploads/m.stl")).unwrap(), b"vorhanden");
    assert_eq!(std::fs::read(f.root.join("uploads/m (3).stl")).unwrap(), b"solid y");
    // The staging names go with the staging folder, after the journal write; only then
    // the journal's single-link check holds.
    assert!(f.store.checked_file(&children[0]).is_err());
    std::fs::remove_dir_all(&staging).unwrap();
    for child in &children {
        f.store.checked_file(child).unwrap();
        assert_eq!(child.state, UploadState::Importing);
    }
    let listed = f.store.list().unwrap();
    let parent = listed.iter().find(|e| e.id == archive.id).unwrap();
    assert!(parent.archive && parent.state == UploadState::Imported);
    // A second hand-over for the same archive is refused: it is no longer importing.
    assert_eq!(f.store.adopt_extracted(&entry.id, "job", &staged, &HashSet::new(), serde_json::json!({})).unwrap_err().kind, "upload_state");
}

fn archive_upload(f: &Fixture) -> Entry {
    let mut p = f.store.begin(4, "paket.zip").unwrap();
    p.write(b"PK\x03\x04").unwrap();
    p.finish().unwrap()
}
fn stage(f: &Fixture, files: &[(&str, &[u8])]) -> Vec<StagedModel> {
    let staging = f.root.join("uploads").join(staging_name());
    std::fs::create_dir_all(&staging).unwrap();
    files
        .iter()
        .map(|(name, data)| {
            std::fs::write(staging.join(name), data).unwrap();
            StagedModel { dir: staging.clone(), file: (*name).into(), name: (*name).into() }
        })
        .collect()
}

#[test]
fn a_crash_between_link_and_journal_leaves_no_unjournalled_model_and_the_archive_importable() {
    let f = Fixture::new();
    let archive = archive_upload(&f);
    let entry = f.store.begin_import(std::slice::from_ref(&archive.id), "job").unwrap().remove(0);
    let staged = stage(&f, &[("m.stl", b"solid m")]);
    f.store.crash_after_link.store(true, Ordering::SeqCst);
    assert!(f.store.adopt_extracted(&entry.id, "job", &staged, &HashSet::new(), serde_json::json!({})).is_err());
    // The process died here: the upload folder has a second name for the staged file.
    assert!(f.root.join("uploads/m.stl").exists());
    let restarted = Fixture::open(&f.root, &HashSet::new());
    assert!(!f.root.join("uploads/m.stl").exists(), "unjournalled link removed");
    let names: Vec<_> = std::fs::read_dir(f.root.join("uploads")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["paket.zip"], "staging folder gone as well");
    let listed = restarted.pending_list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].state, UploadState::Uploaded);
    // Importing again yields the plain name, not " (2)".
    let entry = restarted.begin_import(std::slice::from_ref(&archive.id), "job2").unwrap().remove(0);
    let staged = stage(&f, &[("m.stl", b"solid m")]);
    let children = restarted.adopt_extracted(&entry.id, "job2", &staged, &HashSet::new(), serde_json::json!({})).unwrap();
    assert_eq!(children[0].name, "m.stl");
}

#[test]
fn a_crash_after_the_journal_write_keeps_the_models_and_clears_the_staging_names() {
    let f = Fixture::new();
    let archive = archive_upload(&f);
    let entry = f.store.begin_import(std::slice::from_ref(&archive.id), "job").unwrap().remove(0);
    let staged = stage(&f, &[("m.stl", b"solid m")]);
    f.store.adopt_extracted(&entry.id, "job", &staged, &HashSet::new(), serde_json::json!({})).unwrap();
    // Dies before the staging folder is removed: the journalled file still has two names.
    let restarted = Fixture::open(&f.root, &HashSet::new());
    let pending = restarted.pending_list().unwrap();
    let model = pending.iter().find(|e| e.name == "m.stl").expect("model survives");
    assert_eq!(model.state, UploadState::Uploaded);
    restarted.checked_file(model).unwrap();
}

#[test]
fn a_handled_archive_is_listed_and_discarding_it_only_removes_its_own_file() {
    let f = Fixture::new();
    let archive = archive_upload(&f);
    let entry = f.store.begin_import(std::slice::from_ref(&archive.id), "job").unwrap().remove(0);
    let staged = stage(&f, &[("m.stl", b"solid m")]);
    let children = f.store.adopt_extracted(&entry.id, "job", &staged, &HashSet::new(), serde_json::json!({})).unwrap();
    std::fs::remove_dir_all(staged[0].dir.clone()).unwrap();
    let listed = f.store.pending_list().unwrap();
    assert!(listed.iter().any(|e| e.id == archive.id && e.archive && e.state == UploadState::Imported));
    // It cannot be imported again.
    assert_eq!(kind_of(f.store.begin_import(std::slice::from_ref(&archive.id), "again")), "upload_state");
    f.store.discard(&archive.id).unwrap();
    assert!(!f.root.join("uploads/paket.zip").exists());
    assert!(f.root.join("uploads/m.stl").exists(), "models are not touched");
    assert!(f.store.list().unwrap().iter().all(|e| e.id != archive.id));
    assert!(f.store.list().unwrap().iter().any(|e| e.id == children[0].id));
}

#[test]
fn pruning_never_forgets_a_handled_archive_that_is_still_on_disk() {
    let mut journal = Journal::default();
    let entry = |n: usize, archive: bool| Entry {
        id: format!("{n:032x}"),
        name: format!("e{n}"),
        size: 1,
        dev: 1,
        ino: n as u64,
        time: n as u64,
        state: UploadState::Imported,
        job_id: None,
        result: None,
        archive,
    };
    journal.entries.insert(entry(0, true).id, entry(0, true));
    for n in 1..=1005 {
        journal.entries.insert(entry(n, false).id, entry(n, false));
    }
    journal.prune();
    assert!(journal.entries.contains_key(&entry(0, true).id), "oldest entry is the archive");
    assert_eq!(journal.entries.values().filter(|e| !e.archive).count(), 1000);
}

#[test]
fn reopening_one_archive_leaves_the_other_entries_of_the_job_alone() {
    let f = Fixture::new();
    let a = archive_upload(&f);
    let mut p = f.store.begin(4, "b.3mf").unwrap();
    p.write(b"PK\x03\x04").unwrap();
    let b = p.finish().unwrap();
    f.store.begin_import(&[a.id.clone(), b.id.clone()], "job").unwrap();
    f.store.reopen(&a.id).unwrap();
    let listed = f.store.list().unwrap();
    assert_eq!(listed.iter().find(|e| e.id == a.id).unwrap().state, UploadState::Uploaded);
    assert_eq!(listed.iter().find(|e| e.id == b.id).unwrap().state, UploadState::Importing);
}
