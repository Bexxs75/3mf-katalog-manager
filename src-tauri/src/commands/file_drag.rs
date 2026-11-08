//! Experimental native dragging uses private temporary copies, never catalog originals.
//! File IDs are resolved in the backend; only 3MF, STL and OBJ are offered.

use super::runtime_environment::{DesktopFeature, RuntimeEnvironment};
use super::*;

use katalog_core::error_code::ErrorCode;

pub(crate) const SETTING_DRAG_OUT_ENABLED: &str = "drag_out_enabled";
/// More files per drag are refused (a selection of thousands must not be copied).
const MAX_DRAG_FILES: usize = 64;
/// Keep originals private until the native copy-mode probe passes on every platform.
const DRAG_ORIGINAL_FILE: bool = false;
const DRAG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
#[derive(Default, Clone)]
pub struct NativeDragState(std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>);
impl NativeDragState {
    pub fn is_active(&self) -> bool {
        let mut active = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if active.is_some_and(|start| start.elapsed() >= DRAG_TIMEOUT) { *active = None; }
        active.is_some()
    }
    fn begin(&self) -> CmdResult<std::time::Instant> {
        let mut active = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if active.is_some_and(|start| start.elapsed() < DRAG_TIMEOUT) {
            return Err(CmdError::coded_with(ErrorCode::DragFailed, [String::new()]));
        }
        let token = std::time::Instant::now();
        *active = Some(token);
        Ok(token)
    }
    fn finish(&self, token: std::time::Instant, notify: impl FnOnce()) {
        let mut active = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if *active == Some(token) {
            *active = None;
            notify();
        }
    }
}
/// Preview image of the drag (the app icon, 32 px).
const DRAG_PREVIEW_PNG: &[u8] = include_bytes!("../../icons/32x32.png");

fn is_draggable(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "3mf" | "stl" | "obj"))
}

pub(crate) fn drag_out_enabled(conn: &Connection) -> CmdResult<bool> {
    Ok(db::printer_link::get_setting(conn, SETTING_DRAG_OUT_ENABLED).map_err(|e| e.to_string())?.as_deref() == Some("1"))
}

/// Pure check: ids from the database, only draggable types, files must exist.
fn resolve_drag_files(conn: &Connection, file_ids: &[String]) -> CmdResult<Vec<PathBuf>> {
    if file_ids.is_empty() || file_ids.len() > MAX_DRAG_FILES {
        return Err(CmdError::coded(ErrorCode::ReqInvalidArguments));
    }
    let mut paths = Vec::with_capacity(file_ids.len());
    for raw in file_ids {
        let id: i64 = raw.parse().map_err(|_| CmdError::coded(ErrorCode::InvalidCatalogId))?;
        let file = db::get_file(conn, id).map_err(|e| e.to_string())?
            .ok_or_else(|| CmdError::coded(ErrorCode::ModelNotFoundOrTrashed))?;
        if file.deleted_at.is_some() {
            return Err(CmdError::coded(ErrorCode::ModelNotFoundOrTrashed));
        }
        let path = PathBuf::from(file.current_path());
        if !is_draggable(&path) {
            return Err(CmdError::coded(ErrorCode::DragNotSliceable));
        }
        if !std::fs::metadata(&path).map(|m| m.is_file()).unwrap_or(false) {
            return Err(CmdError::coded(ErrorCode::DragFileMissing));
        }
        paths.push(path);
    }
    Ok(paths)
}

/// Copies each file into its own numbered subfolder under `root` (same name, no collisions)
/// and returns absolute paths. With `original` the sources are returned unchanged.
fn stage_drag_files(sources: &[PathBuf], root: &Path, original: bool) -> CmdResult<Vec<PathBuf>> {
    if original {
        return sources.iter().map(|p| std::fs::canonicalize(p).map_err(|e| CmdError::coded_with(ErrorCode::DragFailed, [e.to_string()]))).collect();
    }
    let mut staged = Vec::with_capacity(sources.len());
    for (index, source) in sources.iter().enumerate() {
        let folder = root.join(index.to_string());
        let name = source.file_name().ok_or_else(|| CmdError::coded(ErrorCode::DragFileMissing))?;
        let target = folder.join(name);
        std::fs::create_dir_all(&folder)
            .and_then(|_| std::fs::copy(source, &target))
            .map_err(|e| CmdError::coded_with(ErrorCode::DragFailed, [e.to_string()]))?;
        staged.push(std::fs::canonicalize(&target).unwrap_or(target));
    }
    Ok(staged)
}

#[tauri::command]
pub fn get_drag_out_enabled(state: State<AppState>) -> CmdResult<bool> {
    let conn = lock_db(&state)?;
    drag_out_enabled(&conn)
}

#[tauri::command]
pub fn set_drag_out_enabled(state: State<AppState>, enabled: bool) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    db::printer_link::set_setting(&conn, SETTING_DRAG_OUT_ENABLED, if enabled { "1" } else { "0" }).map_err(|e| e.to_string().into())
}

/// Starts the operating system drag session for the given catalog files. Returns after the
/// session was started (Windows: after it ended). Result of the drop is only logged.
#[tauri::command]
pub fn start_file_drag(
    window: tauri::WebviewWindow, state: State<AppState>, environment: State<'_, RuntimeEnvironment>, file_ids: Vec<String>,
) -> CmdResult<()> {
    let invoked = std::time::Instant::now();
    log::info!(target: "drag", "start_file_drag invoked t_ms_since_call=0");
    environment.require_desktop(DesktopFeature::ExternalAction)?;
    let sources = {
        let conn = lock_db(&state)?;
        if !drag_out_enabled(&conn)? {
            return Err(CmdError::coded(ErrorCode::DragDisabled));
        }
        resolve_drag_files(&conn, &file_ids)?
    };
    use tauri::{Emitter, Manager};
    let storage = window.try_state::<std::sync::Arc<crate::drag_storage::DragStorage>>()
        .ok_or_else(|| CmdError::coded_with(ErrorCode::DragFailed, [String::new()]))?;
    let native = window.try_state::<NativeDragState>()
        .ok_or_else(|| CmdError::coded_with(ErrorCode::DragFailed, [String::new()]))?.inner().clone();
    let token = native.begin()?;
    let watchdog = native.clone();
    let watchdog_window = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(DRAG_TIMEOUT);
        watchdog.finish(token, || { let _ = watchdog_window.emit("native-drag-ended", ()); });
    });
    let session = storage.active_session().map_err(|e| { native.finish(token, || {}); CmdError::coded_with(ErrorCode::DragFailed, [e.to_string()]) })?;
    let folder = session.path.clone();
    let files = stage_drag_files(&sources, &folder, DRAG_ORIGINAL_FILE).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&folder);
        native.finish(token, || {});
    })?;
    if token.elapsed() >= DRAG_TIMEOUT {
        let _ = std::fs::remove_dir_all(&folder);
        native.finish(token, || {});
        return Err(CmdError::coded_with(ErrorCode::DragFailed, [String::new()]));
    }
    log::info!(target: "drag", "start_file_drag invoked staged t_ms_since_call={}", invoked.elapsed().as_millis());
    let count = files.len();
    let item = drag::DragItem::Files(files);
    let preview = drag::Image::Raw(DRAG_PREVIEW_PNG.to_vec());
    let finished = native.clone();
    let event_window = window.clone();
    let callback_session = std::sync::Arc::new(std::sync::Mutex::new(Some(session.clone())));
    let callback = move |result: drag::DragResult, _: drag::CursorPosition| {
        log::info!(target: "drag", "drag of {count} file(s) ended: {result:?}");
        // Keep self-drops suppressed through delivery of the queued window event.
        let finished = finished.clone();
        let event_window = event_window.clone();
        // Release storage protection on native completion, not on the UI timeout.
        let session = callback_session.lock().unwrap_or_else(|e| e.into_inner()).take();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(250));
            finished.finish(token, || { let _ = event_window.emit("native-drag-ended", ()); });
            drop(session);
        });
    };
    let options = drag::Options { mode: drag::DragMode::Copy, ..Default::default() };
    #[cfg(target_os = "linux")]
    let started = {
        let gtk_window = window.gtk_window().map_err(|e| { let _ = std::fs::remove_dir_all(&folder); native.finish(token, || {}); CmdError::coded_with(ErrorCode::DragFailed, [e.to_string()]) })?;
        drag::start_drag(&gtk_window, item, preview, callback, options)
    };
    #[cfg(not(target_os = "linux"))]
    let started = drag::start_drag(&window, item, preview, callback, options);
    started.map_err(|e| { let _ = std::fs::remove_dir_all(&folder); native.finish(token, || {}); CmdError::coded_with(ErrorCode::DragFailed, [e.to_string()]) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn_with(path: &Path) -> (Connection, String) {
        let conn = db::connect_in_memory().unwrap();
        let id = db::test_insert_minimal_file(&conn, &path.to_string_lossy(), None).unwrap();
        (conn, id.to_string())
    }

    #[test]
    fn native_drag_guard_refuses_overlap_and_releases_after_end() {
        let state = NativeDragState::default();
        assert!(!state.is_active());
        let token = state.begin().unwrap();
        assert!(state.is_active());
        assert!(state.begin().is_err());
        state.finish(token, || {});
        assert!(!state.is_active());
        state.begin().unwrap();
    }

    #[test]
    fn missing_callback_expires_and_late_finish_cannot_clear_next_drag() {
        let state = NativeDragState::default();
        let old = std::time::Instant::now().checked_sub(DRAG_TIMEOUT).expect("monotonic clock must represent the timeout interval");
        *state.0.lock().unwrap() = Some(old);
        assert!(!state.is_active());
        let current = state.begin().unwrap();
        state.finish(old, || panic!("stale session must not emit"));
        assert!(state.is_active());
        state.finish(current, || {});
        assert!(!state.is_active());
    }

    #[test]
    fn overlap_is_refused_before_staging() {
        let state = NativeDragState::default();
        state.begin().unwrap();
        assert!(state.begin().is_err());
    }

    #[test]
    fn the_switch_is_off_by_default_and_can_be_turned_on() {
        let conn = db::connect_in_memory().unwrap();
        assert!(!drag_out_enabled(&conn).unwrap());
        db::printer_link::set_setting(&conn, SETTING_DRAG_OUT_ENABLED, "1").unwrap();
        assert!(drag_out_enabled(&conn).unwrap());
    }

    #[test]
    fn only_3mf_stl_and_obj_are_draggable() {
        let dir = unique_test_dir("drag_types");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, ok) in [("a.3mf", true), ("b.STL", true), ("c.obj", true), ("d.step", false), ("e.stp", false), ("f.gcode", false)] {
            let path = dir.join(name);
            std::fs::write(&path, b"x").unwrap();
            let (conn, id) = conn_with(&path);
            let result = resolve_drag_files(&conn, &[id]);
            if ok { assert_eq!(result.unwrap(), vec![path]); }
            else { assert!(result.unwrap_err().is(ErrorCode::DragNotSliceable, &[]), "{name}"); }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_an_unknown_id_and_bad_lists_are_refused() {
        let dir = unique_test_dir("drag_missing");
        std::fs::create_dir_all(&dir).unwrap();
        let (conn, id) = conn_with(&dir.join("weg.3mf"));
        assert!(resolve_drag_files(&conn, &[id]).unwrap_err().is(ErrorCode::DragFileMissing, &[]));
        assert!(resolve_drag_files(&conn, &["999999".into()]).unwrap_err().is(ErrorCode::ModelNotFoundOrTrashed, &[]));
        assert!(resolve_drag_files(&conn, &["abc".into()]).unwrap_err().is(ErrorCode::InvalidCatalogId, &[]));
        assert!(resolve_drag_files(&conn, &[]).unwrap_err().is(ErrorCode::ReqInvalidArguments, &[]));
        let many: Vec<String> = (0..=MAX_DRAG_FILES).map(|n| n.to_string()).collect();
        assert!(resolve_drag_files(&conn, &many).unwrap_err().is(ErrorCode::ReqInvalidArguments, &[]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn staging_copies_files_and_leaves_the_catalog_file_alone() {
        let dir = unique_test_dir("drag_stage");
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("same.3mf");
        std::fs::write(&a, b"AAA").unwrap();
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let b = sub.join("same.3mf");
        std::fs::write(&b, b"BBB").unwrap();
        let root = dir.join("tmp");
        let staged = stage_drag_files(&[a.clone(), b.clone()], &root, false).unwrap();
        assert_eq!(staged.len(), 2);
        assert_ne!(staged[0], staged[1], "same names must not collide");
        assert_eq!(std::fs::read(&staged[0]).unwrap(), b"AAA");
        assert_eq!(std::fs::read(&staged[1]).unwrap(), b"BBB");
        assert!(staged.iter().all(|p| p.starts_with(std::fs::canonicalize(&root).unwrap())));
        // Simulated "move" by the target: the catalog files must still be there.
        std::fs::remove_file(&staged[0]).unwrap();
        assert!(a.is_file() && b.is_file());
        std::fs::remove_dir_all(&root).unwrap();
        assert!(!root.exists());
        assert!(a.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_original_is_only_handed_over_when_the_constant_says_so() {
        const { assert!(!DRAG_ORIGINAL_FILE, "the original may only be dragged after the probe on all three systems") };
        let dir = unique_test_dir("drag_original");
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("o.stl");
        std::fs::write(&a, b"x").unwrap();
        let staged = stage_drag_files(std::slice::from_ref(&a), &dir.join("tmp"), true).unwrap();
        assert_eq!(staged, vec![std::fs::canonicalize(&a).unwrap()]);
        assert!(!dir.join("tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
