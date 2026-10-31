use katalog_core::printer_link::sync::{sync_once, unix_now, SyncWaker, EVENT_JOBS_CHANGED};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

const FIRST_RUN_AFTER: Duration = Duration::from_secs(10);
const INTERVAL: Duration = Duration::from_secs(300);

pub fn spawn_background(app: tauri::AppHandle) -> SyncWaker {
    use tauri::{Emitter, Manager};
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let mut wait = FIRST_RUN_AFTER;
        while let Ok(()) | Err(RecvTimeoutError::Timeout) = rx.recv_timeout(wait) {
            while rx.try_recv().is_ok() {}
            let state = app.state::<crate::commands::AppState>();
            let maker = |kind: &str, address: &str| {
                crate::printer_link::make_link(kind, address, crate::printer_link::address::AddressPolicy::HOME_NETWORK)
            };
            if let Err(e) = sync_once(&state.db, &maker, unix_now()) {
                log::error!(target: "drucker", "sync failed: {e}");
            }
            let _ = app.emit(EVENT_JOBS_CHANGED, ());
            wait = INTERVAL;
        }
    });
    SyncWaker(tx)
}

