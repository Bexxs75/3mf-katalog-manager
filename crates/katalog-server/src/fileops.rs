//! File operations hold import admission and this runtime mutex inside one
//! non-cancellable blocking worker. DB callbacks and ledger locks never overlap.
use super::*;
use crate::fileops_safety::DestructiveKind;
use crate::fileops_startup::{DbAdapter, ServerLedger};
use katalog_core::fsops::{self, FileOps, TrashSnapshot};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
type Ops = FileOps<DbAdapter, ServerLedger>;
pub(super) struct Runtime {
    pub(super) state: Mutex<RuntimeState>,
    blocked: AtomicBool,
    op_ids: Mutex<Vec<String>>,
}
pub(super) struct RuntimeState {
    ops: Ops,
    snapshots: HashMap<String, Snapshot>,
}
struct Snapshot {
    session: String,
    created: u64,
    snapshot: TrashSnapshot,
    cutoff: Option<chrono::DateTime<chrono::FixedOffset>>,
}
impl Runtime {
    pub(super) fn new(ops: Ops) -> Self {
        Self {
            blocked: AtomicBool::new(ops.journal.is_blocked()),
            op_ids: Mutex::new(ops.journal.pending_op_ids()),
            state: Mutex::new(RuntimeState {
                ops,
                snapshots: HashMap::new(),
            }),
        }
    }
    fn blocked_response(&self) -> Response {
        (StatusCode::SERVICE_UNAVAILABLE,Json(json!({"kind":"restart_required","message":"restart_required","expected":false,"opIds":*self.op_ids.lock().unwrap()}))).into_response()
    }
    #[cfg(test)]
    pub(super) fn block_for_test(&self) {
        self.blocked.store(true, Ordering::Release);
    }
    pub(super) fn available(&self) -> bool {
        !self.blocked.load(Ordering::Acquire)
    }
}
pub(super) fn capability(app: &App) -> bool {
    app.config.file_ops && app.config.write && app.fileops.as_ref().is_some_and(|r| r.available())
}
pub(super) fn failure(status: u16, kind: &str) -> Response {
    (
        StatusCode::from_u16(status).unwrap(),
        Json(json!({"kind":kind,"message":kind,"expected":status<500})),
    )
        .into_response()
}
fn err(status: u16, kind: &str) -> Box<Response> {
    Box::new(failure(status, kind))
}
fn core_error(e: fsops::Error) -> Box<Response> {
    use fsops::Error::*;
    err(
        match e {
            InvalidName | InvalidArguments => 400,
            NotFound => 404,
            Protected | HasCompanions => 409,
            RestartRequired => 503,
            Internal => 500,
            NotStarted => 409,
            _ => 409,
        },
        &e.to_string(),
    )
}
pub(super) fn retry(seconds: u64) -> Box<Response> {
    let mut r = failure(429, "rate_limited");
    r.headers_mut()
        .insert(header::RETRY_AFTER, seconds.to_string().parse().unwrap());
    Box::new(r)
}
pub(super) fn budget_error(error: crate::fileops_safety::BudgetError) -> Box<Response> {
    match error {
        crate::fileops_safety::BudgetError::LimitExceeded => err(422, "limit_exceeded"),
        crate::fileops_safety::BudgetError::Retry(seconds) => retry(seconds),
    }
}
#[derive(Debug)]
enum Command {
    Rename(i64, String),
    Move(Vec<i64>, Option<i64>),
    Trash(Vec<i64>),
    Restore(i64),
    Permanent(i64, String),
    Start(usize, Option<String>, String),
    Block(String),
    Create(i64, String),
    RenameFolder(i64, String),
    MoveFolder(i64, Option<i64>),
    RemoveFolder(i64),
}
fn parse(command: &str, v: &Value) -> Result<Command, Box<Response>> {
    let obj = v.as_object().ok_or_else(|| err(400, "invalid_arguments"))?;
    let keys: &[&str] = match command {
        "rename_file" => &["fileId", "name"],
        "move_files_to_folder" => &["fileIds", "folderId"],
        "delete_files" => &["fileIds"],
        "restore_file" => &["fileId"],
        "delete_file_permanently" => &["fileId", "confirm", "password"],
        "empty_trash_start" => &["expectedCount", "maxDeletedAt", "password"],
        "empty_trash_block" => &["token"],
        "create_folder" => &["parentId", "name"],
        "rename_folder" => &["folderId", "name"],
        "move_folder" => &["folderId", "newParentId"],
        "remove_folder_from_catalog" => &["folderId"],
        _ => return Err(err(404, "unknown_command")),
    };
    if obj.keys().any(|k| !keys.contains(&k.as_str())) {
        return Err(err(422, "unknown_argument"));
    }
    let text = |key: &str| -> Result<String, Box<Response>> {
        v.get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| err(400, "invalid_arguments"))
    };
    let id = |key: &str| -> Result<i64, Box<Response>> {
        let s = text(key)?;
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err(400, "invalid_arguments"));
        }
        s.parse::<i64>()
            .ok()
            .filter(|i| *i > 0)
            .ok_or_else(|| err(400, "invalid_arguments"))
    };
    let nullable = |key: &str| -> Result<Option<i64>, Box<Response>> {
        if v.get(key) == Some(&Value::Null) {
            Ok(None)
        } else {
            id(key).map(Some)
        }
    };
    let ids = || -> Result<Vec<i64>, Box<Response>> {
        let values = v
            .get("fileIds")
            .and_then(Value::as_array)
            .ok_or_else(|| err(400, "invalid_arguments"))?;
        if values.is_empty() || values.len() > 200 {
            return Err(err(422, "limit_exceeded"));
        }
        let mut out = Vec::new();
        for value in values {
            let s = value
                .as_str()
                .ok_or_else(|| err(400, "invalid_arguments"))?;
            let n = s
                .parse::<i64>()
                .ok()
                .filter(|n| *n > 0 && s.bytes().all(|b| b.is_ascii_digit()))
                .ok_or_else(|| err(400, "invalid_arguments"))?;
            if !out.contains(&n) {
                out.push(n)
            }
        }
        Ok(out)
    };
    Ok(match command {
        "rename_file" => Command::Rename(id("fileId")?, text("name")?),
        "move_files_to_folder" => {
            if v.get("folderId") == Some(&Value::Null) { return Err(err(422, "invalid_arguments")); }
            Command::Move(ids()?, Some(id("folderId")?))
        },
        "delete_files" => Command::Trash(ids()?),
        "restore_file" => Command::Restore(id("fileId")?),
        "delete_file_permanently" => {
            if v.get("confirm") != Some(&Value::Bool(true)) {
                return Err(err(422, "confirmation_required"));
            }
            Command::Permanent(id("fileId")?, text("password")?)
        }
        "empty_trash_start" => Command::Start(
            v.get("expectedCount")
                .and_then(Value::as_u64)
                .and_then(|x| usize::try_from(x).ok())
                .ok_or_else(|| err(400, "invalid_arguments"))?,
            match v.get("maxDeletedAt") {
                Some(Value::Null) => None,
                Some(Value::String(s)) => Some(s.clone()),
                _ => return Err(err(400, "invalid_arguments")),
            },
            text("password")?,
        ),
        "empty_trash_block" => Command::Block(text("token")?),
        "create_folder" => Command::Create(id("parentId")?, text("name")?),
        "rename_folder" => Command::RenameFolder(id("folderId")?, text("name")?),
        "move_folder" => Command::MoveFolder(id("folderId")?, nullable("newParentId")?),
        "remove_folder_from_catalog" => Command::RemoveFolder(id("folderId")?),
        _ => unreachable!(),
    })
}
pub(super) fn valid_session(app: &App, current: &Session) -> bool {
    let mut data = app.data.lock().unwrap();
    cleanup(&mut data, app.clock.now());
    data.sessions.contains_key(&current.token) && data.auth.as_ref().is_some_and(|a| !a.must_change)
}
/// The deadline starts at the first operation check, i.e. after admission and
/// backups, so a slow backup cannot make the operation itself "not started".
/// SQLite hooks only read atomics and the clock, never the session mutex.
pub(super) fn operation_cancellation(app: &App, current: &Session) -> crate::fileops_cancel::Cancellation {
    let validity = current.validity.clone();
    let clock = app.clock.clone();
    deadline_cancellation(Duration::from_secs(20), move || validity.valid(clock.now()))
}
fn deadline_cancellation(
    limit: Duration,
    valid: impl Fn() -> bool + Send + Sync + 'static,
) -> crate::fileops_cancel::Cancellation {
    let started = std::sync::OnceLock::new();
    crate::fileops_cancel::Cancellation::new(move || {
        started.get_or_init(Instant::now).elapsed() < limit && valid()
    })
}
fn backup(app: &App, count: usize, event: bool) -> Result<(), Box<Response>> {
    let conn = app
        .writer
        .as_ref()
        .unwrap()
        .conn
        .lock()
        .map_err(|_| err(503, "restart_required"))?;
    app.fileops_safety
        .lock()
        .map_err(|_| err(503, "restart_required"))?
        .ensure_backups(&conn, &app.config.db_path, count, event, app.clock.now())
        .map_err(|_| err(503, "backup_failed"))
}
fn reserve(app: &App, kind: DestructiveKind, count: usize) -> Result<(), Box<Response>> {
    app.fileops_safety
        .lock()
        .map_err(|_| err(503, "restart_required"))?
        .reserve(kind, count, app.clock.now())
        .map_err(budget_error)
}
pub(super) async fn invoke(
    app: Arc<App>,
    current: Session,
    peer: String,
    command: String,
    args: Value,
    admission: tokio::sync::OwnedSemaphorePermit,
    progress: Arc<WriteProgress>,
) -> Response {
    if !app.config.file_ops {
        return failure(404, "fileops_disabled");
    }
    let Some(rt) = app.fileops.clone() else {
        return failure(503, "restart_required");
    };
    if !rt.available() {
        return rt.blocked_response();
    }
    let parsed = match parse(&command, &args) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    match &parsed {
        Command::Rename(id, _) | Command::Restore(id) | Command::Permanent(id, _) => {
            progress.set_file_ids(&[*id])
        }
        Command::Move(ids, _) | Command::Trash(ids) => progress.set_file_ids(ids),
        _ => (),
    }
    if let Command::Permanent(_, pw) | Command::Start(_, _, pw) = &parsed {
        let hash = app
            .data
            .lock()
            .unwrap()
            .auth
            .as_ref()
            .unwrap()
            .password_hash
            .clone();
        match verify_client(app.clone(), &peer, hash, pw.clone(), true).await {
            Ok(true) => (),
            Ok(false) => return failure(403, "password_invalid"),
            Err(r) => return *r,
        }
    }
    let worker_rt = rt.clone();
    tokio::task::spawn_blocking(move || {
        let rt = worker_rt;
        let _admission = admission;
        // Acquire import gate BEFORE runtime/fileops mutex. Never app.changes.
        let _import = if let Some(upload) = &app.uploads {
            let Ok(p) = upload.active.clone().try_acquire_owned() else {
                return crate::write::retry(503);
            };
            match upload.store.has_importing() {
                Ok(false) => Some(p),
                Ok(true) => return crate::write::retry(503),
                Err(_) => {
                    rt.blocked.store(true, Ordering::Release);
                    return rt.blocked_response();
                }
            }
        } else {
            None
        };
        if !valid_session(&app, &current) {
            return failure(401, "session_expired");
        }
        let mut state = match rt.state.try_lock() {
            Ok(s) => s,
            Err(std::sync::TryLockError::WouldBlock) => return crate::write::retry(503),
            Err(_) => {
                rt.blocked.store(true, Ordering::Release);
                return rt.blocked_response();
            }
        };
        if !rt.available() || state.ops.journal.is_blocked() {
            return rt.blocked_response();
        }
        let cancellation = operation_cancellation(&app, &current);
        let check = cancellation.clone();
        let allowed = move || check.allowed();
        let result = execute(
            &app,
            &current,
            &progress,
            parsed,
            &mut state,
            allowed,
            cancellation,
        );
        if state.ops.journal.is_blocked() {
            *rt.op_ids.lock().unwrap() = state.ops.journal.pending_op_ids();
            rt.blocked.store(true, Ordering::Release)
        }
        if let Some(upload) = &app.uploads {
            upload.changed_blocking();
        }
        match result {
            Ok(v) => {
                let completed = completed_items(&command, &v);
                progress.completed_items.store(completed, Ordering::Relaxed);
                Json(v).into_response()
            }
            Err(r) => *r,
        }
    })
    .await
    .unwrap_or_else(|_| {
        rt.blocked.store(true, Ordering::Release);
        failure(503, "restart_required")
    })
}
fn execute(
    app: &App,
    current: &Session,
    progress: &WriteProgress,
    parsed: Command,
    state: &mut RuntimeState,
    mut allowed: impl FnMut() -> bool,
    cancellation: crate::fileops_cancel::Cancellation,
) -> Result<Value, Box<Response>> {
    let now = app.clock.now();
    let ops = &mut state.ops;
    if matches!(&parsed, Command::Rename(..) | Command::Restore(..) | Command::Move(..) | Command::Create(..) | Command::RenameFolder(..) | Command::MoveFolder(..)) && !allowed() {
        return Err(err(409, "not_started"));
    }
    let unit = |r: fsops::Result<()>| r.map(|_| Value::Null).map_err(core_error);
    match parsed {
        Command::Rename(id, name) => unit(ops.rename_file(id, &name)),
        Command::Restore(id) => unit(ops.restore_file(id)),
        Command::Move(ids, folder) => ops
            .move_files_to_folder(&ids, folder, allowed)
            .map(|r| json!(r))
            .map_err(core_error),
        Command::Trash(ids) => {
            let count = eligible(ops, &ids, None, false)?;
            reserve(app, DestructiveKind::Trash, count)?;
            backup(app, count, false)?;
            progress.set_file_ids(&ids);
            ops.delete_files(&ids, allowed)
                .map(|r| json!(r))
                .map_err(core_error)
        }
        Command::Permanent(id, _) => {
            let row = ops.state(id).map_err(core_error)?;
            if row.deleted_at.is_none() {
                return Err(core_error(fsops::Error::Conflict));
            }
            reserve(app, DestructiveKind::Permanent, 1)?;
            backup(app, 1, false)?;
            if !allowed() {
                return Ok(json!({"id":id.to_string(),"ok":false,"kind":"not_started"}));
            }
            progress.set_file_ids(&[id]);
            ops.delete_file_permanently(id)
                .map(|r| json!(r))
                .map_err(core_error)
        }
        Command::Create(parent, name) => ops
            .create_folder(Some(parent), &name)
            .map(|id| json!(id.to_string()))
            .map_err(core_error),
        Command::RenameFolder(id, name) => unit(ops.rename_folder(id, &name)),
        Command::MoveFolder(id, parent) => unit(ops.move_folder(id, parent)),
        Command::RemoveFolder(id) => {
            ops.folder_removal_summary(id).map_err(core_error)?;
            let ids = folder_ids(app, id)?;
            progress.set_file_ids(&ids);
            app.fileops_safety.lock().map_err(|_| err(503, "restart_required"))?
                .reserve_folder(ids.len(), now).map_err(budget_error)?;
            backup(app, ids.len().max(1), true)?;
            if !allowed() {
                return Err(err(409, "not_started"));
            }
            progress.set_file_ids(&ids);
            ops.db.cancellation = Some(cancellation.clone());
            let result = ops.remove_folder_from_catalog(id);
            ops.db.cancellation = None;
            if cancellation.interrupted() {
                return Err(err(409, "not_started"));
            }
            result.map(|v| json!(v)).map_err(core_error)
        }
        Command::Start(count, cutoff, _) => {
            let snapshot = ops
                .empty_trash_snapshot(count, cutoff.as_deref())
                .map_err(core_error)?;
            app.fileops_safety
                .lock()
                .map_err(|_| err(503, "restart_required"))?
                .reserve_empty_start(now)
                .map_err(retry)?;
            if snapshot.remaining() == 0 {
                // Nothing emptiable (e.g. only entries with companions): no item limit, no backup, no token.
                return Ok(json!({"token":"","remaining":0}));
            }
            let cutoff_time = cutoff
                .as_deref()
                .map(chrono::DateTime::parse_from_rfc3339)
                .transpose()
                .map_err(|_| err(400, "invalid_arguments"))?;
            let ids = snapshot.ids();
            progress.set_file_ids(ids);
            backup(app, count, true)?;
            if !allowed() {
                return Err(err(409, "not_started"));
            }
            state
                .snapshots
                .retain(|_, s| now.saturating_sub(s.created) < 3600);
            if state.snapshots.len() >= 100 {
                return Err(err(503, "snapshot_capacity"));
            }
            let token = random_token()?;
            state.snapshots.insert(
                token.clone(),
                Snapshot {
                    session: current.token.clone(),
                    created: now,
                    snapshot,
                    cutoff: cutoff_time,
                },
            );
            Ok(json!({"token":token,"remaining":count}))
        }
        Command::Block(token) => {
            let snap = state
                .snapshots
                .get_mut(&token)
                .filter(|s| s.session == current.token && now.saturating_sub(s.created) < 3600)
                .ok_or_else(|| err(404, "snapshot_not_found"))?;
            progress.set_file_ids(snap.snapshot.next_block_ids());
            app.fileops_safety
                .lock()
                .map_err(|_| err(503, "restart_required"))?
                .reserve_empty_block(now)
                .map_err(retry)?;
            app.fileops_safety
                .lock()
                .map_err(|_| err(503, "restart_required"))?
                .reserve_empty_block_items(snap.snapshot.remaining().min(200), now)
                .map_err(budget_error)?;
            let count = eligible(ops, snap.snapshot.next_block_ids(), snap.cutoff, true)?;
            backup(app, count, false)?;
            let results = ops
                .empty_trash(&mut snap.snapshot, allowed)
                .map_err(core_error)?;
            let ids: Vec<i64> = results.iter().filter_map(|r| r.id.parse().ok()).collect();
            progress.set_file_ids(&ids);
            let remaining = snap.snapshot.remaining();
            if remaining == 0 {
                state.snapshots.remove(&token);
            }
            Ok(json!({"results":results,"remaining":remaining}))
        }
    }
}
pub(super) fn folder_ids(app: &App, id: i64) -> Result<Vec<i64>, Box<Response>> {
    let conn = app
        .writer
        .as_ref()
        .unwrap()
        .conn
        .lock()
        .map_err(|_| err(503, "restart_required"))?;
    let mut stmt=conn.prepare("WITH RECURSIVE subtree(id) AS (SELECT id FROM folders WHERE id=?1 UNION SELECT f.id FROM folders f JOIN subtree s ON f.parent_id=s.id) SELECT id FROM files WHERE folder_id IN (SELECT id FROM subtree) AND deleted_at IS NULL").map_err(|_|err(500,"internal"))?;
    let result = stmt
        .query_map([id], |r| r.get(0))
        .map_err(|_| err(500, "internal"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| err(500, "internal"));
    result
}

fn eligible(
    ops: &mut Ops,
    ids: &[i64],
    cutoff: Option<chrono::DateTime<chrono::FixedOffset>>,
    trash: bool,
) -> Result<usize, Box<Response>> {
    let mut count = 0;
    for id in ids {
        match ops.state(*id) {
            // Blocked items only produce a refusal; they must not spend limits or backups.
            Ok(_) if ops.has_companions(*id).map_err(core_error)? => (),
            Ok(row) => {
                if trash {
                    if let Some(time) = row.deleted_at {
                        let parsed = chrono::DateTime::parse_from_rfc3339(&time)
                            .map_err(|_| err(409, "changed"))?;
                        if cutoff.is_some_and(|c| parsed <= c) {
                            count += 1
                        }
                    }
                } else if row.deleted_at.is_none() {
                    count += 1
                }
            }
            Err(fsops::Error::NotFound) => (),
            Err(e) => return Err(core_error(e)),
        }
    }
    Ok(count)
}

fn completed_items(command: &str, v: &Value) -> u64 {
    if command == "empty_trash_start" {
        0
    } else if let Some(results) = v
        .as_array()
        .or_else(|| v.get("results").and_then(Value::as_array))
    {
        results
            .iter()
            .filter(|r| r.get("ok") == Some(&Value::Bool(true)))
            .count() as u64
    } else if command == "remove_folder_from_catalog" {
        v.get("modelCount").and_then(Value::as_u64).unwrap_or(0)
            + v.get("folderCount").and_then(Value::as_u64).unwrap_or(0)
    } else if v.get("kind").is_some() {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod audit_count_tests {
    use super::*;
    #[test]
    fn folder_audit_counts_actual_camel_case_core_dto() {
        let dto = serde_json::to_value(fsops::FolderRemovalCount {
            model_count: 3,
            folder_count: 2,
        })
        .unwrap();
        assert_eq!(dto, json!({"modelCount":3,"folderCount":2}));
        assert_eq!(completed_items("remove_folder_from_catalog", &dto), 5);
    }
}

#[cfg(test)]
mod deadline_tests {
    use super::*;
    #[test]
    fn deadline_starts_at_first_check_not_at_creation() {
        let c = deadline_cancellation(Duration::from_millis(150), || true);
        // A slow backup between creation and the first check must not count.
        std::thread::sleep(Duration::from_millis(250));
        assert!(c.allowed());
        std::thread::sleep(Duration::from_millis(250));
        assert!(!c.allowed());
    }
    #[test]
    fn revoked_validity_stops_even_inside_deadline() {
        let valid = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let v = valid.clone();
        let c = deadline_cancellation(Duration::from_secs(20), move || v.load(std::sync::atomic::Ordering::Acquire));
        assert!(c.allowed());
        valid.store(false, std::sync::atomic::Ordering::Release);
        assert!(!c.allowed());
    }
}
