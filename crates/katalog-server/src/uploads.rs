//! Instance-owned uploads and import jobs. No transport DTO contains a path.
use super::*;
use crate::upload_store::{Entry, StoreError, UploadState, UploadStore};
use axum::body::Bytes;
use hyper::body::{Body as HttpBody, Frame};
use katalog_core::{
    db,
    import::{
        files::{self, ImportDb, PendingImport},
        BatchOptions, FixedFolder, ImportReader, ImportResult,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};

pub(super) fn command(name: &str) -> bool {
    matches!(
        name,
        "start_upload_import"
            | "discard_upload"
            | "cancel_import"
            | "get_import_state"
            | "get_import_result"
    )
}
fn fail(e: StoreError) -> Response {
    let message = match e.kind {
        "archive_desktop_only" => "Archive: nur in der Desktop-App",
        "pending_limit" | "pending_quota" | "import_or_discard_first" => {
            "Erst importieren oder verwerfen"
        }
        kind => kind,
    };
    let mut r = (
        StatusCode::from_u16(e.status).unwrap(),
        Json(json!({"kind":e.kind,"message":message,"expected":e.status<500})),
    )
        .into_response();
    if matches!(e.status, 429 | 503) {
        r.headers_mut().insert("retry-after", "1".parse().unwrap());
    }
    r
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileResult {
    upload_id: String,
    name: String,
    state: String,
    file_id: Option<String>,
    existing_file_id: Option<String>,
    kind: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobView {
    job_id: String,
    state: String,
    current: Option<String>,
    total: usize,
    done: usize,
    files: Vec<FileResult>,
    kind: Option<String>,
}
struct Job {
    view: Mutex<JobView>,
    cancel: AtomicBool,
}
struct Event {
    id: u64,
    name: &'static str,
    value: Value,
}
impl Event {
    fn bytes(&self) -> Bytes {
        Bytes::from(format!(
            "id: {}\nevent: {}\ndata: {}\n\n",
            self.id, self.name, self.value
        ))
    }
}
#[derive(Default)]
struct Hub {
    sequence: u64,
    clients: HashMap<u64, mpsc::Sender<Bytes>>,
    session_counts: HashMap<String, usize>,
}
impl Hub {
    fn event(&mut self, name: &'static str, value: Value) -> Bytes {
        self.sequence += 1;
        Event {
            id: self.sequence,
            name,
            value,
        }
        .bytes()
    }
    fn publish(&mut self, name: &'static str, value: Value) {
        let bytes = self.event(name, value);
        self.clients
            .retain(|_, tx| tx.try_send(bytes.clone()).is_ok());
    }
}
pub(super) struct Runtime {
    pub store: Arc<UploadStore>,
    active: Arc<Semaphore>,
    jobs: Mutex<HashMap<String, Arc<Job>>>,
    hub: Mutex<Hub>,
}
impl Runtime {
    fn changed(&self) {
        self.hub.lock().unwrap().publish(
            "import-progress",
            json!({"uploads":pending_entries(&self.store)}),
        );
    }
    fn publish(&self, job: &Job, finished: bool) {
        let mut hub = self.hub.lock().unwrap();
        hub.publish(
            if finished {
                "import-finished"
            } else {
                "import-progress"
            },
            serde_json::to_value(&*job.view.lock().unwrap()).unwrap(),
        );
    }
}
fn public_entries(store: &UploadStore) -> Vec<Value> {
    store.list().unwrap_or_default().iter().map(|e|json!({"uploadId":e.id,"name":e.name,"size":e.size,"time":e.time,"state":e.state,"result":e.result})).collect()
}
pub(super) fn initialize(config: &Config) -> Result<Option<Arc<Runtime>>, String> {
    let Some(c) = &config.upload else {
        return Ok(None);
    };
    let conn = db::open_read_only(&config.db_path).map_err(|_| "upload_database".to_string())?;
    let mut stmt = conn
        .prepare("SELECT path,id FROM files")
        .map_err(|_| "upload_database".to_string())?;
    let file_ids: HashMap<PathBuf, String> = stmt
        .query_map([], |r| {
            Ok((
                PathBuf::from(r.get::<_, String>(0)?),
                r.get::<_, i64>(1)?.to_string(),
            ))
        })
        .map_err(|_| "upload_database".to_string())?
        .collect::<Result<_, _>>()
        .map_err(|_| "upload_database".to_string())?;
    let paths = file_ids.keys().cloned().collect();
    let auth_parent = config
        .auth_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(|_| "journal_path".to_string())?;
    let database = config
        .db_path
        .canonicalize()
        .map_err(|_| "upload_database".to_string())?;
    let store = UploadStore::open(
        c.dir.clone(),
        auth_parent.join("web-uploads.json"),
        database.parent().ok_or("upload_database")?.to_path_buf(),
        c.max_bytes,
        c.max_files,
        &paths,
    )
    .map_err(|e| e.kind.to_string())?;
    store
        .reconcile_file_ids(&file_ids)
        .map_err(|e| e.kind.to_string())?;
    Ok(Some(Arc::new(Runtime {
        store,
        active: Arc::new(Semaphore::new(1)),
        jobs: Mutex::new(HashMap::new()),
        hub: Mutex::new(Hub::default()),
    })))
}
pub(super) fn authenticated(app: &App, s: &Session) -> bool {
    let mut data = app.data.lock().unwrap();
    cleanup(&mut data, app.clock.now());
    data.sessions.contains_key(&s.token) && data.auth.as_ref().is_some_and(|a| !a.must_change)
}
#[allow(clippy::result_large_err)]
fn rate(app: &App, s: &Session) -> Result<(), Response> {
    let mut data = app.data.lock().unwrap();
    cleanup(&mut data, app.clock.now());
    let Some(s) = data.sessions.get_mut(&s.token) else {
        return Err(error(401, "session_expired"));
    };
    if !s.bucket.take(app.clock.now()) {
        return Err(crate::write::retry(429));
    }
    Ok(())
}
pub(super) async fn upload(app: Arc<App>, session: Session, req: Request) -> Response {
    if let Err(r) = rate(&app, &session) {
        return r;
    }
    if req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        != Some("application/octet-stream")
    {
        return error(400, "content_type");
    }
    let Some(length) = req.headers().get(header::CONTENT_LENGTH) else {
        return error(411, "length_required");
    };
    let Some(length) = length
        .to_str()
        .ok()
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|s| s.parse::<u64>().ok())
    else {
        return error(400, "length_invalid");
    };
    let Some(name) = req
        .headers()
        .get("x-upload-name")
        .and_then(|h| h.to_str().ok())
    else {
        return error(400, "name_invalid");
    };
    let runtime = app.uploads.as_ref().unwrap().clone();
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if matches!(ext.as_str(), "stl" | "obj")
        && length
            > app
                .config
                .upload
                .as_ref()
                .unwrap()
                .limits
                .max_mesh_file_bytes
    {
        return error(413, "file_too_large");
    }
    let store = runtime.store.clone();
    let name = name.to_owned();
    let mesh_limit = app
        .config
        .upload
        .as_ref()
        .unwrap()
        .limits
        .max_mesh_file_bytes;
    let mut pending =
        match tokio::task::spawn_blocking(move || store.begin_limited(length, &name, mesh_limit))
            .await
        {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => return fail(e),
            Err(_) => return error(500, "upload_internal"),
        };
    let mut body = req.into_body();
    let mut progress = 0usize;
    let started = tokio::time::Instant::now();
    let mut idle = started + Duration::from_secs(60);
    loop {
        let frame = tokio::time::timeout_at(
            idle.min(started + Duration::from_secs(3600)),
            std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)),
        )
        .await;
        let chunk = match frame {
            Err(_) => return error(408, "upload_timeout"),
            Ok(None) => break,
            Ok(Some(Err(_))) => return error(400, "length_mismatch"),
            Ok(Some(Ok(f))) => match f.into_data() {
                Ok(d) => d,
                Err(_) => continue,
            },
        };
        progress += chunk.len();
        if progress >= 65536 {
            progress = 0;
            idle = tokio::time::Instant::now() + Duration::from_secs(60)
        }
        pending = match tokio::task::spawn_blocking(move || {
            let result = pending.write(&chunk);
            (pending, result)
        })
        .await
        {
            Ok((p, Ok(()))) => p,
            Ok((_, Err(e))) => return fail(e),
            Err(_) => return error(500, "upload_internal"),
        };
    }
    // Serialize final registration with logout/password changes. A dropped request
    // cannot leave a registered file unless this last authenticated commit began.
    let _change = app.changes.lock().await;
    let finish_app = app.clone();
    let response = tokio::task::spawn_blocking(move || {
        let mut data = finish_app.data.lock().unwrap();
        cleanup(&mut data, finish_app.clock.now());
        if !data.sessions.contains_key(&session.token) {
            return error(401, "session_expired");
        }
        if data.auth.as_ref().is_none_or(|a| a.must_change) {
            return error(403, "password_change_required");
        }
        match pending.finish() {
            Ok(e) => Json(json!({"uploadId":e.id,"name":e.name,"size":e.size})).into_response(),
            Err(e) => fail(e),
        }
    })
    .await
    .unwrap_or_else(|_| error(500, "upload_internal"));
    runtime.changed();
    response
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Start {
    upload_ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JobId {
    job_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UploadId {
    upload_id: String,
}
#[allow(clippy::result_large_err)]
fn random_id() -> Result<String, Response> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|_| error(500, "random"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}
pub(super) async fn invoke(
    app: Arc<App>,
    session: Session,
    command: &str,
    args: Value,
) -> Response {
    let rt = app.uploads.as_ref().unwrap().clone();
    if let Err(r) = rate(&app, &session) {
        return r;
    }
    match command {
        "discard_upload" => {
            let _change = app.changes.lock().await;
            if !authenticated(&app, &session) {
                return error(401, "session_expired");
            }
            let Ok(arg) = serde_json::from_value::<UploadId>(args) else {
                return error(400, "arguments");
            };
            let copy = rt.clone();
            let result =
                tokio::task::spawn_blocking(move || copy.store.discard(&arg.upload_id)).await;
            rt.changed();
            match result {
                Ok(Ok(())) => Json(json!({"ok":true})).into_response(),
                Ok(Err(e)) => fail(e),
                Err(_) => error(500, "journal"),
            }
        }
        "start_upload_import" => {
            let _change = app.changes.lock().await;
            if !authenticated(&app, &session) {
                return error(401, "session_expired");
            }
            let Ok(arg) = serde_json::from_value::<Start>(args) else {
                return error(400, "arguments");
            };
            if arg.upload_ids.is_empty() {
                return error(400, "empty_uploads");
            }
            if arg.upload_ids.len() > 200 {
                return crate::write::retry(503);
            }
            if arg
                .upload_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != arg.upload_ids.len()
            {
                return error(409, "upload_conflict");
            }
            let known = match rt.store.list() {
                Ok(v) => v,
                Err(e) => return fail(e),
            };
            for id in &arg.upload_ids {
                match known.iter().find(|e| &e.id == id) {
                    None => return error(404, "upload_unknown"),
                    Some(e) if e.state != UploadState::Uploaded => {
                        return error(409, "upload_conflict")
                    }
                    _ => {}
                }
            }
            let permit = match rt.active.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => return crate::write::retry(503),
            };
            let id = match random_id() {
                Ok(id) => id,
                Err(r) => return r,
            };
            let copy = rt.clone();
            let job_id = id.clone();
            let entries = match tokio::task::spawn_blocking(move || {
                copy.store.begin_import(&arg.upload_ids, &job_id)
            })
            .await
            {
                Ok(Ok(e)) => e,
                Ok(Err(e)) => return fail(e),
                Err(_) => return error(500, "journal"),
            };
            let job = Arc::new(Job {
                view: Mutex::new(JobView {
                    job_id: id.clone(),
                    state: "queued".into(),
                    current: None,
                    total: entries.len(),
                    done: 0,
                    files: Vec::new(),
                    kind: None,
                }),
                cancel: AtomicBool::new(false),
            });
            rt.jobs.lock().unwrap().insert(id.clone(), job.clone());
            rt.publish(&job, false);
            let worker_app = app.clone();
            tokio::task::spawn_blocking(move || run_job(worker_app, rt, job, entries, permit));
            Json(json!({"jobId":id})).into_response()
        }
        "get_import_state" | "get_import_result" | "cancel_import" => {
            let Ok(arg) = serde_json::from_value::<JobId>(args) else {
                return error(400, "arguments");
            };
            let job = rt.jobs.lock().unwrap().get(&arg.job_id).cloned();
            if let Some(job) = job {
                if command == "cancel_import" {
                    job.cancel.store(true, Ordering::Release);
                    let mut view = job.view.lock().unwrap();
                    if matches!(view.state.as_str(), "queued" | "importing") {
                        view.state = "cancelling".into();
                    }
                    drop(view);
                    rt.publish(&job, false);
                }
                return Json(serde_json::to_value(&*job.view.lock().unwrap()).unwrap())
                    .into_response();
            }
            match rt.store.job_result(&arg.job_id) {
                Ok(Some(v)) => Json(v).into_response(),
                Ok(None) => error(404, "job_not_found"),
                Err(e) => fail(e),
            }
        }
        _ => error(404, "command_not_found"),
    }
}
struct Reader {
    rt: Arc<Runtime>,
    entries: Vec<Entry>,
    limits: files::WebImportLimits,
}
impl ImportReader for Reader {
    fn read(&self, path: &Path) -> ImportResult<db::models::NewFile> {
        let entry = self
            .entries
            .iter()
            .find(|e| self.rt.store.path(e) == path)
            .ok_or("invalid")?;
        let file = self.rt.store.checked_file(entry).map_err(|_| "invalid")?;
        files::read_web_model(file, path, &entry.name, self.limits).map_err(|_| "invalid".into())
    }
}
struct Database<'a> {
    app: &'a App,
    folder: FixedFolder,
    cancel: &'a AtomicBool,
}
impl ImportDb for Database<'_> {
    fn batch_options(&self) -> BatchOptions {
        BatchOptions::in_place(self.folder.clone())
    }
    fn with_conn<R>(
        &mut self,
        f: impl FnOnce(&mut katalog_core::write::Connection) -> ImportResult<R>,
    ) -> ImportResult<R> {
        // Never keep a DB mutex while parsing, journalling, or waiting for clients.
        loop {
            if self.cancel.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            match self.app.writer.as_ref().unwrap().conn.try_lock() {
                Ok(mut conn) => return f(&mut conn),
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(_) => return Err("database".into()),
            }
        }
    }
}
fn run_job(
    app: Arc<App>,
    rt: Arc<Runtime>,
    job: Arc<Job>,
    entries: Vec<Entry>,
    _permit: OwnedSemaphorePermit,
) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_inner(&app, &rt, &job, &entries)
    }));
    let failed = !matches!(result, Ok(Ok(())));
    let id = job.view.lock().unwrap().job_id.clone();
    // A panic/journal failure may follow a successful SQLite commit. Only
    // release remaining uploads after reading the committed catalog state.
    let reconciled = if failed {
        reconcile_commits(&app, &rt, &job, &entries).is_ok()
    } else {
        true
    };
    if !reconciled || rt.store.reset_importing(&id).is_err() {
        job.view.lock().unwrap().kind = Some("journal".into());
    }
    if let Ok(current) = rt.store.list() {
        let mut view = job.view.lock().unwrap();
        for entry in &entries {
            if !view.files.iter().any(|f| f.upload_id == entry.id) {
                let state = current
                    .iter()
                    .find(|e| e.id == entry.id)
                    .map(|e| e.state)
                    .unwrap_or(UploadState::Invalid);
                view.files.push(FileResult {
                    upload_id: entry.id.clone(),
                    name: entry.name.clone(),
                    state: serde_json::to_value(state)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .into(),
                    file_id: None,
                    existing_file_id: None,
                    kind: Some("not_started".into()),
                });
            }
        }
    }
    {
        let mut view = job.view.lock().unwrap();
        view.state = if job.cancel.load(Ordering::Acquire) && reconciled {
            "cancelled"
        } else if failed {
            "failed"
        } else {
            "completed"
        }
        .into();
        view.current = None;
        if failed && !job.cancel.load(Ordering::Acquire) {
            view.kind = Some("import_failed".into());
        }
    }
    let value = serde_json::to_value(&*job.view.lock().unwrap()).unwrap();
    let persisted = rt.store.save_job(&id, value).is_ok();
    if !persisted {
        job.view.lock().unwrap().kind = Some("journal".into());
        job.view.lock().unwrap().state = "failed".into();
    }
    rt.publish(&job, true);
    // Durable results replace memory retention; controls never wait on DB work.
    if persisted {
        rt.jobs.lock().unwrap().remove(&id);
    }
}
fn reconcile_commits(app: &App, rt: &Runtime, job: &Job, entries: &[Entry]) -> Result<(), ()> {
    let conn = db::open_read_only(&app.config.db_path).map_err(|_| ())?;
    let mut updates = Vec::new();
    for entry in entries {
        let path = rt.store.path(entry).to_string_lossy().into_owned();
        let mut statement = conn
            .prepare("SELECT id FROM files WHERE path=?1")
            .map_err(|_| ())?;
        let ids = statement
            .query_map([path], |r| r.get::<_, i64>(0))
            .map_err(|_| ())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ())?;
        if let Some(id) = ids.first() {
            updates.push((entry.clone(), id.to_string()));
        }
    }
    drop(conn);
    for (entry, id) in updates {
        if !job
            .view
            .lock()
            .unwrap()
            .files
            .iter()
            .any(|f| f.upload_id == entry.id)
        {
            record(rt, job, &entry, "imported", Some(id), None, None)?;
        }
    }
    Ok(())
}
fn run_inner(app: &App, rt: &Arc<Runtime>, job: &Job, entries: &[Entry]) -> Result<(), ()> {
    let root = app.config.upload.as_ref().unwrap().dir.clone();
    let folder = {
        let mut db = Database {
            app,
            cancel: &job.cancel,
            folder: FixedFolder {
                id: 0,
                root: root.clone(),
            },
        };
        db.with_conn(|c| db::ensure_folder_path(c, &root, &root).map_err(|_| "database".into()))
            .map_err(|_| ())?
    };
    let mut database = Database {
        app,
        cancel: &job.cancel,
        folder: FixedFolder { id: folder, root },
    };
    let reader = Reader {
        rt: rt.clone(),
        entries: entries.to_vec(),
        limits: app.config.upload.as_ref().unwrap().limits,
    };
    let mut batch = Vec::new();
    job.view.lock().unwrap().state = "importing".into();
    for entry in entries {
        if job.cancel.load(Ordering::Acquire) {
            break;
        }
        job.view.lock().unwrap().current = Some(entry.name.clone());
        rt.publish(job, false);
        let path = rt.store.path(entry);
        match reader.read(&path) {
            Ok(new_file) => batch.push(PendingImport {
                path,
                folder_root: None,
                new_file,
            }),
            Err(_) => record(rt, job, entry, "invalid", None, None, Some("invalid"))?,
        }
        if !batch.is_empty() {
            flush(&mut database, rt, job, entries, &mut batch)?;
        }
    }
    flush(&mut database, rt, job, entries, &mut batch)
}
fn record(
    rt: &Runtime,
    job: &Job,
    e: &Entry,
    state: &str,
    file_id: Option<String>,
    existing_file_id: Option<String>,
    kind: Option<&str>,
) -> Result<(), ()> {
    let value = FileResult {
        upload_id: e.id.clone(),
        name: e.name.clone(),
        state: state.into(),
        file_id,
        existing_file_id,
        kind: kind.map(str::to_string),
    };
    let state = match state {
        "imported" => UploadState::Imported,
        "duplicate" => UploadState::Duplicate,
        _ => UploadState::Invalid,
    };
    rt.store
        .complete_batch(vec![(
            e.id.clone(),
            state,
            serde_json::to_value(&value).unwrap(),
        )])
        .map_err(|_| ())?;
    let mut view = job.view.lock().unwrap();
    view.files.push(value);
    view.done = view.files.len();
    drop(view);
    rt.publish(job, false);
    Ok(())
}
fn flush(
    db: &mut Database<'_>,
    rt: &Runtime,
    job: &Job,
    entries: &[Entry],
    batch: &mut Vec<PendingImport>,
) -> Result<(), ()> {
    let mut result = files::empty_import_result();
    let mut duplicates = Vec::new();
    let outcome = files::store_batch_detailed(
        db,
        batch,
        &mut result,
        files::ImportMode::Batched,
        &mut duplicates,
    );
    for file in result.imported {
        let e = entries
            .iter()
            .find(|e| rt.store.path(e).to_string_lossy() == file.path)
            .ok_or(())?;
        record(rt, job, e, "imported", Some(file.id), None, None)?;
    }
    for file in duplicates {
        let e = entries
            .iter()
            .find(|e| rt.store.path(e).to_string_lossy() == file.path)
            .ok_or(())?;
        record(
            rt,
            job,
            e,
            "duplicate",
            None,
            file.existing_file_id,
            Some(&file.kind),
        )?;
    }
    for file in result.skipped {
        let e = entries
            .iter()
            .find(|e| rt.store.path(e).to_string_lossy() == file.path)
            .ok_or(())?;
        record(rt, job, e, "invalid", None, None, Some("database"))?;
    }
    outcome.map_err(|_| ())
}
struct StreamGuard {
    rt: Arc<Runtime>,
    id: u64,
    session: String,
}
impl Drop for StreamGuard {
    fn drop(&mut self) {
        let mut hub = self.rt.hub.lock().unwrap();
        hub.clients.remove(&self.id);
        if let Some(n) = hub.session_counts.get_mut(&self.session) {
            *n -= 1;
            if *n == 0 {
                hub.session_counts.remove(&self.session);
            }
        }
    }
}
struct EventBody {
    rx: mpsc::Receiver<Bytes>,
    _guard: StreamGuard,
}
impl HttpBody for EventBody {
    type Data = Bytes;
    type Error = std::convert::Infallible;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        self.rx.poll_recv(cx).map(|v| v.map(|b| Ok(Frame::data(b))))
    }
}
pub(super) fn events(app: Arc<App>, session: Session, headers: &HeaderMap) -> Response {
    if !origin_ok(&app, headers)
        || !matches!(
            headers.get("sec-fetch-site").and_then(|h| h.to_str().ok()),
            Some("same-origin" | "none")
        )
    {
        return error(403, "origin");
    }
    let rt = app.uploads.as_ref().unwrap().clone();
    let (tx, rx) = mpsc::channel(32);
    let id = {
        let mut hub = rt.hub.lock().unwrap();
        if hub.session_counts.get(&session.token).copied().unwrap_or(0) >= 4 {
            return crate::write::retry(429);
        }
        if hub.session_counts.values().sum::<usize>() >= 32 {
            return crate::write::retry(503);
        }
        // Publish and snapshot use this lock: updates that race the snapshot are
        // repeated after it, never lost between observation and subscription.
        let mut jobs = rt.store.recent_job_results().unwrap_or_default();
        let active_ids: std::collections::HashSet<_> =
            rt.jobs.lock().unwrap().keys().cloned().collect();
        jobs.retain(|v| {
            v["jobId"]
                .as_str()
                .is_none_or(|id| !active_ids.contains(id))
        });
        jobs.extend(
            rt.jobs
                .lock()
                .unwrap()
                .values()
                .map(|j| serde_json::to_value(&*j.view.lock().unwrap()).unwrap()),
        );
        let snapshot = hub.event(
            "import-progress",
            json!({"snapshot":true,"jobs":jobs,"uploads":pending_entries(&rt.store)}),
        );
        tx.try_send(snapshot).unwrap();
        let id = hub.sequence;
        hub.clients.insert(id, tx.clone());
        *hub.session_counts.entry(session.token.clone()).or_default() += 1;
        id
    };
    let guard = StreamGuard {
        rt: rt.clone(),
        id,
        session: session.token.clone(),
    };
    tokio::spawn(async move {
        let mut interval = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(15),
            Duration::from_secs(15),
        );
        loop {
            tokio::select! {_ = tx.closed()=>break,_=interval.tick()=>{
                if !authenticated(&app,&session){break}
                let mut hub=rt.hub.lock().unwrap();
                if !hub.clients.contains_key(&id){break}
                let ping=hub.event("ping",json!({}));if tx.try_send(ping).is_err(){break}
            }}
        }
        rt.hub.lock().unwrap().clients.remove(&id);
    });
    let mut response = Response::new(Body::new(EventBody { rx, _guard: guard }));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, "text/event-stream".parse().unwrap());
    response
        .headers_mut()
        .insert("x-accel-buffering", "no".parse().unwrap());
    response
}
pub(super) fn list(rt: &Runtime) -> Response {
    Json(public_entries(&rt.store)).into_response()
}

/// Deliberately stores no headers, file names, arguments, or filesystem errors.
pub(super) struct RequestAudit {
    command: String,
    size: u64,
    time: u64,
    pub status: u16,
}
impl RequestAudit {
    pub fn new(command: &str, size: u64, time: u64) -> Self {
        Self {
            command: command.into(),
            size,
            time,
            status: 499,
        }
    }
}
impl Drop for RequestAudit {
    fn drop(&mut self) {
        eprintln!(
            "upload time={} command={} status={} size={}",
            self.time, self.command, self.status, self.size
        );
    }
}

fn pending_entries(store: &UploadStore) -> Vec<Value> {
    store.pending_list().unwrap_or_default().iter().map(|e|json!({"uploadId":e.id,"name":e.name,"size":e.size,"time":e.time,"state":e.state,"result":e.result})).collect()
}
