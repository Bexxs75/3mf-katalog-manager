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
            let trash_dir = app_data_dir.join("trash");
            app.manage(commands::AppState {
                db: Mutex::new(conn),
                trash_dir,
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
            commands::set_favorite,
            commands::mark_file_viewed,
            commands::add_to_queue,
            commands::remove_from_queue,
            commands::reorder_queue,
            commands::upload_custom_image,
            commands::set_render_snapshot,
            commands::set_source_url,
            commands::save_filter,
            commands::list_saved_filters,
            commands::delete_saved_filter,
            commands::import_files,
            commands::import_folder,
            commands::import_dropped,
            commands::get_model_geometry,
            commands::pick_slicer_executable,
            commands::open_in_slicer,
            commands::scan_catalog_issues,
            commands::delete_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
