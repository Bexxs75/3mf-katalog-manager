//! "Save as STL": converts a catalog file (3MF, OBJ, STEP) into a binary STL.
//!
//! The command takes the file id, never a path; the target comes from the save
//! dialog of the backend. The writer itself lives in the core
//! (`katalog_core::stl`), so the web version can reuse it later. An existing
//! target is never overwritten, even if the system dialog was confirmed, and
//! nothing is moved or added to the catalog.

use super::runtime_environment::{DesktopFeature, RuntimeEnvironment};
use super::*;

use katalog_core::error_code::ErrorCode;
use katalog_core::read::GeometryErrorCode as Code;
use katalog_core::stl::{self, SourceStamp, StlScope, StlWriteError};

/// Which part of the model is converted. The interface offers "all" and one plate; the
/// parts list of a STEP file chooses single parts (`Parts`) or one file per part (`EachPart`,
/// the target is then a folder).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StlExportScope {
    All,
    Plate { number: u32 },
    Parts { indices: Vec<usize> },
    EachPart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StlExportResult {
    /// File name of the new STL.
    pub name: String,
    /// Folder it was written to (shown and opened in the file manager).
    pub folder: String,
    pub triangles: u64,
    /// How many STL files were written (more than one only for `EachPart`).
    pub file_count: u32,
    /// The folder lies inside the catalog folder: "Abgleichen" will find the file.
    pub in_catalog_folder: bool,
}

/// Last folder a STL was saved to, only kept while the app runs (no migration, no setting).
static LAST_STL_FOLDER: Mutex<Option<PathBuf>> = Mutex::new(None);

fn stl_error(error: StlWriteError, target: &Path) -> CmdError {
    match error {
        StlWriteError::NothingToExport => CmdError::coded(ErrorCode::StlNothingToExport),
        StlWriteError::PartOutOfRange { .. } => CmdError::coded(ErrorCode::ReqInvalidArguments),
        StlWriteError::TooManyTriangles { count, max } => CmdError::coded_with(ErrorCode::StlTooLarge, [count, max]),
        StlWriteError::TargetIsSource => CmdError::coded(ErrorCode::StlTargetIsSource),
        StlWriteError::SourceChanged => CmdError::coded(ErrorCode::StlSourceChanged),
        StlWriteError::TargetExists => CmdError::coded_with(
            ErrorCode::StlTargetExists,
            [target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()],
        ),
        StlWriteError::InvalidIndex | StlWriteError::NonFinite => {
            log::warn!(target: "stl", "mesh rejected: {error}");
            CmdError::coded(ErrorCode::StlSourceUnreadable)
        }
        StlWriteError::Io(e) => CmdError::coded_with(ErrorCode::StlWriteFailed, [e.to_string()]),
    }
}

/// Maps a failed read of the source to a registered code (the preview's own
/// geometry codes would need their own wording in this dialog).
fn read_error(code: Option<Code>, message: &str, source: &Path, step: bool) -> CmdError {
    match code {
        Some(Code::NotFound) => CmdError::coded_with(ErrorCode::FileMissingOnDisk, [source.to_string_lossy()]),
        Some(Code::TooLarge) => CmdError::coded(ErrorCode::StlSourceTooLarge),
        Some(Code::Unsupported) if step => CmdError::coded(ErrorCode::StlStepUnavailable),
        _ => {
            log::warn!(target: "stl", "source not readable: {message}");
            CmdError::coded(ErrorCode::StlSourceUnreadable)
        }
    }
}

/// A STL is already what the command would write.
fn ensure_convertible(file_type: db::models::FileType) -> CmdResult<()> {
    if file_type == db::models::FileType::Stl { Err(CmdError::coded(ErrorCode::StlSourceIsStl)) } else { Ok(()) }
}

fn is_step(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "stp" | "step"))
}

fn load_meshes(source: &Path) -> CmdResult<Vec<RenderMesh>> {
    if is_step(source) {
        return super::files::step_geometry(source).map_err(|e| read_error(e.code, &e.message, source, true));
    }
    katalog_core::read::load_render_meshes(source).map_err(|e| read_error(e.code, &e.message, source, false))
}

/// Reads `source`, picks the scope and writes it to the not yet existing `target`.
/// Fails when the source changed while it was read, so a half-replaced file is
/// never converted.
pub(crate) fn write_stl_export(source: &Path, scope: &StlExportScope, target: &Path) -> CmdResult<u64> {
    let stamp = SourceStamp::of(source).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => CmdError::coded_with(ErrorCode::FileMissingOnDisk, [source.to_string_lossy()]),
        _ => CmdError::coded(ErrorCode::StlSourceUnreadable),
    })?;
    let meshes = load_meshes(source)?;
    stamp.ensure_unchanged(source).map_err(|e| stl_error(e, target))?;
    let scope = match scope {
        StlExportScope::All => StlScope::All,
        StlExportScope::Plate { number } => StlScope::Plate(*number),
        StlExportScope::Parts { indices } => StlScope::Parts(indices),
        // One file per part has no single target; see `write_each_part`.
        StlExportScope::EachPart => return Err(CmdError::coded(ErrorCode::ReqInvalidArguments)),
    };
    let selected = stl::select_meshes(&meshes, scope).map_err(|e| stl_error(e, target))?;
    stl::write_stl_file_new(target, Some(source), &selected).map_err(|e| stl_error(e, target))
}

/// `<stem>_Teil<N>.stl` for part `index` (0-based) of `count`; N is zero-padded to the width of
/// `count`, so the files sort in list order.
pub(crate) fn part_file_name(stem: &str, index: usize, count: usize) -> String {
    let width = count.max(1).to_string().len();
    format!("{stem}_Teil{:0width$}.stl", index + 1)
}

/// Writes every non-empty mesh as its own STL into `folder` (named by position in the
/// list, so the number matches the parts list). Nothing is written if one of the files
/// already exists. Returns the number of files and the triangles in total.
pub(crate) fn write_each_part(meshes: &[RenderMesh], source: &Path, folder: &Path) -> CmdResult<(u32, u64)> {
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "model".into());
    let parts: Vec<(usize, &RenderMesh)> = meshes.iter().enumerate().filter(|(_, m)| !m.indices.is_empty()).collect();
    if parts.is_empty() {
        return Err(stl_error(StlWriteError::NothingToExport, folder));
    }
    let targets: Vec<PathBuf> = parts.iter().map(|(i, _)| folder.join(part_file_name(&stem, *i, meshes.len()))).collect();
    if let Some(taken) = targets.iter().find(|t| std::fs::symlink_metadata(t).is_ok()) {
        return Err(stl_error(StlWriteError::TargetExists, taken));
    }
    let mut triangles = 0;
    for ((_, mesh), target) in parts.iter().zip(&targets) {
        triangles += stl::write_stl_file_new(target, Some(source), &[*mesh]).map_err(|e| stl_error(e, target))?;
    }
    Ok((parts.len() as u32, triangles))
}

/// `<name>.stl`, or `<name>_Platte<N>.stl` for one plate.
pub(crate) fn suggested_name(source: &Path, scope: &StlExportScope) -> String {
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "model".into());
    match scope {
        StlExportScope::Plate { number } => format!("{stem}_Platte{number}.stl"),
        _ => format!("{stem}.stl"),
    }
}

/// The save dialog may return a name without the extension; STL files end in `.stl`.
pub(crate) fn with_stl_extension(mut target: PathBuf) -> PathBuf {
    if !target.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("stl")) {
        let mut name = target.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(".stl");
        target.set_file_name(name);
    }
    target
}

/// Whether `folder` is the catalog folder or lies below it.
pub(crate) fn is_inside_catalog(base: &Path, folder: &Path) -> bool {
    match (base.canonicalize(), folder.canonicalize()) {
        (Ok(base), Ok(folder)) => folder.starts_with(base),
        _ => false,
    }
}

/// Opens the save dialog in the backend (the target only comes from the user's pick) and
/// writes the STL. `None` when the dialog was cancelled. async, because a synchronous
/// command runs on the thread GTK needs for the dialog (deadlock).
#[tauri::command]
pub async fn export_model_as_stl<R: tauri::Runtime>(
    app: tauri::AppHandle<R>, state: State<'_, AppState>, environment: State<'_, RuntimeEnvironment>,
    file_id: String, scope: StlExportScope,
) -> CmdResult<Option<StlExportResult>> {
    environment.require_desktop(DesktopFeature::ExternalAction)?;
    let (source, base_dir) = {
        let conn = lock_db(&state)?;
        let id: i64 = file_id.parse().map_err(|_| CmdError::coded(ErrorCode::InvalidCatalogId))?;
        let file = db::get_file(&conn, id).map_err(|e| e.to_string())?
            .ok_or_else(|| CmdError::coded(ErrorCode::ModelNotFoundOrTrashed))?;
        ensure_convertible(file.file_type)?;
        let base = db::printer_link::get_setting(&conn, folders::SETTING_CATALOG_BASE_DIR).map_err(|e| e.to_string())?;
        (PathBuf::from(file.current_path()), base)
    };
    // Proposal: the folder of the source, otherwise the one used last.
    let folder = source.parent().filter(|p| p.is_dir()).map(Path::to_path_buf)
        .or_else(|| LAST_STL_FOLDER.lock().ok().and_then(|f| f.clone()).filter(|p| p.is_dir()));
    if scope == StlExportScope::EachPart {
        return export_each_part(app, source, folder, base_dir).await;
    }
    let name = suggested_name(&source, &scope);
    let picked = tauri::async_runtime::spawn_blocking({
        let app = app.clone();
        move || {
            let mut dialog = app.dialog().file().add_filter("STL", &["stl"]).set_file_name(name);
            if let Some(folder) = folder { dialog = dialog.set_directory(folder); }
            dialog.blocking_save_file()
        }
    }).await.map_err(|e| e.to_string())?;
    let Some(picked) = picked else { return Ok(None) };
    let target = with_stl_extension(picked.into_path().map_err(|e| e.to_string())?);
    let triangles = tauri::async_runtime::spawn_blocking({
        let (source, target) = (source.clone(), target.clone());
        move || write_stl_export(&source, &scope, &target)
    }).await.map_err(|e| e.to_string())??;
    let folder = target.parent().map(Path::to_path_buf).unwrap_or_default();
    if let Ok(mut last) = LAST_STL_FOLDER.lock() { *last = Some(folder.clone()); }
    log::info!(target: "stl", "STL saved ({triangles} triangles)");
    Ok(Some(StlExportResult {
        name: target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        in_catalog_folder: base_dir.is_some_and(|base| is_inside_catalog(Path::new(&base), &folder)),
        folder: folder.to_string_lossy().into_owned(),
        triangles,
        file_count: 1,
    }))
}

/// "Each part as its own file": the user picks a folder instead of a file name.
async fn export_each_part<R: tauri::Runtime>(
    app: tauri::AppHandle<R>, source: PathBuf, folder: Option<PathBuf>, base_dir: Option<String>,
) -> CmdResult<Option<StlExportResult>> {
    let picked = tauri::async_runtime::spawn_blocking({
        let app = app.clone();
        move || {
            let mut dialog = app.dialog().file();
            if let Some(folder) = folder { dialog = dialog.set_directory(folder); }
            dialog.blocking_pick_folder()
        }
    }).await.map_err(|e| e.to_string())?;
    let Some(picked) = picked else { return Ok(None) };
    let folder = picked.into_path().map_err(|e| e.to_string())?;
    let (files, triangles) = tauri::async_runtime::spawn_blocking({
        let (source, folder) = (source.clone(), folder.clone());
        move || {
            let stamp = SourceStamp::of(&source).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => CmdError::coded_with(ErrorCode::FileMissingOnDisk, [source.to_string_lossy()]),
                _ => CmdError::coded(ErrorCode::StlSourceUnreadable),
            })?;
            let meshes = load_meshes(&source)?;
            stamp.ensure_unchanged(&source).map_err(|e| stl_error(e, &folder))?;
            write_each_part(&meshes, &source, &folder)
        }
    }).await.map_err(|e| e.to_string())??;
    if let Ok(mut last) = LAST_STL_FOLDER.lock() { *last = Some(folder.clone()); }
    log::info!(target: "stl", "STL saved ({files} files, {triangles} triangles)");
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(Some(StlExportResult {
        name: format!("{stem}_Teil*.stl"),
        in_catalog_folder: base_dir.is_some_and(|base| is_inside_catalog(Path::new(&base), &folder)),
        folder: folder.to_string_lossy().into_owned(),
        triangles,
        file_count: files,
    }))
}

#[cfg(test)]
#[path = "stl_export_tests.rs"]
mod tests;
