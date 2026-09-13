mod commands;
mod db;
mod geometry;
mod slicers;
mod stl;
mod tagging;
mod threemf;

use std::sync::Mutex;

use tauri::Manager;

// App-Datenverzeichnis (enthaelt catalog.db, Thumbnails, Papierkorb) nur
// fuer den eigenen Benutzer lesbar/schreibbar machen. Auf Single-User-
// Desktops schon durch die Home-Verzeichnis-Rechte geschuetzt, aber auf
// Mehrbenutzer-Systemen relevant (ISO 27002 A.8.28).
#[cfg(unix)]
fn harden_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = if path.is_dir() { 0o700 } else { 0o600 };
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)) {
        eprintln!("[startup] Dateirechte konnten nicht gehaertet werden fuer {path:?}: {e}");
    }
}

#[cfg(not(unix))]
fn harden_permissions(_path: &std::path::Path) {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Versionsnummer im Fenstertitel, damit sie sich automatisch mit
            // jedem Versions-Bump in Cargo.toml mitzieht statt in
            // tauri.conf.json separat gepflegt werden zu muessen.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title(&format!("3MF Katalog Manager {}", env!("CARGO_PKG_VERSION")));
            }

            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            harden_permissions(&app_data_dir);
            let db_path = app_data_dir.join("catalog.db");
            let conn = db::connect(&db_path)?;
            harden_permissions(&db_path);
            if let Err(e) = db::delete_unused_tags(&conn) {
                eprintln!("[startup] Aufraeumen verwaister Tags fehlgeschlagen: {e}");
            }
            commands::backfill_content_hashes(&conn);
            let trash_dir = app_data_dir.join("trash");
            std::fs::create_dir_all(&trash_dir)?;
            commands::purge_expired_trash_on_startup(&conn);
            app.manage(commands::AppState {
                db: Mutex::new(conn),
                trash_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
            commands::rescan_file_metadata,
            commands::save_filter,
            commands::list_saved_filters,
            commands::delete_saved_filter,
            commands::import_files,
            commands::import_folder,
            commands::import_dropped,
            commands::get_model_geometry,
            commands::pick_slicer_executable,
            commands::open_in_slicer,
            commands::scan_installed_slicers,
            commands::scan_catalog_issues,
            commands::delete_files,
            commands::list_trash,
            commands::restore_file,
            commands::delete_file_permanently,
            commands::empty_trash,
            commands::list_collections,
            commands::create_collection,
            commands::rename_collection,
            commands::delete_collection,
            commands::add_files_to_collection,
            commands::remove_file_from_collection,
            commands::reorder_collection,
            commands::list_collection_files,
            commands::import_folder_as_collection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
