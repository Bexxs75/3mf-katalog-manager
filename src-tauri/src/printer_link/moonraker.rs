//! Klipper/Moonraker: `/server/info` und `/server/history/list` lesen.
//! Nur lesende GET-Anfragen.

use serde_json::Value;

use super::{JobOutcome, LinkError, RemoteJob};

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
