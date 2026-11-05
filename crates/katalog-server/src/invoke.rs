//! Explicit read-only command allowlist and bounded blocking geometry work.
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use katalog_core::{
    companion_read, db, disk_report,
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

pub struct GeometryGate {
    workers: Arc<Semaphore>,
    manager_reads: Arc<Semaphore>,
    /// One disk comparison at a time: each walks the roots.
    disk_reports: Arc<Semaphore>,
    report_limits: disk_report::Limits,
    report_timeout: std::time::Duration,
    admission: Arc<Semaphore>,
}
impl Default for GeometryGate {
    fn default() -> Self {
        Self {
            workers: Arc::new(Semaphore::new(2)),
            manager_reads: Arc::new(Semaphore::new(4)),
            disk_reports: Arc::new(Semaphore::new(1)),
            report_limits: disk_report::Limits::default(),
            report_timeout: DISK_REPORT_TIMEOUT,
            admission: Arc::new(Semaphore::new(10)),
        }
    }
}
impl GeometryGate {
    async fn acquire(&self) -> Result<(OwnedSemaphorePermit, OwnedSemaphorePermit), Box<Response>> {
        let admission = self.admission.clone().try_acquire_owned().map_err(|_| {
            error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::WebTooManyRequests, &[])
        })?;
        let worker = self.workers.clone().acquire_owned().await.map_err(|_| {
            error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::GeometryUnavailable, &[])
        })?;
        Ok((admission, worker))
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
pub const READ_COMMANDS: &[&str] = &[
    "folder_removal_summary",
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
        "WITH RECURSIVE tree(id) AS (SELECT id FROM folders WHERE id=?1 UNION SELECT f.id FROM folders f JOIN tree t ON f.parent_id=t.id) SELECT (SELECT COUNT(*) FROM tree)-1,(SELECT COUNT(*) FROM files WHERE folder_id IN (SELECT id FROM tree) AND deleted_at IS NULL)",
        [id],|row|Ok((row.get(0)?,row.get(1)?))).map_err(internal)?;
    Ok(Some(
        json!({"name":name,"subfolderCount":folder_count,"modelCount":model_count}),
    ))
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
        "folder_removal_summary" | "empty_trash_summary" | "disk_sync_report"
    ) {
        let allowed: &[&str] = if command == "folder_removal_summary" {
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
        "folder_removal_summary" => match id(&args, "folderId") {
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
        "is_preview_build" | "has_step_preview" => return Json(json!(false)).into_response(),
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
    ) {
        match gate.manager_reads.clone().try_acquire_owned() {
            Ok(p) => Some(p),
            Err(_) => {
                let mut r = *error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::WebTooManyRequests, &[]);
                r.headers_mut().insert("retry-after", "1".parse().unwrap());
                return r;
            }
        }
    } else {
        None
    };
    let report_permit = if command == "disk_sync_report" {
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
    if command == "disk_sync_report" {
        let (limits, wait) = (gate.report_limits, gate.report_timeout);
        let (tx, rx) = tokio::sync::oneshot::channel();
        // A thread of its own, not the blocking pool: the scan may never return. The permits
        // move into it on purpose and are released only when it ends, so a stuck scan keeps
        // blocking further ones (single flight, manager gate) instead of piling up new threads.
        let started = std::thread::Builder::new().name("disk-report".into()).spawn(move || {
            let _held = (manager_permit, report_permit);
            let outcome = (|| -> Result<Value, ReadError> {
                let conn = db::open_read_only(&db_path).map_err(ReadError::from)?;
                // The database is only read for the copy; the disk scan runs without it.
                let snapshot = disk_report::snapshot(&conn).map_err(|_| ReadError::from("Abgleich nicht verfügbar"))?;
                drop(conn);
                serde_json::to_value(disk_report::build_report(&snapshot, &roots, limits))
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
            "list_file_summaries" => encode(read::list_file_summaries(&conn)?),
            "list_all_file_tags" => encode(read::list_all_file_tags(&conn)?),
            "list_files_by_ids" => encode(read::list_files_by_ids(&conn, file_ids)?),
            "list_tag_counts" => encode(read::list_tag_counts(&conn)?),
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
            "list_folders" => encode(read::list_folders(&conn)?),
            "list_collections" => encode(read::list_collections(&conn)?),
            "list_bundles" => encode(read::list_bundles(&conn)?),
            "list_collection_files" => {
                encode(read::list_collection_files(&conn, file_id.unwrap())?)
            }
            "list_trash" => encode(read::list_trash(&conn)?),
            "empty_trash_summary" => encode(trash_summary(&conn, &roots)?),
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
                // The read-only web has no configured budgets; the defaults are the
                // largest values the configuration can set.
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
        let again = invoke(c.db.clone(), vec![c.dir.clone()], "disk_sync_report".into(), json!({}), gate.clone()).await;
        assert_eq!(again.status(), StatusCode::SERVICE_UNAVAILABLE);
        for _ in 0..100 {
            if gate.disk_reports.available_permits() == 1 && gate.manager_reads.available_permits() == 4 {
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
