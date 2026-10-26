//! Catalog backup download: a consistent snapshot of the configured database,
//! packed as the desktop backup ZIP and streamed to the browser.
//!
//! The snapshot and the archive are built in `<db dir>/backups/` (not `/tmp`:
//! that is a RAM-backed tmpfs in the container and a catalog with many
//! thumbnails can be hundreds of MB). Both temp files are unlinked before the
//! first byte is sent, so an aborted download cannot leave anything behind;
//! leftovers of a crash during preparation are swept at startup.
use axum::{
    body::Body,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use katalog_core::{catalog_export, db, safe_file, upload_fs::Directory};
use std::{
    collections::HashMap,
    future::Future,
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Name prefix of every temp file this module creates; the startup sweep keys on it.
const TEMP_PREFIX: &str = ".web-export-";
/// One export per session per interval: it reads the whole catalog.
const MIN_INTERVAL_SECONDS: u64 = 30;
const BUSY_RETRY_SECONDS: u64 = 10;
/// Free space that must remain after both temp files exist.
const RESERVE: u64 = 64 * 1024 * 1024;
/// Overall limit for sending the archive; a stalled client must not hold the slot forever.
const BODY_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2 * 3600);
/// Temp files younger than this may belong to a run that is still preparing.
const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(3600);

pub(crate) struct Runtime {
    slots: Arc<Semaphore>,
    last: Mutex<HashMap<String, u64>>,
    reserve: u64,
    body_deadline: std::time::Duration,
}

impl Runtime {
    pub(crate) fn new() -> Self {
        Self { slots: Arc::new(Semaphore::new(1)), last: Mutex::new(HashMap::new()), reserve: RESERVE, body_deadline: BODY_DEADLINE }
    }
    /// Lets tests reach the disk-space and deadline paths without huge files or waiting.
    #[cfg(test)]
    fn tuned(reserve: u64, body_deadline: std::time::Duration) -> Self {
        Self { reserve, body_deadline, ..Self::new() }
    }
    /// Seconds to wait before this client may export again.
    fn wait_for(&self, key: &str, now: u64) -> Option<u64> {
        let mut last = self.last.lock().unwrap();
        last.retain(|_, at| now.saturating_sub(*at) < MIN_INTERVAL_SECONDS);
        last.get(key).map(|at| MIN_INTERVAL_SECONDS - now.saturating_sub(*at))
    }
    fn record(&self, key: &str, now: u64) {
        self.last.lock().unwrap().insert(key.to_owned(), now);
    }
}

enum Failure {
    Rate(u64),
    Busy,
    Space,
    Unavailable,
}

fn failure(status: u16, message: &str, retry: Option<u64>) -> Response {
    let mut response = (
        StatusCode::from_u16(status).unwrap(),
        Json(serde_json::json!({"message": message, "expected": status < 500})),
    )
        .into_response();
    if let Some(seconds) = retry {
        response.headers_mut().insert(header::RETRY_AFTER, seconds.to_string().parse().unwrap());
    }
    response
}

/// Space for the snapshot plus the archive (the database is not compressible
/// below its own size in the worst case), and the reserve on top.
fn required_space(database_bytes: u64, reserve: u64) -> u64 {
    database_bytes.saturating_mul(2).saturating_add(reserve)
}
fn check_space(available: u64, database_bytes: u64, reserve: u64) -> Result<(), Failure> {
    if available < required_space(database_bytes, reserve) {
        Err(Failure::Space)
    } else {
        Ok(())
    }
}

/// Removes its file when dropped, so every early return and panic cleans up.
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn work_dir(db_path: &Path, create: bool) -> io::Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let dir = db_path
        .canonicalize()?
        .parent()
        .ok_or_else(|| io::Error::other("database parent missing"))?
        .join("backups");
    if create && !dir.exists() {
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    }
    let metadata = std::fs::symlink_metadata(&dir)?;
    if !metadata.is_dir() || metadata.mode() & 0o022 != 0 {
        return Err(io::Error::other("unsafe backup directory"));
    }
    Ok(dir)
}

/// Deletes temp files (and SQLite journals next to them) a crashed run left behind.
/// Only regular files with our prefix and an age beyond `STALE_AFTER`: a run that is
/// preparing right now (or a second server sharing the directory) keeps its files.
pub(crate) fn sweep_stale(db_path: &Path) {
    sweep_older_than(db_path, STALE_AFTER);
}

fn sweep_older_than(db_path: &Path, age: std::time::Duration) {
    // Never creates the directory: a read-only deployment stays untouched.
    let Ok(dir) = work_dir(db_path, false) else { return };
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(TEMP_PREFIX) {
            continue;
        }
        // symlink_metadata: a link with our prefix is never followed or removed.
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
        let old_enough = meta.modified().ok().and_then(|m| m.elapsed().ok()).is_some_and(|a| a >= age);
        if meta.is_file() && old_enough {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

type Writer = Option<Arc<Mutex<katalog_core::write::Connection>>>;

/// Runs `f` on the connection the snapshot is read from. With a write connection
/// its mutex is held for the duration: the catalog is in rollback-journal mode,
/// where the copy's read lock would make every other committer fail with BUSY,
/// whereas writers queued on the mutex simply wait their turn. Without write mode
/// a private read-only connection is used. Callers keep `f` to the copy itself:
/// zipping and sending happen after the lock is gone.
fn with_source<R>(db_path: &Path, writer: &Writer, f: impl FnOnce(&rusqlite::Connection) -> Result<R, Failure>) -> Result<R, Failure> {
    match writer {
        Some(shared) => {
            let guard = shared.lock().map_err(|_| Failure::Unavailable)?;
            f(&guard)
        }
        None => {
            let conn = db::open_read_only(db_path).map_err(|_| Failure::Unavailable)?;
            f(&conn)
        }
    }
}

/// Builds the archive and returns it as a readable, already unlinked file.
fn prepare(db_path: &Path, writer: &Writer, reserve: u64) -> Result<(File, u64), Failure> {
    let unavailable = |_: &dyn std::fmt::Debug| Failure::Unavailable;
    let dir = work_dir(db_path, true).map_err(|e| unavailable(&e))?;
    let size = with_source(db_path, writer, |conn| catalog_export::database_bytes(conn).map_err(|e| unavailable(&e)))?;
    let available = Directory::open(&dir).and_then(|d| d.available()).map_err(|e| unavailable(&e))?;
    check_space(available, size, reserve)?;

    let (snapshot_path, snapshot_file) =
        safe_file::create_private_temp(&dir, &format!("{TEMP_PREFIX}db"), ".tmp").map_err(|e| unavailable(&e))?;
    drop(snapshot_file);
    let snapshot = Temp(snapshot_path);
    with_source(db_path, writer, |conn| catalog_export::snapshot(conn, &snapshot.0).map_err(|e| unavailable(&e)))?;
    // SQLite's rollback journal of the copy shares the name prefix and is gone after commit.
    let (archive_path, archive_file) =
        safe_file::create_private_temp(&dir, &format!("{TEMP_PREFIX}zip"), ".tmp").map_err(|e| unavailable(&e))?;
    let archive = Temp(archive_path);
    let written = catalog_export::write_zip(&snapshot.0, &catalog_export::settings_json(), archive_file)
        .map_err(|e| unavailable(&e))?;
    drop(written);
    drop(snapshot);
    let reader = File::open(&archive.0).map_err(|e| unavailable(&e))?;
    let length = reader.metadata().map_err(|e| unavailable(&e))?.len();
    // The open handle keeps the data alive; the name must not outlive this call.
    drop(archive);
    Ok((reader, length))
}

/// Ends the stream with an error once the overall deadline has passed and drops the
/// inner body at that moment, which frees the export slot and the file handle.
struct DeadlineBody {
    inner: Option<crate::download::DownloadBody>,
    deadline: std::pin::Pin<Box<tokio::time::Sleep>>,
}
impl DeadlineBody {
    fn new(inner: crate::download::DownloadBody, limit: std::time::Duration) -> Self {
        Self { inner: Some(inner), deadline: Box::pin(tokio::time::sleep(limit)) }
    }
}
impl hyper::body::Body for DeadlineBody {
    type Data = axum::body::Bytes;
    type Error = io::Error;
    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, io::Error>>> {
        use std::task::Poll;
        let this = self.get_mut();
        let Some(inner) = this.inner.as_mut() else { return Poll::Ready(None) };
        if this.deadline.as_mut().poll(cx).is_ready() {
            this.inner = None;
            return Poll::Ready(Some(Err(io::Error::new(io::ErrorKind::TimedOut, "Zeitlimit für die Sicherung überschritten"))));
        }
        let polled = std::pin::Pin::new(inner).poll_frame(cx);
        if matches!(polled, Poll::Ready(None) | Poll::Ready(Some(Err(_)))) {
            this.inner = None;
        }
        polled
    }
    fn is_end_stream(&self) -> bool {
        self.inner.as_ref().is_none_or(hyper::body::Body::is_end_stream)
    }
    fn size_hint(&self) -> hyper::body::SizeHint {
        self.inner.as_ref().map_or_else(|| hyper::body::SizeHint::with_exact(0), hyper::body::Body::size_hint)
    }
}

fn file_name(now: u64) -> String {
    let date = chrono::DateTime::from_timestamp(i64::try_from(now).unwrap_or(0), 0)
        .unwrap_or_default()
        .format("%Y%m%d");
    format!("3mf-katalog-sicherung-{date}.zip")
}

/// One audit line per request; carries a session fingerprint and sizes only.
pub(crate) fn audit_line(time: u64, session: u64, status: u16, size: u64) -> String {
    format!("export time={time} session={session:016x} status={status} size={size}")
}

pub(crate) async fn export(
    runtime: &Runtime,
    db_path: PathBuf,
    writer: Writer,
    key: &str,
    session: u64,
    monotonic: u64,
    now: u64,
) -> Response {
    let admitted = (|| {
        if let Some(wait) = runtime.wait_for(key, monotonic) {
            return Err(Failure::Rate(wait));
        }
        let permit = runtime.slots.clone().try_acquire_owned().map_err(|_| Failure::Busy)?;
        runtime.record(key, monotonic);
        Ok::<OwnedSemaphorePermit, Failure>(permit)
    })();
    let reserve = runtime.reserve;
    let result = match admitted {
        // The permit travels through the blocking task: a cancelled request must
        // not free the slot while the snapshot is still being written.
        Ok(permit) => match tokio::task::spawn_blocking(move || prepare(&db_path, &writer, reserve).map(|ready| (ready, permit))).await {
            Ok(result) => result,
            Err(_) => Err(Failure::Unavailable),
        },
        Err(failure) => Err(failure),
    };
    match result {
        Ok(((file, length), permit)) => {
            eprintln!("{}", audit_line(now, session, 200, length));
            log::info!(target: "backup", "Sicherung erstellt");
            let body = DeadlineBody::new(crate::download::DownloadBody::new(tokio::fs::File::from_std(file), length, permit), runtime.body_deadline);
            let mut response = Body::new(body).into_response();
            for (name, value) in [
                (header::CONTENT_TYPE, "application/zip".to_owned()),
                (header::CONTENT_LENGTH, length.to_string()),
                (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", file_name(now))),
                (header::CACHE_CONTROL, "no-store".to_owned()),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            ] {
                response.headers_mut().insert(name, value.parse().unwrap());
            }
            response
        }
        Err(failed) => {
            let (status, message, retry, kind) = match failed {
                Failure::Rate(wait) => (429, "Bitte kurz warten: Die Sicherung wurde gerade erst erstellt", Some(wait), "rate"),
                Failure::Busy => (503, "Es läuft bereits eine Sicherung", Some(BUSY_RETRY_SECONDS), "busy"),
                Failure::Space => (507, "Nicht genug freier Speicherplatz für die Sicherung", None, "space"),
                Failure::Unavailable => (500, "Die Sicherung konnte nicht erstellt werden", None, "unavailable"),
            };
            eprintln!("{}", audit_line(now, session, status, 0));
            eprintln!("export error_kind={kind}");
            failure(status, message, retry)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_check_needs_two_copies_plus_reserve() {
        let size = 100 * 1024 * 1024;
        assert!(check_space(2 * size + RESERVE, size, RESERVE).is_ok());
        assert!(matches!(check_space(2 * size + RESERVE - 1, size, RESERVE), Err(Failure::Space)));
        assert!(matches!(check_space(0, 0, RESERVE), Err(Failure::Space)));
        assert!(matches!(check_space(u64::MAX, u64::MAX, RESERVE), Ok(()) | Err(Failure::Space)));
    }

    #[test]
    fn rate_limit_is_per_key_and_expires() {
        let runtime = Runtime::new();
        assert_eq!(runtime.wait_for("a", 100), None);
        runtime.record("a", 100);
        assert_eq!(runtime.wait_for("a", 110), Some(20));
        assert_eq!(runtime.wait_for("b", 110), None);
        assert_eq!(runtime.wait_for("a", 130), None);
    }

    #[test]
    fn file_name_has_only_the_date() {
        assert_eq!(file_name(1_793_000_000), "3mf-katalog-sicherung-20261026.zip");
    }

    #[test]
    fn audit_line_has_no_path_and_carries_the_size() {
        let line = audit_line(5, 0xabc, 200, 4096);
        assert_eq!(line, "export time=5 session=0000000000000abc status=200 size=4096");
        assert!(!line.contains('/'));
    }

    fn catalog_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("export-unit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let conn = db::connect(&dir.join("catalog.db")).unwrap();
        db::test_insert_minimal_file(&conn, "/models/a.stl", None).unwrap();
        dir
    }
    fn age(path: &Path, seconds: u64) {
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(seconds)).unwrap();
    }
    async fn status_and_text(response: Response) -> (u16, String) {
        let status = response.status().as_u16();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap_or_default();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    #[test]
    fn sweep_removes_only_old_regular_prefixed_files() {
        use std::os::unix::fs::PermissionsExt;
        let dir = catalog_dir("sweep");
        let backups = dir.join("backups");
        std::fs::create_dir(&backups).unwrap();
        std::fs::set_permissions(&backups, std::fs::Permissions::from_mode(0o700)).unwrap();
        let old = backups.join(".web-export-db.1-2-3.tmp");
        let fresh = backups.join(".web-export-zip.1-2-3.tmp");
        let foreign = backups.join("web-fileops-day-1.db");
        let outside = dir.join("outside");
        std::fs::write(&outside, b"keep").unwrap();
        let link = backups.join(".web-export-link.tmp");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let folder = backups.join(".web-export-dir");
        std::fs::create_dir(&folder).unwrap();
        for file in [&old, &fresh, &foreign] {
            std::fs::write(file, b"x").unwrap();
        }
        age(&old, 7200);
        age(&foreign, 7200);
        sweep_stale(&dir.join("catalog.db"));
        assert!(!old.exists());
        assert!(fresh.exists() && foreign.exists() && outside.exists());
        assert!(link.symlink_metadata().is_ok() && folder.is_dir());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn too_little_free_space_answers_507_and_leaves_nothing() {
        let dir = catalog_dir("space");
        let runtime = Runtime::tuned(u64::MAX / 4, BODY_DEADLINE);
        let response = export(&runtime, dir.join("catalog.db"), None, "k", 0, 0, 1_793_000_000).await;
        let (status, text) = status_and_text(response).await;
        assert_eq!(status, 507);
        assert!(!text.contains(dir.to_str().unwrap()));
        assert_eq!(std::fs::read_dir(dir.join("backups")).unwrap().count(), 0);
        // The slot was returned: an export with a normal reserve now succeeds.
        let runtime2 = Runtime::new();
        let ok = export(&runtime2, dir.join("catalog.db"), None, "k", 0, 0, 1_793_000_000).await;
        assert_eq!(ok.status(), 200);
        drop(ok);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn unsafe_work_directories_are_refused_with_a_generic_500() {
        use std::os::unix::fs::PermissionsExt;
        let dir = catalog_dir("unsafe");
        let backups = dir.join("backups");
        std::fs::create_dir(&backups).unwrap();
        std::fs::set_permissions(&backups, std::fs::Permissions::from_mode(0o777)).unwrap();
        let (status, text) = status_and_text(export(&Runtime::new(), dir.join("catalog.db"), None, "a", 0, 0, 1).await).await;
        assert_eq!(status, 500);
        assert!(!text.contains(dir.to_str().unwrap()) && text.contains("Die Sicherung konnte nicht erstellt werden"));
        std::fs::remove_dir(&backups).unwrap();
        let target = dir.join("elsewhere");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::os::unix::fs::symlink(&target, &backups).unwrap();
        let (status, _) = status_and_text(export(&Runtime::new(), dir.join("catalog.db"), None, "b", 0, 0, 1).await).await;
        assert_eq!(status, 500);
        assert_eq!(std::fs::read_dir(&target).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn body_deadline_ends_the_stream_and_frees_the_slot() {
        use http_body_util::BodyExt;
        let dir = catalog_dir("deadline");
        let runtime = Runtime::tuned(RESERVE, std::time::Duration::from_secs(5));
        let response = export(&runtime, dir.join("catalog.db"), None, "k", 0, 0, 1).await;
        assert_eq!(response.status(), 200);
        assert_eq!(runtime.slots.available_permits(), 0);
        let mut body = response.into_body();
        assert!(body.frame().await.unwrap().is_ok());
        tokio::time::advance(std::time::Duration::from_secs(6)).await;
        assert!(body.frame().await.unwrap().is_err());
        assert_eq!(runtime.slots.available_permits(), 1);
        drop(body);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
