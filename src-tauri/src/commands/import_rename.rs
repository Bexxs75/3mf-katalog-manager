//! Desktop commands for the optional date prefix on import. The naming and placement
//! logic lives in `katalog_core::import::rename`, shared with the import worker.
use super::*;

#[allow(unused_imports)] // Keep the module paths the desktop code and its tests use.
pub(super) use katalog_core::import::rename::{
    catalog_base, date_prefix, has_date_prefix, is_internal, race_point, PREFIX_LEN,
};
#[cfg(test)]
pub(super) use katalog_core::import::rename::set_race_hook;

pub(super) fn enabled(conn: &Connection) -> CmdResult<bool> {
    katalog_core::import::rename::enabled(conn).map_err(Into::into)
}

#[tauri::command]
pub fn get_import_rename_enabled(state: State<AppState>) -> CmdResult<bool> {
    let conn = lock_db(&state)?;
    enabled(&conn)
}

#[tauri::command]
pub fn set_import_rename_enabled(state: State<AppState>, enabled: bool) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    db::printer_link::set_setting(&conn, katalog_core::import::rename::SETTING, if enabled { "1" } else { "0" })
        .map_err(|e| e.to_string().into())
}
