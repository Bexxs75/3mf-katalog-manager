//! Klipper/Moonraker: `/server/info` und `/server/history/list` lesen.
//! Nur lesende GET-Anfragen.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;

use super::address::{base_url, resolve, AddressPolicy};
use super::{ConnectionInfo, JobOutcome, LinkError, PrinterLink, RemoteJob};

fn bad(msg: &str) -> LinkError {
    LinkError::BadResponse(msg.to_string())
}

/// Liefert die Moonraker-Version. Verlangt die Komponente `history`.
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
    /// Gesamtzahl laut Drucker (für das Blättern).
    pub count: u64,
    /// Anzahl Einträge auf dieser Seite, auch übersprungene.
    pub raw_len: usize,
    /// Beendete, lesbare Drucke dieser Seite.
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
                eprintln!("[printer_link] Moonraker-Auftrag uebersprungen (unvollstaendig): {:?}", j.get("job_id"));
            }
            parsed
        })
        .collect();
    Ok(HistoryPage { count, raw_len: raw.len(), jobs })
}

fn positive(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|x| *x > 0.0)
}

/// `None` = nicht übernehmen (läuft noch oder Pflichtfeld fehlt).
fn parse_job(j: &Value) -> Option<RemoteJob> {
    let status = j.get("status")?.as_str()?;
    if status == "in_progress" {
        return None;
    }
    let ended_at = j.get("end_time")?.as_f64()?;
    let remote_id = j.get("job_id")?.as_str()?.to_string();
    let used_mm = j.get("filament_used")?.as_f64()?;
    let filename = j.get("filename").and_then(Value::as_str).unwrap_or("");
    let (dir, file_name) = match filename.rsplit_once('/') {
        Some((d, f)) => (Some(d), f),
        None => (None, filename),
    };
    let meta = j.get("metadata");
    let material = meta
        .and_then(|m| m.get("filament_type"))
        .and_then(Value::as_str)
        .map(|s| s.split(';').next().unwrap_or("").trim().to_string())
        .filter(|s| !s.is_empty());
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
/// Moonraker filtert `since` nach dem Auftragsbeginn; ein Druck, der vor
/// `since` begann und danach endete, würde sonst fehlen.
pub const LOOKBACK_S: f64 = 2.0 * 24.0 * 3600.0;

pub struct MoonrakerLink {
    address: String,
    policy: AddressPolicy,
    client: reqwest::blocking::Client,
}

impl MoonrakerLink {
    pub fn new(address: &str, policy: AddressPolicy) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(TIMEOUT)
            .connect_timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("HTTP-Client");
        MoonrakerLink { address: address.to_string(), policy, client }
    }

    /// Zuerst Port 80 (Mainsail/nginx), dann 7125 (Moonraker direkt), außer
    /// die Adresse nennt selbst einen Port.
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

    fn get_bytes(&self, url: reqwest::Url, limit: u64) -> Result<Vec<u8>, LinkError> {
        let resp = self.client.get(url).send().map_err(|_| LinkError::Unreachable)?;
        match resp.status().as_u16() {
            401 | 403 => return Err(LinkError::AuthRequired),
            s if !(200..300).contains(&s) => return Err(LinkError::BadResponse(format!("HTTP {s}"))),
            _ => {}
        }
        let mut buf = Vec::new();
        resp.take(limit + 1).read_to_end(&mut buf).map_err(|_| LinkError::Unreachable)?;
        if buf.len() as u64 > limit {
            return Err(LinkError::BadResponse("Antwort zu gross".into()));
        }
        Ok(buf)
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
            match self.get_json(url(&base, "/server/info")?).and_then(|v| parse_server_info(&v)) {
                Ok(version) => return Ok(ConnectionInfo { version, base_url: base }),
                Err(e @ (LinkError::AuthRequired | LinkError::HistoryMissing)) => return Err(e),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn jobs_ended_since(&self, base: &str, since: f64) -> Result<Vec<RemoteJob>, LinkError> {
        self.check_base(base)?;
        let query_since = (since - LOOKBACK_S).max(0.0);
        let mut jobs = Vec::new();
        let mut start = 0usize;
        for _ in 0..MAX_PAGES {
            let page = parse_history_page(&self.get_json(url(
                base,
                &format!("/server/history/list?since={query_since}&order=asc&limit={PAGE}&start={start}"),
            )?)?)?;
            jobs.extend(page.jobs.into_iter().filter(|j| j.ended_at > since));
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
    fn missing_result_is_a_bad_response() {
        assert!(matches!(parse_history_page(&serde_json::json!({"error": "x"})), Err(LinkError::BadResponse(_))));
    }
}

#[cfg(test)]
mod client_tests {
    use super::*;
    use crate::printer_link::address::AddressPolicy;
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
        // Port 1 auf Loopback ist praktisch immer geschlossen.
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
        let base = link.test().unwrap().base_url;
        let jobs = link.jobs_ended_since(&base, 1_788_970_000.0).unwrap();
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
        let base = link.test().unwrap().base_url;
        assert_eq!(link.jobs_ended_since(&base, 0.0).unwrap().len(), 120);
    }

    #[test]
    fn a_foreign_base_url_is_refused() {
        let server = sv08();
        let link = MoonrakerLink::new(&server.address(), AddressPolicy::TEST);
        assert_eq!(link.jobs_ended_since("http://8.8.8.8", 0.0), Err(LinkError::AddressNotAllowed));
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
}
