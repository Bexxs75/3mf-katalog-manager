//! Tests of the start comparison (W2 5.4, Barrier test 3 of 7.3). A child of `fileops_http_tests`
//! to use its fixture; the waits are shortened through `Timing`.
use super::*;
use crate::invoke::{StartSyncPhase, StartSyncState};
use fileops::disk_start_sync::{row_hooks, start_with, Timing};
use katalog_core::disk_sync_settings::{set_flag, SETTING_AUTO_ADD, SETTING_ON_START};
use std::hash::{Hash, Hasher};
use std::os::unix::fs::symlink;
use std::time::Duration;

const OBJ: &[u8] = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
const OTHER_OBJ: &[u8] = b"v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n";

fn quick() -> Timing {
    Timing { first_wait: Duration::from_millis(10), retry_wait: Duration::from_millis(60), deadline: Duration::from_secs(120) }
}

impl Fixture {
    fn switches(&self, on_start: bool, auto_add: bool) {
        let conn = self.app.writer.as_ref().unwrap().conn.lock().unwrap();
        set_flag(&conn, SETTING_ON_START, on_start).unwrap();
        set_flag(&conn, SETTING_AUTO_ADD, auto_add).unwrap();
    }
    fn put_in(&self, root: &str, name: &str, bytes: &[u8]) {
        let path = self.dir.join(root).join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    fn put_model(&self, name: &str) {
        self.put_in("models", name, OBJ);
    }
    /// A catalog row for a file that is not on the disk (or that is, when written separately).
    fn row_for(&self, root: &str, name: &str) -> i64 {
        let path = self.dir.join(root).join(name);
        db::test_insert_minimal_file(&self.app.writer.as_ref().unwrap().conn.lock().unwrap(), path.to_str().unwrap(), None).unwrap()
    }
    /// Everything the catalog holds about files and folders, as one number.
    fn fingerprint(&self) -> u64 {
        let conn = self.app.writer.as_ref().unwrap().conn.lock().unwrap();
        let mut text = String::new();
        let mut files = conn.prepare("SELECT id, path, deleted_at, trash_path FROM files ORDER BY id").unwrap();
        for row in files.query_map([], |r| Ok(format!("{:?}|{:?}|{:?}|{:?};", r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?))).unwrap() {
            text.push_str(&row.unwrap());
        }
        let mut folders = conn.prepare("SELECT id, name FROM folders ORDER BY id").unwrap();
        for row in folders.query_map([], |r| Ok(format!("{:?}|{:?};", r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).unwrap() {
            text.push_str(&row.unwrap());
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }
    fn paths(&self) -> Vec<String> {
        let conn = self.app.writer.as_ref().unwrap().conn.lock().unwrap();
        let mut statement = conn.prepare("SELECT path FROM files WHERE deleted_at IS NULL ORDER BY path").unwrap();
        let rows = statement.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect();
        rows
    }
    fn result(&self) -> StartSyncState {
        self.app.geometry.start_sync.get()
    }
    fn backups_exist(&self) -> bool {
        std::fs::read_dir(self.dir.join("config/backups")).map(|d| d.count() > 0).unwrap_or(false)
    }
    /// Starts the run and waits for the end of its task.
    async fn run(&self) -> StartSyncState {
        self.run_with(quick()).await
    }
    async fn run_with(&self, timing: Timing) -> StartSyncState {
        let handle = start_with(&self.app, timing).expect("the run starts");
        tokio::time::timeout(Duration::from_secs(60), handle).await.expect("the run ends").unwrap();
        self.result()
    }
}

#[tokio::test]
async fn switched_off_nothing_happens_and_the_state_stays_off() {
    let f = Fixture::new(true).await;
    f.put_model("new.obj");
    let before = f.fingerprint();
    // Neither switch set, only the second one set: both mean no run.
    assert!(start_with(&f.app, quick()).is_none());
    f.switches(false, true);
    assert!(start_with(&f.app, quick()).is_none());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(f.result(), StartSyncState::default());
    assert_eq!(f.result().state, StartSyncPhase::Off);
    assert_eq!(f.fingerprint(), before);
    assert!(!f.backups_exist());
}

#[tokio::test]
async fn without_file_ops_or_write_nothing_is_started() {
    let f = Fixture::new(false).await;
    f.switches(true, true);
    f.put_model("new.obj");
    let before = f.fingerprint();
    assert!(start_with(&f.app, quick()).is_none());
    assert_eq!(f.result().state, StartSyncPhase::Off);
    assert_eq!(f.fingerprint(), before);
    let mut f = Fixture::new(true).await;
    f.switches(true, true);
    f.put_model("new.obj");
    Arc::get_mut(&mut f.app).unwrap().config.write = false;
    let before = f.fingerprint();
    assert!(start_with(&f.app, quick()).is_none());
    assert_eq!(f.result().state, StartSyncPhase::Off);
    assert_eq!(f.fingerprint(), before);
}

#[tokio::test]
async fn a_blocked_journal_means_no_automatic_writing() {
    let f = Fixture::new(true).await;
    f.switches(true, true);
    f.put_model("new.obj");
    let before = f.fingerprint();
    f.app.fileops.as_ref().unwrap().block_journal_for_test();
    let handle = start_with(&f.app, quick());
    if let Some(handle) = handle {
        handle.await.unwrap();
    }
    assert!(matches!(f.result().state, StartSyncPhase::Off | StartSyncPhase::Skipped));
    assert!(!f.result().ran);
    assert_eq!(f.fingerprint(), before);
}

#[tokio::test]
async fn report_only_without_auto_add_leaves_the_catalog_byte_equal() {
    let f = Fixture::new(true).await;
    f.switches(true, false);
    f.put_model("a.obj");
    f.put_model("sub/b.obj");
    f.row_for("models", "gone.obj");
    let before = f.fingerprint();
    let result = f.run().await;
    assert_eq!(f.fingerprint(), before);
    assert!(!f.backups_exist(), "a report-only run does not even make a copy");
    assert_eq!(
        result,
        StartSyncState { ran: true, busy: false, state: StartSyncPhase::Done, new_files: 2, missing_files: 1, ..Default::default() }
    );
    assert!(result.applied.is_none());
}

#[tokio::test]
async fn auto_add_takes_new_files_over_and_never_removes_anything() {
    let f = Fixture::new(true).await;
    f.switches(true, true);
    f.put_model("a.obj");
    f.put_model("sub/b.obj");
    let kept = f.row_for("models", "gone.obj");
    let existing = f.dir.join("models/known.obj");
    std::fs::write(&existing, OBJ).unwrap();
    f.row_for("models", "known.obj");
    let result = f.run().await;
    assert_eq!(result.state, StartSyncPhase::Done);
    assert_eq!((result.ran, result.new_files, result.missing_files), (true, 2, 1));
    assert_eq!(result.applied.map(|a| (a.added, a.renamed, a.failed)), Some((2, 0, 0)));
    let paths = f.paths();
    assert_eq!(paths.len(), 4, "{paths:?}");
    // The missing model stays a normal active row.
    let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    let (deleted_at, trash): (Option<String>, Option<String>) =
        conn.query_row("SELECT deleted_at, trash_path FROM files WHERE id=?1", [kept], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert!(deleted_at.is_none() && trash.is_none());
    drop(conn);
    assert!(f.backups_exist(), "the day copy exists before the first change");
    assert_eq!(f.app.fileops.as_ref().unwrap().pending_entries_for_test(), 0);
    // The files on the disk stay where they are.
    assert!(f.dir.join("models/a.obj").exists() && f.dir.join("models/sub/b.obj").exists());
}

#[tokio::test]
async fn at_most_two_hundred_files_are_taken_over_and_the_rest_stays_open() {
    let f = Fixture::new(true).await;
    f.switches(true, true);
    for n in 0..205 {
        f.put_model(&format!("f{n:03}.obj"));
    }
    let result = f.run().await;
    assert_eq!(result.new_files, 205);
    let applied = result.applied.unwrap();
    assert_eq!((applied.added, applied.failed), (200, 0));
    assert_eq!(f.paths().len(), 200);
    // The five that stayed open are the last ones, still on the disk and not in the catalog.
    for n in 200..205 {
        assert!(!f.paths().iter().any(|p| p.ends_with(&format!("f{n:03}.obj"))));
    }
}

#[tokio::test]
async fn an_unreachable_empty_or_replaced_root_means_nothing_is_applied() {
    // Two roots: models/x/a (with the upload folder) and models/x/b.
    for case in ["empty", "empty-with-trash", "gone", "replaced"] {
        let f = Fixture::with_layout(true, true).await;
        f.switches(true, true);
        f.put_in("models/x/a", "new.obj", OBJ);
        f.put_in("models/x/b", "known.obj", OBJ);
        f.row_for("models/x/b", "known.obj");
        match case {
            // Unmounted volume: an empty mount point that holds nothing but the trash folder the
            // server creates in every root at its start (the folder stays!).
            "empty-with-trash" => {
                for entry in std::fs::read_dir(f.dir.join("models/x/b")).unwrap() {
                    let path = entry.unwrap().path();
                    if path.file_name().is_some_and(|n| n == katalog_core::fsops::TRASH) {
                        continue;
                    }
                    if path.is_dir() { std::fs::remove_dir_all(path).unwrap() } else { std::fs::remove_file(path).unwrap() }
                }
                assert!(f.dir.join("models/x/b").join(katalog_core::fsops::TRASH).is_dir(), "the trash folder is there");
            }
            // Empty root (also without the hidden trash folder, as an unmounted volume is).
            "empty" => {
                for entry in std::fs::read_dir(f.dir.join("models/x/b")).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() { std::fs::remove_dir_all(path).unwrap() } else { std::fs::remove_file(path).unwrap() }
                }
            }
            "gone" => std::fs::remove_dir_all(f.dir.join("models/x/b")).unwrap(),
            _ => {
                std::fs::remove_dir_all(f.dir.join("models/x/b")).unwrap();
                std::fs::create_dir(f.dir.join("models/x/b")).unwrap();
            }
        }
        let before = f.fingerprint();
        let result = f.run().await;
        assert_eq!(f.fingerprint(), before, "{case}");
        assert!(!f.backups_exist(), "{case}");
        assert_eq!(
            result,
            StartSyncState { ran: true, state: StartSyncPhase::Done, unreachable_roots: 1, ..Default::default() },
            "{case}"
        );
    }
}

#[tokio::test]
async fn a_root_with_content_next_to_the_trash_folder_stays_usable() {
    let f = Fixture::with_layout(true, true).await;
    f.switches(true, true);
    f.put_in("models/x/a", "new.obj", OBJ);
    f.put_in("models/x/b", "known.obj", OBJ);
    f.row_for("models/x/b", "known.obj");
    assert!(f.dir.join("models/x/b").join(katalog_core::fsops::TRASH).is_dir());
    let result = f.run().await;
    assert_eq!(result.unreachable_roots, 0);
    assert_eq!(result.new_files, 1);
    assert_eq!(result.applied.map(|a| (a.added, a.failed)), Some((1, 0)));
}

#[tokio::test]
async fn maintenance_at_the_start_means_skipped() {
    for exclusive in [false, true] {
        let f = Fixture::new(true).await;
        f.switches(true, true);
        f.put_model("new.obj");
        let before = f.fingerprint();
        let mut window = f.app.geometry.barrier.enter_draining(Duration::from_secs(5)).await.unwrap();
        if exclusive {
            window = window.enter_exclusive(Duration::from_secs(5)).await.unwrap();
        }
        let result = f.run().await;
        assert_eq!(result.state, StartSyncPhase::Skipped);
        assert!(!result.ran && !result.busy);
        assert_eq!(f.fingerprint(), before);
        window.release();
        assert_eq!(f.app.geometry.barrier.snapshot().writers, 0);
    }
}

#[tokio::test]
async fn a_running_import_means_busy_first_and_skipped_after_the_second_try() {
    let f = Fixture::new(true).await;
    f.switches(true, true);
    f.put_model("new.obj");
    let before = f.fingerprint();
    let place = f.app.uploads.as_ref().unwrap().active.clone().try_acquire_owned().unwrap();
    let handle = start_with(&f.app, Timing { retry_wait: Duration::from_millis(300), ..quick() }).unwrap();
    // Immediately after the start and while it waits for the second try: running and busy.
    assert_eq!((f.result().state, f.result().busy), (StartSyncPhase::Running, true));
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!((f.result().state, f.result().busy), (StartSyncPhase::Running, true));
    handle.await.unwrap();
    assert_eq!(f.result().state, StartSyncPhase::Skipped);
    assert_eq!(f.fingerprint(), before);
    drop(place);
}

#[tokio::test]
async fn the_second_try_succeeds_when_the_place_is_free_again() {
    let f = Fixture::new(true).await;
    f.switches(true, true);
    f.put_model("new.obj");
    let place = f.app.uploads.as_ref().unwrap().active.clone().try_acquire_owned().unwrap();
    let handle = start_with(&f.app, Timing { retry_wait: Duration::from_millis(400), ..quick() }).unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    drop(place);
    handle.await.unwrap();
    let result = f.result();
    assert_eq!((result.state, result.applied.map(|a| a.added)), (StartSyncPhase::Done, Some(1)));
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn a_held_runtime_lock_means_busy_then_skipped() {
    let f = Fixture::new(true).await;
    f.switches(true, true);
    f.put_model("new.obj");
    let before = f.fingerprint();
    let rt = f.app.fileops.clone().unwrap();
    let held = rt.state.lock().unwrap();
    f.run().await;
    drop(held);
    assert_eq!(f.result().state, StartSyncPhase::Skipped);
    assert_eq!(f.fingerprint(), before);
}

#[tokio::test]
async fn maintenance_during_the_run_stops_between_rows_and_keeps_what_was_taken() {
    let f = Fixture::new(true).await;
    f.switches(true, true);
    for n in 0..5 {
        f.put_model(&format!("f{n}.obj"));
    }
    let window: Arc<Mutex<Option<crate::barrier::MaintenanceWindow>>> = Arc::default();
    let barrier = f.app.geometry.barrier.clone();
    let (handle, held) = (tokio::runtime::Handle::current(), window.clone());
    row_hooks().lock().unwrap().insert(
        f.app.config.db_path.clone(),
        Arc::new(move |done| {
            if done != 1 {
                return;
            }
            let (b, held) = (barrier.clone(), held.clone());
            handle.spawn(async move {
                let w = b.enter_draining(Duration::from_secs(20)).await.unwrap();
                *held.lock().unwrap() = Some(w);
            });
            for _ in 0..400 {
                if barrier.snapshot().state == crate::barrier::State::Draining {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }),
    );
    let result = f.run().await;
    row_hooks().lock().unwrap().remove(&f.app.config.db_path);
    // The row that was running ended, the next ones were not started.
    assert_eq!(result.state, StartSyncPhase::Done);
    assert_eq!(result.applied.map(|a| a.added), Some(1));
    assert_eq!(f.paths().len(), 1);
    // Barrier test 3: the window is granted, no writer is left, new writers are refused.
    for _ in 0..400 {
        if window.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(f.app.geometry.barrier.snapshot().writers, 0);
    assert!(f.app.geometry.barrier.writer().is_err());
    let w = window.lock().unwrap().take().expect("the exchange got its window");
    w.release();
    assert!(f.app.geometry.barrier.writer().is_ok());
}

#[tokio::test]
async fn the_row_checks_of_the_manual_apply_hold_for_changed_deleted_exchanged_and_linked_files() {
    let f = Fixture::new(true).await;
    f.switches(true, true);
    for name in ["a.obj", "b.obj", "c.obj", "d.obj", "e.obj", "sub/g.obj"] {
        f.put_model(name);
    }
    // A file in the upload folder is never offered, a link is no new file.
    f.put_in("models/uploads", "u.obj", OBJ);
    f.put_model("other/g.obj");
    symlink(f.dir.join("models/a.obj"), f.dir.join("models/zlink.obj")).unwrap();
    let dir = f.dir.clone();
    row_hooks().lock().unwrap().insert(
        f.app.config.db_path.clone(),
        Arc::new(move |done| {
            if done != 1 {
                return;
            }
            let models = dir.join("models");
            // changed: different length
            std::fs::write(models.join("b.obj"), OTHER_OBJ).unwrap();
            // deleted
            std::fs::remove_file(models.join("c.obj")).unwrap();
            // exchanged: same bytes, other inode
            std::fs::write(models.join("d.tmp"), OBJ).unwrap();
            std::fs::rename(models.join("d.tmp"), models.join("d.obj")).unwrap();
            // file replaced by a link
            std::fs::remove_file(models.join("e.obj")).unwrap();
            symlink(models.join("a.obj"), models.join("e.obj")).unwrap();
            // the folder replaced by a link to another folder with the same name inside
            std::fs::rename(models.join("sub"), models.join("sub.away")).unwrap();
            symlink(models.join("other"), models.join("sub")).unwrap();
        }),
    );
    let result = f.run().await;
    row_hooks().lock().unwrap().remove(&f.app.config.db_path);
    let applied = result.applied.unwrap();
    assert_eq!((applied.added, applied.failed), (2, 5), "{result:?}");
    let paths = f.paths();
    assert_eq!(paths.len(), 2, "{paths:?}");
    assert!(paths[0].ends_with("/models/a.obj") && paths[1].ends_with("/models/other/g.obj"), "{paths:?}");
    assert!(!paths.iter().any(|p| p.contains("uploads")));
    // The report listed a..e, other/g and sub/g, but neither the upload file nor the link.
    assert_eq!(result.new_files, 7);
}

#[tokio::test]
async fn a_failing_backup_ends_the_run_without_a_change() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(true).await;
    f.switches(true, true);
    f.put_model("new.obj");
    let backups = f.dir.join("config/backups");
    std::fs::create_dir_all(&backups).unwrap();
    // Group- and world-writable: the backup folder is refused as unsafe.
    std::fs::set_permissions(&backups, std::fs::Permissions::from_mode(0o777)).unwrap();
    let before = f.fingerprint();
    let result = f.run().await;
    assert_eq!(result.state, StartSyncPhase::Skipped);
    assert_eq!(f.fingerprint(), before);
}

#[tokio::test]
async fn the_task_holds_only_a_weak_reference_and_ends_with_the_server() {
    let mut f = Fixture::new(true).await;
    f.switches(true, true);
    f.put_model("new.obj");
    let weak = Arc::downgrade(&f.app);
    let handle = start_with(&f.app, Timing { first_wait: Duration::from_millis(300), ..quick() }).unwrap();
    // One weak reference is the task's, the other one is `weak` above.
    assert_eq!((Arc::strong_count(&f.app), Arc::weak_count(&f.app)), (1, 2));
    // The server goes away while the task sleeps: the last strong reference is dropped.
    let other = Fixture::new(true).await;
    drop(std::mem::replace(&mut f.app, other.app.clone()));
    assert!(weak.upgrade().is_none());
    tokio::time::timeout(Duration::from_secs(5), handle).await.expect("the task ended").unwrap();
    // Nothing was done.
    assert_eq!(f.app.geometry.start_sync.get().state, StartSyncPhase::Off);
}

#[tokio::test]
async fn exactly_one_run_per_start() {
    let f = Fixture::new(true).await;
    f.switches(true, true);
    f.put_model("a.obj");
    let handle = start_with(&f.app, quick()).unwrap();
    // A second call while it runs, and one after it is done, start nothing.
    assert!(start_with(&f.app, quick()).is_none());
    handle.await.unwrap();
    assert_eq!(f.result().state, StartSyncPhase::Done);
    f.put_model("b.obj");
    assert!(start_with(&f.app, quick()).is_none());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(f.paths().len(), 1);
}

#[tokio::test]
async fn run_start_sync_shows_running_and_then_the_numbers() {
    let f = Fixture::new(true).await;
    // Off: not run.
    let off = value(f.invoke("run_start_sync", json!({})).await).await;
    assert_eq!((off["ran"].clone(), off["state"].clone()), (json!(false), json!("off")));
    f.switches(true, true);
    f.put_model("a.obj");
    f.put_model("b.obj");
    f.row_for("models", "gone.obj");
    let handle = start_with(&f.app, Timing { first_wait: Duration::from_millis(200), ..quick() }).unwrap();
    let running = value(f.invoke("run_start_sync", json!({})).await).await;
    assert_eq!((running["busy"].clone(), running["state"].clone()), (json!(true), json!("running")));
    handle.await.unwrap();
    let done = value(f.invoke("run_start_sync", json!({})).await).await;
    assert_eq!(
        done,
        json!({"ran":true,"busy":false,"state":"done","newFiles":2,"renamed":0,"missingFiles":1,"missingFolders":0,
               "duplicates":0,"unreachableRoots":0,"applied":{"added":2,"renamed":0,"failed":0}})
    );
    assert!(!done.to_string().contains(f.dir.to_str().unwrap()));
}
