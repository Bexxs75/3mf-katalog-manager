use super::*;

use katalog_core::catalog_restore::{self, StdRestoreFs, TrashPolicy};

pub(super) use imported_paths::validate_path as validate_imported_local_path;

/// Exports the complete catalog state (DB + frontend settings) as a ZIP file.
/// Uses SQLite's online backup API instead of copying the raw file: the live
/// connection may be in WAL mode, and fs::copy could catch an inconsistent state.
#[tauri::command]
pub async fn export_catalog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<bool> {

    let picked = app
        .dialog()
        .file()
        .add_filter("ZIP-Archiv", &["zip"])
        .set_file_name(format!(
            "3mf-katalog-backup_{}.zip",
            chrono::Utc::now().format("%Y-%m-%d")
        ))
        .blocking_save_file();

    let Some(picked) = picked else {
        return Ok(false);
    };
    let dest_path = picked.into_path().map_err(|e| e.to_string())?;
    let settings_json = katalog_core::ui_settings::backup(
        state
            .db_path
            .parent()
            .ok_or_else(|| CmdError::coded(ErrorCode::UiSettingsUnavailable))?,
    )
    .map_err(CmdError::coded)?;
    write_catalog_backup(&state.db, &dest_path, &settings_json)?;
    log::info!(target: "backup", "Sicherung erstellt");
    Ok(true)
}

/// Writes the backup ZIP (consistent snapshot of the catalog DB plus the
/// frontend settings) to `dest`. The DB is locked only for the snapshot.
fn write_catalog_backup(db: &std::sync::Mutex<Connection>, dest: &Path, settings_json: &str) -> CmdResult<()> {
    // Exclusive and private from the start (symlink race, readable copy of the
    // catalog), under a fresh name per export, see `safe_file::create_private_temp`.
    let (backup_db_path, backup_db_file) =
        crate::safe_file::create_private_temp(&std::env::temp_dir(), "3mf-katalog-export", ".db")
            .map_err(|e| e.to_string())?;
    drop(backup_db_file);

    // A closure, so both temp files are cleaned up on failure too.
    let result: CmdResult<()> = (|| {
        {
            crate::safe_file::harden_permissions(&backup_db_path);

            let conn = db.lock().map_err(|_| CmdError::from("database lock poisoned"))?;
            let mut dst = Connection::open(&backup_db_path).map_err(|e| e.to_string())?;
            {
                let backup =
                    rusqlite::backup::Backup::new(&conn, &mut dst).map_err(|e| e.to_string())?;
                // One step for the whole database: `run_to_completion` sleeps after
                // every step that isn't the last, while the catalog lock (and with it
                // every synchronous command of the UI) waits - see updater/backup.rs.
                backup
                    .run_to_completion(i32::MAX, std::time::Duration::from_millis(250), None)
                    .map_err(|e| e.to_string())?;
            }
            // The ZIP is meant to be kept and passed on: no API keys in the copy.
            db::scrub_secrets(&dst).map_err(|e| e.to_string())?;
        }

        let db_bytes = std::fs::read(&backup_db_path).map_err(|e| e.to_string())?;
        write_zip_atomically(dest, &[("catalog.db", &db_bytes), ("settings.json", settings_json.as_bytes())])
            .map_err(|e| e.to_string())?;
        Ok(())
    })();

    let _ = std::fs::remove_file(&backup_db_path);
    result
}

/// Writes a ZIP with `entries` to `dest` via a private temp file next to it and
/// renames it into place only when complete: never a partial archive at the
/// destination. The temp name is unpredictable and created exclusively, so a
/// symlink planted under a guessable name can't redirect the write.
fn write_zip_atomically(dest: &Path, entries: &[(&str, &[u8])]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let base = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (tmp_path, file) = crate::safe_file::create_private_temp(parent, &format!(".{base}"), ".tmp")?;

    let written = (|| {
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, options).map_err(std::io::Error::other)?;
            zip.write_all(bytes)?;
        }
        zip.finish().map_err(std::io::Error::other)?.sync_all()?;
        std::fs::rename(&tmp_path, dest)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    written
}
/// Writes `bytes` exclusively (`create_new`) to `path` and sets 0600.
/// `fs::write` would follow a pre-planted symlink in a shared `/tmp` and create
/// the file with umask permissions (often world-readable) (CWE-377, TOCTOU).
fn write_temp_file_exclusive(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = crate::safe_file::create_new_private(path)?;
    file.write_all(bytes)?;
    drop(file);
    crate::safe_file::harden_permissions(path);
    Ok(())
}
fn read_catalog_archive<R: std::io::Read + std::io::Seek>(reader: R) -> CmdResult<(Vec<u8>, String)> {
    catalog_restore::read_catalog_archive(reader).map_err(Into::into)
}

/// Keep the adapter's temporary databases in a private, host-owned directory.
struct RestoreDirectory(PathBuf);

impl RestoreDirectory {
    fn new() -> CmdResult<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "3mf-katalog-restore-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }

    fn adapter(&self) -> StdRestoreFs {
        StdRestoreFs::new(self.0.clone())
    }
}

impl Drop for RestoreDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn validate_catalog_db_bytes(
    bytes: &[u8],
    sensitive_dirs: &[PathBuf],
    trash_dir: &Path,
    known_folders: &[String],
) -> CmdResult<()> {
    let directory = RestoreDirectory::new()?;
    let roots: Vec<PathBuf> = known_folders.iter().map(PathBuf::from).collect();
    catalog_restore::validate_catalog_db_bytes(
        bytes,
        sensitive_dirs,
        &TrashPolicy {
            desktop_trash: Some(trash_dir),
            catalog_roots: &roots,
        },
        known_folders,
        &directory.adapter(),
    )
    .map_err(Into::into)
}
/// Boundary used by `import_catalog`: a rejected backup file (bad schema,
/// corrupted DB, a path escaping the trash/sensitive-dir checks, ...) is always
/// the user's file, not a fault here - hence an expected code, not a plain `?`
/// (which would mark it a fault and log it at ERROR via `From<String>`).
/// Extracted so this conversion is exercised by a test instead of only living
/// inline in the `#[tauri::command]`.
fn validate_backup_for_import(bytes: &[u8], sensitive_dirs: &[PathBuf], trash_dir: &Path, known_folders: &[String]) -> CmdResult<()> {
    validate_catalog_db_bytes(bytes, sensitive_dirs, trash_dir, known_folders)
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCatalogResultDto {
    pub imported: bool,
    pub settings_json: Option<String>,
    pub settings_error: Option<CmdError>,
}
/// Imports an export created by `export_catalog`. Replaces the running
/// `catalog.db` only after successful validation, see `replace_catalog_db`.
/// A restart is still recommended, because the frontend isn't built for
/// switching catalogs at runtime.
#[tauri::command]
pub async fn import_catalog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ImportCatalogResultDto> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let picked = app.dialog().file().add_filter("ZIP-Archiv", &["zip"]).blocking_pick_file();
    let Some(picked) = picked else {
        return Ok(ImportCatalogResultDto { imported: false, settings_json: None, settings_error: None });
    };
    let archive_path = picked.into_path().map_err(|e| e.to_string())?;

    let file = std::fs::File::open(&archive_path).map_err(|e| e.to_string())?;
    let (db_bytes, settings_json) = read_catalog_archive(file)?;

    let known_folders = {
        let conn = state.db.lock().map_err(|_| CmdError::from("database lock poisoned"))?;
        imported_paths::folder_paths(&conn)?
    };
    validate_backup_for_import(&db_bytes, &state.sensitive_dirs, &state.trash_dir, &known_folders)?;

    let tmp_db_path =
        std::env::temp_dir().join(format!("3mf-katalog-import-{}.db", std::process::id()));
    write_temp_file_exclusive(&tmp_db_path, &db_bytes).map_err(|e| e.to_string())?;

    let settings_dir = state.db_path.parent().map(PathBuf::from);
    let replace_result = replace_catalog_db(&state, &tmp_db_path);
    let _ = std::fs::remove_file(&tmp_db_path);
    replace_result?;

    let restore_json = settings_json.clone();
    // The catalog has already been replaced; settings failure is a warning, never import failure.
    let settings_error = tauri::async_runtime::spawn_blocking(move || {
        settings_dir.ok_or(ErrorCode::UiSettingsUnavailable)
            .and_then(|dir| katalog_core::ui_settings::restore(&dir, &restore_json))
    }).await.unwrap_or(Err(ErrorCode::UiSettingsUnavailable)).err().map(CmdError::coded);
    log::info!(target: "backup", "Sicherung wiederhergestellt");
    Ok(ImportCatalogResultDto { imported: true, settings_json: Some(settings_json), settings_error })
}
/// Replaces the running `catalog.db` with `new_db_path`.
///
/// `registered_slicers` is machine-local: a backup must neither inject foreign
/// executable paths nor delete local slicers. So the INCOMING DB is fully
/// sanitized before it becomes active; if that fails, the old DB stays untouched.
///
/// Before renaming, the connection is replaced by an in-memory placeholder; only
/// that closes the file handle (Windows: sharing violation). The old DB is renamed
/// to `.bak-<timestamp>`. If copying fails it is renamed back; if that fails too,
/// the error message names the path of the backup.
fn replace_catalog_db(state: &AppState, new_db_path: &Path) -> CmdResult<()> {
    replace_catalog_db_with_copy_fn(state, new_db_path, copy_file_default)
}
/// Non-generic wrapper so `std::fs::copy` fits as a function pointer for
/// `replace_catalog_db_with_copy_fn`.
fn copy_file_default(from: &Path, to: &Path) -> std::io::Result<u64> {
    std::fs::copy(from, to)
}
/// Core of [`replace_catalog_db`] with an exchangeable copy function, so tests can
/// force a failure exactly in the copy step (file permissions can't do that
/// without already failing the rename).
fn replace_catalog_db_with_copy_fn(
    state: &AppState,
    new_db_path: &Path,
    copy_fn: fn(&Path, &Path) -> std::io::Result<u64>,
) -> CmdResult<()> {
    // Save the local slicers from the running DB before anything changes.
    let local_slicers = {
        let guard = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
        db::list_registered_slicers(&guard).map_err(|e| e.to_string())?
    };
    // The Thingiverse key belongs to this computer: a backup never holds it, so the
    // working database's key (and switch) is put back into the incoming one below.
    let local_thingiverse = {
        let guard = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
        db::thingiverse::local_settings(&guard).map_err(|e| e.to_string())?
    };

    // Prepare through the platform adapter before touching the active connection.
    {
        let known_folders = {
            let guard = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
            imported_paths::folder_paths(&guard)?
        };
        let roots: Vec<PathBuf> = known_folders.iter().map(PathBuf::from).collect();
        let directory = RestoreDirectory::new()?;
        let bytes = std::fs::read(new_db_path).map_err(|e| e.to_string())?;
        let mut prepared = catalog_restore::prepare_catalog(
            &bytes,
            &state.sensitive_dirs,
            &TrashPolicy {
                desktop_trash: Some(&state.trash_dir),
                catalog_roots: &roots,
            },
            &known_folders,
            &directory.adapter(),
            &local_slicers,
            &local_thingiverse,
        )?;
        // Materialize the prepared connection for the existing desktop copy/rollback path.
        // SQLite's backup API includes committed data even when the copy uses WAL.
        let mut incoming = Connection::open(new_db_path).map_err(|e| e.to_string())?;
        let backup = rusqlite::backup::Backup::new(prepared.connection(), &mut incoming)
            .map_err(|e| e.to_string())?;
        backup.run_to_completion(100, std::time::Duration::from_millis(10), None)
            .map_err(|e| e.to_string())?;
    }

    // Only now swap the active DB; new_db_path is already sanitized.
    {
        let mut guard = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
        let placeholder = Connection::open_in_memory().map_err(|e| e.to_string())?;
        *guard = placeholder; // old connection drops here -> OS handle on catalog.db is closed
    }

    let backup_path = state.db_path.with_file_name(format!(
        "catalog.db.bak-{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    ));

    if let Err(e) = std::fs::rename(&state.db_path, &backup_path) {
        // Rename failed, file unchanged: reconnect, otherwise the app stays on the
        // placeholder until restart.
        if let Ok(mut guard) = state.db.lock() {
            if let Ok(conn) = db::connect(&state.db_path) {
                *guard = conn;
            }
        }
        return Err(e.to_string().into());
    }

    if let Err(e) = copy_fn(new_db_path, &state.db_path) {
        let restore_result = std::fs::rename(&backup_path, &state.db_path);
        if restore_result.is_ok() {
            if let Ok(mut guard) = state.db.lock() {
                if let Ok(conn) = db::connect(&state.db_path) {
                    *guard = conn;
                }
            }
        }
        return match restore_result {
            Ok(()) => Err(format!(
                "Kopieren der neuen Datenbank fehlgeschlagen, alter Katalog wiederhergestellt: {e}"
            )
            .into()),
            Err(restore_err) => Err(format!(
                "Kopieren der neuen Datenbank fehlgeschlagen UND Wiederherstellung der alten \
                 Datenbank fehlgeschlagen ({restore_err}). Die vorherige Datenbank liegt noch \
                 unter {}. Bitte manuell nach {} zurückbenennen und anschließend die App neu \
                 starten. Ursprünglicher Fehler: {e}",
                backup_path.display(),
                state.db_path.display()
            )
            .into()),
        };
    }

    // db::connect instead of Connection::open: sets foreign_keys and migrates,
    // otherwise older exports would miss columns until restart.
    let mut guard = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
    *guard = db::connect(&state.db_path).map_err(|e| e.to_string())?;
    drop(guard);

    // The new database is active, so the fallback copy has done its job as a way back: from now on
    // it is only an old catalog that must not keep the keys of this computer (printer API keys,
    // Thingiverse token) that the user may delete later. A failure leaves the copy as it is and is
    // logged, the restore itself succeeded.
    if let Err(e) = scrub_fallback_copy(&backup_path) {
        log::warn!(target: "backup", "Rückfallkopie {} konnte nicht von Schlüsseln bereinigt werden: {e}", backup_path.display());
    }

    Ok(())
}
/// Removes API keys and tokens from the fallback copy of the replaced database.
fn scrub_fallback_copy(path: &Path) -> Result<(), String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    db::scrub_secrets(&conn).map_err(|e| e.to_string())
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogIssuesDto {
    pub orphaned: Vec<ModelFileDto>,
    pub duplicate_groups: Vec<Vec<ModelFileDto>>,
}
#[cfg(test)]
use katalog_core::catalog_issues::group_duplicates;
use katalog_core::catalog_issues::{self, IssueLimits, MissingProbe, StdMissingProbe};

pub(super) fn is_missing(path: &Path) -> std::io::Result<bool> {
    StdMissingProbe.is_missing(path)
}

#[tauri::command]
pub fn scan_catalog_issues(state: State<AppState>) -> CmdResult<CatalogIssuesDto> {
    let conn = lock_db(&state)?;
    let files = db::list_files(&conn).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;

    // Only NotFound means "missing" (StdMissingProbe): PermissionDenied or IO errors (e.g. an
    // unmounted network drive) must not mark files for deletion.
    let issues = catalog_issues::scan_records(&files, &StdMissingProbe, IssueLimits::UNLIMITED);
    let by_id: HashMap<i64, &FileRecord> = files.iter().map(|f| (f.id, f)).collect();
    let dto = |id: &i64| to_dto(by_id[id].clone(), &spools);
    let orphaned: Vec<ModelFileDto> = issues.orphaned.iter().map(dto).collect();
    let duplicate_groups: Vec<Vec<ModelFileDto>> =
        issues.duplicate_groups.iter().map(|group| group.iter().map(dto).collect()).collect();

    Ok(CatalogIssuesDto { orphaned, duplicate_groups })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn imported_names_preserve_native_characters_and_trailing_spaces() {
        let dir = unique_test_dir("native_import_names");
        for name in ["Benchy: v2.stl", "what?.stl", "a*b|c\"<d>", "model.", "model "] {
            let bytes = catalog_db_bytes_with_file_row(name, dir.join(name).to_str().unwrap(), None);
            assert!(validate_catalog_db_bytes(&bytes, &[], &dir, &[]).is_ok(), "{name}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imported_empty_trash_paths_are_ignored() {
        let dir = unique_test_dir("empty_import_trash");
        for trash in ["", "   ", "\t\n", "\u{2003}"] {
            let bytes = catalog_db_bytes_with_file_row("x.stl", dir.join("x.stl").to_str().unwrap(), Some(trash));
            assert!(validate_catalog_db_bytes(&bytes, &[], &dir, &[]).is_ok(), "{trash:?}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imported_paths_reject_network_and_relative_strings_without_io() {
        for path in [r"\\192.0.2.10\probe\x", r"\\?\UNC\host\share\x", r"\\host@SSL\share\x", r"\\.\device", "//host/share/x", "relative/x", r"C:x"] {
            assert!(validate_imported_local_path(path, &[]).is_err(), "{path}");
        }
        assert!(validate_imported_local_path(std::env::temp_dir().to_str().unwrap(), &[]).is_ok());
        let bytes = catalog_db_bytes_with_file_row("x.3mf", r"\\192.0.2.10\probe\x", None);
        assert!(validate_catalog_db_bytes(&bytes, &[], &std::env::temp_dir(), &[]).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn imported_windows_paths_require_an_absolute_local_drive() {
        assert!(validate_imported_local_path(r"C:\models\x.3mf", &[]).is_ok());
        for path in [r"\models\x", r"C:x", r"\\?\GLOBALROOT\Device\x"] {
            assert!(validate_imported_local_path(path, &[]).is_err());
        }
    }

    #[test]
    fn imported_folder_names_use_portable_component_rules() {
        let dir = unique_test_dir("import_folder_names");
        let path = dir.join("catalog.db");
        {
            let conn = db::connect(&path).unwrap();
            db::insert_folder_with_parent(&conn, "CON", None, dir.to_str().unwrap()).unwrap();
        }
        assert_eq!(validate_catalog_db_bytes(&std::fs::read(&path).unwrap(), &[], &dir, &[]).is_err(), cfg!(windows));
        let _ = std::fs::remove_dir_all(dir);
    }

    use rusqlite::params;

    fn sample_new_file_for_rescan_test(path: &std::path::Path) -> NewFile {
        NewFile {
            name: "missing.3mf".to_string(),
            path: path.to_string_lossy().to_string(),
            file_type: FileType::ThreeMf,
            folder_id: None,
            origin: "local".to_string(),
            cloud_id: None,
            sync_status: "local-only".to_string(),
            file_size_bytes: 0,
            dimensions_mm: None,
            volume_cm3: None,
            object_count: None,
            thumbnail_png: None,
            imported_at: "2026-09-13T00:00:00Z".to_string(),
            file_modified_at: None,
            materials: Vec::new(),
            metadata: BTreeMap::new(),
            tags: Vec::new(),
            print_status: "not_printed".to_string(),
            last_viewed_at: None,
            creator: None,
            content_hash: None,
            render_snapshot_png: None,
            custom_image_png: None,
            source_url: None,
            license: None,
            source: Default::default(),
            queue_position: None,
            favorite: false,
            plate_count: None,
            plate_dimensions_json: None,
            slice_info_json: None,
            print_info: None,
        }
    }
    #[test]
    fn catalog_license_validation_accepts_old_exports_and_validates_new_values() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            crate::db::test_insert_minimal_file(conn, "/model.3mf", None).unwrap();
            conn.execute_batch("ALTER TABLE files DROP COLUMN license").unwrap();
            // The license step is the one before the source-column step.
            conn.pragma_update(None, "user_version", crate::db::SOURCE_COLUMNS_MIGRATION_VERSION - 2).unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_ok());
        for (value, valid) in [(None, true), (Some("cc-by".to_string()), true),
            (Some(" Free license ".to_string()), true), (Some("é".repeat(200)), true),
            (Some("a".repeat(201)), false), (Some("bad\0text".to_string()), false)] {
            let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
                let id = crate::db::test_insert_minimal_file(conn, "/model.3mf", None).unwrap();
                conn.execute("UPDATE files SET license = ?1 WHERE id = ?2", rusqlite::params![value, id]).unwrap();
            });
            let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
            assert_eq!(result.is_ok(), valid, "{result:?}");
        }
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let id = crate::db::test_insert_minimal_file(conn, "/model.3mf", None).unwrap();
            conn.execute("UPDATE files SET license = X'1234' WHERE id = ?1", [id]).unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
    }

    fn state_with_catalog(name: &str) -> (AppState, PathBuf) {
        let dir = unique_test_dir(name);
        let db_path = dir.join("catalog.db");
        let state = AppState {
            import_jobs: ImportJobs::default(),
            db: Mutex::new(db::connect(&db_path).unwrap()),
            trash_dir: dir.join("trash"),
            db_path,
            sensitive_dirs: Vec::new(),
        };
        (state, dir)
    }

    fn stored_source(state: &AppState) -> Vec<(Option<String>, Option<String>, Option<String>)> {
        let conn = state.db.lock().unwrap();
        db::list_files(&conn).unwrap().into_iter()
            .map(|f| (f.source_platform, f.source_model_id, f.source_profile_id)).collect()
    }

    #[test]
    fn platform_and_ids_survive_export_and_restore() {
        let (source, dir) = state_with_catalog("backup_source_roundtrip_a");
        {
            let mut conn = source.db.lock().unwrap();
            let mut file = sample_new_file_for_rescan_test(&dir.join("model.3mf"));
            file.source = db::models::SourceInfo {
                platform: Some("makerworld".into()),
                model_id: Some("USfa963547e39de4".into()),
                profile_id: Some("1067399466".into()),
            };
            db::insert_file(&mut conn, &file).unwrap();
            let mut plain = sample_new_file_for_rescan_test(&dir.join("plain.3mf"));
            plain.name = "plain.3mf".into();
            db::insert_file(&mut conn, &plain).unwrap();
        }
        let zip_path = dir.join("backup.zip");
        write_catalog_backup(&source.db, &zip_path, "{}").unwrap();

        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        validate_catalog_db_bytes(&db_bytes, &[], &dir, &[]).expect("a current export must validate");
        let incoming = dir.join("incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_source_roundtrip_b");
        assert!(stored_source(&target).is_empty());
        replace_catalog_db(&target, &incoming).unwrap();

        let mut restored = stored_source(&target);
        restored.sort();
        assert_eq!(restored, vec![
            (None, None, None),
            (Some("makerworld".into()), Some("USfa963547e39de4".into()), Some("1067399466".into())),
        ]);
    }

    #[test]
    fn a_backup_from_before_the_source_columns_can_be_restored() {
        let (old, dir) = state_with_catalog("backup_source_old_a");
        {
            let mut conn = old.db.lock().unwrap();
            let mut file = sample_new_file_for_rescan_test(&dir.join("old.3mf"));
            file.creator = Some("Jane".into());
            db::insert_file(&mut conn, &file).unwrap();
            conn.execute_batch("ALTER TABLE files DROP COLUMN source_platform; ALTER TABLE files DROP COLUMN source_model_id; ALTER TABLE files DROP COLUMN source_profile_id").unwrap();
            conn.pragma_update(None, "user_version", crate::db::SOURCE_COLUMNS_MIGRATION_VERSION - 1).unwrap();
        }
        let zip_path = dir.join("old-backup.zip");
        write_catalog_backup(&old.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let probe = dir.join("probe.db");
        std::fs::write(&probe, &db_bytes).unwrap();
        assert!(Connection::open(&probe).unwrap().prepare("SELECT source_platform FROM files").is_err(), "precondition: the backup has no such column");

        validate_catalog_db_bytes(&db_bytes, &[], &dir, &[]).expect("an older backup must validate");
        let incoming = dir.join("old-incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_source_old_b");
        replace_catalog_db(&target, &incoming).unwrap();

        assert_eq!(stored_source(&target), vec![(None, None, None)]);
        let conn = target.db.lock().unwrap();
        assert_eq!(db::list_files(&conn).unwrap()[0].creator.as_deref(), Some("Jane"));
    }

    fn spool_colors(state: &AppState) -> Vec<(Option<String>, Vec<String>)> {
        let conn = state.db.lock().unwrap();
        db::list_filament_spools(&conn).unwrap().into_iter().map(|s| (s.color_hex, s.color_hexes)).collect()
    }

    fn colour_spool(colors: &[&str]) -> db::models::NewFilamentSpool {
        db::models::NewFilamentSpool {
            nozzle_temp_min: None,
            nozzle_temp_max: None,
            bed_temp_min: None,
            bed_temp_max: None,
            material: "PLA".into(),
            manufacturer: None,
            color: None,
            location: None,
            diameter_mm: 1.75,
            original_weight_g: 1000.0,
            remaining_weight_g: 500.0,
            price: None,
            image_png: None,
            color_hex: None,
            color_hexes: colors.iter().map(|c| c.to_string()).collect(),
            kind: "filament".into(),
        }
    }

    #[test]
    fn spool_color_lists_survive_export_and_restore() {
        let (source, dir) = state_with_catalog("backup_colors_roundtrip_a");
        {
            let conn = source.db.lock().unwrap();
            db::insert_filament_spool(&conn, &colour_spool(&["#ff0000", "#00ff00", "#ff0000"])).unwrap();
            db::insert_filament_spool(&conn, &colour_spool(&[])).unwrap();
            conn.execute("UPDATE filament_spools SET nozzle_temp_min = 200, nozzle_temp_max = 240, bed_temp_min = 50, bed_temp_max = 65", []).unwrap();
        }
        let zip_path = dir.join("backup.zip");
        write_catalog_backup(&source.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        validate_catalog_db_bytes(&db_bytes, &[], &dir, &[]).expect("a current export must validate");
        let incoming = dir.join("incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_colors_roundtrip_b");
        assert!(spool_colors(&target).is_empty());
        replace_catalog_db(&target, &incoming).unwrap();

        {
            let conn = target.db.lock().unwrap();
            let spools = db::list_filament_spools(&conn).unwrap();
            assert!(spools.iter().all(|s| (
                s.nozzle_temp_min,
                s.nozzle_temp_max,
                s.bed_temp_min,
                s.bed_temp_max
            ) == (Some(200), Some(240), Some(50), Some(65))));
        }
        let mut restored = spool_colors(&target);
        restored.sort();
        assert_eq!(restored, vec![
            (None, vec![]),
            (Some("#ff0000".into()), vec!["#ff0000".into(), "#00ff00".into(), "#ff0000".into()]),
        ]);
    }

    #[test]
    fn a_backup_from_before_the_spool_color_list_can_be_restored() {
        let (old, dir) = state_with_catalog("backup_colors_old_a");
        {
            let conn = old.db.lock().unwrap();
            let mut spool = colour_spool(&[]);
            spool.color_hex = Some("#b03020".into());
            db::insert_filament_spool(&conn, &spool).unwrap();
            conn.execute_batch("ALTER TABLE filament_spools DROP COLUMN color_hexes")
                .unwrap();
            conn.execute_batch("ALTER TABLE filament_spools DROP COLUMN nozzle_temp_min")
                .unwrap();
            conn.execute_batch("ALTER TABLE filament_spools DROP COLUMN nozzle_temp_max")
                .unwrap();
            conn.execute_batch("ALTER TABLE filament_spools DROP COLUMN bed_temp_min")
                .unwrap();
            conn.execute_batch("ALTER TABLE filament_spools DROP COLUMN bed_temp_max")
                .unwrap();

            conn.pragma_update(None, "user_version", crate::db::SPOOL_COLOR_LIST_MIGRATION_VERSION - 1).unwrap();
        }
        let zip_path = dir.join("old-backup.zip");
        write_catalog_backup(&old.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let probe = dir.join("probe.db");
        std::fs::write(&probe, &db_bytes).unwrap();
        assert!(Connection::open(&probe).unwrap().prepare("SELECT color_hexes FROM filament_spools").is_err(), "precondition: the backup has no such column");
        validate_catalog_db_bytes(&db_bytes, &[], &dir, &[]).expect("an older backup must validate");
        let incoming = dir.join("old-incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_colors_old_b");
        replace_catalog_db(&target, &incoming).unwrap();

        assert_eq!(spool_colors(&target), vec![(Some("#b03020".into()), vec![])]);
        let conn = target.db.lock().unwrap();
        let spools = db::list_filament_spools(&conn).unwrap();
        assert_eq!(
            (
                spools[0].nozzle_temp_min,
                spools[0].nozzle_temp_max,
                spools[0].bed_temp_min,
                spools[0].bed_temp_max
            ),
            (None, None, None, None)
        );
    }

    #[test]
    fn imported_spool_print_temperatures_are_checked() {
        for (min, max) in [
            (Some(9999), None),
            (Some(-1), None),
            (None, Some(501)),
            (Some(240), Some(200)),
        ] {
            for (min_column, max_column) in [
                ("nozzle_temp_min", "nozzle_temp_max"),
                ("bed_temp_min", "bed_temp_max"),
            ] {
                let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
                    db::insert_filament_spool(conn, &colour_spool(&[])).unwrap();
                    conn.execute(
                        &format!("UPDATE filament_spools SET {min_column} = ?1, {max_column} = ?2"),
                        rusqlite::params![min, max],
                    )
                    .unwrap();
                });
                let error = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).unwrap_err();
                assert!(error.is(ErrorCode::BackupInvalidDatabase, &[]), "{error:?}");
            }
        }
    }

    #[test]
    fn valid_and_null_spool_print_temperatures_restore() {
        for values in [
            (Some(200), Some(240), Some(50), Some(65)),
            (None, None, None, None),
            (Some(0), None, None, Some(500)),
        ] {
            let (source, dir) = state_with_catalog("backup_print_temperatures_a");
            {
                let conn = source.db.lock().unwrap();
                db::insert_filament_spool(&conn, &colour_spool(&[])).unwrap();
                conn.execute(
                    "UPDATE filament_spools SET nozzle_temp_min = ?1, nozzle_temp_max = ?2, bed_temp_min = ?3, bed_temp_max = ?4",
                    rusqlite::params![values.0, values.1, values.2, values.3],
                ).unwrap();
            }
            let zip_path = dir.join("backup.zip");
            write_catalog_backup(&source.db, &zip_path, "{}").unwrap();
            let (bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
            validate_catalog_db_bytes(&bytes, &[], &dir, &[]).unwrap();
            let incoming = dir.join("incoming.db");
            std::fs::write(&incoming, &bytes).unwrap();
            let (target, _target_dir) = state_with_catalog("backup_print_temperatures_b");
            replace_catalog_db(&target, &incoming).unwrap();
            let conn = target.db.lock().unwrap();
            let spools = db::list_filament_spools(&conn).unwrap();
            assert_eq!(spools.len(), 1);
            let spool = &spools[0];
            assert_eq!(
                (
                    spool.nozzle_temp_min,
                    spool.nozzle_temp_max,
                    spool.bed_temp_min,
                    spool.bed_temp_max
                ),
                values
            );
        }
    }

    #[test]
    fn imported_spool_color_lists_are_checked() {
        for (value, valid) in [
            (r##"["#ff0000","#00ff00","#0000ff","#ffffff"]"##, true),
            (r##"["#ff0000","#ff0000"]"##, true),
            (r##"["#ff0000","#00ff00","#0000ff","#ffffff","#000000"]"##, false),
            (r##"["#ff0000","red"]"##, false),
            (r##"["#ff00001"]"##, false),
            (r##"{"a":1}"##, false),
            ("not json", false),
        ] {
            let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
                db::insert_filament_spool(conn, &colour_spool(&[])).unwrap();
                conn.execute("UPDATE filament_spools SET color_hexes = ?1, color_hex = '#ff0000'", [value]).unwrap();
            });
            let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
            assert_eq!(result.is_ok(), valid, "{value}: {result:?}");
        }
    }

    #[test]
    fn an_imported_color_list_must_start_with_the_single_color() {
        for (single, valid) in [(Some("#ff0000"), true), (Some("#FF0000"), true), (Some("#0000ff"), false), (None, false)] {
            let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
                db::insert_filament_spool(conn, &colour_spool(&[])).unwrap();
                conn.execute(
                    "UPDATE filament_spools SET color_hexes = '[\"#ff0000\",\"#00ff00\"]', color_hex = ?1",
                    [single],
                )
                .unwrap();
            });
            let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
            assert_eq!(result.is_ok(), valid, "{single:?}: {result:?}");
        }
    }

    #[test]
    fn imported_source_values_are_checked() {
        for (column, value, valid) in [
            ("source_platform", "makerworld".to_string(), true),
            ("source_model_id", "é".repeat(128), true),
            ("source_model_id", "é".repeat(129), false),
            ("source_profile_id", "12\u{7}34".to_string(), false),
            ("source_profile_id", "12\u{202E}34".to_string(), false),
        ] {
            let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
                let id = crate::db::test_insert_minimal_file(conn, "/model.3mf", None).unwrap();
                conn.execute(&format!("UPDATE files SET {column} = ?1 WHERE id = ?2"), rusqlite::params![value, id]).unwrap();
            });
            let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
            assert_eq!(result.is_ok(), valid, "{column}: {result:?}");
        }
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let id = crate::db::test_insert_minimal_file(conn, "/model.3mf", None).unwrap();
            conn.execute("UPDATE files SET source_model_id = X'1234' WHERE id = ?1", [id]).unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err(), "a BLOB is the wrong storage class");
    }

    #[test]
    fn imported_legacy_text_fields_remain_valid() {
        for (column, limit) in [("manufacturer", 150), ("model", 150), ("tag", 120)] {
            for length in [limit, limit + 1] {
                let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
                    let value = "a".repeat(length);
                    if column == "tag" {
                        conn.execute("INSERT INTO tags (name, color_hue) VALUES (?1, 0)", [&value]).unwrap();
                    } else {
                        conn.execute(&format!("INSERT INTO printers (name, {column}) VALUES ('Printer', ?1)"), [&value]).unwrap();
                    }
                });
                let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
                assert!(result.is_ok(), "{column}: {result:?}");
            }
        }
    }

    #[test]
    fn imported_paths_have_a_length_limit() {
        for length in [4096, 4097] {
            let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
                conn.execute("INSERT INTO folders (name, path) VALUES ('root', ?1)", [format!("/{}", "a".repeat(length - 1))]).unwrap();
            });
            let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
            assert_eq!(result.is_ok(), length == 4096, "{result:?}");
        }
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_a_real_sqlite_database_with_files_table() {
        let tmp_path = std::env::temp_dir().join(format!(
            "validate_catalog_db_test_{}.db",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        {
            let conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
            drop(conn);
        }
        let bytes = std::fs::read(&tmp_path).expect("read temp db");

        let result = validate_catalog_db_bytes(&bytes, &[], &std::env::temp_dir(), &[]);

        let _ = std::fs::remove_file(&tmp_path);
        assert!(result.is_ok(), "expected valid catalog db to pass validation: {result:?}");
    }
    /// Freshly migrated catalog DB; `setup` can insert rows first.
    fn backup_test_db_with(setup: impl FnOnce(&Connection)) -> (Vec<u8>, Vec<PathBuf>, PathBuf) {
        let tmp_path = std::env::temp_dir().join(format!(
            "backup_test_db_with_{}.db",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        {
            let conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
            setup(&conn);
        }
        let bytes = std::fs::read(&tmp_path).expect("read temp db");
        let _ = std::fs::remove_file(&tmp_path);
        (bytes, Vec::new(), std::env::temp_dir())
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_printer_link_tables() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            conn.execute_batch(
                "INSERT INTO printers (id, name, position) VALUES (1, 'SV08', 0);
                 INSERT INTO app_settings (key, value) VALUES ('printer_link_enabled', '1');
                 INSERT INTO printer_connections (printer_id, kind, address, connected_since) VALUES (1, 'moonraker', '192.168.1.60', 1000.0);
                 INSERT INTO printer_jobs (printer_id, remote_id, file_name, outcome, raw_status, ended_at, print_duration_s, used_mm)
                     VALUES (1, 'A', 'a.gcode', 'completed', 'completed', 2000.0, 10.0, 5.0);",
            )
            .unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_ok());
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_printer_jobs_with_wrong_types() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            conn.execute_batch(
                "INSERT INTO printers (id, name, position) VALUES (1, 'SV08', 0);
                 INSERT INTO printer_jobs (printer_id, remote_id, file_name, outcome, raw_status, ended_at, print_duration_s, used_mm)
                     VALUES (1, 'A', 'a.gcode', 'completed', 'completed', 'gestern', 10.0, 5.0);",
            )
            .unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
    }

    #[test]
    fn validate_catalog_db_bytes_accepts_fractional_spool_weights() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                 VALUES ('PLA', 1.75, 1000.0, 612.4, '2026-09-24')",
                [],
            )
            .unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_ok());
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_garbage_bytes() {
        let result = validate_catalog_db_bytes(b"this is not a sqlite database", &[], &std::env::temp_dir(), &[]);
        assert!(result.is_err());
    }
    #[test]
    fn validate_backup_for_import_reports_a_rejected_backup_as_expected() {
        // Exercises the real boundary `import_catalog` calls, not a hand-copied
        // conversion: a bad backup file is the user's problem, not a fault, so it
        // must come out `expected`, not the default `expected: false` of a plain
        // String, and it is reported by its code.
        let err = validate_backup_for_import(b"this is not a sqlite database", &[], &std::env::temp_dir(), &[])
            .unwrap_err();
        assert!(err.expected, "a rejected backup file must be reported as expected, not a fault");
        assert!(err.is(ErrorCode::BackupInvalidDatabase, &[]), "{err:?}");
    }
    fn unique_test_db_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{name}_{}.db",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ))
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_corrupted_sqlite_file() {
        let bytes = b"SQLite format 3\x00this-is-not-actually-a-valid-database-body".to_vec();
        let sensitive_dirs = vec![];
        let trash_dir = unique_test_dir("validate_db_corrupt_trash");
        let result = validate_catalog_db_bytes(&bytes, &sensitive_dirs, &trash_dir, &[]);
        assert!(result.is_err());
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_foreign_key_violation() {
        let tmp_path = unique_test_db_path("validate_db_fk_violation");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            // File with a folder_id that points to no existing row in folders.
            conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
            conn.execute(
                "INSERT INTO files (name, path, file_type, folder_id, file_size_bytes, imported_at) VALUES ('x', '/tmp/x.3mf', '3mf', 999999, 1, '2026-01-01T00:00:00Z')",
                [],
            ).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err());
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_cyclic_folder_graph() {
        let tmp_path = unique_test_db_path("validate_db_folder_cycle");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute("INSERT INTO folders (name, path) VALUES ('A', '/tmp/A')", []).unwrap();
            let a_id = conn.last_insert_rowid();
            conn.execute("INSERT INTO folders (name, path) VALUES ('B', '/tmp/B')", []).unwrap();
            let b_id = conn.last_insert_rowid();
            conn.execute("UPDATE folders SET parent_id = ?1 WHERE id = ?2", params![b_id, a_id]).unwrap();
            conn.execute("UPDATE folders SET parent_id = ?1 WHERE id = ?2", params![a_id, b_id]).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err());
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_a_legitimate_pre_migration_backup() {
        // An older backup without migration columns (e.g. folders.parent_id) must be
        // accepted thanks to the migration up front.
        let tmp_path = unique_test_db_path("validate_db_pre_migration_backup");
        {
            let conn = rusqlite::Connection::open(&tmp_path).unwrap();
            conn.execute_batch(crate::db::SCHEMA_SQL).unwrap();
            // Deliberately NO run_migrations() here - simulates a DB exported before the
            // parent_id/path migration.
            conn.execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('x', '/tmp/x.3mf', '3mf', 1, '2026-01-01T00:00:00Z')",
                [],
            ).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();

        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);

        assert!(result.is_ok(), "a legitimate pre-migration backup must be accepted after internal migration, got: {result:?}");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_database_with_an_injected_trigger() {
        let tmp_path = unique_test_db_path("validate_db_injected_trigger");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute_batch(
                "CREATE TRIGGER evil AFTER DELETE ON folders
                 BEGIN
                     INSERT INTO folders (name, path) VALUES ('Injected', '/tmp/injected-by-trigger');
                 END;",
            ).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine importierte Datenbank mit einem eingeschleusten Trigger muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_database_with_an_injected_view() {
        let tmp_path = unique_test_db_path("validate_db_injected_view");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute_batch("CREATE VIEW evil_view AS SELECT * FROM folders;").unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine importierte Datenbank mit einer eingeschleusten View muss abgelehnt werden");
    }
    /// Runs the validation on a thread; a query that never ends fails the test
    /// instead of hanging it.
    fn validate_with_timeout(bytes: Vec<u8>) -> CmdResult<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]));
        });
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .expect("the validation must reject the database before running its SQL")
    }
    #[test]
    fn catalog_backup_copies_in_one_step_without_pauses() {
        // The catalog lock is held for the whole copy and the UI's synchronous
        // commands wait for it; a copy with pauses between pages froze the app
        // for minutes on catalogs with many embedded preview images.
        let conn = crate::db::connect_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE blobs(b BLOB);").unwrap();
        let blob = vec![7u8; 60 * 1024];
        for _ in 0..20 {
            conn.execute("INSERT INTO blobs(b) VALUES (?1)", [&blob]).unwrap();
        }
        let db = std::sync::Mutex::new(conn);
        let dest = unique_test_dir("backup_one_step").join("backup.zip");

        let started = std::time::Instant::now();
        write_catalog_backup(&db, &dest, "{}").unwrap();

        assert!(started.elapsed() < std::time::Duration::from_secs(2), "export took {:?}", started.elapsed());
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut zip.by_name("catalog.db").unwrap(), &mut bytes).unwrap();
        let copy = dest.with_file_name("copy.db");
        std::fs::write(&copy, bytes).unwrap();
        let copied = Connection::open(&copy).unwrap();
        let (rows, total): (i64, i64) =
            copied.query_row("SELECT count(*), sum(length(b)) FROM blobs", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((rows, total), (20, 20 * 60 * 1024));
    }

    #[test]
    fn catalog_backup_works_when_an_old_temp_db_with_the_same_name_exists() {
        // A leftover or a second export in the same process must not make the
        // export fail with "File exists".
        let stale = std::env::temp_dir().join(format!("3mf-katalog-export-{}.db", std::process::id()));
        std::fs::write(&stale, b"leftover").unwrap();
        let db = std::sync::Mutex::new(crate::db::connect_in_memory().unwrap());
        let dest = unique_test_dir("backup_stale_temp").join("backup.zip");

        let result = write_catalog_backup(&db, &dest, "{}");
        let _ = std::fs::remove_file(&stale);

        result.unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        assert!(zip.by_name("catalog.db").is_ok());
        assert!(zip.by_name("settings.json").is_ok());
    }

    #[test]
    fn catalog_backup_stores_the_given_settings_verbatim_in_the_two_entries_in_order() {
        // The desktop takes the settings text from the UI as it is; the core
        // export writes a fixed all-null document instead.
        let db = std::sync::Mutex::new(crate::db::connect_in_memory().unwrap());
        let dest = unique_test_dir("backup_settings_verbatim").join("backup.zip");
        let settings = r#"{"3mf-katalog-theme":"dark","extra":[1,2]}"#;

        write_catalog_backup(&db, &dest, settings).unwrap();

        let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        let names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().unwrap().to_string()).collect();
        assert_eq!(names, ["catalog.db", "settings.json"]);
        let mut stored = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("settings.json").unwrap(), &mut stored).unwrap();
        assert_eq!(stored, settings);
    }

    #[test]
    fn catalog_backup_replaces_an_existing_target_and_leaves_none_on_failure() {
        let db = std::sync::Mutex::new(crate::db::connect_in_memory().unwrap());
        let dir = unique_test_dir("backup_target");
        let dest = dir.join("backup.zip");
        std::fs::write(&dest, b"old content").unwrap();
        write_catalog_backup(&db, &dest, "{}").unwrap();
        assert!(zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).is_ok());

        let missing = dir.join("no-such-folder").join("backup.zip");
        assert!(write_catalog_backup(&db, &missing, "{}").is_err());
        assert!(!missing.exists());
    }

    #[cfg(unix)]
    #[test]
    fn backup_zip_ignores_a_planted_temp_symlink_and_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = unique_test_dir("backup_zip_symlink");
        let dest = dir.join("backup.zip");
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, b"KEEP").unwrap();
        std::os::unix::fs::symlink(&victim, dir.join("backup.zip.tmp")).unwrap();

        write_zip_atomically(&dest, &[("catalog.db", b"DB"), ("settings.json", b"{}")]).unwrap();

        assert_eq!(std::fs::read(&victim).unwrap(), b"KEEP");
        assert_eq!(std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777, 0o600);
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        assert_eq!(zip.len(), 2);
        assert_eq!(zip.by_name("catalog.db").unwrap().size(), 2);
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".tmp") && n != "backup.zip.tmp")
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
    #[cfg(unix)]
    #[test]
    fn temp_copy_for_validation_is_private_from_the_start() {
        use std::os::unix::fs::PermissionsExt;
        let path = unique_test_dir("temp_exclusive").join("check.db");
        write_temp_file_exclusive(&path, b"x").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_an_endless_view_named_files_without_running_it() {
        let tmp_path = unique_test_db_path("validate_db_endless_view");
        {
            let conn = rusqlite::Connection::open(&tmp_path).unwrap();
            conn.execute_batch(
                "CREATE VIEW files AS WITH RECURSIVE r(x) AS (VALUES(1) UNION ALL SELECT x + 1 FROM r) SELECT x FROM r;",
            )
            .unwrap();
        }
        let result = validate_with_timeout(std::fs::read(&tmp_path).unwrap());
        assert!(result.is_err());
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_virtual_table() {
        let tmp_path = unique_test_db_path("validate_db_virtual_table");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute_batch("CREATE VIRTUAL TABLE evil USING json_each('[1,2]');").ok();
            conn.execute_batch("CREATE VIRTUAL TABLE evil2 USING fts5(x);").ok();
            let virtual_tables: i64 = conn
                .query_row("SELECT COUNT(*) FROM sqlite_schema WHERE sql LIKE 'CREATE VIRTUAL TABLE%'", [], |r| r.get(0))
                .unwrap();
            if virtual_tables == 0 {
                return; // this SQLite build offers no virtual table module to test with
            }
        }
        let result = validate_with_timeout(std::fs::read(&tmp_path).unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn validate_catalog_db_bytes_rejects_a_database_missing_a_required_column() {
        // Same as above, but with exactly one column removed.
        let tmp_path = unique_test_db_path("validate_db_missing_column");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            // 'imported_at', because DROP COLUMN won't remove columns with UNIQUE or an index.
            conn.execute_batch("ALTER TABLE files DROP COLUMN imported_at;").unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine 'files'-Tabelle ohne die erforderliche Spalte 'imported_at' muss abgelehnt werden");
    }
    /// Migrated DB with one printer and one unit ("AMS A", 4 slots). Tests insert
    /// invalid rows via raw SQL, the API would reject them.
    fn printer_with_one_ams_unit(conn: &Connection) -> (i64, i64) {
        let printer_id = crate::db::printers::insert_printer(conn, "X1C").unwrap();
        let unit_id = crate::db::printers::insert_unit(conn, printer_id, "bambu_ams", "AMS A", None).unwrap();
        (printer_id, unit_id)
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_an_unknown_kind() {
        let tmp_path = unique_test_db_path("validate_db_unit_bad_kind");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let (printer_id, _) = printer_with_one_ams_unit(&conn);
            conn.execute("PRAGMA ignore_check_constraints = ON", []).unwrap();
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Toaster', 'toaster', 4, NULL, 1)",
                params![printer_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine Einheit mit unbekanntem 'kind' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_an_ams_2_pro_unit() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let (printer, _) = printer_with_one_ams_unit(conn);
            crate::db::printers::insert_unit(conn, printer, "bambu_ams_2_pro", "AMS 2 Pro", None).unwrap();
        });
        let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
        assert!(result.is_ok(), "ein Katalog mit AMS-2-Pro-Einheit muss gehen: {result:?}");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_slot_count_out_of_range() {
        let tmp_path = unique_test_db_path("validate_db_unit_bad_slot_count");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let (printer_id, _) = printer_with_one_ams_unit(&conn);
            conn.execute("PRAGMA ignore_check_constraints = ON", []).unwrap();
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Riesig', 'custom', 1000000000, NULL, 1)",
                params![printer_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(
            result.is_err(),
            "eine Einheit mit riesiger 'slot_count' muss abgelehnt werden (sonst haengt PrinterColumn.tsx beim Rendern)"
        );
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_bambu_ams_index_out_of_range() {
        let tmp_path = unique_test_db_path("validate_db_unit_bad_ams_index");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let (printer_id, _) = printer_with_one_ams_unit(&conn);
            conn.execute("PRAGMA ignore_check_constraints = ON", []).unwrap();
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'AMS X', 'bambu_ams', 4, 99, 1)",
                params![printer_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine Einheit mit 'bambu_ams_index' ausserhalb 0..=3 muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_dangling_printer_id() {
        let tmp_path = unique_test_db_path("validate_db_unit_dangling_printer");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (999999, 'Verwaist', 'custom', 4, NULL, 0)",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine Einheit mit nicht existierendem 'printer_id' muss abgelehnt werden");
    }
    /// Recreate `material_units` without NOT NULL and CHECK, as in a crafted DB;
    /// otherwise the INSERT itself would reject the test data.
    fn drop_material_units_constraints(conn: &Connection) {
        conn.execute_batch(
            "DROP TABLE material_units;
             CREATE TABLE material_units (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                printer_id INTEGER,
                name TEXT,
                kind TEXT,
                slot_count INTEGER,
                bambu_ams_index INTEGER,
                position INTEGER
             );",
        )
        .unwrap();
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_null_kind() {
        // `kind NOT IN (...)` is UNKNOWN for NULL, so it needs `kind IS NULL`.
        let tmp_path = unique_test_db_path("validate_db_unit_null_kind");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let printer_id = crate::db::printers::insert_printer(&conn, "X1C").unwrap();
            drop_material_units_constraints(&conn);
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Kaputt', NULL, 4, NULL, 0)",
                params![printer_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine Einheit mit NULL 'kind' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_null_slot_count() {
        let tmp_path = unique_test_db_path("validate_db_unit_null_slot_count");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let printer_id = crate::db::printers::insert_printer(&conn, "X1C").unwrap();
            drop_material_units_constraints(&conn);
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Kaputt', 'custom', NULL, NULL, 0)",
                params![printer_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine Einheit mit NULL 'slot_count' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_null_printer_id() {
        let tmp_path = unique_test_db_path("validate_db_unit_null_printer_id");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            drop_material_units_constraints(&conn);
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (NULL, 'Kaputt', 'custom', 4, NULL, 0)",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine Einheit mit NULL 'printer_id' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_printer_with_a_null_name() {
        let tmp_path = unique_test_db_path("validate_db_printer_null_name");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute_batch(
                "DROP TABLE printers;
                 CREATE TABLE printers (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT,
                    position INTEGER
                 );",
            )
            .unwrap();
            conn.execute("INSERT INTO printers (name, position) VALUES (NULL, 0)", []).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "ein Drucker mit NULL 'name' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_spool_in_a_slot_of_a_unit_with_a_null_slot_count() {
        // Covers the COALESCE guard in `bad_spools`.
        let tmp_path = unique_test_db_path("validate_db_spool_slot_in_null_slot_count_unit");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let printer_id = crate::db::printers::insert_printer(&conn, "X1C").unwrap();
            drop_material_units_constraints(&conn);
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Kaputt', 'custom', NULL, NULL, 0)",
                params![printer_id],
            )
            .unwrap();
            let unit_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', ?1, 999)",
                params![unit_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine Spule mit slot_index 999 in einer Einheit ohne gueltige Fachanzahl muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_spool_with_slot_index_set_but_no_unit() {
        let tmp_path = unique_test_db_path("validate_db_spool_half_placement");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', NULL, 0)",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(
            result.is_err(),
            "eine Spule mit gesetztem 'slot_index' aber ohne 'unit_id' muss abgelehnt werden"
        );
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_spool_with_a_dangling_unit_id() {
        let tmp_path = unique_test_db_path("validate_db_spool_dangling_unit");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', 999999, 0)",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine Spule mit nicht existierender 'unit_id' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_spool_with_slot_index_outside_the_units_range() {
        let tmp_path = unique_test_db_path("validate_db_spool_slot_out_of_range");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let (_, unit_id) = printer_with_one_ams_unit(&conn); // 4 slots, valid are 0..=3
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', ?1, 4)",
                params![unit_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine Spule mit 'slot_index' >= der Fachanzahl ihrer Einheit muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_two_spools_in_the_same_slot() {
        let tmp_path = unique_test_db_path("validate_db_duplicate_slot");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let (_, unit_id) = printer_with_one_ams_unit(&conn);
            // Drop the unique index, as it may be missing in a crafted DB.
            conn.execute("DROP INDEX idx_filament_spools_slot", []).unwrap();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', ?1, 0)",
                params![unit_id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PETG', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', ?1, 0)",
                params![unit_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "zwei Spulen im selben Fach muessen abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_an_invalid_color_hex() {
        let tmp_path = unique_test_db_path("validate_db_bad_color_hex");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, color_hex)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', 'rot')",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "ein 'color_hex' ausserhalb des #rrggbb-Formats muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_a_valid_backup_with_loaded_spools() {
        let tmp_path = unique_test_db_path("validate_db_valid_with_loaded_spools");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let (_, unit_id) = printer_with_one_ams_unit(&conn);
            let spool_id = crate::db::insert_filament_spool(
                &conn,
                &crate::db::models::NewFilamentSpool {
                    nozzle_temp_min: None,
                    nozzle_temp_max: None,
                    bed_temp_min: None,
                    bed_temp_max: None,
                    material: "PLA".to_string(),
                    manufacturer: None,
                    color: None,
                    location: Some("Regal 2".to_string()),
                    diameter_mm: 1.75,
                    original_weight_g: 1000.0,
                    remaining_weight_g: 800.0,
                    price: None,
                    image_png: None,
                    color_hex: Some("#1a1a1a".to_string()),
                    color_hexes: Vec::new(),
                    kind: "filament".to_string(),
                },
            )
            .unwrap();
            crate::db::printers::load_spool(&conn, spool_id, unit_id, 0).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_ok(), "ein gueltiges Backup mit belegten Faechern muss akzeptiert werden: {result:?}");
    }
    // ---- Storage classes ----
    // Values can look valid and still have the wrong storage class (e.g.
    // `slot_count = 4.5`); rusqlite then fails on read.
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_real_slot_count() {
        // 4.5 stays REAL despite INTEGER affinity and lies "between" 1 and 16.
        let tmp_path = unique_test_db_path("validate_db_unit_real_slot_count");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let printer_id = crate::db::printers::insert_printer(&conn, "X1C").unwrap();
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Kaputt', 'custom', 4.5, NULL, 0)",
                params![printer_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine Einheit mit 'slot_count' als REAL-Speicherklasse muss abgelehnt werden");
    }
    /// Recreate `printers` with an untyped `name`: without affinity an INTEGER value
    /// stays INTEGER instead of becoming TEXT.
    fn drop_printers_constraints(conn: &Connection) {
        conn.execute_batch(
            "DROP TABLE printers;
             CREATE TABLE printers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name,
                position INTEGER,
                kind TEXT
             );",
        )
        .unwrap();
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_printer_with_an_integer_name() {
        let tmp_path = unique_test_db_path("validate_db_printer_integer_name");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            drop_printers_constraints(&conn);
            conn.execute("INSERT INTO printers (name, position, kind) VALUES (12345, 0, 'filament')", []).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "ein Drucker mit 'name' als INTEGER-Speicherklasse muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_filament_spool_with_remaining_weight_g_stored_as_text() {
        // No constraint rebuild needed: INTEGER affinity only converts a TEXT value
        // if it LOOKS like a number - 'viel' stays TEXT.
        let tmp_path = unique_test_db_path("validate_db_spool_text_remaining_weight");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                 VALUES ('PLA', 1.75, 1000, 'viel', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine Spule mit 'remaining_weight_g' als TEXT-Speicherklasse muss abgelehnt werden");
    }
    /// Recreate `filament_spools` without NOT NULL (see `drop_material_units_constraints`).
    fn drop_filament_spools_constraints(conn: &Connection) {
        conn.execute_batch(
            "DROP TABLE filament_spools;
             CREATE TABLE filament_spools (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                material TEXT, manufacturer TEXT, color TEXT, location TEXT,
                diameter_mm REAL, original_weight_g INTEGER, remaining_weight_g INTEGER,
                price REAL, image_png BLOB, created_at TEXT,
                unit_id INTEGER, slot_index INTEGER, home_location TEXT, color_hex TEXT
             );",
        )
        .unwrap();
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_filament_spool_with_a_null_material() {
        // `material` is a non-optional String in Rust; only the type check catches NULL.
        let tmp_path = unique_test_db_path("validate_db_spool_null_material");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            drop_filament_spools_constraints(&conn);
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                 VALUES (NULL, 1.75, 1000, 1000, '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(result.is_err(), "eine Spule mit NULL 'material' muss abgelehnt werden");
    }

    #[test]
    fn validate_catalog_db_bytes_accepts_a_full_catalog_with_data_in_every_table() {
        // A normally filled catalog (data in every table) must not be rejected.
        let tmp_path = unique_test_db_path("validate_db_full_catalog");
        {
            let mut conn = crate::db::connect(&tmp_path).unwrap();

            let folder_id =
                crate::db::insert_folder_with_parent(&conn, "Projekte", None, "/tmp/Projekte").unwrap();

            let mut metadata = BTreeMap::new();
            metadata.insert("Slicer".to_string(), "Bambu Studio".to_string());
            let new_file = NewFile {
                license: None,
                source: Default::default(),
                name: "Gehaeuse.3mf".to_string(),
                path: "/tmp/Projekte/Gehaeuse.3mf".to_string(),
                file_type: FileType::ThreeMf,
                folder_id: Some(folder_id),
                origin: "local".to_string(),
                cloud_id: None,
                sync_status: "local-only".to_string(),
                file_size_bytes: 12345,
                dimensions_mm: Some([10.0, 20.0, 30.0]),
                volume_cm3: Some(123.4),
                object_count: Some(3),
                thumbnail_png: Some(vec![1, 2, 3]),
                imported_at: "2026-09-23T00:00:00Z".to_string(),
                file_modified_at: Some("2026-09-23T00:00:00Z".to_string()),
                materials: vec![MaterialRecord { name: "PLA".to_string(), display_color: Some("#ff0000".to_string()) }],
                metadata,
                tags: vec!["Prototyp".to_string()],
                print_status: "printed".to_string(),
                last_viewed_at: Some("2026-09-23T00:00:00Z".to_string()),
                creator: Some("Max".to_string()),
                content_hash: Some("abc123".to_string()),
                render_snapshot_png: Some(vec![4, 5, 6]),
                custom_image_png: Some(vec![7, 8, 9]),
                source_url: Some("https://example.com/model".to_string()),
                queue_position: Some(1),
                favorite: true,
                plate_count: Some(2),
                plate_dimensions_json: None,
                slice_info_json: Some("{}".to_string()),
                print_info: None,
            };
            let file_id = crate::db::insert_file(&mut conn, &new_file).unwrap();

            let created_at = chrono::Utc::now().to_rfc3339();
            let collection_id = crate::db::create_collection(&conn, "Sammlung", &created_at).unwrap();
            crate::db::add_file_to_collection(&conn, collection_id, file_id, 0).unwrap();

            let slicer_executable = std::env::current_exe().unwrap().to_string_lossy().to_string();
            crate::db::insert_registered_slicer(&conn, "Bambu Studio", &slicer_executable, true).unwrap();

            let (_, unit_id) = printer_with_one_ams_unit(&conn);
            let spool_id = crate::db::insert_filament_spool(
                &conn,
                &crate::db::models::NewFilamentSpool {
                    nozzle_temp_min: None,
                    nozzle_temp_max: None,
                    bed_temp_min: None,
                    bed_temp_max: None,
                    material: "PLA".to_string(),
                    manufacturer: Some("Bambu".to_string()),
                    color: Some("Schwarz".to_string()),
                    location: Some("Regal 2".to_string()),
                    diameter_mm: 1.75,
                    original_weight_g: 1000.0,
                    remaining_weight_g: 800.0,
                    price: Some(19.99),
                    image_png: Some(vec![9, 9, 9]),
                    color_hex: Some("#1a1a1a".to_string()),
                    color_hexes: Vec::new(),
                    kind: "filament".to_string(),
                },
            )
            .unwrap();
            crate::db::printers::load_spool(&conn, spool_id, unit_id, 0).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
        assert!(
            result.is_ok(),
            "ein regulaer befuellter Katalog mit Daten in jeder Tabelle muss weiterhin akzeptiert werden: {result:?}"
        );
    }
    // --- print data tables (file_print_info, file_print_plates) ---

    fn print_record(slice_state: crate::threemf::print_info::SliceState, plates: &[i64]) -> db::models::PrintInfoRecord {
        use db::models::{PrintInfoRecord, PrintPlateRecord};
        use crate::threemf::print_info::SettingsState;
        PrintInfoRecord {
            reader_version: 1,
            read_at: "2026-10-31T10:00:00+00:00".to_string(),
            slice_state,
            settings_state: SettingsState::Read,
            printer_model: Some("Test Printer".to_string()),
            bed_type: Some("Test Plate".to_string()),
            layer_height_mm: Some(0.2),
            nozzle_mm: Some(0.4),
            nozzle_mixed: false,
            bed_temp_c: Some(60.0),
            bed_temp_mixed: false,
            nozzle_temp_c: None,
            nozzle_temp_mixed: true,
            plates: plates
                .iter()
                .map(|i| PrintPlateRecord {
                    plate_index: *i,
                    plate_name: Some(format!("Plate {i}")),
                    print_seconds: Some(600 + i),
                    first_layer_seconds: Some(12.5),
                    plate_bed_type: Some("supertack_plate".to_string()),
                })
                .collect(),
            filament_colors: vec!["#ff0000".to_string(), "#00ff00".to_string()],
        }
        .validated()
    }

    fn file_with_print_data(path: &std::path::Path, record: db::models::PrintInfoRecord) -> NewFile {
        let mut file = sample_new_file_for_rescan_test(path);
        file.name = path.file_name().unwrap().to_string_lossy().to_string();
        file.print_info = Some(record);
        file
    }

    #[test]
    fn print_data_survives_a_backup_and_restore() {
        use crate::threemf::print_info::SliceState;
        let (source, dir) = state_with_catalog("backup_print_data_a");
        let (sliced_id, source_id, sliced, no_data);
        {
            let mut conn = source.db.lock().unwrap();
            sliced = print_record(SliceState::Sliced, &[1, 2]);
            no_data = print_record(SliceState::NoSliceData, &[1, 2, 4]);
            let mut sliced_file = file_with_print_data(&dir.join("sliced.3mf"), sliced.clone());
            sliced_file.plate_dimensions_json = Some(r#"[{"number":1,"name":"lithophane","dimensionsMm":[144,108,2.5]}]"#.into());
            sliced_id = db::insert_file(&mut conn, &sliced_file).unwrap();
            source_id = db::insert_file(&mut conn, &file_with_print_data(&dir.join("source.3mf"), no_data.clone())).unwrap();
        }
        let zip_path = dir.join("backup.zip");
        write_catalog_backup(&source.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        validate_catalog_db_bytes(&db_bytes, &[], &dir, &[]).expect("a backup with print data validates");
        let incoming = dir.join("incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_print_data_b");
        replace_catalog_db(&target, &incoming).unwrap();

        let conn = target.db.lock().unwrap();
        let restored = |id: i64| db::get_file(&conn, id).unwrap().unwrap().print_info.expect("print data restored");
        assert_eq!(restored(sliced_id), sliced);
        assert_eq!(restored(source_id), no_data);
        assert_eq!(db::get_file(&conn, sliced_id).unwrap().unwrap().plate_dimensions_json.as_deref(),
            Some(r#"[{"number":1,"name":"lithophane","dimensionsMm":[144,108,2.5]}]"#));
        assert!(db::get_file(&conn, source_id).unwrap().unwrap().plate_dimensions_json.is_none());
    }

    #[test]
    fn a_backup_from_before_the_print_data_tables_can_be_restored() {
        let (old, dir) = state_with_catalog("backup_print_data_old_a");
        {
            let mut conn = old.db.lock().unwrap();
            let file = sample_new_file_for_rescan_test(&dir.join("old.3mf"));
            db::insert_file(&mut conn, &file).unwrap();
            conn.execute_batch("DROP TABLE file_print_plates; DROP TABLE file_print_info; ALTER TABLE files DROP COLUMN plate_dimensions_json;").unwrap();
            conn.pragma_update(None, "user_version", 52).unwrap(); // the version before the print tables (a shipped number, not derived from the current one)
        }
        let zip_path = dir.join("old-backup.zip");
        write_catalog_backup(&old.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let probe = dir.join("probe.db");
        std::fs::write(&probe, &db_bytes).unwrap();
        assert!(Connection::open(&probe).unwrap().prepare("SELECT file_id FROM file_print_info").is_err(), "precondition: the backup has no such table");

        validate_catalog_db_bytes(&db_bytes, &[], &dir, &[]).expect("an older backup must validate");
        let incoming = dir.join("old-incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_print_data_old_b");
        replace_catalog_db(&target, &incoming).unwrap();

        let conn = target.db.lock().unwrap();
        let count = |table: &str| conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!((count("file_print_info"), count("file_print_plates")), (0, 0), "tables exist and are empty");
        assert_eq!(db::list_files(&conn).unwrap().len(), 1, "the file itself is kept");
        assert!(db::list_files(&conn).unwrap()[0].plate_dimensions_json.is_none());
    }

    #[test]
    fn a_backup_from_version_53_without_the_color_column_can_be_restored() {
        use crate::threemf::print_info::SliceState;
        let (old, dir) = state_with_catalog("backup_filament_colors_old_a");
        let (id, mut record);
        {
            let mut conn = old.db.lock().unwrap();
            record = print_record(SliceState::Sliced, &[1, 2]);
            record.filament_colors.clear();
            id = db::insert_file(&mut conn, &file_with_print_data(&dir.join("v53.3mf"), record.clone())).unwrap();
            conn.execute_batch("ALTER TABLE file_print_info DROP COLUMN filament_colors").unwrap();
            conn.pragma_update(None, "user_version", 53).unwrap(); // the version before the color column
        }
        let zip_path = dir.join("v53-backup.zip");
        write_catalog_backup(&old.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let probe = dir.join("v53-probe.db");
        std::fs::write(&probe, &db_bytes).unwrap();
        assert!(Connection::open(&probe).unwrap().prepare("SELECT filament_colors FROM file_print_info").is_err(), "precondition: the backup has no color column");

        validate_catalog_db_bytes(&db_bytes, &[], &dir, &[]).expect("a version 53 backup must validate");
        let incoming = dir.join("v53-incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_filament_colors_old_b");
        replace_catalog_db(&target, &incoming).unwrap();

        let conn = target.db.lock().unwrap();
        let restored = db::get_file(&conn, id).unwrap().unwrap().print_info.expect("print data restored");
        assert_eq!(restored, record, "the row is kept, its colors are unknown");
        assert!(conn.prepare("SELECT filament_colors FROM file_print_info").is_ok(), "the migration added the column");
    }

    // --- bundle tables (bundles, bundle_members) ---

    #[test]
    fn bundles_survive_a_backup_and_restore_with_order_and_labels() {
        let (source, dir) = state_with_catalog("backup_bundles_a");
        let ids: Vec<i64> = {
            let conn = source.db.lock().unwrap();
            let ids: Vec<i64> = (0..3).map(|i| db::test_insert_minimal_file(&conn, &format!("/tmp/b{i}.3mf"), None).unwrap()).collect();
            let bundle = db::bundles::create_bundle(&conn, "Vase", db::bundles::BundleKind::Parts, &[ids[2], ids[0], ids[1]], "2026-11-04T10:00:00Z").unwrap();
            db::bundles::update_bundle_member(&conn, ids[0], Some("Lid"), false).unwrap();
            assert!(db::bundles::get_bundle(&conn, bundle).unwrap().is_some());
            ids
        };
        let zip_path = dir.join("backup.zip");
        write_catalog_backup(&source.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        validate_catalog_db_bytes(&db_bytes, &[], &dir, &[]).expect("a backup with bundles validates");
        let incoming = dir.join("incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_bundles_b");
        replace_catalog_db(&target, &incoming).unwrap();

        let conn = target.db.lock().unwrap();
        let bundles = db::bundles::list_bundles(&conn).unwrap();
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].name, "Vase");
        assert_eq!(bundles[0].members.iter().map(|m| m.file_id).collect::<Vec<_>>(), vec![ids[2], ids[0], ids[1]]);
        assert_eq!(bundles[0].members[1].label.as_deref(), Some("Lid"));
    }

    #[test]
    fn a_backup_from_before_the_bundle_tables_can_be_restored() {
        let (old, dir) = state_with_catalog("backup_bundles_old_a");
        {
            let conn = old.db.lock().unwrap();
            db::test_insert_minimal_file(&conn, "/tmp/old.3mf", None).unwrap();
            conn.execute_batch("DROP TABLE bundle_members; DROP TABLE bundles").unwrap();
            conn.pragma_update(None, "user_version", 56).unwrap(); // the version before the bundle tables (a development number, not derived from the current one)
        }
        let zip_path = dir.join("old-backup.zip");
        write_catalog_backup(&old.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let probe = dir.join("probe.db");
        std::fs::write(&probe, &db_bytes).unwrap();
        assert!(Connection::open(&probe).unwrap().prepare("SELECT file_id FROM bundle_members").is_err(), "precondition: the backup has no such table");

        validate_catalog_db_bytes(&db_bytes, &[], &dir, &[]).expect("an older backup must validate");
        let incoming = dir.join("old-incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_bundles_old_b");
        replace_catalog_db(&target, &incoming).unwrap();

        let conn = target.db.lock().unwrap();
        let count = |table: &str| conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!((count("bundles"), count("bundle_members")), (0, 0), "tables exist and are empty");
        assert_eq!(db::list_files(&conn).unwrap().len(), 1, "the file itself is kept");
    }

    /// A backup whose `slicer_reports` carries no CHECK (as a crafted file can); `setup` adds the rows under test.
    fn slicer_report_case(setup: &str) -> Result<(), String> {
        static TEMPLATE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        let template = TEMPLATE.get_or_init(|| {
            backup_test_db_with(|conn| {
                conn.execute_batch(
                    "DROP TABLE slicer_reports;
                     CREATE TABLE slicer_reports (id INTEGER PRIMARY KEY, reported_at TEXT, reported_path TEXT, file_name TEXT,
                         file_size_bytes INTEGER, slicer TEXT, summary_json TEXT, state TEXT, suggested_file_id INTEGER,
                         model_file_id INTEGER, decided_at TEXT);",
                )
                .unwrap();
            })
            .0
        });
        let path = unique_test_db_path("slicer_report_case");
        std::fs::write(&path, template).unwrap();
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(setup).unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        validate_catalog_db_bytes(&bytes, &[], &std::env::temp_dir(), &[]).map_err(|e| e.message)
    }

    const GOOD_REPORT: &str = "INSERT INTO slicer_reports (reported_at, reported_path, file_name, file_size_bytes, summary_json, state)
        VALUES ('2026-11-05T10:00:00+00:00', '/o/Box.gcode', 'Box.gcode', 5, '{\"printSeconds\":60}', 'open');";

    #[test]
    fn slicer_reports_survive_the_restore_check_and_crafted_ones_do_not() {
        assert_eq!(slicer_report_case(GOOD_REPORT), Ok(()));
        let cases: Vec<(&str, String)> = vec![
            ("unknown state", format!("{GOOD_REPORT} UPDATE slicer_reports SET state = 'done';")),
            ("negative size", format!("{GOOD_REPORT} UPDATE slicer_reports SET file_size_bytes = -1;")),
            ("size NULL", format!("{GOOD_REPORT} UPDATE slicer_reports SET file_size_bytes = NULL;")),
            ("control character in the path", format!("{GOOD_REPORT} UPDATE slicer_reports SET reported_path = '/o/a' || char(7) || 'b';")),
            ("name empty", format!("{GOOD_REPORT} UPDATE slicer_reports SET file_name = '';")),
            ("path too long", format!("{GOOD_REPORT} UPDATE slicer_reports SET reported_path = '/{}';", "p".repeat(1100))),
            ("time not a time", format!("{GOOD_REPORT} UPDATE slicer_reports SET reported_at = 'gestern';")),
            ("summary not JSON", format!("{GOOD_REPORT} UPDATE slicer_reports SET summary_json = 'x';")),
            ("summary too large", format!("{GOOD_REPORT} UPDATE slicer_reports SET summary_json = '{{\"x\":\"{}\"}}';", "a".repeat(17_000))),
            ("name stored as a BLOB", format!("{GOOD_REPORT} UPDATE slicer_reports SET file_name = x'4142';")),
        ];
        for (what, setup) in cases {
            assert!(slicer_report_case(&setup).is_err(), "{what}: the restore validation must refuse it");
        }
    }

    #[test]
    fn restore_forgets_catalog_location_and_allows_current_location() {
        for saved in [Some("/backup/location-a"), None] {
            let dir = unique_test_dir("restore_catalog_location");
            let db_path = dir.join("catalog.db");
            let state = AppState {
                import_jobs: ImportJobs::default(),
                db: Mutex::new(db::connect(&db_path).unwrap()),
                trash_dir: dir.join("trash"),
                db_path,
                sensitive_dirs: Vec::new(),
            };
            let incoming_path = dir.join("incoming.db");
            let incoming = db::connect(&incoming_path).unwrap();
            if let Some(saved) = saved {
                db::printer_link::set_setting(&incoming, "catalog_base_dir", saved).unwrap();
            }
            drop(incoming);
            replace_catalog_db(&state, &incoming_path).unwrap();
            let conn = state.db.lock().unwrap();
            assert_eq!(db::printer_link::get_setting(&conn, "catalog_base_dir").unwrap(), None);
            let current = dir.join("location-b");
            std::fs::create_dir(&current).unwrap();
            let folder = super::super::folders::register_existing_catalog_base_dir_with_conn(&conn, &current, &[]).unwrap().unwrap();
            assert_eq!(folder.path, current.to_string_lossy());
            assert_eq!(db::printer_link::get_setting(&conn, "catalog_base_dir").unwrap(), Some(folder.path));
        }
    }

    #[test]
    fn replace_catalog_db_backs_up_old_db_and_installs_new_one() {
        let dir = unique_test_dir("replace_catalog_db_success");
        let db_path = dir.join("catalog.db");

        // "Old" running DB: empty catalog.
        let old_conn = crate::db::connect(&db_path).expect("connect creates schema");
        drop(old_conn);
        let old_bytes = std::fs::read(&db_path).expect("read old db bytes");

        // "New" DB (simulates the catalog.db extracted from the zip) with one record,
        // so old and new can be told apart.
        let new_db_path = dir.join("incoming_catalog.db");
        let new_conn = crate::db::connect(&new_db_path).expect("connect creates schema");
        new_conn
            .execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at)
                 VALUES ('new.3mf', '/incoming/new.3mf', '3mf', 42, '2026-09-13T00:00:00Z')",
                [],
            )
            .expect("seed new db with a marker row");
        new_conn.execute("UPDATE files SET content_hash = 'forged'", []).unwrap();
        drop(new_conn);
        let new_bytes = std::fs::read(&new_db_path).expect("read new db bytes");
        assert_ne!(old_bytes, new_bytes, "old and new db content must differ for this test to be meaningful");

        // AppState initially holds the "old" connection on db_path.
        let running_conn = crate::db::connect(&db_path).expect("reopen db for AppState");
        let state = AppState {
            import_jobs: ImportJobs::default(),
            db: Mutex::new(running_conn),
            trash_dir: dir.join("trash"),
            db_path: db_path.clone(),
            sensitive_dirs: Vec::new(),
        };

        let result = replace_catalog_db(&state, &new_db_path);
        assert!(result.is_ok(), "expected successful replacement: {result:?}");
        {
            let conn = state.db.lock().unwrap();
            let file = db::list_files(&conn).unwrap().remove(0);
            assert!(file.content_hash.is_none());
            assert_eq!(file.imported_at, "2026-09-13T00:00:00Z");
        }


        // The old DB was renamed to exactly one .bak-* file (not deleted) and its
        // content matches the old catalog.
        let bak_entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("catalog.db.bak-"))
            .collect();
        assert_eq!(bak_entries.len(), 1, "expected exactly one backup file");
        // Not byte-identical: the keys are scrubbed from it (and the file vacuumed) once the new
        // database is active. It still holds the old catalog.
        let bak_conn = Connection::open(bak_entries[0].path()).expect("open backup db");
        let bak_files: i64 = bak_conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0)).unwrap();
        assert_eq!(bak_files, 0, "backup file must contain the old (empty) catalog, not the new one");
        drop(bak_conn);

        // No byte comparison possible: sanitizing rewrites the incoming DB before
        // copying. So check the content instead.
        let installed_bytes = std::fs::read(&db_path).expect("read installed db bytes");
        assert_ne!(installed_bytes, old_bytes, "db_path must no longer contain the old db content");

        // The connection points to the new content, not the placeholder.
        let guard = state.db.lock().unwrap();
        let count: i64 =
            guard.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "connection must reflect the newly installed db's content");
        drop(guard);

        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn replace_catalog_db_leaves_no_api_key_or_token_in_the_fallback_copy() {
        let dir = unique_test_dir("replace_catalog_db_fallback_secrets");
        let db_path = dir.join("catalog.db");
        {
            let old = crate::db::connect(&db_path).unwrap();
            old.execute("INSERT INTO printers (id, name, position) VALUES (1, 'P', 0)", []).unwrap();
            old.execute(
                "INSERT INTO printer_connections (printer_id, kind, address, connected_since, api_key) VALUES (1, 'moonraker', 'printer', 0, 'secretkey123')",
                [],
            )
            .unwrap();
            db::printer_link::set_setting(&old, db::thingiverse::SETTING_TOKEN, "secrettoken123").unwrap();
        }
        let new_db_path = dir.join("incoming_catalog.db");
        drop(crate::db::connect(&new_db_path).unwrap());
        let state = AppState {
            import_jobs: ImportJobs::default(),
            db: Mutex::new(crate::db::connect(&db_path).unwrap()),
            trash_dir: dir.join("trash"),
            db_path: db_path.clone(),
            sensitive_dirs: Vec::new(),
        };
        replace_catalog_db(&state, &new_db_path).unwrap();
        let bak: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("catalog.db.bak-"))
            .collect();
        assert_eq!(bak.len(), 1);
        let bytes = std::fs::read(bak[0].path()).unwrap();
        for secret in [&b"secretkey123"[..], &b"secrettoken123"[..]] {
            assert!(!bytes.windows(secret.len()).any(|w| w == secret), "the fallback copy still holds a secret");
        }
        // It is still the old catalog, openable.
        let old = Connection::open(bak[0].path()).unwrap();
        let printers: i64 = old.query_row("SELECT COUNT(*) FROM printers", [], |r| r.get(0)).unwrap();
        assert_eq!(printers, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_catalog_db_fails_closed_when_the_incoming_db_path_does_not_exist() {
        // A missing file makes Connection::open create an empty DB; the migration
        // then fails before the old catalog is touched.
        let dir = unique_test_dir("replace_catalog_db_missing_incoming");
        let db_path = dir.join("catalog.db");

        let old_conn = crate::db::connect(&db_path).expect("connect creates schema");
        old_conn
            .execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at)
                 VALUES ('old.3mf', '/old/old.3mf', '3mf', 7, '2026-09-13T00:00:00Z')",
                [],
            )
            .expect("seed old db with a marker row");
        drop(old_conn);
        let old_bytes = std::fs::read(&db_path).expect("read old db bytes");

        // Doesn't exist on purpose.
        let missing_new_db_path = dir.join("does_not_exist.db");

        let running_conn = crate::db::connect(&db_path).expect("reopen db for AppState");
        let state = AppState {
            import_jobs: ImportJobs::default(),
            db: Mutex::new(running_conn),
            trash_dir: dir.join("trash"),
            db_path: db_path.clone(),
            sensitive_dirs: Vec::new(),
        };

        let result = replace_catalog_db(&state, &missing_new_db_path);
        assert!(result.is_err(), "expected an error when the new db file is missing");

        // No .bak-* was created at all - fail-closed kicks in BEFORE the first
        // rename, state.db_path was never touched.
        let bak_entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("catalog.db.bak-"))
            .collect();
        assert!(bak_entries.is_empty(), "no backup file should ever have been created - the sanitization gate fails before any rename");
        assert!(db_path.exists(), "catalog.db must still exist under its original, untouched path");
        let restored_bytes = std::fs::read(&db_path).expect("read db bytes");
        assert_eq!(restored_bytes, old_bytes, "old db content must be completely untouched");

        // The connection was never swapped, the old DB is still visible.
        let guard = state.db.lock().unwrap();
        let count: i64 =
            guard.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "connection must still reflect the untouched original db");
        drop(guard);

        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn replace_catalog_db_restores_backup_when_the_copy_step_fails_for_a_valid_sanitized_incoming_db() {
        // Checks the rollback when exactly `fs::copy` fails. File permissions can't
        // force this (the rename would fail first), hence a failing `copy_fn`.
        let dir = unique_test_dir("replace_catalog_db_copy_step_fails");
        let db_path = dir.join("catalog.db");

        let old_conn = crate::db::connect(&db_path).expect("connect creates schema");
        old_conn
            .execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at)
                 VALUES ('old.3mf', '/old/old.3mf', '3mf', 7, '2026-09-13T00:00:00Z')",
                [],
            )
            .expect("seed old db with a marker row");
        drop(old_conn);
        let old_bytes = std::fs::read(&db_path).expect("read old db bytes");

        // A valid incoming DB, so the copy step really is what fails.
        let new_db_path = dir.join("incoming_catalog.db");
        let new_conn = crate::db::connect(&new_db_path).expect("connect creates schema");
        new_conn
            .execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at)
                 VALUES ('new.3mf', '/incoming/new.3mf', '3mf', 42, '2026-09-13T00:00:00Z')",
                [],
            )
            .expect("seed new db with a marker row");
        drop(new_conn);

        let running_conn = crate::db::connect(&db_path).expect("reopen db for AppState");
        let state = AppState {
            import_jobs: ImportJobs::default(),
            db: Mutex::new(running_conn),
            trash_dir: dir.join("trash"),
            db_path: db_path.clone(),
            sensitive_dirs: Vec::new(),
        };

        let result = replace_catalog_db_with_copy_fn(&state, &new_db_path, |_from, _to| {
            Err(std::io::Error::other("simulated disk failure during copy"))
        });
        assert!(result.is_err(), "expected an error when the copy step itself fails");
        let err = result.unwrap_err();
        assert!(
            err.contains("wiederhergestellt"),
            "expected error to mention successful restoration, got: {err}"
        );

        // After the failed copy, the old DB was renamed back to its original place -
        // no .bak-* is left and db_path holds the old content again.
        let bak_entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("catalog.db.bak-"))
            .collect();
        assert!(bak_entries.is_empty(), "backup file must have been renamed back after successful restore");
        assert!(db_path.exists(), "catalog.db must exist again after restore");
        let restored_bytes = std::fs::read(&db_path).expect("read restored db bytes");
        assert_eq!(restored_bytes, old_bytes, "restored db must match the original content");

        // The connection reads the old DB again, neither the placeholder nor the
        // never-activated incoming DB.
        let guard = state.db.lock().unwrap();
        let count: i64 =
            guard.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "connection must reflect the restored old db's content, not the placeholder");
        let has_new_marker: bool = guard
            .query_row("SELECT COUNT(*) FROM files WHERE name = 'new.3mf'", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|c| c > 0)
            .unwrap_or(false);
        assert!(!has_new_marker, "the never-activated incoming db's content must not be visible");
        drop(guard);

        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn restoring_a_catalog_backup_does_not_overwrite_the_local_slicer_registry() {
        // A restore must neither adopt nor merge registered_slicers.
        let dir = unique_test_dir("restore_preserves_local_slicers");
        let db_path = dir.join("catalog.db");
        let trash_dir = dir.join("trash");
        std::fs::create_dir_all(&trash_dir).unwrap();

        // current_exe() is an existing, executable path on every platform.
        let local_executable = std::env::current_exe().unwrap();
        let conn = crate::db::connect(&db_path).unwrap();
        register_slicer_with_conn(&conn, "Lokaler Slicer".into(), local_executable.to_string_lossy().to_string()).unwrap();
        drop(conn);

        // Backup with a foreign slicer, inserted directly: register_slicer_with_conn
        // would reject the non-existent path, but a manipulated backup never went
        // through that check.
        let backup_db_path = dir.join("incoming.db");
        let backup_conn = crate::db::connect(&backup_db_path).unwrap();
        db::insert_registered_slicer(&backup_conn, "Fremder Slicer", "/tmp/attacker-binary", false).unwrap();
        drop(backup_conn);
        let backup_bytes = std::fs::read(&backup_db_path).unwrap();

        let state = AppState {
            import_jobs: ImportJobs::default(),
            db: std::sync::Mutex::new(crate::db::connect(&db_path).unwrap()),
            trash_dir: trash_dir.clone(),
            db_path: db_path.clone(),
            sensitive_dirs: vec![],
        };
        validate_catalog_db_bytes(&backup_bytes, &state.sensitive_dirs, &state.trash_dir, &[]).unwrap();
        std::fs::write(&backup_db_path, &backup_bytes).unwrap();
        replace_catalog_db(&state, &backup_db_path).unwrap();

        let conn = state.db.lock().unwrap();
        let slicers = db::list_registered_slicers(&conn).unwrap();
        assert!(
            slicers.iter().all(|s| s.executable_path != "/tmp/attacker-binary"),
            "ein aus dem Backup importierter Slicer-Pfad darf niemals in der lokalen Registry landen"
        );
        assert!(
            slicers.iter().any(|s| s.name == "Lokaler Slicer"),
            "der bereits lokal registrierte Slicer darf durch den Restore nicht verloren gehen (Korrektur nach dritter Review-Runde: die vorherige Fassung loeschte registered_slicers unconditional nach jedem Restore und riss dabei auch echte, lokal gueltige Eintraege mit)"
        );
    }
    #[test]
    fn replace_catalog_db_never_activates_an_unsanitized_incoming_database() {
        // If sanitizing fails, the foreign DB must never become active.
        // Fault injection: `registered_slicers` as a VIEW, so DROP TABLE leaves it and
        // CREATE TABLE fails on the name conflict. Deliberately without
        // validate_catalog_db_bytes (which rejects views) to test this barrier alone.
        let dir = unique_test_dir("restore_fail_closed");
        let db_path = dir.join("catalog.db");
        let trash_dir = dir.join("trash");
        std::fs::create_dir_all(&trash_dir).unwrap();

        let local_executable = std::env::current_exe().unwrap();
        let conn = crate::db::connect(&db_path).unwrap();
        register_slicer_with_conn(&conn, "Lokaler Slicer".into(), local_executable.to_string_lossy().to_string()).unwrap();
        drop(conn);

        let backup_db_path = dir.join("incoming.db");
        let backup_conn = rusqlite::Connection::open(&backup_db_path).unwrap();
        backup_conn.execute_batch(crate::db::SCHEMA_SQL).unwrap();
        // Deliberately BEFORE any migration: registered_slicers is NOT a table here
        // but a view with one row that would pass as a "foreign slicer" IF the
        // sanitizing wrongly succeeded.
        backup_conn.execute_batch(
            "CREATE VIEW registered_slicers AS
             SELECT 1 AS id, 'Fremder Slicer' AS name, '/tmp/attacker-binary' AS executable_path, 0 AS is_auto_detected;",
        ).unwrap();
        drop(backup_conn);
        let backup_bytes = std::fs::read(&backup_db_path).unwrap();

        let state = AppState {
            import_jobs: ImportJobs::default(),
            db: std::sync::Mutex::new(crate::db::connect(&db_path).unwrap()),
            trash_dir: trash_dir.clone(),
            db_path: db_path.clone(),
            sensitive_dirs: vec![],
        };
        std::fs::write(&backup_db_path, &backup_bytes).unwrap();

        let result = replace_catalog_db(&state, &backup_db_path);
        assert!(result.is_err(), "replace_catalog_db muss fehlschlagen, wenn die eingehende Datenbank nicht sanierbar ist");

        let conn = state.db.lock().unwrap();
        let slicers = db::list_registered_slicers(&conn).unwrap();
        assert!(
            slicers.iter().any(|s| s.name == "Lokaler Slicer"),
            "die alte/lokale Datenbank muss aktiv bleiben und den lokalen Slicer behalten, wenn die Sanierung fehlschlaegt"
        );
        assert!(
            slicers.iter().all(|s| s.executable_path != "/tmp/attacker-binary"),
            "der fremde Slicer aus der eingehenden Datenbank darf ueber state.db niemals sichtbar werden"
        );
        assert!(db_path.exists(), "die urspruengliche catalog.db darf nicht durch die unsanierte eingehende Datenbank ersetzt worden sein");
    }

    #[test]
    fn duplicate_dtos_preserve_each_full_path_for_cleanup() {
        let mut a = sample_file_record(1, Some("hash"), "2026-01-01");
        let mut b = sample_file_record(2, Some("hash"), "2026-01-02");
        a.path = "/catalog/first/model.stl".into();
        b.path = "/catalog/second/model.stl".into();
        let dtos: Vec<_> = group_duplicates(vec![a, b], &HashSet::new()).remove(0)
            .into_iter().map(|file| to_dto(file, &[])).collect();
        assert_eq!(dtos[0].path, "/catalog/first/model.stl");
        assert_eq!(dtos[1].path, "/catalog/second/model.stl");
    }
    #[test]
    fn group_duplicates_groups_two_matching_hashes_oldest_first() {
        let older = sample_file_record(1, Some("hash-a"), "2026-09-01T00:00:00Z");
        let newer = sample_file_record(2, Some("hash-a"), "2026-09-05T00:00:00Z");
        let files = vec![newer.clone(), older.clone()];

        let groups = group_duplicates(files, &HashSet::new());

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
        assert_eq!(groups[0][0].id, older.id);
        assert_eq!(groups[0][1].id, newer.id);
    }
    #[test]
    fn group_duplicates_excludes_orphaned_member_leaving_no_group() {
        // If one of two same-hash files is missing on disk, no group may form: the
        // missing file could otherwise become the "keep" anchor while the last real
        // copy is pre-checked for deletion.
        let orphaned = sample_file_record(1, Some("hash-a"), "2026-09-01T00:00:00Z");
        let surviving = sample_file_record(2, Some("hash-a"), "2026-09-05T00:00:00Z");
        let files = vec![orphaned.clone(), surviving.clone()];
        let mut orphaned_ids = HashSet::new();
        orphaned_ids.insert(orphaned.id);

        let groups = group_duplicates(files, &orphaned_ids);

        assert!(groups.is_empty());
    }
    #[test]
    fn group_duplicates_ignores_files_without_content_hash() {
        let a = sample_file_record(1, None, "2026-09-01T00:00:00Z");
        let b = sample_file_record(2, None, "2026-09-02T00:00:00Z");

        let groups = group_duplicates(vec![a, b], &HashSet::new());

        assert!(groups.is_empty());
    }
    #[test]
    fn group_duplicates_groups_three_matching_hashes_sorted() {
        let a = sample_file_record(1, Some("hash-a"), "2026-09-03T00:00:00Z");
        let b = sample_file_record(2, Some("hash-a"), "2026-09-01T00:00:00Z");
        let c = sample_file_record(3, Some("hash-a"), "2026-09-02T00:00:00Z");

        let groups = group_duplicates(vec![a, b, c], &HashSet::new());

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 3);
        assert_eq!(
            groups[0].iter().map(|f| f.id).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_folder_path_in_sensitive_directory() {
        // A technically valid backup with `folders.path` in a protected directory must be rejected.
        let tmp_path = std::env::temp_dir().join(format!(
            "validate_catalog_db_sensitive_test_{}.db",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        // A real directory: otherwise only the checked path gets resolved (macOS:
        // /tmp -> /private/tmp) and the prefix comparison fails because of the test setup.
        let sensitive_root = unique_test_dir("3mf-test-sensitive-root");
        {
            let conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
            db::insert_folder_with_parent(&conn, "evil", None, &sensitive_root.join("evil").to_string_lossy())
                .expect("insert malicious folder row");
            drop(conn);
        }
        let bytes = std::fs::read(&tmp_path).expect("read temp db");

        let result = validate_catalog_db_bytes(&bytes, std::slice::from_ref(&sensitive_root), &std::env::temp_dir(), &[]);

        let _ = std::fs::remove_file(&tmp_path);
        let _ = std::fs::remove_dir_all(&sensitive_root);
        assert!(result.is_err(), "must reject an imported catalog whose folder path lies in a sensitive directory");
    }
    /// Valid catalog DB with exactly one adjusted `files` row, as raw bytes.
    fn catalog_db_bytes_with_file_row(
        name: &str,
        path: &str,
        trash_path: Option<&str>,
    ) -> Vec<u8> {
        let tmp_path = std::env::temp_dir().join(format!(
            "validate_catalog_db_files_test_{}.db",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        {
            let mut conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
            let new_file = sample_new_file_for_rescan_test(std::path::Path::new(path));
            let id = db::insert_file(&mut conn, &new_file).expect("insert file row");
            conn.execute(
                "UPDATE files SET name = ?1, path = ?2, trash_path = ?3 WHERE id = ?4",
                rusqlite::params![name, path, trash_path, id],
            )
            .expect("patch file row");
            drop(conn);
        }
        let bytes = std::fs::read(&tmp_path).expect("read temp db");
        let _ = std::fs::remove_file(&tmp_path);
        bytes
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_harmless_file_rows() {
        let dir = unique_test_dir("validate_catalog_db_files_ok");
        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&dir.join("1-modell.3mf").to_string_lossy()),
        );
        let result = validate_catalog_db_bytes(&bytes, &[std::path::PathBuf::from("/etc")], &dir, &[]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "a legitimate backup must still import: {result:?}");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_file_name_with_path_traversal() {
        // `files.name` ends up in `trash_dir.join(...)`; "../" would write outside.
        let dir = unique_test_dir("validate_catalog_db_files_name");
        let bytes = catalog_db_bytes_with_file_row(
            "../../../.config/autostart/evil.desktop",
            &dir.join("modell.3mf").to_string_lossy(),
            None,
        );
        let result = validate_catalog_db_bytes(&bytes, &[], &dir, &[]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_err(), "must reject an imported file row whose name escapes the trash dir");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_file_path_in_sensitive_directory() {
        // Created as a real directory, not just a path string - see
        // `..._rejects_folder_path_in_sensitive_directory`.
        let sensitive_root = unique_test_dir("3mf-test-sensitive-file-path");
        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &sensitive_root.join("modell.3mf").to_string_lossy(),
            None,
        );
        let result = validate_catalog_db_bytes(&bytes, std::slice::from_ref(&sensitive_root), &std::env::temp_dir(), &[]);
        let _ = std::fs::remove_dir_all(&sensitive_root);
        assert!(result.is_err(), "must reject an imported file row pointing into a sensitive directory");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_trash_path_outside_the_real_trash_dir() {
        // `~/Documents/...` is on no denylist, but `purge_expired_trash_on_startup`
        // would still have deleted the file on the next start.
        let dir = unique_test_dir("validate_catalog_db_trash_containment");
        let trash_dir = dir.join("trash");
        std::fs::create_dir_all(&trash_dir).expect("create trash dir");
        let victim = dir.join("irgendwas-wichtiges.pdf");
        std::fs::write(&victim, b"wichtig").expect("create victim file");

        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&victim.to_string_lossy()),
        );
        let result = validate_catalog_db_bytes(&bytes, &[], &trash_dir, &[]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            result.is_err(),
            "must reject a trash_path outside the app's real trash directory: {result:?}"
        );
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_trash_path_inside_the_real_trash_dir() {
        // Counter-check: a genuine backup of this app must stay importable.
        let dir = unique_test_dir("validate_catalog_db_trash_containment_ok");
        let trash_dir = dir.join("trash");
        std::fs::create_dir_all(&trash_dir).expect("create trash dir");

        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&trash_dir.join("1-modell.3mf").to_string_lossy()),
        );
        let result = validate_catalog_db_bytes(&bytes, &[], &trash_dir, &[]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "a legitimate backup must still import: {result:?}");
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_trash_path_under_sensitive_ancestor() {
        let dir = unique_test_dir("validate_catalog_db_trash_sensitive_ancestor_ok");
        let sensitive_root = dir.join("sensitive");
        let trash_dir = sensitive_root.join("app/trash");
        std::fs::create_dir_all(&trash_dir).expect("create trash dir");

        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&trash_dir.join("1-modell.3mf").to_string_lossy()),
        );
        let result = validate_catalog_db_bytes(&bytes, &[sensitive_root], &trash_dir, &[]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "a legitimate backup must import with a sensitive trash ancestor: {result:?}");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_trash_path_outside_trash_under_sensitive_ancestor() {
        let dir = unique_test_dir("validate_catalog_db_trash_sensitive_ancestor_outside");
        let sensitive_root = dir.join("sensitive");
        let trash_dir = sensitive_root.join("app/trash");
        std::fs::create_dir_all(&trash_dir).expect("create trash dir");

        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&sensitive_root.join("other/x.conf").to_string_lossy()),
        );
        let result = validate_catalog_db_bytes(&bytes, &[sensitive_root], &trash_dir, &[]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_err(), "must reject a trash_path outside trash even under a sensitive ancestor: {result:?}");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_trash_path_escaping_to_sensitive_ancestor() {
        let dir = unique_test_dir("validate_catalog_db_trash_sensitive_ancestor_dotdot");
        let sensitive_root = dir.join("sensitive");
        let trash_dir = sensitive_root.join("app/trash");
        std::fs::create_dir_all(&trash_dir).expect("create trash dir");

        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&trash_dir.join("../../x.conf").to_string_lossy()),
        );
        let result = validate_catalog_db_bytes(&bytes, &[sensitive_root], &trash_dir, &[]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_err(), "must reject a trash_path escaping to a sensitive ancestor via parent components: {result:?}");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_trash_path_escaping_the_trash_dir_via_parent_components() {
        // `<trash_dir>/../victim.pdf` passes the raw prefix comparison; the path is
        // therefore resolved first.
        let dir = unique_test_dir("validate_catalog_db_trash_containment_dotdot");
        let trash_dir = dir.join("trash");
        std::fs::create_dir_all(&trash_dir).expect("create trash dir");

        let escaping = format!("{}/../opfer.pdf", trash_dir.to_string_lossy());
        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&escaping),
        );
        let result = validate_catalog_db_bytes(&bytes, &[], &trash_dir, &[]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            result.is_err(),
            "must reject a trash_path that escapes the trash dir via \"..\": {result:?}"
        );
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_trash_path_in_sensitive_directory() {
        // `files.trash_path` goes straight to `fs::remove_file` - among others in
        // `purge_expired_trash_on_startup`, which runs at startup without any user
        // interaction.
        let dir = unique_test_dir("validate_catalog_db_trash_path");
        // Created as a real directory, not just a path string - see
        // `..._rejects_folder_path_in_sensitive_directory`.
        let sensitive_root = unique_test_dir("3mf-test-sensitive-trash-path");
        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&sensitive_root.join("wichtig.conf").to_string_lossy()),
        );
        // trash_dir deliberately set to temp_dir: the crafted trash_path lies inside,
        // so the containment check does NOT apply - this test still covers exactly
        // the denylist.
        let result = validate_catalog_db_bytes(&bytes, std::slice::from_ref(&sensitive_root), &std::env::temp_dir(), &[]);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&sensitive_root);
        assert!(result.is_err(), "must reject an imported trash_path pointing into a sensitive directory");
    }

    fn zip_bytes_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default();
            for (name, contents) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(contents).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn read_catalog_archive_rejects_a_file_that_is_not_a_zip_at_all() {
        let err = read_catalog_archive(std::io::Cursor::new(b"not a zip file".to_vec())).unwrap_err();
        assert!(err.expected, "a non-ZIP file is the user's file, not a fault");
        assert!(err.is(ErrorCode::BackupNotZip, &[]), "{err:?}");
    }

    #[test]
    fn read_catalog_archive_rejects_a_zip_without_catalog_db() {
        let buf = zip_bytes_with(&[("settings.json", b"{}")]);
        let err = read_catalog_archive(std::io::Cursor::new(buf)).unwrap_err();
        assert!(err.expected, "a ZIP missing catalog.db is the user's file, not a fault");
        assert!(err.is(ErrorCode::BackupMissingEntry, &["catalog.db"]), "{err:?}");
    }

    #[test]
    fn read_catalog_archive_rejects_a_zip_without_settings_json() {
        let buf = zip_bytes_with(&[("catalog.db", b"fake db bytes")]);
        let err = read_catalog_archive(std::io::Cursor::new(buf)).unwrap_err();
        assert!(err.expected, "a ZIP missing settings.json is the user's file, not a fault");
        assert!(err.is(ErrorCode::BackupMissingEntry, &["settings.json"]), "{err:?}");
    }

    /// Fixture written with the previous ZIP library and its default options,
    /// exactly like `write_zip_atomically` produced it.
    #[test]
    fn read_catalog_archive_reads_a_backup_written_by_the_previous_library() {
        let bytes = include_bytes!("testdata/backup_zip2.zip").to_vec();
        let (db_bytes, settings_json) = read_catalog_archive(std::io::Cursor::new(bytes)).expect("legacy backup readable");
        assert!(db_bytes.starts_with(b"SQLite format 3\0"));
        assert_eq!(settings_json, r#"{"language":"de","theme":"dark"}"#);
    }

    #[test]
    fn read_catalog_archive_returns_both_entries_when_present() {
        let buf = zip_bytes_with(&[("catalog.db", b"fake db bytes"), ("settings.json", br#"{"a":1}"#)]);
        let (db_bytes, settings_json) = read_catalog_archive(std::io::Cursor::new(buf)).expect("both entries present");
        assert_eq!(db_bytes, b"fake db bytes");
        assert_eq!(settings_json, r#"{"a":1}"#);
    }

    #[test]
    fn validate_catalog_db_bytes_accepts_an_old_backup_without_the_kind_column() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
            conn.execute_batch(
                "DROP INDEX IF EXISTS idx_filament_spools_slot;
                 DROP TABLE filament_spools;
                 CREATE TABLE filament_spools (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    material TEXT NOT NULL, manufacturer TEXT, color TEXT, location TEXT,
                    diameter_mm REAL NOT NULL, original_weight_g INTEGER NOT NULL,
                    remaining_weight_g INTEGER NOT NULL, price REAL, image_png BLOB,
                    created_at TEXT NOT NULL,
                    unit_id INTEGER REFERENCES material_units(id) ON DELETE SET NULL,
                    slot_index INTEGER, home_location TEXT, color_hex TEXT
                 );
                 INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                    VALUES ('PLA', 1.75, 1000, 600, '2026-09-01');",
            )
            .unwrap();
            // Before step 36 (catch-up step for `kind`); step 37 (resin printers) came
            // later and runs after it.
            assert!(version >= 36);
            conn.pragma_update(None, "user_version", 35).unwrap();
        });
        let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
        assert!(result.is_ok(), "aeltere Sicherung ohne kind muss gehen: {result:?}");
    }

    /// A v0.13.1 backup (user_version 32) has `kind` but no printer connection
    /// tables. Checking and restoring only use `run_migrations`, so steps 33-35 must
    /// create them themselves.
    #[test]
    fn a_v0131_backup_without_printer_link_tables_validates_and_restores() {
        const V0131_SCHEMA_VERSION: i64 = 32;
        let dir = unique_test_dir("restore_v0131_backup");
        let incoming_path = dir.join("incoming_catalog.db");
        {
            let conn = crate::db::connect(&incoming_path).expect("connect creates schema");
            conn.execute_batch(
                "DROP TABLE printer_jobs;
                 DROP TABLE printer_connections;
                 DROP TABLE app_settings;
                 INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, kind)
                    VALUES ('Standard', 1.75, 1000, 640.5, '2026-09-25', 'resin');",
            )
            .unwrap();
            conn.pragma_update(None, "user_version", V0131_SCHEMA_VERSION).unwrap();
        }
        let table_exists = |conn: &Connection, name: &str| -> bool {
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [name],
                |r| r.get(0),
            )
            .unwrap()
        };
        {
            let conn = Connection::open(&incoming_path).unwrap();
            assert!(!table_exists(&conn, "app_settings"), "Vorbedingung: v0.13.1-Form ohne app_settings");
        }
        let bytes = std::fs::read(&incoming_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &dir.join("trash"), &[]);
        assert!(result.is_ok(), "v0.13.1-Sicherung muss die Pruefung bestehen: {result:?}");

        let db_path = dir.join("catalog.db");
        let state = AppState {
            import_jobs: ImportJobs::default(),
            db: Mutex::new(crate::db::connect(&db_path).expect("connect running db")),
            trash_dir: dir.join("trash"),
            db_path: db_path.clone(),
            sensitive_dirs: Vec::new(),
        };
        let restored = replace_catalog_db(&state, &incoming_path);
        assert!(restored.is_ok(), "v0.13.1-Sicherung muss sich wiederherstellen lassen: {restored:?}");

        // The installed file itself (not just the connection opened via `init`) then
        // has all tables and the current version.
        let installed = Connection::open(&db_path).unwrap();
        for table in ["app_settings", "printer_connections", "printer_jobs"] {
            assert!(table_exists(&installed, table), "{table} fehlt nach der Wiederherstellung");
        }
        let version: i64 = installed.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        let current: i64 = crate::db::connect_in_memory()
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, current);
        drop(installed);
        let guard = state.db.lock().unwrap();
        let kind: String = guard.query_row("SELECT kind FROM filament_spools", [], |r| r.get(0)).unwrap();
        assert_eq!(kind, "resin", "die Resin-Flasche aus der Sicherung bleibt Resin");
        drop(guard);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_catalog_db_bytes_rejects_resin_in_a_slot() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            conn.execute_batch(
                "INSERT INTO printers (name) VALUES ('X1C');
                 INSERT INTO material_units (printer_id, name, kind, slot_count) VALUES (1, 'AMS', 'bambu_ams', 4);
                 INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, kind, unit_id, slot_index)
                    VALUES ('Standard', 1.75, 1000, 500, '2026-09-25', 'resin', 1, 0);",
            )
            .unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
    }

    #[test]
    fn validate_catalog_db_bytes_rejects_an_unknown_kind_without_check_constraint() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            conn.execute_batch(
                "DROP INDEX IF EXISTS idx_filament_spools_slot;
                 DROP TABLE filament_spools;
                 CREATE TABLE filament_spools (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    material TEXT NOT NULL, manufacturer TEXT, color TEXT, location TEXT,
                    diameter_mm REAL NOT NULL, original_weight_g INTEGER NOT NULL,
                    remaining_weight_g INTEGER NOT NULL, price REAL, image_png BLOB,
                    created_at TEXT NOT NULL, unit_id INTEGER, slot_index INTEGER,
                    home_location TEXT, color_hex TEXT, kind TEXT
                 );
                 INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, kind)
                    VALUES ('PLA', 1.75, 1000, 600, '2026-09-01', 'pla');",
            )
            .unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
    }

    // ---- Resin printers ----

    fn resin_bottle(conn: &Connection) -> i64 {
        crate::db::insert_filament_spool(
            conn,
            &crate::db::models::NewFilamentSpool {
                nozzle_temp_min: None,
                nozzle_temp_max: None,
                bed_temp_min: None,
                bed_temp_max: None,
                material: "Standard".to_string(),
                manufacturer: None,
                color: Some("Grau".to_string()),
                location: Some("Resin-Schrank".to_string()),
                diameter_mm: 1.75,
                original_weight_g: 1000.0,
                remaining_weight_g: 620.0,
                price: None,
                image_png: None,
                color_hex: None,
                color_hexes: Vec::new(),
                kind: "resin".to_string(),
            },
        )
        .unwrap()
    }

    fn saturn_with_vat(conn: &Connection) -> (i64, i64) {
        let printer = crate::db::printers::insert_printer_of_kind(conn, "Saturn 4", "resin").unwrap();
        let vat = crate::db::printers::insert_resin_vat(conn, printer, "Harzwanne").unwrap();
        (printer, vat)
    }

    #[test]
    fn validate_catalog_db_bytes_accepts_a_resin_printer_with_a_bottle_in_its_vat() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let (_, vat) = saturn_with_vat(conn);
            let bottle = resin_bottle(conn);
            crate::db::printers::load_spool(conn, bottle, vat, 0).unwrap();
        });
        let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
        assert!(result.is_ok(), "gueltiger Resin-Drucker muss durchgehen: {result:?}");
    }

    /// Backup from v0.13.x/early v0.14.0 (user_version 36): `printers` without
    /// `kind`, `material_units` with the old CHECK, one spool in a slot. Must be
    /// migrated and accepted by step 37; the table rebuild must not throw the spool
    /// out of its slot.
    #[test]
    fn validate_catalog_db_bytes_accepts_an_old_backup_without_printer_kind_and_keeps_loaded_spools() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            conn.execute_batch(
                "DROP INDEX IF EXISTS idx_filament_spools_slot;
                 DROP TABLE printer_jobs;
                 DROP TABLE printer_connections;
                 DROP TABLE material_units;
                 DROP TABLE printers;
                 CREATE TABLE printers (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    position INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE material_units (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    printer_id INTEGER NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL CHECK (kind IN ('bambu_ams', 'bambu_ams_lite', 'bambu_ams_ht', 'creality_cfs',
                                                       'prusa_mmu3', 'anycubic_ace', 'external', 'custom')),
                    slot_count INTEGER NOT NULL CHECK (slot_count BETWEEN 1 AND 16),
                    bambu_ams_index INTEGER CHECK (bambu_ams_index BETWEEN 0 AND 3),
                    position INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE UNIQUE INDEX idx_filament_spools_slot ON filament_spools (unit_id, slot_index) WHERE unit_id IS NOT NULL;
                 CREATE TABLE printer_connections (
                    printer_id INTEGER PRIMARY KEY REFERENCES printers(id) ON DELETE CASCADE,
                    kind TEXT NOT NULL CHECK (kind IN ('moonraker')),
                    address TEXT NOT NULL, base_url TEXT, remote_version TEXT,
                    connected_since REAL NOT NULL, last_synced_at REAL, last_error TEXT, error_since REAL,
                    paused INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0, 1))
                 );
                 CREATE TABLE printer_jobs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    printer_id INTEGER NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
                    remote_id TEXT NOT NULL, file_name TEXT NOT NULL,
                    outcome TEXT NOT NULL CHECK (outcome IN ('completed', 'partial')),
                    raw_status TEXT NOT NULL, ended_at REAL NOT NULL, print_duration_s REAL NOT NULL,
                    used_mm REAL NOT NULL, slicer_total_mm REAL, slicer_weight_g REAL, material TEXT,
                    thumbnail_path TEXT,
                    state TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'confirmed', 'ignored')),
                    booked_spool_id INTEGER REFERENCES filament_spools(id) ON DELETE SET NULL,
                    booked_file_id INTEGER REFERENCES files(id) ON DELETE SET NULL,
                    booked_g REAL, decided_at TEXT,
                    UNIQUE (printer_id, remote_id)
                 );
                 INSERT INTO printers (id, name, position) VALUES (1, 'X1C', 0);
                 INSERT INTO material_units (id, printer_id, name, kind, slot_count, bambu_ams_index, position)
                     VALUES (10, 1, 'AMS A', 'bambu_ams', 4, 0, 0);
                 INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index, home_location)
                     VALUES ('PLA', 1.75, 1000, 600, '2026-09-01', 10, 1, 'Regal 2');
                 INSERT INTO printer_connections (printer_id, kind, address, connected_since) VALUES (1, 'moonraker', '192.168.1.60', 1.0);",
            )
            .unwrap();
            conn.pragma_update(None, "user_version", 36).unwrap();
        });
        let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
        assert!(result.is_ok(), "alte Sicherung ohne printers.kind muss gehen: {result:?}");

        // And the restore (same path: run_migrations on the copy) keeps the spool in
        // its slot.
        let path = unique_test_db_path("old_backup_restore_resin_step");
        std::fs::write(&path, &bytes).unwrap();
        let mut conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        crate::db::run_migrations(&mut conn).unwrap();
        let (kind, unit_id): (String, Option<i64>) = conn
            .query_row(
                "SELECT p.kind, s.unit_id FROM printers p, filament_spools s WHERE p.id = 1 AND s.material = 'PLA'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((kind.as_str(), unit_id), ("filament", Some(10)));
        drop(conn);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn validate_catalog_db_bytes_rejects_a_resin_bottle_in_a_filament_unit() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let (_, unit_id) = printer_with_one_ams_unit(conn);
            let bottle = resin_bottle(conn);
            conn.execute("UPDATE filament_spools SET unit_id = ?1, slot_index = 0 WHERE id = ?2", params![unit_id, bottle])
                .unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
    }

    #[test]
    fn validate_catalog_db_bytes_rejects_a_filament_spool_in_a_vat() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let (_, vat) = saturn_with_vat(conn);
            let spool = crate::db::insert_filament_spool(
                conn,
                &crate::db::models::NewFilamentSpool { kind: "filament".to_string(), material: "PLA".to_string(), ..resin_new() },
            )
            .unwrap();
            conn.execute("UPDATE filament_spools SET unit_id = ?1, slot_index = 0 WHERE id = ?2", params![vat, spool])
                .unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
    }

    fn resin_new() -> crate::db::models::NewFilamentSpool {
        crate::db::models::NewFilamentSpool {
            nozzle_temp_min: None,
            nozzle_temp_max: None,
            bed_temp_min: None,
            bed_temp_max: None,
            material: "Standard".to_string(),
            manufacturer: None,
            color: None,
            location: Some("Regal".to_string()),
            diameter_mm: 1.75,
            original_weight_g: 1000.0,
            remaining_weight_g: 500.0,
            price: None,
            image_png: None,
            color_hex: None,
            color_hexes: Vec::new(),
            kind: "resin".to_string(),
        }
    }

    #[test]
    fn validate_catalog_db_bytes_rejects_a_vat_on_a_filament_printer() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let (printer, _) = printer_with_one_ams_unit(conn);
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, position) VALUES (?1, 'Wanne', 'resin_vat', 1, 1)",
                params![printer],
            )
            .unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
    }

    #[test]
    fn validate_catalog_db_bytes_rejects_other_units_or_a_second_vat_on_a_resin_printer() {
        for extra in ["'AMS', 'bambu_ams', 4", "'Wanne 2', 'resin_vat', 1"] {
            let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
                let (printer, _) = saturn_with_vat(conn);
                conn.execute(
                    &format!("INSERT INTO material_units (printer_id, name, kind, slot_count, position) VALUES (?1, {extra}, 1)"),
                    params![printer],
                )
                .unwrap();
            });
            assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err(), "{extra}");
        }
    }

    #[test]
    fn validate_catalog_db_bytes_rejects_a_vat_with_more_than_one_place_or_an_unknown_printer_kind() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let (_, vat) = saturn_with_vat(conn);
            conn.execute("UPDATE material_units SET slot_count = 2 WHERE id = ?1", params![vat]).unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err(), "Wanne mit 2 Plaetzen");

        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            conn.execute("PRAGMA ignore_check_constraints = ON", []).unwrap();
            conn.execute("INSERT INTO printers (name, kind, position) VALUES ('X', 'toast', 0)", []).unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err(), "unbekannte Druckerart");
    }

    #[test]
    fn validate_catalog_db_bytes_rejects_a_connection_for_a_resin_printer() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let (printer, _) = saturn_with_vat(conn);
            conn.execute(
                "INSERT INTO printer_connections (printer_id, kind, address, connected_since) VALUES (?1, 'moonraker', '192.168.1.60', 1.0)",
                params![printer],
            )
            .unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
    }
}

#[cfg(test)]
mod companion_tests {
    use super::*;

    fn state_with_catalog(name: &str) -> (AppState, PathBuf) {
        let dir = unique_test_dir(name);
        let db_path = dir.join("catalog.db");
        let state = AppState {
            import_jobs: ImportJobs::default(),
            db: Mutex::new(db::connect(&db_path).unwrap()),
            trash_dir: dir.join("trash"),
            db_path,
            sensitive_dirs: Vec::new(),
        };
        (state, dir)
    }

    /// The bytes of a catalog database after `setup` ran on it, plus the trash folder to validate against.
    fn catalog_bytes(setup: impl FnOnce(&Connection)) -> (Vec<u8>, PathBuf) {
        let dir = unique_test_dir("backup_companion_bytes");
        let path = dir.join("catalog.db");
        {
            let conn = crate::db::connect(&path).unwrap();
            setup(&conn);
        }
        (std::fs::read(&path).unwrap(), dir.join("trash"))
    }

    fn with_companion(conn: &Connection, name: &str, path: &str, kind: &str, trash_path: Option<&str>) {
        // A crafted backup need not carry the CHECK constraints.
        conn.pragma_update(None, "ignore_check_constraints", true).unwrap();
        let id = crate::db::test_insert_minimal_file(conn, "/catalog/Foo.3mf", None).unwrap();
        conn.execute(
            "INSERT INTO companion_files (file_id, name, path, kind, added_at, deleted_at, trash_path) VALUES (?1, ?2, ?3, ?4, 'now', ?5, ?6)",
            rusqlite::params![id, name, path, kind, trash_path.map(|_| "2026-01-01T00:00:00+00:00"), trash_path],
        )
        .unwrap();
    }

    fn validate(bytes: &[u8], trash: &Path) -> CmdResult<()> {
        validate_catalog_db_bytes(bytes, &[], trash, &[])
    }

    #[test]
    fn restore_uses_only_locally_known_catalog_roots_for_folder_trash() {
        let (source, dir) = state_with_catalog("backup_folder_trash_source");
        let root = dir.join("models");
        std::fs::create_dir_all(root.join(".katalog-papierkorb")).unwrap();
        let trash_path = root.join(".katalog-papierkorb/1-0123abcd-model.3mf");
        {
            let conn = source.db.lock().unwrap();
            let id = db::test_insert_minimal_file(&conn, root.join("model.3mf").to_str().unwrap(), None).unwrap();
            assert_eq!(id, 1);
            conn.execute("UPDATE files SET deleted_at = 't', trash_path = ?1 WHERE id = 1", [trash_path.to_str().unwrap()]).unwrap();
            // A folder in the incoming backup cannot authorize its own trash path.
            conn.execute("INSERT INTO folders (name, path) VALUES ('models', ?1)", [root.to_str().unwrap()]).unwrap();
        }
        let incoming = dir.join("incoming.db");
        std::fs::copy(&source.db_path, &incoming).unwrap();
        let bytes = std::fs::read(&incoming).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_folder_trash_target");
        assert!(validate_backup_for_import(&bytes, &[], &target.trash_dir, &[]).is_err());
        assert!(replace_catalog_db(&target, &incoming).is_err());
        {
            let conn = target.db.lock().unwrap();
            assert!(db::list_files(&conn).unwrap().is_empty());
            conn.execute("INSERT INTO folders (name, path) VALUES ('models', ?1)", [root.to_str().unwrap()]).unwrap();
        }
        let known = vec![root.to_string_lossy().into_owned()];
        validate_backup_for_import(&bytes, &[], &target.trash_dir, &known).unwrap();
        replace_catalog_db(&target, &incoming).unwrap();
        let conn = target.db.lock().unwrap();
        let restored: String = conn.query_row("SELECT trash_path FROM files WHERE id = 1", [], |row| row.get(0)).unwrap();
        assert_eq!(restored, trash_path.to_string_lossy());
    }

    #[test]
    fn catalog_lock_survives_replace_catalog_db() {
        use crate::db::instance_lock::{InstanceLock, LockError};
        let (target, target_dir) = state_with_catalog("backup_lock_target");
        let (source, source_dir) = state_with_catalog("backup_lock_source");
        let lock = InstanceLock::acquire(&target.db_path, "desktop").unwrap();
        let sidecar = InstanceLock::path_for(&target.db_path).unwrap();
        let metadata = std::fs::read_to_string(&sidecar).unwrap();
        let incoming = source.db_path.clone();
        drop(source);
        replace_catalog_db(&target, &incoming).unwrap();
        assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), metadata);
        assert!(matches!(InstanceLock::acquire(&target.db_path, "server"), Err(LockError::InUse)));
        drop(lock);
        drop(InstanceLock::acquire(&target.db_path, "server").unwrap());
        drop(target);
        std::fs::remove_dir_all(target_dir).unwrap();
        std::fs::remove_dir_all(source_dir).unwrap();
    }

    #[test]
    fn companions_and_source_folders_survive_export_and_restore() {
        let (source, dir) = state_with_catalog("backup_companion_roundtrip_a");
        {
            let conn = source.db.lock().unwrap();
            with_companion(&conn, "pic.png", "/catalog/Foo_extras/pic.png", "image", None);
            conn.execute("INSERT INTO source_folders (path, added_at) VALUES ('/downloads', 't')", []).unwrap();
            conn.execute("INSERT INTO source_rejected (path, size_bytes, rejected_at) VALUES ('/downloads/x.stl', 12, 't')", []).unwrap();
            crate::db::printer_link::set_setting(&conn, "source_check_on_start", "1").unwrap();
        }
        let zip_path = dir.join("backup.zip");
        write_catalog_backup(&source.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        validate(&db_bytes, &dir).expect("a current export must validate");
        let incoming = dir.join("incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();

        let (target, _target_dir) = state_with_catalog("backup_companion_roundtrip_b");
        replace_catalog_db(&target, &incoming).unwrap();

        let conn = target.db.lock().unwrap();
        let restored: (String, String, String) = conn
            .query_row("SELECT name, path, kind FROM companion_files", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!(restored, ("pic.png".into(), "/catalog/Foo_extras/pic.png".into(), "image".into()));
        let count = |table: &str| conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!((count("source_folders"), count("source_rejected")), (1, 1));
        assert!(crate::db::sources::source_check_on_start(&conn).unwrap());
    }

    #[test]
    fn a_backup_from_before_the_new_tables_can_be_restored() {
        let (old, dir) = state_with_catalog("backup_companion_old_a");
        {
            let conn = old.db.lock().unwrap();
            let id = crate::db::test_insert_minimal_file(&conn, "/catalog/old.3mf", None).unwrap();
            conn.execute("UPDATE files SET creator = 'Jane' WHERE id = ?1", [id]).unwrap();
            conn.execute_batch("DROP TABLE companion_files; DROP TABLE source_folders; DROP TABLE source_rejected").unwrap();
            conn.pragma_update(None, "user_version", crate::db::SOURCE_COLUMNS_MIGRATION_VERSION).unwrap();
        }
        let zip_path = dir.join("old-backup.zip");
        write_catalog_backup(&old.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let probe = dir.join("probe.db");
        std::fs::write(&probe, &db_bytes).unwrap();
        assert!(Connection::open(&probe).unwrap().prepare("SELECT * FROM companion_files").is_err(), "precondition: the backup has no such table");

        validate(&db_bytes, &dir).expect("an older backup must validate");
        let incoming = dir.join("old-incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_companion_old_b");
        replace_catalog_db(&target, &incoming).unwrap();

        let conn = target.db.lock().unwrap();
        for table in ["companion_files", "source_folders", "source_rejected"] {
            let rows: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(rows, 0, "{table} exists and is empty");
        }
        assert_eq!(db::list_files(&conn).unwrap()[0].creator.as_deref(), Some("Jane"));
    }

    #[test]
    fn a_well_formed_companion_row_is_accepted() {
        let (bytes, trash) = catalog_bytes(|conn| with_companion(conn, "pic.png", "/catalog/Foo_extras/pic.png", "image", None));
        assert!(validate(&bytes, &trash).is_ok());
    }

    #[test]
    fn crafted_companion_rows_are_rejected() {
        let cases: [(&str, &str, &str, &str); 7] = [
            ("not an _extras folder", "secret.png", "/home/user/.ssh/secret.png", "image"),
            ("name does not match the path", "other.png", "/catalog/Foo_extras/pic.png", "image"),
            ("a kind that does not exist", "pic.png", "/catalog/Foo_extras/pic.png", "exe"),
            ("kind and extension disagree", "pic.png", "/catalog/Foo_extras/pic.png", "pdf"),
            ("an executable as an image", "tool.exe", "/catalog/Foo_extras/tool.exe", "image"),
            ("a parent reference", "pic.png", "/catalog/Foo_extras/../pic.png", "image"),
            ("a relative path", "pic.png", "Foo_extras/pic.png", "image"),
        ];
        for (why, name, path, kind) in cases {
            let (bytes, trash) = catalog_bytes(|conn| with_companion(conn, name, path, kind, None));
            assert!(validate(&bytes, &trash).is_err(), "{why}");
        }
    }

    #[test]
    fn a_companion_row_in_a_protected_folder_is_rejected() {
        let (bytes, trash) = catalog_bytes(|conn| with_companion(conn, "a.png", "/home/user/.config/autostart_extras/a.png", "image", None));
        assert!(validate_catalog_db_bytes(&bytes, &[PathBuf::from("/home/user/.config")], &trash, &[]).is_err());
        assert!(validate(&bytes, &trash).is_ok(), "counter-check: allowed when nothing is protected there");
    }

    #[test]
    fn a_companion_trash_path_must_lie_inside_the_trash_folder() {
        let (bytes, trash) = catalog_bytes(|conn| with_companion(conn, "pic.png", "/catalog/Foo_extras/pic.png", "image", Some("/home/user/Documents/precious.png")));
        assert!(validate(&bytes, &trash).is_err(), "a restored backup must not make the app delete a document");
        let inside = trash.join("companion-1-pic.png");
        let (bytes, trash) = catalog_bytes(|conn| with_companion(conn, "pic.png", "/catalog/Foo_extras/pic.png", "image", Some(&inside.to_string_lossy())));
        let _ = trash;
        assert!(validate(&bytes, inside.parent().unwrap()).is_ok(), "counter-check: inside the trash it is fine");
    }

    #[test]
    fn blocked_auto_tags_survive_export_and_restore() {
        let (source, dir) = state_with_catalog("backup_blocked_tags_a");
        {
            let conn = source.db.lock().unwrap();
            db::blocked_tags::block_auto_tag(&conn, "Miniature").unwrap();
            db::blocked_tags::block_auto_tag(&conn, "kabelhalter").unwrap();
        }
        let zip_path = dir.join("backup.zip");
        write_catalog_backup(&source.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        validate(&db_bytes, &dir).expect("a current export must validate");
        let incoming = dir.join("incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();

        let (target, _target_dir) = state_with_catalog("backup_blocked_tags_b");
        replace_catalog_db(&target, &incoming).unwrap();

        let conn = target.db.lock().unwrap();
        assert_eq!(db::blocked_tags::list_blocked_auto_tags(&conn).unwrap(), vec!["kabelhalter", "miniatur"]);
    }

    #[test]
    fn a_backup_from_before_the_blocked_tags_table_can_be_restored() {
        let (old, dir) = state_with_catalog("backup_blocked_tags_old_a");
        {
            let conn = old.db.lock().unwrap();
            let id = crate::db::test_insert_minimal_file(&conn, "/catalog/old.3mf", None).unwrap();
            conn.execute("UPDATE files SET creator = 'Jane' WHERE id = ?1", [id]).unwrap();
            conn.execute_batch("DROP TABLE blocked_auto_tags").unwrap();
            conn.pragma_update(None, "user_version", crate::db::BLOCKED_AUTO_TAGS_MIGRATION_VERSION - 1).unwrap();
        }
        let zip_path = dir.join("old-backup.zip");
        write_catalog_backup(&old.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let probe = dir.join("probe.db");
        std::fs::write(&probe, &db_bytes).unwrap();
        assert!(Connection::open(&probe).unwrap().prepare("SELECT * FROM blocked_auto_tags").is_err(), "precondition: the backup has no such table");

        validate(&db_bytes, &dir).expect("an older backup must validate");
        let incoming = dir.join("old-incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_blocked_tags_old_b");
        replace_catalog_db(&target, &incoming).unwrap();

        let conn = target.db.lock().unwrap();
        assert!(db::blocked_tags::list_blocked_auto_tags(&conn).unwrap().is_empty());
        assert_eq!(db::list_files(&conn).unwrap()[0].creator.as_deref(), Some("Jane"));
    }

    #[test]
    fn crafted_blocked_tag_rows_are_rejected() {
        let long = "x".repeat(101);
        for (why, name) in [("too long", long.as_str()), ("control character", "a\u{7}b"), ("empty", "  ")] {
            let (bytes, trash) = catalog_bytes(|conn| {
                conn.execute("INSERT INTO blocked_auto_tags (name, blocked_at) VALUES (?1, 't')", [name]).unwrap();
            });
            assert!(validate(&bytes, &trash).is_err(), "{why}");
        }
        let (bytes, trash) = catalog_bytes(|conn| {
            conn.execute("INSERT INTO blocked_auto_tags (name, blocked_at) VALUES ('miniatur', 't')", []).unwrap();
        });
        assert!(validate(&bytes, &trash).is_ok(), "counter-check: a normal entry passes");
    }

    #[test]
    fn a_backup_with_too_many_blocked_tags_is_rejected() {
        let limit = crate::db::blocked_tags::MAX_BLOCKED_TAGS;
        for (rows, accepted) in [(limit, true), (limit + 1, false)] {
            let (bytes, trash) = catalog_bytes(|conn| {
                // One transaction: a commit per row makes this test take a minute.
                conn.execute_batch("BEGIN").unwrap();
                for i in 0..rows {
                    conn.execute("INSERT INTO blocked_auto_tags (name, blocked_at) VALUES (?1, 't')", [format!("tag{i}")]).unwrap();
                }
                conn.execute_batch("COMMIT").unwrap();
            });
            assert_eq!(validate(&bytes, &trash).is_ok(), accepted, "{rows} rows");
        }
    }

    #[test]
    fn wrong_storage_classes_in_blocked_auto_tags_are_rejected() {
        let (bytes, trash) = catalog_bytes(|conn| {
            conn.execute("INSERT INTO blocked_auto_tags (name, blocked_at) VALUES (X'1234', 't')", []).unwrap();
        });
        assert!(validate(&bytes, &trash).is_err());
    }

    #[test]
    fn wrong_storage_classes_in_the_new_tables_are_rejected() {
        let (bytes, trash) = catalog_bytes(|conn| {
            conn.execute("INSERT INTO source_rejected (path, size_bytes, rejected_at) VALUES ('/d/x.stl', 'big', 't')", []).unwrap();
        });
        assert!(validate(&bytes, &trash).is_err());
        let (bytes, trash) = catalog_bytes(|conn| {
            conn.execute("INSERT INTO source_folders (path, added_at) VALUES (X'1234', 't')", []).unwrap();
        });
        assert!(validate(&bytes, &trash).is_err());
    }

    const COPY_HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn insert_known_copy(conn: &Connection, path: &str, size: i64, hash: &str) {
        conn.execute(
            "INSERT INTO disk_sync_known_copies (path, size_bytes, content_hash, noted_at) VALUES (?1, ?2, ?3, 't')",
            rusqlite::params![path, size, hash],
        )
        .unwrap();
    }

    #[test]
    fn remembered_copies_survive_export_and_restore() {
        let (source, dir) = state_with_catalog("backup_known_copies_a");
        {
            let conn = source.db.lock().unwrap();
            db::known_copies::note_known_copy(&conn, "/models/copy.3mf", 42, COPY_HASH, "2026-11-01T10:00:00Z").unwrap();
        }
        let zip_path = dir.join("backup.zip");
        write_catalog_backup(&source.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        validate(&db_bytes, &dir).expect("a current export must validate");
        let incoming = dir.join("incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();

        let (target, _target_dir) = state_with_catalog("backup_known_copies_b");
        replace_catalog_db(&target, &incoming).unwrap();

        let conn = target.db.lock().unwrap();
        let rows = db::known_copies::list_known_copies(&conn).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].path.as_str(), rows[0].size_bytes, rows[0].content_hash.as_str()), ("/models/copy.3mf", 42, COPY_HASH));
    }

    #[test]
    fn a_backup_from_before_the_known_copies_table_can_be_restored() {
        let (old, dir) = state_with_catalog("backup_known_copies_old_a");
        {
            let conn = old.db.lock().unwrap();
            let id = crate::db::test_insert_minimal_file(&conn, "/catalog/old.3mf", None).unwrap();
            conn.execute("UPDATE files SET creator = 'Jane' WHERE id = ?1", [id]).unwrap();
            conn.execute_batch("DROP TABLE disk_sync_known_copies").unwrap();
            conn.pragma_update(None, "user_version", 54).unwrap();
        }
        let zip_path = dir.join("old-backup.zip");
        write_catalog_backup(&old.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let probe = dir.join("probe.db");
        std::fs::write(&probe, &db_bytes).unwrap();
        assert!(Connection::open(&probe).unwrap().prepare("SELECT * FROM disk_sync_known_copies").is_err(), "precondition: the backup has no such table");

        validate(&db_bytes, &dir).expect("an older backup must validate");
        let incoming = dir.join("old-incoming.db");
        std::fs::write(&incoming, &db_bytes).unwrap();
        let (target, _target_dir) = state_with_catalog("backup_known_copies_old_b");
        replace_catalog_db(&target, &incoming).unwrap();

        let conn = target.db.lock().unwrap();
        assert_eq!(db::known_copies::count_known_copies(&conn).unwrap(), 0, "the table exists and is empty");
        assert_eq!(db::list_files(&conn).unwrap()[0].creator.as_deref(), Some("Jane"));
    }

    #[test]
    fn crafted_known_copy_rows_are_rejected() {
        let long = format!("/{}", "x".repeat(4100));
        let cases: [(&str, &str, i64, &str); 8] = [
            ("parent component", "/models/../etc/x.3mf", 1, COPY_HASH),
            ("control character", "/models/a\u{7}.3mf", 1, COPY_HASH),
            ("relative path", "models/a.3mf", 1, COPY_HASH),
            ("negative size", "/models/a.3mf", -1, COPY_HASH),
            ("short hash", "/models/a.3mf", 1, "abcd"),
            ("non-hex hash", "/models/a.3mf", 1, "zz23456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
            ("empty path", "", 1, COPY_HASH),
            ("endless path", long.as_str(), 1, COPY_HASH),
        ];
        for (why, path, size, hash) in cases {
            let (bytes, trash) = catalog_bytes(|conn| insert_known_copy(conn, path, size, hash));
            assert!(validate(&bytes, &trash).is_err(), "{why}");
        }
        let (bytes, trash) = catalog_bytes(|conn| insert_known_copy(conn, "/models/a.3mf", 1, COPY_HASH));
        assert!(validate(&bytes, &trash).is_ok(), "counter-check: a normal row passes");
    }

    #[test]
    fn a_known_copy_in_a_protected_location_is_rejected() {
        let (bytes, trash) = catalog_bytes(|conn| insert_known_copy(conn, "/protected/dir/a.3mf", 1, COPY_HASH));
        let sensitive = [PathBuf::from("/protected")];
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
        assert!(validate_catalog_db_bytes(&bytes, &[], &trash, &[]).is_ok(), "counter-check: without the protection the row is fine");
    }

    #[test]
    fn a_backup_with_too_many_known_copies_is_rejected() {
        let limit = crate::db::known_copies::MAX_KNOWN_COPIES;
        for (rows, accepted) in [(limit, true), (limit + 1, false)] {
            let (bytes, trash) = catalog_bytes(|conn| {
                conn.execute_batch("BEGIN").unwrap();
                for i in 0..rows {
                    insert_known_copy(conn, &format!("/models/{i}.3mf"), 1, COPY_HASH);
                }
                conn.execute_batch("COMMIT").unwrap();
            });
            assert_eq!(validate(&bytes, &trash).is_ok(), accepted, "{rows} rows");
        }
    }

    #[test]
    fn wrong_storage_classes_in_known_copies_are_rejected() {
        let (bytes, trash) = catalog_bytes(|conn| {
            conn.execute("INSERT INTO disk_sync_known_copies (path, size_bytes, content_hash, noted_at) VALUES ('/m/a.3mf', 'big', ?1, 't')", [COPY_HASH]).unwrap();
        });
        assert!(validate(&bytes, &trash).is_err());
        let (bytes, trash) = catalog_bytes(|conn| {
            conn.execute("INSERT INTO disk_sync_known_copies (path, size_bytes, content_hash, noted_at) VALUES (X'1234', 1, ?1, 't')", [COPY_HASH]).unwrap();
        });
        assert!(validate(&bytes, &trash).is_err());
    }

    const TV_KEY: &str = "TVKEY-0123456789-abcdefghijkl";

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    fn set_thingiverse(state: &AppState) {
        let conn = state.db.lock().unwrap();
        db::thingiverse::set_enabled(&conn, true).unwrap();
        db::thingiverse::set_token(&conn, &katalog_core::thingiverse::Token::parse(TV_KEY).unwrap()).unwrap();
    }

    #[test]
    fn a_user_backup_holds_the_switch_but_never_the_api_key() {
        let (source, dir) = state_with_catalog("backup_thingiverse_key_a");
        set_thingiverse(&source);
        let zip_path = dir.join("backup.zip");
        write_catalog_backup(&source.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        assert!(!contains(&db_bytes, TV_KEY.as_bytes()), "the key is in the bytes of the backup database");
        assert!(!contains(&std::fs::read(&zip_path).unwrap(), TV_KEY.as_bytes()));
        let probe = dir.join("probe.db");
        std::fs::write(&probe, &db_bytes).unwrap();
        let copy = Connection::open(&probe).unwrap();
        assert!(db::thingiverse::get_token(&copy).unwrap().is_none());
        assert!(db::thingiverse::get_enabled(&copy).unwrap(), "the switch is not a secret");
        // The working database keeps its key.
        let conn = source.db.lock().unwrap();
        assert!(db::thingiverse::get_token(&conn).unwrap().is_some());
    }

    #[test]
    fn a_key_inside_a_restored_database_never_takes_effect() {
        let (source, dir) = state_with_catalog("backup_thingiverse_key_b");
        set_thingiverse(&source);
        // A file somebody handed over: the working database as it is, key included.
        let incoming = dir.join("incoming.db");
        std::fs::copy(&source.db_path, &incoming).unwrap();
        assert!(contains(&std::fs::read(&incoming).unwrap(), TV_KEY.as_bytes()), "precondition: the incoming file holds the key");

        let (target, target_dir) = state_with_catalog("backup_thingiverse_key_c");
        replace_catalog_db(&target, &incoming).unwrap();
        {
            let conn = target.db.lock().unwrap();
            assert!(db::thingiverse::get_token(&conn).unwrap().is_none());
            assert!(db::thingiverse::get_enabled(&conn).unwrap(), "the switch comes along");
        }
        assert!(!contains(&std::fs::read(target_dir.join("catalog.db")).unwrap(), TV_KEY.as_bytes()), "the key is still in the restored file");
    }

    const TV_KEY_LOCAL: &str = "LOCALKEY-9876543210-zyxwvutsrq";

    #[test]
    fn a_restore_keeps_the_key_of_this_computer_and_never_takes_the_one_from_the_file() {
        // A user backup of another computer: switch on, no key (the backup never holds it).
        let (source, dir) = state_with_catalog("backup_thingiverse_key_d");
        {
            let conn = source.db.lock().unwrap();
            db::thingiverse::set_enabled(&conn, true).unwrap();
        }
        let incoming = dir.join("incoming.db");
        std::fs::copy(&source.db_path, &incoming).unwrap();

        // This computer: its own key, switch ON.
        let (target, target_dir) = state_with_catalog("backup_thingiverse_key_e");
        {
            let conn = target.db.lock().unwrap();
            db::thingiverse::set_enabled(&conn, true).unwrap();
            db::thingiverse::set_token(&conn, &katalog_core::thingiverse::Token::parse(TV_KEY_LOCAL).unwrap()).unwrap();
        }
        replace_catalog_db(&target, &incoming).unwrap();
        {
            let conn = target.db.lock().unwrap();
            assert!(db::thingiverse::get_token(&conn).unwrap().is_some(), "the local key survives the restore");
            assert!(db::thingiverse::get_enabled(&conn).unwrap(), "the switch survives the restore");
        }
        assert!(contains(&std::fs::read(target_dir.join("catalog.db")).unwrap(), TV_KEY_LOCAL.as_bytes()), "the key is in the restored working file");

        // The same with a file that carries a foreign key: the local one wins, the foreign one is gone.
        set_thingiverse(&source);
        let foreign = dir.join("foreign.db");
        std::fs::copy(&source.db_path, &foreign).unwrap();
        assert!(contains(&std::fs::read(&foreign).unwrap(), TV_KEY.as_bytes()), "precondition: the incoming file holds a foreign key");
        replace_catalog_db(&target, &foreign).unwrap();
        {
            let conn = target.db.lock().unwrap();
            assert!(db::thingiverse::get_token(&conn).unwrap().is_some());
        }
        let bytes = std::fs::read(target_dir.join("catalog.db")).unwrap();
        assert!(contains(&bytes, TV_KEY_LOCAL.as_bytes()), "the local key, not the foreign one, is in the restored file");
        assert!(!contains(&bytes, TV_KEY.as_bytes()), "the foreign key is still in the restored file");

        // The next backup of this computer still does not carry the key.
        let zip_path = dir.join("after.zip");
        write_catalog_backup(&target.db, &zip_path, "{}").unwrap();
        assert!(!contains(&std::fs::read(&zip_path).unwrap(), TV_KEY_LOCAL.as_bytes()));
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        assert!(!contains(&db_bytes, TV_KEY_LOCAL.as_bytes()));
    }

    const PRINTER_KEY: &str = "PRKEY0123456789abcdef0123456789";

    fn set_printer_key(state: &AppState) {
        let conn = state.db.lock().unwrap();
        conn.execute("INSERT INTO printers (id, name, position) VALUES (1, 'Sovol SV08', 0)", []).unwrap();
        db::printer_link::save_connection_after_test(
            &conn, 1, "moonraker", "192.168.1.60",
            Some(&katalog_core::printer_link::api_key::ApiKey::parse(PRINTER_KEY).unwrap()), "http://192.168.1.60", "v0.8.0", 1000.0,
        )
        .unwrap();
    }

    #[test]
    fn a_user_backup_keeps_the_printer_connection_but_never_its_api_key() {
        let (source, dir) = state_with_catalog("backup_printer_key_a");
        set_printer_key(&source);
        assert!(contains(&std::fs::read(&source.db_path).unwrap(), PRINTER_KEY.as_bytes()), "precondition: the working file holds the key");
        let zip_path = dir.join("backup.zip");
        write_catalog_backup(&source.db, &zip_path, "{}").unwrap();
        let (db_bytes, _) = read_catalog_archive(std::fs::File::open(&zip_path).unwrap()).unwrap();
        assert!(!contains(&db_bytes, PRINTER_KEY.as_bytes()), "the key is in the bytes of the backup database");
        assert!(!contains(&std::fs::read(&zip_path).unwrap(), PRINTER_KEY.as_bytes()));
        let probe = dir.join("probe.db");
        std::fs::write(&probe, &db_bytes).unwrap();
        let copy = Connection::open(&probe).unwrap();
        let connection = db::printer_link::get_connection(&copy, 1).unwrap().unwrap();
        assert_eq!(connection.address, "192.168.1.60", "the connection itself is in the backup");
        assert!(!connection.has_api_key);
        // The working database keeps its key.
        let conn = source.db.lock().unwrap();
        assert!(db::printer_link::get_api_key(&conn, 1).unwrap().is_some());
    }

    #[test]
    fn a_printer_key_inside_a_restored_database_never_takes_effect() {
        let (source, dir) = state_with_catalog("backup_printer_key_b");
        set_printer_key(&source);
        let incoming = dir.join("incoming.db");
        std::fs::copy(&source.db_path, &incoming).unwrap();
        assert!(contains(&std::fs::read(&incoming).unwrap(), PRINTER_KEY.as_bytes()), "precondition: the incoming file holds the key");

        let (target, target_dir) = state_with_catalog("backup_printer_key_c");
        replace_catalog_db(&target, &incoming).unwrap();
        {
            let conn = target.db.lock().unwrap();
            let connection = db::printer_link::get_connection(&conn, 1).unwrap().unwrap();
            assert!(!connection.has_api_key);
            assert!(connection.paused, "a restored connection waits for a new test");
        }
        assert!(!contains(&std::fs::read(target_dir.join("catalog.db")).unwrap(), PRINTER_KEY.as_bytes()), "the key is still in the restored file");
    }
}
