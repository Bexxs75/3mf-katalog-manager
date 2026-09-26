//! Tauri commands for the "detailed log" switch (see `diagnostics::verbose`).

use super::*;
use crate::db::printer_link::{get_setting, set_setting};
use crate::diagnostics::verbose;

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
