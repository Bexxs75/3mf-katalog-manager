use super::*;
use katalog_core::ui_settings::{self, Settings, Values};

// File syncs must run off the UI thread, including recovery on the initial read.
async fn run<T: Send + 'static>(
    state: State<'_, AppState>,
    work: impl FnOnce(std::path::PathBuf) -> Result<T, ErrorCode> + Send + 'static,
) -> CmdResult<T> {
    let dir = state
        .db_path
        .parent()
        .ok_or_else(|| CmdError::coded(ErrorCode::UiSettingsUnavailable))?
        .to_owned();
    tauri::async_runtime::spawn_blocking(move || work(dir))
        .await
        .map_err(|_| CmdError::coded(ErrorCode::UiSettingsUnavailable))?
        .map_err(CmdError::coded)
}
#[tauri::command]
pub async fn get_ui_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    run(state, |dir| ui_settings::get(&dir)).await
}
#[tauri::command]
pub async fn set_ui_setting(
    state: State<'_, AppState>,
    key: String,
    value: Option<String>,
) -> CmdResult<()> {
    run(state, move |dir| ui_settings::set(&dir, key, value)).await
}
#[tauri::command]
pub async fn migrate_ui_settings(
    state: State<'_, AppState>,
    values: Values,
) -> CmdResult<Settings> {
    run(state, move |dir| ui_settings::migrate(&dir, values)).await
}
