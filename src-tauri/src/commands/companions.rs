//! Companion files of a model: pictures, PDFs and text notes that belong to it.
//!
//! The files live in a `<model name>_extras` folder next to the model; the
//! database only records them. Only a short allowlist of types is ever accepted,
//! and the type is decided by extension AND content, so an executable renamed to
//! `.png` or a PNG without a matching extension is refused. Nothing here can open
//! or read a file the allowlist did not accept.

use super::*;
use crate::db::models::CompanionRecord;
use std::io::{Read, Write};

pub(super) use katalog_core::companions::validate_stored_fields;
pub(crate) use katalog_core::companions::{
    classify, extras_dir_for, is_extras_dir_name, CompanionKind, Refusal, MAX_COMPANIONS_PER_MODEL, MAX_COMPANION_BYTES,
};
use katalog_core::companions::{has_parent_component, kind_for_extension, lowercase_extension, CONTENT_CHECK_BYTES};

/// Largest image sent to the preview; bigger pictures are opened externally.
const MAX_PREVIEW_BYTES: u64 = 10 * 1024 * 1024;
/// Extensions of the allowed types, for the file picker.
const PICKER_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "pdf", "txt", "md"];

/// The user-facing text for a refusal; the rule itself lives in `katalog_core::companions`.
trait RefusalText {
    fn message(self) -> &'static str;
}

impl RefusalText for Refusal {
    fn message(self) -> &'static str {
        match self {
            Self::Type => "Dieser Dateityp ist als Begleitdatei nicht erlaubt. Erlaubt sind Bilder (PNG, JPG, GIF, WebP), PDF und Textdateien (TXT, MD).",
            Self::Content => "Der Inhalt der Datei passt nicht zu ihrem Dateityp und wird nicht übernommen.",
        }
    }
}

fn image_mime(name: &str) -> &'static str {
    match lowercase_extension(name).as_deref() {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    }
}

/// Reads at most the first [`CONTENT_CHECK_BYTES`] of an open file.
fn read_head(file: &mut std::fs::File) -> std::io::Result<Vec<u8>> {
    let mut head = Vec::new();
    Read::by_ref(file).take(CONTENT_CHECK_BYTES as u64).read_to_end(&mut head)?;
    Ok(head)
}

/// A harmless single-component name for the copy of a user-chosen file.
fn companion_file_name(source: &Path) -> Option<String> {
    let original = source.file_name()?.to_string_lossy().into_owned();
    let cleaned: String =
        original.chars().map(|c| if c.is_control() || folders::FORBIDDEN_DIR_NAME_CHARS.contains(&c) { '_' } else { c }).collect();
    let extension = Path::new(&cleaned).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase)?;
    let stem = &cleaned[..cleaned.len() - extension.len() - 1];
    let mut stem = stem.trim_matches(|c: char| c == '.' || c == ' ').to_string();
    if stem.is_empty() {
        stem = "datei".to_string();
    }
    // Leave room for a collision counter and the folder's own limit.
    while stem.len() + extension.len() + 8 > folders::MAX_DIR_NAME_BYTES {
        stem.pop();
    }
    let full = format!("{stem}.{extension}");
    if folders::validate_name(&full, true, folders::NameKind::File).is_ok() {
        return Some(full);
    }
    let prefixed = format!("_{full}");
    folders::validate_name(&prefixed, true, folders::NameKind::File).is_ok().then_some(prefixed)
}

/// Creates a new file under the first free `name`, `name (2)`, ... Never replaces and
/// never follows a symlink at the target (create_new). A path that a catalog row
/// already holds (a trashed companion keeps its old path) counts as taken too, because
/// the row's path is unique; `own_row` is the row that is being restored, which may
/// take its own old path back.
fn create_unique(conn: &Connection, dir: &Path, name: &str, own_row: Option<i64>) -> std::io::Result<(PathBuf, std::fs::File)> {
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) => (stem, format!(".{extension}")),
        None => (name, String::new()),
    };
    for attempt in 1..=999u32 {
        let candidate = if attempt == 1 { name.to_string() } else { format!("{stem} ({attempt}){extension}") };
        let path = dir.join(candidate);
        let held = db::companions::path_held_by_other_row(conn, &path.to_string_lossy(), own_row)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        if held {
            continue;
        }
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no free name"))
}

fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

fn is_real_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

/// The extras folder of a model, after checking that it may be written to: next to
/// the model, free of `..`, outside protected folders, below a catalog folder (also
/// after resolving symlinks) and not itself a symlink.
fn resolve_extras_target(conn: &Connection, model_path: &Path, sensitive_dirs: &[PathBuf]) -> CmdResult<PathBuf> {
    if has_parent_component(model_path) || !model_path.is_absolute() {
        return Err(CmdError::expected("Der Pfad des Modells ist nicht gültig."));
    }
    let extras = extras_dir_for(model_path).ok_or_else(|| CmdError::expected("Der Pfad des Modells ist nicht gültig."))?;
    let extras_name = extras.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    folders::validate_name(&extras_name, true, folders::NameKind::Folder)?;
    reject_if_sensitive_path(&extras, sensitive_dirs)?;
    let parent = extras.parent().ok_or_else(|| CmdError::expected("Der Pfad des Modells ist nicht gültig."))?;
    let resolved_parent = parent
        .canonicalize()
        .map_err(|_| CmdError::expected("Der Ordner des Modells existiert auf der Festplatte nicht mehr."))?;
    let catalog_folders = db::list_folders(conn)?;
    let owner = folders::find_catalog_parent(&extras, &catalog_folders)
        .and_then(|id| catalog_folders.iter().find(|f| f.id == id))
        .and_then(|f| Path::new(&f.path).canonicalize().ok());
    let inside_catalog = owner.is_some_and(|root| folders::comparison_path(&resolved_parent).starts_with(folders::comparison_path(&root)));
    if !inside_catalog {
        return Err(CmdError::expected("Das Modell liegt nicht in einem Katalogordner. Begleitdateien können nur dort abgelegt werden."));
    }
    if let Some(problem) = extras_dir_problem(&extras, &extras) {
        return Err(CmdError::coded(problem));
    }
    Ok(extras)
}

/// Whether two folder paths name the same folder, by the same rule everywhere (Windows
/// ignores case; macOS volumes are usually case-insensitive too).
fn same_dir_with(a: &Path, b: &Path, case_insensitive: bool) -> bool {
    let (a, b) = (folders::comparison_path(a), folders::comparison_path(b));
    if case_insensitive {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    same_dir_with(a, b, CASE_INSENSITIVE_PATHS)
}

/// Why `dir` cannot be trusted as the companion folder `expected` (the `_extras` folder
/// next to a model). A link in place of the folder, or a model folder reached through a
/// different route, would make a file operation land on someone else's file, because a
/// check on the file alone follows links in its parent folders. A folder that does not
/// exist is fine: there is nothing to touch.
fn extras_dir_problem(dir: &Path, expected: &Path) -> Option<ErrorCode> {
    let meta = match std::fs::symlink_metadata(dir) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => return Some(ErrorCode::CompanionFolderUnverifiable),
    };
    if !meta.is_dir() {
        return Some(ErrorCode::CompanionFolderNotReal);
    }
    let (Some(model_dir), Some(name)) = (expected.parent(), expected.file_name()) else {
        return Some(ErrorCode::ModelPathInvalid);
    };
    let (Ok(resolved_dir), Ok(resolved_model_dir)) = (dir.canonicalize(), model_dir.canonicalize()) else {
        return Some(ErrorCode::CompanionFolderUnverifiable);
    };
    if !same_dir(&resolved_dir, &resolved_model_dir.join(name)) {
        return Some(ErrorCode::CompanionFolderElsewhere);
    }
    None
}

/// The folder of a stored companion must be exactly the genuine `_extras` folder of the
/// model the row belongs to, before any file in it is moved, opened or read.
fn ensure_row_folder_genuine(conn: &Connection, row: &CompanionRecord) -> CmdResult<()> {
    let model = load_model(conn, row.file_id)?;
    let expected = extras_dir_for(Path::new(&model.path)).ok_or_else(|| CmdError::expected("Der Pfad des Modells ist nicht gültig."))?;
    let dir = Path::new(&row.path).parent().ok_or_else(|| CmdError::expected("Diese Begleitdatei ist ungültig und wird nicht verwendet."))?;
    if !same_dir(dir, &expected) {
        return Err(CmdError::expected("Die Begleitdatei liegt nicht im Begleitdateien-Ordner ihres Modells."));
    }
    match extras_dir_problem(dir, &expected) {
        Some(problem) => Err(CmdError::coded(problem)),
        None => Ok(()),
    }
}

/// Checks a stored row before anything touches its file.
fn verify_row(row: &CompanionRecord) -> CmdResult<CompanionKind> {
    validate_stored_fields(&row.name, &row.path, &row.kind)
        .map_err(|_| CmdError::expected("Diese Begleitdatei ist ungültig und wird nicht verwendet."))
}

/// Opens a stored companion for reading after all checks: structure, not trashed, a
/// regular file (never a symlink), size, protected folders, and the content check.
/// The returned handle is the one that was checked.
fn open_checked(conn: &Connection, row: &CompanionRecord, sensitive_dirs: &[PathBuf], max_bytes: u64) -> CmdResult<(CompanionKind, std::fs::File, Vec<u8>)> {
    let kind = verify_row(row)?;
    if row.deleted_at.is_some() {
        return Err(CmdError::expected("Diese Begleitdatei liegt im Papierkorb."));
    }
    let path = Path::new(&row.path);
    reject_if_sensitive_path(path, sensitive_dirs)?;
    ensure_row_folder_genuine(conn, row)?;
    if !is_real_file(path) {
        return Err(CmdError::expected("Die Begleitdatei existiert nicht mehr oder ist keine normale Datei."));
    }
    let mut file = crate::safe_file::open_regular(path).map_err(|_| CmdError::expected("Die Begleitdatei kann nicht gelesen werden."))?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    if length > max_bytes {
        return Err(CmdError::expected("Die Begleitdatei ist zu groß."));
    }
    let head = read_head(&mut file).map_err(|_| CmdError::expected("Die Begleitdatei kann nicht gelesen werden."))?;
    let checked = classify(&row.name, &head).map_err(|r| CmdError::expected(r.message()))?;
    if checked != kind {
        return Err(CmdError::expected(Refusal::Content.message()));
    }
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
    let missing = !is_real_file(Path::new(location));
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
    let model = load_model(conn, file_id)?;
    if model.deleted_at.is_some() {
        return Err(CmdError::expected("Das Modell liegt im Papierkorb."));
    }
    if db::companions::count_active_companions(conn, file_id)? >= MAX_COMPANIONS_PER_MODEL {
        return Err(CmdError::expected(format!("Ein Modell kann höchstens {MAX_COMPANIONS_PER_MODEL} Begleitdateien haben.")));
    }

    if !source.is_absolute() || has_parent_component(source) {
        return Err(CmdError::expected("Der Pfad der Datei ist nicht gültig."));
    }
    reject_if_sensitive_path(source, sensitive_dirs)?;
    match std::fs::symlink_metadata(source) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(CmdError::expected("Verknüpfungen (Symlinks) werden nicht als Begleitdatei übernommen."));
        }
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return Err(CmdError::expected("Das ist keine normale Datei.")),
        Err(_) => return Err(CmdError::expected("Die Datei existiert nicht oder kann nicht gelesen werden.")),
    }
    let source_name = source.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    // The same handle is checked and copied, so what was inspected is what is stored.
    let mut input = crate::safe_file::open_regular(source).map_err(|_| CmdError::expected("Die Datei kann nicht gelesen werden."))?;
    if input.metadata().map_err(|e| e.to_string())?.len() > MAX_COMPANION_BYTES {
        return Err(CmdError::expected(format!("Die Datei ist zu groß (höchstens {} MB).", MAX_COMPANION_BYTES / 1024 / 1024)));
    }
    let head = read_head(&mut input).map_err(|_| CmdError::expected("Die Datei kann nicht gelesen werden."))?;
    let kind = classify(source_name, &head).map_err(|r| CmdError::expected(r.message()))?;
    let name = companion_file_name(source).ok_or_else(|| CmdError::expected("Der Dateiname ist nicht verwendbar."))?;

    let extras = resolve_extras_target(conn, Path::new(&model.path), sensitive_dirs)?;
    let created_dir = if is_real_dir(&extras) {
        false
    } else {
        std::fs::create_dir(&extras).map_err(|e| format!("Begleitdateien-Ordner konnte nicht angelegt werden: {e}"))?;
        true
    };
    let cleanup_dir = || {
        if created_dir {
            let _ = std::fs::remove_dir(&extras);
        }
    };
    let (target, mut output) = match create_unique(conn, &extras, &name, None) {
        Ok(created) => created,
        Err(e) => {
            cleanup_dir();
            return Err(e.to_string().into());
        }
    };
    let copied = (|| -> std::io::Result<u64> {
        output.write_all(&head)?;
        // The size can grow after the check above; the cap keeps the copy finite.
        let rest = std::io::copy(&mut Read::by_ref(&mut input).take(MAX_COMPANION_BYTES + 1 - head.len() as u64), &mut output)?;
        output.sync_all()?;
        Ok(head.len() as u64 + rest)
    })();
    drop(output);
    let size = match copied {
        Ok(size) if size <= MAX_COMPANION_BYTES => size,
        other => {
            let _ = std::fs::remove_file(&target);
            cleanup_dir();
            return Err(match other {
                Ok(_) => CmdError::expected("Die Datei ist zu groß."),
                Err(e) => e.to_string().into(),
            });
        }
    };

    let stored_name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(name);
    let added_at = chrono::Utc::now().to_rfc3339();
    let id = match db::companions::insert_companion(
        conn,
        file_id,
        &stored_name,
        &target.to_string_lossy(),
        kind.as_str(),
        Some(size as i64),
        &added_at,
    ) {
        Ok(id) => id,
        Err(e) => {
            let _ = std::fs::remove_file(&target);
            cleanup_dir();
            return Err(e.into());
        }
    };
    log::debug!(target: "datei", "Begleitdatei hinzugefügt: {}", target.display());
    load_row(conn, id)
}

/// Where a trashed companion goes. The id keeps names unique across models, and the
/// `companion-` prefix keeps them apart from trashed models (`{id}-{name}`).
fn trash_target(trash_dir: &Path, row: &CompanionRecord) -> PathBuf {
    trash_dir.join(format!("companion-{}-{}", row.id, row.name))
}

/// Removes the directory of a companion if it is empty now. Never removes a folder
/// that still holds anything.
fn remove_dir_if_empty(path: &Path) {
    if let Some(dir) = path.parent().filter(|d| is_extras_dir_name(d)) {
        let _ = std::fs::remove_dir(dir);
    }
}

/// Moves one companion file into the app trash and marks its row. With
/// `drop_row_if_gone`, a file that no longer exists removes the row instead (nothing
/// could be restored); otherwise the row is only marked, so it comes back with its model.
fn trash_one(conn: &Connection, row: &CompanionRecord, trash_dir: &Path, deleted_at: &str, drop_row_if_gone: bool) -> CmdResult<()> {
    verify_row(row)?;
    let path = Path::new(&row.path);
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return if drop_row_if_gone {
                db::companions::delete_companion(conn, row.id).map_err(Into::into)
            } else {
                db::companions::soft_delete_companion(conn, row.id, None, deleted_at).map_err(Into::into)
            };
        }
        Err(e) => return Err(e.to_string().into()),
        Ok(meta) if !meta.is_file() => return Err(CmdError::expected("Die Begleitdatei ist keine normale Datei.")),
        Ok(_) => {}
    }
    ensure_row_folder_genuine(conn, row)?;
    let target = trash_target(trash_dir, row);
    move_file(path, &target).map_err(|e| e.to_string())?;
    if let Err(db_err) = db::companions::soft_delete_companion(conn, row.id, Some(&target.to_string_lossy()), deleted_at) {
        if let Err(rollback_err) = move_file(&target, path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback aus dem Papierkorb fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}",
                target.display()
            )
            .into());
        }
        return Err(db_err.to_string().into());
    }
    remove_dir_if_empty(path);
    Ok(())
}

pub(super) fn remove_companion_with_conn(conn: &Connection, id: i64, trash_dir: &Path) -> CmdResult<()> {
    let row = load_row(conn, id)?;
    if row.deleted_at.is_some() {
        return Err(CmdError::expected("Die Begleitdatei liegt schon im Papierkorb."));
    }
    trash_one(conn, &row, trash_dir, &chrono::Utc::now().to_rfc3339(), true)
}

/// Sends every active companion of a model to the trash together with it, stamped with
/// the model's `deleted_at` so a restore of the model finds exactly these. A companion
/// that cannot be moved stays where it is and active; the model's removal never fails
/// because of it.
pub(super) fn trash_with_model(conn: &Connection, file_id: i64, trash_dir: &Path, deleted_at: &str) {
    let rows = match db::companions::list_active_companions(conn, file_id) {
        Ok(rows) => rows,
        Err(e) => {
            log::warn!(target: "datei", "Begleitdateien konnten nicht gelesen werden: {e}");
            return;
        }
    };
    for row in rows {
        if let Err(e) = trash_one(conn, &row, trash_dir, deleted_at, false) {
            log::warn!(target: "datei", "Begleitdatei bleibt liegen ({}): {e}", row.path);
        }
    }
}

/// Moves a trashed file back next to `extras`, under a free name, and updates its row.
fn restore_one(conn: &Connection, row: &CompanionRecord, extras: &Path) -> CmdResult<()> {
    let Some(trash_path) = row.trash_path.as_deref() else {
        // Nothing was moved (the file was already gone): the entry just becomes visible.
        return db::companions::restore_companion(conn, row.id, &extras.join(&row.name).to_string_lossy()).map_err(Into::into);
    };
    let created_dir = if is_real_dir(extras) {
        false
    } else {
        std::fs::create_dir(extras).map_err(|e| e.to_string())?;
        true
    };
    let cleanup_dir = || {
        if created_dir {
            let _ = std::fs::remove_dir(extras);
        }
    };
    // Reserve a free name first: move_file never replaces, but the name decides the new path.
    let (target, reserved) = match create_unique(conn, extras, &row.name, Some(row.id)) {
        Ok(created) => created,
        Err(e) => {
            cleanup_dir();
            return Err(e.to_string().into());
        }
    };
    drop(reserved);
    let _ = std::fs::remove_file(&target);
    if let Err(e) = move_file(Path::new(trash_path), &target) {
        cleanup_dir();
        return Err(e.to_string().into());
    }
    let new_name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| row.name.clone());
    // Name, path and trash state change in one statement, so a failure cannot leave the
    // row half restored.
    let db_result = db::companions::restore_companion_as(conn, row.id, &new_name, &target.to_string_lossy());
    if let Err(db_err) = db_result {
        if let Err(rollback_err) = move_file(&target, Path::new(trash_path)) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback in den Papierkorb fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}",
                target.display()
            )
            .into());
        }
        cleanup_dir();
        return Err(db_err.to_string().into());
    }
    Ok(())
}

pub(super) fn restore_companion_with_conn(conn: &Connection, id: i64, sensitive_dirs: &[PathBuf]) -> CmdResult<CompanionRecord> {
    let row = load_row(conn, id)?;
    if row.deleted_at.is_none() {
        return Err(CmdError::expected("Die Begleitdatei liegt nicht im Papierkorb."));
    }
    let model = load_model(conn, row.file_id)?;
    if model.deleted_at.is_some() {
        return Err(CmdError::expected("Das Modell liegt im Papierkorb. Zuerst das Modell wiederherstellen."));
    }
    if db::companions::count_active_companions(conn, model.id)? >= MAX_COMPANIONS_PER_MODEL {
        return Err(CmdError::expected(format!("Ein Modell kann höchstens {MAX_COMPANIONS_PER_MODEL} Begleitdateien haben.")));
    }
    // The stored name decides the new file name and path; make sure it is harmless first.
    folders::validate_name(&row.name, true, folders::NameKind::File)?;
    if lowercase_extension(&row.name).and_then(|e| kind_for_extension(&e)).is_none() {
        return Err(CmdError::expected("Diese Begleitdatei ist ungültig und wird nicht verwendet."));
    }
    let extras = resolve_extras_target(conn, Path::new(&model.path), sensitive_dirs)?;
    restore_one(conn, &row, &extras)?;
    load_row(conn, id)
}

/// Brings back the companions that went to the trash with `model_deleted_at`, after the
/// model itself was restored (its path may have changed). Failures are logged; the file
/// stays in the trash and can be restored on its own.
pub(super) fn restore_with_model(conn: &Connection, file_id: i64, model_deleted_at: &str, sensitive_dirs: &[PathBuf]) {
    let rows = match db::companions::list_trashed_with_model(conn, file_id, model_deleted_at) {
        Ok(rows) if !rows.is_empty() => rows,
        Ok(_) => return,
        Err(e) => {
            log::warn!(target: "datei", "Begleitdateien konnten nicht gelesen werden: {e}");
            return;
        }
    };
    let model = match db::get_file(conn, file_id) {
        Ok(Some(model)) => model,
        _ => return,
    };
    let extras = match resolve_extras_target(conn, Path::new(&model.path), sensitive_dirs) {
        Ok(extras) => extras,
        Err(e) => {
            log::warn!(target: "datei", "Begleitdateien bleiben im Papierkorb: {e}");
            return;
        }
    };
    for row in rows {
        if let Err(e) = folders::validate_name(&row.name, true, folders::NameKind::File).and_then(|_| restore_one(conn, &row, &extras)) {
            log::warn!(target: "datei", "Begleitdatei bleibt im Papierkorb ({}): {e}", row.name);
        }
    }
}

/// Deletes the trash file of a row, but only inside the trash folder: a row from a
/// restored backup must never make the app delete anything elsewhere.
fn remove_trash_file(row: &CompanionRecord, trash_dir: &Path) -> Result<(), String> {
    let Some(trash_path) = row.trash_path.as_deref().filter(|p| !p.trim().is_empty()) else { return Ok(()) };
    let resolved_trash = resolve_path_for_sensitivity_check(trash_dir).map_err(|e| e.message)?;
    if reject_if_outside_trash_dir(Path::new(trash_path), &resolved_trash).is_err() {
        log::warn!(target: "trash", "Begleitdatei außerhalb des Papierkorbs wird nicht gelöscht (ID {})", row.id);
        return Ok(());
    }
    match std::fs::remove_file(trash_path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Removes the trash files of all companions of a model that is about to be deleted for
/// good (its rows follow by cascade). An error means a file could not be removed; the
/// caller then keeps the model entry, like for the model file itself.
pub(super) fn purge_trash_files_of_model(conn: &Connection, file_id: i64, trash_dir: &Path) -> Result<(), String> {
    for row in db::companions::list_trashed_files_of_model(conn, file_id).map_err(|e| e.to_string())? {
        remove_trash_file(&row, trash_dir)?;
    }
    Ok(())
}

fn purge_rows(conn: &Connection, rows: Vec<CompanionRecord>, trash_dir: &Path, target: &str) {
    for row in rows {
        if let Err(e) = remove_trash_file(&row, trash_dir) {
            log::error!(target: "trash", "{target}: Begleitdatei ID {} konnte nicht entfernt werden: {e}", row.id);
            continue;
        }
        if let Err(e) = db::companions::delete_companion(conn, row.id) {
            log::error!(target: "trash", "{target}: Zeile der Begleitdatei ID {} konnte nicht gelöscht werden: {e}", row.id);
        }
    }
}

/// Final removal of every companion in the trash (for "empty trash").
pub(super) fn purge_all_trashed(conn: &Connection, trash_dir: &Path) {
    match db::companions::list_all_trashed_companions(conn) {
        Ok(rows) => purge_rows(conn, rows, trash_dir, "trash"),
        Err(e) => log::error!(target: "trash", "Begleitdateien konnten nicht gelesen werden: {e}"),
    }
}

/// Final removal of companions that have been in the trash longer than the cutoff.
pub(super) fn purge_expired_trashed(conn: &Connection, trash_dir: &Path, cutoff: &str) {
    match db::companions::list_expired_trashed_companions(conn, cutoff) {
        Ok(rows) => purge_rows(conn, rows, trash_dir, "startup"),
        Err(e) => log::error!(target: "startup", "trash cleanup: Begleitdateien konnten nicht gelesen werden: {e}"),
    }
}

/// Moves `source` into `extras` under the first free name (never replacing a file) and
/// returns the final path.
fn move_into_unique(source: &Path, extras: &Path, name: &str) -> std::io::Result<PathBuf> {
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) => (stem, format!(".{extension}")),
        None => (name, String::new()),
    };
    for attempt in 1..=999u32 {
        let candidate = if attempt == 1 { name.to_string() } else { format!("{stem} ({attempt}){extension}") };
        let target = extras.join(candidate);
        match move_file(source, &target) {
            Ok(()) => return Ok(target),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no free name"))
}

/// After an archive was extracted and its models imported: gives the files of an allowed
/// type to the model they belong to. With exactly one model in the archive all of them
/// belong to it; with several, a file belongs to the model that lies in the same archive
/// folder (and only if that folder holds exactly one model). Anything else stays where it
/// is. Returns `(assigned, ignored)`, where `ignored` counts extracted files whose type or
/// content is not allowed; those are never touched.
pub(super) fn assign_from_archive(
    conn: &Connection,
    written: &[PathBuf],
    imported: &[ModelFileDto],
    sensitive_dirs: &[PathBuf],
) -> (u32, u32) {
    let key = |p: &Path| folders::comparison_path(p);
    let model_files: Vec<&PathBuf> = written.iter().filter(|p| files::is_supported_extension(p)).collect();
    let imported_id: HashMap<PathBuf, i64> =
        imported.iter().filter_map(|m| Some((key(Path::new(&m.path)), m.id.parse().ok()?))).collect();
    let owner_of = |candidate: &Path| -> Option<i64> {
        let model = if model_files.len() == 1 {
            model_files[0]
        } else {
            let mut same_folder = model_files.iter().filter(|m| m.parent() == candidate.parent());
            let first = same_folder.next()?;
            if same_folder.next().is_some() {
                return None;
            }
            first
        };
        imported_id.get(&key(model)).copied()
    };

    let mut assigned = 0u32;
    let mut ignored = 0u32;
    let mut extras_of: HashMap<i64, Option<PathBuf>> = HashMap::new();
    let mut count_of: HashMap<i64, i64> = HashMap::new();
    for candidate in written.iter().filter(|p| !files::is_supported_extension(p) && !files::is_ignored_import_path(p)) {
        let Some(name) = candidate.file_name().and_then(|n| n.to_str()) else { continue };
        let Ok(mut file) = crate::safe_file::open_regular(candidate) else { continue };
        let Ok(length) = file.metadata().map(|m| m.len()) else { continue };
        let Ok(head) = read_head(&mut file) else { continue };
        drop(file);
        let kind = match classify(name, &head) {
            Ok(kind) => kind,
            Err(_) => {
                ignored += 1;
                continue;
            }
        };
        if length > MAX_COMPANION_BYTES {
            continue;
        }
        let Some(model_id) = owner_of(candidate) else { continue };
        let Some(model) = db::get_file(conn, model_id).ok().flatten() else { continue };
        let extras = extras_of
            .entry(model_id)
            .or_insert_with(|| match resolve_extras_target(conn, Path::new(&model.path), sensitive_dirs) {
                Ok(extras) => Some(extras),
                Err(e) => {
                    log::warn!(target: "archive", "Begleitdateien ohne Ablage ({}): {}", model.path, e.message);
                    None
                }
            })
            .clone();
        let Some(extras) = extras else { continue };
        let count = count_of.entry(model_id).or_insert_with(|| db::companions::count_active_companions(conn, model_id).unwrap_or(i64::MAX));
        if *count >= MAX_COMPANIONS_PER_MODEL {
            continue;
        }
        let created_dir = !is_real_dir(&extras);
        if created_dir && std::fs::create_dir(&extras).is_err() {
            continue;
        }
        let Some(safe_name) = companion_file_name(candidate) else { continue };
        let target = match move_into_unique(candidate, &extras, &safe_name) {
            Ok(target) => target,
            Err(e) => {
                log::warn!(target: "archive", "Begleitdatei bleibt liegen ({}): {e}", candidate.display());
                if created_dir {
                    let _ = std::fs::remove_dir(&extras);
                }
                continue;
            }
        };
        let stored_name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(safe_name);
        let added_at = chrono::Utc::now().to_rfc3339();
        match db::companions::insert_companion(conn, model_id, &stored_name, &target.to_string_lossy(), kind.as_str(), Some(length as i64), &added_at) {
            Ok(_) => {
                *count += 1;
                assigned += 1;
            }
            Err(e) => {
                log::warn!(target: "archive", "Begleitdatei konnte nicht eingetragen werden: {e}");
                if move_file(&target, candidate).is_err() {
                    log::error!(target: "archive", "Begleitdatei liegt jetzt unter {}", target.display());
                }
            }
        }
    }
    (assigned, ignored)
}

/// Companion files that were moved along with their model, so the move can be undone.
#[derive(Default)]
pub(super) struct Carried {
    moves: Vec<(i64, PathBuf, PathBuf)>,
    old_paths: Vec<(i64, String)>,
    created_dir: Option<PathBuf>,
    old_dir: Option<PathBuf>,
    /// The folder itself was renamed (a rename that only changes letter case).
    renamed_dir: Option<(PathBuf, PathBuf)>,
}

/// On these systems two names that differ only in case are the same folder.
const CASE_INSENSITIVE_PATHS: bool = cfg!(any(windows, target_os = "macos"));

impl Carried {
    /// Everything went through: the old folder goes away if it is empty now.
    pub(super) fn finish(self) {
        if let Some(dir) = self.old_dir.filter(|_| !self.moves.is_empty()) {
            let _ = std::fs::remove_dir(dir);
        }
    }

    /// Puts files and rows back where they were (best effort, failures are logged).
    pub(super) fn undo(self, conn: &Connection) {
        if let Some((from, to)) = &self.renamed_dir {
            if let Err(e) = std::fs::rename(to, from) {
                log::error!(target: "datei", "Begleitdateien-Ordner konnte nicht zurückbenannt werden ({}): {e}", to.display());
            }
        }
        if let Some(old_dir) = &self.old_dir {
            if !self.moves.is_empty() && !old_dir.exists() {
                let _ = std::fs::create_dir(old_dir);
            }
        }
        for (_, from, to) in self.moves.iter().rev() {
            if let Err(e) = move_file(to, from) {
                log::error!(target: "datei", "Begleitdatei konnte nicht zurückgelegt werden ({}): {e}", to.display());
            }
        }
        for (id, path) in &self.old_paths {
            if let Err(e) = db::companions::update_companion_path(conn, *id, path) {
                log::error!(target: "datei", "Pfad der Begleitdatei konnte nicht zurückgesetzt werden: {e}");
            }
        }
        if let Some(dir) = &self.created_dir {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

/// A model moved or was renamed from `old_model` to `new_model` (the model file is
/// already at its new place): its companions follow into the new extras folder. Only
/// the model's own files move, never other files of a folder two models share. Nothing is
/// overwritten; on any failure everything done here is undone and an error returned, so
/// the caller can undo the model move.
pub(super) fn carry(conn: &Connection, file_id: i64, old_model: &Path, new_model: &Path) -> CmdResult<Carried> {
    carry_with(conn, file_id, old_model, new_model, CASE_INSENSITIVE_PATHS)
}

fn carry_with(conn: &Connection, file_id: i64, old_model: &Path, new_model: &Path, case_insensitive: bool) -> CmdResult<Carried> {
    let (Some(old_dir), Some(new_dir)) = (extras_dir_for(old_model), extras_dir_for(new_model)) else {
        return Ok(Carried::default());
    };
    if old_dir == new_dir {
        return Ok(Carried::default());
    }
    // Only rows that really sit in the model's own folder travel with it; a row that
    // points anywhere else is left exactly as it is, so a move never reaches a file
    // outside that folder.
    let rows: Vec<CompanionRecord> = db::companions::list_active_companions(conn, file_id)?
        .into_iter()
        .filter(|row| {
            let inside = Path::new(&row.path).parent().is_some_and(|dir| same_dir_with(dir, &old_dir, case_insensitive));
            if !inside {
                log::warn!(target: "datei", "Begleitdatei liegt nicht im Ordner ihres Modells und wird nicht mitgeführt: {}", row.path);
            }
            inside
        })
        .collect();
    if rows.is_empty() {
        return Ok(Carried::default());
    }
    // Files are only taken out of the genuine old folder; through a link they could belong
    // to someone else.
    if let Some(problem) = extras_dir_problem(&old_dir, &old_dir) {
        return Err(CmdError::coded(problem));
    }
    let mut carried = Carried { old_dir: Some(old_dir.clone()), ..Default::default() };
    let fail = |carried: Carried, error: CmdError| -> CmdResult<Carried> {
        carried.undo(conn);
        Err(error)
    };
    if case_insensitive && old_dir.to_string_lossy().to_lowercase() == new_dir.to_string_lossy().to_lowercase() {
        // The model's new name differs only in case: it is the same folder, which has to be
        // renamed in place (moving the files into "the other" folder would hit themselves).
        // A folder that another model's companions live in is left alone: renaming it
        // would leave that model's rows pointing at a name that no longer exists on
        // volumes that tell the two spellings apart.
        let own = rows.len();
        let in_folder = db::companions::list_active_companion_paths(conn)?
            .iter()
            .filter(|path| Path::new(path).parent().is_some_and(|dir| same_dir_with(dir, &old_dir, case_insensitive)))
            .count();
        if in_folder > own {
            return Ok(Carried::default());
        }
        if is_real_dir(&old_dir) {
            std::fs::rename(&old_dir, &new_dir).map_err(|e| CmdError::from(e.to_string()))?;
            carried.renamed_dir = Some((old_dir.clone(), new_dir.clone()));
        }
        for row in &rows {
            let Some(rest) = row.path.get(old_dir.to_string_lossy().len()..) else { continue };
            let moved_path = format!("{}{rest}", new_dir.to_string_lossy());
            carried.old_paths.push((row.id, row.path.clone()));
            if let Err(e) = db::companions::update_companion_path(conn, row.id, &moved_path) {
                return fail(carried, e.into());
            }
        }
        carried.old_dir = None;
        return Ok(carried);
    }
    let movable: Vec<&CompanionRecord> = rows.iter().filter(|row| is_real_file(Path::new(&row.path))).collect();
    if !movable.is_empty() && !is_real_dir(&new_dir) {
        if std::fs::symlink_metadata(&new_dir).is_ok() {
            return Err(CmdError::coded(ErrorCode::CompanionTargetFolderBlocked));
        }
        if let Err(e) = std::fs::create_dir(&new_dir) {
            return Err(CmdError::coded_with(ErrorCode::CompanionTargetFolderCreateFailed, [e]));
        }
        carried.created_dir = Some(new_dir.clone());
    }
    for row in &rows {
        let old_path = PathBuf::from(&row.path);
        let target = new_dir.join(&row.name);
        carried.old_paths.push((row.id, row.path.clone()));
        if is_real_file(&old_path) {
            if let Err(e) = move_file(&old_path, &target) {
                let error = if e.kind() == std::io::ErrorKind::AlreadyExists {
                    CmdError::coded_with(ErrorCode::CompanionNameTaken, [&row.name])
                } else {
                    CmdError::from(e.to_string())
                };
                // The failed file never moved, so it is not in the list to undo.
                carried.old_paths.pop();
                return fail(carried, error);
            }
            carried.moves.push((row.id, old_path, target.clone()));
        }
        if let Err(e) = db::companions::update_companion_path(conn, row.id, &target.to_string_lossy()) {
            return fail(carried, e.into());
        }
    }
    Ok(carried)
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
        let picked = app.dialog().file().add_filter("Bilder, PDF, Text", PICKER_EXTENSIONS).blocking_pick_files();
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

/// Path and kind of a companion, only if it still passes every check for an allowed type.
pub(super) fn open_info_with_conn(conn: &Connection, id: i64, sensitive_dirs: &[PathBuf]) -> CmdResult<CompanionOpenInfoDto> {
    let row = load_row(conn, id)?;
    let (kind, _, _) = open_checked(conn, &row, sensitive_dirs, MAX_COMPANION_BYTES)?;
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
        open_external(&info.path, environment)
    })
    .await
}

/// The picture as a data URI for the preview. Only images, with a size cap; anything
/// else (or a file whose content is not the claimed picture) is refused.
pub(super) fn image_data_with_conn(conn: &Connection, id: i64, sensitive_dirs: &[PathBuf]) -> CmdResult<String> {
    use base64::Engine;
    let row = load_row(conn, id)?;
    if CompanionKind::parse(&row.kind) != Some(CompanionKind::Image) {
        return Err(CmdError::expected("Diese Begleitdatei ist kein Bild."));
    }
    let (_, mut file, head) = open_checked(conn, &row, sensitive_dirs, MAX_PREVIEW_BYTES)?;
    let mut bytes = head;
    Read::by_ref(&mut file).take(MAX_PREVIEW_BYTES + 1 - bytes.len() as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_PREVIEW_BYTES {
        return Err(CmdError::expected("Das Bild ist für die Vorschau zu groß."));
    }
    Ok(format!("data:{};base64,{}", image_mime(&row.name), base64::engine::general_purpose::STANDARD.encode(bytes)))
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
