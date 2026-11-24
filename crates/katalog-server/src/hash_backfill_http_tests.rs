// Forgejo #75: after a restored backup the catalog has no content hashes, and an import of
// the same source folder copied every known file again.
use super::super::{hash_backfill, restore, restore_upload};
use super::*;
use std::time::Duration;

const OTHER: &[u8] = b"v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n";
const RETRY: Duration = Duration::from_millis(10);

async fn import(f: &Fixture) -> Value {
    let response = f.invoke("start_source_import", json!({"rootIdx":0,"rel":""})).await;
    assert_eq!(response.status(), 200);
    let id = value(response).await["jobId"].clone();
    let result = wait_result(f, &id).await;
    assert_eq!(result["state"], "completed", "{result}");
    result
}
fn state_of<'a>(result: &'a Value, name: &str) -> &'a str {
    result["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["name"] == name)
        .unwrap_or_else(|| panic!("{name} missing in {result}"))["state"]
        .as_str()
        .unwrap()
}
/// (models, models with a content hash)
fn hashes(f: &Fixture) -> (i64, i64) {
    let conn = db::open_read_only(&f.app.config.db_path).unwrap();
    conn.query_row("SELECT count(*), count(content_hash) FROM files", [], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
}
fn sql(f: &Fixture, statement: &str, params: &[&dyn rusqlite::ToSql]) {
    f.app.writer.as_ref().unwrap().conn.lock().unwrap().execute(statement, params).unwrap();
}
fn model_files(f: &Fixture) -> usize {
    std::fs::read_dir(f.dir.join("models"))
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().is_file())
        .count()
}
/// Two imported models whose hashes are gone, as a restore leaves them.
async fn two_models_without_hashes() -> Fixture {
    let f = source_fixture(false);
    std::fs::write(f.app.config.source_roots[0].join("other.obj"), OTHER).unwrap();
    let result = import(&f).await;
    assert_eq!((state_of(&result, "source.obj"), state_of(&result, "other.obj")), ("imported", "imported"));
    sql(&f, "UPDATE files SET content_hash = NULL", &[]);
    f
}
/// The server process ends after a restore; the next one opens the same folders.
fn restart(mut f: Fixture) -> Fixture {
    let dir = std::mem::take(&mut f.dir);
    let source_roots = f.app.config.source_roots.clone();
    drop(f);
    let mut next = Fixture::open(dir, true, &[]);
    Arc::get_mut(&mut next.app).unwrap().config.source_roots = source_roots;
    next
}

/// Holds a run after every row until the test lets it go on.
struct Pause {
    reached: std::sync::mpsc::Receiver<usize>,
    go: std::sync::mpsc::Sender<()>,
    /// Set by a test that wants the run to go on only once its cancellation has arrived.
    await_cancel: Arc<std::sync::atomic::AtomicBool>,
    key: std::path::PathBuf,
}
impl Pause {
    fn install(f: &Fixture) -> Self {
        let (reached_tx, reached) = std::sync::mpsc::channel();
        let (go, go_rx) = std::sync::mpsc::channel::<()>();
        let (reached_tx, go_rx) = (Mutex::new(reached_tx), Mutex::new(go_rx));
        let key = f.app.config.db_path.clone();
        let await_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let wait = await_cancel.clone();
        hash_backfill::row_hooks().lock().unwrap().insert(
            key.clone(),
            Arc::new(move |rows, cancel| {
                let _ = reached_tx.lock().unwrap().send(rows);
                // A dropped sender means the test is over: go on.
                let _ = go_rx.lock().unwrap().recv();
                // Bounded, so a missing cancellation fails the test instead of hanging it.
                for _ in 0..20_000 {
                    if !wait.load(Ordering::SeqCst) || cancel.load(Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }),
        );
        Self { reached, go, await_cancel, key }
    }
    async fn row(&self) -> usize {
        for _ in 0..2000 {
            if let Ok(rows) = self.reached.try_recv() {
                return rows;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("the run did not reach a row");
    }
}
impl Drop for Pause {
    fn drop(&mut self) {
        hash_backfill::row_hooks().lock().unwrap().remove(&self.key);
    }
}
async fn until(what: &str, done: impl Fn() -> bool) {
    for _ in 0..2000 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("{what}");
}

#[tokio::test]
async fn import_after_a_restored_backup_finds_the_known_files_again() {
    let f = source_fixture(false);
    assert_eq!(state_of(&import(&f).await, "source.obj"), "imported");
    assert_eq!(state_of(&import(&f).await, "source.obj"), "duplicate");
    assert_eq!((hashes(&f), model_files(&f)), ((1, 1), 1));

    // A backup of exactly this state, restored the way the route does it.
    let (snapshot, archive) = (f.dir.join("backup.db"), f.dir.join("backup.zip"));
    {
        let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        katalog_core::catalog_export::snapshot(&conn, &snapshot).unwrap();
    }
    drop(
        katalog_core::catalog_export::write_zip(
            &snapshot,
            &katalog_core::catalog_export::settings_json(),
            std::fs::File::create(&archive).unwrap(),
        )
        .unwrap(),
    );
    let (catalog, report) = restore_upload::stage(&f.app.config, &std::fs::read(&archive).unwrap()).unwrap();
    assert_eq!(report.model_count, 1);
    let response = restore::activate(f.app.clone(), f.session.clone(), catalog, Duration::from_secs(5)).await;
    assert_eq!(response.status(), 200, "{}", value(response).await);

    let f = restart(f);
    // The restore keeps the model and its file but drops the hash.
    assert_eq!((hashes(&f), model_files(&f)), ((1, 0), 1));
    let run = hash_backfill::start_with(&f.app, RETRY).expect("a run is due after the restore");
    run.await.unwrap();
    assert!(!f.app.geometry.hash_backfill.pending());
    assert_eq!(hashes(&f), (1, 1));

    assert_eq!(state_of(&import(&f).await, "source.obj"), "duplicate");
    assert_eq!((hashes(&f), model_files(&f)), ((1, 1), 1));
    // Nothing is left for the next start.
    assert!(hash_backfill::start_with(&f.app, RETRY).is_none());
    assert!(!f.app.geometry.hash_backfill.pending());
}

#[tokio::test]
async fn imports_and_the_new_files_scan_wait_until_the_hashes_are_back() {
    let f = two_models_without_hashes().await;
    // One row points outside the catalog roots (here: at the source file). It is never
    // read and keeps no hash, and it must not hold the imports back for ever.
    let outside = f.app.config.source_roots[0].join("other.obj");
    sql(&f, "UPDATE files SET path = ?1 WHERE path LIKE '%other.obj'", &[&outside.to_string_lossy().into_owned()]);
    let pause = Pause::install(&f);
    let run = hash_backfill::start_with(&f.app, RETRY).expect("a run is due");
    pause.row().await;
    assert!(f.app.geometry.hash_backfill.pending());
    for (command, args) in [
        ("start_source_import", json!({"rootIdx":0,"rel":""})),
        ("start_upload_import", json!({"uploadIds":["0123456789abcdef"]})),
        ("scan_source_folders", json!({})),
    ] {
        let response = f.invoke(command, args).await;
        assert_eq!(response.status(), 409, "{command}");
        assert_eq!(value(response).await["message"], "catalogHashCheckPending", "{command}");
    }
    // Everything else stays available.
    assert_eq!(f.invoke("list_source_folders", json!({})).await.status(), 200);
    assert_eq!(model_files(&f), 2);

    drop(pause);
    run.await.unwrap();
    assert!(!f.app.geometry.hash_backfill.pending());
    assert_eq!(hashes(&f), (2, 1));
    assert_eq!(f.invoke("scan_source_folders", json!({})).await.status(), 200);
    assert_eq!(state_of(&import(&f).await, "source.obj"), "duplicate");
}

#[tokio::test]
async fn maintenance_interrupts_the_run_and_it_resumes_afterwards() {
    let f = two_models_without_hashes().await;
    let pause = Pause::install(&f);
    let run = hash_backfill::start_with(&f.app, RETRY).expect("a run is due");
    assert_eq!(pause.row().await, 1);

    let barrier = f.app.geometry.barrier.clone();
    let drain = tokio::spawn(async move { barrier.enter_draining(Duration::from_secs(20)).await });
    until("maintenance did not begin", || {
        f.app.geometry.barrier.snapshot().state == crate::barrier::State::Draining
    })
    .await;
    // The run goes on only after its watcher has passed the cancellation on.
    pause.await_cancel.store(true, Ordering::SeqCst);
    pause.go.send(()).unwrap();
    // Draining only ends when the run has given up its writer guard.
    let window = drain.await.unwrap().unwrap();
    assert_eq!(hashes(&f), (2, 1));
    assert!(f.app.geometry.hash_backfill.pending());

    drop(pause);
    window.release();
    run.await.unwrap();
    assert_eq!(hashes(&f), (2, 2));
    assert!(!f.app.geometry.hash_backfill.pending());
}

#[tokio::test]
async fn a_run_whose_task_is_dropped_stops_before_the_next_file() {
    let f = two_models_without_hashes().await;
    let pause = Pause::install(&f);
    let run = hash_backfill::start_with(&f.app, RETRY).expect("a run is due");
    assert_eq!(pause.row().await, 1);
    // What the runtime does to the task when the server shuts down.
    run.abort();
    assert!(run.await.unwrap_err().is_cancelled());
    pause.await_cancel.store(true, Ordering::SeqCst);
    pause.go.send(()).unwrap();
    until("the run kept its writer guard", || f.app.geometry.barrier.snapshot().writers == 0).await;
    assert_eq!(hashes(&f), (2, 1));
    assert!(f.app.geometry.hash_backfill.pending());
}
