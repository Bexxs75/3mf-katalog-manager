use katalog_core::printer_link::sync::{sync_with, unix_now, SyncOptions, SyncWaker, EVENT_JOBS_CHANGED};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

const FIRST_RUN_AFTER: Duration = Duration::from_secs(10);
const INTERVAL: Duration = Duration::from_secs(300);

pub fn spawn_background(app: tauri::AppHandle) -> SyncWaker {
    use tauri::{Emitter, Manager};
    let (tx, rx) = mpsc::channel::<bool>();
    std::thread::spawn(move || {
        let mut wait = FIRST_RUN_AFTER;
        loop {
            // A wake-up is "Connect now" when any of the merged requests was.
            let mut force = match rx.recv_timeout(wait) {
                Ok(force) => force,
                Err(RecvTimeoutError::Timeout) => false,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            while let Ok(more) = rx.try_recv() {
                force |= more;
            }
            let state = app.state::<crate::commands::AppState>();
            let maker = |kind: &str, address: &str, key: Option<&crate::printer_link::api_key::ApiKey>| {
                crate::printer_link::make_link(kind, address, key, crate::printer_link::address::AddressPolicy::HOME_NETWORK)
            };
            let options = SyncOptions { force, ..SyncOptions::default() };
            if let Err(e) = sync_with(&state.db, &maker, unix_now(), &options) {
                log::error!(target: "drucker", "sync failed: {e}");
            }
            let _ = app.emit(EVENT_JOBS_CHANGED, ());
            wait = INTERVAL;
        }
    });
    SyncWaker(tx)
}

