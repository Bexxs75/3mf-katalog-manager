//! What the UI sees of the printer connection: the serialized shapes (camelCase
//! JSON) and the database-only logic behind the commands (open prints with
//! suggestions, preview, confirmation, thumbnail plan). No framework types:
//! errors are plain strings, the caller maps them to its own error type.

use rusqlite::Connection;
use crate::printer_link::sync::unix_now;
use serde::{Deserialize, Serialize};

use crate::db::printer_link as store;
use crate::printer_link::booking::{self, Decision};
use crate::printer_link::matching::{best_match, Candidate, MatchKind};

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PrinterConnectionDto {
    pub printer_id: String,
    pub kind: String,
    pub address: String,
    pub base_url: Option<String>,
    pub remote_version: Option<String>,
    pub connected_since: f64,
    pub last_synced_at: Option<f64>,
    pub last_error: Option<String>,
    pub error_since: Option<f64>,
    pub paused: bool,
    pub clock_offset_s: f64,
    /// Failed attempts in a row.
    pub failures: i64,
    /// Whether a key is stored. The key itself never leaves the database layer.
    pub has_api_key: bool,
    /// `ok`, `reconnecting`, `offline`, `error` or `paused` (`printer_link::status`).
    pub status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResultDto {
    pub ok: bool,
    pub error: Option<String>,
    pub connection: Option<PrinterConnectionDto>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelMatchDto {
    pub file_id: String,
    pub file_name: String,
    pub sure: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OpenPrinterJobDto {
    pub id: String,
    pub printer_id: String,
    pub printer_name: String,
    pub file_name: String,
    pub outcome: String,
    pub raw_status: String,
    pub ended_at: f64,
    pub print_duration_s: f64,
    pub used_mm: f64,
    pub partial_percent: Option<u8>,
    pub material: Option<String>,
    pub has_thumbnail: bool,
    pub suggested_spool_id: Option<String>,
    pub grams: Option<f64>,
    pub material_mismatch: bool,
    pub model_match: Option<ModelMatchDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobPreviewDto {
    pub grams: f64,
    pub material_mismatch: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDecisionDto {
    pub job_id: String,
    pub spool_id: String,
    pub file_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmResultDto {
    pub confirmed: usize,
    pub failed: usize,
}

/// Parses an ID the UI sends as a string.
pub fn parse_id(value: &str, what: &str) -> Result<i64, String> {
    value.parse().map_err(|_| format!("ungueltige {what}-ID"))
}

pub fn connection_dto(c: store::PrinterConnectionRecord) -> PrinterConnectionDto {
    let status = crate::printer_link::status::connection_status(&c, unix_now()).as_str().to_string();
    PrinterConnectionDto {
        printer_id: c.printer_id.to_string(),
        kind: c.kind,
        address: c.address,
        base_url: c.base_url,
        remote_version: c.remote_version,
        connected_since: c.connected_since,
        last_synced_at: c.last_synced_at,
        last_error: c.last_error,
        error_since: c.error_since,
        paused: c.paused,
        clock_offset_s: c.clock_offset_s,
        failures: c.failures,
        has_api_key: c.has_api_key,
        status,
    }
}

pub fn list_printer_connections(conn: &Connection) -> Result<Vec<PrinterConnectionDto>, String> {
    Ok(store::list_connections(conn).map_err(|e| e.to_string())?.into_iter().map(connection_dto).collect())
}

pub fn list_open_printer_jobs(conn: &Connection) -> Result<Vec<OpenPrinterJobDto>, String> {
    let jobs = store::list_open_jobs(conn).map_err(|e| e.to_string())?;
    if jobs.is_empty() {
        return Ok(Vec::new());
    }
    // Models only: a G-code or resin entry of the catalog would win by name and take the print away
    // from the model.
    let candidates: Vec<Candidate> = crate::slicer_report::model_candidates(conn).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(jobs.len());
    for j in jobs {
        let printer_name: String = conn
            .query_row("SELECT name FROM printers WHERE id = ?1", [j.printer_id], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let suggested = booking::suggest_spool(conn, j.printer_id).map_err(|e| e.to_string())?;
        let spool = match suggested {
            Some(sid) => booking::spool_info(conn, sid).map_err(|e| e.to_string())?,
            None => None,
        };
        let grams = spool.as_ref().map(|s| {
            booking::grams(j.used_mm, j.slicer_total_mm, j.slicer_weight_g, s.diameter_mm, &s.material)
        });
        let material_mismatch = spool
            .as_ref()
            .is_some_and(|s| !booking::materials_match(j.material.as_deref(), &s.material));
        // A report the user confirmed names the model by the sliced file, no guessing from the name.
        let confirmed = crate::db::slicer_reports::confirmed_model_for_name(conn, &j.file_name).map_err(|e| e.to_string())?;
        let model_match = match confirmed {
            Some((file_id, file_name)) => Some(ModelMatchDto { file_id: file_id.to_string(), file_name, sure: true }),
            None => best_match(&j.file_name, &candidates).map(|m| ModelMatchDto {
                file_id: m.file_id.to_string(),
                file_name: m.file_name,
                sure: m.kind == MatchKind::Sure,
            }),
        };
        out.push(OpenPrinterJobDto {
            id: j.id.to_string(),
            printer_id: j.printer_id.to_string(),
            printer_name,
            partial_percent: if j.outcome == "partial" { booking::partial_percent(j.used_mm, j.slicer_total_mm) } else { None },
            file_name: j.file_name,
            outcome: j.outcome,
            raw_status: j.raw_status,
            ended_at: j.ended_at,
            print_duration_s: j.print_duration_s,
            used_mm: j.used_mm,
            material: j.material,
            has_thumbnail: j.thumbnail_path.is_some(),
            suggested_spool_id: suggested.map(|s| s.to_string()),
            grams,
            material_mismatch,
            model_match,
        });
    }
    Ok(out)
}

pub fn preview_printer_job(conn: &Connection, job_id: &str, spool_id: &str) -> Result<JobPreviewDto, String> {
    let job = store::get_job(conn, parse_id(job_id, "Druck")?)
        .map_err(|e| e.to_string())?
        .ok_or("Druck nicht gefunden")?;
    let spool_id = parse_id(spool_id, "Spulen")?;
    booking::ensure_filament(conn, spool_id).map_err(|e| e.to_string())?;
    let spool = booking::spool_info(conn, spool_id)
        .map_err(|e| e.to_string())?
        .ok_or("Spule nicht gefunden")?;
    Ok(JobPreviewDto {
        grams: booking::grams(job.used_mm, job.slicer_total_mm, job.slicer_weight_g, spool.diameter_mm, &spool.material),
        material_mismatch: !booking::materials_match(job.material.as_deref(), &spool.material),
    })
}

/// Gate before the connection test: if the printer connection is off, not a
/// single packet goes to a printer, not even for the test. `Some(...)` = return the
/// result directly, `None` = continue. Resin printers and unknown IDs give an
/// error before anything goes to the network.
pub fn test_printer_connection_gate(conn: &Connection, printer_id: i64) -> Result<Option<TestResultDto>, String> {
    crate::db::printers::ensure_filament_printer(conn, printer_id).map_err(|e| e.to_string())?;
    if store::printer_link_enabled(conn).map_err(|e| e.to_string())? {
        Ok(None)
    } else {
        Ok(Some(TestResultDto { ok: false, error: Some("disabled".into()), connection: None }))
    }
}

/// `decided_at` is the timestamp (RFC 3339) stored with every decision.
pub fn confirm_printer_jobs(conn: &mut Connection, decisions: Vec<JobDecisionDto>, decided_at: &str) -> Result<ConfirmResultDto, String> {
    let parsed = decisions
        .iter()
        .map(|d| {
            Ok(Decision {
                job_id: parse_id(&d.job_id, "Druck")?,
                spool_id: parse_id(&d.spool_id, "Spulen")?,
                file_id: d.file_id.as_deref().map(|f| parse_id(f, "Modell")).transpose()?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let (confirmed, failed) = booking::confirm_many(conn, &parsed, decided_at);
    Ok(ConfirmResultDto { confirmed, failed })
}

/// Decides without network access whether a thumbnail may be fetched: `None` if
/// the switch is off, the print/path/connection is missing or the connection is
/// paused (only a new connection test lifts that).
pub fn thumbnail_fetch_plan(
    conn: &Connection,
    job_id: &str,
) -> Result<Option<(store::PrinterConnectionRecord, String)>, String> {
    let jid = parse_id(job_id, "Druck")?;
    if !store::printer_link_enabled(conn).map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let Some(job) = store::get_job(conn, jid).map_err(|e| e.to_string())? else { return Ok(None) };
    let Some(path) = job.thumbnail_path else { return Ok(None) };
    let Some(c) = store::get_connection(conn, job.printer_id).map_err(|e| e.to_string())? else { return Ok(None) };
    if c.paused {
        return Ok(None);
    }
    Ok(Some((c, path)))
}

/// Upper limit of thumbnail fetches running at once. Each one opens its own
/// HTTP client (threads, sockets); hundreds in parallel exhaust the process's
/// file handles.
pub const MAX_CONCURRENT_THUMBNAIL_FETCHES: usize = 4;

/// Counting semaphore on std primitives; the slot is returned on drop, so also
/// when the fetch fails or panics.
pub struct FetchLimiter {
    running: std::sync::Mutex<usize>,
    freed: std::sync::Condvar,
    max: usize,
}

pub struct FetchSlot<'a>(&'a FetchLimiter);

impl FetchLimiter {
    pub const fn new(max: usize) -> Self {
        FetchLimiter { running: std::sync::Mutex::new(0), freed: std::sync::Condvar::new(), max }
    }

    /// Blocks until a slot is free. Only call from a blocking thread.
    pub fn acquire(&self) -> FetchSlot<'_> {
        // A poisoned lock only means another holder panicked; the counter stays valid.
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        while *running >= self.max {
            running = self.freed.wait(running).unwrap_or_else(|e| e.into_inner());
        }
        *running += 1;
        FetchSlot(self)
    }
}

impl Drop for FetchSlot<'_> {
    fn drop(&mut self) {
        let mut running = self.0.running.lock().unwrap_or_else(|e| e.into_inner());
        *running -= 1;
        self.0.freed.notify_one();
    }
}

pub static THUMBNAIL_FETCHES: FetchLimiter = FetchLimiter::new(MAX_CONCURRENT_THUMBNAIL_FETCHES);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_limiter_never_exceeds_its_maximum_and_frees_slots_on_panic() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let limiter = Arc::new(FetchLimiter::new(3));
        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = std::sync::mpsc::channel();
        for i in 0..12 {
            let (limiter, current, peak, tx) = (limiter.clone(), current.clone(), peak.clone(), tx.clone());
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(|| {
                    let _slot = limiter.acquire();
                    let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    current.fetch_sub(1, Ordering::SeqCst);
                    if i % 4 == 0 {
                        panic!("fetch failed");
                    }
                });
                let _ = tx.send(result.is_err());
            });
        }
        drop(tx);
        // Do not join blocked workers: a leaked slot must fail within a bounded time.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut panics = 0;
        for _ in 0..12 {
            let result = rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()));
            assert!(result.is_ok(), "slots were not released: workers did not finish within 5 seconds");
            panics += usize::from(result.unwrap());
        }
        assert_eq!(panics, 3);
        assert!(peak.load(Ordering::SeqCst) <= 3, "peak {}", peak.load(Ordering::SeqCst));
        assert_eq!(*limiter.running.lock().unwrap_or_else(|e| e.into_inner()), 0);
    }

    #[test]
    fn dto_json_keys_are_camel_case() {
        let value = serde_json::to_value(ConfirmResultDto { confirmed: 1, failed: 2 }).unwrap();
        assert_eq!(value, serde_json::json!({"confirmed": 1, "failed": 2}));
        let value = serde_json::to_value(JobPreviewDto { grams: 1.5, material_mismatch: true }).unwrap();
        assert_eq!(value, serde_json::json!({"grams": 1.5, "materialMismatch": true}));
        let decision: JobDecisionDto = serde_json::from_str(r#"{"jobId":"1","spoolId":"2","fileId":null}"#).unwrap();
        assert_eq!((decision.job_id.as_str(), decision.spool_id.as_str(), decision.file_id), ("1", "2", None));
    }

    #[test]
    fn invalid_ids_give_the_known_message() {
        assert_eq!(parse_id("x", "Druck"), Err("ungueltige Druck-ID".to_string()));
        assert_eq!(parse_id("12", "Druck"), Ok(12));
    }
}
