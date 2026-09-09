mod cloud;
mod commands;
mod db;
mod geometry;
mod stl;
mod tagging;
mod threemf;

use std::sync::Mutex;

use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let db_path = app_data_dir.join("catalog.db");
            let conn = db::connect(&db_path)?;
            app.manage(commands::AppState {
                db: Mutex::new(conn),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::list_files,
            commands::list_folders,
            commands::list_tag_counts,
            commands::add_tag,
            commands::remove_tag,
            commands::delete_file,
            commands::import_files,
            commands::import_folder,
            commands::import_dropped,
            commands::get_model_geometry,
            cloud::commands::connect_google_drive,
            cloud::commands::disconnect_cloud_account,
            cloud::commands::list_cloud_accounts,
            cloud::commands::browse_cloud_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
