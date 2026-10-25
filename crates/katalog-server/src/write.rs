//! Explicit write allowlist, bounded admission, and transport-neutral core dispatch.
use axum::{
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use katalog_core::write::{web, Connection, WriteError};
use serde::Serialize;
use serde_json::Value;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use tokio::sync::Semaphore;
pub const FILE_OPS_COMMANDS: &[&str] = &[
    "rename_file",
    "move_files_to_folder",
    "delete_files",
    "restore_file",
    "delete_file_permanently",
    "empty_trash_start",
    "empty_trash_block",
    "create_folder",
    "rename_folder",
    "move_folder",
    "remove_folder_from_catalog",
];
pub const WRITE_COMMANDS: &[&str] = &[
    "rename_file",
    "move_files_to_folder",
    "delete_files",
    "restore_file",
    "delete_file_permanently",
    "empty_trash_start",
    "empty_trash_block",
    "create_folder",
    "rename_folder",
    "move_folder",
    "remove_folder_from_catalog",

    "remove_files_from_catalog",
    "start_upload_import",
    "discard_upload",
    "cancel_import",

    "set_favorite",
    "set_print_status",
    "add_tag",
    "remove_tag",
    "set_license",
    "set_source_url",
    "add_print_log_entry",
    "delete_print_log_entry",
    "mark_file_viewed",
    "add_to_queue",
    "remove_from_queue",
    "reorder_queue",
    "create_collection",
    "rename_collection",
    "delete_collection",
    "add_files_to_collection",
    "remove_file_from_collection",
    "reorder_collection",
    "add_filament_spool",
    "update_filament_spool",
    "delete_filament_spool",
    "add_printer",
    "rename_printer",
    "update_printer_details",
    "reorder_printers",
    "delete_printer",
    "add_unit",
    "update_unit",
    "delete_unit",
    "reorder_units",
    "load_spool",
    "unload_spool",
    "restock_filament_spool",
    "consume_resin",
    "set_render_snapshot",
    "set_auto_tagging",
    "block_auto_tag",
    "unblock_auto_tag",
];
pub(crate) fn image_command(command: &str) -> bool {
    matches!(
        command,
        "add_print_log_entry" | "add_filament_spool" | "update_filament_spool" | "set_render_snapshot"
    )
}
pub(crate) fn body_limit(command: &str) -> usize {
    if image_command(command) {
        katalog_core::write::MAX_CUSTOM_IMAGE_BYTES * 4 / 3 + 16 * 1024
    } else if WRITE_COMMANDS.contains(&command) {
        16 * 1024
    } else {
        64 * 1024
    }
}
pub(crate) struct WriteGate {
    pub conn: Arc<Mutex<Connection>>,
    pub admission: Arc<Semaphore>,
    pub worker: Arc<Semaphore>,
}
impl WriteGate {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
            admission: Arc::new(Semaphore::new(33)),
            worker: Arc::new(Semaphore::new(1)),
        }
    }
}
#[derive(Clone)]
pub(crate) struct Bucket {
    tokens: u64,
    updated: u64,
}
impl Bucket {
    pub fn new(now: u64) -> Self {
        Self {
            tokens: 120,
            updated: now,
        }
    }
    pub fn take(&mut self, now: u64) -> bool {
        self.tokens = self
            .tokens
            .saturating_add(now.saturating_sub(self.updated).saturating_mul(60))
            .min(120);
        self.updated = now;
        if self.tokens == 0 {
            false
        } else {
            self.tokens -= 1;
            true
        }
    }
}
pub(crate) struct WriteAudit {
    now: u64,
    session: u64,
    command: &'static str,
    pub status: u16,
}
#[derive(Default)]
pub(crate) struct WriteProgress {
    pub changes: AtomicU64,
    pub completed_items: AtomicU64,
    pub file_ids: Mutex<Vec<i64>>,
    pub audit: std::sync::OnceLock<WriteAudit>,
}
/// Finalizes only once access checks have returned or the request was cancelled.
/// Progress can outlive the HTTP future while blocking database work finishes.
pub(crate) struct AuditCompletion {
    command: String,
    headers: HeaderMap,
    cookie_name: &'static str,
    now: u64,
    pub status: u16,
    pub progress: Arc<WriteProgress>,
}
impl AuditCompletion {
    pub fn new(command: &str, headers: &HeaderMap, cookie_name: &'static str, now: u64) -> Self {
        Self {
            command: command.into(),
            headers: headers.clone(),
            cookie_name,
            now,
            // Log-only convention: the caller cancelled, so no HTTP response exists.
            status: 499,
            progress: Arc::new(WriteProgress::default()),
        }
    }
}
impl Drop for AuditCompletion {
    fn drop(&mut self) {
        let mut record = WriteAudit::new(&self.command, &self.headers, self.cookie_name, self.now);
        record.status = self.status;
        let _ = self.progress.audit.set(record);
    }
}
impl WriteAudit {
    pub fn new(command: &str, headers: &HeaderMap, cookie_name: &str, now: u64) -> Self {
        let token = headers
            .get("cookie")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| {
                s.split(';')
                    .filter_map(|c| c.trim().split_once('='))
                    .find(|(name, _)| *name == cookie_name)
            })
            .map(|(_, v)| v)
            .unwrap_or("");
        Self {
            now,
            session: session_fingerprint(token),
            command: WRITE_COMMANDS
                .iter()
                .copied()
                .find(|c| *c == command)
                .expect("allowlisted audit"),
            status: 500,
        }
    }
}
pub(crate) fn session_fingerprint(token:&str)->u64 {
    use std::hash::BuildHasher;
    static HASH:std::sync::LazyLock<std::collections::hash_map::RandomState>=std::sync::LazyLock::new(std::collections::hash_map::RandomState::new);
    HASH.hash_one(token)&0xffff_ffff
}
impl WriteProgress {
    pub fn set_file_ids(&self,ids:&[i64]) { *self.file_ids.lock().unwrap()=ids.to_vec(); }
    fn line(&self,audit:&WriteAudit)->String {
        let metric=if FILE_OPS_COMMANDS.contains(&audit.command) {
            // Completed logical items include successful no-ops, not changed rows.
            format!("completed_items={}",self.completed_items.load(Ordering::Relaxed))
        }else{format!("changes={}",self.changes.load(Ordering::Relaxed))};
        let ids = self.file_ids.lock().unwrap();
        let detail = if audit.command == "empty_trash_start" {
            format!("count={} ids={:?}", ids.len(), &ids[..ids.len().min(10)])
        } else { format!("ids={:?}", *ids) };
        format!("write time={} session={:08x} command={} status={} {} {}",audit.now,audit.session,audit.command,audit.status,metric,detail)
    }
}
impl Drop for WriteProgress {
    fn drop(&mut self) {
        if let Some(audit)=self.audit.get(){eprintln!("{}",self.line(audit));}
    }
}
#[cfg(test)]
mod fileops_audit_tests {
    use super::*;
    #[test]
    fn single_record_correlates_fingerprint_ids_and_completed_items_without_contents() {
        let mut headers=HeaderMap::new();headers.insert("cookie","katalog_session=secret-session".parse().unwrap());
        headers.insert("x-name","private-model.3mf".parse().unwrap());
        let file=WriteAudit::new("rename_file",&headers,"katalog_session",100);
        let metadata=WriteAudit::new("set_favorite",&headers,"katalog_session",100);
        assert_eq!(file.session,metadata.session);assert_eq!(file.session,session_fingerprint("secret-session"));
        let progress=WriteProgress::default();progress.set_file_ids(&[12,34]);progress.completed_items.store(2,Ordering::Relaxed);
        let line=progress.line(&file);assert!(line.contains("ids=[12, 34]"));assert!(line.contains("completed_items=2"));assert!(!line.contains("changes="));
        assert!(!line.contains("secret-session"));assert!(!line.contains("private-model"));
    }
    #[test]
    fn empty_trash_start_logs_count_and_only_first_ten_ids() {
        let start=WriteAudit{now:1,session:2,command:"empty_trash_start",status:200};
        let progress=WriteProgress::default();
        progress.set_file_ids(&(1..=500).collect::<Vec<i64>>());
        let line=progress.line(&start);
        assert!(line.contains("count=500"));
        assert!(line.contains("ids=[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]"));
        assert!(!line.contains("11,"));
    }
}
pub(crate) fn retry(status: u16) -> Response {
    let mut r = super::web::error(status, "Schreibzugriff derzeit ausgelastet");
    r.headers_mut().insert("retry-after", "1".parse().unwrap());
    r
}
fn map_error(e: WriteError) -> Box<Response> {
    Box::new(match e {
        WriteError::Invalid { message } => super::web::error(422, &message),
        WriteError::NotFound { message } => super::web::error(404, &message),
        WriteError::Busy { .. } => retry(503),
        WriteError::Other { kind, .. } => {
            eprintln!("write error_kind={kind}");
            super::web::error(500, "Interner Schreibfehler")
        }
    })
}
fn bad() -> Box<Response> {
    Box::new(super::web::error(400, "Ungültige Argumente"))
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Box<Response>> {
    v.get(key).and_then(Value::as_str).ok_or_else(bad)
}
fn optional(v: &Value, key: &str) -> Result<Option<String>, Box<Response>> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(bad()),
    }
}
fn number(v: &Value, key: &str) -> Result<f64, Box<Response>> {
    v.get(key).and_then(Value::as_f64).ok_or_else(bad)
}
fn id_value(v: &Value) -> Result<i64, Box<Response>> {
    v.as_str()
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|i| *i > 0)
        .ok_or_else(bad)
}
fn id(v: &Value, key: &str) -> Result<i64, Box<Response>> {
    id_value(v.get(key).ok_or_else(bad)?)
}
fn ids(v: &Value) -> Result<Vec<i64>, Box<Response>> {
    let values = v.get("fileIds").and_then(Value::as_array).ok_or_else(bad)?;
    if values.len() > 500 {
        return Err(Box::new(super::web::error(
            422,
            "Höchstens 500 IDs erlaubt",
        )));
    }
    values.iter().map(id_value).collect()
}
fn updates(v: &Value) -> Result<Vec<(i64, i64)>, Box<Response>> {
    let values = v.get("updates").and_then(Value::as_array).ok_or_else(bad)?;
    if values.len() > 500 {
        return Err(Box::new(super::web::error(
            422,
            "Höchstens 500 IDs erlaubt",
        )));
    }
    values
        .iter()
        .map(|u| {
            Ok((
                id(u, "fileId")?,
                u.get("position").and_then(Value::as_i64).ok_or_else(bad)?,
            ))
        })
        .collect()
}
fn encode<T: Serialize>(
    r: Result<web::Outcome<T>, WriteError>,
) -> Result<(Response, u64), Box<Response>> {
    let result = r.map_err(map_error)?;
    Ok((Json(result.value).into_response(), result.changes))
}
fn spool(a: &Value) -> Result<katalog_core::read::manager::FilamentSpoolDto, Box<Response>> {
    let value = a.get("spool").ok_or_else(bad)?;
    let object = value.as_object().ok_or_else(bad)?;
    const FIELDS: &[&str] = &[
        "id",
        "material",
        "manufacturer",
        "color",
        "location",
        "diameterMm",
        "originalWeightG",
        "remainingWeightG",
        "price",
        "imagePng",
        "colorHex",
        "colorHexes",
        "homeLocation",
        "unitId",
        "slotIndex",
        "kind",
    ];
    if object.keys().any(|k| !FIELDS.contains(&k.as_str())) {
        return Err(bad());
    }
    serde_json::from_value(value.clone()).map_err(|_| bad())
}
fn printer_details(a: &Value) -> Result<katalog_core::write::printers::PrinterDetails, Box<Response>> {
    let value = a.get("details").ok_or_else(bad)?;
    let object = value.as_object().ok_or_else(bad)?;
    const FIELDS: &[&str] = &["manufacturer", "model", "nozzleMm", "bedXMm", "bedYMm", "bedZMm"];
    if object.keys().any(|k| !FIELDS.contains(&k.as_str())) {
        return Err(map_error(WriteError::expected("Unbekanntes Druckerdetail")));
    }
    serde_json::from_value(value.clone()).map_err(|_| bad())
}
fn manager_ids(a: &Value, key: &str) -> Result<Vec<i64>, Box<Response>> {
    a.get(key)
        .and_then(Value::as_array)
        .ok_or_else(bad)?
        .iter()
        .map(id_value)
        .collect()
}
fn slots(a: &Value) -> Result<Option<i64>, Box<Response>> {
    match a.get("slotCount") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_i64().map(Some).ok_or_else(bad),
    }
}
pub(crate) fn removal_ids(args: &Value) -> Result<Vec<i64>, Box<Response>> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Args { file_ids: Vec<String> }
    let invalid = || map_error(WriteError::expected("Ungültige Katalog-ID."));
    let args: Args = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
    if args.file_ids.len() > 200 {
        return Err(map_error(WriteError::expected("Höchstens 200 IDs erlaubt")));
    }
    args.file_ids.into_iter().map(|id| id_value(&Value::String(id)).map_err(|_| invalid())).collect()
}
pub(crate) fn execute(
    c: &mut Connection,
    cmd: &str,
    a: &Value,
) -> Result<(Response, u64), Box<Response>> {
    match cmd {
        "remove_files_from_catalog" => encode(web::remove_files_from_catalog(c, &removal_ids(a)?)),
        "set_render_snapshot" => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct SnapshotArgs { file_id: String, image_base64: String }
            let args: SnapshotArgs = serde_json::from_value(a.clone())
                .map_err(|_| map_error(WriteError::expected("Ungültige Snapshot-Argumente")))?;
            let file_id = id_value(&Value::String(args.file_id))?;
            encode(web::set_render_snapshot(c, file_id, &args.image_base64))
        }
        "add_filament_spool" => encode(web::add_filament_spool(c, spool(a)?)),
        "update_filament_spool" => {
            let s = spool(a)?;
            let id = id_value(&Value::String(s.id.clone()))?;
            encode(web::update_filament_spool(c, id, s, a["spool"].as_object().expect("validated spool").contains_key("imagePng")))
        }
        "delete_filament_spool" => encode(web::delete_filament_spool(c, id(a, "spoolId")?)),
        "restock_filament_spool" => encode(web::restock_filament_spool(
            c,
            id(a, "templateId")?,
            a.get("count").and_then(Value::as_i64).ok_or_else(bad)?,
            number(a, "weight")?,
            match a.get("price") {
                None | Some(Value::Null) => None,
                Some(v) => Some(v.as_f64().ok_or_else(bad)?),
            },
            optional(a, "location")?,
        )),
        "consume_resin" => encode(web::consume_resin(c, id(a, "spoolId")?, number(a, "amountMl")?)),
        "add_printer" => encode(web::add_printer(
            c,
            text(a, "name")?,
            text(a, "holderName")?,
            optional(a, "kind")?.as_deref().unwrap_or("filament"),
        )),
        "rename_printer" => encode(web::rename_printer(
            c,
            id(a, "printerId")?,
            text(a, "name")?,
        )),
        "update_printer_details" => encode(web::update_printer_details(
            c,
            id(a, "id")?,
            printer_details(a)?,
        )),
        "reorder_printers" => encode(web::reorder_printers(c, &manager_ids(a, "ids")?)),
        "delete_printer" => encode(web::delete_printer(c, id(a, "printerId")?)),
        "add_unit" => encode(web::add_unit(
            c,
            id(a, "printerId")?,
            text(a, "kind")?,
            text(a, "name")?,
            slots(a)?,
        )),
        "update_unit" => encode(web::update_unit(
            c,
            id(a, "unitId")?,
            text(a, "name")?,
            slots(a)?,
        )),
        "delete_unit" => encode(web::delete_unit(c, id(a, "unitId")?)),
        "reorder_units" => encode(web::reorder_units(
            c,
            id(a, "printerId")?,
            &manager_ids(a, "unitIds")?,
        )),
        "load_spool" => encode(web::load_spool(
            c,
            id(a, "spoolId")?,
            id(a, "unitId")?,
            a.get("slotIndex").and_then(Value::as_i64).ok_or_else(bad)?,
        )),
        "unload_spool" => encode(web::unload_spool(
            c,
            id(a, "spoolId")?,
            optional(a, "location")?,
        )),

        "set_favorite" => encode(web::set_favorite(
            c,
            id(a, "fileId")?,
            a.get("favorite").and_then(Value::as_bool).ok_or_else(bad)?,
        )),
        "set_print_status" => encode(web::set_print_status(
            c,
            id(a, "fileId")?,
            text(a, "status")?,
        )),
        "set_auto_tagging" => encode(web::set_auto_tagging(
            c,
            a.get("enabled").and_then(Value::as_bool).ok_or_else(bad)?,
        )),
        "block_auto_tag" => encode(web::block_auto_tag(c, text(a, "tag")?)),
        "unblock_auto_tag" => encode(web::unblock_auto_tag(c, text(a, "tag")?)),
        "mark_file_viewed" => encode(web::mark_file_viewed(c, id(a, "fileId")?)),
        "add_tag" => encode(web::add_tag(c, id(a, "fileId")?, text(a, "tag")?)),
        "remove_tag" => encode(web::remove_tag(c, id(a, "fileId")?, text(a, "tag")?)),
        "set_source_url" => encode(web::set_source_url(
            c,
            id(a, "fileId")?,
            optional(a, "url")?,
        )),
        "set_license" => encode(web::set_license(
            c,
            &ids(a)?,
            optional(a, "license")?.as_deref(),
        )),
        "add_print_log_entry" => encode(web::add_print_log_entry(
            c,
            id(a, "fileId")?,
            text(a, "printedAt")?.into(),
            optional(a, "note")?,
            optional(a, "photoBase64")?,
        )),
        "delete_print_log_entry" => encode(web::delete_print_log_entry(c, id(a, "entryId")?)),
        "add_to_queue" => encode(web::add_to_queue(c, id(a, "fileId")?)),
        "remove_from_queue" => encode(web::remove_from_queue(c, id(a, "fileId")?)),
        "reorder_queue" => encode(web::reorder_queue(c, &updates(a)?)),
        "create_collection" => encode(web::create_collection(c, text(a, "name")?)),
        "rename_collection" => encode(web::rename_collection(
            c,
            id(a, "collectionId")?,
            text(a, "name")?,
        )),
        "delete_collection" => encode(web::delete_collection(c, id(a, "collectionId")?)),
        "add_files_to_collection" => encode(web::add_files_to_collection(
            c,
            id(a, "collectionId")?,
            &ids(a)?,
        )),
        "remove_file_from_collection" => encode(web::remove_file_from_collection(
            c,
            id(a, "collectionId")?,
            id(a, "fileId")?,
        )),
        "reorder_collection" => encode(web::reorder_collection(
            c,
            id(a, "collectionId")?,
            &updates(a)?,
        )),
        _ => Err(Box::new(super::web::error(
            404,
            "Befehl im Web nicht verfügbar",
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_hash_uses_only_the_authenticated_cookie_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            "katalog_session=one; __Host-katalog_session=secure"
                .parse()
                .unwrap(),
        );
        let a = WriteAudit::new("set_favorite", &headers, "__Host-katalog_session", 100);
        headers.insert(
            "cookie",
            "katalog_session=two; __Host-katalog_session=secure"
                .parse()
                .unwrap(),
        );
        let b = WriteAudit::new("set_favorite", &headers, "__Host-katalog_session", 100);
        assert_eq!(a.session, b.session);
        headers.insert("cookie", "__Host-katalog_session=secure".parse().unwrap());
        assert_eq!(
            a.session,
            WriteAudit::new("set_favorite", &headers, "__Host-katalog_session", 100).session
        );
    }
}

#[cfg(test)]
mod filament_audit_tests {
    use super::*;
    #[test]
    fn restock_and_consume_are_audited_with_their_row_count() {
        let headers = HeaderMap::new();
        for command in ["restock_filament_spool", "consume_resin"] {
            let audit = WriteAudit::new(command, &headers, "katalog_session", 1);
            let progress = WriteProgress::default();
            progress.changes.store(3, Ordering::Relaxed);
            let line = progress.line(&audit);
            assert!(line.contains(&format!("command={command}")) && line.contains("changes=3"), "{line}");
        }
    }
}

#[cfg(test)]
mod auto_tag_audit_tests {
    use super::*;
    #[test]
    fn block_and_unblock_are_audited_with_their_row_count() {
        let headers = HeaderMap::new();
        for command in ["block_auto_tag", "unblock_auto_tag"] {
            let audit = WriteAudit::new(command, &headers, "katalog_session", 1);
            let progress = WriteProgress::default();
            progress.changes.store(1, Ordering::Relaxed);
            let line = progress.line(&audit);
            assert!(line.contains(&format!("command={command}")) && line.contains("changes=1"), "{line}");
        }
    }
}
