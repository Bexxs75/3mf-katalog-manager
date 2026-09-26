//! Tauri commands for the "detailed log" switch (see `diagnostics::verbose`)
//! and for exporting a log excerpt for a bug report.

use super::*;
use crate::db::printer_link::{get_setting, set_setting};
use crate::diagnostics::{anonymize, export, form_url, verbose};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerboseLoggingDto {
    pub enabled: bool,
    pub until_ms: Option<i64>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Reads the switch, turns it off once it has expired and sets the log level
/// accordingly. Called at startup, hourly and by the commands.
pub(crate) fn apply_verbose_state(conn: &Connection) -> VerboseLoggingDto {
    let until = verbose::parse_until(get_setting(conn, verbose::SETTING_KEY).ok().flatten());
    let active = verbose::is_active(until, now_ms());
    if !active && until.is_some() {
        let _ = set_setting(conn, verbose::SETTING_KEY, "");
        log::info!(target: "start", "Ausführliches Protokoll automatisch ausgeschaltet");
    }
    log::set_max_level(verbose::level(active));
    VerboseLoggingDto { enabled: active, until_ms: if active { until } else { None } }
}

#[tauri::command]
pub fn get_verbose_logging(state: State<AppState>) -> CmdResult<VerboseLoggingDto> {
    let conn = lock_db(&state)?;
    Ok(apply_verbose_state(&conn))
}

#[tauri::command]
pub fn set_verbose_logging(state: State<AppState>, enabled: bool) -> CmdResult<VerboseLoggingDto> {
    let conn = lock_db(&state)?;
    let value = if enabled { verbose::until_from(now_ms()).to_string() } else { String::new() };
    set_setting(&conn, verbose::SETTING_KEY, &value).map_err(|e| e.to_string())?;
    log::info!(target: "start", "Ausführliches Protokoll {}", if enabled { "eingeschaltet" } else { "ausgeschaltet" });
    Ok(apply_verbose_state(&conn))
}

#[derive(Default)]
pub struct DiagnosticsState {
    /// Exactly the text the user saw in the preview; saving writes this, not a
    /// fresh read, so nothing unseen leaves the computer.
    pub last_preview: Mutex<Option<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BugReportInfoDto {
    pub version: String,
    pub os: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPreviewDto {
    pub segments: Vec<anonymize::Segment>,
    pub contains_debug: bool,
    pub replace_file_names: bool,
    pub empty: bool,
}

#[tauri::command]
pub fn get_bug_report_info() -> BugReportInfoDto {
    BugReportInfoDto {
        version: form_url::core_version(env!("CARGO_PKG_VERSION")).to_string(),
        os: form_url::current_os().to_string(),
    }
}

fn anonymize_context(app: &tauri::AppHandle, conn: &Connection, replace_file_names: bool) -> anonymize::Context {
    use tauri::Manager;
    let catalog_roots = db::list_folders(conn)
        .map(|fs| fs.into_iter().filter(|f| f.parent_id.is_none()).map(|f| f.path).collect())
        .unwrap_or_default();
    let printer_addresses = crate::db::printer_link::list_connections(conn)
        .map(|cs| cs.into_iter().map(|c| c.address).collect())
        .unwrap_or_default();
    anonymize::Context {
        home: app.path().home_dir().ok().map(|p| p.to_string_lossy().into_owned()),
        user: std::env::var("USER").or_else(|_| std::env::var("USERNAME")).ok(),
        host: gethostname::gethostname().into_string().ok(),
        catalog_roots,
        printer_addresses,
        replace_file_names,
    }
}

#[tauri::command]
pub fn preview_log_export(
    app: tauri::AppHandle,
    state: State<AppState>,
    diag: State<DiagnosticsState>,
    replace_file_names: Option<bool>,
) -> CmdResult<LogPreviewDto> {
    use tauri::Manager;
    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    let raw = export::collect_tail(&export::read_logs(&dir), export::EXPORT_LIMIT);
    let contains_debug = export::contains_debug(&raw);
    let replace = replace_file_names.unwrap_or(contains_debug);
    let ctx = {
        let conn = lock_db(&state)?;
        anonymize_context(&app, &conn, replace)
    };
    let segments = anonymize::anonymize(&raw, &ctx);
    *diag.last_preview.lock().map_err(|_| "preview lock poisoned".to_string())? = Some(anonymize::to_text(&segments));
    Ok(LogPreviewDto { empty: raw.trim().is_empty(), segments, contains_debug, replace_file_names: replace })
}

#[tauri::command]
pub fn save_log_export(app: tauri::AppHandle, diag: State<DiagnosticsState>) -> CmdResult<String> {
    use tauri::Manager;
    let text = diag
        .last_preview
        .lock()
        .map_err(|_| "preview lock poisoned".to_string())?
        .clone()
        .ok_or_else(|| "no preview to save".to_string())?;
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    let path = export::free_file_name(&dir, &chrono::Local::now().format("%Y-%m-%d").to_string());
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    log::info!(target: "start", "Logdatei für Fehlerbericht gespeichert");
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn open_log_folder(app: tauri::AppHandle) -> CmdResult<()> {
    use tauri::Manager;
    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    super::security::open_external(&dir.to_string_lossy())
}

#[tauri::command]
pub fn open_bug_report_form(lang: String, with_log: bool) -> CmdResult<()> {
    let url = form_url::bug_report_url(&lang, env!("CARGO_PKG_VERSION"), form_url::current_os(), with_log);
    super::security::open_external(&url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_setting_is_cleared_and_reported_off() {
        let conn = crate::db::connect_in_memory().unwrap();
        set_setting(&conn, verbose::SETTING_KEY, "1").unwrap(); // 1970 → long expired
        let dto = apply_verbose_state(&conn);
        assert_eq!(dto, VerboseLoggingDto { enabled: false, until_ms: None });
        assert_eq!(get_setting(&conn, verbose::SETTING_KEY).unwrap().as_deref(), Some(""));
    }
}
