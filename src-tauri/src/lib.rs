mod commands;
mod db;
mod geometry;
mod slicers;
mod stl;
mod tagging;
mod threemf;
mod update_check;

use std::sync::Mutex;

use tauri::Manager;

// App-Datenverzeichnis (enthaelt catalog.db, Thumbnails, Papierkorb) nur
// fuer den eigenen Benutzer lesbar/schreibbar machen. Auf Single-User-
// Desktops schon durch die Home-Verzeichnis-Rechte geschuetzt, aber auf
// Mehrbenutzer-Systemen relevant (ISO 27002 A.8.28).
#[cfg(unix)]
pub(crate) fn harden_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = if path.is_dir() { 0o700 } else { 0o600 };
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)) {
        eprintln!("[startup] Dateirechte konnten nicht gehaertet werden fuer {path:?}: {e}");
    }
}

#[cfg(not(unix))]
pub(crate) fn harden_permissions(_path: &std::path::Path) {}

// Verzeichnisse, in die `create_folder`/`rename_folder`/`move_folder`/
// `move_file_to_folder` niemals schreiben duerfen - siehe
// `commands::reject_if_sensitive_path` (Security-Review 2026-09-18,
// Finding 1). Einmalig beim Start berechnet, da sich Home-/Config-/
// Datenverzeichnis waehrend der Laufzeit nicht aendern.
fn sensitive_dirs(app: &tauri::AppHandle) -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(d) = app.path().config_dir() {
        dirs.push(d);
    }
    if let Ok(d) = app.path().data_dir() {
        dirs.push(d);
    }
    if let Ok(home) = app.path().home_dir() {
        dirs.push(home.join(".ssh"));
        dirs.push(home.join(".gnupg"));
        dirs.push(home.join(".password-store"));
        #[cfg(target_os = "macos")]
        dirs.push(home.join("Library"));
    }
    #[cfg(target_os = "linux")]
    for root in ["/etc", "/usr", "/bin", "/sbin", "/boot", "/root", "/var", "/sys", "/proc"] {
        dirs.push(std::path::PathBuf::from(root));
    }
    #[cfg(target_os = "windows")]
    for root in ["C:\\Windows", "C:\\Program Files", "C:\\Program Files (x86)"] {
        dirs.push(std::path::PathBuf::from(root));
    }
    dirs
}

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
            let sensitive_dirs = sensitive_dirs(app.handle());
            app.manage(commands::AppState {
                db: Mutex::new(conn),
                trash_dir,
                db_path,
                sensitive_dirs,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_files,
            commands::list_folders,
            commands::create_folder,
            commands::move_file_to_folder,
            commands::rename_folder,
            commands::move_folder,
            commands::pick_folder_path,
            commands::register_catalog_base_dir,
            commands::open_in_file_manager,
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
            commands::export_catalog,
            commands::import_catalog,
            commands::list_print_log_entries,
            commands::add_print_log_entry,
            commands::delete_print_log_entry,
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
            commands::get_app_version,
            commands::check_for_update,
            commands::open_release_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
