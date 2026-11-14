//! Tauri commands for the slicer reports: the settings page (switch, command lines, test report), the
//! list of reports with the suggested model, and the decision. The logic and the tables live in
//! `katalog_core::slicer_report` and `db::slicer_reports`; a report never imports, moves or assigns
//! anything by itself. Importing the reported file is a separate, explicit step of the interface through
//! the normal import command.
use super::*;
use katalog_core::slicer_report as core;
use serde::Serialize;
use tauri::{Emitter, Manager};

pub(crate) const SLICER_REPORTS_EVENT: &str = "slicer-reports-changed";

/// Slicers whose post-processing script takes the command line.
const SLICERS: [&str; 5] = ["PrusaSlicer", "SuperSlicer", "OrcaSlicer", "Bambu Studio", "Creality Print"];

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlicerCommandDto {
    pub slicer: String,
    pub command_line: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlicerReportSetupDto {
    pub enabled: bool,
    pub program_path: Option<String>,
    /// The intake is on and the command lines were made for another program path than the running one.
    pub path_changed: bool,
    pub commands: Vec<SlicerCommandDto>,
    pub open_count: usize,
    /// The last failed report call, if any.
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReportModelDto {
    pub file_id: String,
    pub file_name: String,
    pub sure: bool,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlicerReportDto {
    pub id: String,
    pub reported_at: String,
    pub reported_path: String,
    pub file_name: String,
    pub file_size_bytes: i64,
    pub slicer: Option<String>,
    pub print_seconds: Option<u64>,
    pub weight_g: Option<f64>,
    pub filament_types: Vec<String>,
    /// `open`, `linked` or `dismissed`.
    pub state: String,
    pub is_test: bool,
    /// Open reports: the model the name points to (`sure` = same normalized name).
    pub suggestion: Option<ReportModelDto>,
    /// Linked reports: the model the user chose.
    pub model: Option<ReportModelDto>,
    pub decided_at: Option<String>,
}

fn inbox_dir(state: &AppState) -> std::path::PathBuf {
    state.db_path.parent().unwrap_or_else(|| std::path::Path::new("")).join(core::INBOX_DIR)
}

/// The file the user started. In an AppImage the running binary sits in a temporary mount that changes
/// with every start; `APPIMAGE` names the file itself.
fn program_path() -> Option<String> {
    let appimage = std::env::var_os("APPIMAGE").filter(|v| !v.is_empty()).map(std::path::PathBuf::from);
    let path = appimage.or_else(|| std::env::current_exe().ok())?;
    let text = path.to_string_lossy().into_owned();
    (!text.is_empty()).then_some(text)
}

/// `"<program>" --report-sliced`; the slicer appends the path of the file. A path with a quote or a
/// control character cannot be written as a command line.
fn command_line(program: &str) -> Option<String> {
    (!program.chars().any(|c| c == '"' || c.is_control())).then(|| format!("\"{program}\" {}", core::REPORT_FLAG))
}

fn setup_dto(conn: &Connection, inbox: &std::path::Path, program: Option<String>) -> CmdResult<SlicerReportSetupDto> {
    let enabled = db::slicer_reports::is_enabled(conn)?;
    let recorded = db::slicer_reports::recorded_exe(conn)?;
    let path_changed = enabled && recorded.is_some() && recorded != program;
    let line = program.as_deref().and_then(command_line);
    let open_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM slicer_reports WHERE state = 'open'", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(SlicerReportSetupDto {
        enabled,
        program_path: program,
        path_changed,
        commands: SLICERS.iter().map(|s| SlicerCommandDto { slicer: (*s).to_string(), command_line: line.clone() }).collect(),
        open_count: open_count as usize,
        last_error: core::last_error(inbox),
    })
}

fn reports_dto(conn: &Connection) -> CmdResult<Vec<SlicerReportDto>> {
    let candidates = core::model_candidates(conn)?;
    let name_of = |id: i64| -> Option<String> { conn.query_row("SELECT name FROM files WHERE id = ?1", [id], |r| r.get(0)).ok() };
    let mut out = Vec::new();
    for r in db::slicer_reports::list(conn)? {
        let summary: core::Summary = serde_json::from_str(&r.summary_json).unwrap_or_default();
        let open = r.state == "open";
        let suggestion = if open {
            core::suggestion(&candidates, &r.file_name).map(|m| ReportModelDto { file_id: m.file_id.to_string(), file_name: m.file_name.clone(), sure: core::is_sure(&m) })
        } else {
            None
        };
        let model = r.model_file_id.and_then(|id| name_of(id).map(|file_name| ReportModelDto { file_id: id.to_string(), file_name, sure: true }));
        let mut filament_types: Vec<String> = Vec::new();
        for t in summary.filaments.iter().filter_map(|f| f.filament_type.clone()) {
            if !filament_types.contains(&t) {
                filament_types.push(t);
            }
        }
        out.push(SlicerReportDto {
            id: r.id.to_string(),
            reported_at: r.reported_at,
            reported_path: r.reported_path,
            file_name: r.file_name,
            file_size_bytes: r.file_size_bytes,
            slicer: r.slicer,
            print_seconds: summary.print_seconds,
            weight_g: summary.weight_g,
            filament_types,
            state: r.state,
            is_test: summary.test,
            suggestion,
            model,
            decided_at: r.decided_at,
        });
    }
    Ok(out)
}

/// Takes over what the slicers dropped into the inbox and tells the interface when something is new.
/// Called at the start and when a second start wakes the program; never fails the caller.
pub(crate) fn ingest_and_notify<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let outcome = {
        let Ok(conn) = state.db.lock() else { return };
        core::ingest_inbox(&conn, &inbox_dir(&state))
    };
    match outcome {
        Ok(o) if o.added + o.merged > 0 => {
            if let Err(error) = app.emit(SLICER_REPORTS_EVENT, ()) {
                log::warn!(target: "slicer_report", "could not notify the frontend: {error}");
            }
        }
        Ok(_) => {}
        Err(error) => log::warn!(target: "slicer_report", "could not take over the reports: {error}"),
    }
}

#[tauri::command]
pub fn get_slicer_report_setup(state: State<AppState>) -> CmdResult<SlicerReportSetupDto> {
    let conn = lock_db(&state)?;
    setup_dto(&conn, &inbox_dir(&state), program_path())
}

/// Switches the intake on or off. Switching on remembers the program path of the command lines shown, so
/// a later move of the program is noticed; switching on again after a move makes them current.
#[tauri::command]
pub fn set_slicer_report_enabled(state: State<AppState>, enabled: bool) -> CmdResult<SlicerReportSetupDto> {
    let conn = lock_db(&state)?;
    let program = program_path();
    db::slicer_reports::set_enabled(&conn, enabled, program.as_deref())?;
    setup_dto(&conn, &inbox_dir(&state), program)
}

/// A report of the settings page that takes the same way as a real one. Only while the intake is on.
#[tauri::command]
pub fn send_slicer_test_report(state: State<AppState>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    if !db::slicer_reports::is_enabled(&conn)? {
        return Err(katalog_core::db::error::DbError::coded(katalog_core::error_code::ErrorCode::SlicerReportActionInvalid).into());
    }
    let inbox = inbox_dir(&state);
    core::write_test_ticket(&inbox).map_err(|e| e.to_string())?;
    core::ingest_inbox(&conn, &inbox)?;
    Ok(())
}

/// Open reports and the last handled ones. Takes over new tickets first, so the list is current.
#[tauri::command]
pub fn list_slicer_reports(state: State<AppState>) -> CmdResult<Vec<SlicerReportDto>> {
    let conn = lock_db(&state)?;
    core::ingest_inbox(&conn, &inbox_dir(&state))?;
    reports_dto(&conn)
}

/// The user's decision: `link` (with the model) or `dismiss`. The file itself is not touched.
#[tauri::command]
pub fn resolve_slicer_report(state: State<AppState>, id: String, action: String, model_file_id: Option<String>) -> CmdResult<()> {
    use katalog_core::error_code::ErrorCode;
    let conn = lock_db(&state)?;
    let report: i64 = id.parse().map_err(|_| katalog_core::db::error::DbError::coded(ErrorCode::SlicerReportNotFound))?;
    let now = chrono::Utc::now().to_rfc3339();
    match action.as_str() {
        "link" => {
            let model: i64 = model_file_id
                .and_then(|m| m.parse().ok())
                .ok_or_else(|| katalog_core::db::error::DbError::coded(ErrorCode::SlicerReportModelInvalid))?;
            db::slicer_reports::link(&conn, report, model, &now)?;
        }
        "dismiss" => db::slicer_reports::dismiss(&conn, report, &now)?,
        _ => return Err(katalog_core::db::error::DbError::coded(ErrorCode::SlicerReportActionInvalid).into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        crate::db::connect_in_memory().unwrap()
    }

    fn add_report(conn: &Connection, path: &str) -> i64 {
        db::slicer_reports::insert_or_merge(conn, &db::slicer_reports::NewReport {
            reported_at: "2026-11-05T10:00:00Z".into(),
            reported_path: path.into(),
            file_name: db::slicer_reports::last_component(path).into(),
            file_size_bytes: 5,
            slicer: Some("OrcaSlicer 2.4.2".into()),
            summary_json: r#"{"printSeconds":3600,"weightG":10.5,"filaments":[{"filamentType":"PLA"},{"filamentType":"PLA"},{"filamentType":"PETG"}]}"#.into(),
        })
        .unwrap()
        .0
    }

    #[test]
    fn the_command_line_carries_the_quoted_program_and_the_flag() {
        assert_eq!(command_line("C:\\Program Files\\3MF\\app.exe").unwrap(), "\"C:\\Program Files\\3MF\\app.exe\" --report-sliced");
        assert_eq!(command_line("/opt/app"), Some("\"/opt/app\" --report-sliced".to_string()));
        assert!(command_line("/opt/a\"b").is_none() && command_line("/opt/a\nb").is_none());
    }

    #[test]
    fn the_setup_notices_a_moved_program_only_while_the_intake_is_on() {
        let conn = conn();
        let inbox = std::env::temp_dir().join("sr-setup-none");
        let dto = |program: &str| setup_dto(&conn, &inbox, Some(program.to_string())).unwrap();
        assert!(!dto("/a/app").enabled && !dto("/a/app").path_changed);
        db::slicer_reports::set_enabled(&conn, true, Some("/a/app")).unwrap();
        assert!(!dto("/a/app").path_changed);
        let moved = dto("/b/app");
        assert!(moved.path_changed && moved.commands.len() == 5);
        assert_eq!(moved.commands[2].slicer, "OrcaSlicer");
        assert_eq!(moved.commands[0].command_line.as_deref(), Some("\"/b/app\" --report-sliced"));
        db::slicer_reports::set_enabled(&conn, true, Some("/b/app")).unwrap();
        assert!(!dto("/b/app").path_changed, "switching on again makes the command lines current");
        db::slicer_reports::set_enabled(&conn, false, Some("/c/app")).unwrap();
        assert!(!dto("/c/app").path_changed, "off: nothing to warn about");
    }

    #[test]
    fn the_list_suggests_a_model_for_open_reports_and_names_the_chosen_one_for_linked_ones() {
        let conn = conn();
        let model = crate::db::test_insert_minimal_file(&conn, "Phone Stand.3mf", None).unwrap();
        let sure = add_report(&conn, "/o/Phone Stand_PLA_0.2_1h.gcode");
        let unrelated = add_report(&conn, "/o/zzzzz.gcode");
        let list = reports_dto(&conn).unwrap();
        let s = list.iter().find(|r| r.id == sure.to_string()).unwrap();
        let hit = s.suggestion.as_ref().unwrap();
        assert_eq!((hit.file_id.as_str(), hit.sure), (model.to_string().as_str(), true));
        assert_eq!((s.print_seconds, s.weight_g, s.filament_types.clone()), (Some(3600), Some(10.5), vec!["PLA".to_string(), "PETG".to_string()]));
        assert!(list.iter().find(|r| r.id == unrelated.to_string()).unwrap().suggestion.is_none());
        db::slicer_reports::link(&conn, sure, model, "2026-11-05T11:00:00Z").unwrap();
        let list = reports_dto(&conn).unwrap();
        let linked = list.iter().find(|r| r.id == sure.to_string()).unwrap();
        assert!(linked.suggestion.is_none());
        assert_eq!(linked.model.as_ref().unwrap().file_name, "Phone Stand.3mf");
        assert_eq!(list[0].state, "open", "open reports come first");
    }
}
