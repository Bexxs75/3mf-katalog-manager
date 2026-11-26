//! W2 5.4: the one comparison of the disk with the catalog after the server start. Child of
//! `fileops` (like `fileops_disksync`) because it needs the runtime state and the backup helper.
//!
//! Safety rules this file carries (spec 5.4 and 3):
//! - Without `file_ops`, `write`, a usable file-ops runtime and the setting `sync_on_start`
//!   nothing runs and nothing is written; the result stays `off`.
//! - Without `sync_auto_add` the run only reports (numbers in memory, the catalog stays untouched).
//! - It only ever adds entries. Nothing is removed, renamed or moved, and no file on disk changes.
//! - A root that is not `Ok` (unplugged, empty, replaced) stops everything: counted, nothing applied.
//! - Every new file goes through the same functions as `disk_sync_apply_block`
//!   (`plan_new_file`, `read_new_file`, `store_new_file`), so the per-row checks of spec 2.6 apply.
//! - The run holds the barrier writer, the import place and the runtime lock like a file action,
//!   and stops between two rows when maintenance starts or the 120-second server limit is over.
//! - The task holds only a weak reference to the app and runs once per server start.
use super::*;
use crate::invoke::{StartSyncApplied, StartSyncPhase, StartSyncState};
use katalog_core::disk_apply::{self, Anchored, CatalogPaths};
use katalog_core::fsops::FsDatabase;
use katalog_core::disk_report::{self, RootStatus};
use katalog_core::disk_sync_settings::{self, SETTING_AUTO_ADD, SETTING_ON_START};
use std::path::PathBuf;
use std::sync::Weak;
#[cfg(test)]
use std::collections::HashMap;

/// New files one run takes over; more stay open for the manual comparison.
const MAX_ADD: usize = 200;

#[derive(Clone, Copy)]
pub(crate) struct Timing {
    /// Let the main services start first.
    pub(crate) first_wait: Duration,
    /// One more try after this long when an import or a file action holds the place.
    pub(crate) retry_wait: Duration,
    /// Server limit for the taking over.
    pub(crate) deadline: Duration,
}
impl Default for Timing {
    fn default() -> Self {
        Self { first_wait: Duration::from_secs(5), retry_wait: Duration::from_secs(60), deadline: Duration::from_secs(120) }
    }
}

enum Outcome {
    /// Switched off in the meantime: nothing happened.
    Off,
    /// An import or another file action holds the place.
    Busy,
    /// Not started, or ended without a change.
    Skipped,
    Done(StartSyncState),
}

/// Test hook per database path: called after each row that was looked at, with the number of rows done so far.
#[cfg(test)]
pub(crate) type RowHook = Arc<dyn Fn(usize) + Send + Sync>;
#[cfg(test)]
pub(crate) fn row_hooks() -> &'static Mutex<HashMap<PathBuf, RowHook>> {
    static HOOKS: std::sync::OnceLock<Mutex<HashMap<PathBuf, RowHook>>> = std::sync::OnceLock::new();
    HOOKS.get_or_init(Default::default)
}

/// Called once at the end of the server build; returns at once. The work runs in a task.
pub(in crate::web) fn start(app: &Arc<App>) {
    let _ = start_with(app, Timing::default());
}

pub(crate) fn start_with(app: &Arc<App>, timing: Timing) -> Option<tokio::task::JoinHandle<()>> {
    let store = app.geometry.start_sync.clone();
    // One run per server start: a result (or a running run) is never replaced.
    if store.get().state != StartSyncPhase::Off || !capability(app) {
        return None;
    }
    let enabled = katalog_core::db::open_read_only(&app.config.db_path)
        .ok()
        .and_then(|conn| disk_sync_settings::flag(&conn, SETTING_ON_START).ok())
        .unwrap_or(false);
    if !enabled {
        return None;
    }
    store.set(StartSyncState { busy: true, state: StartSyncPhase::Running, ..Default::default() });
    // Startup holds the instance lock already; a sleeping task must not keep the app (and with
    // it that lock) alive after the server is gone.
    let weak = Arc::downgrade(app);
    Some(tokio::spawn(async move {
        tokio::time::sleep(timing.first_wait).await;
        let mut retried = false;
        let outcome = loop {
            let Some(app) = weak.upgrade() else { return };
            match once(app, &weak, timing).await {
                Outcome::Busy if !retried => {
                    retried = true;
                    tokio::time::sleep(timing.retry_wait).await;
                }
                other => break other,
            }
        };
        store.set(match outcome {
            Outcome::Done(state) => state,
            Outcome::Off => StartSyncState::default(),
            Outcome::Busy | Outcome::Skipped => StartSyncState { state: StartSyncPhase::Skipped, ..Default::default() },
        });
    }))
}

/// One try: barrier writer first, then the blocking work with its guards.
async fn once(app: Arc<App>, weak: &Weak<App>, timing: Timing) -> Outcome {
    // Maintenance is running (or a restore): the run does not start.
    let Ok(guard) = app.geometry.barrier.writer() else {
        return Outcome::Skipped;
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let barrier = app.geometry.barrier.clone();
    let mut work = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        attempt(&app, &cancel, timing)
    });
    loop {
        tokio::select! {
            joined = &mut work => return joined.unwrap_or(Outcome::Skipped),
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                if barrier.snapshot().state != crate::barrier::State::Open || weak.strong_count() <= 1 {
                    signal.store(true, Ordering::SeqCst);
                }
            }
        }
    }
}

/// Import place and runtime lock, like every file action. Never waits.
fn attempt(app: &Arc<App>, cancel: &Arc<AtomicBool>, timing: Timing) -> Outcome {
    let Some(rt) = app.fileops.clone() else { return Outcome::Skipped };
    let _import = match &app.uploads {
        Some(upload) => {
            let Ok(place) = upload.active.clone().try_acquire_owned() else { return Outcome::Busy };
            match upload.store.has_importing() {
                Ok(false) => Some(place),
                Ok(true) => return Outcome::Busy,
                Err(_) => return Outcome::Skipped,
            }
        }
        None => None,
    };
    let mut state = match rt.state.try_lock() {
        Ok(state) => state,
        Err(std::sync::TryLockError::WouldBlock) => return Outcome::Busy,
        Err(_) => return Outcome::Skipped,
    };
    // A journal that needs a manual look means: no automatic writing.
    if !rt.available() || state.ops.journal.is_blocked() {
        return Outcome::Skipped;
    }
    let outcome = work(app, &mut state, cancel, timing);
    state.ops.db.cancellation = None;
    outcome
}

fn work(app: &Arc<App>, state: &mut RuntimeState, cancel: &Arc<AtomicBool>, timing: Timing) -> Outcome {
    let ops = &mut state.ops;
    // Read again: the switches may have changed while the run waited.
    let flags = ops.db.with_conn(|conn| {
        Ok(disk_sync_settings::flag(conn, SETTING_ON_START)
            .and_then(|on| disk_sync_settings::flag(conn, SETTING_AUTO_ADD).map(|auto| (on, auto))))
    });
    let Ok(Ok((on_start, auto_add))) = flags else {
        return Outcome::Skipped;
    };
    if !on_start {
        return Outcome::Off;
    }
    let Ok(Ok(snapshot)) = ops.db.with_conn(|conn| Ok(disk_report::snapshot(conn))) else {
        return Outcome::Skipped;
    };
    let excluded: Vec<PathBuf> = app.config.upload.iter().map(|u| u.dir.clone()).collect();
    let report = disk_report::build_report_with(
        &snapshot,
        &app.config.file_roots,
        disk_report::ReportOptions { limits: disk_report::Limits::default(), excluded: &excluded },
    );
    // An unplugged or replaced drive must never lead to a change.
    let unreachable = report.roots.iter().filter(|r| r.status != RootStatus::Ok).count();
    if unreachable > 0 {
        return Outcome::Done(StartSyncState {
            ran: true,
            state: StartSyncPhase::Done,
            unreachable_roots: unreachable,
            ..Default::default()
        });
    }
    let mut result = StartSyncState {
        ran: true,
        state: StartSyncPhase::Done,
        new_files: report.new_files.len(),
        missing_files: report.missing_models.len(),
        missing_folders: report.missing_folders.len(),
        ..Default::default()
    };
    // Without the second switch the run is a report only.
    if !auto_add {
        return Outcome::Done(result);
    }
    let Ok(anchored) = Anchored::new(&app.config.file_roots, &excluded) else {
        return Outcome::Done(result);
    };
    let Ok(Ok(known)) = ops.db.with_conn(|conn| Ok(CatalogPaths::load(conn))) else {
        return Outcome::Done(result);
    };
    // The plan is made from the state just read; each row is read again right before its change.
    let mut failed = 0;
    let mut rows = Vec::new();
    for file in report.new_files.iter().take(MAX_ADD) {
        match disk_apply::plan_new_file(&anchored, &known, file.root, &file.relative_path) {
            Ok(row) => rows.push(row),
            Err(_) => failed += 1,
        }
    }
    let mut applied = StartSyncApplied { added: 0, renamed: 0, failed };
    if rows.is_empty() {
        result.applied = Some(applied);
        return Outcome::Done(result);
    }
    // The day copy first; no copy, no change.
    if backup(app, rows.len(), false).is_err() {
        return Outcome::Skipped;
    }
    let barrier = app.geometry.barrier.clone();
    let flag = cancel.clone();
    let started = Instant::now();
    let cancellation = crate::fileops_cancel::Cancellation::new(move || {
        barrier.snapshot().state == crate::barrier::State::Open
            && !flag.load(Ordering::SeqCst)
            && started.elapsed() < timing.deadline
    });
    ops.db.cancellation = Some(cancellation.clone());
    let limits = app.config.upload.as_ref().map(|u| u.limits).unwrap_or_default();
    let step_fn = |file: &std::fs::File| crate::write::step_metadata(&app.geometry.step, file);
    let step = app.geometry.step.available().then_some(&step_fn as katalog_core::rescan::StepReader<'_>);
    let mut aborted = false;
    for row in &rows {
        if !cancellation.allowed() {
            aborted = true;
            break;
        }
        match disk_apply::read_new_file(row, &anchored, limits, step) {
            Err(_) => applied.failed += 1,
            Ok(prepared) => match ops.db.with_conn(|conn| Ok(disk_apply::store_new_file(conn, &anchored, prepared))) {
                Ok(Ok(_)) => applied.added += 1,
                Ok(Err(_)) => applied.failed += 1,
                // Interrupted by maintenance or the time limit: nothing of this row was kept.
                Err(_) => {
                    aborted = true;
                    break;
                }
            },
        }
        #[cfg(test)]
        {
            let hook = row_hooks().lock().unwrap().get(&app.config.db_path).cloned();
            if let Some(hook) = hook {
                hook(applied.added + applied.failed);
            }
        }
    }
    if aborted {
        eprintln!("start comparison stopped early: added={} failed={}", applied.added, applied.failed);
    }
    result.applied = Some(applied);
    Outcome::Done(result)
}

