mod archive;
mod commands;
mod db;
pub mod diagnostics;
mod filament_check;
mod geometry;
mod obj;
// Printer connection: connection and sync logic.
mod printer_link;
mod slicers;
#[cfg(feature = "step-preview")]
pub mod step;
mod stl;
mod tagging;
mod threemf;
mod update_check;

use std::sync::Mutex;

use tauri::Manager;

// Data directory for the own user only (relevant on multi-user systems).
#[cfg(unix)]
pub(crate) fn harden_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = if path.is_dir() { 0o700 } else { 0o600 };
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)) {
        log::error!(target: "startup", "could not harden file permissions for {path:?}: {e}");
    }
}

#[cfg(not(unix))]
pub(crate) fn harden_permissions(_path: &std::path::Path) {}

/// Indents continuation lines of a multi-line log message (e.g. a stack
/// trace) so they read as part of the same entry, not as separate log lines.
fn indent_continuation_lines(message: &str) -> String {
    message.replace('\n', "\n    ")
}

// Directories that folder and file operations must never write to (see
// `commands::reject_if_sensitive_path`); computed once at startup.
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
        .plugin(
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name: None }))
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stderr))
                // Debug passes the plugin; the effective level is set at runtime with
                // log::set_max_level (verbose mode), see diagnostics::verbose.
                .level(log::LevelFilter::Debug)
                .level_for("tauri", log::LevelFilter::Info)
                .level_for("tao", log::LevelFilter::Warn)
                .level_for("wry", log::LevelFilter::Warn)
                .level_for("reqwest", log::LevelFilter::Warn)
                .level_for("hyper", log::LevelFilter::Warn)
                .level_for("hyper_util", log::LevelFilter::Warn)
                .level_for("rustls", log::LevelFilter::Warn)
                .level_for("h2", log::LevelFilter::Warn)
                .level_for("mio", log::LevelFilter::Warn)
                .level_for("tokio", log::LevelFilter::Warn)
                .level_for("tracing", log::LevelFilter::Warn)
                .level_for("rusqlite", log::LevelFilter::Warn)
                .max_file_size(2_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(4))
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .format(|out, message, record| {
                    let message = indent_continuation_lines(&message.to_string());
                    out.finish(format_args!(
                        "{} {:<5} [{}] {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                        record.level(),
                        record.target(),
                        message
                    ))
                })
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Version in the window title, taken straight from Cargo.toml.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title(&format!("3MF Katalog Manager {}", env!("CARGO_PKG_VERSION")));
            }

            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            harden_permissions(&app_data_dir);
            let db_path = app_data_dir.join("catalog.db");
            let conn = db::connect(&db_path)?;
            commands::apply_verbose_state(&conn);
            diagnostics::log_startup(&conn);
            harden_permissions(&db_path);
            if let Err(e) = db::delete_unused_tags(&conn) {
                log::error!(target: "startup", "cleaning up orphaned tags failed: {e}");
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
            // Re-checks the verbose-logging switch hourly so it turns itself off
            // within an hour of expiring, even on a long-running session.
            let verbose_handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
                if let Some(state) = verbose_handle.try_state::<commands::AppState>() {
                    if let Ok(conn) = state.db.lock() {
                        commands::apply_verbose_state(&conn);
                    }
                }
            });
            app.manage(commands::PendingArchives::default());
            app.manage(commands::ApprovedTargets::default());
            let waker = printer_link::sync::spawn_background(app.handle().clone());
            app.manage(waker);
            app.manage(commands::DroppedImages::default());
            app.manage(commands::DiagnosticsState::default());
            Ok(())
        })
        // Observe drops in the backend itself: `import_dropped` only approves archives
        // seen here (otherwise a compromised frontend could report arbitrary paths as a
        // "drop"). Ordering assumption: this handler runs synchronously in the same
        // event loop iteration in which Tauri forwards the drop event to the frontend -
        // the following IPC call `import_dropped` is therefore always processed after it.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                if let Some(pending) = window.try_state::<commands::PendingArchives>() {
                    pending.observe_drop(paths);
                }
                if let Some(images) = window.try_state::<commands::DroppedImages>() {
                    images.observe_drop(paths);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_file_summaries,
            commands::list_all_file_tags,
            commands::list_files_by_ids,
            commands::list_folders,
            commands::create_folder,
            commands::move_file_to_folder,
            commands::rename_file,
            commands::rename_folder,
            commands::move_folder,
            commands::pick_folder_path,
            commands::register_catalog_base_dir,
            commands::register_existing_catalog_base_dir,
            commands::open_in_file_manager,
            commands::list_tag_counts,
            commands::list_filament_spools,
            commands::check_filament,
            commands::add_filament_spool,
            commands::update_filament_spool,
            commands::delete_filament_spool,
            commands::restock_filament_spool,
            commands::consume_resin,
            commands::list_printers,
            commands::add_printer,
            commands::rename_printer,
            commands::delete_printer,
            commands::add_unit,
            commands::update_unit,
            commands::delete_unit,
            commands::reorder_units,
            commands::load_spool,
            commands::unload_spool,
            commands::get_printer_link_enabled,
            commands::set_printer_link_enabled,
            commands::list_printer_connections,
            commands::test_printer_connection,
            commands::remove_printer_connection,
            commands::sync_printers_now,
            commands::list_open_printer_jobs,
            commands::preview_printer_job,
            commands::get_printer_job_thumbnail,
            commands::ignore_printer_job,
            commands::confirm_printer_jobs,
            commands::pick_and_read_image,
            commands::read_dropped_image,
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
            commands::import_files,
            commands::import_folder,
            commands::import_dropped,
            commands::inspect_archives,
            commands::archive_target_conflicts,
            commands::extract_archives,
            commands::get_model_geometry,
            commands::pick_and_register_slicer,
            commands::list_registered_slicers,
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
            commands::get_app_version,
            commands::check_for_update,
            commands::open_release_url,
            commands::open_discord_invite,
            commands::get_verbose_logging,
            commands::set_verbose_logging,
            commands::get_bug_report_info,
            commands::preview_log_export,
            commands::save_log_export,
            commands::open_log_folder,
            commands::open_bug_report_form,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuation_lines_are_indented() {
        assert_eq!(indent_continuation_lines("single line"), "single line");
        assert_eq!(
            indent_continuation_lines("panic message\nat src/lib.rs:1\nat main"),
            "panic message\n    at src/lib.rs:1\n    at main"
        );
    }
}
