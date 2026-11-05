//! Tauri commands for "what do you print with": the choice only hides parts of
//! the interface, the logic and the stored values live in `katalog_core`.
use super::*;
use katalog_core::read::PrintTechnologyDto;

#[tauri::command]
pub fn get_print_technology(state: State<AppState>) -> CmdResult<PrintTechnologyDto> {
    let conn = lock_db(&state)?;
    katalog_core::read::get_print_technology(&conn).map_err(Into::into)
}

#[tauri::command]
pub fn set_print_technology(state: State<AppState>, value: String) -> CmdResult<PrintTechnologyDto> {
    let conn = lock_db(&state)?;
    db::print_technology::set_print_technology(&conn, &value)?;
    katalog_core::read::get_print_technology(&conn).map_err(Into::into)
}
