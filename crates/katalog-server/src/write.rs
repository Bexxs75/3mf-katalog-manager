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
pub const WRITE_COMMANDS: &[&str] = &[
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
];
pub(crate) fn image_command(command: &str) -> bool {
    matches!(
        command,
        "add_print_log_entry" | "add_filament_spool" | "update_filament_spool"
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
    pub conn: Mutex<Connection>,
    pub admission: Arc<Semaphore>,
    pub worker: Arc<Semaphore>,
}
impl WriteGate {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
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
        use std::hash::BuildHasher;
        static HASH: std::sync::LazyLock<std::collections::hash_map::RandomState> =
            std::sync::LazyLock::new(std::collections::hash_map::RandomState::new);
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
            session: HASH.hash_one(token) & 0xffff_ffff,
            command: WRITE_COMMANDS
                .iter()
                .copied()
                .find(|c| *c == command)
                .expect("allowlisted audit"),
            status: 500,
        }
    }
}
impl Drop for WriteProgress {
    fn drop(&mut self) {
        if let Some(audit) = self.audit.get() {
            eprintln!(
                "write time={} session={:08x} command={} status={} changes={}",
                audit.now,
                audit.session,
                audit.command,
                audit.status,
                self.changes.load(Ordering::Relaxed)
            );
        }
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
pub(crate) fn execute(
    c: &mut Connection,
    cmd: &str,
    a: &Value,
) -> Result<(Response, u64), Box<Response>> {
    match cmd {
        "add_filament_spool" => encode(web::add_filament_spool(c, spool(a)?)),
        "update_filament_spool" => {
            let s = spool(a)?;
            let id = id_value(&Value::String(s.id.clone()))?;
            encode(web::update_filament_spool(c, id, s, a["spool"].as_object().expect("validated spool").contains_key("imagePng")))
        }
        "delete_filament_spool" => encode(web::delete_filament_spool(c, id(a, "spoolId")?)),
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
