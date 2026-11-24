//! Explicit read-only command allowlist and bounded blocking geometry work.
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use katalog_core::{
    catalog_issues, companion_read, db, disk_report, disk_sync_settings,
    error_code::{ErrorCode, LEGACY_CODES},
    read::{self, GeometryErrorCode, ReadError},
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// How long a request waits for the disk comparison. A hanging mount can block a directory read
/// for good, which no in-process limit can interrupt; the request is answered instead.
const DISK_REPORT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Where the one start comparison of this server run stands (W2 5.4). The read command
/// `run_start_sync` only reports it; it never starts anything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StartSyncPhase {
    #[default]
    Off,
    Running,
    Done,
    Skipped,
}
/// What the start comparison took over (only when new files were added automatically).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSyncApplied {
    pub added: usize,
    pub renamed: usize,
    pub failed: usize,
}
/// Same shape as the desktop `StartSyncResult` plus `state`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSyncState {
    pub ran: bool,
    pub busy: bool,
    pub state: StartSyncPhase,
    pub new_files: usize,
    pub renamed: usize,
    pub missing_files: usize,
    pub missing_folders: usize,
    pub duplicates: usize,
    pub unreachable_roots: usize,
    pub applied: Option<StartSyncApplied>,
}
/// In-memory result of the start comparison; gone with the process.
#[derive(Debug, Default)]
pub struct StartSyncStore(std::sync::Mutex<StartSyncState>);
impl StartSyncStore {
    pub fn get(&self) -> StartSyncState {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn set(&self, state: StartSyncState) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = state;
    }
}

/// Set while catalog rows still wait for their content hash (`hash_backfill` in the web
/// module). Imports and the "new files" scan are refused meanwhile: without hashes they
/// would take every known file for a new one.
#[derive(Default)]
pub struct HashBackfillGate(std::sync::atomic::AtomicBool);
impl HashBackfillGate {
    pub fn pending(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub fn set(&self, pending: bool) {
        self.0.store(pending, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Limits of the web scan for missing models and duplicates (W2 2.11).
const ISSUE_MAX_ORPHANED: usize = 500;
const ISSUE_MAX_GROUPS: usize = 200;
const ISSUE_MAX_DURATION: std::time::Duration = std::time::Duration::from_secs(20);

pub struct GeometryGate {
    /// The application shares this admission barrier with every catalog route and job.
    pub barrier: crate::barrier::CatalogBarrier,
    workers: Arc<Semaphore>,
    manager_reads: Arc<Semaphore>,
    /// One disk comparison at a time: each walks the roots.
    disk_reports: Arc<Semaphore>,
    report_limits: disk_report::Limits,
    /// Folders the comparison does not walk (the upload folder), like the preview.
    report_excluded: Vec<PathBuf>,
    report_timeout: std::time::Duration,
    issue_limits: catalog_issues::IssueLimits,
    /// Result of the start comparison, written by `disk_start_sync`, read by `run_start_sync`.
    pub start_sync: Arc<StartSyncStore>,
    /// Open hash run after a restore; see [`HashBackfillGate`].
    pub hash_backfill: Arc<HashBackfillGate>,
    admission: Arc<Semaphore>,
    /// STEP parsing in a separate process; disabled unless the server found a working one.
    pub step: Arc<crate::step_worker::StepWorker>,
}
impl Default for GeometryGate {
    fn default() -> Self {
        Self {
            barrier: crate::barrier::CatalogBarrier::default(),
            workers: Arc::new(Semaphore::new(2)),
            manager_reads: Arc::new(Semaphore::new(4)),
            disk_reports: Arc::new(Semaphore::new(1)),
            report_limits: disk_report::Limits::default(),
            report_excluded: Vec::new(),
            report_timeout: DISK_REPORT_TIMEOUT,
            issue_limits: catalog_issues::IssueLimits {
                max_orphaned: ISSUE_MAX_ORPHANED,
                max_groups: ISSUE_MAX_GROUPS,
                max_duration: ISSUE_MAX_DURATION,
            },
            start_sync: Arc::new(StartSyncStore::default()),
            hash_backfill: Arc::new(HashBackfillGate::default()),
            admission: Arc::new(Semaphore::new(10)),
            step: Arc::new(crate::step_worker::StepWorker::disabled()),
        }
    }
}
impl GeometryGate {
    pub fn with_step(step: crate::step_worker::StepWorker) -> Self {
        Self { step: Arc::new(step), ..Self::default() }
    }
    /// The folders the disk comparison leaves out, the same ones the preview refuses.
    pub fn with_report_excluded(mut self, excluded: Vec<PathBuf>) -> Self {
        self.report_excluded = excluded;
        self
    }
    pub(crate) async fn acquire(&self) -> Result<(OwnedSemaphorePermit, OwnedSemaphorePermit), Box<Response>> {
        let admission = self.admission.clone().try_acquire_owned().map_err(|_| {
            error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::WebTooManyRequests, &[])
        })?;
        let worker = self.workers.clone().acquire_owned().await.map_err(|_| {
            error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::GeometryUnavailable, &[])
        })?;
        Ok((admission, worker))
    }
    #[cfg(test)]
    pub(crate) fn manager_reads_available(&self) -> usize {
        self.manager_reads.available_permits()
    }
    /// Admission for catalog reads that load and match many rows: at most four run at
    /// once, the rest get a repeatable answer. The caller moves the permit into the
    /// blocking work so it is held until that work really ends, not until the HTTP
    /// request goes away.
    pub(crate) fn manager_read(&self) -> Result<OwnedSemaphorePermit, Box<Response>> {
        self.manager_reads.clone().try_acquire_owned().map_err(|_| {
            let mut r = *error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::WebTooManyRequests, &[]);
            r.headers_mut().insert("retry-after", "1".parse().unwrap());
            Box::new(r)
        })
    }
}
/// An answer whose `message` is a registered code; the interface owns the wording.
fn error(status: StatusCode, code: ErrorCode, params: &[String]) -> Box<Response> {
    Box::new(crate::web::error_coded(status.as_u16(), code, params))
}
/// Whether `message` is a code the interface can translate. A read error that
/// claims to be expected but carries anything else would put free text on the wire.
fn is_wire_code(message: &str) -> bool {
    ErrorCode::ALL.iter().any(|code| code.as_str() == message) || LEGACY_CODES.contains(&message)
}
pub(crate) fn read_error(e: ReadError) -> Response {
    // A busy database is a condition of the server, not of the request: the answer is
    // a fixed code and never the database's own text.
    if e.busy {
        return *error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::WebServerBusy, &[]);
    }
    let status = if e.code == Some(GeometryErrorCode::NotFound) {
        StatusCode::NOT_FOUND
    } else if e.expected {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    if status == StatusCode::INTERNAL_SERVER_ERROR || !is_wire_code(&e.message) {
        // Read errors contain catalog/SQLite diagnostics, never authentication inputs.
        eprintln!(
            "Interner Lesefehler: {}",
            e.message
                .chars()
                .take(1024)
                .flat_map(char::escape_default)
                .collect::<String>()
        );
        return *error(StatusCode::INTERNAL_SERVER_ERROR, ErrorCode::WebServerInternal, &[]);
    }
    (status, Json(e)).into_response()
}
fn encode(value: impl Serialize) -> Result<Response, ReadError> {
    serde_json::to_value(value)
        .map(|v| Json(v).into_response())
        .map_err(|_| ReadError::from("Antwort konnte nicht kodiert werden"))
}
fn id(args: &Value, key: &str) -> Result<String, Box<Response>> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| s.parse::<i64>().is_ok())
        .map(str::to_owned)
        .ok_or_else(|| {
            error(StatusCode::BAD_REQUEST, ErrorCode::ReqInvalidIds, &[])
        })
}
const MAX_FILE_IDS: usize = 500;

fn ids(args: &Value, images: bool) -> Result<Vec<String>, Box<Response>> {
    let values = args
        .get("ids")
        .and_then(Value::as_array)
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, ErrorCode::ReqInvalidIds, &[]))?;
    if images && values.len() > 200 {
        return Err(error(StatusCode::BAD_REQUEST, ErrorCode::ReqIdsTooMany, &["200".to_string()]));
    }
    // Each ID loads a full file record; an unbounded list would hold the
    // database lock and build an arbitrarily large response.
    if !images && values.len() > MAX_FILE_IDS {
        return Err(error(StatusCode::BAD_REQUEST, ErrorCode::ReqIdsTooMany, &["500".to_string()]));
    }
    values
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| s.parse::<i64>().is_ok())
                .map(str::to_owned)
                .ok_or_else(|| {
                    error(StatusCode::BAD_REQUEST, ErrorCode::ReqInvalidIds, &[])
                })
        })
        .collect()
}

/// Encoded picture bytes after which `list_file_images` defers the remaining entries.
const IMAGE_RESPONSE_BUDGET: usize = 24 * 1024 * 1024;
// The two list commands that answer with every row of a table. Measured with 100 000 rows:
// ~71 MB / ~580 MB RSS (summaries), ~90 MB / ~690 MB RSS (trash). Past these counts the
// answer is refused (`webLimitReached`) before any row is loaded; ordinary catalogs are
// unaffected (50 000 summaries are ~36 MB). Server path only, the desktop calls the core directly.
const MAX_LIST_SUMMARIES: i64 = 50_000;
const MAX_LIST_TRASH: i64 = 20_000;
fn guard_row_count(conn: &rusqlite::Connection, count_sql: &str, max: i64) -> Result<(), ReadError> {
    let count: i64 = conn.query_row(count_sql, [], |r| r.get(0)).map_err(|e| ReadError::from(db::error::DbError::from(e)))?;
    if count > max {
        return Err(ReadError::coded_with(ErrorCode::WebLimitReached, [max]));
    }
    Ok(())
}
pub const READ_COMMANDS: &[&str] = &[
    "list_source_dirs",
    "list_source_folders",
    "scan_source_folders",
    "get_source_check_on_start",
    "scan_catalog_issues",
    "get_sync_on_start",
    "get_sync_auto_add",
    "run_start_sync",
    "get_printer_link_enabled",
    "list_printer_connections",
    "list_open_printer_jobs",
    "preview_printer_job",
    "get_printer_job_thumbnail",

    "folder_removal_summary",
    "folder_trash_summary",
    "empty_trash_summary",
    "get_import_state",
    "get_import_result",
    "list_file_summaries",
    "list_all_file_tags",
    "list_files_by_ids",
    "list_tag_counts",
    "list_print_log_entries",
    "list_folders",
    "list_collections",
    "list_collection_files",
    "list_bundles",
    "list_trash",
    "list_file_images",
    "get_model_geometry",
    "get_app_version",
    "is_preview_build",
    "has_step_preview",
    "list_filament_spools",
    "list_printers",
    "list_printer_history",
    "get_print_statistics",
    "get_last_printer_for_file",
    "check_filament",
    "get_filament_spool_image",
    "get_auto_tagging",
    "get_print_technology",
    "list_suggested_auto_tags",
    "list_blocked_auto_tags",
    "list_companions",
    "companion_image_data",
    "disk_sync_report",
    "read_plate_overview",
];

/// Companion refusals reach the client as plain statements: no path, no SQLite text.
pub(crate) fn companion_error(e: companion_read::RowError) -> ReadError {
    use companion_read::RowError as R;
    match e {
        R::TooLarge => ReadError::coded(ErrorCode::CompanionTooLarge).with_code(GeometryErrorCode::TooLarge),
        // Everything else looks the same from outside: no hint which check failed.
        _ => ReadError::coded(ErrorCode::CompanionNotFound).with_code(GeometryErrorCode::NotFound),
    }
}

// New summaries intentionally expose no server paths or SQLite error text.
fn trash_summary(conn: &katalog_core::write::Connection, roots: &[PathBuf]) -> Result<Value, ReadError> {
    // Entries a permanent delete would refuse are counted apart and not offered for emptying.
    let summary = katalog_core::fsops::trash_summary(conn, roots)
        .map_err(|e| match e {
            katalog_core::fsops::Error::Changed => ReadError::from("Ungültiger Papierkorb-Zeitstempel"),
            _ => ReadError::from("Zusammenfassung nicht verfügbar"),
        })?;
    Ok(json!({
        "count": summary.count,
        "blockedByCompanions": summary.blocked_by_companions,
        "companionCount": summary.companion_count,
        "looseCompanionCount": summary.loose_companion_count,
        "looseCompanionBytes": summary.loose_companion_bytes,
        "bytes": summary.bytes,
        "maxDeletedAt": summary.max_deleted_at,
    }))
}
/// Same counts and rules as the desktop dialog. A folder outside the configured roots is not
/// something the web manages, so it is answered like an unknown one and its disk is not scanned.
fn folder_trash_summary(
    conn: &katalog_core::write::Connection,
    id: &str,
    roots: &[PathBuf],
) -> Result<Option<Value>, ReadError> {
    use katalog_core::folder_trash::{folder_trash_summary_with_conn, StdFolderTrashFs};
    let path: Option<String> = conn
        .query_row("SELECT path FROM folders WHERE id=?1", [id], |r| r.get(0))
        .ok();
    let Some(path) = path else { return Ok(None) };
    if !roots.iter().any(|root| std::path::Path::new(&path).starts_with(root)) {
        return Ok(None);
    }
    match folder_trash_summary_with_conn(&StdFolderTrashFs, conn, id, &[]) {
        Ok(summary) => Ok(Some(json!(summary))),
        Err(_) => Err(ReadError::coded(ErrorCode::WebServerInternal)),
    }
}
fn folder_summary(
    conn: &katalog_core::write::Connection,
    id: i64,
) -> Result<Option<Value>, ReadError> {
    let internal = |_| ReadError::from("Zusammenfassung nicht verfügbar");
    let mut stmt = conn
        .prepare("SELECT name FROM folders WHERE id=?1")
        .map_err(internal)?;
    let mut rows = stmt
        .query_map([id], |row| row.get::<_, String>(0))
        .map_err(internal)?;
    let Some(name) = rows.next().transpose().map_err(internal)? else {
        return Ok(None);
    };
    let (folder_count,model_count):(i64,i64)=conn.query_row(
        &format!("{} SELECT (SELECT COUNT(*) FROM subtree)-1,(SELECT COUNT(*) FROM files WHERE folder_id IN (SELECT id FROM subtree) AND deleted_at IS NULL)",db::FOLDER_SUBTREE_CTE),
        [id],|row|Ok((row.get(0)?,row.get(1)?))).map_err(internal)?;
    Ok(Some(
        json!({"name":name,"subfolderCount":folder_count,"modelCount":model_count}),
    ))
}

/// Missing models and duplicates for the web: only rows the probe positively judged count, the
/// answer carries full DTOs for the hits only (never the whole catalog with its pictures).
fn scan_issues(conn: &rusqlite::Connection, roots: &[PathBuf], limits: catalog_issues::IssueLimits) -> Result<Value, ReadError> {
    // Same root numbering as `disk_sync_report`, so `root` means the same in both answers.
    let ordered = disk_report::ordered_roots(roots);
    let probe = catalog_issues::RootsMissingProbe::new(&ordered);
    let issues = catalog_issues::scan(conn, &probe, limits).map_err(|_| ReadError::from("Abgleich nicht verfügbar"))?;
    let ids: Vec<i64> = issues.orphaned.iter().chain(issues.duplicate_groups.iter().flatten()).copied().collect();
    // Chunked: a few huge duplicate groups must not exceed SQLite's variable limit.
    let mut files = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(500) {
        files.extend(db::list_files_by_ids(conn, chunk).map_err(ReadError::from)?);
    }
    let spools = db::list_filament_spools(conn).map_err(ReadError::from)?;
    let mut by_id: std::collections::HashMap<i64, read::ModelFileDto> =
        files.into_iter().map(|f| (f.id, read::to_dto(f, &spools))).collect();
    // A row that vanished since the scan (deleted meanwhile) is left out, not an error.
    let orphaned: Vec<_> = issues.orphaned.iter().filter_map(|id| by_id.remove(id)).collect();
    let by_listed: std::collections::HashSet<i64> = orphaned.iter().filter_map(|m| m.id.parse().ok()).collect();
    let groups: Vec<Vec<_>> = issues
        .duplicate_groups
        .iter()
        .map(|g| g.iter().filter_map(|id| by_id.remove(id)).collect::<Vec<_>>())
        .filter(|g| g.len() >= 2)
        .collect();
    let mut answer = json!({"orphaned": orphaned, "duplicateGroups": groups, "truncated": issues.truncated});
    // Only present when there is something to say (older clients ignore both).
    if !issues.unclear.is_empty() {
        answer["unclear"] = issues
            .unclear
            .iter()
            .map(|u| json!({"path": u.place.relative, "root": u.place.root, "reason": u.reason}))
            .collect();
    }
    // Display names only (like the report's `roots[].name`), indexed like `root`; never absolute paths.
    answer["roots"] = json!(disk_report::root_names(&ordered));
    // Always present, also empty: a present field tells the client the folders were verified here.
    answer["missingFolders"] = issues
        .missing_folders
        .iter()
        .map(|f| {
            let models: Vec<String> =
                f.models.iter().filter(|id| by_listed.contains(id)).map(|id| id.to_string()).collect();
            json!({"relativePath": f.place.relative, "root": f.place.root, "models": models})
        })
        .collect();
    Ok(answer)
}

pub async fn invoke(
    db_path: PathBuf,
    roots: Vec<PathBuf>,
    command: String,
    args: Value,
    gate: Arc<GeometryGate>,
) -> Response {
    if !READ_COMMANDS.contains(&command.as_str()) {
        return *error(StatusCode::NOT_FOUND, ErrorCode::WebCommandUnavailable, &[]);
    }
    if !args.is_object() {
        return *error(StatusCode::BAD_REQUEST, ErrorCode::ReqInvalidArguments, &[]);
    }
    if matches!(
        command.as_str(),
        "folder_removal_summary"
            | "folder_trash_summary"
            | "empty_trash_summary"
            | "disk_sync_report"
            | "scan_catalog_issues"
            | "get_sync_on_start"
            | "get_sync_auto_add"
            | "run_start_sync"
    ) {
        let allowed: &[&str] = if command == "folder_removal_summary" || command == "folder_trash_summary" {
            &["folderId"]
        } else {
            &[]
        };
        if args
            .as_object()
            .unwrap()
            .keys()
            .any(|key| !allowed.contains(&key.as_str()))
        {
            return *error(StatusCode::UNPROCESSABLE_ENTITY, ErrorCode::ReqInvalidArguments, &[]);
        }
    }
    let file_id = match command.as_str() {
        "folder_removal_summary" | "folder_trash_summary" => match id(&args, "folderId") {
            Ok(value) => Some(value),
            Err(error) => return *error,
        },
        "list_print_log_entries" | "get_model_geometry" | "read_plate_overview" | "get_last_printer_for_file" | "list_suggested_auto_tags" | "list_companions" => {
            match id(&args, "fileId") {
                Ok(v) => Some(v),
                Err(e) => return *e,
            }
        }
        "list_printer_history" => match id(&args, "printerId") {
            Ok(v) => Some(v),
            Err(e) => return *e,
        },
        // Optional: missing or null means all printers.
        "get_print_statistics" => match args.get("printerId") {
            None | Some(Value::Null) => None,
            Some(_) => match id(&args, "printerId") {
                Ok(v) => Some(v),
                Err(e) => return *e,
            },
        },
        "companion_image_data" => match id(&args, "companionId") {
            Ok(v) => Some(v),
            Err(e) => return *e,
        },
        "get_filament_spool_image" => match id(&args, "id") {
            Ok(v) => Some(v),
            Err(e) => return *e,
        },
        "list_collection_files" => match id(&args, "collectionId") {
            Ok(v) => Some(v),
            Err(e) => return *e,
        },
        _ => None,
    };
    let file_ids = match command.as_str() {
        "list_file_images" | "list_files_by_ids" => match ids(&args, command == "list_file_images")
        {
            Ok(v) => v,
            Err(e) => return *e,
        },
        _ => Vec::new(),
    };
    match command.as_str() {
        "get_app_version" => return Json(json!(env!("CARGO_PKG_VERSION"))).into_response(),
        "is_preview_build" => return Json(json!(false)).into_response(),
        "has_step_preview" => return Json(json!(gate.step.available())).into_response(),
        _ => {}
    }
    // The file-management UI asks for the individually removed files as well; the plain listing stays as it was.
    let include_removed = args.get("includeRemoved") == Some(&Value::Bool(true));
    let check_ids = if command == "check_filament" {
        let Some(values) = args.get("fileIds").and_then(Value::as_array) else {
            return *error(StatusCode::BAD_REQUEST, ErrorCode::ReqInvalidIds, &[]);
        };
        if values.len() > 1000 {
            return *error(StatusCode::UNPROCESSABLE_ENTITY, ErrorCode::ReqIdsTooMany, &["1000".to_string()]);
        }
        let Some(ids) = values
            .iter()
            .map(|v| v.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
        else {
            return *error(StatusCode::BAD_REQUEST, ErrorCode::ReqInvalidIds, &[]);
        };
        ids
    } else {
        Vec::new()
    };
    // Only the plate overview reads the flag; absent means "no pictures".
    let with_previews = match args.get("withPreviews") {
        None => false,
        Some(Value::Bool(flag)) => *flag,
        Some(_) => {
            return *error(StatusCode::BAD_REQUEST, ErrorCode::ReqInvalidArguments, &[]);
        }
    };
    let manager_permit = if matches!(
        command.as_str(),
        "list_filament_spools"
            | "list_printers"
            | "list_printer_history"
            | "get_print_statistics"
            | "get_last_printer_for_file"
            | "check_filament"
            | "get_filament_spool_image"
            | "companion_image_data"
            | "list_companions"
            | "disk_sync_report"
            | "scan_catalog_issues"
    ) {
        match gate.manager_read() {
            Ok(p) => Some(p),
            Err(r) => return *r,
        }
    } else {
        None
    };
    let is_scan = matches!(command.as_str(), "disk_sync_report" | "scan_catalog_issues");
    let report_permit = if is_scan {
        match gate.disk_reports.clone().try_acquire_owned() {
            Ok(p) => Some(p),
            Err(_) => {
                let mut r = *error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::RescanRunning, &[]);
                r.headers_mut().insert("retry-after", "5".parse().unwrap());
                return r;
            }
        }
    } else {
        None
    };
    let catalog_guard = match gate.barrier.reader() { Ok(g) => g, Err(_) => return crate::barrier::maintenance() };
    // The start comparison's stored result: memory only, no catalog access.
    if command == "run_start_sync" {
        drop(catalog_guard);
        return Json(gate.start_sync.get()).into_response();
    }
    if is_scan {
        let (limits, wait, excluded) = (gate.report_limits, gate.report_timeout, gate.report_excluded.clone());
        let (issue_limits, issues) = (gate.issue_limits, command == "scan_catalog_issues");
        let (tx, rx) = tokio::sync::oneshot::channel();
        // A thread of its own, not the blocking pool: the scan may never return. The permits
        // move into it on purpose and are released only when it ends, so a stuck scan keeps
        // blocking further ones (single flight, manager gate) instead of piling up new threads.
        let started = std::thread::Builder::new().name("disk-report".into()).spawn(move || {
            let _held = (catalog_guard, manager_permit, report_permit);
            let outcome = (|| -> Result<Value, ReadError> {
                let conn = db::open_read_only(&db_path).map_err(ReadError::from)?;
                if issues {
                    return scan_issues(&conn, &roots, issue_limits);
                }
                // The database is only read for the copy; the disk scan runs without it.
                let snapshot = disk_report::snapshot(&conn).map_err(|_| ReadError::from("Abgleich nicht verfügbar"))?;
                drop(conn);
                serde_json::to_value(disk_report::build_report_with(&snapshot, &roots, disk_report::ReportOptions { limits, excluded: &excluded }))
                    .map_err(|_| ReadError::from("Antwort konnte nicht kodiert werden"))
            })();
            let _ = tx.send(outcome);
        });
        if started.is_err() {
            return *error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::RescanUnavailable, &[]);
        }
        return match tokio::time::timeout(wait, rx).await {
            Ok(Ok(Ok(value))) => Json(value).into_response(),
            Ok(Ok(Err(e))) => read_error(e),
            Ok(Err(_)) => *error(StatusCode::INTERNAL_SERVER_ERROR, ErrorCode::WebServerInternal, &[]),
            Err(_) => *error(StatusCode::GATEWAY_TIMEOUT, ErrorCode::RescanTimeout, &[]),
        };
    }
    // A STEP preview has its own small number of places and must hear "busy" at once instead of
    // queueing behind 3MF parses, so its type is known before the parser gate is waited for.
    if command == "get_model_geometry" && gate.step.available() {
        let (lookup_db, lookup_id) = (db_path.clone(), file_id.clone().unwrap());
        let source = tokio::task::spawn_blocking(move || {
            let conn = db::open_read_only(&lookup_db).map_err(ReadError::from)?;
            read::load_geometry_source(&conn, &lookup_id)
        })
        .await;
        match source {
            Ok(Ok(source)) if source.file_type == katalog_core::db::models::FileType::Stp => {
                let Ok(slot) = gate.step.try_slot() else { return step_busy() };
                let step = gate.step.clone();
                let result = tokio::task::spawn_blocking(move || {
                    // Place and catalog guard live with the blocking work: a dropped request does
                    // not free a place while its worker still runs.
                    let _held = (catalog_guard, slot);
                    step_geometry(&step, &source.path, &roots)
                })
                .await;
                return match result {
                    Ok(Ok(response)) => response,
                    Ok(Err(e)) => read_error(e),
                    Err(_) => *error(StatusCode::INTERNAL_SERVER_ERROR, ErrorCode::WebServerInternal, &[]),
                };
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return read_error(e),
            Err(_) => return *error(StatusCode::INTERNAL_SERVER_ERROR, ErrorCode::WebServerInternal, &[]),
        }
    }
    // Both open and parse a 3MF, so both wait for the parser gate.
    let permits = if matches!(command.as_str(), "get_model_geometry" | "read_plate_overview") {
        match gate.acquire().await {
            Ok(p) => Some(p),
            Err(e) => return *e,
        }
    } else {
        None
    };
    let result = tokio::task::spawn_blocking(move || -> Result<Response, ReadError> {
        // The permits belong to this closure: cancelling HTTP does not release a running parser.
        let _catalog_guard = catalog_guard;
        let _permits = permits;
        let _manager_permit = manager_permit;
        let _report_permit = report_permit;
        let conn = db::open_read_only(&db_path).map_err(ReadError::from)?;
        match command.as_str() {
            "list_filament_spools" => encode(read::manager::list_filament_spools_web(&conn)?),
            "list_printers" => encode(read::manager::list_printers_with_conn(&conn)?),
            "list_printer_history" => encode(read::manager::list_printer_history_limit(
                &conn,
                file_id.unwrap().parse().unwrap(),
                500,
            )?),
            "get_print_statistics" => encode(read::manager::print_statistics_rows(
                &conn,
                file_id.map(|v| v.parse().unwrap()),
                read::manager::PRINT_STATS_MAX_ROWS,
            )?),
            "get_last_printer_for_file" => {
                encode(read::manager::get_last_printer_for_file_with_conn(
                    &conn,
                    file_id.unwrap().parse().unwrap(),
                )?)
            }
            "get_filament_spool_image" => encode(read::manager::get_filament_spool_image(
                &conn,
                file_id.unwrap().parse().unwrap(),
            )?),
            "check_filament" => encode(read::manager::check_filament_with_conn(&conn, &check_ids)?),

            "list_companions" => encode(
                companion_read::list_for_web_with(&conn, file_id.unwrap().parse().unwrap(), &roots, include_removed).map_err(companion_error)?,
            ),
            "companion_image_data" => {
                let opened = companion_read::open_for_web(
                    &conn,
                    file_id.unwrap().parse().unwrap(),
                    &roots,
                    companion_read::MAX_PREVIEW_BYTES,
                );
                drop(conn);
                let (row, kind, mut file, head) = opened.map_err(companion_error)?;
                if kind != katalog_core::companions::CompanionKind::Image {
                    return Err(companion_error(companion_read::RowError::Invalid));
                }
                encode(companion_read::image_data_uri(&mut file, head, &row.name).map_err(companion_error)?)
            }
            "list_file_summaries" => {
                guard_row_count(&conn, "SELECT COUNT(*) FROM files WHERE deleted_at IS NULL", MAX_LIST_SUMMARIES)?;
                encode(read::list_file_summaries(&conn)?)
            }
            "list_all_file_tags" => encode(read::list_all_file_tags(&conn)?),
            "list_files_by_ids" => encode(read::list_files_by_ids(&conn, file_ids)?),
            "list_tag_counts" => encode(read::list_tag_counts(&conn)?),
            "get_sync_on_start" => encode(disk_sync_settings::flag(&conn, disk_sync_settings::SETTING_ON_START).map_err(ReadError::from)?),
            "get_sync_auto_add" => encode(disk_sync_settings::flag(&conn, disk_sync_settings::SETTING_AUTO_ADD).map_err(ReadError::from)?),
            "get_auto_tagging" => encode(read::get_auto_tagging(&conn)?),
            "get_print_technology" => encode(read::get_print_technology(&conn)?),
            "list_blocked_auto_tags" => encode(read::list_blocked_auto_tags(&conn)?),
            "list_suggested_auto_tags" => encode(read::suggested_auto_tags_with_conn(
                &conn,
                file_id.unwrap().parse().unwrap(),
            )?),
            "list_print_log_entries" => {
                encode(read::list_print_log_entries(&conn, file_id.unwrap())?)
            }
            "list_folders" => {
                let mut folders = read::list_folders(&conn)?;
                drop(conn);
                crate::folder_missing::annotate(&mut folders, &roots);
                encode(folders)
            }
            "list_collections" => encode(read::list_collections(&conn)?),
            "list_bundles" => encode(read::list_bundles(&conn)?),
            "list_collection_files" => {
                encode(read::list_collection_files(&conn, file_id.unwrap())?)
            }
            "list_trash" => {
                guard_row_count(&conn, "SELECT COUNT(*) FROM files WHERE deleted_at IS NOT NULL", MAX_LIST_TRASH)?;
                let mut value = serde_json::to_value(read::list_trash(&conn)?).map_err(|_| ReadError::coded(ErrorCode::WebServerInternal))?;
                let desktop_ids = katalog_core::fsops::desktop_trash_ids(&conn)
                    .map_err(|_| ReadError::coded(ErrorCode::WebServerInternal))?;
                if let Some(items) = value.as_array_mut() {
                    for item in items {
                        if let Some(id) = item["id"].as_str().and_then(|id| id.parse::<i64>().ok()) {
                            if desktop_ids.contains(&id) {
                                item["desktopTrash"] = serde_json::Value::Bool(true);
                            }
                        }
                    }
                }
                encode(value)
            },
            "empty_trash_summary" => encode(trash_summary(&conn, &roots)?),
            "folder_trash_summary" => {
                match folder_trash_summary(&conn, &file_id.unwrap(), &roots) {
                    Ok(Some(summary)) => encode(summary),
                    Ok(None) => Ok(*error(StatusCode::NOT_FOUND, ErrorCode::FolderNotFound, &[])),
                    Err(e) => Err(e),
                }
            }
            "folder_removal_summary" => {
                match folder_summary(&conn, file_id.unwrap().parse().unwrap())? {
                    Some(summary) => encode(summary),
                    None => Ok(*error(StatusCode::NOT_FOUND, ErrorCode::FolderNotFound, &[])),
                }
            }
            "list_file_images" => {
                let numeric = file_ids
                    .iter()
                    .map(|s| s.parse::<i64>().expect("validated ID"))
                    .collect::<Vec<_>>();
                let images = read::load_file_images(&conn, &numeric)?;
                drop(conn);
                // Up to 200 full-size covers could add up to hundreds of MiB in one answer.
                encode(read::encode_file_images_budgeted(images, IMAGE_RESPONSE_BUDGET))
            }
            "get_model_geometry" => {
                let source = read::load_geometry_source(&conn, &file_id.unwrap())?;
                drop(conn);
                // STEP with a working worker was answered above; without one it takes the common
                // path, which answers "unsupported". The read-only web has no configured budgets;
                // the defaults are the largest values the configuration can set.
                let bytes = read::get_model_geometry_with_limits(
                    &source.path,
                    Some(&roots),
                    Some(katalog_core::import::files::WebImportLimits::default()),
                )?;
                Ok(([("content-type", "application/octet-stream")], bytes).into_response())
            }
            "read_plate_overview" => {
                let source = read::load_geometry_source(&conn, &file_id.unwrap())?;
                drop(conn);
                encode(read::read_plate_overview(&source.path, source.file_type, Some(&roots), with_previews)?)
            }
            _ => unreachable!("allowlist checked"),
        }
    })
    .await;
    match result {
        Ok(Ok(response)) => response,
        Ok(Err(e)) => read_error(e),
        Err(_) => *error(StatusCode::INTERNAL_SERVER_ERROR, ErrorCode::WebServerInternal, &[]),
    }
}

/// A STEP preview from the worker; the caller holds a worker place. The file is admitted and
/// opened here; the worker only gets the handle.
fn step_geometry(
    step: &crate::step_worker::StepWorker,
    path: &std::path::Path,
    roots: &[PathBuf],
) -> Result<Response, ReadError> {
    let file = read::open_geometry_file(path, roots)?;
    let bytes = step
        .run(crate::step_worker::Mode::Geometry, &file, step.limits().wall)
        .map_err(step_read_error)?;
    Ok(([("content-type", "application/octet-stream")], bytes).into_response())
}
/// Too many STEP previews at once: repeatable, with a hint when to try again.
pub(crate) fn step_busy() -> Response {
    let mut r = *error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::WebTooManyRequests, &[]);
    r.headers_mut().insert(
        "retry-after",
        crate::step_worker::StepWorker::RETRY_AFTER_SECONDS.to_string().parse().unwrap(),
    );
    r
}
/// Every worker outcome as a registered code; nothing of the parser's own text reaches the client.
pub(crate) fn step_read_error(e: crate::step_worker::WorkerError) -> ReadError {
    use crate::step_worker::WorkerError as W;
    match e {
        W::TooLarge => ReadError::coded_with(ErrorCode::FileTooLarge, [crate::step_worker::MAX_INPUT_BYTES.to_string()])
            .with_code(GeometryErrorCode::TooLarge),
        W::TooComplex => ReadError::coded(ErrorCode::PreviewTooLarge).with_code(GeometryErrorCode::TooLarge),
        W::NoGeometry => ReadError::coded(ErrorCode::GeometryEmpty).with_code(GeometryErrorCode::Unreadable),
        // For the viewer a crash or a broken answer means the same as a refused file: no preview.
        W::Unreadable | W::Crashed | W::Protocol => {
            ReadError::coded(ErrorCode::GeometryUnavailable).with_code(GeometryErrorCode::Unreadable)
        }
        W::Unavailable | W::NoIsolation | W::Busy => ReadError::coded_with(ErrorCode::FileFormatUnsupported, ["step"])
            .with_code(GeometryErrorCode::Unsupported),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use sha2::{Digest, Sha256};
    struct Catalog {
        dir: PathBuf,
        db: PathBuf,
        id: String,
        collection: String,
    }
    impl Drop for Catalog {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    fn catalog() -> Catalog {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).unwrap();
        let dir =
            std::env::temp_dir().join(format!("server-invoke-{:x}", u128::from_le_bytes(random)));
        std::fs::create_dir(&dir).unwrap();
        let db = dir.join("catalog.db");
        let conn = db::connect(&db).unwrap();
        let path = dir.join("one.stl");
        let mut stl = vec![0u8; 80];
        stl.extend(1u32.to_le_bytes());
        stl.extend([0u8; 12]);
        for c in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
            stl.extend(c.to_le_bytes());
        }
        stl.extend([0u8; 2]);
        std::fs::write(&path, stl).unwrap();
        let folder =
            db::insert_folder_with_parent(&conn, "Ordner", None, dir.to_str().unwrap()).unwrap();
        let id = db::test_insert_minimal_file(&conn, path.to_str().unwrap(), Some(folder)).unwrap();
        conn.execute("UPDATE files SET file_type='stl', thumbnail_png=x'010203', render_snapshot_png=x'04', custom_image_png=x'05'", []).unwrap();
        db::add_tag_to_file(&conn, id, "Tag").unwrap();
        let collection = db::create_collection(&conn, "Sammlung", "now").unwrap();
        db::add_file_to_collection(&conn, collection, id, 0).unwrap();
        db::insert_print_log_entry(
            &conn,
            &db::models::NewPrintLogEntry {
                file_id: id,
                printed_at: "2026-10-16".into(),
                note: Some("Druck".into()),
                photo_png: Some(vec![1, 2]),
            },
        )
        .unwrap();
        let trash =
            db::test_insert_minimal_file(&conn, "/missing/original.stl", Some(folder)).unwrap();
        db::soft_delete_file(&conn, trash, Some(path.to_str().unwrap()), "2026-10-16").unwrap();
        Catalog {
            dir,
            db,
            id: id.to_string(),
            collection: collection.to_string(),
        }
    }
    async fn call(c: &Catalog, command: &str, args: Value) -> Response {
        invoke(
            c.db.clone(),
            vec![c.dir.clone()],
            command.into(),
            args,
            Arc::new(GeometryGate::default()),
        )
        .await
    }
    #[tokio::test]
    async fn companion_reads_share_the_manager_gate() {
        let c = catalog();
        let gate = Arc::new(GeometryGate::default());
        let held: Vec<_> = (0..4).map(|_| gate.manager_reads.clone().try_acquire_owned().unwrap()).collect();
        for (command, args) in [("list_companions", json!({"fileId": c.id})), ("companion_image_data", json!({"companionId": "1"}))] {
            let r = invoke(c.db.clone(), vec![c.dir.clone()], command.into(), args, gate.clone()).await;
            assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE, "{command}");
        }
        drop(held);
        let r = invoke(c.db.clone(), vec![c.dir.clone()], "list_companions".into(), json!({"fileId": c.id}), gate).await;
        assert_eq!(r.status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn only_one_disk_report_runs_at_a_time_and_it_shares_the_manager_gate() {
        let c = catalog();
        let gate = Arc::new(GeometryGate::default());
        let running = gate.disk_reports.clone().try_acquire_owned().unwrap();
        let r = invoke(c.db.clone(), vec![c.dir.clone()], "disk_sync_report".into(), json!({}), gate.clone()).await;
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(r.headers().contains_key("retry-after"));
        drop(running);
        let held: Vec<_> = (0..4).map(|_| gate.manager_reads.clone().try_acquire_owned().unwrap()).collect();
        let r = invoke(c.db.clone(), vec![c.dir.clone()], "disk_sync_report".into(), json!({}), gate.clone()).await;
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
        drop(held);
        let r = invoke(c.db.clone(), vec![c.dir.clone()], "disk_sync_report".into(), json!({}), gate).await;
        assert_eq!(r.status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn a_scan_that_hangs_is_answered_with_a_timeout_and_keeps_its_permits_until_it_ends() {
        let c = catalog();
        let hook: &'static (dyn Fn(&std::path::Path) + Sync) = Box::leak(Box::new(|_: &std::path::Path| std::thread::sleep(std::time::Duration::from_millis(700))));
        let gate = Arc::new(GeometryGate {
            report_timeout: std::time::Duration::from_millis(150),
            report_limits: disk_report::Limits { walk_hook: Some(hook), ..Default::default() },
            ..Default::default()
        });
        let started = std::time::Instant::now();
        let r = invoke(c.db.clone(), vec![c.dir.clone()], "disk_sync_report".into(), json!({}), gate.clone()).await;
        assert_eq!(r.status(), StatusCode::GATEWAY_TIMEOUT);
        assert!(started.elapsed() < std::time::Duration::from_millis(600));
        let text = String::from_utf8(body(r).await).unwrap();
        assert!(text.contains("\"rescanTimeout\"") && !text.contains(c.dir.to_str().unwrap()), "{text}");
        // The scan thread still owns the single-flight permit and one manager permit.
        assert_eq!(gate.disk_reports.available_permits(), 0);
        assert_eq!(gate.manager_reads.available_permits(), 3);
        assert_eq!(gate.barrier.snapshot().readers, 1);
        let again = invoke(c.db.clone(), vec![c.dir.clone()], "disk_sync_report".into(), json!({}), gate.clone()).await;
        assert_eq!(again.status(), StatusCode::SERVICE_UNAVAILABLE);
        for _ in 0..100 {
            if gate.disk_reports.available_permits() == 1 && gate.manager_reads.available_permits() == 4 && gate.barrier.snapshot().readers == 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("permits were never released");
    }

    #[tokio::test]
    async fn a_writer_commits_while_the_disk_scan_is_running() {
        let c = catalog();
        let db = c.db.clone();
        let committed: &'static std::sync::atomic::AtomicBool = Box::leak(Box::new(std::sync::atomic::AtomicBool::new(false)));
        let hook: &'static (dyn Fn(&std::path::Path) + Sync) = Box::leak(Box::new(move |_: &std::path::Path| {
            let writer = db::connect(&db).unwrap();
            writer.execute("UPDATE files SET path = path", []).unwrap();
            committed.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
        let gate = GeometryGate { report_limits: disk_report::Limits { walk_hook: Some(hook), ..Default::default() }, ..Default::default() };
        let r = invoke(c.db.clone(), vec![c.dir.clone()], "disk_sync_report".into(), json!({}), Arc::new(gate)).await;
        assert_eq!(r.status(), StatusCode::OK);
        assert!(committed.load(std::sync::atomic::Ordering::SeqCst), "the walk never ran");
    }

    /// A catalog with: two live copies of one hash, a missing model, a model that is only
    /// "unclear" (directory without permission is not available as root), and one outside the roots.
    struct Issues {
        dir: PathBuf,
        db: PathBuf,
    }
    impl Drop for Issues {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.dir.parent().unwrap());
        }
    }
    fn issues_catalog() -> Issues {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).unwrap();
        let base = std::env::temp_dir().join(format!("server-issues-{:x}", u128::from_le_bytes(random)));
        let (dir, outside) = (base.join("root"), base.join("outside"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let (dir, outside) = (dir.canonicalize().unwrap(), outside.canonicalize().unwrap());
        let db = base.join("catalog.db");
        let conn = db::connect(&db).unwrap();
        for name in ["a.stl", "b.stl"] {
            std::fs::write(dir.join(name), b"same").unwrap();
            let id = db::test_insert_minimal_file(&conn, dir.join(name).to_str().unwrap(), None).unwrap();
            conn.execute("UPDATE files SET content_hash='h1' WHERE id=?1", [id]).unwrap();
        }
        db::test_insert_minimal_file(&conn, dir.join("gone.stl").to_str().unwrap(), None).unwrap();
        std::fs::write(outside.join("o.stl"), b"x").unwrap();
        let o = db::test_insert_minimal_file(&conn, outside.join("o.stl").to_str().unwrap(), None).unwrap();
        conn.execute("UPDATE files SET content_hash='h2' WHERE id=?1", [o]).unwrap();
        let o2 = db::test_insert_minimal_file(&conn, outside.join("missing.stl").to_str().unwrap(), None).unwrap();
        conn.execute("UPDATE files SET content_hash='h2' WHERE id=?1", [o2]).unwrap();
        Issues { dir, db }
    }
    async fn scan(i: &Issues, gate: Arc<GeometryGate>) -> Response {
        invoke(i.db.clone(), vec![i.dir.clone()], "scan_catalog_issues".into(), json!({}), gate).await
    }
    async fn json_body(r: Response) -> Value {
        serde_json::from_slice(&body(r).await).unwrap()
    }
    fn names(files: &Value) -> Vec<String> {
        let mut n: Vec<String> = files.as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap().to_owned()).collect();
        n.sort();
        n
    }

    #[tokio::test]
    async fn issue_scan_has_the_desktop_shape_and_ignores_rows_it_cannot_judge() {
        let i = issues_catalog();
        let r = scan(&i, Arc::new(GeometryGate::default())).await;
        assert_eq!(r.status(), StatusCode::OK);
        let v = json_body(r).await;
        let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["duplicateGroups", "missingFolders", "orphaned", "roots", "truncated"]);
        assert_eq!(v["truncated"], false);
        assert_eq!(v["missingFolders"], json!([]));
        // Only the model under the root that is really gone; rows outside the roots (even a
        // missing one) are unclear and never listed or grouped.
        assert_eq!(v["orphaned"].as_array().unwrap().len(), 1);
        assert!(v["orphaned"][0]["path"].as_str().unwrap().ends_with("gone.stl"));
        let groups = v["duplicateGroups"].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(names(&groups[0]).len(), 2);
        assert!(groups[0][0]["id"].is_string(), "full file DTO like the desktop");
    }
    #[tokio::test]
    async fn issue_scan_is_read_only() {
        let i = issues_catalog();
        let before = Sha256::digest(std::fs::read(&i.db).unwrap());
        assert_eq!(scan(&i, Arc::new(GeometryGate::default())).await.status(), StatusCode::OK);
        assert_eq!(Sha256::digest(std::fs::read(&i.db).unwrap()), before);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn issue_scan_does_not_call_an_unreadable_directory_missing() {
        use std::os::unix::fs::PermissionsExt;
        let i = issues_catalog();
        std::fs::create_dir(i.dir.join("locked")).unwrap();
        std::fs::write(i.dir.join("locked/x.stl"), b"x").unwrap();
        let conn = db::connect(&i.db).unwrap();
        db::test_insert_minimal_file(&conn, i.dir.join("locked/x.stl").to_str().unwrap(), None).unwrap();
        drop(conn);
        std::fs::set_permissions(i.dir.join("locked"), std::fs::Permissions::from_mode(0o000)).unwrap();
        let v = json_body(scan(&i, Arc::new(GeometryGate::default())).await).await;
        std::fs::set_permissions(i.dir.join("locked"), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(v["orphaned"].as_array().unwrap().iter().all(|f| !f["path"].as_str().unwrap().contains("locked")), "{v}");
    }
    #[tokio::test]
    async fn issue_scan_limits_report_truncated_and_form_no_group_from_unchecked_rows() {
        let i = issues_catalog();
        let conn = db::connect(&i.db).unwrap();
        for n in 0..3 {
            db::test_insert_minimal_file(&conn, i.dir.join(format!("gone{n}.stl")).to_str().unwrap(), None).unwrap();
        }
        drop(conn);
        let limited = |limits| Arc::new(GeometryGate { issue_limits: limits, ..Default::default() });
        let v = json_body(scan(&i, limited(catalog_issues::IssueLimits { max_orphaned: 2, ..GeometryGate::default().issue_limits })).await).await;
        assert_eq!(v["truncated"], true);
        assert_eq!(v["orphaned"].as_array().unwrap().len(), 2);
        // A zero time budget checks no row at all: nothing may be listed or grouped.
        let v = json_body(scan(&i, limited(catalog_issues::IssueLimits { max_duration: std::time::Duration::ZERO, ..GeometryGate::default().issue_limits })).await).await;
        assert_eq!(v["truncated"], true);
        assert_eq!((v["orphaned"].as_array().unwrap().len(), v["duplicateGroups"].as_array().unwrap().len()), (0, 0));
        // Group limit.
        let conn = db::connect(&i.db).unwrap();
        for (n, hash) in [("c1", "h9"), ("c2", "h9"), ("d1", "h8"), ("d2", "h8")] {
            std::fs::write(i.dir.join(format!("{n}.stl")), n).unwrap();
            let id = db::test_insert_minimal_file(&conn, i.dir.join(format!("{n}.stl")).to_str().unwrap(), None).unwrap();
            conn.execute("UPDATE files SET content_hash=?2 WHERE id=?1", rusqlite::params![id, hash]).unwrap();
        }
        drop(conn);
        let v = json_body(scan(&i, limited(catalog_issues::IssueLimits { max_groups: 1, ..GeometryGate::default().issue_limits })).await).await;
        assert_eq!((v["truncated"].clone(), v["duplicateGroups"].as_array().unwrap().len()), (json!(true), 1));
    }
    #[tokio::test]
    async fn issue_scan_limits_default_to_the_specified_values() {
        let l = GeometryGate::default().issue_limits;
        assert_eq!((l.max_orphaned, l.max_groups, l.max_duration), (500, 200, std::time::Duration::from_secs(20)));
    }
    #[tokio::test]
    async fn issue_scan_is_single_flight_and_shares_the_report_and_manager_gates() {
        let i = issues_catalog();
        let gate = Arc::new(GeometryGate::default());
        let running = gate.disk_reports.clone().try_acquire_owned().unwrap();
        let r = scan(&i, gate.clone()).await;
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(r.headers().contains_key("retry-after"));
        drop(running);
        let held: Vec<_> = (0..4).map(|_| gate.manager_reads.clone().try_acquire_owned().unwrap()).collect();
        assert_eq!(scan(&i, gate.clone()).await.status(), StatusCode::SERVICE_UNAVAILABLE);
        drop(held);
        assert_eq!(scan(&i, gate.clone()).await.status(), StatusCode::OK);
        for _ in 0..100 {
            if gate.disk_reports.available_permits() == 1 && gate.manager_reads.available_permits() == 4 && gate.barrier.snapshot().readers == 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("permits were never released");
    }
    #[tokio::test]
    async fn issue_scan_runs_on_its_own_thread_and_answers_a_timeout() {
        let i = issues_catalog();
        let gate = Arc::new(GeometryGate { report_timeout: std::time::Duration::ZERO, ..Default::default() });
        let r = scan(&i, gate.clone()).await;
        assert_eq!(r.status(), StatusCode::GATEWAY_TIMEOUT);
        assert!(String::from_utf8(body(r).await).unwrap().contains("\"rescanTimeout\""));
        // The permits stay with the thread until it ends, then come back.
        for _ in 0..100 {
            if gate.disk_reports.available_permits() == 1 && gate.barrier.snapshot().readers == 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("permits were never released");
    }
    #[test]
    fn the_row_guard_refuses_past_the_limit_and_lets_the_limit_itself_through() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE files (id INTEGER PRIMARY KEY, deleted_at TEXT); INSERT INTO files (deleted_at) VALUES (NULL),(NULL),(NULL),('x');").unwrap();
        let live = "SELECT COUNT(*) FROM files WHERE deleted_at IS NULL";
        assert_eq!(guard_row_count(&conn, live, 3), Ok(()));
        assert_eq!(guard_row_count(&conn, live, 2), Err(ReadError::coded_with(ErrorCode::WebLimitReached, [2])));
        assert_eq!(guard_row_count(&conn, "SELECT COUNT(*) FROM files WHERE deleted_at IS NOT NULL", 1), Ok(()));
    }
    #[tokio::test]
    async fn the_new_read_commands_respect_the_barrier() {
        let i = issues_catalog();
        let gate = Arc::new(GeometryGate::default());
        let window = gate.barrier.enter_draining(std::time::Duration::from_secs(1)).await.unwrap();
        // Draining lets readers in.
        assert_eq!(scan(&i, gate.clone()).await.status(), StatusCode::OK);
        let window = window.enter_exclusive(std::time::Duration::from_secs(1)).await.unwrap();
        for command in ["scan_catalog_issues", "get_sync_on_start", "get_sync_auto_add", "run_start_sync"] {
            let r = invoke(i.db.clone(), vec![i.dir.clone()], command.into(), json!({}), gate.clone()).await;
            assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE, "{command}");
            assert_eq!(json_body(r).await["message"], "maintenance", "{command}");
        }
        window.release();
        assert_eq!(scan(&i, gate).await.status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn the_new_read_commands_take_no_arguments() {
        let i = issues_catalog();
        let gate = Arc::new(GeometryGate::default());
        for command in ["scan_catalog_issues", "get_sync_on_start", "get_sync_auto_add", "run_start_sync"] {
            let r = invoke(i.db.clone(), vec![i.dir.clone()], command.into(), json!({"path": "/etc"}), gate.clone()).await;
            assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY, "{command}");
            let r = invoke(i.db.clone(), vec![i.dir.clone()], command.into(), json!([]), gate.clone()).await;
            assert_eq!(r.status(), StatusCode::BAD_REQUEST, "{command}");
        }
    }
    #[tokio::test]
    async fn sync_settings_default_to_off_and_read_the_stored_keys() {
        let i = issues_catalog();
        let gate = Arc::new(GeometryGate::default());
        let get = |command: &'static str| {
            let (db_path, dir, gate) = (i.db.clone(), i.dir.clone(), gate.clone());
            async move { json_body(invoke(db_path, vec![dir], command.into(), json!({}), gate).await).await }
        };
        assert_eq!((get("get_sync_on_start").await, get("get_sync_auto_add").await), (json!(false), json!(false)));
        let conn = db::connect(&i.db).unwrap();
        disk_sync_settings::set_flag(&conn, disk_sync_settings::SETTING_AUTO_ADD, true).unwrap();
        drop(conn);
        assert_eq!((get("get_sync_on_start").await, get("get_sync_auto_add").await), (json!(false), json!(true)));
    }
    #[tokio::test]
    async fn run_start_sync_only_reports_the_stored_state_and_never_touches_the_catalog() {
        let gate = Arc::new(GeometryGate::default());
        // The catalog does not even exist: the answer comes from memory.
        let missing = std::env::temp_dir().join("server-no-such-catalog.db");
        let ask = |gate: Arc<GeometryGate>| invoke(missing.clone(), vec![], "run_start_sync".into(), json!({}), gate);
        let v = json_body(ask(gate.clone()).await).await;
        assert_eq!(v, json!({"ran":false,"busy":false,"state":"off","newFiles":0,"renamed":0,"missingFiles":0,"missingFolders":0,"duplicates":0,"unreachableRoots":0,"applied":null}));
        gate.start_sync.set(StartSyncState { busy: true, state: StartSyncPhase::Running, ..Default::default() });
        let v = json_body(ask(gate.clone()).await).await;
        assert_eq!((v["busy"].clone(), v["state"].clone(), v["ran"].clone()), (json!(true), json!("running"), json!(false)));
        gate.start_sync.set(StartSyncState {
            ran: true,
            state: StartSyncPhase::Done,
            new_files: 3,
            applied: Some(StartSyncApplied { added: 2, renamed: 0, failed: 1 }),
            ..Default::default()
        });
        let v = json_body(ask(gate).await).await;
        assert_eq!((v["state"].clone(), v["newFiles"].clone(), v["applied"].clone()), (json!("done"), json!(3), json!({"added":2,"renamed":0,"failed":1})));
    }

    async fn body(response: Response) -> Vec<u8> {
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec()
    }
    fn entries(c: &Catalog) -> Vec<std::ffi::OsString> {
        let mut v = std::fs::read_dir(&c.dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>();
        v.sort();
        v
    }
    #[tokio::test]
    async fn unexpected_errors_hide_details_and_expected_errors_keep_their_code() {
        let busy = ReadError { busy: true, ..ReadError::from("database is locked: /srv/catalog.db") };
        for (e, status, message) in [
            (
                ReadError::from("no such table: private_table"),
                500,
                "webServerInternal",
            ),
            (ReadError::coded(ErrorCode::FileNotFound), 422, "fileNotFound"),
            (ReadError::expected("imageUploadTooLarge"), 422, "imageUploadTooLarge"),
            // Free text that claims to be expected never reaches the client.
            (ReadError::expected("Fachfehler"), 500, "webServerInternal"),
            (ReadError::expected("/srv/models/a.stl fehlt").with_code(GeometryErrorCode::NotFound), 500, "webServerInternal"),
            (busy, 503, "webServerBusy"),
        ] {
            let response = read_error(e);
            assert_eq!(response.status(), status);
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["message"], message);
            assert!(!String::from_utf8_lossy(&bytes).contains("private_table"));
            assert!(!String::from_utf8_lossy(&bytes).contains("/srv/"));
        }
    }
    #[tokio::test]
    async fn all_commands_match_core_and_catalog_end_state_is_unchanged() {
        let c = catalog();
        // A 3MF with print data, so the new fields travel through the same comparison.
        let print_id = {
            let conn = db::connect(&c.db).unwrap();
            let path = c.dir.join("project.3mf").to_string_lossy().to_string();
            let id = db::test_insert_minimal_file(&conn, &path, None).unwrap();
            conn.execute_batch(&format!(
                "INSERT INTO file_print_info (file_id, reader_version, read_at, slice_state, settings_state,
                        printer_model, bed_type, layer_height_mm, nozzle_mm)
                 VALUES ({id}, {version}, '2026-10-31T10:00:00+00:00', 'no_slice_data', 'read',
                        'Bambu Lab X1 Carbon', 'Supertack Plate', 0.2, 0.4);
                 INSERT INTO file_print_plates (file_id, plate_index, first_layer_seconds, plate_bed_type)
                 VALUES ({id}, 1, 123.5, 'supertack_plate');",
                version = katalog_core::threemf::print_info::PRINT_INFO_READER_VERSION,
            ))
            .unwrap();
            id.to_string()
        };
        // A real 3MF with plates for the plate overview, which reads the file itself.
        let plates_id = {
            let conn = db::connect(&c.db).unwrap();
            let path = c.dir.join("plates.3mf");
            std::fs::copy(katalog_core_fixture("creality-3plates.3mf"), &path).unwrap();
            db::test_insert_minimal_file(&conn, path.to_str().unwrap(), None).unwrap().to_string()
        };
        // A bundle, so the bundle list travels through the same comparison with a member in it.
        {
            let conn = db::connect(&c.db).unwrap();
            let members = [print_id.parse().unwrap(), c.id.parse().unwrap()];
            db::bundles::create_bundle(&conn, "Pair", db::bundles::BundleKind::Parts, &members, "2026-11-04T10:00:00+00:00").unwrap();
        }
        // A printer with prints, so the statistics travel through the same comparison.
        {
            let conn = db::connect(&c.db).unwrap();
            let printer = db::printers::insert_printer(&conn, "Stats").unwrap();
            conn.execute_batch(&format!(
                "INSERT INTO printer_connections(printer_id,kind,address,connected_since) VALUES ({printer},'moonraker','a',900.5);
                 INSERT INTO printer_jobs(printer_id,remote_id,file_name,outcome,raw_status,ended_at,print_duration_s,used_mm,material,state,booked_g,failure_reason)
                 VALUES ({printer},'a','x','completed','completed',1000,60,10,'PLA','confirmed',12.5,NULL),
                        ({printer},'b','x','partial','cancelled',1200,30,5,NULL,'open',NULL,'user_cancel');"
            ))
            .unwrap();
        }
        // A stored choice, so the comparison is not just the default.
        {
            let conn = db::connect(&c.db).unwrap();
            db::print_technology::set_print_technology(&conn, "resin").unwrap();
        }
        let before = Sha256::digest(std::fs::read(&c.db).unwrap());
        let names = entries(&c);
        let conn = db::open_read_only(&c.db).unwrap();
        let cases = vec![
            (
                "list_files_by_ids",
                json!({"ids":[print_id]}),
                serde_json::to_value(read::list_files_by_ids(&conn, vec![print_id.clone()]).unwrap())
                    .unwrap(),
            ),
            (
                "list_file_summaries",
                json!({}),
                serde_json::to_value(read::list_file_summaries(&conn).unwrap()).unwrap(),
            ),
            (
                "list_all_file_tags",
                json!({}),
                serde_json::to_value(read::list_all_file_tags(&conn).unwrap()).unwrap(),
            ),
            (
                "list_files_by_ids",
                json!({"ids":[c.id]}),
                serde_json::to_value(read::list_files_by_ids(&conn, vec![c.id.clone()]).unwrap())
                    .unwrap(),
            ),
            (
                "list_tag_counts",
                json!({}),
                serde_json::to_value(read::list_tag_counts(&conn).unwrap()).unwrap(),
            ),
            (
                "get_print_statistics",
                json!({}),
                serde_json::to_value(read::manager::print_statistics_rows(&conn, None, read::manager::PRINT_STATS_MAX_ROWS).unwrap()).unwrap(),
            ),
            (
                "get_print_statistics",
                json!({"printerId":null}),
                serde_json::to_value(read::manager::print_statistics_rows(&conn, None, read::manager::PRINT_STATS_MAX_ROWS).unwrap()).unwrap(),
            ),
            (
                "get_print_statistics",
                json!({"printerId":"1"}),
                serde_json::to_value(read::manager::print_statistics_rows(&conn, Some(1), read::manager::PRINT_STATS_MAX_ROWS).unwrap()).unwrap(),
            ),
            (
                "get_print_technology",
                json!({}),
                serde_json::to_value(read::get_print_technology(&conn).unwrap()).unwrap(),
            ),
            (
                "list_print_log_entries",
                json!({"fileId":c.id}),
                serde_json::to_value(read::list_print_log_entries(&conn, c.id.clone()).unwrap())
                    .unwrap(),
            ),
            (
                "list_folders",
                json!({}),
                serde_json::to_value(read::list_folders(&conn).unwrap()).unwrap(),
            ),
            (
                "list_collections",
                json!({}),
                serde_json::to_value(read::list_collections(&conn).unwrap()).unwrap(),
            ),
            (
                "list_bundles",
                json!({}),
                serde_json::to_value(read::list_bundles(&conn).unwrap()).unwrap(),
            ),
            (
                "list_collection_files",
                json!({"collectionId":c.collection}),
                serde_json::to_value(
                    read::list_collection_files(&conn, c.collection.clone()).unwrap(),
                )
                .unwrap(),
            ),
            (
                "list_trash",
                json!({}),
                serde_json::to_value(read::list_trash(&conn).unwrap()).unwrap(),
            ),
            (
                "list_file_images",
                json!({"ids":[c.id]}),
                serde_json::to_value(read::encode_file_images(
                    read::load_file_images(&conn, &[c.id.parse().unwrap()]).unwrap(),
                ))
                .unwrap(),
            ),
            (
                "get_app_version",
                json!({}),
                json!(env!("CARGO_PKG_VERSION")),
            ),
            ("is_preview_build", json!({}), json!(false)),
            ("has_step_preview", json!({}), json!(false)),
            (
                "read_plate_overview",
                json!({"fileId":plates_id,"withPreviews":true}),
                serde_json::to_value(
                    read::read_plate_overview(&c.dir.join("plates.3mf"), db::models::FileType::ThreeMf, Some(std::slice::from_ref(&c.dir)), true).unwrap(),
                )
                .unwrap(),
            ),
            (
                "read_plate_overview",
                json!({"fileId":plates_id}),
                serde_json::to_value(
                    read::read_plate_overview(&c.dir.join("plates.3mf"), db::models::FileType::ThreeMf, Some(std::slice::from_ref(&c.dir)), false).unwrap(),
                )
                .unwrap(),
            ),
        ];
        drop(conn);
        for (command, args, expected) in cases {
            let response = call(&c, command, args).await;
            assert_eq!(response.status(), StatusCode::OK, "{command}");
            assert_eq!(
                serde_json::from_slice::<Value>(&body(response).await).unwrap(),
                expected,
                "{command}"
            );
        }
        // The fields are really there, not just equal to a core answer that lacks them.
        let stats = serde_json::from_slice::<Value>(&body(call(&c, "get_print_statistics", json!({})).await).await).unwrap();
        assert_eq!(stats["rows"].as_array().unwrap().len(), 2);
        assert_eq!(stats["rows"][0]["endedAt"], 1200.0);
        assert_eq!(stats["firstConnectedAt"], 900.5);
        assert_eq!(stats["truncated"], false);
        let listed = serde_json::from_slice::<Value>(&body(call(&c, "list_files_by_ids", json!({"ids":[print_id]})).await).await).unwrap();
        assert_eq!(listed[0]["printInfo"]["sliceState"], "no_slice_data");
        assert_eq!(listed[0]["printInfo"]["plates"][0]["firstLayerSeconds"], 123.5);
        let summaries = serde_json::from_slice::<Value>(&body(call(&c, "list_file_summaries", json!({})).await).await).unwrap();
        let state_of = |id: &str| summaries.as_array().unwrap().iter().find(|s| s["id"] == id).unwrap()["printDataState"].clone();
        assert_eq!(state_of(&print_id), "no_slice_data");
        assert_eq!(state_of(&c.id), "none", "an STL has no print data");
        // The answers really carry the objects and pictures, not just equal a core answer without them.
        let overview = serde_json::from_slice::<Value>(&body(call(&c, "read_plate_overview", json!({"fileId":plates_id,"withPreviews":true})).await).await).unwrap();
        assert_eq!(overview["plates"].as_array().unwrap().iter().map(|p| p["objectCount"].as_u64().unwrap()).collect::<Vec<_>>(), [1, 3, 1]);
        assert!(overview["plates"][1]["preview"].as_str().unwrap().starts_with("data:image/png;base64,"));
        let plain = serde_json::from_slice::<Value>(&body(call(&c, "read_plate_overview", json!({"fileId":plates_id})).await).await).unwrap();
        assert!(plain["plates"][0].get("preview").is_none());
        let response = call(&c, "get_model_geometry", json!({"fileId":c.id})).await;
        assert_eq!(
            response.headers()["content-type"],
            "application/octet-stream"
        );
        assert_eq!(
            body(response).await,
            read::get_model_geometry(&c.dir.join("one.stl"), Some(std::slice::from_ref(&c.dir)))
                .unwrap()
        );
        assert_eq!(before, Sha256::digest(std::fs::read(&c.db).unwrap()));
        assert_eq!(names, entries(&c));
    }
    #[tokio::test]
    async fn invalid_arguments_and_unavailable_commands() {
        let c = catalog();
        for command in ["unknown", "add_tag", "delete_file", "start_import"] {
            let r = call(&c, command, json!({})).await;
            assert_eq!(r.status(), 404);
            assert_eq!(
                serde_json::from_slice::<Value>(&body(r).await).unwrap(),
                json!({"message":"webCommandUnavailable","expected":true})
            );
        }
        for (cmd, args) in [
            ("list_file_summaries", json!([])),
            ("get_model_geometry", json!({"fileId":"no"})),
            ("list_files_by_ids", json!({"ids":[1]})),
            ("list_collection_files", json!({"collectionId":"1.0"})),
            ("list_file_images", json!({"ids":vec!["1";201]})),
            ("get_print_statistics", json!({"printerId":"x"})),
            ("get_print_statistics", json!({"printerId":5})),
        ] {
            assert_eq!(call(&c, cmd, args).await.status(), 400);
        }
    }
    /// Every refusal of the read allowlist names a registered code and carries only
    /// numbers as parameters; the status codes stay as they were.
    #[tokio::test]
    async fn read_refusals_answer_with_registered_codes() {
        let c = catalog();
        let thousand = vec!["1"; 1001];
        for (cmd, args, status, expected) in [
            ("list_file_summaries", json!([]), 400, json!({"message":"reqInvalidArguments","expected":true})),
            ("get_model_geometry", json!({"fileId":"no"}), 400, json!({"message":"reqInvalidIds","expected":true})),
            ("list_file_images", json!({"ids":vec!["1";201]}), 400, json!({"message":"reqIdsTooMany","expected":true,"params":["200"]})),
            ("list_files_by_ids", json!({"ids":vec!["1";501]}), 400, json!({"message":"reqIdsTooMany","expected":true,"params":["500"]})),
            ("check_filament", json!({"fileIds":thousand}), 422, json!({"message":"reqIdsTooMany","expected":true,"params":["1000"]})),
            ("check_filament", json!({"fileIds":"x"}), 400, json!({"message":"reqInvalidIds","expected":true})),
            ("folder_removal_summary", json!({"folderId":"999999"}), 404, json!({"message":"folderNotFound","expected":true})),
            ("empty_trash_summary", json!({"x":1}), 422, json!({"message":"reqInvalidArguments","expected":true})),
            ("get_model_geometry", json!({"fileId":"999999"}), 404, json!({"message":"fileNotFound","expected":true,"code":"notFound"})),
        ] {
            let r = call(&c, cmd, args).await;
            assert_eq!(r.status(), status, "{cmd}");
            assert_eq!(serde_json::from_slice::<Value>(&body(r).await).unwrap(), expected, "{cmd}");
        }
    }
    #[tokio::test]
    async fn geometry_outside_the_roots_names_a_code_and_no_path() {
        let c = catalog();
        let root = c.dir.join("allowed");
        std::fs::create_dir(&root).unwrap();
        let r = invoke(
            c.db.clone(),
            vec![root],
            "get_model_geometry".into(),
            json!({"fileId":c.id}),
            Arc::new(GeometryGate::default()),
        )
        .await;
        assert_eq!(r.status(), 422);
        let text = String::from_utf8(body(r).await).unwrap();
        assert!(text.contains("\"dlPathNotAllowed\""), "{text}");
        assert!(!text.contains(c.dir.to_str().unwrap()), "{text}");
    }
    /// A catalog row for a STEP file below the catalog folder.
    fn step_entry(c: &Catalog) -> String {
        let path = c.dir.join("teil.step");
        std::fs::write(&path, b"ISO-10303-21;\nEND-ISO-10303-21;\n").unwrap();
        let conn = db::connect(&c.db).unwrap();
        let id = db::test_insert_minimal_file(&conn, path.to_str().unwrap(), None).unwrap();
        conn.execute("UPDATE files SET file_type='stp' WHERE id=?1", [id]).unwrap();
        id.to_string()
    }
    fn step_gate(parallel: usize) -> Arc<GeometryGate> {
        // Never started in these tests: the answers come before a process would be spawned,
        // or from a spawn that fails.
        let worker = crate::step_worker::StepWorker::with_exe(
            PathBuf::from("/nonexistent/katalog-server"),
            crate::step_worker::Limits::default(),
            parallel,
        );
        Arc::new(GeometryGate::with_step(worker))
    }
    #[tokio::test]
    async fn has_step_preview_follows_the_worker() {
        let c = catalog();
        let disabled = invoke(c.db.clone(), vec![c.dir.clone()], "has_step_preview".into(), json!({}), Arc::new(GeometryGate::default())).await;
        assert_eq!(serde_json::from_slice::<Value>(&body(disabled).await).unwrap(), json!(false));
        let enabled = invoke(c.db.clone(), vec![c.dir.clone()], "has_step_preview".into(), json!({}), step_gate(1)).await;
        assert_eq!(serde_json::from_slice::<Value>(&body(enabled).await).unwrap(), json!(true));
    }
    #[tokio::test]
    async fn step_geometry_without_a_worker_stays_unsupported() {
        let c = catalog();
        let id = step_entry(&c);
        let r = invoke(c.db.clone(), vec![c.dir.clone()], "get_model_geometry".into(), json!({"fileId":id}), Arc::new(GeometryGate::default())).await;
        assert_eq!(r.status(), 422);
        let v: Value = serde_json::from_slice(&body(r).await).unwrap();
        assert_eq!(v["message"], "fileFormatUnsupported");
        assert_eq!(v["code"], "unsupported");
    }
    #[tokio::test]
    async fn step_geometry_over_the_worker_limit_is_429_with_retry_after() {
        let c = catalog();
        let id = step_entry(&c);
        let r = invoke(c.db.clone(), vec![c.dir.clone()], "get_model_geometry".into(), json!({"fileId":id}), step_gate(0)).await;
        assert_eq!(r.status(), 429);
        assert_eq!(r.headers()["retry-after"], "5");
        let v: Value = serde_json::from_slice(&body(r).await).unwrap();
        assert_eq!(v["message"], "webTooManyRequests");
    }
    #[tokio::test]
    async fn step_previews_do_not_queue_behind_busy_3mf_parsers() {
        let c = catalog();
        let id = step_entry(&c);
        for parallel in [0, 1] {
            let gate = step_gate(parallel);
            // Both general parser places are taken by long 3MF work.
            let _busy = (
                gate.workers.clone().try_acquire_owned().unwrap(),
                gate.workers.clone().try_acquire_owned().unwrap(),
            );
            let call = invoke(c.db.clone(), vec![c.dir.clone()], "get_model_geometry".into(), json!({"fileId":id}), gate.clone());
            let r = tokio::time::timeout(std::time::Duration::from_secs(5), call)
                .await
                .unwrap_or_else(|_| panic!("STEP request waited for the 3MF gate (parallel {parallel})"));
            if parallel == 0 {
                assert_eq!(r.status(), 429);
                assert_eq!(r.headers()["retry-after"], "5");
            } else {
                // The worker cannot start here; what matters is the answer came without waiting.
                assert_eq!(r.status(), 422);
            }
        }
    }
    #[tokio::test]
    async fn step_geometry_checks_the_roots_before_any_worker_and_names_no_path() {
        let c = catalog();
        let id = step_entry(&c);
        let root = c.dir.join("allowed");
        std::fs::create_dir(&root).unwrap();
        let r = invoke(c.db.clone(), vec![root], "get_model_geometry".into(), json!({"fileId":id}), step_gate(1)).await;
        assert_eq!(r.status(), 422);
        let text = String::from_utf8(body(r).await).unwrap();
        assert!(text.contains("\"dlPathNotAllowed\""), "{text}");
        assert!(!text.contains(c.dir.to_str().unwrap()), "{text}");
    }
    #[tokio::test]
    async fn a_worker_that_cannot_start_is_a_coded_answer_not_a_crash() {
        let c = catalog();
        let id = step_entry(&c);
        let r = invoke(c.db.clone(), vec![c.dir.clone()], "get_model_geometry".into(), json!({"fileId":id}), step_gate(1)).await;
        assert_eq!(r.status(), 422);
        let v: Value = serde_json::from_slice(&body(r).await).unwrap();
        assert_eq!(v["message"], "fileFormatUnsupported");
        assert_eq!(v["code"], "unsupported");
    }
    #[tokio::test]
    async fn a_busy_worker_answers_at_once_with_a_retry_hint_and_frees_up_again() {
        let c = catalog();
        let id = step_entry(&c);
        let gate = step_gate(1);
        let held = gate.step.try_slot().expect("the only place is free");
        let busy = invoke(c.db.clone(), vec![c.dir.clone()], "get_model_geometry".into(), json!({"fileId": id}), gate.clone()).await;
        assert_eq!(busy.status(), 429);
        assert_eq!(busy.headers()["retry-after"], crate::step_worker::StepWorker::RETRY_AFTER_SECONDS.to_string().as_str());
        let v: Value = serde_json::from_slice(&body(busy).await).unwrap();
        assert_eq!(v["message"], "webTooManyRequests");
        drop(held);
        let again = invoke(c.db.clone(), vec![c.dir.clone()], "get_model_geometry".into(), json!({"fileId": id}), gate).await;
        assert_eq!(again.status(), 422, "a freed place is used, not refused");
    }
    #[test]
    fn every_worker_outcome_is_a_wire_code() {
        use crate::step_worker::WorkerError as W;
        for e in [W::TooLarge, W::TooComplex, W::NoGeometry, W::Unreadable, W::Crashed, W::Protocol, W::Unavailable, W::NoIsolation, W::Busy] {
            let r = step_read_error(e.clone());
            assert!(r.expected, "{e:?}");
            assert!(is_wire_code(&r.message), "{e:?}: {}", r.message);
            assert!(r.code.is_some(), "{e:?}");
        }
    }
    #[tokio::test]
    async fn list_files_by_ids_is_limited_to_500_ids() {
        let c = catalog();
        let too_many = call(&c, "list_files_by_ids", json!({"ids": vec!["1"; 501]})).await;
        assert_eq!(too_many.status(), 400);
        let at_limit = call(&c, "list_files_by_ids", json!({"ids": vec![c.id.clone(); 500]})).await;
        assert_eq!(at_limit.status(), 200);
    }
    #[tokio::test]
    async fn geometry_roots_reject_outside_and_symlink() {
        let c = catalog();
        let root = c.dir.join("allowed");
        std::fs::create_dir(&root).unwrap();
        let r = invoke(
            c.db.clone(),
            vec![root.clone()],
            "get_model_geometry".into(),
            json!({"fileId":c.id}),
            Arc::new(GeometryGate::default()),
        )
        .await;
        assert_eq!(r.status(), 422);
        #[cfg(unix)]
        {
            let link = root.join("escape.stl");
            std::os::unix::fs::symlink(c.dir.join("one.stl"), &link).unwrap();
            let conn = db::connect(&c.db).unwrap();
            conn.execute(
                "UPDATE files SET path=?1 WHERE deleted_at IS NULL",
                [link.to_str().unwrap()],
            )
            .unwrap();
            drop(conn);
            let r = invoke(
                c.db.clone(),
                vec![root],
                "get_model_geometry".into(),
                json!({"fileId":c.id}),
                Arc::new(GeometryGate::default()),
            )
            .await;
            assert_eq!(r.status(), 422);
        }
    }
    fn katalog_core_fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../katalog-core/tests/fixtures").join(name)
    }
    #[tokio::test]
    async fn plate_overview_checks_arguments_roots_and_missing_files() {
        let c = catalog();
        // An STL answers with an empty list, not an error.
        let r = call(&c, "read_plate_overview", json!({"fileId":c.id,"withPreviews":true})).await;
        assert_eq!(r.status(), 200);
        assert_eq!(serde_json::from_slice::<Value>(&body(r).await).unwrap(), json!({"plates":[]}));
        for args in [json!({"fileId":"no"}), json!({}), json!({"fileId":c.id,"withPreviews":"yes"}), json!({"fileId":c.id,"withPreviews":1})] {
            assert_eq!(call(&c, "read_plate_overview", args.clone()).await.status(), 400, "{args}");
        }
        assert_eq!(call(&c, "read_plate_overview", json!([])).await.status(), 400);
        // Unknown id and a file that is gone.
        let conn = db::connect(&c.db).unwrap();
        let gone = db::test_insert_minimal_file(&conn, c.dir.join("gone.3mf").to_str().unwrap(), None).unwrap();
        let inside = c.dir.join("in.3mf");
        std::fs::copy(katalog_core_fixture("creality-3plates.3mf"), &inside).unwrap();
        let present = db::test_insert_minimal_file(&conn, inside.to_str().unwrap(), None).unwrap();
        drop(conn);
        assert_eq!(call(&c, "read_plate_overview", json!({"fileId":"999999"})).await.status(), 404);
        assert_eq!(call(&c, "read_plate_overview", json!({"fileId":gone.to_string()})).await.status(), 404);
        // Outside the shared roots the file is refused and nothing of it is read.
        let root = c.dir.join("allowed");
        std::fs::create_dir(&root).unwrap();
        let outside = invoke(c.db.clone(), vec![root], "read_plate_overview".into(), json!({"fileId":present.to_string(),"withPreviews":true}), Arc::new(GeometryGate::default())).await;
        assert_eq!(outside.status(), 422);
        assert!(!String::from_utf8_lossy(&body(outside).await).contains("base64"));
    }
    #[tokio::test]
    async fn plate_overview_waits_for_the_same_parser_gate_as_the_geometry() {
        let c = catalog();
        let gate = Arc::new(GeometryGate::default());
        let held: Vec<_> = (0..10).map(|_| gate.admission.clone().try_acquire_owned().unwrap()).collect();
        let r = invoke(c.db.clone(), vec![c.dir.clone()], "read_plate_overview".into(), json!({"fileId":c.id}), gate.clone()).await;
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
        drop(held);
        let r = invoke(c.db.clone(), vec![c.dir.clone()], "read_plate_overview".into(), json!({"fileId":c.id}), gate).await;
        assert_eq!(r.status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn busy_marker_maps_to_503_without_message_parsing() {
        let mut e = ReadError::from("nicht textabhängig");
        e.busy = true;
        assert_eq!(read_error(e).status(), 503);
        assert_eq!(read_error(ReadError::from("database locked")).status(), 500);
    }
    #[tokio::test]
    async fn geometry_queue_is_bounded_and_cancellation_retains_running_permits() {
        let gate = Arc::new(GeometryGate::default());
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let g = gate.clone();
        let running = tokio::spawn(async move {
            let permits = g.acquire().await.unwrap();
            tokio::task::spawn_blocking(move || {
                let _permits = permits;
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .await
            .unwrap();
        });
        started_rx.await.unwrap();
        running.abort();
        let second = gate.acquire().await.unwrap();
        assert_eq!(gate.workers.available_permits(), 0);
        let mut waiters = Vec::new();
        for _ in 0..8 {
            let g = gate.clone();
            waiters.push(tokio::spawn(async move { g.acquire().await }));
        }
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while gate.admission.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(gate.acquire().await.unwrap_err().status(), 503);
        for waiter in waiters {
            waiter.abort();
            let _ = waiter.await;
        }
        assert_eq!(gate.workers.available_permits(), 0);
        drop(second);
        release_tx.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while gate.workers.available_permits() != 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}

#[cfg(test)]
mod manager_tests {
    use super::*;
    #[tokio::test]
    async fn manager_read_concurrency_is_bounded_and_returns_retry_after() {
        let gate = Arc::new(GeometryGate::default());
        let permits = gate
            .manager_reads
            .clone()
            .acquire_many_owned(4)
            .await
            .unwrap();
        for command in [
            "list_filament_spools",
            "get_filament_spool_image",
            "check_filament",
            "list_printers",
            "list_printer_history",
            "get_print_statistics",
            "get_last_printer_for_file",
        ] {
            let r = invoke(
                PathBuf::from("unused"),
                vec![],
                command.into(),
                json!({"id":"1","fileIds":[],"printerId":"1","fileId":"1"}),
                gate.clone(),
            )
            .await;
            assert_eq!(r.status(), 503, "{command}");
            assert_eq!(r.headers()["retry-after"], "1");
        }
        drop(permits);
        assert_eq!(gate.manager_reads.available_permits(), 4);
    }
}

#[cfg(test)]
mod fileops_summary_tests {
    use super::*;
    #[test]
    fn trash_summary_compares_instants_and_rejects_invalid_dates() {
        let conn = db::connect_in_memory().unwrap();
        db::test_insert_minimal_file(&conn, "/one", None).unwrap();
        db::test_insert_minimal_file(&conn, "/two", None).unwrap();
        conn.execute("UPDATE files SET deleted_at=CASE id WHEN 1 THEN '2026-01-01T01:00:00+02:00' ELSE '2026-01-01T00:00:00+00:00' END",[]).unwrap();
        let summary = trash_summary(&conn, &[]).unwrap();
        assert_eq!(summary["count"], 2);
        assert_eq!(summary["maxDeletedAt"], "2026-01-01T00:00:00+00:00");
        conn.execute("UPDATE files SET deleted_at='invalid' WHERE id=1", [])
            .unwrap();
        assert!(trash_summary(&conn, &[]).is_err());
    }
    #[test]
    fn folder_summary_counts_only_active_models_and_excludes_root() {
        let conn = db::connect_in_memory().unwrap();
        conn.execute_batch("INSERT INTO folders(id,name,path,parent_id) VALUES(1,'Parent','/parent',NULL),(2,'Child','/parent/child',1)").unwrap();
        db::test_insert_minimal_file(&conn, "/one", Some(1)).unwrap();
        db::test_insert_minimal_file(&conn, "/two", Some(2)).unwrap();
        conn.execute(
            "UPDATE files SET deleted_at='2026-01-01T00:00:00Z' WHERE id=2",
            [],
        )
        .unwrap();
        assert_eq!(
            folder_summary(&conn, 1).unwrap(),
            Some(json!({"name":"Parent","subfolderCount":1,"modelCount":1}))
        );
        assert_eq!(folder_summary(&conn, 999).unwrap(), None);
    }
}
