use super::runtime_environment::{DesktopFeature, RuntimeEnvironment};
use super::*;
use crate::db::models::RegisteredSlicer;

/// The frontend's view of a `registered_slicers` entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlicerDto {
    pub id: String,
    pub name: String,
    pub executable_path: String,
}
fn slicer_dto(r: &RegisteredSlicer) -> SlicerDto {
    SlicerDto { id: r.id.to_string(), name: r.name.clone(), executable_path: r.executable_path.clone() }
}
/// True when both paths name the same file: identical text, or both resolve
/// (symlinks, `..`) to the same target. A stored path that no longer resolves
/// (program removed) can only match literally, which is all that is needed to
/// avoid the UNIQUE collision.
fn same_executable(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}
/// Shared by manual registration and the auto-scan, so both treat the same
/// program reached through different paths as one entry.
fn find_registered_by_path<'a>(registered: &'a [RegisteredSlicer], path: &str) -> Option<&'a RegisteredSlicer> {
    registered.iter().find(|r| same_executable(&r.executable_path, path))
}
/// Core logic of the registration, without a dialog, so it's testable.
pub(crate) fn register_slicer_with_conn(conn: &Connection, name: String, executable_path: String) -> CmdResult<SlicerDto> {
    validate_slicer_path(&executable_path)?;
    // Choosing a program that is already registered (directly, or through a symlink
    // or another path to the same file) is not an error: hand back the existing
    // entry unchanged instead of tripping the UNIQUE constraint.
    let existing = db::list_registered_slicers(conn).map_err(|e| e.to_string())?;
    if let Some(found) = find_registered_by_path(&existing, &executable_path) {
        return Ok(slicer_dto(found));
    }
    let id = db::insert_registered_slicer(conn, &name, &executable_path, false).map_err(|e| e.to_string())?;
    Ok(SlicerDto { id: id.to_string(), name, executable_path })
}
/// Opens the file dialog in the backend: the path only comes from the user's
/// pick, never as a string from the frontend. async, because a synchronous
/// command runs on the thread GTK needs for the dialog (deadlock).
#[tauri::command]
pub async fn pick_and_register_slicer<R: tauri::Runtime>(app: tauri::AppHandle<R>, state: State<'_, AppState>, environment: State<'_, RuntimeEnvironment>) -> CmdResult<Option<SlicerDto>> {
    environment.require_desktop(DesktopFeature::Slicer)?;
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
pub fn list_registered_slicers(state: State<AppState>, environment: State<'_, RuntimeEnvironment>) -> CmdResult<Vec<SlicerDto>> {
    environment.require_desktop(DesktopFeature::Slicer)?;
    let conn = lock_db(&state)?;
    db::list_registered_slicers(&conn)
        .map_err(|e| e.to_string().into())
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
    // A missing slicer executable (uninstalled, moved) is a normal situation for the
    // user to fix, not an application fault.
    let metadata = std::fs::metadata(path).map_err(|_| CmdError::expected("slicer executable not found"))?;
    if !metadata.is_file() {
        return Err(CmdError::expected("slicer path is not a file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(CmdError::expected("slicer path is not executable"));
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
            return Err(CmdError::expected("slicer path must be an .exe file"));
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
        return Err("model file path must not start with '-'".into());
    }
    if !is_sliceable_extension(path) {
        return Err("model file must be a .3mf or .stl file".into());
    }
    if !std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
        // The model file was deleted or moved outside the app - a normal situation,
        // not an application fault.
        return Err(CmdError::expected("model file not found"));
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
    super::external_env::sanitize_external_command(&mut cmd);
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
pub fn open_in_slicer(state: State<AppState>, file_id: String, slicer_id: String, environment: State<'_, RuntimeEnvironment>) -> CmdResult<()> {
    environment.require_desktop(DesktopFeature::Slicer)?;
    let conn = lock_db(&state)?;
    open_in_slicer_with_conn(&conn, &file_id, &slicer_id)
}
/// Runs the auto-detection and adds new slicers (`is_auto_detected`) to the
/// registry; known paths are skipped (UNIQUE). Returns the complete registry.
#[tauri::command]
pub fn scan_installed_slicers(state: State<AppState>, environment: State<'_, RuntimeEnvironment>) -> CmdResult<Vec<SlicerDto>> {
    environment.require_desktop(DesktopFeature::Slicer)?;
    let conn = lock_db(&state)?;
    let existing = db::list_registered_slicers(&conn).map_err(|e| e.to_string())?;
    let mut known = existing;
    for detected in detect_slicers() {
        if find_registered_by_path(&known, &detected.path).is_some() {
            continue;
        }
        // Best effort: a single failing insert (e.g. a race with a parallel scan) must
        // not abort detecting the other slicers.
        if let Ok(id) = db::insert_registered_slicer(&conn, &detected.name, &detected.path, true) {
            // Also guards against two detected paths that resolve to the same file.
            known.push(RegisteredSlicer { id, name: detected.name.clone(), executable_path: detected.path.clone(), is_auto_detected: true });
        }
    }
    db::list_registered_slicers(&conn)
        .map_err(|e| e.to_string().into())
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
        let err = validate_slicer_path("/does/not/exist/slicer").unwrap_err();
        // A slicer that's been uninstalled/moved is normal feedback, not a fault.
        assert!(err.expected, "a missing slicer executable must be an expected error");
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
    fn registering_the_same_file_twice_is_idempotent() {
        let conn = db::connect_in_memory().unwrap();
        let exe = std::env::current_exe().unwrap().to_string_lossy().to_string();
        let first = register_slicer_with_conn(&conn, "Orca".into(), exe.clone()).unwrap();
        let second = register_slicer_with_conn(&conn, "Other name".into(), exe).unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.name, "Orca", "the existing entry must stay unchanged");
        assert_eq!(db::list_registered_slicers(&conn).unwrap().len(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn registering_a_symlink_to_a_registered_file_returns_the_existing_entry() {
        let dir = unique_test_dir("register_slicer_symlink");
        let link = dir.join("orca-slicer");
        std::os::unix::fs::symlink(std::env::current_exe().unwrap(), &link).unwrap();
        let conn = db::connect_in_memory().unwrap();
        let real = register_slicer_with_conn(&conn, "Orca".into(), std::env::current_exe().unwrap().to_string_lossy().to_string()).unwrap();
        let via_link = register_slicer_with_conn(&conn, "orca-slicer".into(), link.to_string_lossy().to_string());
        let rows = db::list_registered_slicers(&conn).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(via_link.unwrap().id, real.id);
        assert_eq!(rows.len(), 1);
    }
    #[test]
    fn registering_two_different_files_creates_two_entries() {
        let dir = unique_test_dir("register_slicer_two_files");
        let other = dir.join("other-slicer");
        std::fs::copy(std::env::current_exe().unwrap(), &other).unwrap();
        let conn = db::connect_in_memory().unwrap();
        let a = register_slicer_with_conn(&conn, "A".into(), std::env::current_exe().unwrap().to_string_lossy().to_string()).unwrap();
        let b = register_slicer_with_conn(&conn, "B".into(), other.to_string_lossy().to_string()).unwrap();
        let rows = db::list_registered_slicers(&conn).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_ne!(a.id, b.id);
        assert_eq!(rows.len(), 2);
    }
    #[cfg(unix)]
    #[test]
    fn find_registered_by_path_matches_canonically_equal_paths_and_literal_missing_ones() {
        let dir = unique_test_dir("find_registered_by_path");
        let real = dir.join("real");
        std::fs::write(&real, b"x").unwrap();
        let link = dir.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let registered = vec![
            RegisteredSlicer { id: 1, name: "Real".into(), executable_path: real.to_string_lossy().to_string(), is_auto_detected: true },
            RegisteredSlicer { id: 2, name: "Gone".into(), executable_path: "/does/not/exist/slicer".into(), is_auto_detected: false },
        ];
        let via_link = find_registered_by_path(&registered, &link.to_string_lossy()).map(|r| r.id);
        let gone = find_registered_by_path(&registered, "/does/not/exist/slicer").map(|r| r.id);
        let unknown = find_registered_by_path(&registered, "/does/not/exist/else").map(|r| r.id);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(via_link, Some(1));
        assert_eq!(gone, Some(2));
        assert_eq!(unknown, None);
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
        assert!(
            bad_missing.as_ref().unwrap_err().expected,
            "a model file deleted outside the app is normal feedback, not a fault"
        );
        assert!(bad_dir.is_err(), "a directory must be rejected");
        // .stp can be cataloged, but not launched in a slicer.
        assert!(bad_stp.is_err(), "a .stp file must not be launchable in a slicer");
    }
}
