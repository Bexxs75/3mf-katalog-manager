//! Server transport for the shared printer logic. Permits outlive HTTP requests.
use super::*;
use base64::Engine;
use katalog_core::{
    db::{self, printer_link as store},
    printer_link::{
        address::{AddressAllowlist, AddressPolicy},
        api_key::ApiKey,
        booking, make_link,
        sync::{self, SyncOptions},
        views, LinkError,
    },
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

const BUDGET: Duration = Duration::from_secs(60);
const FIRST_RUN: Duration = Duration::from_secs(10);
const INTERVAL: Duration = Duration::from_secs(300);

pub(super) struct Runtime {
    network: Arc<Semaphore>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            network: Arc::new(Semaphore::new(1)),
        }
    }
}

pub(super) fn command(name: &str) -> bool {
    matches!(
        name,
        "get_printer_link_enabled"
            | "set_printer_link_enabled"
            | "list_printer_connections"
            | "test_printer_connection"
            | "clear_printer_api_key"
            | "remove_printer_connection"
            | "include_older_printer_history"
            | "sync_printers_now"
            | "list_open_printer_jobs"
            | "preview_printer_job"
            | "get_printer_job_thumbnail"
            | "ignore_printer_job"
            | "set_printer_job_failure"
            | "confirm_printer_jobs"
    )
}

fn policy(config: &Config) -> Result<AddressPolicy, LinkError> {
    use crate::config::PrinterAllowEntry;
    let entries = config
        .printer_allow
        .entries()
        .iter()
        .map(|entry| match entry {
            PrinterAllowEntry::Host(host) => host.clone(),
            PrinterAllowEntry::Network(net) => format!("{}/{}", net.address(), net.prefix()),
        });
    Ok(AddressPolicy::HOME_NETWORK.with_allowlist(AddressAllowlist::from_strings(entries)?))
}

fn disabled(app: &App) -> bool {
    app.config.printer_allow.entries().is_empty()
}

fn changed(app: &App) {
    app.events
        .lock()
        .unwrap()
        .publish(sync::EVENT_JOBS_CHANGED, Value::Null);
}

pub(super) fn start(app: &Arc<App>) {
    if disabled(app) || app.writer.is_none() {
        return;
    }
    // Startup has acquired the instance lock already; a sleeping service must not
    // keep the router or that lock alive after shutdown.
    let weak = Arc::downgrade(app);
    tokio::spawn(async move {
        let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + FIRST_RUN, INTERVAL);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            timer.tick().await;
            let Some(app) = weak.upgrade() else { break };
            let Ok(guard) = app.geometry.barrier.writer() else {
                continue;
            };
            let Ok(slot) = app.printers.network.clone().try_acquire_owned() else {
                continue;
            };
            let gate = app.writer.as_ref().unwrap();
            let Ok(worker) = gate.worker.clone().try_acquire_owned() else {
                continue;
            };
            let cancel = Arc::new(AtomicBool::new(false));
            let signal = cancel.clone();
            let barrier = app.geometry.barrier.clone();
            let mut work = tokio::task::spawn_blocking(move || {
                let (_guard, _slot, _worker) = (guard, slot, worker);
                let _ = run_sync(&app, false, cancel);
                changed(&app);
            });
            loop {
                tokio::select! {
                    _ = &mut work => break,
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {
                        if barrier.snapshot().state != crate::barrier::State::Open || weak.strong_count() <= 1 {
                            signal.store(true, Ordering::SeqCst);
                        }
                    }
                }
            }
        }
    });
}

fn run_sync(app: &App, force: bool, cancel: Arc<AtomicBool>) -> Result<Value, ()> {
    let db = Mutex::new(db::open_read_write(&app.config.db_path).map_err(|_| ())?);
    let policy = policy(&app.config).map_err(|_| ())?;
    let make = move |kind: &str, address: &str, key: Option<&ApiKey>| {
        make_link(kind, address, key, policy.clone())
    };
    let options = SyncOptions {
        force,
        cancel: Some(cancel.clone()),
        deadline: Some(Instant::now() + BUDGET),
        ..SyncOptions::default()
    };
    let finished = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !finished.load(Ordering::SeqCst) {
                if app.geometry.barrier.snapshot().state != crate::barrier::State::Open
                    || Instant::now() >= options.deadline.unwrap()
                {
                    cancel.store(true, Ordering::SeqCst);
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
        let result = sync::sync_with(&db, &make, sync::unix_now(), &options);
        finished.store(true, Ordering::SeqCst);
        result
    })
    .map_err(|_| ())?;
    Ok(Value::Null)
}

pub(super) async fn read(app: Arc<App>, command: String, args: Value) -> Response {
    // Admission comes before any work: each read opens a connection and may match
    // every open job against the model candidates, so concurrent callers must not
    // multiply that load beyond the limit shared with the other manager reads.
    let permit = match app.geometry.manager_read() {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let guard = match app.geometry.barrier.reader() {
        Ok(g) => g,
        Err(_) => return crate::barrier::maintenance(),
    };
    tokio::task::spawn_blocking(move || {
        // Both belong to the closure: a request that times out or is dropped does not
        // stop this work, so neither may be released before it finishes.
        let _held = (guard, permit);
        execute(&app, &command, &args)
    })
    .await
    .unwrap_or_else(|_| error_coded(500, ErrorCode::WebServerInternal, &[]))
}

fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str, ()> {
    args.get(key).and_then(Value::as_str).ok_or(())
}
fn optional_string<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>, ()> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        _ => Err(()),
    }
}
fn id(args: &Value, key: &str) -> Result<i64, ()> {
    string(args, key)?.parse().map_err(|_| ())
}
fn encode(value: impl serde::Serialize) -> Result<Value, ()> {
    serde_json::to_value(value).map_err(|_| ())
}
fn test_result(code: &str) -> Value {
    json!({"ok":false,"error":code,"connection":null})
}

pub(super) fn execute(app: &App, name: &str, args: &Value) -> Response {
    if disabled(app) {
        return error_coded(403, ErrorCode::PrinterAddressNotAllowed, &[]);
    }
    let network = matches!(
        name,
        "test_printer_connection"
            | "sync_printers_now"
            | "include_older_printer_history"
            | "get_printer_job_thumbnail"
    );
    let _slot = if network {
        match app.printers.network.clone().try_acquire_owned() {
            Ok(p) => Some(p),
            Err(_) => return crate::write::retry(503),
        }
    } else {
        None
    };
    let result = execute_inner(app, name, args);
    if crate::write::WRITE_COMMANDS.contains(&name) {
        changed(app);
    }
    match result {
        Ok(v) => Json(v).into_response(),
        // Never serialize or log database, key parser or remote response diagnostics.
        Err(()) => error_coded(422, ErrorCode::ReqInvalidArguments, &[]),
    }
}

fn execute_inner(app: &App, name: &str, args: &Value) -> Result<Value, ()> {
    if name == "sync_printers_now" {
        return run_sync(app, true, Arc::new(AtomicBool::new(false)));
    }
    let mut conn = if crate::write::WRITE_COMMANDS.contains(&name) {
        db::open_read_write(&app.config.db_path)
    } else {
        db::open_read_only(&app.config.db_path)
    }
    .map_err(|_| ())?;
    let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339();
    match name {
        "get_printer_link_enabled" => encode(store::printer_link_enabled(&conn).map_err(|_| ())?),
        "set_printer_link_enabled" => {
            store::set_printer_link_enabled(
                &conn,
                args.get("enabled").and_then(Value::as_bool).ok_or(())?,
            )
            .map_err(|_| ())?;
            Ok(Value::Null)
        }
        "list_printer_connections" => {
            encode(views::list_printer_connections(&conn).map_err(|_| ())?)
        }
        "list_open_printer_jobs" => encode(views::list_open_printer_jobs(&conn).map_err(|_| ())?),
        "preview_printer_job" => encode(
            views::preview_printer_job(&conn, string(args, "jobId")?, string(args, "spoolId")?)
                .map_err(|_| ())?,
        ),
        "clear_printer_api_key" => {
            store::clear_api_key(&conn, id(args, "printerId")?).map_err(|_| ())?;
            Ok(Value::Null)
        }
        "remove_printer_connection" => {
            store::delete_connection(&conn, id(args, "printerId")?).map_err(|_| ())?;
            Ok(Value::Null)
        }
        "ignore_printer_job" => {
            booking::ignore_job(&conn, id(args, "jobId")?, &now).map_err(|_| ())?;
            Ok(Value::Null)
        }
        "set_printer_job_failure" => {
            booking::set_job_failure(
                &conn,
                id(args, "jobId")?,
                optional_string(args, "reason")?,
                optional_string(args, "note")?,
            )
            .map_err(|_| ())?;
            Ok(Value::Null)
        }
        "confirm_printer_jobs" => {
            let decisions =
                serde_json::from_value(args.get("decisions").ok_or(())?.clone()).map_err(|_| ())?;
            encode(views::confirm_printer_jobs(&mut conn, decisions, &now).map_err(|_| ())?)
        }
        "include_older_printer_history" => {
            let policy = policy(&app.config).map_err(|_| ())?;
            let make = move |kind: &str, address: &str, key: Option<&ApiKey>| {
                make_link(kind, address, key, policy.clone())
            };
            match sync::include_older_history(&Mutex::new(conn), &make, id(args, "printerId")?)
                .map_err(|_| ())?
            {
                Ok(added) => Ok(json!({"added":added,"error":null})),
                Err(code) => Ok(json!({"added":0,"error":code})),
            }
        }
        "test_printer_connection" => test_connection(app, &conn, args),
        "get_printer_job_thumbnail" => {
            let Some((connection, path)) =
                views::thumbnail_fetch_plan(&conn, string(args, "jobId")?).map_err(|_| ())?
            else {
                return Ok(Value::Null);
            };
            let key = store::get_api_key(&conn, connection.printer_id).map_err(|_| ())?;
            drop(conn);
            let result = (|| {
                let mut link = make_link(
                    &connection.kind,
                    &connection.address,
                    key.as_ref(),
                    policy(&app.config)?,
                )?;
                link.set_deadline(Instant::now() + BUDGET);
                let info = link.test()?;
                link.thumbnail(&info.base_url, &path)
            })();
            Ok(result
                .ok()
                .map(|bytes| Value::String(base64::engine::general_purpose::STANDARD.encode(bytes)))
                .unwrap_or(Value::Null))
        }
        _ => Err(()),
    }
}

fn test_connection(app: &App, conn: &rusqlite::Connection, args: &Value) -> Result<Value, ()> {
    let pid = id(args, "printerId")?;
    if let Some(result) = views::test_printer_connection_gate(conn, pid).map_err(|_| ())? {
        return encode(result);
    }
    let address = string(args, "address")?.trim();
    let kind = string(args, "kind")?;
    let typed = match optional_string(args, "apiKey")?
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => match ApiKey::parse(raw) {
            Ok(key) => Some(key),
            Err(_) => return Ok(test_result("api_key_invalid")),
        },
        None => None,
    };
    let key = if typed.is_some() {
        typed.clone()
    } else if store::get_connection(conn, pid)
        .map_err(|_| ())?
        .is_some_and(|c| c.address == address)
    {
        store::get_api_key(conn, pid).map_err(|_| ())?
    } else {
        None
    };
    let result = (|| {
        let mut link = make_link(kind, address, key.as_ref(), policy(&app.config)?)?;
        link.set_deadline(Instant::now() + BUDGET);
        link.test()
    })();
    match result {
        Err(e) => Ok(test_result(e.code())),
        Ok(info) => {
            store::save_connection_after_test(
                conn,
                pid,
                kind,
                address,
                typed.as_ref(),
                &info.base_url,
                &info.version,
                sync::unix_now(),
            )
            .map_err(|_| ())?;
            store::set_clock_offset(conn, pid, info.clock_offset_s).map_err(|_| ())?;
            encode(views::TestResultDto {
                ok: true,
                error: None,
                connection: Some(views::connection_dto(
                    store::get_connection(conn, pid)
                        .map_err(|_| ())?
                        .ok_or(())?,
                )),
            })
        }
    }
}
