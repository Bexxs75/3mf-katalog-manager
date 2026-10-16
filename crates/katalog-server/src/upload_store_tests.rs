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
            catalogued,
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
    assert_eq!(
        sanitize_name("x.zip").unwrap_err().kind,
        "archive_desktop_only"
    );
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
    let mut entry = f.upload();
    entry.size = u64::MAX - RESERVE;
    f.store
        .inner
        .lock()
        .unwrap()
        .journal
        .entries
        .insert(entry.id.clone(), entry);
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
