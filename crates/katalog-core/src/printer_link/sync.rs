//! Sync: fetches finished prints of all connected printers. The database is only
//! locked for short read/write steps, never during a network request.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use super::api_key::ApiKey;
use super::{LinkError, PrinterLink};
use crate::db::error::DbError;
use crate::db::printer_link as store;

pub const EVENT_JOBS_CHANGED: &str = "printer-jobs-changed";
const MAX_NEW_JOBS: usize = 200;
/// Printers asked at the same time. More would not help a home network and only multiply the
/// threads waiting for slow printers.
pub const MAX_PARALLEL: usize = 4;
/// Waits before the first and second repetition of a request that failed in a way that usually
/// passes (printer restarting, Wi-Fi hiccup).
const RETRY_PAUSES: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(5)];

pub type LinkMaker = dyn Fn(&str, &str, Option<&ApiKey>) -> Result<Box<dyn PrinterLink>, LinkError> + Send + Sync;

pub fn unix_now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn lock(db: &Mutex<Connection>) -> Result<std::sync::MutexGuard<'_, Connection>, DbError> {
    db.lock().map_err(|_| DbError::Other("database lock poisoned".into()))
}

/// How one pass runs. The defaults are what the app uses; tests shorten the waits.
#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// "Connect now": ignores the waiting time after failures.
    pub force: bool,
    pub max_parallel: usize,
    /// Pauses before each repetition; its length is the number of repetitions.
    pub retry_pauses: Vec<Duration>,
    /// Everything one printer may take in this pass, repetitions included.
    pub printer_budget: Duration,
}

impl Default for SyncOptions {
    fn default() -> Self {
        SyncOptions {
            force: false,
            max_parallel: MAX_PARALLEL,
            retry_pauses: RETRY_PAUSES.to_vec(),
            printer_budget: super::moonraker::PRINTER_BUDGET,
        }
    }
}

struct PlanItem {
    printer_id: i64,
    kind: String,
    address: String,
    since: f64,
}

/// One pass over all non-paused connections that are due. Returns the number of newly taken over prints.
pub fn sync_once(db: &Mutex<Connection>, make: &LinkMaker, now: f64) -> Result<usize, DbError> {
    sync_with(db, make, now, &SyncOptions::default())
}

/// Like [`sync_once`], with options. The printers are asked side by side (at most
/// `max_parallel`), so a slow one delays only itself. The database is locked for short steps
/// only, never during a request.
pub fn sync_with(db: &Mutex<Connection>, make: &LinkMaker, now: f64, options: &SyncOptions) -> Result<usize, DbError> {
    let (plan, waiting) = {
        let conn = lock(db)?;
        if !store::printer_link_enabled(&conn)? {
            return Ok(0);
        }
        let mut plan = Vec::new();
        let mut waiting = 0;
        for c in store::list_connections(&conn)?.into_iter().filter(|c| !c.paused) {
            if !super::status::is_due(&c, now, options.force) {
                waiting += 1;
                continue;
            }
            let since = store::sync_from(&conn, c.printer_id, now)?;
            plan.push(PlanItem { printer_id: c.printer_id, kind: c.kind, address: c.address, since });
        }
        (plan, waiting)
    };
    let printer_count = plan.len();
    if waiting > 0 {
        log::debug!(target: "drucker", "{waiting} Drucker warten nach Fehlern auf den nächsten Versuch");
    }

    let new_jobs = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let failure: Mutex<Option<DbError>> = Mutex::new(None);
    // (position in the plan, error code) so the log does not depend on which thread finishes first.
    let errors: Mutex<Vec<(usize, &'static str)>> = Mutex::new(Vec::new());
    let queue: Mutex<VecDeque<(usize, PlanItem)>> = Mutex::new(plan.into_iter().enumerate().collect());
    let workers = options.max_parallel.clamp(1, MAX_PARALLEL).min(printer_count);

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Some((position, item)) = queue.lock().ok().and_then(|mut q| q.pop_front()) else { break };
                match sync_printer(db, make, now, options, &item, &new_jobs, &stop) {
                    Ok(Some(code)) => {
                        if let Ok(mut e) = errors.lock() {
                            e.push((position, code));
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        if let Ok(mut f) = failure.lock() {
                            f.get_or_insert(e);
                        }
                        stop.store(true, Ordering::SeqCst);
                    }
                }
            });
        }
    });

    if let Some(e) = failure.into_inner().ok().flatten() {
        return Err(e);
    }
    let mut error_list = errors.into_inner().unwrap_or_default();
    error_list.sort_by_key(|(position, _)| *position);
    let error_codes: Vec<&'static str> = error_list.into_iter().map(|(_, code)| code).collect();
    let new_jobs = new_jobs.load(Ordering::SeqCst);
    log::info!(target: "drucker", "{}", sync_summary(printer_count, new_jobs, &error_codes));
    Ok(new_jobs)
}

/// "Include older history": takes over the prints that ended before the connection was made, as
/// history for the statistics. They are closed from the start and never offered for deduction.
/// `Ok(Err(code))` is the reason nothing was read; `Ok(Ok(n))` the number of new rows. One attempt,
/// no repetitions: the user is waiting and can press the button again.
pub fn include_older_history(
    db: &Mutex<Connection>,
    make: &LinkMaker,
    printer_id: i64,
) -> Result<Result<usize, &'static str>, DbError> {
    let (kind, address, api_key, before) = {
        let conn = lock(db)?;
        if !store::printer_link_enabled(&conn)? {
            return Ok(Err("disabled"));
        }
        let Some(c) = store::get_connection(&conn, printer_id)? else { return Ok(Err("no_connection")) };
        (c.kind, c.address, store::get_api_key(&conn, printer_id)?, store::older_history_before(&conn, printer_id, c.connected_since)?)
    };
    let fetched = make(&kind, &address, api_key.as_ref()).and_then(|mut link| {
        link.set_deadline(Instant::now() + super::moonraker::PRINTER_BUDGET);
        let info = link.test()?;
        link.jobs_ended_before(&info, before)
    });
    let jobs = match fetched {
        Ok(jobs) => jobs,
        Err(e) => return Ok(Err(e.code())),
    };
    let conn = lock(db)?;
    // The connection may have been removed while the printer was asked.
    if store::get_connection(&conn, printer_id)?.is_none() {
        return Ok(Err("no_connection"));
    }
    let mut added = 0;
    for job in &jobs {
        if store::insert_older_job_if_new(&conn, printer_id, job)? {
            added += 1;
        }
    }
    log::info!(target: "drucker", "Drucker {printer_id}: {added} ältere Drucke als Verlauf übernommen");
    Ok(Ok(added))
}

/// Whether the printer should still be contacted: the switch is on and the connection exists and
/// is not paused. Turning the switch off ends the whole pass.
fn still_wanted(db: &Mutex<Connection>, printer_id: i64, stop: &AtomicBool) -> Result<bool, DbError> {
    let conn = lock(db)?;
    if !store::printer_link_enabled(&conn)? {
        stop.store(true, Ordering::SeqCst);
        return Ok(false);
    }
    Ok(matches!(store::get_connection(&conn, printer_id)?, Some(c) if !c.paused))
}

/// One printer of a pass. `Some(code)` is the error that was recorded for it.
fn sync_printer(
    db: &Mutex<Connection>,
    make: &LinkMaker,
    now: f64,
    options: &SyncOptions,
    item: &PlanItem,
    new_jobs: &AtomicUsize,
    stop: &AtomicBool,
) -> Result<Option<&'static str>, DbError> {
    let printer_id = item.printer_id;
    let mut api_key: Option<ApiKey>;
    if new_jobs.load(Ordering::SeqCst) >= MAX_NEW_JOBS {
        return Ok(None);
    }
    // The plan was made once at the start and the pass can take a while. So check again briefly
    // before contacting the printer: if the switch was turned off meanwhile, the whole pass ends
    // (no more packets to any printer); if only this connection was removed or paused, only it is skipped.
    {
        let conn = lock(db)?;
        if !store::printer_link_enabled(&conn)? {
            stop.store(true, Ordering::SeqCst);
            return Ok(None);
        }
        match store::get_connection(&conn, printer_id)? {
            Some(c) if !c.paused => {}
            _ => return Ok(None),
        }
        // The plan holds no key: read it now, so one deleted or replaced since is not sent.
        api_key = store::get_api_key(&conn, printer_id)?;
        if store::open_job_limit_reached(&conn, printer_id)? {
            store::record_sync_error(&conn, printer_id, "too_many_open_jobs", now)?;
            return Ok(Some("too_many_open_jobs"));
        }
    }

    let deadline = Instant::now() + options.printer_budget;
    let mut repetition = 0;
    let result = loop {
        let attempt = make(&item.kind, &item.address, api_key.as_ref()).and_then(|mut link| {
            link.set_deadline(deadline);
            let info = link.test()?;
            let jobs = link.jobs_ended_since(&info, item.since)?;
            Ok((info, jobs))
        });
        match attempt {
            Err(e) if e.is_transient() && repetition < options.retry_pauses.len() => {
                let pause = options.retry_pauses[repetition];
                if Instant::now() + pause >= deadline {
                    break Err(e);
                }
                log::debug!(target: "drucker", "Drucker {printer_id}: {} , neuer Versuch in {} s", e.code(), pause.as_secs());
                std::thread::sleep(pause);
                repetition += 1;
                if !still_wanted(db, printer_id, stop)? {
                    return Ok(None);
                }
                let conn = lock(db)?;
                api_key = store::get_api_key(&conn, printer_id)?;
            }
            other => break other,
        }
    };

    let conn = lock(db)?;
    // The connection may have been removed during the network request; then the
    // result must not be written anymore.
    if store::get_connection(&conn, printer_id)?.is_none() {
        return Ok(None);
    }
    match result {
        Ok((info, mut jobs)) => {
            // The cursor follows end time, whereas Moonraker pages by start time.
            jobs.sort_by(|a, b| a.ended_at.total_cmp(&b.ended_at));
            for job in &jobs {
                // Stop at the first print that no longer fits: a later one (even a history-only
                // print, which has no limit) would move the cursor past the dropped one.
                if new_jobs.load(Ordering::SeqCst) >= MAX_NEW_JOBS || store::open_job_limit_reached(&conn, printer_id)? {
                    break;
                }
                if store::insert_job_if_new(&conn, printer_id, job)? {
                    new_jobs.fetch_add(1, Ordering::SeqCst);
                }
            }
            let error = if store::open_job_limit_reached(&conn, printer_id)? {
                store::record_sync_error(&conn, printer_id, "too_many_open_jobs", now)?;
                Some("too_many_open_jobs")
            } else {
                store::record_sync_success(&conn, printer_id, now, &info.base_url, &info.version)?;
                None
            };
            store::set_clock_offset(&conn, printer_id, info.clock_offset_s)?;
            // A printer with a wrong system clock (see `record_sync_success`'s use of
            // `info.clock_offset_s` to translate its print end times) is otherwise
            // invisible in the log; note it once per pass while it's non-zero.
            if info.clock_offset_s.abs() >= 1.0 {
                log::info!(target: "drucker", "Drucker {printer_id}: Uhrabweichung {:.0} s", info.clock_offset_s);
            }
            Ok(error)
        }
        Err(e) => {
            store::record_sync_error(&conn, printer_id, e.code(), now)?;
            Ok(Some(e.code()))
        }
    }
}

/// One-line summary logged exactly once per sync pass.
fn sync_summary(printer_count: usize, new_jobs: usize, error_codes: &[&str]) -> String {
    if error_codes.is_empty() {
        format!("{printer_count} Drucker abgefragt, {new_jobs} neue Drucke")
    } else {
        format!(
            "{printer_count} Drucker abgefragt, {new_jobs} neue Drucke, {} Fehler ({})",
            error_codes.len(),
            error_codes.join(", ")
        )
    }
}

/// Wakes the background sync immediately (button, switch, new connection). The flag is "Connect
/// now": the pass then also contacts printers that are waiting after failures.
pub struct SyncWaker(pub mpsc::Sender<bool>);

impl SyncWaker {
    pub fn wake(&self) {
        let _ = self.0.send(false);
    }

    pub fn wake_now(&self) {
        let _ = self.0.send(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::printer_link::{get_connection, list_open_jobs, save_connection_after_test, set_printer_link_enabled};
    use crate::printer_link::address::AddressPolicy;
    use crate::printer_link::fake_moonraker::{sv08, FakeServer, Reply};
    use crate::printer_link::{ConnectionInfo, JobOutcome, RemoteJob};
    use std::sync::{Arc, Mutex};

    #[test]
    fn summary_lists_error_codes_when_present() {
        assert_eq!(sync_summary(3, 2, &[]), "3 Drucker abgefragt, 2 neue Drucke");
        assert_eq!(
            sync_summary(3, 2, &["unreachable"]),
            "3 Drucker abgefragt, 2 neue Drucke, 1 Fehler (unreachable)"
        );
        assert_eq!(
            sync_summary(3, 0, &["unreachable", "auth_required"]),
            "3 Drucker abgefragt, 0 neue Drucke, 2 Fehler (unreachable, auth_required)"
        );
    }

    fn db_with_connection(address: &str, connected_since: f64) -> Mutex<Connection> {
        let conn = crate::db::connect_in_memory().unwrap();
        conn.execute("INSERT INTO printers (id, name, position) VALUES (1, 'Sovol SV08', 0)", []).unwrap();
        save_connection_after_test(&conn, 1, "moonraker", address, None, "", "", connected_since).unwrap();
        Mutex::new(conn)
    }

    fn test_maker() -> Box<LinkMaker> {
        Box::new(|kind: &str, address: &str, key: Option<&ApiKey>| crate::printer_link::make_link(kind, address, key, AddressPolicy::TEST))
    }

    struct BoundedHistory;

    impl PrinterLink for BoundedHistory {
        fn test(&self) -> Result<ConnectionInfo, LinkError> {
            Ok(ConnectionInfo { version: "v0".into(), base_url: "http://printer".into(), clock_offset_s: 0.0 })
        }
        fn jobs_ended_since(&self, _info: &ConnectionInfo, since: f64) -> Result<Vec<RemoteJob>, LinkError> {
            let jobs: Vec<_> = (0..600).map(|i| serde_json::json!({
                "job_id": i.to_string(), "end_time": if i == 0 { 1100.0 } else { 1000.0 },
                "filament_used": 1.0, "filename": "界".repeat(600), "status": "completed"
            })).collect();
            Ok(crate::printer_link::moonraker::parse_history_page(&serde_json::json!({
                "result": {"count": 1_000_000, "jobs": jobs}
            }))?.jobs.into_iter().filter(|j| j.ended_at > since).collect())
        }
        fn thumbnail(&self, _base_url: &str, _path: &str) -> Result<Vec<u8>, LinkError> {
            Err(LinkError::Unreachable)
        }
    }

    #[test]
    fn bounded_batches_keep_tied_end_times_without_sockets() {
        let db = db_with_connection("printer", 0.0);
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        let make: Box<LinkMaker> = Box::new(|_, _, _| Ok(Box::new(BoundedHistory)));
        for expected in [200, 200, 100] {
            assert_eq!(sync_once(&db, &*make, 2000.0).unwrap(), expected);
        }
        // Once full, the preflight must explain the limit without contacting the printer.
        let never_contact: Box<LinkMaker> = Box::new(|_, _, _| panic!("full queue contacted printer"));
        assert_eq!(sync_once(&db, &*never_contact, 2001.0).unwrap(), 0);
        let conn = db.lock().unwrap();
        let jobs = list_open_jobs(&conn).unwrap();
        assert_eq!(jobs.len(), 500);
        assert!(jobs.iter().all(|job| job.ended_at == 1000.0 && job.file_name.len() == 510));
        assert_eq!(get_connection(&conn, 1).unwrap().unwrap().last_error.as_deref(), Some("too_many_open_jobs"));
    }

    struct ThreeAfterFull;

    impl PrinterLink for ThreeAfterFull {
        fn test(&self) -> Result<ConnectionInfo, LinkError> {
            Ok(ConnectionInfo { version: "v0".into(), base_url: "http://printer".into(), clock_offset_s: 0.0 })
        }
        fn jobs_ended_since(&self, _info: &ConnectionInfo, since: f64) -> Result<Vec<RemoteJob>, LinkError> {
            let job = |id: &str, ended_at: f64, deductible: bool| RemoteJob {
                remote_id: id.into(),
                file_name: format!("{id}.gcode"),
                outcome: JobOutcome::Completed,
                raw_status: "completed".into(),
                ended_at,
                print_duration_s: 60.0,
                used_mm: if deductible { 100.0 } else { 0.0 },
                deductible,
                slicer_total_mm: None,
                slicer_weight_g: None,
                material: None,
                thumbnail_path: None,
            };
            Ok(vec![job("A", 1100.0, true), job("B", 1200.0, true), job("C", 1300.0, false)]
                .into_iter()
                .filter(|j| j.ended_at > since)
                .collect())
        }
        fn thumbnail(&self, _base_url: &str, _path: &str) -> Result<Vec<u8>, LinkError> {
            Err(LinkError::Unreachable)
        }
    }

    #[test]
    fn a_print_dropped_for_the_open_limit_is_not_skipped_by_the_cursor() {
        let db = db_with_connection("printer", 1000.0);
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        {
            let conn = db.lock().unwrap();
            for i in 0..store::MAX_OPEN_JOBS - 1 {
                conn.execute(
                    "INSERT INTO printer_jobs (printer_id, remote_id, file_name, outcome, raw_status, ended_at, print_duration_s, used_mm, state, auto_closed)
                     VALUES (1, ?1, 'x.gcode', 'completed', 'completed', 1000.0, 1.0, 1.0, 'open', 0)",
                    [format!("old{i}")],
                )
                .unwrap();
            }
        }
        let make: Box<LinkMaker> = Box::new(|_, _, _| Ok(Box::new(ThreeAfterFull)));
        assert_eq!(sync_once(&db, &*make, 2000.0).unwrap(), 1, "only A fits");
        let count = |id: &str| -> i64 {
            db.lock().unwrap().query_row("SELECT COUNT(*) FROM printer_jobs WHERE remote_id = ?1", [id], |r| r.get(0)).unwrap()
        };
        assert_eq!((count("A"), count("B"), count("C")), (1, 0, 0), "C must not be stored behind the dropped B");
        assert!(store::sync_from(&db.lock().unwrap(), 1, 2000.0).unwrap() < 1200.0, "the cursor stays before B");
        {
            let conn = db.lock().unwrap();
            let id = list_open_jobs(&conn).unwrap()[0].id;
            store::mark_job_ignored(&conn, id, "now").unwrap();
        }
        assert_eq!(sync_once(&db, &*make, 2000.0).unwrap(), 1, "B is fetched once a slot is free");
        assert_eq!((count("B"), count("C")), (1, 0));
    }

    struct KeyRecorder(Arc<Mutex<Vec<Option<String>>>>);

    #[test]
    fn the_key_is_read_when_the_printer_is_asked_not_when_the_plan_was_made() {
        let db = db_with_connection("printer", 0.0);
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        db.lock().unwrap().execute("UPDATE printer_connections SET api_key = 'secretkey123'", []).unwrap();
        let item = PlanItem { printer_id: 1, kind: "moonraker".into(), address: "printer".into(), since: 0.0 };
        let seen = KeyRecorder(Arc::new(Mutex::new(Vec::new())));
        let log = seen.0.clone();
        let make: Box<LinkMaker> = Box::new(move |_, _, key| {
            log.lock().unwrap().push(key.map(|_| "key".to_string()));
            Err(LinkError::Unreachable)
        });
        let options = SyncOptions { retry_pauses: vec![], ..SyncOptions::default() };
        let ask = || sync_printer(&db, &*make, 10.0, &options, &item, &AtomicUsize::new(0), &AtomicBool::new(false)).unwrap();
        ask();
        // The user deletes the key while the pass is under way: the next attempt must not send it.
        store::clear_api_key(&db.lock().unwrap(), 1).unwrap();
        ask();
        assert_eq!(*seen.0.lock().unwrap(), vec![Some("key".to_string()), None]);
    }

    struct OlderPages;

    impl PrinterLink for OlderPages {
        fn test(&self) -> Result<ConnectionInfo, LinkError> {
            Ok(ConnectionInfo { version: "v0".into(), base_url: "http://printer".into(), clock_offset_s: 0.0 })
        }
        fn jobs_ended_since(&self, _info: &ConnectionInfo, _since: f64) -> Result<Vec<RemoteJob>, LinkError> {
            Ok(vec![])
        }
        /// Five older prints; the printer answers with the two newest ones before `before`.
        fn jobs_ended_before(&self, _info: &ConnectionInfo, before: f64) -> Result<Vec<RemoteJob>, LinkError> {
            let mut jobs: Vec<RemoteJob> = (1..=5)
                .map(|i| RemoteJob {
                    remote_id: format!("o{i}"),
                    file_name: format!("o{i}.gcode"),
                    outcome: JobOutcome::Completed,
                    raw_status: "completed".into(),
                    ended_at: 100.0 * i as f64,
                    print_duration_s: 60.0,
                    used_mm: 10.0,
                    deductible: true,
                    slicer_total_mm: None,
                    slicer_weight_g: None,
                    material: None,
                    thumbnail_path: None,
                })
                .filter(|j| j.ended_at <= before)
                .collect();
            jobs.sort_by(|a, b| b.ended_at.total_cmp(&a.ended_at));
            jobs.truncate(2);
            Ok(jobs)
        }
        fn thumbnail(&self, _base_url: &str, _path: &str) -> Result<Vec<u8>, LinkError> {
            Err(LinkError::Unreachable)
        }
    }

    #[test]
    fn older_history_goes_on_where_the_last_press_stopped() {
        let db = db_with_connection("printer", 1000.0);
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        let make: Box<LinkMaker> = Box::new(|_, _, _| Ok(Box::new(OlderPages)));
        // The boundary print of the last press is read again (ties in end time must not be lost)
        // and ignored as a duplicate, so each further press adds the next older one.
        let added: Vec<usize> = (0..6).map(|_| include_older_history(&db, &*make, 1).unwrap().unwrap()).collect();
        assert_eq!(added, [2, 1, 1, 1, 0, 0], "every press goes on behind the last one, not over the newest page again");
        let total: i64 = db.lock().unwrap().query_row("SELECT COUNT(*) FROM printer_jobs", [], |r| r.get(0)).unwrap();
        assert_eq!(total, 5);
    }

    #[test]
    fn new_job_budget_is_shared_by_all_printers() {
        let server = FakeServer::start(|target| {
            if target.starts_with("/server/info") {
                return Some((200, crate::printer_link::fake_moonraker::fixture_bytes("server_info_v0_8_0_209.json")));
            }
            let jobs: Vec<_> = (0..250).map(|i| serde_json::json!({
                "job_id": i.to_string(), "end_time": if i == 0 { 1100.0 } else { 1000.0 },
                "filament_used": 1.0, "filename": "a.gcode", "status": "completed"
            })).collect();
            Some((200, serde_json::to_vec(&serde_json::json!({"result": {"count": 250, "jobs": jobs}})).unwrap()))
        });
        let db = db_with_connection(&server.address(), 0.0);
        {
            let conn = db.lock().unwrap();
            conn.execute("INSERT INTO printers (id, name, position) VALUES (2, 'B', 1)", []).unwrap();
            save_connection_after_test(&conn, 2, "moonraker", &server.address(), None, "", "", 0.0).unwrap();
            set_printer_link_enabled(&conn, true).unwrap();
        }
        assert_eq!(sync_once(&db, &*test_maker(), 2000.0).unwrap(), MAX_NEW_JOBS);
        assert_eq!(list_open_jobs(&db.lock().unwrap()).unwrap().len(), MAX_NEW_JOBS);
        assert_eq!(sync_once(&db, &*test_maker(), 2000.0).unwrap(), MAX_NEW_JOBS);
        assert_eq!(sync_once(&db, &*test_maker(), 2000.0).unwrap(), 100);
        assert_eq!(list_open_jobs(&db.lock().unwrap()).unwrap().len(), 500);
    }

    #[test]
    fn hostile_history_stays_bounded_across_three_passes() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let page = AtomicUsize::new(0);
        let server = FakeServer::start(move |target| {
            if target.starts_with("/server/info") {
                return Some((200, crate::printer_link::fake_moonraker::fixture_bytes("server_info_v0_8_0_209.json")));
            }
            let base = page.fetch_add(1, Ordering::SeqCst) * 50;
            let jobs: Vec<_> = (0..50).map(|i| serde_json::json!({
                "job_id": format!("{}", base + i), "end_time": 1e12 + (base + i) as f64,
                "filament_used": 1.0, "filename": "界".repeat(600), "status": "界".repeat(600),
                "metadata": {"filament_type": "界".repeat(600),
                    "thumbnails": [{"width": 1, "height": 1, "relative_path": "x".repeat(2048)}]}
            })).collect();
            Some((200, serde_json::to_vec(&serde_json::json!({"result": {"count": 1_000_000, "jobs": jobs}})).unwrap()))
        });
        let db = db_with_connection(&server.address(), 0.0);
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        for expected in [200, 200, 100] {
            assert_eq!(sync_once(&db, &*test_maker(), 2000.0).unwrap(), expected);
            let conn = db.lock().unwrap();
            let jobs = list_open_jobs(&conn).unwrap();
            assert!(jobs.len() <= store::MAX_OPEN_JOBS);
            assert!(jobs.iter().all(|j| j.remote_id.len() <= 512 && j.file_name.len() <= 512
                && j.raw_status.len() <= 512 && j.material.as_ref().is_none_or(|s| s.len() <= 512)
                && j.thumbnail_path.as_ref().is_none_or(|s| s.len() <= 1024)));
            assert_eq!(store::sync_from(&conn, 1, 2000.0).unwrap(), 0.0);
        }
        assert_eq!(get_connection(&db.lock().unwrap(), 1).unwrap().unwrap().last_error.as_deref(), Some("too_many_open_jobs"));
        let requests = server.requests().len();
        assert_eq!(sync_once(&db, &*test_maker(), 2000.0).unwrap(), 0);
        assert_eq!(server.requests().len(), requests);
    }

    #[test]
    fn nothing_happens_while_switched_off() {
        let server = sv08();
        let db = db_with_connection(&server.address(), 0.0);
        assert_eq!(sync_once(&db, &*test_maker(), 2e9).unwrap(), 0);
        assert!(server.requests().is_empty());
    }

    #[test]
    fn only_jobs_ending_after_connected_since_are_taken() {
        let server = sv08();
        let db = db_with_connection(&server.address(), 1_788_970_000.0);
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        assert_eq!(sync_once(&db, &*test_maker(), 2e9).unwrap(), 2);
        assert_eq!(sync_once(&db, &*test_maker(), 2e9).unwrap(), 0);
        let conn = db.lock().unwrap();
        assert_eq!(list_open_jobs(&conn).unwrap().len(), 2);
        let c = get_connection(&conn, 1).unwrap().unwrap();
        assert_eq!(c.last_synced_at, Some(2e9));
        assert_eq!(c.remote_version.as_deref(), Some("v0.8.0-209-g4235789-dirty"));
    }

    /// Qidi Smart 3 with its clock in 2023: prints after connecting are taken over,
    /// with local end times, and the next pass finds nothing new.
    #[test]
    fn prints_of_a_printer_with_a_wrong_clock_are_taken() {
        let offset = 1_702_314_000.0 - unix_now();
        let server = crate::printer_link::fake_moonraker::qidi_smart3(offset);
        // Connected at 12:50 printer time.
        let db = db_with_connection(&server.address(), 1_702_299_000.0 - offset);
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        assert_eq!(sync_once(&db, &*test_maker(), unix_now()).unwrap(), 4);
        assert_eq!(sync_once(&db, &*test_maker(), unix_now()).unwrap(), 0);
        let jobs = list_open_jobs(&db.lock().unwrap()).unwrap();
        assert!(jobs.iter().all(|j| j.ended_at > unix_now() - 6.0 * 3600.0), "end times are local, not 2023");
        let c = get_connection(&db.lock().unwrap(), 1).unwrap().unwrap();
        assert!((c.clock_offset_s - offset).abs() < 3.0, "offset is stored for the UI note");
    }

    #[test]
    fn errors_are_recorded_per_printer() {
        let server = FakeServer::start(|_| Some((401, b"{}".to_vec())));
        let db = db_with_connection(&server.address(), 0.0);
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        assert_eq!(sync_once(&db, &*test_maker(), 5.0).unwrap(), 0);
        let c = get_connection(&db.lock().unwrap(), 1).unwrap().unwrap();
        assert_eq!(c.last_error.as_deref(), Some("auth_required"));
        assert!(c.paused);
        // a paused connection is no longer queried
        let before = server.requests().len();
        sync_once(&db, &*test_maker(), 6.0).unwrap();
        assert_eq!(server.requests().len(), before);
    }

    /// A printer with the login on: it answers only to its own key.
    fn locked_server(expected: &'static str) -> FakeServer {
        FakeServer::start_replying(move |request| {
            if request.header("x-api-key") != Some(expected) {
                return Reply::Answer(401, b"{}".to_vec());
            }
            let path = request.target.split('?').next().unwrap_or("");
            match path {
                "/server/info" => Reply::Answer(200, crate::printer_link::fake_moonraker::fixture_bytes("server_info_v0_8_0_209.json")),
                "/server/history/list" => Reply::Answer(200, crate::printer_link::fake_moonraker::fixture_bytes("history_completed.json")),
                _ => Reply::Answer(404, b"{}".to_vec()),
            }
        })
    }

    #[test]
    fn the_stored_key_is_used_by_the_pass() {
        const KEY: &str = "0123456789abcdef0123456789abcdef";
        let server = locked_server(KEY);
        let db = db_with_connection(&server.address(), 0.0);
        {
            let conn = db.lock().unwrap();
            save_connection_after_test(&conn, 1, "moonraker", &server.address(), Some(&ApiKey::parse(KEY).unwrap()), "", "", 0.0).unwrap();
            set_printer_link_enabled(&conn, true).unwrap();
        }
        sync_once(&db, &*test_maker(), 1e10).unwrap();
        let c = get_connection(&db.lock().unwrap(), 1).unwrap().unwrap();
        assert_eq!((c.last_error, c.paused), (None, false), "the key did not reach the printer");
        assert!(c.last_synced_at.is_some());
    }

    #[test]
    fn a_wrong_stored_key_ends_in_a_paused_connection_with_its_own_error() {
        let server = locked_server("0123456789abcdef0123456789abcdef");
        let db = db_with_connection(&server.address(), 0.0);
        {
            let conn = db.lock().unwrap();
            let wrong = ApiKey::parse("ffffffffffffffffffffffffffffffff").unwrap();
            save_connection_after_test(&conn, 1, "moonraker", &server.address(), Some(&wrong), "", "", 0.0).unwrap();
            set_printer_link_enabled(&conn, true).unwrap();
        }
        sync_once(&db, &*test_maker(), 5.0).unwrap();
        let c = get_connection(&db.lock().unwrap(), 1).unwrap().unwrap();
        assert_eq!(c.last_error.as_deref(), Some("auth_rejected"));
        assert!(c.paused);
        let before = server.requests().len();
        sync_once(&db, &*test_maker(), 6.0).unwrap();
        assert_eq!(server.requests().len(), before, "a paused connection is not asked again");
    }

    #[test]
    fn a_running_print_is_taken_once_it_has_ended() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let ended = Arc::new(AtomicBool::new(false));
        let flag = ended.clone();
        let server = FakeServer::start(move |target| {
            if target.starts_with("/server/info") {
                return Some((200, crate::printer_link::fake_moonraker::fixture_bytes("server_info_v0_8_0_209.json")));
            }
            let job = if flag.load(Ordering::SeqCst) {
                r#"{"end_time": 1000.0, "filament_used": 50.0, "filename": "a.gcode", "metadata": {}, "print_duration": 9.0, "status": "completed", "start_time": 900.0, "job_id": "A"}"#
            } else {
                r#"{"end_time": null, "filament_used": 20.0, "filename": "a.gcode", "metadata": {}, "print_duration": 5.0, "status": "in_progress", "start_time": 900.0, "job_id": "A"}"#
            };
            Some((200, format!(r#"{{"result": {{"count": 1, "jobs": [{job}]}}}}"#).into_bytes()))
        });
        let db = db_with_connection(&server.address(), 950.0);
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        assert_eq!(sync_once(&db, &*test_maker(), 960.0).unwrap(), 0);
        ended.store(true, Ordering::SeqCst);
        assert_eq!(sync_once(&db, &*test_maker(), 1100.0).unwrap(), 1);
    }

    /// A printer whose history holds, besides a normal print after the connection, a cancelled
    /// print without usage and two older prints. Answers both orders.
    fn history_with_unused_and_older_prints() -> FakeServer {
        FakeServer::start(|target| {
            if target.starts_with("/server/info") {
                return Some((200, crate::printer_link::fake_moonraker::fixture_bytes("server_info_v0_8_0_209.json")));
            }
            let job = |id: &str, status: &str, used: f64, end: f64| {
                format!(r#"{{"job_id": "{id}", "status": "{status}", "filament_used": {used}, "end_time": {end}, "start_time": {}, "filename": "{id}.gcode", "metadata": {{}}, "print_duration": 5.0}}"#, end - 60.0)
            };
            let jobs = if target.contains("before=") {
                [job("old2", "completed", 100.0, 900.0), job("old1", "cancelled", 0.0, 800.0)]
            } else {
                [job("none", "cancelled", 0.0, 1100.0), job("used", "completed", 50.0, 1200.0)]
            };
            Some((200, format!(r#"{{"result": {{"count": 2, "jobs": [{}, {}]}}}}"#, jobs[0], jobs[1]).into_bytes()))
        })
    }

    #[test]
    fn a_pass_records_a_print_without_usage_as_history_and_offers_the_other() {
        let server = history_with_unused_and_older_prints();
        let db = db_with_connection(&server.address(), 1000.0);
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        assert_eq!(sync_once(&db, &*test_maker(), 1300.0).unwrap(), 2);
        let conn = db.lock().unwrap();
        let rows: Vec<(String, String, bool)> = conn
            .prepare("SELECT file_name, state, auto_closed FROM printer_jobs ORDER BY ended_at")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, [("none.gcode".into(), "ignored".into(), true), ("used.gcode".into(), "open".into(), false)]);
        assert_eq!(list_open_jobs(&conn).unwrap().len(), 1);
        drop(conn);
        // A second pass finds both again and adds nothing.
        assert_eq!(sync_once(&db, &*test_maker(), 1400.0).unwrap(), 0);
        let total: i64 = db.lock().unwrap().query_row("SELECT COUNT(*) FROM printer_jobs", [], |r| r.get(0)).unwrap();
        assert_eq!(total, 2);
    }

    #[test]
    fn older_history_is_taken_as_history_only_and_asked_once_per_press() {
        let server = history_with_unused_and_older_prints();
        let db = db_with_connection(&server.address(), 1000.0);
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        assert_eq!(include_older_history(&db, &*test_maker(), 1).unwrap(), Ok(2));
        let conn = db.lock().unwrap();
        let rows: Vec<(String, String, bool, f64)> = conn
            .prepare("SELECT file_name, state, auto_closed, used_mm FROM printer_jobs ORDER BY ended_at")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, [("old1.gcode".into(), "ignored".into(), true, 0.0), ("old2.gcode".into(), "ignored".into(), true, 100.0)]);
        assert!(list_open_jobs(&conn).unwrap().is_empty(), "nothing of it is offered");
        assert_eq!(store::sync_from(&conn, 1, 2000.0).unwrap(), 1000.0, "the cursor of the normal pass stays at the connection");
        drop(conn);
        let asked = server.requests().iter().filter(|r| r.contains("before=")).count();
        assert_eq!(asked, 1);
        // Pressing again finds the same prints and adds nothing.
        assert_eq!(include_older_history(&db, &*test_maker(), 1).unwrap(), Ok(0));
        // The normal pass afterwards does not turn them into prints to confirm.
        assert_eq!(sync_once(&db, &*test_maker(), 1300.0).unwrap(), 2);
        assert_eq!(list_open_jobs(&db.lock().unwrap()).unwrap().len(), 1);
    }

    #[test]
    fn older_history_needs_the_switch_and_a_connection_and_reports_what_the_printer_said() {
        let server = history_with_unused_and_older_prints();
        let db = db_with_connection(&server.address(), 1000.0);
        assert_eq!(include_older_history(&db, &*test_maker(), 1).unwrap(), Err("disabled"));
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        assert_eq!(include_older_history(&db, &*test_maker(), 99).unwrap(), Err("no_connection"));
        assert!(server.requests().is_empty(), "the printer is not asked in either case");
        // A printer that is not there ends in its error code and writes nothing.
        let gone = db_with_connection("127.0.0.1:1", 1000.0);
        set_printer_link_enabled(&gone.lock().unwrap(), true).unwrap();
        assert_eq!(include_older_history(&gone, &*test_maker(), 1).unwrap(), Err("connection_refused"));
        let rows: i64 = gone.lock().unwrap().query_row("SELECT COUNT(*) FROM printer_jobs", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
    }

    /// `test()` of the first printer turns the connection off midway. The second
    /// printer must then not be contacted - the switch is checked again per printer
    /// during the pass, not just once when the plan is made.
    struct SwitchOffOnTest {
        db: Arc<Mutex<Connection>>,
    }

    impl PrinterLink for SwitchOffOnTest {
        fn test(&self) -> Result<ConnectionInfo, LinkError> {
            set_printer_link_enabled(&self.db.lock().unwrap(), false).unwrap();
            Ok(ConnectionInfo { version: "v0".into(), base_url: "http://printer-1".into(), clock_offset_s: 0.0 })
        }
        fn jobs_ended_since(&self, _info: &ConnectionInfo, _since: f64) -> Result<Vec<RemoteJob>, LinkError> {
            Ok(Vec::new())
        }
        fn thumbnail(&self, _base_url: &str, _path: &str) -> Result<Vec<u8>, LinkError> {
            Err(LinkError::Unreachable)
        }
    }

    #[test]
    fn switch_off_mid_pass_stops_the_next_printer_from_being_contacted() {
        let server2 = sv08();
        let conn = crate::db::connect_in_memory().unwrap();
        conn.execute("INSERT INTO printers (id, name, position) VALUES (1, 'A', 0)", []).unwrap();
        conn.execute("INSERT INTO printers (id, name, position) VALUES (2, 'B', 1)", []).unwrap();
        save_connection_after_test(&conn, 1, "moonraker", "printer-1", None, "", "", 0.0).unwrap();
        save_connection_after_test(&conn, 2, "moonraker", &server2.address(), None, "", "", 0.0).unwrap();
        set_printer_link_enabled(&conn, true).unwrap();
        let db = Arc::new(Mutex::new(conn));
        let maker_db = db.clone();

        let maker: Box<LinkMaker> = Box::new(move |kind: &str, address: &str, key: Option<&ApiKey>| {
            if address == "printer-1" {
                Ok(Box::new(SwitchOffOnTest { db: maker_db.clone() }) as Box<dyn PrinterLink>)
            } else {
                crate::printer_link::make_link(kind, address, key, AddressPolicy::TEST)
            }
        });

        // One printer at a time: with several side by side the second would already be under way.
        let one_at_a_time = SyncOptions { max_parallel: 1, ..SyncOptions::default() };
        assert_eq!(sync_with(&db, &*maker, 2e9, &one_at_a_time).unwrap(), 0);
        assert!(server2.requests().is_empty());
    }

    /// The connection disappears during the network request (e.g. because the user
    /// removed the printer meanwhile). The returning print must then not be written.
    struct DeleteSelfOnTest {
        db: Arc<Mutex<Connection>>,
        printer_id: i64,
    }

    impl PrinterLink for DeleteSelfOnTest {
        fn test(&self) -> Result<ConnectionInfo, LinkError> {
            store::delete_connection(&self.db.lock().unwrap(), self.printer_id).unwrap();
            Ok(ConnectionInfo { version: "v0".into(), base_url: "http://printer-1".into(), clock_offset_s: 0.0 })
        }
        fn jobs_ended_since(&self, _info: &ConnectionInfo, _since: f64) -> Result<Vec<RemoteJob>, LinkError> {
            Ok(vec![RemoteJob {
                remote_id: "A".into(),
                file_name: "a.gcode".into(),
                outcome: JobOutcome::Completed,
                raw_status: "completed".into(),
                ended_at: 1000.0,
                print_duration_s: 9.0,
                used_mm: 50.0,
                deductible: true,
                slicer_total_mm: None,
                slicer_weight_g: None,
                material: None,
                thumbnail_path: None,
            }])
        }
        fn thumbnail(&self, _base_url: &str, _path: &str) -> Result<Vec<u8>, LinkError> {
            Err(LinkError::Unreachable)
        }
    }

    #[test]
    fn connection_removed_during_the_network_call_is_not_written_to() {
        let conn = crate::db::connect_in_memory().unwrap();
        conn.execute("INSERT INTO printers (id, name, position) VALUES (1, 'A', 0)", []).unwrap();
        save_connection_after_test(&conn, 1, "moonraker", "printer-1", None, "", "", 0.0).unwrap();
        set_printer_link_enabled(&conn, true).unwrap();
        let db = Arc::new(Mutex::new(conn));
        let maker_db = db.clone();

        let maker: Box<LinkMaker> = Box::new(move |_kind: &str, _address: &str, _key: Option<&ApiKey>| {
            Ok(Box::new(DeleteSelfOnTest { db: maker_db.clone(), printer_id: 1 }) as Box<dyn PrinterLink>)
        });

        assert_eq!(sync_once(&db, &*maker, 2e9).unwrap(), 0);
        assert!(get_connection(&db.lock().unwrap(), 1).unwrap().is_none());
        assert_eq!(list_open_jobs(&db.lock().unwrap()).unwrap().len(), 0);
    }

    // ---- connection foundation: retries, counter, back-off, side-by-side pass ----

    fn quick() -> SyncOptions {
        SyncOptions { retry_pauses: vec![Duration::from_millis(5), Duration::from_millis(5)], ..SyncOptions::default() }
    }

    fn enabled_db(address: &str) -> Mutex<Connection> {
        let db = db_with_connection(address, 0.0);
        set_printer_link_enabled(&db.lock().unwrap(), true).unwrap();
        db
    }

    fn info_script(info: Vec<Reply>) -> FakeServer {
        use crate::printer_link::fake_moonraker::{fixture_bytes, Script};
        let script = Script::new(vec![
            ("/server/info", info),
            ("/server/history/list", vec![Reply::Answer(200, fixture_bytes("history_completed.json"))]),
        ]);
        FakeServer::start_replying(move |request| script.reply(request))
    }

    fn info_requests(server: &FakeServer) -> usize {
        server.requests().iter().filter(|r| r.starts_with("/server/info")).count()
    }

    #[test]
    fn a_passing_failure_is_repeated_and_succeeds_in_the_same_pass() {
        let ok = Reply::Answer(200, crate::printer_link::fake_moonraker::fixture_bytes("server_info_v0_8_0_209.json"));
        let server = info_script(vec![Reply::Answer(503, b"{}".to_vec()), Reply::Answer(503, b"{}".to_vec()), ok]);
        let db = enabled_db(&server.address());
        sync_with(&db, &*test_maker(), 10.0, &quick()).unwrap();
        // Two failed tries and the third one: no pass is lost to a restart of Moonraker.
        assert_eq!(info_requests(&server), 3);
        let c = get_connection(&db.lock().unwrap(), 1).unwrap().unwrap();
        assert_eq!((c.last_error, c.failures), (None, 0));
        assert_eq!(c.last_synced_at, Some(10.0));
    }

    #[test]
    fn repetitions_stop_after_two_and_the_failure_is_counted_once_per_pass() {
        let server = info_script(vec![Reply::Answer(503, b"{}".to_vec())]);
        let db = enabled_db(&server.address());
        sync_with(&db, &*test_maker(), 10.0, &quick()).unwrap();
        assert_eq!(info_requests(&server), 3, "the first try and two repetitions");
        let c = get_connection(&db.lock().unwrap(), 1).unwrap().unwrap();
        assert_eq!((c.last_error.as_deref(), c.failures, c.last_attempt_at), (Some("server_busy"), 1, Some(10.0)));
    }

    #[test]
    fn failures_that_need_the_user_are_never_repeated() {
        for (error, expected) in [
            (LinkError::AuthRequired, "auth_required"),
            (LinkError::AuthRejected, "auth_rejected"),
            (LinkError::HistoryMissing, "history_missing"),
            (LinkError::AddressNotAllowed, "address_not_allowed"),
            (LinkError::BadResponse("x".into()), "bad_response"),
        ] {
            let db = enabled_db("printer");
            let tries = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let counted = tries.clone();
            let failing = error.clone();
            let make: Box<LinkMaker> = Box::new(move |_, _, _| {
                counted.fetch_add(1, Ordering::SeqCst);
                Err(failing.clone())
            });
            sync_with(&db, &*make, 10.0, &quick()).unwrap();
            assert_eq!(tries.load(Ordering::SeqCst), 1, "{expected}");
            assert_eq!(get_connection(&db.lock().unwrap(), 1).unwrap().unwrap().last_error.as_deref(), Some(expected));
        }
    }

    #[test]
    fn the_counter_grows_per_pass_and_a_success_resets_it() {
        let server = info_script(vec![Reply::Close]);
        let db = enabled_db(&server.address());
        let force = SyncOptions { force: true, ..quick() };
        for pass in 1..=3 {
            sync_with(&db, &*test_maker(), 10.0 * pass as f64, &force).unwrap();
            let c = get_connection(&db.lock().unwrap(), 1).unwrap().unwrap();
            assert_eq!((c.failures, c.last_error.as_deref()), (pass, Some("connection_reset")));
            assert_eq!(c.error_since, Some(10.0), "the first failure stays the start of the outage");
        }
        let healthy = sv08();
        db.lock().unwrap().execute("UPDATE printer_connections SET address = ?1", [healthy.address()]).unwrap();
        sync_with(&db, &*test_maker(), 100.0, &force).unwrap();
        let c = get_connection(&db.lock().unwrap(), 1).unwrap().unwrap();
        assert_eq!((c.failures, c.last_error, c.error_since), (0, None, None));
    }

    #[test]
    fn a_printer_that_stays_away_is_asked_less_and_less_often() {
        let server = info_script(vec![Reply::Answer(503, b"{}".to_vec())]);
        let db = enabled_db(&server.address());
        let options = SyncOptions { retry_pauses: vec![], ..SyncOptions::default() };
        let asked = |now: f64, force: bool| {
            let before = info_requests(&server);
            sync_with(&db, &*test_maker(), now, &SyncOptions { force, ..options.clone() }).unwrap();
            info_requests(&server) > before
        };
        assert!(asked(0.0, false), "first failure");
        assert!(asked(300.0, false), "second failure: still every pass");
        // Two failures are counted, now 10 minutes between the attempts.
        assert!(!asked(300.0 + 599.0, false));
        assert!(asked(300.0 + 600.0, false), "third failure");
        assert!(!asked(900.0 + 599.0, false));
        assert!(asked(900.0 + 600.0, false), "fourth failure");
        // From four failures on: 30 minutes.
        assert!(!asked(1500.0 + 1799.0, false));
        assert!(asked(1500.0 + 1800.0, false));
        // Connect now does not wait.
        assert!(asked(3300.0 + 1.0, true));
    }

    #[test]
    fn a_failure_that_needs_the_user_is_asked_again_every_pass() {
        let server = FakeServer::start(|_| Some((200, b"{}".to_vec())));
        let db = enabled_db(&server.address());
        let options = SyncOptions { retry_pauses: vec![], ..SyncOptions::default() };
        for pass in 0..6 {
            let before = server.requests().len();
            sync_with(&db, &*test_maker(), 300.0 * pass as f64, &options).unwrap();
            assert!(server.requests().len() > before, "pass {pass}");
        }
        assert_eq!(get_connection(&db.lock().unwrap(), 1).unwrap().unwrap().last_error.as_deref(), Some("bad_response"));
    }

    /// A link that records how many run at the same time and waits for the others.
    struct MeetingLink {
        gate: std::sync::Arc<(Mutex<usize>, std::sync::Condvar)>,
        peak: std::sync::Arc<AtomicUsize>,
        wait_for: usize,
    }

    impl PrinterLink for MeetingLink {
        fn test(&self) -> Result<ConnectionInfo, LinkError> {
            let (count, changed) = &*self.gate;
            let mut inside = count.lock().unwrap();
            *inside += 1;
            self.peak.fetch_max(*inside, Ordering::SeqCst);
            changed.notify_all();
            // Waits for the others, but not for ever: a pass that runs one after the other must still end.
            let (mut guard, _) = changed
                .wait_timeout_while(inside, Duration::from_millis(1500), |n| *n < self.wait_for)
                .unwrap();
            *guard -= 1;
            drop(guard);
            changed.notify_all();
            Err(LinkError::AuthRequired)
        }
        fn jobs_ended_since(&self, _info: &ConnectionInfo, _since: f64) -> Result<Vec<RemoteJob>, LinkError> {
            Ok(Vec::new())
        }
        fn thumbnail(&self, _base_url: &str, _path: &str) -> Result<Vec<u8>, LinkError> {
            Err(LinkError::Unreachable)
        }
    }

    fn db_with_printers(count: i64) -> Mutex<Connection> {
        let conn = crate::db::connect_in_memory().unwrap();
        for id in 1..=count {
            conn.execute("INSERT INTO printers (id, name, position) VALUES (?1, ?2, ?1)", rusqlite::params![id, format!("P{id}")]).unwrap();
            save_connection_after_test(&conn, id, "moonraker", &format!("printer-{id}"), None, "", "", 0.0).unwrap();
        }
        set_printer_link_enabled(&conn, true).unwrap();
        Mutex::new(conn)
    }

    fn meeting_pass(printers: i64, max_parallel: usize) -> usize {
        let db = db_with_printers(printers);
        let gate = std::sync::Arc::new((Mutex::new(0usize), std::sync::Condvar::new()));
        let peak = std::sync::Arc::new(AtomicUsize::new(0));
        let (g, p) = (gate.clone(), peak.clone());
        let make: Box<LinkMaker> = Box::new(move |_, _, _| {
            Ok(Box::new(MeetingLink { gate: g.clone(), peak: p.clone(), wait_for: printers.min(max_parallel as i64) as usize }) as Box<dyn PrinterLink>)
        });
        sync_with(&db, &*make, 10.0, &SyncOptions { max_parallel, ..quick() }).unwrap();
        // Every printer got its own result.
        let conn = db.lock().unwrap();
        for id in 1..=printers {
            assert_eq!(get_connection(&conn, id).unwrap().unwrap().last_error.as_deref(), Some("auth_required"));
        }
        peak.load(Ordering::SeqCst)
    }

    #[test]
    fn printers_are_asked_side_by_side() {
        assert_eq!(meeting_pass(3, MAX_PARALLEL), 3, "all three were inside at the same time");
    }

    #[test]
    fn no_more_than_four_printers_are_asked_at_once() {
        assert_eq!(meeting_pass(6, MAX_PARALLEL), 4);
    }

    #[test]
    fn one_at_a_time_when_asked_to() {
        assert_eq!(meeting_pass(3, 1), 1);
    }

    #[test]
    fn a_slow_printer_does_not_hold_up_the_others() {
        let slow = FakeServer::start(|_| None);
        let fast = sv08();
        let conn = crate::db::connect_in_memory().unwrap();
        for (id, address) in [(1, slow.address()), (2, fast.address())] {
            conn.execute("INSERT INTO printers (id, name, position) VALUES (?1, ?2, ?1)", rusqlite::params![id, format!("P{id}")]).unwrap();
            save_connection_after_test(&conn, id, "moonraker", &address, None, "", "", 1_788_970_000.0).unwrap();
        }
        set_printer_link_enabled(&conn, true).unwrap();
        let db = Mutex::new(conn);
        // The slow printer needs its whole time limit; the fast one is done long before.
        let make: Box<LinkMaker> = Box::new(|_kind: &str, address: &str, _key: Option<&ApiKey>| {
            let link = crate::printer_link::moonraker::MoonrakerLink::new_with_timeout(address, AddressPolicy::TEST, Duration::from_millis(700));
            Ok(Box::new(link) as Box<dyn PrinterLink>)
        });
        let fast_done = std::sync::Arc::new(Mutex::new(None::<Duration>));
        let started = Instant::now();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                // Watches the fast printer's result appear while the slow one is still running.
                while started.elapsed() < Duration::from_secs(3) {
                    if get_connection(&db.lock().unwrap(), 2).unwrap().unwrap().last_synced_at.is_some() {
                        *fast_done.lock().unwrap() = Some(started.elapsed());
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            });
            sync_with(&db, &*make, 2e9, &SyncOptions { retry_pauses: vec![], ..SyncOptions::default() }).unwrap();
        });
        let total = started.elapsed();
        let fast_at = fast_done.lock().unwrap().expect("the fast printer finished");
        assert!(fast_at < Duration::from_millis(600), "fast printer done after {fast_at:?}");
        assert!(total >= Duration::from_millis(600), "the pass still waited for the slow one: {total:?}");
        let conn = db.lock().unwrap();
        assert_eq!(get_connection(&conn, 1).unwrap().unwrap().last_error.as_deref(), Some("timeout"));
        assert_eq!(list_open_jobs(&conn).unwrap().len(), 2);
    }

    /// The database lock is free while a printer is being asked.
    struct LockProbe {
        db: std::sync::Arc<Mutex<Connection>>,
        free: std::sync::Arc<AtomicBool>,
    }

    impl PrinterLink for LockProbe {
        fn test(&self) -> Result<ConnectionInfo, LinkError> {
            self.free.store(self.db.try_lock().is_ok(), Ordering::SeqCst);
            Err(LinkError::AuthRequired)
        }
        fn jobs_ended_since(&self, _info: &ConnectionInfo, _since: f64) -> Result<Vec<RemoteJob>, LinkError> {
            Ok(Vec::new())
        }
        fn thumbnail(&self, _base_url: &str, _path: &str) -> Result<Vec<u8>, LinkError> {
            Err(LinkError::Unreachable)
        }
    }

    #[test]
    fn the_database_is_not_locked_while_a_printer_is_asked() {
        let db = std::sync::Arc::new(db_with_printers(2));
        let free = std::sync::Arc::new(AtomicBool::new(false));
        let (d, f) = (db.clone(), free.clone());
        let make: Box<LinkMaker> = Box::new(move |_, _, _| Ok(Box::new(LockProbe { db: d.clone(), free: f.clone() }) as Box<dyn PrinterLink>));
        sync_with(&db, &*make, 10.0, &quick()).unwrap();
        assert!(free.load(Ordering::SeqCst));
    }

    /// Remembers the deadline the pass hands over.
    struct DeadlineProbe {
        seen: std::sync::Arc<Mutex<Option<Instant>>>,
    }

    impl PrinterLink for DeadlineProbe {
        fn test(&self) -> Result<ConnectionInfo, LinkError> {
            Err(LinkError::AuthRequired)
        }
        fn jobs_ended_since(&self, _info: &ConnectionInfo, _since: f64) -> Result<Vec<RemoteJob>, LinkError> {
            Ok(Vec::new())
        }
        fn thumbnail(&self, _base_url: &str, _path: &str) -> Result<Vec<u8>, LinkError> {
            Err(LinkError::Unreachable)
        }
        fn set_deadline(&mut self, deadline: Instant) {
            *self.seen.lock().unwrap() = Some(deadline);
        }
    }

    #[test]
    fn every_printer_gets_a_deadline_of_its_budget() {
        let db = db_with_printers(1);
        let seen = std::sync::Arc::new(Mutex::new(None));
        let s = seen.clone();
        let make: Box<LinkMaker> = Box::new(move |_, _, _| Ok(Box::new(DeadlineProbe { seen: s.clone() }) as Box<dyn PrinterLink>));
        let before = Instant::now();
        sync_with(&db, &*make, 10.0, &SyncOptions { printer_budget: Duration::from_secs(30), ..quick() }).unwrap();
        let deadline = seen.lock().unwrap().expect("a deadline was handed over");
        assert!(deadline >= before + Duration::from_secs(30) && deadline <= Instant::now() + Duration::from_secs(30));
    }

    #[test]
    fn switching_off_during_a_repetition_pause_ends_the_pass() {
        let db = std::sync::Arc::new(db_with_printers(1));
        let tries = std::sync::Arc::new(AtomicUsize::new(0));
        let (d, t) = (db.clone(), tries.clone());
        let make: Box<LinkMaker> = Box::new(move |_, _, _| {
            // The first try fails and switches the connection off while the pass waits to repeat it.
            if t.fetch_add(1, Ordering::SeqCst) == 0 {
                set_printer_link_enabled(&d.lock().unwrap(), false).unwrap();
            }
            Err(LinkError::Timeout)
        });
        sync_with(&db, &*make, 10.0, &quick()).unwrap();
        assert_eq!(tries.load(Ordering::SeqCst), 1, "no packet after the switch went off");
    }

    #[test]
    fn a_pass_at_the_edge_of_the_budget_does_not_repeat() {
        let db = db_with_printers(1);
        let tries = std::sync::Arc::new(AtomicUsize::new(0));
        let t = tries.clone();
        let make: Box<LinkMaker> = Box::new(move |_, _, _| {
            t.fetch_add(1, Ordering::SeqCst);
            Err(LinkError::Timeout)
        });
        let options = SyncOptions { printer_budget: Duration::from_millis(50), retry_pauses: vec![Duration::from_millis(500)], ..SyncOptions::default() };
        let started = Instant::now();
        sync_with(&db, &*make, 10.0, &options).unwrap();
        assert_eq!(tries.load(Ordering::SeqCst), 1, "the pause would end after the budget");
        assert!(started.elapsed() < Duration::from_millis(400));
    }
}
