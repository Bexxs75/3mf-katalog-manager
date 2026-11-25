//! Companion files of a model: pictures, PDFs and text notes that belong to it.
//!
//! The files live in a `<model name>_extras` folder next to the model; the
//! database only records them. Only a short allowlist of types is ever accepted,
//! and the type is decided by extension AND content, so an executable renamed to
//! `.png` or a PNG without a matching extension is refused. Nothing here can open
//! or read a file the allowlist did not accept.
//!
//! The actions (add, trash, restore, purge, assign from an archive, carry with the model)
//! live in `katalog_core::companion_actions` (issue #20, P3); this file keeps the Tauri
//! commands, the DTOs and the reading side, and passes the desktop's `Fs` boundary on.

use super::*;
use crate::db::models::CompanionRecord;

use katalog_core::companion_actions::{self as actions, CompanionFs};
pub(crate) use katalog_core::companions::{extras_dir_for, CompanionKind, MAX_COMPANION_BYTES_CONFIGURABLE};
use katalog_core::companion_read::{self, RowError};
pub(super) use katalog_core::companion_read::MAX_PREVIEW_BYTES;
use katalog_core::companions::ALLOWED_EXTENSIONS;
// Used by the tests next to this file (`use super::*`).
#[cfg(test)]
use katalog_core::companions::{classify, validate_stored_fields, Refusal, CONTENT_CHECK_BYTES, MAX_COMPANIONS_PER_MODEL, MAX_COMPANION_BYTES};

/// The companion actions' disk access (core trait) on top of the desktop's `Fs` boundary.
/// Every call goes to the same `Fs` method the code used before the actions moved into the
/// core, so the call order seen by `FaultFs` is unchanged.
pub(super) struct CompanionFsAdapter<'a>(pub(super) &'a dyn Fs);

impl CompanionFs for CompanionFsAdapter<'_> {
    fn symlink_metadata(&self, path: &Path) -> std::io::Result<std::fs::Metadata> {
        self.0.symlink_metadata(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.0.exists(path)
    }
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        self.0.canonicalize(path)
    }
    fn create_dir(&self, path: &Path) -> std::io::Result<()> {
        self.0.create_dir(path)
    }
    fn remove_dir(&self, path: &Path) -> std::io::Result<()> {
        self.0.remove_dir(path)
    }
    fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        self.0.remove_file(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        self.0.rename(from, to)
    }
    fn create_new(&self, path: &Path) -> std::io::Result<std::fs::File> {
        std::fs::OpenOptions::new().write(true).create_new(true).open(path)
    }
    fn move_file(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        move_file_with(self.0, from, to)
    }
}

fn is_real_file(fs: &dyn Fs, path: &Path) -> bool {
    fs.symlink_metadata(path).is_ok_and(|m| m.is_file())
}

/// The user-facing text for a refusal to open a stored companion.
fn row_error(error: RowError) -> CmdError {
    actions::row_error(error).into()
}

/// The folder of a stored companion must be exactly the genuine `_extras` folder of the
/// model the row belongs to, before any file in it is moved, opened or read.
fn ensure_row_folder_genuine(conn: &Connection, row: &CompanionRecord) -> CmdResult<()> {
    actions::ensure_row_folder_genuine(conn, row).map_err(Into::into)
}

/// Opens a stored companion for reading after all checks: structure, not trashed, a
/// regular file (never a symlink), size, protected folders, and the content check.
/// The returned handle is the one that was checked.
fn open_checked(conn: &Connection, row: &CompanionRecord, sensitive_dirs: &[PathBuf], max_bytes: u64) -> CmdResult<(CompanionKind, std::fs::File, Vec<u8>)> {
    let kind = companion_read::check_row_state(row).map_err(row_error)?;
    reject_if_sensitive_path(Path::new(&row.path), sensitive_dirs)?;
    ensure_row_folder_genuine(conn, row)?;
    let (file, head) = companion_read::open_row_file(row, kind, max_bytes, false, None).map_err(row_error)?;
    Ok((kind, file, head))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionDto {
    pub id: String,
    pub file_id: String,
    pub name: String,
    pub path: String,
    /// "image", "pdf" or "text".
    pub kind: String,
    pub size_bytes: Option<i64>,
    pub added_at: String,
    /// Set while the file sits in the app trash.
    pub deleted_at: Option<String>,
    /// The file is gone from disk (or from the trash). Only a state of this entry, never a catalog change.
    pub missing: bool,
}

fn to_dto(row: CompanionRecord) -> CompanionDto {
    let location = row.trash_path.as_deref().filter(|_| row.deleted_at.is_some()).unwrap_or(&row.path);
    let missing = !is_real_file(&RealFs, Path::new(location));
    CompanionDto {
        id: row.id.to_string(),
        file_id: row.file_id.to_string(),
        name: row.name,
        path: row.path,
        kind: row.kind,
        size_bytes: row.size_bytes,
        added_at: row.added_at,
        deleted_at: row.deleted_at,
        missing,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionOpenInfoDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub kind: String,
}

fn parse_companion_id(id: &str) -> CmdResult<i64> {
    catalog_removal::parse_id(id)
}

fn load_row(conn: &Connection, id: i64) -> CmdResult<CompanionRecord> {
    db::companions::get_companion(conn, id)?.ok_or_else(|| CmdError::expected("Begleitdatei nicht gefunden."))
}

fn load_model(conn: &Connection, file_id: i64) -> CmdResult<db::models::FileRecord> {
    db::get_file(conn, file_id)?.ok_or_else(|| CmdError::expected("Modell nicht gefunden."))
}

pub(super) fn list_companions_with_conn(conn: &Connection, file_id: i64) -> CmdResult<Vec<CompanionRecord>> {
    load_model(conn, file_id)?;
    Ok(db::companions::list_companions(conn, file_id)?)
}

/// Copies a user-chosen file into the model's extras folder. The source is only read.
pub(super) fn add_companion_with_conn(
    conn: &Connection,
    file_id: i64,
    source: &Path,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<CompanionRecord> {
    actions::add_companion(conn, &CompanionFsAdapter(&RealFs), file_id, source, sensitive_dirs).map_err(Into::into)
}

pub(super) fn remove_companion_with_conn(conn: &Connection, id: i64, trash_dir: &Path) -> CmdResult<()> {
    actions::remove_companion(conn, &CompanionFsAdapter(&RealFs), id, trash_dir).map_err(Into::into)
}

/// Sends every active companion of a model to the trash together with it, stamped with
/// the model's `deleted_at` so a restore of the model finds exactly these. A companion
/// that cannot be moved stays where it is and active; the model's removal never fails
/// because of it.
pub(super) fn trash_with_model_fs(fs: &dyn Fs, conn: &Connection, file_id: i64, trash_dir: &Path, deleted_at: &str) {
    actions::trash_with_model(&CompanionFsAdapter(fs), conn, file_id, trash_dir, deleted_at)
}

pub(super) fn restore_companion_with_conn(conn: &Connection, id: i64, sensitive_dirs: &[PathBuf]) -> CmdResult<CompanionRecord> {
    actions::restore_companion(conn, &CompanionFsAdapter(&RealFs), id, sensitive_dirs).map_err(Into::into)
}

/// Brings back the companions that went to the trash with `model_deleted_at`, after the
/// model itself was restored (its path may have changed). Failures are logged; the file
/// stays in the trash and can be restored on its own.
pub(super) fn restore_with_model(conn: &Connection, file_id: i64, model_deleted_at: &str, sensitive_dirs: &[PathBuf]) {
    actions::restore_with_model(&CompanionFsAdapter(&RealFs), conn, file_id, model_deleted_at, sensitive_dirs)
}

/// Removes the trash files of all companions of a model that is about to be deleted for
/// good (its rows follow by cascade). An error means a file could not be removed; the
/// caller then keeps the model entry, like for the model file itself.
pub(super) fn purge_trash_files_of_model(conn: &Connection, file_id: i64, trash_dir: &Path) -> Result<(), String> {
    actions::purge_trash_files_of_model(&CompanionFsAdapter(&RealFs), conn, file_id, trash_dir)
}

/// Final removal of every companion in the trash (for "empty trash").
pub(super) fn purge_all_trashed(conn: &Connection, trash_dir: &Path) {
    actions::purge_all_trashed(&CompanionFsAdapter(&RealFs), conn, trash_dir)
}

/// Final removal of companions that have been in the trash longer than the cutoff.
pub(super) fn purge_expired_trashed(conn: &Connection, trash_dir: &Path, cutoff: &str) {
    actions::purge_expired_trashed(&CompanionFsAdapter(&RealFs), conn, trash_dir, cutoff)
}

/// After an archive was extracted and its models imported: gives the files of an allowed
/// type to the model they belong to (see `katalog_core::companion_actions::assign_from_archive`).
/// Returns `(assigned, ignored)`.
pub(super) fn assign_from_archive(
    conn: &Connection,
    written: &[PathBuf],
    imported: &[ModelFileDto],
    sensitive_dirs: &[PathBuf],
) -> (u32, u32) {
    actions::assign_from_archive(&CompanionFsAdapter(&RealFs), conn, written, imported, sensitive_dirs)
}

/// Companion files that were moved along with their model, so the move can be undone.
pub(super) struct Carried(actions::Carried);

/// On these systems two names that differ only in case are the same folder.
const CASE_INSENSITIVE_PATHS: bool = cfg!(any(windows, target_os = "macos"));

impl Carried {
    /// Everything went through: the old folder goes away if it is empty now.
    pub(super) fn finish(self) {
        self.finish_with(&RealFs)
    }

    pub(super) fn finish_with(self, fs: &dyn Fs) {
        self.0.finish(&CompanionFsAdapter(fs))
    }

    /// Puts files and rows back where they were (best effort, failures are logged).
    pub(super) fn undo(self, conn: &Connection) {
        self.undo_with(&RealFs, conn)
    }

    pub(super) fn undo_with(self, fs: &dyn Fs, conn: &Connection) {
        self.0.undo(&CompanionFsAdapter(fs), conn)
    }
}

/// A model moved or was renamed from `old_model` to `new_model` (the model file is
/// already at its new place): its companions follow into the new extras folder. On any
/// failure everything done here is undone and an error returned, so the caller can undo
/// the model move.
pub(super) fn carry(conn: &Connection, file_id: i64, old_model: &Path, new_model: &Path) -> CmdResult<Carried> {
    carry_with(conn, file_id, old_model, new_model, CASE_INSENSITIVE_PATHS)
}

pub(super) fn carry_with(conn: &Connection, file_id: i64, old_model: &Path, new_model: &Path, case_insensitive: bool) -> CmdResult<Carried> {
    carry_with_fs(&RealFs, conn, file_id, old_model, new_model, case_insensitive)
}

/// `carry` on an injected file system; the `Carried` it returns must be finished or undone
/// with the same one (`finish_with` / `undo_with`).
pub(super) fn carry_fs(fs: &dyn Fs, conn: &Connection, file_id: i64, old_model: &Path, new_model: &Path) -> CmdResult<Carried> {
    carry_with_fs(fs, conn, file_id, old_model, new_model, CASE_INSENSITIVE_PATHS)
}

pub(super) fn carry_with_fs(fs: &dyn Fs, conn: &Connection, file_id: i64, old_model: &Path, new_model: &Path, case_insensitive: bool) -> CmdResult<Carried> {
    actions::carry(&CompanionFsAdapter(fs), conn, file_id, old_model, new_model, case_insensitive).map(Carried).map_err(Into::into)
}

async fn blocking<T: Send + 'static>(
    app: tauri::AppHandle,
    work: impl FnOnce(&tauri::AppHandle) -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(move || work(&app)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_companions(app: tauri::AppHandle, file_id: String) -> CmdResult<Vec<CompanionDto>> {
    blocking(app, move |app| {
        use tauri::Manager;
        let id = parse_companion_id(&file_id)?;
        let rows = {
            let state = app.state::<AppState>();
            let conn = lock_db(&state)?;
            list_companions_with_conn(&conn, id)?
        };
        // The disk is only looked at after the catalog lock is released.
        Ok(rows.into_iter().map(to_dto).collect())
    })
    .await
}

/// Opens the file dialog for companions. The chosen paths are remembered for the next
/// `add_companion` call, so the webview cannot name a file the user never picked.
#[tauri::command]
pub async fn pick_companion_files(app: tauri::AppHandle) -> CmdResult<Vec<String>> {
    blocking(app, |app| {
        use tauri::Manager;
        let picked = app.dialog().file().add_filter("Bilder, PDF, Text, weitere Formate", ALLOWED_EXTENSIONS).blocking_pick_files();
        let paths: Vec<PathBuf> = picked.unwrap_or_default().into_iter().filter_map(|p| p.into_path().ok()).collect();
        let state = app.state::<AppState>();
        for path in &paths {
            state.import_jobs.observe_picker(path);
        }
        Ok(paths.into_iter().map(|p| p.to_string_lossy().into_owned()).collect())
    })
    .await
}

/// Copies a file the user picked (or dropped on the window) into the model's extras folder.
#[tauri::command]
pub async fn add_companion(app: tauri::AppHandle, file_id: String, source_path: String) -> CmdResult<CompanionDto> {
    blocking(app, move |app| {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let _catalog_share = state.import_jobs.gate.exclusive()?;
        let id = parse_companion_id(&file_id)?;
        let source = PathBuf::from(&source_path);
        if !state.import_jobs.claim_user_selected(&source) {
            return Err(CmdError::expected("Die Datei wurde nicht in der App ausgewählt."));
        }
        let conn = lock_db(&state)?;
        add_companion_with_conn(&conn, id, &source, &state.sensitive_dirs).map(to_dto)
    })
    .await
}

#[tauri::command]
pub async fn remove_companion(app: tauri::AppHandle, companion_id: String) -> CmdResult<()> {
    blocking(app, move |app| {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let _catalog_share = state.import_jobs.gate.exclusive()?;
        let id = parse_companion_id(&companion_id)?;
        let conn = lock_db(&state)?;
        remove_companion_with_conn(&conn, id, &state.trash_dir)
    })
    .await
}

#[tauri::command]
pub async fn restore_companion(app: tauri::AppHandle, companion_id: String) -> CmdResult<CompanionDto> {
    blocking(app, move |app| {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let _catalog_share = state.import_jobs.gate.exclusive()?;
        let id = parse_companion_id(&companion_id)?;
        let conn = lock_db(&state)?;
        restore_companion_with_conn(&conn, id, &state.sensitive_dirs).map(to_dto)
    })
    .await
}

/// Rows added through the web interface may be as big as it accepts; the desktop opens them,
/// although it adds nothing above `MAX_COMPANION_BYTES` itself.
const MAX_OPEN_BYTES: u64 = MAX_COMPANION_BYTES_CONFIGURABLE;

/// Path and kind of a companion, only if it still passes every check for an allowed type.
pub(super) fn open_info_with_conn(conn: &Connection, id: i64, sensitive_dirs: &[PathBuf]) -> CmdResult<CompanionOpenInfoDto> {
    let row = load_row(conn, id)?;
    let (kind, _, _) = open_checked(conn, &row, sensitive_dirs, MAX_OPEN_BYTES)?;
    Ok(CompanionOpenInfoDto { id: row.id.to_string(), name: row.name, path: row.path, kind: kind.as_str().to_string() })
}

#[tauri::command]
pub async fn companion_open_info(app: tauri::AppHandle, companion_id: String) -> CmdResult<CompanionOpenInfoDto> {
    blocking(app, move |app| {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let id = parse_companion_id(&companion_id)?;
        let conn = lock_db(&state)?;
        open_info_with_conn(&conn, id, &state.sensitive_dirs)
    })
    .await
}

/// A file of the `other` kind is never handed to a program: the default program of a document or a
/// CAD source may run active content (macros, scripts).
fn refuse_unopenable(info: &CompanionOpenInfoDto) -> CmdResult<()> {
    if CompanionKind::parse(&info.kind) == Some(CompanionKind::Other) {
        return Err(CmdError::expected("Dieses Format wird nicht geöffnet. Die Datei kann im Ordner angezeigt werden."));
    }
    Ok(())
}

/// Opens a companion with the system's default program. It takes an id, never a path:
/// the only files that can be opened are catalog companions that pass the checks again
/// at this moment (allowed type, matching content, a regular file in an extras folder).
#[tauri::command]
pub async fn open_companion(app: tauri::AppHandle, companion_id: String) -> CmdResult<()> {
    blocking(app, move |app| {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let environment = *app.state::<RuntimeEnvironment>();
        environment.require_desktop(runtime_environment::DesktopFeature::ExternalAction)?;
        let id = parse_companion_id(&companion_id)?;
        let info = {
            let conn = lock_db(&state)?;
            open_info_with_conn(&conn, id, &state.sensitive_dirs)?
        };
        refuse_unopenable(&info)?;
        open_external(&info.path, environment)
    })
    .await
}

/// Shows a companion in the system's file manager, selected. Like [`open_companion`] it takes an
/// id and checks the row again; unlike it, it starts no program on the file, so it is also
/// the one way to reach a file of a format that is never opened.
#[tauri::command]
pub async fn reveal_companion(app: tauri::AppHandle, companion_id: String) -> CmdResult<()> {
    blocking(app, move |app| {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let environment = *app.state::<RuntimeEnvironment>();
        environment.require_desktop(runtime_environment::DesktopFeature::FileManager)?;
        let id = parse_companion_id(&companion_id)?;
        let info = {
            let conn = lock_db(&state)?;
            open_info_with_conn(&conn, id, &state.sensitive_dirs)?
        };
        super::files::reveal_catalog_path(Path::new(&info.path))
    })
    .await
}

/// The picture as a data URI for the preview. Only images, with a size cap; anything
/// else (or a file whose content is not the claimed picture) is refused.
pub(super) fn image_data_with_conn(conn: &Connection, id: i64, sensitive_dirs: &[PathBuf]) -> CmdResult<String> {
    let row = load_row(conn, id)?;
    if CompanionKind::parse(&row.kind) != Some(CompanionKind::Image) {
        return Err(CmdError::expected("Diese Begleitdatei ist kein Bild."));
    }
    let (_, mut file, head) = open_checked(conn, &row, sensitive_dirs, MAX_PREVIEW_BYTES)?;
    companion_read::image_data_uri(&mut file, head, &row.name).map_err(|e| match e {
        RowError::TooLarge => CmdError::expected("Das Bild ist für die Vorschau zu groß."),
        other => row_error(other),
    })
}

#[tauri::command]
pub async fn companion_image_data(app: tauri::AppHandle, companion_id: String) -> CmdResult<String> {
    blocking(app, move |app| {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let id = parse_companion_id(&companion_id)?;
        let conn = lock_db(&state)?;
        image_data_with_conn(&conn, id, &state.sensitive_dirs)
    })
    .await
}

#[cfg(test)]
#[path = "companions_tests.rs"]
mod tests;
