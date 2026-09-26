//! Klipper/Moonraker: reads `/server/info` and `/server/history/list`. Read-only GET requests.

use std::io::Read;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::address::{base_url, resolve, AddressPolicy};
use super::sync::unix_now;
use super::{ConnectionInfo, JobOutcome, LinkError, PrinterLink, RemoteJob};

fn bad(msg: &str) -> LinkError {
    LinkError::BadResponse(msg.to_string())
}

/// Returns the Moonraker version. Requires the `history` component.
pub fn parse_server_info(v: &Value) -> Result<String, LinkError> {
    let result = v.get("result").ok_or_else(|| bad("result fehlt"))?;
    let version = result
        .get("moonraker_version")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("moonraker_version fehlt"))?;
    let has_history = result
        .get("components")
        .and_then(Value::as_array)
        .is_some_and(|c| c.iter().any(|x| x.as_str() == Some("history")));
    if !has_history {
        return Err(LinkError::HistoryMissing);
    }
    Ok(version.to_string())
}

pub struct HistoryPage {
    /// Total count according to the printer (for paging).
    pub count: u64,
    /// Number of entries on this page, including skipped ones.
    pub raw_len: usize,
    /// Finished, readable prints on this page.
    pub jobs: Vec<RemoteJob>,
}

pub fn parse_history_page(v: &Value) -> Result<HistoryPage, LinkError> {
    let result = v.get("result").ok_or_else(|| bad("result fehlt"))?;
    let count = result.get("count").and_then(Value::as_u64).ok_or_else(|| bad("count fehlt"))?;
    let raw = result.get("jobs").and_then(Value::as_array).ok_or_else(|| bad("jobs fehlt"))?;
    let jobs = raw
        .iter()
        .filter_map(|j| {
            let parsed = parse_job(j);
            if parsed.is_none() && j.get("status").and_then(Value::as_str) != Some("in_progress") {
                eprintln!("[printer_link] skipped incomplete Moonraker job: {:?}", j.get("job_id"));
            }
            parsed
        })
        .collect();
    Ok(HistoryPage { count, raw_len: raw.len(), jobs })
}

fn positive(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|x| *x > 0.0)
}

/// Material of a print. Multi-material units (AMS, ACE, MMU) list every loaded slot
/// in `filament_type` ("PLA;PETG"); when `filament_weights` has one weight per slot,
/// the slot that used the most filament wins, so the suggested spool fits.
fn main_material(meta: &Value) -> Option<String> {
    let types: Vec<&str> = meta.get("filament_type")?.as_str()?.split(';').map(str::trim).collect();
    let weights: Vec<f64> = meta
        .get("filament_weights")
        .and_then(Value::as_array)
        .map(|w| w.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect())
        .unwrap_or_default();
    let mut index = 0;
    if weights.len() == types.len() {
        for (i, w) in weights.iter().enumerate() {
            if *w > weights[index] {
                index = i;
            }
        }
    }
    Some(types[index].to_string()).filter(|s| !s.is_empty())
}

/// `None` = don't take over (still running, a required field is missing, or no
/// filament was used - e.g. cancelled right at the start - so nothing to deduct).
fn parse_job(j: &Value) -> Option<RemoteJob> {
    let status = j.get("status")?.as_str()?;
    if status == "in_progress" {
        return None;
    }
    let ended_at = j.get("end_time")?.as_f64()?;
    let remote_id = j.get("job_id")?.as_str()?.to_string();
    let used_mm = j.get("filament_used")?.as_f64()?;
    if used_mm <= 0.0 {
        return None;
    }
    let filename = j.get("filename").and_then(Value::as_str).unwrap_or("");
    let (dir, file_name) = match filename.rsplit_once('/') {
        Some((d, f)) => (Some(d), f),
        None => (None, filename),
    };
    let meta = j.get("metadata");
    let material = meta.and_then(main_material);
    let thumbnail_path = meta
        .and_then(|m| m.get("thumbnails"))
        .and_then(Value::as_array)
        .and_then(|thumbs| {
            thumbs
                .iter()
                .filter_map(|t| {
                    let w = t.get("width")?.as_u64()?;
                    let h = t.get("height")?.as_u64()?;
                    let p = t.get("relative_path")?.as_str()?;
                    Some((w * h, p))
                })
                .max_by_key(|(area, _)| *area)
                .map(|(_, p)| match dir {
                    Some(d) => format!("{d}/{p}"),
                    None => p.to_string(),
                })
        });
    Some(RemoteJob {
        remote_id,
        file_name: file_name.to_string(),
        outcome: if status == "completed" { JobOutcome::Completed } else { JobOutcome::Partial },
        raw_status: status.to_string(),
        ended_at,
        print_duration_s: j.get("print_duration").and_then(Value::as_f64).unwrap_or(0.0),
        used_mm,
        slicer_total_mm: positive(meta.and_then(|m| m.get("filament_total"))),
        slicer_weight_g: positive(meta.and_then(|m| m.get("filament_weight_total"))),
        material,
        thumbnail_path,
    })
}

const TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;
const MAX_THUMB_BYTES: u64 = 512 * 1024;
const PAGE: usize = 50;
const MAX_PAGES: usize = 40;
/// Moonraker filters `since` by the job start; a print that started before
/// `since` and ended after it would otherwise be missing.
pub const LOOKBACK_S: f64 = 2.0 * 24.0 * 3600.0;
/// Smaller clock differences are ignored: the HTTP `Date` header only has whole
/// seconds, and a few minutes of drift don't matter for finished prints.
pub const CLOCK_TOLERANCE_S: f64 = 300.0;

/// `Date` header ("Mon, 11 Dec 2023 17:00:00 GMT") as Unix seconds.
fn parse_http_date(value: &str) -> Option<f64> {
    chrono::DateTime::parse_from_rfc2822(value.trim()).ok().map(|d| d.timestamp() as f64)
}

/// Printer clock minus local clock. `local_mid` is the local time halfway through
/// the request; the header is truncated to whole seconds, hence the half second.
fn clock_offset(printer_now: Option<f64>, local_mid: f64) -> f64 {
    match printer_now {
        Some(p) if (p + 0.5 - local_mid).abs() > CLOCK_TOLERANCE_S => p + 0.5 - local_mid,
        _ => 0.0,
    }
}

/// A response body plus the printer clock from its `Date` header (if any) and
/// the local time halfway through the request.
struct Fetched {
    body: Vec<u8>,
    printer_now: Option<f64>,
    local_mid: f64,
}

pub struct MoonrakerLink {
    address: String,
    policy: AddressPolicy,
    client: reqwest::blocking::Client,
    /// Total time limit of a request including the body; `TIMEOUT` (5 s) in
    /// production, tests set a shorter one via `new_with_timeout`.
    timeout: Duration,
}

impl MoonrakerLink {
    pub fn new(address: &str, policy: AddressPolicy) -> Self {
        Self::with_timeout(address, policy, TIMEOUT)
    }

    /// Tests only: own time limit instead of production's fixed 5 s.
    #[cfg(test)]
    pub(crate) fn new_with_timeout(address: &str, policy: AddressPolicy, timeout: Duration) -> Self {
        Self::with_timeout(address, policy, timeout)
    }

    fn with_timeout(address: &str, policy: AddressPolicy, timeout: Duration) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout)
            // Never use the system proxy (HTTP_PROXY/ALL_PROXY): the request must go to the
            // already checked `Target.ip`, not to a proxy that talks to a different address.
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("HTTP-Client");
        MoonrakerLink { address: address.to_string(), policy, client, timeout }
    }

    /// First port 80 (Mainsail/nginx), then 7125 (Moonraker directly), unless the address names a port itself.
    fn candidate_bases(&self) -> Result<Vec<String>, LinkError> {
        let target = resolve(&self.address, self.policy)?;
        Ok(match target.port {
            Some(p) => vec![base_url(target.ip, p)],
            None => vec![base_url(target.ip, 80), base_url(target.ip, 7125)],
        })
    }

    fn check_base(&self, base: &str) -> Result<(), LinkError> {
        if self.candidate_bases()?.iter().any(|b| b == base) {
            Ok(())
        } else {
            Err(LinkError::AddressNotAllowed)
        }
    }

    /// Fetches headers and body with a hard total limit of `self.timeout`.
    ///
    /// reqwest gives every single `read()` a fresh time limit; a trickling printer
    /// could reach a multiple of it. So the request runs on a helper thread, and here
    /// we only wait with `recv_timeout`. A still hanging helper thread finishes later
    /// and sends into the void, which is harmless.
    fn get_bytes(&self, url: reqwest::Url, limit: u64) -> Result<Vec<u8>, LinkError> {
        self.fetch(url, limit).map(|f| f.body)
    }

    fn fetch(&self, url: reqwest::Url, limit: u64) -> Result<Fetched, LinkError> {
        let client = self.client.clone();
        let timeout = self.timeout;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Self::fetch_bytes(&client, url, limit, timeout));
        });
        rx.recv_timeout(timeout).unwrap_or(Err(LinkError::Unreachable))
    }

    /// Network part of `get_bytes` on the helper thread: reads in chunks with its own time limit check.
    fn fetch_bytes(client: &reqwest::blocking::Client, url: reqwest::Url, limit: u64, timeout: Duration) -> Result<Fetched, LinkError> {
        let deadline = Instant::now() + timeout;
        let sent_at = unix_now();
        let mut resp = client.get(url).send().map_err(|_| LinkError::Unreachable)?;
        let local_mid = (sent_at + unix_now()) / 2.0;
        let printer_now = resp
            .headers()
            .get(reqwest::header::DATE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_http_date);
        match resp.status().as_u16() {
            401 | 403 => return Err(LinkError::AuthRequired),
            s if !(200..300).contains(&s) => return Err(LinkError::BadResponse(format!("HTTP {s}"))),
            _ => {}
        }
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            if Instant::now() >= deadline {
                return Err(LinkError::Unreachable);
            }
            let n = resp.read(&mut chunk).map_err(|_| LinkError::Unreachable)?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.len() as u64 > limit {
                return Err(LinkError::BadResponse("Antwort zu gross".into()));
            }
        }
        Ok(Fetched { body: buf, printer_now, local_mid })
    }

    fn get_json(&self, url: reqwest::Url) -> Result<Value, LinkError> {
        let bytes = self.get_bytes(url, MAX_JSON_BYTES)?;
        serde_json::from_slice(&bytes).map_err(|e| LinkError::BadResponse(e.to_string()))
    }
}

fn url(base: &str, path: &str) -> Result<reqwest::Url, LinkError> {
    reqwest::Url::parse(&format!("{base}{path}")).map_err(|e| LinkError::BadResponse(e.to_string()))
}

fn valid_thumbnail_path(path: &str) -> bool {
    let in_thumbs = path.starts_with(".thumbs/") || path.contains("/.thumbs/");
    in_thumbs
        && path.ends_with(".png")
        && !path.starts_with('/')
        && !path.split('/').any(|seg| seg == ".." || seg.is_empty())
        && !path.contains(['?', '#', '%', '\\'])
}

impl PrinterLink for MoonrakerLink {
    fn test(&self) -> Result<ConnectionInfo, LinkError> {
        let mut last = LinkError::Unreachable;
        for base in self.candidate_bases()? {
            let answer = self.fetch(url(&base, "/server/info")?, MAX_JSON_BYTES).and_then(|f| {
                let v: Value = serde_json::from_slice(&f.body).map_err(|e| LinkError::BadResponse(e.to_string()))?;
                Ok((parse_server_info(&v)?, clock_offset(f.printer_now, f.local_mid)))
            });
            match answer {
                Ok((version, clock_offset_s)) => return Ok(ConnectionInfo { version, base_url: base, clock_offset_s }),
                Err(e @ (LinkError::AuthRequired | LinkError::HistoryMissing)) => return Err(e),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn jobs_ended_since(&self, info: &ConnectionInfo, since: f64) -> Result<Vec<RemoteJob>, LinkError> {
        let base = info.base_url.as_str();
        self.check_base(base)?;
        // Moonraker's times come from the printer clock: shift `since` into it for
        // the query and shift the answers back into local time.
        let offset = info.clock_offset_s;
        let query_since = (since + offset - LOOKBACK_S).max(0.0);
        let mut jobs = Vec::new();
        let mut start = 0usize;
        for _ in 0..MAX_PAGES {
            let page = parse_history_page(&self.get_json(url(
                base,
                &format!("/server/history/list?since={query_since}&order=asc&limit={PAGE}&start={start}"),
            )?)?)?;
            jobs.extend(page.jobs.into_iter().filter_map(|mut j| {
                j.ended_at -= offset;
                (j.ended_at > since).then_some(j)
            }));
            start += PAGE;
            if page.raw_len < PAGE || start as u64 >= page.count {
                break;
            }
        }
        Ok(jobs)
    }

    fn thumbnail(&self, base: &str, path: &str) -> Result<Vec<u8>, LinkError> {
        self.check_base(base)?;
        if !valid_thumbnail_path(path) {
            return Err(LinkError::BadResponse("ungueltiger Bildpfad".into()));
        }
        let mut u = url(base, "/")?;
        {
            let mut segs = u.path_segments_mut().map_err(|_| LinkError::BadResponse("URL".into()))?;
            segs.clear().extend(["server", "files", "gcodes"]).extend(path.split('/'));
        }
        self.get_bytes(u, MAX_THUMB_BYTES)
    }
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    fn fixture(name: &str) -> serde_json::Value {
        let path = format!("{}/tests/fixtures/moonraker/{name}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn server_info_of_the_sv08_is_accepted() {
        assert_eq!(parse_server_info(&fixture("server_info_v0_8_0_209.json")).unwrap(), "v0.8.0-209-g4235789-dirty");
    }

    #[test]
    fn server_info_without_history_is_rejected() {
        assert_eq!(parse_server_info(&fixture("server_info_without_history.json")), Err(LinkError::HistoryMissing));
    }

    #[test]
    fn old_access_info_is_not_mistaken_for_server_info() {
        assert!(matches!(parse_server_info(&fixture("access_info_old.json")), Err(LinkError::BadResponse(_))));
    }

    #[test]
    fn completed_jobs_are_parsed() {
        let page = parse_history_page(&fixture("history_completed.json")).unwrap();
        assert_eq!(page.count, 64);
        assert_eq!(page.raw_len, 3);
        let j = &page.jobs[0];
        assert_eq!(j.remote_id, "00003F");
        assert_eq!(j.file_name, "Distanzhulse_13,40mm_PLA_0.2_6m29s.gcode");
        assert_eq!(j.outcome, JobOutcome::Completed);
        assert_eq!(j.used_mm, 303.265800000015);
        assert_eq!(j.slicer_total_mm, Some(279.17));
        assert_eq!(j.slicer_weight_g, Some(0.83));
        assert_eq!(j.material.as_deref(), Some("PLA"));
        assert_eq!(j.thumbnail_path.as_deref(), Some(".thumbs/Distanzhulse_13,40mm_PLA_0.2_6m29s-400x300.png"));
    }

    #[test]
    fn klippy_shutdown_is_partial_and_broken_or_running_jobs_are_skipped() {
        let page = parse_history_page(&fixture("history_klippy_shutdown.json")).unwrap();
        assert_eq!(page.raw_len, 3);
        assert_eq!(page.jobs.len(), 1);
        let j = &page.jobs[0];
        assert_eq!(j.outcome, JobOutcome::Partial);
        assert_eq!(j.raw_status, "klippy_shutdown");
        assert_eq!(j.used_mm, 1331.9600600000035);
        assert_eq!(j.thumbnail_path.as_deref(), Some(".thumbs/USB-3DBenchy_Fast_12m-300x300.png"));
    }

    #[test]
    fn thumbnails_of_files_in_subfolders_keep_the_folder() {
        let v = serde_json::json!({"result": {"count": 1, "jobs": [{"end_time": 2.0, "filament_used": 1.0, "filename": "ordner/x.gcode",
            "metadata": {"thumbnails": [{"width": 32, "height": 32, "relative_path": ".thumbs/x-32x32.png"}]},
            "print_duration": 1.0, "status": "cancelled", "job_id": "1"}]}});
        let page = parse_history_page(&v).unwrap();
        assert_eq!(page.jobs[0].file_name, "x.gcode");
        assert_eq!(page.jobs[0].thumbnail_path.as_deref(), Some("ordner/.thumbs/x-32x32.png"));
        assert_eq!(page.jobs[0].outcome, JobOutcome::Partial);
    }

    #[test]
    fn rinkhals_on_anycubic_kobra_s1_is_accepted() {
        // Real anonymous test report (Anycubic Kobra S1, Rinkhals, ACE Pro): Moonraker
        // version only "?", files in the .3mf_temp/ folder, material list of all ACE
        // slots in filament_type.
        assert_eq!(parse_server_info(&fixture("server_info_rinkhals_kobra_s1.json")).unwrap(), "?");
        let page = parse_history_page(&fixture("history_rinkhals_kobra_s1.json")).unwrap();
        assert_eq!(page.jobs.len(), 2);
        let j = page.jobs.iter().find(|j| j.remote_id == "000158").unwrap();
        assert_eq!(j.file_name, "S1_OrcaToleranceTest_PLA_12m28s.gcode");
        assert_eq!(j.outcome, JobOutcome::Completed);
        assert_eq!(j.material.as_deref(), Some("PLA"));
        assert_eq!(j.slicer_weight_g, Some(3.65));
    }

    #[test]
    fn second_kobra_s1_report_skips_a_print_without_usage() {
        // Real test report (Anycubic Kobra S1, Rinkhals, ACE Pro): a print cancelled right
        // at the start reports 0 mm; there is nothing to deduct, so it is not offered.
        assert_eq!(parse_server_info(&fixture("server_info_rinkhals_kobra_s1_ace.json")).unwrap(), "?");
        let page = parse_history_page(&fixture("history_rinkhals_kobra_s1_ace.json")).unwrap();
        assert_eq!(page.count, 5);
        let ids: Vec<&str> = page.jobs.iter().map(|j| j.remote_id.as_str()).collect();
        assert_eq!(ids, ["00001B", "00001A", "000019", "000018"]);
        let slide = &page.jobs[0];
        assert_eq!(slide.file_name, "0926-1506-slide(01)_PETG_0.12_2h56m50s.gcode");
        assert_eq!(slide.outcome, JobOutcome::Completed);
        assert_eq!(slide.slicer_weight_g, Some(66.69));
        assert_eq!(page.jobs[1].file_name, "Axle Cleaning Tool_plate_1(1).gcode");
        // "PLA;PETG" with weights [0.35, 66.34]: almost everything was PETG.
        assert_eq!(slide.material.as_deref(), Some("PETG"));
    }

    #[test]
    fn material_follows_the_heaviest_slot() {
        let job = |types: &str, weights: serde_json::Value| {
            parse_job(&serde_json::json!({"end_time": 2.0, "filament_used": 10.0, "filename": "x.gcode",
                "metadata": {"filament_type": types, "filament_weights": weights},
                "print_duration": 1.0, "status": "completed", "job_id": "1"}))
            .unwrap()
            .material
        };
        assert_eq!(job("PLA;PLA;ABS", serde_json::json!([0.0, 0.0, 5.0])).as_deref(), Some("ABS"));
        // Without usable weights (missing, wrong length, all zero) the first entry stays.
        assert_eq!(job("PLA;PETG", serde_json::Value::Null).as_deref(), Some("PLA"));
        assert_eq!(job("PLA;PETG", serde_json::json!([1.0])).as_deref(), Some("PLA"));
        assert_eq!(job("PLA;PETG", serde_json::json!([0.0, 0.0])).as_deref(), Some("PLA"));
        // Equal weights keep the earlier slot.
        assert_eq!(job("PLA;PETG", serde_json::json!([2.0, 2.0])).as_deref(), Some("PLA"));
    }

    #[test]
    fn a_finished_print_without_usage_is_skipped() {
        let v = serde_json::json!({"end_time": 2.0, "filament_used": 0.0, "filename": "x.gcode",
            "metadata": {}, "print_duration": 1.0, "status": "completed", "job_id": "1"});
        assert!(parse_job(&v).is_none());
    }

    #[test]
    fn old_moonraker_on_qidi_smart_3_is_accepted() {
        // Real anonymous test report (Qidi Smart 3, MKS-Pi image, Moonraker v0.7.1,
        // API 1.0.5). The printer clock is wrong: prints sliced with OrcaSlicer 2.4.2
        // (released July 2026) end in December 2023.
        assert_eq!(parse_server_info(&fixture("server_info_qidi_smart3.json")).unwrap(), "v0.7.1-609-gbdd0222-dirty");
        let page = parse_history_page(&fixture("history_qidi_smart3.json")).unwrap();
        assert_eq!(page.count, 442);
        assert_eq!(page.jobs.len(), 5);
        let j = page.jobs.iter().find(|j| j.remote_id == "0001B9").unwrap();
        assert_eq!(j.outcome, JobOutcome::Completed);
        assert_eq!(j.material.as_deref(), Some("PLA"));
        assert_eq!(j.slicer_weight_g, Some(39.13));
        assert!((j.used_mm - 13118.148).abs() < 0.01);
        assert!(j.ended_at < 1_704_067_200.0, "printer clock reports 2023");
    }

    #[test]
    fn missing_result_is_a_bad_response() {
        assert!(matches!(parse_history_page(&serde_json::json!({"error": "x"})), Err(LinkError::BadResponse(_))));
    }
}

#[cfg(test)]
mod client_tests {
    use super::*;
    use crate::printer_link::address::AddressPolicy;
    use crate::printer_link::fake_moonraker::qidi_smart3;
    use crate::printer_link::fake_moonraker::{fixture_bytes, sv08, FakeServer};
    use crate::printer_link::PrinterLink;

    #[test]
    fn test_connects_to_the_fake_sv08() {
        let server = sv08();
        let link = MoonrakerLink::new(&server.address(), AddressPolicy::TEST);
        let info = link.test().unwrap();
        assert_eq!(info.version, "v0.8.0-209-g4235789-dirty");
        assert_eq!(info.base_url, format!("http://{}", server.address()));
    }

    #[test]
    fn public_or_loopback_addresses_never_send_a_request() {
        let server = sv08();
        let link = MoonrakerLink::new(&server.address(), AddressPolicy::HOME_NETWORK);
        assert_eq!(link.test(), Err(LinkError::AddressNotAllowed));
        assert!(server.requests().is_empty());
    }

    #[test]
    fn http_401_means_auth_required() {
        let server = FakeServer::start(|_| Some((401, b"{}".to_vec())));
        let link = MoonrakerLink::new(&server.address(), AddressPolicy::TEST);
        assert_eq!(link.test(), Err(LinkError::AuthRequired));
    }

    #[test]
    fn unreachable_port_is_reported() {
        // Port 1 on loopback is practically always closed.
        let link = MoonrakerLink::new("127.0.0.1:1", AddressPolicy::TEST);
        assert_eq!(link.test(), Err(LinkError::Unreachable));
    }

    #[test]
    fn a_silent_printer_times_out_as_unreachable() {
        let server = FakeServer::start(|_| None);
        let link = MoonrakerLink::new(&server.address(), AddressPolicy::TEST);
        let started = std::time::Instant::now();
        assert_eq!(link.test(), Err(LinkError::Unreachable));
        assert!(started.elapsed() < std::time::Duration::from_secs(8));
    }

    #[test]
    fn jobs_are_filtered_by_end_time_and_query_uses_lookback() {
        let server = sv08();
        let link = MoonrakerLink::new(&server.address(), AddressPolicy::TEST);
        let info = link.test().unwrap();
        assert_eq!(info.clock_offset_s, 0.0, "no Date header -> no correction");
        let jobs = link.jobs_ended_since(&info, 1_788_970_000.0).unwrap();
        assert_eq!(jobs.iter().map(|j| j.remote_id.as_str()).collect::<Vec<_>>(), vec!["00003F", "00003E"]);
        let query = server.requests().into_iter().find(|r| r.starts_with("/server/history/list")).unwrap();
        let expected_since = 1_788_970_000.0 - LOOKBACK_S;
        assert!(query.contains(&format!("since={expected_since}")), "{query}");
        assert!(query.contains("order=asc") && query.contains("limit=50") && query.contains("start=0"));
    }

    #[test]
    fn history_is_paged() {
        let server = FakeServer::start(|target| {
            if target.starts_with("/server/history/list") {
                let start: usize = target.split("start=").nth(1).unwrap_or("0").split('&').next().unwrap().parse().unwrap();
                let jobs: Vec<String> = (start..(start + 50).min(120))
                    .map(|i| format!(r#"{{"end_time": {}.0, "filament_used": 1.0, "filename": "a.gcode", "metadata": {{}}, "print_duration": 1.0, "status": "completed", "job_id": "{i}"}}"#, 1000 + i))
                    .collect();
                Some((200, format!(r#"{{"result": {{"count": 120, "jobs": [{}]}}}}"#, jobs.join(",")).into_bytes()))
            } else {
                Some((200, fixture_bytes("server_info_v0_8_0_209.json")))
            }
        });
        let link = MoonrakerLink::new(&server.address(), AddressPolicy::TEST);
        let info = link.test().unwrap();
        assert_eq!(link.jobs_ended_since(&info, 0.0).unwrap().len(), 120);
    }

    #[test]
    fn http_dates_are_parsed() {
        assert_eq!(parse_http_date("Mon, 11 Dec 2023 17:00:00 GMT"), Some(1_702_314_000.0));
        assert_eq!(parse_http_date("kaputt"), None);
    }

    #[test]
    fn small_clock_differences_are_ignored() {
        assert_eq!(clock_offset(None, 1000.0), 0.0);
        assert_eq!(clock_offset(Some(1000.0 + CLOCK_TOLERANCE_S - 1.0), 1000.0), 0.0);
        assert_eq!(clock_offset(Some(1000.0 - 1_000_000.0), 1000.0), -999_999.5);
    }

    /// Qidi Smart 3 from a real test report: its clock is in December 2023 while
    /// the prints happened in 2026. Without the correction no print was ever found.
    #[test]
    fn a_printer_with_a_wrong_clock_still_delivers_its_prints() {
        let printer_now = 1_702_314_000.0; // 2023-12-11 17:00 UTC, printer time
        let offset = printer_now - unix_now();
        let server = qidi_smart3(offset);
        let link = MoonrakerLink::new(&server.address(), AddressPolicy::TEST);
        let info = link.test().unwrap();
        assert!((info.clock_offset_s - offset).abs() < 3.0, "{} vs {offset}", info.clock_offset_s);

        // "Connected" at 12:50 printer time = that moment in local time.
        let since_local = 1_702_299_000.0 - info.clock_offset_s;
        let jobs = link.jobs_ended_since(&info, since_local).unwrap();
        let mut ids: Vec<_> = jobs.iter().map(|j| j.remote_id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["0001B6", "0001B7", "0001B8", "0001B9"]);
        let b9 = jobs.iter().find(|j| j.remote_id == "0001B9").unwrap();
        assert!((b9.ended_at - (1_702_311_947.72 - info.clock_offset_s)).abs() < 0.01, "ended_at in local time");
        assert!(b9.ended_at > unix_now() - 3.0 * 3600.0, "local time is recent, not 2023");

        let query = server.requests().into_iter().find(|r| r.starts_with("/server/history/list")).unwrap();
        let queried: f64 = query.split("since=").nth(1).unwrap().split('&').next().unwrap().parse().unwrap();
        assert!((queried - (1_702_299_000.0 - LOOKBACK_S)).abs() < 1e-3, "query uses printer time: {query}");
    }

    #[test]
    fn a_foreign_base_url_is_refused() {
        let server = sv08();
        let link = MoonrakerLink::new(&server.address(), AddressPolicy::TEST);
        let foreign = ConnectionInfo { version: "v0".into(), base_url: "http://8.8.8.8".into(), clock_offset_s: 0.0 };
        assert_eq!(link.jobs_ended_since(&foreign, 0.0), Err(LinkError::AddressNotAllowed));
    }

    #[test]
    fn thumbnails_are_fetched_only_from_thumbs_folders() {
        let server = sv08();
        let link = MoonrakerLink::new(&server.address(), AddressPolicy::TEST);
        let base = link.test().unwrap().base_url;
        assert!(link.thumbnail(&base, ".thumbs/Distanzhulse_13,40mm_PLA_0.2_6m29s-300x300.png").unwrap().starts_with(b"\x89PNG"));
        assert!(server.requests().iter().any(|r| r.starts_with("/server/files/gcodes/.thumbs/Distanzhulse_13,40mm")));
        for bad in ["../x.png", ".thumbs/../../x.png", "x.png", ".thumbs/x.gcode", "/etc/.thumbs/x.png", ".thumbs/x.png?a=b"] {
            assert!(link.thumbnail(&base, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn oversized_answers_are_rejected() {
        let big = vec![b' '; (MAX_JSON_BYTES + 10) as usize];
        let server = FakeServer::start(move |_| Some((200, big.clone())));
        let link = MoonrakerLink::new(&server.address(), AddressPolicy::TEST);
        assert!(matches!(link.test(), Err(LinkError::BadResponse(_))));
    }

    /// A trickling printer must not block `get_bytes` indefinitely.
    #[test]
    fn a_dripping_body_times_out_as_unreachable_within_the_overall_deadline() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
            }
            // Announce a large Content-Length, then one byte every 30 ms.
            let head = "HTTP/1.1 200 X\r\nContent-Length: 1000000\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n";
            if stream.write_all(head.as_bytes()).is_err() {
                return;
            }
            for _ in 0..50 {
                if stream.write_all(b" ").is_err() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        });
        let link = MoonrakerLink::new_with_timeout(&format!("127.0.0.1:{port}"), AddressPolicy::TEST, Duration::from_millis(120));
        let started = std::time::Instant::now();
        assert_eq!(link.test(), Err(LinkError::Unreachable));
        assert!(started.elapsed() < std::time::Duration::from_secs(2), "{:?}", started.elapsed());
    }

    /// A single hanging `read()` shortly before the deadline must not extend the time
    /// limit: first byte after ~100 ms, then a 140 ms pause with a 150 ms limit.
    #[test]
    fn a_single_stalled_read_still_returns_unreachable_within_the_overall_deadline() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
            }
            let head = "HTTP/1.1 200 X\r\nContent-Length: 1000000\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n";
            if stream.write_all(head.as_bytes()).is_err() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            if stream.write_all(b" ").is_err() {
                return;
            }
            // The next `read()` call now hangs for ~140 ms - longer than the remaining
            // budget (150 ms total limit minus ~100 ms already elapsed).
            std::thread::sleep(std::time::Duration::from_millis(140));
            let _ = stream.write_all(b" ");
        });
        let link = MoonrakerLink::new_with_timeout(&format!("127.0.0.1:{port}"), AddressPolicy::TEST, Duration::from_millis(150));
        let started = std::time::Instant::now();
        assert_eq!(link.test(), Err(LinkError::Unreachable));
        assert!(started.elapsed() < Duration::from_millis(210), "{:?}", started.elapsed());
    }
}
