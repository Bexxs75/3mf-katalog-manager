//! Sync: fetches finished prints of all connected printers. The database is only
//! locked for short read/write steps, never during a network request.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use super::{LinkError, PrinterLink};
use crate::db::error::DbError;
use crate::db::printer_link as store;

pub const EVENT_JOBS_CHANGED: &str = "printer-jobs-changed";
const FIRST_RUN_AFTER: Duration = Duration::from_secs(10);
const INTERVAL: Duration = Duration::from_secs(300);

pub type LinkMaker = dyn Fn(&str, &str) -> Result<Box<dyn PrinterLink>, LinkError> + Send + Sync;

pub fn unix_now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn lock(db: &Mutex<Connection>) -> Result<std::sync::MutexGuard<'_, Connection>, DbError> {
    db.lock().map_err(|_| DbError::Other("database lock poisoned".into()))
}

/// One pass over all non-paused connections. Returns the number of newly taken over prints.
pub fn sync_once(db: &Mutex<Connection>, make: &LinkMaker, now: f64) -> Result<usize, DbError> {
    let plan: Vec<(i64, String, String, f64)> = {
        let conn = lock(db)?;
        if !store::printer_link_enabled(&conn)? {
            return Ok(0);
        }
        let mut plan = Vec::new();
        for c in store::list_connections(&conn)?.into_iter().filter(|c| !c.paused) {
            let since = store::sync_from(&conn, c.printer_id)?;
            plan.push((c.printer_id, c.kind, c.address, since));
        }
        plan
    };

    let mut new_jobs = 0;
    for (printer_id, kind, address, since) in plan {
        // The plan was made once at the start; a single network request (`test()`) can
        // take seconds. So check again briefly before contacting the next printer: if
        // the switch was turned off meanwhile, the whole pass ends immediately (no more
        // packets to any printer); if only this connection was removed or paused, only
        // it is skipped.
        {
            let conn = lock(db)?;
            if !store::printer_link_enabled(&conn)? {
                break;
            }
            match store::get_connection(&conn, printer_id)? {
                Some(c) if !c.paused => {}
                _ => continue,
            }
        }

        let result = make(&kind, &address).and_then(|link| {
            let info = link.test()?;
            let jobs = link.jobs_ended_since(&info, since)?;
            Ok((info, jobs))
        });

        let conn = lock(db)?;
        // The connection may have been removed during the network request; then the
        // result must not be written anymore.
        if store::get_connection(&conn, printer_id)?.is_none() {
            continue;
        }
        match result {
            Ok((info, jobs)) => {
                for job in &jobs {
                    if store::insert_job_if_new(&conn, printer_id, job)? {
                        new_jobs += 1;
                    }
                }
                store::record_sync_success(&conn, printer_id, now, &info.base_url, &info.version)?;
            }
            Err(e) => store::record_sync_error(&conn, printer_id, e.code(), now)?,
        }
    }
    Ok(new_jobs)
}

/// Wakes the background sync immediately (button, switch, new connection).
pub struct SyncWaker(pub mpsc::Sender<()>);

impl SyncWaker {
    pub fn wake(&self) {
        let _ = self.0.send(());
    }
}

pub fn spawn_background(app: tauri::AppHandle) -> SyncWaker {
    use tauri::{Emitter, Manager};
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let mut wait = FIRST_RUN_AFTER;
        while let Ok(()) | Err(RecvTimeoutError::Timeout) = rx.recv_timeout(wait) {
            while rx.try_recv().is_ok() {}
            let state = app.state::<crate::commands::AppState>();
            let maker = |kind: &str, address: &str| {
                super::make_link(kind, address, super::address::AddressPolicy::HOME_NETWORK)
            };
            if let Err(e) = sync_once(&state.db, &maker, unix_now()) {
                eprintln!("[printer_link] sync failed: {e}");
            }
            let _ = app.emit(EVENT_JOBS_CHANGED, ());
            wait = INTERVAL;
        }
    });
    SyncWaker(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::printer_link::{get_connection, list_open_jobs, save_connection_after_test, set_printer_link_enabled};
    use crate::printer_link::address::AddressPolicy;
    use crate::printer_link::fake_moonraker::{sv08, FakeServer};
    use crate::printer_link::{ConnectionInfo, JobOutcome, RemoteJob};
    use std::sync::{Arc, Mutex};

    fn db_with_connection(address: &str, connected_since: f64) -> Mutex<Connection> {
        let conn = crate::db::connect_in_memory().unwrap();
        conn.execute("INSERT INTO printers (id, name, position) VALUES (1, 'Sovol SV08', 0)", []).unwrap();
        save_connection_after_test(&conn, 1, "moonraker", address, "", "", connected_since).unwrap();
        Mutex::new(conn)
    }

    fn test_maker() -> Box<LinkMaker> {
        Box::new(|kind: &str, address: &str| crate::printer_link::make_link(kind, address, AddressPolicy::TEST))
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
        save_connection_after_test(&conn, 1, "moonraker", "printer-1", "", "", 0.0).unwrap();
        save_connection_after_test(&conn, 2, "moonraker", &server2.address(), "", "", 0.0).unwrap();
        set_printer_link_enabled(&conn, true).unwrap();
        let db = Arc::new(Mutex::new(conn));
        let maker_db = db.clone();

        let maker: Box<LinkMaker> = Box::new(move |kind: &str, address: &str| {
            if address == "printer-1" {
                Ok(Box::new(SwitchOffOnTest { db: maker_db.clone() }) as Box<dyn PrinterLink>)
            } else {
                crate::printer_link::make_link(kind, address, AddressPolicy::TEST)
            }
        });

        assert_eq!(sync_once(&db, &*maker, 2e9).unwrap(), 0);
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
        save_connection_after_test(&conn, 1, "moonraker", "printer-1", "", "", 0.0).unwrap();
        set_printer_link_enabled(&conn, true).unwrap();
        let db = Arc::new(Mutex::new(conn));
        let maker_db = db.clone();

        let maker: Box<LinkMaker> = Box::new(move |_kind: &str, _address: &str| {
            Ok(Box::new(DeleteSelfOnTest { db: maker_db.clone(), printer_id: 1 }) as Box<dyn PrinterLink>)
        });

        assert_eq!(sync_once(&db, &*maker, 2e9).unwrap(), 0);
        assert!(get_connection(&db.lock().unwrap(), 1).unwrap().is_none());
        assert_eq!(list_open_jobs(&db.lock().unwrap()).unwrap().len(), 0);
    }
}
