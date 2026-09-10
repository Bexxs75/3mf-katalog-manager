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
            if let Err(e) = db::delete_unused_tags(&conn) {
                eprintln!("[startup] Aufraeumen verwaister Tags fehlgeschlagen: {e}");
            }
            commands::backfill_content_hashes(&conn);
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
            commands::list_creators,
            commands::list_filament_spools,
            commands::add_filament_spool,
            commands::update_filament_spool,
            commands::delete_filament_spool,
            commands::pick_and_read_image,
            commands::add_tag,
            commands::remove_tag,
            commands::delete_file,
            commands::set_print_status,
            commands::mark_file_viewed,
            commands::upload_custom_image,
            commands::import_files,
            commands::import_folder,
            commands::import_dropped,
            commands::get_model_geometry,
            commands::pick_slicer_executable,
            commands::open_in_slicer,
            cloud::commands::connect_google_drive,
            cloud::commands::disconnect_cloud_account,
            cloud::commands::list_cloud_accounts,
            cloud::commands::open_drive_picker,
            cloud::commands::import_from_cloud,
            cloud::commands::check_cloud_sync_status,
            cloud::commands::upload_file_to_cloud,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
