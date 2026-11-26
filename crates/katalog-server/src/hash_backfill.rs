//! Content hashes for rows that have none, most often right after a restored backup.
//!
//! A restore clears every hash on purpose: duplicate decisions must rest on the local
//! files. The desktop fills them in at start; this is the server's counterpart. While a
//! run is due, imports and the "new files" scan are refused (`GeometryGate::hash_backfill`),
//! because without hashes they would take every known file for a new one.
//!
//! The run is a catalog writer like any other job: it holds a barrier writer guard, stops
//! as soon as maintenance begins or the server goes away, and starts over afterwards.
//! Paths come from the catalog and with it possibly from an uploaded backup, so a file is
//! only read when it resolves into a configured catalog root (the rule of `download.rs`).
use super::*;
use katalog_core::{
    db::error::DbError,
    import::{
        files::compute_reader_hash,
        hash_backfill::{self, CatalogWork, HashBackfill, HashBackfillSummary},
    },
    safe_file,
};
use std::{
    io,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Weak,
    },
    time::{Duration, Instant},
};

const RETRY: Duration = Duration::from_secs(5);
const REPORT_EVERY: Duration = Duration::from_secs(30);
/// Unreadable files named in the log; the rest is only counted.
const NAMED_FAILURES: usize = 20;

/// Called once at the end of the server build; returns at once.
pub(in crate::web) fn start(app: &Arc<App>) {
    let _ = start_with(app, RETRY);
}

pub(crate) fn start_with(app: &Arc<App>, retry: Duration) -> Option<tokio::task::JoinHandle<()>> {
    let writer = app.writer.as_ref()?;
    let waiting = writer
        .conn
        .lock()
        .map_err(|_| DbError::Other("catalog lock poisoned".into()))
        .and_then(|conn| hash_backfill::has_missing_content_hash(&conn));
    match waiting {
        Ok(true) => (),
        Ok(false) => return None,
        Err(e) => {
            eprintln!("Prüfsummen: Katalog nicht lesbar, Prüfung entfällt: {e}");
            return None;
        }
    }
    // Set before the first request is served: no import can slip in ahead of the run.
    let gate = app.geometry.hash_backfill.clone();
    gate.set(true);
    eprintln!("Prüfsummen: Der Katalog enthält Dateien ohne Prüfsumme; Importe warten, bis sie ergänzt sind.");
    // Like the start comparison: a waiting task must not keep the app alive.
    let weak = Arc::downgrade(app);
    Some(tokio::spawn(async move {
        loop {
            let Some(app) = weak.upgrade() else { return };
            if once(app, &weak).await {
                gate.set(false);
                return;
            }
            tokio::time::sleep(retry).await;
        }
    }))
}

/// Stops the blocking run when its watcher goes away, also when the runtime drops the task.
struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// One pass. `true` once every row was visited; `false` asks for another try later.
async fn once(app: Arc<App>, weak: &Weak<App>) -> bool {
    // Maintenance or a finished restore: no writer is admitted.
    let Ok(guard) = app.geometry.barrier.writer() else {
        return false;
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = CancelOnDrop(cancel.clone());
    let barrier = app.geometry.barrier.clone();
    let mut work = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        run(&app, &cancel)
    });
    loop {
        tokio::select! {
            joined = &mut work => return joined.unwrap_or(false),
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                if barrier.snapshot().state != crate::barrier::State::Open || weak.strong_count() <= 1 {
                    signal.0.store(true, Ordering::SeqCst);
                }
            }
        }
    }
}

fn run(app: &App, cancel: &AtomicBool) -> bool {
    let Some(writer) = &app.writer else { return true };
    let mut host = Server {
        conn: &writer.conn,
        roots: &app.config.roots,
        cancel,
        reported: Instant::now(),
        named: 0,
        #[cfg(test)]
        row_hook: row_hooks().lock().unwrap().get(&app.config.db_path).cloned(),
    };
    match hash_backfill::backfill_content_hashes(&mut host) {
        Ok(summary) => {
            let HashBackfillSummary { hashed, failed, refused, complete } = summary;
            let state = if complete { "abgeschlossen" } else { "unterbrochen, wird fortgesetzt" };
            eprintln!("Prüfsummen: {state}: {hashed} ergänzt, {failed} nicht lesbar, {refused} außerhalb der Katalogordner.");
            complete
        }
        Err(e) => {
            eprintln!("Prüfsummen: Katalogfehler, neuer Versuch folgt: {e}");
            false
        }
    }
}

#[cfg(test)]
pub(crate) type RowHook = Arc<dyn Fn(usize, &AtomicBool) + Send + Sync>;
/// Tests pause a run between two rows, keyed by the catalog path of their server. The hook
/// also sees the run's cancel flag, so a test can wait for the cancellation itself.
#[cfg(test)]
pub(crate) fn row_hooks() -> &'static Mutex<HashMap<PathBuf, RowHook>> {
    static HOOKS: std::sync::OnceLock<Mutex<HashMap<PathBuf, RowHook>>> = std::sync::OnceLock::new();
    HOOKS.get_or_init(Default::default)
}

struct Server<'a> {
    conn: &'a Mutex<rusqlite::Connection>,
    roots: &'a [PathBuf],
    cancel: &'a AtomicBool,
    reported: Instant,
    named: usize,
    #[cfg(test)]
    row_hook: Option<RowHook>,
}
impl Server<'_> {
    fn access(&self, work: CatalogWork<'_>) -> Result<(), DbError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| DbError::Other("catalog lock poisoned".into()))?;
        work(&mut conn)
    }
}
impl HashBackfill for Server<'_> {
    fn read_batch(&mut self, work: CatalogWork<'_>) -> Result<(), DbError> {
        self.access(work)
    }
    fn store(&mut self, work: CatalogWork<'_>) -> Result<(), DbError> {
        self.access(work)
    }
    fn hash(&mut self, path: &str) -> Option<Result<String, String>> {
        let opened = open_admitted(self.roots, Path::new(path))?;
        Some(opened.map_err(|e| e.to_string()).and_then(|file| {
            compute_reader_hash(Cancellable(file, self.cancel)).map_err(|e| e.to_string())
        }))
    }
    fn failed(&mut self, path: &str, error: &str) {
        // An interrupted read is not a finding about the file.
        if self.cancel.load(Ordering::SeqCst) {
            return;
        }
        self.named += 1;
        if self.named <= NAMED_FAILURES {
            eprintln!("Prüfsummen: übersprungen: {path}: {error}");
        }
    }
    fn progress(&mut self, summary: &HashBackfillSummary) {
        #[cfg(test)]
        if let Some(hook) = &self.row_hook {
            hook(summary.hashed + summary.failed + summary.refused, self.cancel);
        }
        if self.reported.elapsed() >= REPORT_EVERY {
            self.reported = Instant::now();
            eprintln!("Prüfsummen: {} Dateien bearbeitet …", summary.hashed + summary.failed + summary.refused);
        }
    }
    fn stop(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// Ends a long read between two chunks once the run is cancelled.
struct Cancellable<'a, R>(R, &'a AtomicBool);
impl<R: io::Read> io::Read for Cancellable<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.1.load(Ordering::SeqCst) {
            return Err(io::Error::other("cancelled"));
        }
        self.0.read(buf)
    }
}

/// Opens a catalog file for reading. `None`: the path is not admitted and nothing of the
/// file is read. Admitted is an absolute path without `.`/`..` that is no link itself and
/// resolves into a catalog root; the opened handle must be the file that was checked.
pub(crate) fn open_admitted(roots: &[PathBuf], path: &Path) -> Option<io::Result<std::fs::File>> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return None;
    }
    let canonical_roots: Vec<PathBuf> = roots.iter().filter_map(|r| r.canonicalize().ok()).collect();
    // Decided on the text alone, so a path elsewhere is not even looked at.
    if !roots.iter().chain(&canonical_roots).any(|root| path.starts_with(root)) {
        return None;
    }
    let listed = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) => return Some(Err(e)),
    };
    if listed.file_type().is_symlink() {
        return None;
    }
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => return Some(Err(e)),
    };
    // A linked folder on the way must not lead out of the roots.
    if !canonical_roots.iter().any(|root| canonical.starts_with(root)) {
        return None;
    }
    Some((|| {
        let admitted = std::fs::symlink_metadata(&canonical)?;
        if !admitted.is_file() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "keine reguläre Datei"));
        }
        let file = safe_file::open_regular(&canonical)?;
        // Same guarantee as a download: never read from a handle that is not the checked file.
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let opened = file.metadata()?;
            if admitted.dev() != opened.dev() || admitted.ino() != opened.ino() {
                return Err(io::Error::other("Datei während des Öffnens ausgetauscht"));
            }
        }
        Ok(file)
    })())
}

#[cfg(test)]
#[path = "hash_backfill_tests.rs"]
mod tests;
