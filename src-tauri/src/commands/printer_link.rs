//! Tauri commands of the printer connection. Network work runs in `spawn_blocking`, the database isn't locked meanwhile.

use super::*;
use tauri::Manager;

use crate::db::printer_link as store;
use crate::printer_link::address::AddressPolicy;
use crate::printer_link::booking;
use crate::printer_link::sync::{unix_now, SyncWaker};
use crate::printer_link::{make_link, LinkError};

pub use crate::printer_link::views::{
    ConfirmResultDto, JobDecisionDto, JobPreviewDto, OpenPrinterJobDto, PrinterConnectionDto,
    TestResultDto,
};
use crate::printer_link::views::{self, connection_dto, parse_id as id};

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn wake(app: &tauri::AppHandle) {
    if let Some(w) = app.try_state::<SyncWaker>() {
        w.wake();
    }
}

pub(crate) fn list_printer_connections_with_conn(conn: &Connection) -> CmdResult<Vec<PrinterConnectionDto>> {
    Ok(views::list_printer_connections(conn)?)
}

pub(crate) fn list_open_printer_jobs_with_conn(conn: &Connection) -> CmdResult<Vec<OpenPrinterJobDto>> {
    Ok(views::list_open_printer_jobs(conn)?)
}

pub(crate) fn preview_printer_job_with_conn(conn: &Connection, job_id: &str, spool_id: &str) -> CmdResult<JobPreviewDto> {
    Ok(views::preview_printer_job(conn, job_id, spool_id)?)
}

/// Gate before `test_printer_connection`, see `views::test_printer_connection_gate`.
pub(crate) fn test_printer_connection_gate_with_conn(conn: &Connection, printer_id: i64) -> CmdResult<Option<TestResultDto>> {
    Ok(views::test_printer_connection_gate(conn, printer_id)?)
}

pub(crate) fn confirm_printer_jobs_with_conn(conn: &mut Connection, decisions: Vec<JobDecisionDto>) -> CmdResult<ConfirmResultDto> {
    Ok(views::confirm_printer_jobs(conn, decisions, &now_rfc3339())?)
}

#[tauri::command]
pub fn get_printer_link_enabled(state: State<AppState>) -> CmdResult<bool> {
    let conn = lock_db(&state)?;
    store::printer_link_enabled(&conn).map_err(|e| e.to_string().into())
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

fn test_link(kind: &str, address: &str, container: bool) -> Result<crate::printer_link::ConnectionInfo, LinkError> {
    use crate::printer_link::address::{check_container_address, explain_address_error};
    check_container_address(address, container)?;
    make_link(kind, address, AddressPolicy::HOME_NETWORK)
        .and_then(|link| link.test())
        .map_err(|error| explain_address_error(address, container, error))
}

#[tauri::command]
pub async fn test_printer_connection(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    printer_id: String,
    kind: String,
    address: String,
    environment: State<'_, RuntimeEnvironment>,
) -> CmdResult<TestResultDto> {
    let pid = id(&printer_id, "Drucker")?;
    {
        let conn = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
        if let Some(disabled) = test_printer_connection_gate_with_conn(&conn, pid)? {
            return Ok(disabled);
        }
    }
    let container = environment.container;
    let (k, a) = (kind.clone(), address.trim().to_string());
    let tested = tauri::async_runtime::spawn_blocking(move || {
        test_link(&k, &a, container)
    })
    .await
    .map_err(|e| e.to_string())?;
    match tested {
        Ok(info) => {
            let conn = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
            store::save_connection_after_test(&conn, pid, &kind, address.trim(), &info.base_url, &info.version, unix_now())
                .map_err(|e| e.to_string())?;
            store::set_clock_offset(&conn, pid, info.clock_offset_s).map_err(|e| e.to_string())?;
            log::info!(target: "drucker", "Uhr des Druckers weicht {:.0} s ab", info.clock_offset_s);
            let saved = store::get_connection(&conn, pid)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "Verbindung fehlt nach dem Speichern".to_string())?;
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
    store::delete_connection(&conn, id(&printer_id, "Drucker")?).map_err(|e| e.to_string().into())
}

#[tauri::command]
pub fn sync_printers_now(app: tauri::AppHandle) -> CmdResult<()> {
    wake(&app);
    Ok(())
}

// async + spawn_blocking, because several queries run per open print. `State`
// can't move into the `'static` closure, hence the `AppHandle` (as in the
// background sync).
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

pub(crate) fn thumbnail_fetch_plan_with_conn(
    conn: &Connection,
    job_id: &str,
) -> CmdResult<Option<(store::PrinterConnectionRecord, String)>> {
    Ok(views::thumbnail_fetch_plan(conn, job_id)?)
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
        let _slot = views::THUMBNAIL_FETCHES.acquire();
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
    booking::ignore_job(&conn, id(&job_id, "Druck")?, &now_rfc3339()).map_err(|e| e.to_string().into())
}

// async for the same reason as `list_open_printer_jobs`.
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

    #[test]
    fn container_local_names_are_rejected_before_connecting() {
        for address in ["printer.local", "PRINTER.LOCAL.:7125", " printer.local:80 "] {
            assert_eq!(test_link("moonraker", address, true), Err(LinkError::ContainerLocalName));
            assert!(crate::printer_link::address::check_container_address(address, false).is_ok());
        }
    }

    #[test]
    fn loopback_hint_only_decorates_container_address_rejections() {
        for address in ["127.0.0.1", "localhost", "[::1]:7125", "::ffff:127.0.0.1"] {
            assert_eq!(test_link("moonraker", address, true), Err(LinkError::ContainerLoopback));
            assert_eq!(test_link("moonraker", address, false), Err(LinkError::AddressNotAllowed));
        }
        assert_eq!(test_link("moonraker", "8.8.8.8", true), Err(LinkError::AddressNotAllowed));
        assert_eq!(test_link("moonraker", "http://localhost", true), Err(LinkError::AddressNotAllowed));
    }

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

    // The switch must block EVERY network access, not just the sync.
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

    // Paused connections only deliver thumbnails again after a new test.
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
