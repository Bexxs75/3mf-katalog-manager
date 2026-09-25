//! Tauri-Befehle der Druckeranbindung. Netzwerkarbeit läuft in
//! `spawn_blocking`, die Datenbank wird dabei nicht gesperrt.

use super::*;
use serde::Deserialize;
use tauri::Manager;

use crate::db::printer_link as store;
use crate::printer_link::address::AddressPolicy;
use crate::printer_link::booking::{self, Decision};
use crate::printer_link::matching::{best_match, Candidate, MatchKind};
use crate::printer_link::sync::{unix_now, SyncWaker};
use crate::printer_link::{make_link, LinkError};

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

fn id(value: &str, what: &str) -> CmdResult<i64> {
    value.parse().map_err(|_| format!("ungueltige {what}-ID"))
}

fn connection_dto(c: store::PrinterConnectionRecord) -> PrinterConnectionDto {
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
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn wake(app: &tauri::AppHandle) {
    if let Some(w) = app.try_state::<SyncWaker>() {
        w.wake();
    }
}

pub(crate) fn list_printer_connections_with_conn(conn: &Connection) -> CmdResult<Vec<PrinterConnectionDto>> {
    Ok(store::list_connections(conn).map_err(|e| e.to_string())?.into_iter().map(connection_dto).collect())
}

pub(crate) fn list_open_printer_jobs_with_conn(conn: &Connection) -> CmdResult<Vec<OpenPrinterJobDto>> {
    let jobs = store::list_open_jobs(conn).map_err(|e| e.to_string())?;
    if jobs.is_empty() {
        return Ok(Vec::new());
    }
    let candidates: Vec<Candidate> = conn
        .prepare("SELECT id, name, imported_at FROM files WHERE deleted_at IS NULL")
        .and_then(|mut s| {
            s.query_map([], |r| Ok(Candidate { file_id: r.get(0)?, name: r.get(1)?, imported_at: r.get(2)? }))?
                .collect()
        })
        .map_err(|e| e.to_string())?;
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
        let model_match = best_match(&j.file_name, &candidates).map(|m| ModelMatchDto {
            file_id: m.file_id.to_string(),
            file_name: m.file_name,
            sure: m.kind == MatchKind::Sure,
        });
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

pub(crate) fn preview_printer_job_with_conn(conn: &Connection, job_id: &str, spool_id: &str) -> CmdResult<JobPreviewDto> {
    let job = store::get_job(conn, id(job_id, "Druck")?)
        .map_err(|e| e.to_string())?
        .ok_or("Druck nicht gefunden")?;
    let spool_id = id(spool_id, "Spulen")?;
    booking::ensure_filament(conn, spool_id).map_err(|e| e.to_string())?;
    let spool = booking::spool_info(conn, spool_id)
        .map_err(|e| e.to_string())?
        .ok_or("Spule nicht gefunden")?;
    Ok(JobPreviewDto {
        grams: booking::grams(job.used_mm, job.slicer_total_mm, job.slicer_weight_g, spool.diameter_mm, &spool.material),
        material_mismatch: !booking::materials_match(job.material.as_deref(), &spool.material),
    })
}

/// Sperre vor `test_printer_connection`: ist die Druckeranbindung aus, geht kein
/// Paket an einen Drucker, auch nicht beim Test. `Some(...)` = Ergebnis direkt
/// zurueckgeben, `None` = weitermachen. Resin-Drucker und unbekannte IDs geben
/// einen Fehler, bevor etwas ins Netz geht.
pub(crate) fn test_printer_connection_gate_with_conn(conn: &Connection, printer_id: i64) -> CmdResult<Option<TestResultDto>> {
    db::printers::ensure_filament_printer(conn, printer_id).map_err(|e| e.to_string())?;
    if store::printer_link_enabled(conn).map_err(|e| e.to_string())? {
        Ok(None)
    } else {
        Ok(Some(TestResultDto { ok: false, error: Some("disabled".into()), connection: None }))
    }
}

pub(crate) fn confirm_printer_jobs_with_conn(conn: &mut Connection, decisions: Vec<JobDecisionDto>) -> CmdResult<ConfirmResultDto> {
    let parsed = decisions
        .iter()
        .map(|d| {
            Ok(Decision {
                job_id: id(&d.job_id, "Druck")?,
                spool_id: id(&d.spool_id, "Spulen")?,
                file_id: d.file_id.as_deref().map(|f| id(f, "Modell")).transpose()?,
            })
        })
        .collect::<CmdResult<Vec<_>>>()?;
    let (confirmed, failed) = booking::confirm_many(conn, &parsed, &now_rfc3339());
    Ok(ConfirmResultDto { confirmed, failed })
}

#[tauri::command]
pub fn get_printer_link_enabled(state: State<AppState>) -> CmdResult<bool> {
    let conn = lock_db(&state)?;
    store::printer_link_enabled(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_printer_link_enabled(app: tauri::AppHandle, state: State<AppState>, enabled: bool) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    store::set_printer_link_enabled(&conn, enabled).map_err(|e| e.to_string())?;
    drop(conn);
    if enabled {
        wake(&app);
    }
    Ok(())
}

#[tauri::command]
pub fn list_printer_connections(state: State<AppState>) -> CmdResult<Vec<PrinterConnectionDto>> {
    let conn = lock_db(&state)?;
    list_printer_connections_with_conn(&conn)
}

#[tauri::command]
pub async fn test_printer_connection(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    printer_id: String,
    kind: String,
    address: String,
) -> CmdResult<TestResultDto> {
    let pid = id(&printer_id, "Drucker")?;
    {
        let conn = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
        if let Some(disabled) = test_printer_connection_gate_with_conn(&conn, pid)? {
            return Ok(disabled);
        }
    }
    let (k, a) = (kind.clone(), address.trim().to_string());
    let tested = tauri::async_runtime::spawn_blocking(move || {
        make_link(&k, &a, AddressPolicy::HOME_NETWORK).and_then(|l| l.test())
    })
    .await
    .map_err(|e| e.to_string())?;
    match tested {
        Ok(info) => {
            let conn = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
            let saved = store::save_connection_after_test(&conn, pid, &kind, address.trim(), &info.base_url, &info.version, unix_now())
                .map_err(|e| e.to_string())?;
            drop(conn);
            wake(&app);
            Ok(TestResultDto { ok: true, error: None, connection: Some(connection_dto(saved)) })
        }
        Err(e) => Ok(TestResultDto { ok: false, error: Some(e.code().to_string()), connection: None }),
    }
}

#[tauri::command]
pub fn remove_printer_connection(state: State<AppState>, printer_id: String) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    store::delete_connection(&conn, id(&printer_id, "Drucker")?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn sync_printers_now(app: tauri::AppHandle) -> CmdResult<()> {
    wake(&app);
    Ok(())
}

// async + spawn_blocking, weil pro offenem Druck mehrere Abfragen laufen.
// `State` kann nicht in die `'static`-Closure, deshalb das `AppHandle` (wie im
// Hintergrund-Abgleich).
#[tauri::command]
pub async fn list_open_printer_jobs(app: tauri::AppHandle) -> CmdResult<Vec<OpenPrinterJobDto>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
        list_open_printer_jobs_with_conn(&conn)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn preview_printer_job(state: State<AppState>, job_id: String, spool_id: String) -> CmdResult<JobPreviewDto> {
    let conn = lock_db(&state)?;
    preview_printer_job_with_conn(&conn, &job_id, &spool_id)
}

/// Entscheidet ohne Netzwerkzugriff, ob ein Vorschaubild geholt werden darf:
/// `None` bei ausgeschaltetem Schalter, fehlendem Druck/Pfad/Verbindung oder
/// pausierter Verbindung (die hebt nur ein erneuter Verbindungstest auf).
pub(crate) fn thumbnail_fetch_plan_with_conn(
    conn: &Connection,
    job_id: &str,
) -> CmdResult<Option<(store::PrinterConnectionRecord, String)>> {
    let jid = id(job_id, "Druck")?;
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

#[tauri::command]
pub async fn get_printer_job_thumbnail(state: State<'_, AppState>, job_id: String) -> CmdResult<Option<String>> {
    let (connection, path) = {
        let conn = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
        match thumbnail_fetch_plan_with_conn(&conn, &job_id)? {
            Some(plan) => plan,
            None => return Ok(None),
        }
    };
    let bytes = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, LinkError> {
        let link = make_link(&connection.kind, &connection.address, AddressPolicy::HOME_NETWORK)?;
        let info = link.test()?;
        link.thumbnail(&info.base_url, &path)
    })
    .await
    .map_err(|e| e.to_string())?;
    use base64::Engine;
    Ok(bytes.ok().map(|b| base64::engine::general_purpose::STANDARD.encode(b)))
}

#[tauri::command]
pub fn ignore_printer_job(state: State<AppState>, job_id: String) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    booking::ignore_job(&conn, id(&job_id, "Druck")?, &now_rfc3339()).map_err(|e| e.to_string())
}

// async aus demselben Grund wie `list_open_printer_jobs`.
#[tauri::command]
pub async fn confirm_printer_jobs(app: tauri::AppHandle, decisions: Vec<JobDecisionDto>) -> CmdResult<ConfirmResultDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut conn = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
        confirm_printer_jobs_with_conn(&mut conn, decisions)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::printer_link::{insert_job_if_new, save_connection_after_test};
    use crate::printer_link::{JobOutcome, RemoteJob};

    fn setup() -> Connection {
        let conn = crate::db::connect_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO printers (id, name, position) VALUES (1, 'Sovol SV08', 0);
             INSERT INTO material_units (id, printer_id, name, kind, slot_count, position) VALUES (10, 1, 'Spulenhalter', 'external', 1, 0);
             INSERT INTO filament_spools (id, material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES (100, 'PLA', 1.75, 1000, 612.4, '2026-09-24', 10, 0);
             INSERT INTO filament_spools (id, material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                 VALUES (101, 'PETG', 1.75, 1000, 900, '2026-09-24');",
        )
        .unwrap();
        crate::db::test_insert_minimal_file(&conn, "Distanzhülse 13,40mm.3mf", None).unwrap();
        save_connection_after_test(&conn, 1, "moonraker", "192.168.1.60", "http://192.168.1.60", "v0.8.0", 1.0).unwrap();
        insert_job_if_new(&conn, 1, &RemoteJob {
            remote_id: "00003F".into(),
            file_name: "Distanzhulse_13,40mm_PLA_0.2_6m29s.gcode".into(),
            outcome: JobOutcome::Completed,
            raw_status: "completed".into(),
            ended_at: 1789042135.8,
            print_duration_s: 394.65,
            used_mm: 303.2658,
            slicer_total_mm: Some(279.17),
            slicer_weight_g: Some(0.83),
            material: Some("PLA".into()),
            thumbnail_path: Some(".thumbs/x-300x300.png".into()),
        })
        .unwrap();
        conn
    }

    #[test]
    fn open_jobs_come_with_suggestions() {
        let conn = setup();
        let jobs = list_open_printer_jobs_with_conn(&conn).unwrap();
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(j.printer_name, "Sovol SV08");
        assert_eq!(j.suggested_spool_id.as_deref(), Some("100"));
        assert_eq!(j.grams, Some(0.9));
        assert!(!j.material_mismatch);
        assert!(j.has_thumbnail);
        let m = j.model_match.as_ref().unwrap();
        assert!(m.sure);
        assert_eq!(m.file_name, "Distanzhülse 13,40mm.3mf");
    }

    #[test]
    fn preview_with_another_spool_warns_about_material() {
        let conn = setup();
        let job_id = list_open_printer_jobs_with_conn(&conn).unwrap()[0].id.clone();
        let p = preview_printer_job_with_conn(&conn, &job_id, "101").unwrap();
        assert!(p.material_mismatch);
        assert_eq!(p.grams, 0.9);
    }

    #[test]
    fn confirm_through_the_command_layer() {
        let mut conn = setup();
        let job_id = list_open_printer_jobs_with_conn(&conn).unwrap()[0].id.clone();
        let r = confirm_printer_jobs_with_conn(
            &mut conn,
            vec![JobDecisionDto { job_id, spool_id: "100".into(), file_id: None }],
        )
        .unwrap();
        assert_eq!((r.confirmed, r.failed), (1, 0));
        assert!(list_open_printer_jobs_with_conn(&conn).unwrap().is_empty());
    }

    #[test]
    fn resin_is_rejected_by_preview_and_confirm() {
        let mut conn = setup();
        conn.execute(
            "INSERT INTO filament_spools (id, kind, material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
             VALUES (300, 'resin', 'Standard', 1.75, 1000, 640.5, '2026-09-25')",
            [],
        )
        .unwrap();
        let job_id = list_open_printer_jobs_with_conn(&conn).unwrap()[0].id.clone();
        let err = preview_printer_job_with_conn(&conn, &job_id, "300").unwrap_err();
        assert!(err.contains("Resin"), "unerwartete Meldung: {err}");
        let r = confirm_printer_jobs_with_conn(
            &mut conn,
            vec![JobDecisionDto { job_id, spool_id: "300".into(), file_id: None }],
        )
        .unwrap();
        assert_eq!((r.confirmed, r.failed), (0, 1));
        assert_eq!(list_open_printer_jobs_with_conn(&conn).unwrap().len(), 1);
        let rest: f64 = conn.query_row("SELECT remaining_weight_g FROM filament_spools WHERE id = 300", [], |r| r.get(0)).unwrap();
        assert_eq!(rest, 640.5);
    }

    #[test]
    fn connections_are_listed_as_dtos() {
        let conn = setup();
        let list = list_printer_connections_with_conn(&conn).unwrap();
        assert_eq!(list[0].printer_id, "1");
        assert_eq!(list[0].address, "192.168.1.60");
        assert!(!list[0].paused);
    }

    #[test]
    fn invalid_ids_are_rejected() {
        let conn = setup();
        assert!(preview_printer_job_with_conn(&conn, "x", "100").is_err());
    }

    // Der Schalter muss JEDEN Netzwerkzugriff sperren, nicht nur den Abgleich.
    #[test]
    fn test_connection_is_blocked_while_switched_off() {
        let conn = setup();
        let gated = test_printer_connection_gate_with_conn(&conn, 1).unwrap();
        let dto = gated.expect("switch is off in setup(), gate must trigger");
        assert!(!dto.ok);
        assert_eq!(dto.error.as_deref(), Some("disabled"));
        assert!(dto.connection.is_none());
    }

    #[test]
    fn test_connection_is_allowed_once_switched_on() {
        let conn = setup();
        crate::db::printer_link::set_printer_link_enabled(&conn, true).unwrap();
        assert!(test_printer_connection_gate_with_conn(&conn, 1).unwrap().is_none());
    }

    #[test]
    fn a_resin_printer_never_gets_a_connection() {
        let conn = setup();
        crate::db::printer_link::set_printer_link_enabled(&conn, true).unwrap();
        let saturn = crate::db::printers::insert_printer_of_kind(&conn, "Saturn 4", "resin").unwrap();
        crate::db::printers::insert_resin_vat(&conn, saturn, "Harzwanne").unwrap();

        assert!(test_printer_connection_gate_with_conn(&conn, saturn).is_err(), "kein Netzwerkzugriff");
        assert!(save_connection_after_test(&conn, saturn, "moonraker", "192.168.1.70", "http://192.168.1.70", "v1", 1.0).is_err());
        assert!(list_printer_connections_with_conn(&conn).unwrap().iter().all(|c| c.printer_id != saturn.to_string()));
        assert!(test_printer_connection_gate_with_conn(&conn, 999).is_err());
    }

    // Pausierte Verbindungen liefern erst nach erneutem Test wieder Vorschaubilder.
    #[test]
    fn thumbnail_plan_is_none_while_switch_is_off() {
        let conn = setup();
        let job_id = list_open_printer_jobs_with_conn(&conn).unwrap()[0].id.clone();
        assert!(thumbnail_fetch_plan_with_conn(&conn, &job_id).unwrap().is_none());
    }

    #[test]
    fn thumbnail_plan_is_none_for_a_paused_connection() {
        let conn = setup();
        crate::db::printer_link::set_printer_link_enabled(&conn, true).unwrap();
        let job_id = list_open_printer_jobs_with_conn(&conn).unwrap()[0].id.clone();
        crate::db::printer_link::record_sync_error(&conn, 1, "auth_required", 2.0).unwrap();
        assert!(thumbnail_fetch_plan_with_conn(&conn, &job_id).unwrap().is_none());
    }

    #[test]
    fn thumbnail_plan_finds_the_connection_when_everything_is_ready() {
        let conn = setup();
        crate::db::printer_link::set_printer_link_enabled(&conn, true).unwrap();
        let job_id = list_open_printer_jobs_with_conn(&conn).unwrap()[0].id.clone();
        let (connection, path) = thumbnail_fetch_plan_with_conn(&conn, &job_id).unwrap().unwrap();
        assert_eq!(connection.address, "192.168.1.60");
        assert!(!connection.paused);
        assert_eq!(path, ".thumbs/x-300x300.png");
    }
}
