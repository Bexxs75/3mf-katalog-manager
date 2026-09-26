use super::*;

// On startup the AppImage exports various environment variables meant only for
// its own runtime; they must not be passed on to independently started external
// programs (slicers).
const APPIMAGE_ENV_VARS_TO_STRIP: &[&str] = &[
    "APPDIR",
    "APPIMAGE",
    "OWD",
    "ARGV0",
    "LD_LIBRARY_PATH",
    "GTK_EXE_PREFIX",
    "GTK_DATA_PREFIX",
    "GTK_THEME",
    "GTK_PATH",
    "GTK_IM_MODULE_FILE",
    "GDK_PIXBUF_MODULE_FILE",
    "GDK_BACKEND",
    "GIO_EXTRA_MODULES",
    "GSETTINGS_SCHEMA_DIR",
    "XDG_DATA_DIRS",
    "PYTHONPATH",
    "QT_PLUGIN_PATH",
    "GST_PLUGIN_SYSTEM_PATH",
    "WEBKIT_DISABLE_DMABUF_RENDERER",
];

/// The frontend's view of a `registered_slicers` entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlicerDto {
    pub id: String,
    pub name: String,
    pub executable_path: String,
}
/// Core logic of the registration, without a dialog, so it's testable.
pub(crate) fn register_slicer_with_conn(conn: &Connection, name: String, executable_path: String) -> CmdResult<SlicerDto> {
    validate_slicer_path(&executable_path)?;
    let id = db::insert_registered_slicer(conn, &name, &executable_path, false).map_err(|e| e.to_string())?;
    Ok(SlicerDto { id: id.to_string(), name, executable_path })
}
/// Opens the file dialog in the backend: the path only comes from the user's
/// pick, never as a string from the frontend. async, because a synchronous
/// command runs on the thread GTK needs for the dialog (deadlock).
#[tauri::command]
pub async fn pick_and_register_slicer(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult<Option<SlicerDto>> {
    let dialog = app.dialog().file();
    #[cfg(target_os = "windows")]
    let dialog = dialog.add_filter("Programme", &["exe"]);
    let picked = dialog.blocking_pick_file();
    let Some(picked) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let name = picked
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Slicer")
        .to_string();
    let executable_path = picked.to_string_lossy().to_string();
    let conn = lock_db(&state)?;
    register_slicer_with_conn(&conn, name, executable_path).map(Some)
}
#[tauri::command]
pub fn list_registered_slicers(state: State<AppState>) -> CmdResult<Vec<SlicerDto>> {
    let conn = lock_db(&state)?;
    db::list_registered_slicers(&conn)
        .map_err(|e| e.to_string())
        .map(|rows| {
            rows.into_iter()
                .map(|r| SlicerDto { id: r.id.to_string(), name: r.name, executable_path: r.executable_path })
                .collect()
        })
}
// `open_in_slicer` only starts registered slicers (`slicer_id`), never a free
// path. This check runs on registration and again on every start: the path must
// (still) point to an existing executable file.
fn validate_slicer_path(slicer_path: &str) -> CmdResult<()> {
    let path = Path::new(slicer_path);
    let metadata = std::fs::metadata(path)
        .map_err(|_| "slicer executable not found".to_string())?;
    if !metadata.is_file() {
        return Err("slicer path is not a file".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("slicer path is not executable".to_string());
        }
    }
    #[cfg(windows)]
    {
        let is_exe = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("exe"))
            .unwrap_or(false);
        if !is_exe {
            return Err("slicer path must be an .exe file".to_string());
        }
    }
    Ok(())
}
/// Checks the model argument for the slicer: a leading "-" would be a CLI flag,
/// and only sliceable files are allowed.
fn validate_slicer_target_file(file_path: &str) -> CmdResult<()> {
    let path = Path::new(file_path);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "invalid model file path".to_string())?;
    if file_path.starts_with('-') || file_name.starts_with('-') {
        return Err("model file path must not start with '-'".to_string());
    }
    if !is_sliceable_extension(path) {
        return Err("model file must be a .3mf or .stl file".to_string());
    }
    if !std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
        return Err("model file not found".to_string());
    }
    Ok(())
}
/// Result of the registry/security resolution: both paths are already checked
/// by `validate_slicer_path`/`validate_slicer_target_file`.
struct ResolvedSlicerLaunch {
    executable_path: String,
    model_path: String,
}
/// Resolution and checks without starting a process, so it's testable without side effects.
fn resolve_registered_slicer_and_model(
    conn: &Connection,
    file_id: &str,
    slicer_id: &str,
) -> CmdResult<ResolvedSlicerLaunch> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let sid: i64 = slicer_id.parse().map_err(|_| "invalid slicer id".to_string())?;
    let file = db::get_file(conn, id).map_err(|e| e.to_string())?.ok_or_else(|| "file not found".to_string())?;
    let slicer = db::get_registered_slicer(conn, sid)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "slicer not registered".to_string())?;
    validate_slicer_path(&slicer.executable_path)?;
    validate_slicer_target_file(&file.path)?;
    Ok(ResolvedSlicerLaunch { executable_path: slicer.executable_path, model_path: file.path })
}
/// The real process start, deliberately not unit-tested.
fn launch_slicer(resolved: &ResolvedSlicerLaunch) -> CmdResult<()> {
    let mut cmd = std::process::Command::new(&resolved.executable_path);
    cmd.arg(&resolved.model_path);
    for var in APPIMAGE_ENV_VARS_TO_STRIP {
        cmd.env_remove(var);
    }
    if let Some(parent) = std::path::Path::new(&resolved.executable_path).parent() {
        cmd.current_dir(parent);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}
fn open_in_slicer_with_conn(conn: &Connection, file_id: &str, slicer_id: &str) -> CmdResult<()> {
    let resolved = resolve_registered_slicer_and_model(conn, file_id, slicer_id)?;
    launch_slicer(&resolved)
}
#[tauri::command]
pub fn open_in_slicer(state: State<AppState>, file_id: String, slicer_id: String) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    open_in_slicer_with_conn(&conn, &file_id, &slicer_id)
}
/// Runs the auto-detection and adds new slicers (`is_auto_detected`) to the
/// registry; known paths are skipped (UNIQUE). Returns the complete registry.
#[tauri::command]
pub fn scan_installed_slicers(state: State<AppState>) -> CmdResult<Vec<SlicerDto>> {
    let conn = lock_db(&state)?;
    let existing = db::list_registered_slicers(&conn).map_err(|e| e.to_string())?;
    let known_paths: HashSet<String> = existing.iter().map(|s| s.executable_path.clone()).collect();
    for detected in detect_slicers() {
        if known_paths.contains(&detected.path) {
            continue;
        }
        // Best effort: a single failing insert (e.g. a race with a parallel scan) must
        // not abort detecting the other slicers.
        let _ = db::insert_registered_slicer(&conn, &detected.name, &detected.path, true);
    }
    db::list_registered_slicers(&conn)
        .map_err(|e| e.to_string())
        .map(|rows| {
            rows.into_iter()
                .map(|r| SlicerDto { id: r.id.to_string(), name: r.name, executable_path: r.executable_path })
                .collect()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_slicer_path_rejects_missing_file() {
        assert!(validate_slicer_path("/does/not/exist/slicer").is_err());
    }
    #[test]
    fn validate_slicer_path_rejects_directory() {
        let dir = std::env::temp_dir();
        assert!(validate_slicer_path(dir.to_str().unwrap()).is_err());
    }
    #[test]
    fn open_in_slicer_rejects_an_unregistered_slicer_id() {
        let dir = unique_test_dir("open_in_slicer_rejects");
        std::fs::create_dir_all(&dir).unwrap();
        let model_path = dir.join("model.3mf");
        std::fs::write(&model_path, b"CONTENT").unwrap();

        let conn = db::connect_in_memory().unwrap();
        let file_id = db::test_insert_minimal_file(&conn, &model_path.to_string_lossy(), None).unwrap();
        let result = resolve_registered_slicer_and_model(&conn, &file_id.to_string(), "not-a-registered-id");
        assert!(result.is_err());
    }
    #[test]
    fn a_registered_slicer_resolves_to_a_validated_executable_and_model_path() {
        // Only the resolution, without starting a process.
        let dir = unique_test_dir("open_in_slicer_resolve");
        std::fs::create_dir_all(&dir).unwrap();
        let model_path = dir.join("model.3mf");
        std::fs::write(&model_path, b"CONTENT").unwrap();

        let conn = db::connect_in_memory().unwrap();
        let file_id = db::test_insert_minimal_file(&conn, &model_path.to_string_lossy(), None).unwrap();
        // current_exe() exists and is executable, on Windows too (/bin/true isn't).
        let fake_slicer = std::env::current_exe().unwrap();
        let slicer = register_slicer_with_conn(&conn, "Test Slicer".into(), fake_slicer.to_string_lossy().to_string()).unwrap();

        let resolved = resolve_registered_slicer_and_model(&conn, &file_id.to_string(), &slicer.id).unwrap();

        assert_eq!(resolved.executable_path, fake_slicer.to_string_lossy());
        assert_eq!(resolved.model_path, model_path.to_string_lossy());
    }
    #[test]
    fn validate_slicer_target_file_rejects_flags_unsupported_types_and_missing_files() {
        let dir = unique_test_dir("validate_slicer_target");
        let model = dir.join("modell.3mf");
        std::fs::write(&model, b"x").unwrap();
        let stl = dir.join("modell.STL");
        std::fs::write(&stl, b"x").unwrap();
        let other = dir.join("notiz.txt");
        std::fs::write(&other, b"x").unwrap();
        let flag = dir.join("--export-gcode.3mf");
        std::fs::write(&flag, b"x").unwrap();
        let stp = dir.join("modell.stp");
        std::fs::write(&stp, b"x").unwrap();

        let ok_3mf = validate_slicer_target_file(&model.to_string_lossy());
        let ok_stl = validate_slicer_target_file(&stl.to_string_lossy());
        let bad_ext = validate_slicer_target_file(&other.to_string_lossy());
        let bad_flag = validate_slicer_target_file(&flag.to_string_lossy());
        let bad_bare_flag = validate_slicer_target_file("--version");
        let bad_missing = validate_slicer_target_file(&dir.join("weg.3mf").to_string_lossy());
        let bad_dir = validate_slicer_target_file(&dir.to_string_lossy());
        let bad_stp = validate_slicer_target_file(&stp.to_string_lossy());

        let _ = std::fs::remove_dir_all(&dir);
        assert!(ok_3mf.is_ok(), "a real .3mf file must still open: {ok_3mf:?}");
        assert!(ok_stl.is_ok(), "extension check must be case-insensitive: {ok_stl:?}");
        assert!(bad_ext.is_err(), "unsupported extension must be rejected");
        assert!(bad_flag.is_err(), "a file name starting with '-' must be rejected");
        assert!(bad_bare_flag.is_err(), "a bare CLI flag must be rejected");
        assert!(bad_missing.is_err(), "a non-existent file must be rejected");
        assert!(bad_dir.is_err(), "a directory must be rejected");
        // .stp can be cataloged, but not launched in a slicer.
        assert!(bad_stp.is_err(), "a .stp file must not be launchable in a slicer");
    }
}
