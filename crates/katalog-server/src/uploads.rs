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
        "archive_format_unsupported" => "Archivformat nicht unterstützt (nur ZIP und 7z)",
        "archive_too_many_models" => "Archiv enthält zu viele Modelle",
        "archive_memory_limit" => "Archiv braucht zu viel Arbeitsspeicher beim Entpacken",
        "archive_timeout" => "Entpacken dauert zu lange",
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
    if matches!(e.status, 429 | 503) && e.kind != "restart_required" {
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
    /// Only for an unpacked archive: models taken over, files left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    models: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped: Option<u32>,
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
    pub(super) active: Arc<Semaphore>,
    jobs: Mutex<HashMap<String, Arc<Job>>>,
    hub: Mutex<Hub>,
}
impl Runtime {
    async fn changed(self: &Arc<Self>) {
        let copy = self.clone();
        let _ = tokio::task::spawn_blocking(move || copy.changed_blocking()).await;
    }
    pub(super) fn changed_blocking(&self) {
        loop {
            let sequence = self.hub.lock().unwrap().sequence;
            let uploads = pending_entries(&self.store);
            let mut hub = self.hub.lock().unwrap();
            if hub.sequence != sequence {
                continue;
            }
            hub.publish("import-progress", json!({"uploads": uploads}));
            break;
        }
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
    store.pending_list().unwrap_or_default().iter().map(|e|json!({"uploadId":e.id,"name":e.name,"size":e.size,"time":e.time,"state":e.state,"result":e.result,"archive":e.archive})).collect()
}
pub(super) fn initialize(config: &Config) -> Result<Option<Arc<Runtime>>, String> {
    let Some(c) = &config.upload else {
        return Ok(None);
    };
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
    let store = UploadStore::load(
        c.dir.clone(),
        auth_parent.join("web-uploads.json"),
        database.parent().ok_or("upload_database")?.to_path_buf(),
        c.max_bytes,
        c.max_files,
        &HashMap::new(),
    )
    .map_err(|e| e.kind.to_string())?;
    Ok(Some(Arc::new(Runtime {
        store,
        active: Arc::new(Semaphore::new(1)),
        jobs: Mutex::new(HashMap::new()),
        hub: Mutex::new(Hub::default()),
    })))
}
/// Runs only after fsops recovery and upload detachment have completed.
pub(super) fn reconcile(config: &Config, runtime: Option<&Arc<Runtime>>) -> Result<(), String> {
    let Some(runtime) = runtime else { return Ok(()) };
    let conn = db::open_read_only(&config.db_path).map_err(|_| "upload_database")?;
    let mut stmt = conn.prepare("SELECT path,id,content_hash FROM files").map_err(|_| "upload_database")?;
    let rows: Vec<(PathBuf, String, Option<String>)> = stmt.query_map([], |r| Ok((PathBuf::from(r.get::<_,String>(0)?), r.get::<_,i64>(1)?.to_string(), r.get(2)?)))
        .map_err(|_| "upload_database")?.collect::<Result<_,_>>().map_err(|_| "upload_database")?;
    let ids = rows.iter().map(|(p,id,_)| (p.clone(),id.clone())).collect();
    let hashes = rows.into_iter().map(|(p,_,h)| (p,h)).collect();
    runtime.store.reconcile_catalog(&ids, &hashes).map_err(|e| e.kind.to_owned())
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
    pending = match tokio::task::spawn_blocking(move || pending.prepare().map(|()| pending)).await {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => return fail(e),
        Err(_) => return error(500, "upload_internal"),
    };
    // Serialize final registration with logout/password changes. A dropped request
    // cannot leave a registered file unless this last authenticated commit began.
    let change = app.changes.clone().lock_owned().await;
    let finish_app = app.clone();
    let response = tokio::task::spawn_blocking(move || {
        // Keep logout serialized even if the HTTP future is cancelled.
        let _change = change;
        let mut data = finish_app.data.lock().unwrap();
        cleanup(&mut data, finish_app.clock.now());
        if !data.sessions.contains_key(&session.token) {
            return error(401, "session_expired");
        }
        if data.auth.as_ref().is_none_or(|a| a.must_change) {
            return error(403, "password_change_required");
        }
        drop(data);
        let occupied = (|| {
            let conn = db::open_read_only(&finish_app.config.db_path).map_err(|_| ())?;
            let mut stmt = conn.prepare("SELECT path FROM files").map_err(|_| ())?;
            let paths = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|_| ())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| ())?;
            Ok::<_, ()>(paths.into_iter().map(PathBuf::from).collect())
        })();
        let Ok(occupied) = occupied else {
            return error(500, "upload_database");
        };
        match pending.register(&occupied) {
            Ok(e) => Json(json!({"uploadId":e.id,"name":e.name,"size":e.size})).into_response(),
            Err(e) => fail(e),
        }
    })
    .await
    .unwrap_or_else(|_| error(500, "upload_internal"));
    runtime.changed().await;
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
    if matches!(command, "start_upload_import" | "discard_upload") {
        if let Err(r) = rate(&app, &session) {
            return r;
        }
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
            rt.changed().await;
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
            if let Err(e) = rt.store.ensure_importable() {
                return fail(e);
            }
            let copy = rt.clone();
            let known = match tokio::task::spawn_blocking(move || copy.store.list()).await {
                Ok(Ok(v)) => v,
                Ok(Err(e)) => return fail(e),
                Err(_) => return error(500, "journal"),
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
            match tokio::task::spawn_blocking(move || rt.store.job_result(&arg.job_id)).await {
                Ok(Ok(Some(v))) => Json(v).into_response(),
                Ok(Ok(None)) => error(404, "job_not_found"),
                Ok(Err(e)) => fail(e),
                Err(_) => error(500, "journal"),
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
    mut entries: Vec<Entry>,
    _permit: OwnedSemaphorePermit,
) {
    // `entries` grows while the job runs: an unpacked archive adds its models.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_inner(&app, &rt, &job, &mut entries)
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
        rt.store.require_restart();
        job.view.lock().unwrap().kind = Some("restart_required".into());
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
                    models: None,
                    skipped: None,
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
        if failed && !job.cancel.load(Ordering::Acquire) && view.kind.is_none() {
            view.kind = Some("import_failed".into());
        }
    }
    let value = serde_json::to_value(&*job.view.lock().unwrap()).unwrap();
    let persisted = rt.store.save_job(&id, value).is_ok();
    if !persisted {
        rt.store.require_restart();
        job.view.lock().unwrap().kind = Some("restart_required".into());
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
            .prepare("SELECT id,content_hash FROM files WHERE path=?1")
            .map_err(|_| ())?;
        let ids = statement
            .query_map([path], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
            })
            .map_err(|_| ())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ())?;
        if let Some((id, Some(hash))) = ids.first() {
            let file = rt.store.checked_file(entry).map_err(|_| ())?;
            if files::compute_reader_hash(file).map_err(|_| ())? == *hash {
                updates.push((entry.clone(), id.to_string()));
            }
        }
    }
    drop(conn);
    let mut records = Vec::new();
    for (entry, id) in updates {
        if !job
            .view
            .lock()
            .unwrap()
            .files
            .iter()
            .any(|f| f.upload_id == entry.id)
        {
            record(&mut records, &entry, "imported", Some(id), None, None);
        }
    }
    record_batch(rt, job, records)
}
fn archive_result(entry: &Entry, models: u32, skipped: u32) -> FileResult {
    FileResult {
        upload_id: entry.id.clone(),
        name: entry.name.clone(),
        state: "imported".into(),
        file_id: None,
        existing_file_id: None,
        kind: Some("archive".into()),
        models: Some(models),
        skipped: Some(skipped),
    }
}
/// Unpacks every archive of the job into separate uploads and appends them to
/// `entries`. The heavy work (`upload_archive::expand`) holds neither the database
/// connection nor any journal lock; only the final journal write is serialized.
fn expand_archives(app: &App, rt: &Arc<Runtime>, job: &Job, entries: &mut Vec<Entry>) -> Result<(), ()> {
    let archives: Vec<Entry> = entries
        .iter()
        .filter(|e| crate::upload_store::is_archive_name(&e.name))
        .cloned()
        .collect();
    let upload = app.config.upload.as_ref().unwrap();
    let job_id = job.view.lock().unwrap().job_id.clone();
    for archive in archives {
        if job.cancel.load(Ordering::Acquire) {
            break;
        }
        job.view.lock().unwrap().current = Some(archive.name.clone());
        rt.publish(job, false);
        let context = crate::upload_archive::Context {
            store: &rt.store,
            upload,
            db_path: &app.config.db_path,
            cancel: &job.cancel,
            job_id: &job_id,
            timeout: crate::upload_archive::ARCHIVE_TIMEOUT,
        };
        let summary = |models, skipped| serde_json::to_value(archive_result(&archive, models, skipped)).unwrap();
        match crate::upload_archive::expand(&context, &archive, &summary) {
            Ok(done) => {
                entries.extend(done.children);
                let mut view = job.view.lock().unwrap();
                view.files.push(archive_result(&archive, done.models, done.skipped));
                view.done = view.files.len();
                view.total = entries.len();
                drop(view);
                rt.publish(job, false);
            }
            Err(crate::upload_archive::Failure::Cancelled) => break,
            // A property of the archive: it will never import, the user discards it.
            Err(crate::upload_archive::Failure::Invalid(kind)) => {
                let mut records = Vec::new();
                record(&mut records, &archive, "invalid", None, None, Some(kind));
                record_batch(rt, job, records)?;
            }
            // Space, quota or I/O: nothing wrong with the archive, so it goes back to
            // "uploaded" and the import can simply be started again.
            Err(crate::upload_archive::Failure::Store(e)) => {
                rt.store.reopen(&archive.id).map_err(|_| ())?;
                let mut records = Vec::new();
                record(&mut records, &archive, "uploaded", None, None, Some(e.kind));
                let mut view = job.view.lock().unwrap();
                view.files.extend(records);
                view.done = view.files.len();
                drop(view);
                rt.publish(job, false);
            }
        }
    }
    Ok(())
}
fn run_inner(app: &App, rt: &Arc<Runtime>, job: &Job, entries: &mut Vec<Entry>) -> Result<(), ()> {
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
    job.view.lock().unwrap().state = "importing".into();
    expand_archives(app, rt, job, entries)?;
    let entries: &[Entry] = entries;
    let reader = Reader {
        rt: rt.clone(),
        entries: entries.to_vec(),
        limits: app.config.upload.as_ref().unwrap().limits,
    };
    let mut batch = Vec::new();
    let mut records = Vec::new();
    let mut batch_bytes = 0;
    // Archives were handled above; what remains are the models to import.
    for (index, entry) in entries
        .iter()
        .filter(|e| !crate::upload_store::is_archive_name(&e.name))
        .enumerate()
    {
        if job.cancel.load(Ordering::Acquire) {
            break;
        }
        job.view.lock().unwrap().current = Some(entry.name.clone());
        rt.publish(job, false);
        let path = rt.store.path(entry);
        match reader.read(&path) {
            Ok(new_file) => {
                batch_bytes += new_file.thumbnail_png.as_ref().map_or(0, Vec::len)
                    + new_file.slice_info_json.as_ref().map_or(0, String::len)
                    + new_file
                        .metadata
                        .iter()
                        .map(|(k, v)| k.len() + v.len())
                        .sum::<usize>()
                    + new_file
                        .materials
                        .iter()
                        .map(|m| m.name.len() + m.display_color.as_ref().map_or(0, String::len))
                        .sum::<usize>()
                    + new_file.tags.iter().map(String::len).sum::<usize>();
                batch.push(PendingImport {
                    path,
                    folder_root: None,
                    new_file,
                });
            }
            Err(_) => record(&mut records, entry, "invalid", None, None, Some("invalid")),
        }
        if (index + 1) % 20 == 0 || batch_bytes >= 16 * 1024 * 1024 {
            flush(&mut database, rt, job, entries, &mut batch, &mut records)?;
            batch_bytes = 0;
        }
    }
    flush(&mut database, rt, job, entries, &mut batch, &mut records)
}
fn record(
    records: &mut Vec<FileResult>,
    e: &Entry,
    state: &str,
    file_id: Option<String>,
    existing_file_id: Option<String>,
    kind: Option<&str>,
) {
    let value = FileResult {
        upload_id: e.id.clone(),
        name: e.name.clone(),
        state: state.into(),
        file_id,
        existing_file_id,
        kind: kind.map(str::to_string),
        models: None,
        skipped: None,
    };
    records.push(value);
}
fn record_batch(rt: &Runtime, job: &Job, records: Vec<FileResult>) -> Result<(), ()> {
    if records.is_empty() {
        return Ok(());
    }
    let updates = records
        .iter()
        .map(|value| {
            let state = match value.state.as_str() {
                "imported" => UploadState::Imported,
                "duplicate" => UploadState::Duplicate,
                _ => UploadState::Invalid,
            };
            (
                value.upload_id.clone(),
                state,
                serde_json::to_value(value).unwrap(),
            )
        })
        .collect();
    rt.store.complete_batch(updates).map_err(|_| ())?;
    let mut view = job.view.lock().unwrap();
    view.files.extend(records);
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
    records: &mut Vec<FileResult>,
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
        record(records, e, "imported", Some(file.id), None, None);
    }
    for file in duplicates {
        let e = entries
            .iter()
            .find(|e| rt.store.path(e).to_string_lossy() == file.path)
            .ok_or(())?;
        record(
            records,
            e,
            "duplicate",
            None,
            file.existing_file_id,
            Some(&file.kind),
        );
    }
    for file in result.skipped {
        let e = entries
            .iter()
            .find(|e| rt.store.path(e).to_string_lossy() == file.path)
            .ok_or(())?;
        record(records, e, "invalid", None, None, Some("database"));
    }
    record_batch(rt, job, std::mem::take(records))?;
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
/// Browsers send Sec-Fetch-* only to potentially trustworthy origins (HTTPS,
/// localhost). Over plain HTTP with an IP or LAN name the header is absent;
/// then origin_ok decides: a cross-origin EventSource is a CORS request and
/// always carries Origin, a same-origin GET carries none. A present header must
/// still be same-origin/none (cross-site, same-site, cross-origin stay 403).
fn events_origin_ok(app: &App, headers: &HeaderMap) -> bool {
    if !origin_ok(app, headers) {
        return false;
    }
    let mut site = headers.get_all("sec-fetch-site").iter();
    match (site.next(), site.next()) {
        (None, _) => true,
        (Some(v), None) => matches!(v.to_str(), Ok("same-origin" | "none")),
        _ => false,
    }
}
pub(super) async fn events(app: Arc<App>, session: Session, headers: &HeaderMap) -> Response {
    if !events_origin_ok(&app, headers) {
        return error(403, "origin");
    }
    let rt = app.uploads.as_ref().unwrap().clone();
    let (tx, rx) = mpsc::channel(32);
    let copy = rt.clone();
    let token = session.token.clone();
    let snapshot_tx = tx.clone();
    let guard = match tokio::task::spawn_blocking(move || {
        let rt = copy;
        let tx = snapshot_tx;
        // Never hold the hub while waiting on journal fsync. Retry if a
        // publication raced the snapshot; subscribe under the same hub lock.
        let (mut jobs, uploads, mut hub) = loop {
            let sequence = rt.hub.lock().unwrap().sequence;
            let jobs = rt.store.recent_job_results().unwrap_or_default();
            let uploads = pending_entries(&rt.store);
            let hub = rt.hub.lock().unwrap();
            if hub.sequence == sequence {
                break (jobs, uploads, hub);
            }
        };
        if hub.session_counts.get(&token).copied().unwrap_or(0) >= 4 {
            return Err(429);
        }
        if hub.session_counts.values().sum::<usize>() >= 32 {
            return Err(503);
        }
        // Publish and snapshot use this lock: updates that race the snapshot are
        // repeated after it, never lost between observation and subscription.
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
            json!({"snapshot":true,"jobs":jobs,"uploads":uploads}),
        );
        tx.try_send(snapshot).unwrap();
        let id = hub.sequence;
        hub.clients.insert(id, tx.clone());
        *hub.session_counts.entry(token.clone()).or_default() += 1;
        Ok(StreamGuard {
            rt: rt.clone(),
            id,
            session: token,
        })
    })
    .await
    {
        Ok(Ok(guard)) => guard,
        Ok(Err(status)) => return crate::write::retry(status),
        Err(_) => return error(500, "journal"),
    };
    let id = guard.id;
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
pub(super) async fn list(rt: Arc<Runtime>) -> Response {
    tokio::task::spawn_blocking(move || Json(public_entries(&rt.store)).into_response())
        .await
        .unwrap_or_else(|_| error(500, "journal"))
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
    store.pending_list().unwrap_or_default().iter().map(|e|json!({"uploadId":e.id,"name":e.name,"size":e.size,"time":e.time,"state":e.state,"result":e.result,"archive":e.archive})).collect()
}
