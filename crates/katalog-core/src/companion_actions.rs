//! Companion-file actions of the desktop catalog (issue #20, package P3): add, trash, restore,
//! purge (delete for good), assign from an archive, and the carry that makes the companions
//! follow their model when it is moved or renamed. Pictures, PDFs and notes live in a
//! `<model name>_extras` folder next to the model; the database only records them.
//!
//! The desktop commands keep the dialogs, locks and DTOs; this module has no `fsops` /
//! `upload_fs` dependency, so it builds on Windows and macOS. Every disk access that changes
//! something goes through [`CompanionFs`], which the desktop implements on top of its
//! file-system boundary (`Fs`, P0a). Tests use a fake that fails in step n.
//!
//! Whether two spellings of a folder name are the same folder (macOS, Windows) is the
//! `case_insensitive` parameter of [`carry`], so Linux tests cover both branches.
use std::collections::HashMap;
use std::fs::{File, Metadata};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::companion_read::{self, FolderProblem, RowError};
use crate::companions::{
    classify, extras_dir_for, has_parent_component, is_extras_dir_name, is_text_name, kind_for_extension, lowercase_extension, validate_stored_fields,
    CompanionKind, Refusal, MAX_COMPANIONS_PER_MODEL, MAX_COMPANION_BYTES,
};
use crate::db;
use crate::db::models::CompanionRecord;
use crate::error_code::ErrorCode;
use crate::folder_create::find_catalog_parent;
use crate::import::file_names::{validate_name, NameKind};
use crate::import::files::{is_ignored_import_path, is_model_extension};
use crate::import::placement::{comparison_path, reject_if_sensitive_path, resolve_path_for_sensitivity_check};
use crate::import::{ImportError as CmdError, ImportResult as CmdResult};
use crate::read::ModelFileDto;

/// The disk operations the companion actions use.
pub trait CompanionFs {
    /// Same as `std::fs::symlink_metadata`.
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata>;
    /// Same as `Path::exists` (follows links, any error is `false`).
    fn exists(&self, path: &Path) -> bool;
    /// Same as `std::fs::canonicalize`.
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    /// Same as `std::fs::create_dir` (not `create_dir_all`).
    fn create_dir(&self, path: &Path) -> io::Result<()>;
    fn remove_dir(&self, path: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Creates a new file for writing; never replaces and never follows a link at `path`.
    fn create_new(&self, path: &Path) -> io::Result<File>;
    /// A move that never replaces its target (hard link first, copy as fallback), see
    /// `import::placement::move_file`.
    fn move_file(&self, from: &Path, to: &Path) -> io::Result<()>;
}

/// `std::fs`, 1:1.
pub struct StdCompanionFs;

impl CompanionFs for StdCompanionFs {
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
        std::fs::symlink_metadata(path)
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir(path)
    }
    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir(path)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
    fn create_new(&self, path: &Path) -> io::Result<File> {
        std::fs::OpenOptions::new().write(true).create_new(true).open(path)
    }
    fn move_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        crate::import::placement::move_file(from, to)
    }
}

/// The user-facing text for a refusal of a type or content.
pub fn refusal_message(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::Type => "Dieser Dateityp ist als Begleitdatei nicht erlaubt. Erlaubt sind Bilder (PNG, JPG, GIF, WebP), PDF, Textdateien (TXT, MD) und weitere Formate (FCStd, SCAD, DXF, CSV, DOCX, ODT).",
        Refusal::Content => "Der Inhalt der Datei passt nicht zu ihrem Dateityp und wird nicht übernommen.",
    }
}

pub fn folder_error_code(problem: FolderProblem) -> ErrorCode {
    match problem {
        FolderProblem::Unverifiable => ErrorCode::CompanionFolderUnverifiable,
        FolderProblem::NotReal => ErrorCode::CompanionFolderNotReal,
        FolderProblem::Elsewhere => ErrorCode::CompanionFolderElsewhere,
        FolderProblem::ModelPathInvalid => ErrorCode::ModelPathInvalid,
    }
}

/// The user-facing text for a refusal to open a stored companion.
pub fn row_error(error: RowError) -> CmdError {
    match error {
        RowError::Invalid => CmdError::expected("Diese Begleitdatei ist ungültig und wird nicht verwendet."),
        RowError::Trashed => CmdError::expected("Diese Begleitdatei liegt im Papierkorb."),
        RowError::ModelMissing => CmdError::expected("Modell nicht gefunden."),
        RowError::ModelTrashed => CmdError::expected("Das Modell liegt im Papierkorb."),
        RowError::ModelPathInvalid => CmdError::expected("Der Pfad des Modells ist nicht gültig."),
        RowError::NotInModelFolder => CmdError::expected("Die Begleitdatei liegt nicht im Begleitdateien-Ordner ihres Modells."),
        RowError::Folder(problem) => CmdError::coded(folder_error_code(problem)),
        RowError::NotAFile => CmdError::expected("Die Begleitdatei existiert nicht mehr oder ist keine normale Datei."),
        RowError::Unreadable => CmdError::expected("Die Begleitdatei kann nicht gelesen werden."),
        RowError::TooLarge => CmdError::expected("Die Begleitdatei ist zu groß."),
        RowError::Content(refusal) => CmdError::expected(refusal_message(refusal)),
    }
}

/// The folder of a stored companion must be exactly the genuine `_extras` folder of the
/// model the row belongs to, before any file in it is moved, opened or read.
pub fn ensure_row_folder_genuine(conn: &Connection, row: &CompanionRecord) -> CmdResult<()> {
    companion_read::check_row_folder(conn, row).map(|_| ()).map_err(row_error)
}

/// Checks a stored row before anything touches its file.
fn verify_row(row: &CompanionRecord) -> CmdResult<CompanionKind> {
    validate_stored_fields(&row.name, &row.path, &row.kind)
        .map_err(|_| CmdError::expected("Diese Begleitdatei ist ungültig und wird nicht verwendet."))
}

/// A harmless single-component name for the copy of a user-chosen file; the rule itself
/// lives in `companions`, shared with the web upload.
fn companion_file_name(source: &Path) -> Option<String> {
    crate::companions::companion_file_name(&source.file_name()?.to_string_lossy())
}

fn load_row(conn: &Connection, id: i64) -> CmdResult<CompanionRecord> {
    db::companions::get_companion(conn, id)?.ok_or_else(|| CmdError::expected("Begleitdatei nicht gefunden."))
}

fn load_model(conn: &Connection, file_id: i64) -> CmdResult<db::models::FileRecord> {
    db::get_file(conn, file_id)?.ok_or_else(|| CmdError::expected("Modell nicht gefunden."))
}

fn is_real_dir(fs: &dyn CompanionFs, path: &Path) -> bool {
    fs.symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

fn is_real_file(fs: &dyn CompanionFs, path: &Path) -> bool {
    fs.symlink_metadata(path).is_ok_and(|m| m.is_file())
}

/// `name`, `name (2)`, ... as the candidate for attempt 1, 2, ...
fn candidate_name(stem: &str, extension: &str, name: &str, attempt: u32) -> String {
    if attempt == 1 {
        name.to_string()
    } else {
        format!("{stem} ({attempt}){extension}")
    }
}

fn split_extension(name: &str) -> (&str, String) {
    match name.rsplit_once('.') {
        Some((stem, extension)) => (stem, format!(".{extension}")),
        None => (name, String::new()),
    }
}

/// Creates a new file under the first free `name`, `name (2)`, ... Never replaces and
/// never follows a symlink at the target (create_new). A path that a catalog row
/// already holds (a trashed companion keeps its old path) counts as taken too, because
/// the row's path is unique; `own_row` is the row that is being restored, which may
/// take its own old path back.
fn create_unique(conn: &Connection, fs: &dyn CompanionFs, dir: &Path, name: &str, own_row: Option<i64>) -> io::Result<(PathBuf, File)> {
    let (stem, extension) = split_extension(name);
    for attempt in 1..=999u32 {
        let path = dir.join(candidate_name(stem, &extension, name, attempt));
        let held = db::companions::path_held_by_other_row(conn, &path.to_string_lossy(), own_row).map_err(|e| io::Error::other(e.to_string()))?;
        if held {
            continue;
        }
        match fs.create_new(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "no free name"))
}

/// The extras folder of a model, after checking that it may be written to: next to
/// the model, free of `..`, outside protected folders, below a catalog folder (also
/// after resolving symlinks) and not itself a symlink.
pub fn resolve_extras_target(conn: &Connection, fs: &dyn CompanionFs, model_path: &Path, sensitive_dirs: &[PathBuf]) -> CmdResult<PathBuf> {
    if has_parent_component(model_path) || !model_path.is_absolute() {
        return Err(CmdError::expected("Der Pfad des Modells ist nicht gültig."));
    }
    let extras = extras_dir_for(model_path).ok_or_else(|| CmdError::expected("Der Pfad des Modells ist nicht gültig."))?;
    let extras_name = extras.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    validate_name(&extras_name, true, NameKind::Folder)?;
    reject_if_sensitive_path(&extras, sensitive_dirs)?;
    let parent = extras.parent().ok_or_else(|| CmdError::expected("Der Pfad des Modells ist nicht gültig."))?;
    let resolved_parent = fs
        .canonicalize(parent)
        .map_err(|_| CmdError::expected("Der Ordner des Modells existiert auf der Festplatte nicht mehr."))?;
    let catalog_folders = db::list_folders(conn)?;
    let owner = find_catalog_parent(&extras, &catalog_folders)
        .and_then(|id| catalog_folders.iter().find(|f| f.id == id))
        .and_then(|f| fs.canonicalize(Path::new(&f.path)).ok());
    let inside_catalog = owner.is_some_and(|root| comparison_path(&resolved_parent).starts_with(comparison_path(&root)));
    if !inside_catalog {
        return Err(CmdError::expected("Das Modell liegt nicht in einem Katalogordner. Begleitdateien können nur dort abgelegt werden."));
    }
    if let Some(problem) = extras_dir_problem(&extras, &extras) {
        return Err(CmdError::coded(problem));
    }
    Ok(extras)
}

/// Why `dir` cannot be trusted as the companion folder `expected` (the `_extras` folder
/// next to a model). The check itself lives in `companion_read`, shared with the web server.
fn extras_dir_problem(dir: &Path, expected: &Path) -> Option<ErrorCode> {
    companion_read::extras_dir_problem(dir, expected).map(folder_error_code)
}

/// Copies a user-chosen file into the model's extras folder. The source is only read.
pub fn add_companion(conn: &Connection, fs: &dyn CompanionFs, file_id: i64, source: &Path, sensitive_dirs: &[PathBuf]) -> CmdResult<CompanionRecord> {
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
    match fs.symlink_metadata(source) {
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
    let head = companion_read::read_head(&mut input).map_err(|_| CmdError::expected("Die Datei kann nicht gelesen werden."))?;
    let kind = classify(source_name, &head).map_err(|r| CmdError::expected(refusal_message(r)))?;
    let name = companion_file_name(source).ok_or_else(|| CmdError::expected("Der Dateiname ist nicht verwendbar."))?;
    // A text file is valid from its first to its last byte, not just in the inspected head.
    if is_text_name(&name) {
        use std::io::Seek;
        let mut scan = crate::companions::TextScan::default();
        let mut ok = scan.feed(&head);
        let mut buffer = vec![0u8; 64 * 1024];
        let mut limited = Read::by_ref(&mut input).take(MAX_COMPANION_BYTES + 1 - head.len() as u64);
        while ok {
            let n = limited.read(&mut buffer).map_err(|_| CmdError::expected("Die Datei kann nicht gelesen werden."))?;
            if n == 0 {
                break;
            }
            ok = scan.feed(&buffer[..n]);
        }
        if !ok || !scan.finish() {
            return Err(CmdError::expected(refusal_message(Refusal::Content)));
        }
        input.seek(io::SeekFrom::Start(head.len() as u64)).map_err(|_| CmdError::expected("Die Datei kann nicht gelesen werden."))?;
    }

    let extras = resolve_extras_target(conn, fs, Path::new(&model.path), sensitive_dirs)?;
    let created_dir = if is_real_dir(fs, &extras) {
        false
    } else {
        fs.create_dir(&extras).map_err(|e| format!("Begleitdateien-Ordner konnte nicht angelegt werden: {e}"))?;
        true
    };
    let cleanup_dir = || {
        if created_dir {
            let _ = fs.remove_dir(&extras);
        }
    };
    let (target, mut output) = match create_unique(conn, fs, &extras, &name, None) {
        Ok(created) => created,
        Err(e) => {
            cleanup_dir();
            return Err(e.to_string().into());
        }
    };
    let copied = (|| -> io::Result<u64> {
        output.write_all(&head)?;
        // The size can grow after the check above; the cap keeps the copy finite.
        let rest = io::copy(&mut Read::by_ref(&mut input).take(MAX_COMPANION_BYTES + 1 - head.len() as u64), &mut output)?;
        output.sync_all()?;
        Ok(head.len() as u64 + rest)
    })();
    drop(output);
    let size = match copied {
        Ok(size) if size <= MAX_COMPANION_BYTES => size,
        other => {
            let _ = fs.remove_file(&target);
            cleanup_dir();
            return Err(match other {
                Ok(_) => CmdError::expected("Die Datei ist zu groß."),
                Err(e) => e.to_string().into(),
            });
        }
    };

    let stored_name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(name);
    let added_at = chrono::Utc::now().to_rfc3339();
    let id = match db::companions::insert_companion(conn, file_id, &stored_name, &target.to_string_lossy(), kind.as_str(), Some(size as i64), &added_at) {
        Ok(id) => id,
        Err(e) => {
            let _ = fs.remove_file(&target);
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
fn remove_dir_if_empty(fs: &dyn CompanionFs, path: &Path) {
    if let Some(dir) = path.parent().filter(|d| is_extras_dir_name(d)) {
        let _ = fs.remove_dir(dir);
    }
}

/// Moves one companion file into the app trash and marks its row. With
/// `drop_row_if_gone`, a file that no longer exists removes the row instead (nothing
/// could be restored); otherwise the row is only marked, so it comes back with its model.
fn trash_one(fs: &dyn CompanionFs, conn: &Connection, row: &CompanionRecord, trash_dir: &Path, deleted_at: &str, drop_row_if_gone: bool) -> CmdResult<()> {
    verify_row(row)?;
    let path = Path::new(&row.path);
    match fs.symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
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
    fs.move_file(path, &target).map_err(|e| e.to_string())?;
    if let Err(db_err) = db::companions::soft_delete_companion(conn, row.id, Some(&target.to_string_lossy()), deleted_at) {
        if let Err(rollback_err) = fs.move_file(&target, path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback aus dem Papierkorb fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}",
                target.display()
            )
            .into());
        }
        return Err(db_err.to_string().into());
    }
    remove_dir_if_empty(fs, path);
    Ok(())
}

/// Moves one companion into the trash (the "remove" button).
pub fn remove_companion(conn: &Connection, fs: &dyn CompanionFs, id: i64, trash_dir: &Path) -> CmdResult<()> {
    let row = load_row(conn, id)?;
    if row.deleted_at.is_some() {
        return Err(CmdError::expected("Die Begleitdatei liegt schon im Papierkorb."));
    }
    trash_one(fs, conn, &row, trash_dir, &chrono::Utc::now().to_rfc3339(), true)
}

/// Sends every active companion of a model to the trash together with it, stamped with
/// the model's `deleted_at` so a restore of the model finds exactly these. A companion
/// that cannot be moved stays where it is and active; the model's removal never fails
/// because of it.
pub fn trash_with_model(fs: &dyn CompanionFs, conn: &Connection, file_id: i64, trash_dir: &Path, deleted_at: &str) {
    let rows = match db::companions::list_active_companions(conn, file_id) {
        Ok(rows) => rows,
        Err(e) => {
            log::warn!(target: "datei", "Begleitdateien konnten nicht gelesen werden: {e}");
            return;
        }
    };
    for row in rows {
        if let Err(e) = trash_one(fs, conn, &row, trash_dir, deleted_at, false) {
            log::warn!(target: "datei", "Begleitdatei bleibt liegen ({}): {e}", row.path);
        }
    }
}

/// Moves a trashed file back next to `extras`, under a free name, and updates its row.
fn restore_one(fs: &dyn CompanionFs, conn: &Connection, row: &CompanionRecord, extras: &Path) -> CmdResult<()> {
    let Some(trash_path) = row.trash_path.as_deref() else {
        // Nothing was moved (the file was already gone): the entry just becomes visible.
        return db::companions::restore_companion(conn, row.id, &extras.join(&row.name).to_string_lossy()).map_err(Into::into);
    };
    let created_dir = if is_real_dir(fs, extras) {
        false
    } else {
        fs.create_dir(extras).map_err(|e| e.to_string())?;
        true
    };
    let cleanup_dir = || {
        if created_dir {
            let _ = fs.remove_dir(extras);
        }
    };
    // Reserve a free name first: move_file never replaces, but the name decides the new path.
    let (target, reserved) = match create_unique(conn, fs, extras, &row.name, Some(row.id)) {
        Ok(created) => created,
        Err(e) => {
            cleanup_dir();
            return Err(e.to_string().into());
        }
    };
    drop(reserved);
    let _ = fs.remove_file(&target);
    if let Err(e) = fs.move_file(Path::new(trash_path), &target) {
        cleanup_dir();
        return Err(e.to_string().into());
    }
    let new_name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| row.name.clone());
    // Name, path and trash state change in one statement, so a failure cannot leave the
    // row half restored.
    let db_result = db::companions::restore_companion_as(conn, row.id, &new_name, &target.to_string_lossy());
    if let Err(db_err) = db_result {
        if let Err(rollback_err) = fs.move_file(&target, Path::new(trash_path)) {
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

/// Brings a single companion back from the trash.
pub fn restore_companion(conn: &Connection, fs: &dyn CompanionFs, id: i64, sensitive_dirs: &[PathBuf]) -> CmdResult<CompanionRecord> {
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
    validate_name(&row.name, true, NameKind::File)?;
    if lowercase_extension(&row.name).and_then(|e| kind_for_extension(&e)).is_none() {
        return Err(CmdError::expected("Diese Begleitdatei ist ungültig und wird nicht verwendet."));
    }
    let extras = resolve_extras_target(conn, fs, Path::new(&model.path), sensitive_dirs)?;
    restore_one(fs, conn, &row, &extras)?;
    load_row(conn, id)
}

/// Brings back the companions that went to the trash with `model_deleted_at`, after the
/// model itself was restored (its path may have changed). Failures are logged; the file
/// stays in the trash and can be restored on its own.
pub fn restore_with_model(fs: &dyn CompanionFs, conn: &Connection, file_id: i64, model_deleted_at: &str, sensitive_dirs: &[PathBuf]) {
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
    let extras = match resolve_extras_target(conn, fs, Path::new(&model.path), sensitive_dirs) {
        Ok(extras) => extras,
        Err(e) => {
            log::warn!(target: "datei", "Begleitdateien bleiben im Papierkorb: {e}");
            return;
        }
    };
    for row in rows {
        if let Err(e) = validate_name(&row.name, true, NameKind::File).and_then(|_| restore_one(fs, conn, &row, &extras)) {
            log::warn!(target: "datei", "Begleitdatei bleibt im Papierkorb ({}): {e}", row.name);
        }
    }
}

/// The web version keeps its trash in `<root>/.katalog-papierkorb` and names a
/// companion there `companion-<row id>-...`. Such a file is ours to delete: otherwise a
/// model deleted for good here would leave it behind with no row to find it by. The
/// folder must be a real folder (no link), and only the row's own name qualifies.
fn is_web_trash_file(fs: &dyn CompanionFs, row_id: i64, path: &Path) -> bool {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else { return false };
    dir.file_name().is_some_and(|n| n == ".katalog-papierkorb")
        && name.starts_with(&format!("companion-{row_id}-"))
        && fs.symlink_metadata(dir).is_ok_and(|m| m.is_dir())
}

/// Containment instead of a denylist: `path` must lie inside the trash. Otherwise
/// a backup could set `trash_path = ~/Documents/important.pdf` with an old
/// `deleted_at`, and the startup cleanup would delete the file on the next start.
/// Both sides are resolved (`..`, symlinks).
pub fn reject_if_outside_trash_dir(path: &Path, resolved_trash_dir: &Path) -> CmdResult<()> {
    let resolved = resolve_path_for_sensitivity_check(path)?;
    if resolved == resolved_trash_dir || !resolved.starts_with(resolved_trash_dir) {
        return Err(format!(
            "Papierkorb-Pfad liegt ausserhalb des Papierkorb-Verzeichnisses ({}) und wird abgelehnt",
            resolved_trash_dir.display()
        )
        .into());
    }
    Ok(())
}

/// Deletes the trash file of a row, but only inside the trash folder: a row from a
/// restored backup must never make the app delete anything elsewhere.
fn remove_trash_file(fs: &dyn CompanionFs, row: &CompanionRecord, trash_dir: &Path) -> Result<(), String> {
    let Some(trash_path) = row.trash_path.as_deref().filter(|p| !p.trim().is_empty()) else { return Ok(()) };
    let resolved_trash = resolve_path_for_sensitivity_check(trash_dir).map_err(|e| e.message)?;
    if reject_if_outside_trash_dir(Path::new(trash_path), &resolved_trash).is_err() && !is_web_trash_file(fs, row.id, Path::new(trash_path)) {
        log::warn!(target: "trash", "Begleitdatei außerhalb des Papierkorbs wird nicht gelöscht (ID {})", row.id);
        return Err("Die Begleitdatei liegt außerhalb des Papierkorbs und wird nicht gelöscht".into());
    }
    match fs.remove_file(Path::new(trash_path)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Removes the trash files of all companions of a model that is about to be deleted for
/// good (its rows follow by cascade). An error means a file could not be removed; the
/// caller then keeps the model entry, like for the model file itself.
pub fn purge_trash_files_of_model(fs: &dyn CompanionFs, conn: &Connection, file_id: i64, trash_dir: &Path) -> Result<(), String> {
    for row in db::companions::list_trashed_files_of_model(conn, file_id).map_err(|e| e.to_string())? {
        remove_trash_file(fs, &row, trash_dir)?;
    }
    Ok(())
}

fn purge_rows(fs: &dyn CompanionFs, conn: &Connection, rows: Vec<CompanionRecord>, trash_dir: &Path, target: &str) {
    for row in rows {
        if let Err(e) = remove_trash_file(fs, &row, trash_dir) {
            log::error!(target: "trash", "{target}: Begleitdatei ID {} konnte nicht entfernt werden: {e}", row.id);
            continue;
        }
        if let Err(e) = db::companions::delete_companion(conn, row.id) {
            log::error!(target: "trash", "{target}: Zeile der Begleitdatei ID {} konnte nicht gelöscht werden: {e}", row.id);
        }
    }
}

/// Final removal of every companion in the trash (for "empty trash").
pub fn purge_all_trashed(fs: &dyn CompanionFs, conn: &Connection, trash_dir: &Path) {
    match db::companions::list_all_trashed_companions(conn) {
        Ok(rows) => purge_rows(fs, conn, rows, trash_dir, "trash"),
        Err(e) => log::error!(target: "trash", "Begleitdateien konnten nicht gelesen werden: {e}"),
    }
}

/// Final removal of companions that have been in the trash longer than the cutoff.
pub fn purge_expired_trashed(fs: &dyn CompanionFs, conn: &Connection, trash_dir: &Path, cutoff: &str) {
    match db::companions::list_expired_trashed_companions(conn, cutoff) {
        Ok(rows) => purge_rows(fs, conn, rows, trash_dir, "startup"),
        Err(e) => log::error!(target: "startup", "trash cleanup: Begleitdateien konnten nicht gelesen werden: {e}"),
    }
}

/// Moves `source` into `extras` under the first free name (never replacing a file) and
/// returns the final path.
fn move_into_unique(fs: &dyn CompanionFs, source: &Path, extras: &Path, name: &str) -> io::Result<PathBuf> {
    let (stem, extension) = split_extension(name);
    for attempt in 1..=999u32 {
        let target = extras.join(candidate_name(stem, &extension, name, attempt));
        match fs.move_file(source, &target) {
            Ok(()) => return Ok(target),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "no free name"))
}

/// After an archive was extracted and its models imported: gives the files of an allowed
/// type to the model they belong to. With exactly one model in the archive all of them
/// belong to it; with several, a file belongs to the model that lies in the same archive
/// folder (and only if that folder holds exactly one model). Anything else stays where it
/// is. Returns `(assigned, ignored)`, where `ignored` counts extracted files whose type or
/// content is not allowed; those are never touched.
pub fn assign_from_archive(
    fs: &dyn CompanionFs,
    conn: &Connection,
    written: &[PathBuf],
    imported: &[ModelFileDto],
    sensitive_dirs: &[PathBuf],
) -> (u32, u32) {
    let key = |p: &Path| comparison_path(p);
    let model_files: Vec<&PathBuf> = written.iter().filter(|p| is_model_extension(p)).collect();
    let imported_id: HashMap<PathBuf, i64> = imported.iter().filter_map(|m| Some((key(Path::new(&m.path)), m.id.parse().ok()?))).collect();
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
    for candidate in written.iter().filter(|p| !is_model_extension(p) && !is_ignored_import_path(p)) {
        let Some(name) = candidate.file_name().and_then(|n| n.to_str()) else { continue };
        let Ok(mut file) = crate::safe_file::open_regular(candidate) else { continue };
        let Ok(length) = file.metadata().map(|m| m.len()) else { continue };
        let Ok(head) = companion_read::read_head(&mut file) else { continue };
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
            .or_insert_with(|| match resolve_extras_target(conn, fs, Path::new(&model.path), sensitive_dirs) {
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
        let created_dir = !is_real_dir(fs, &extras);
        if created_dir && fs.create_dir(&extras).is_err() {
            continue;
        }
        let Some(safe_name) = companion_file_name(candidate) else { continue };
        let target = match move_into_unique(fs, candidate, &extras, &safe_name) {
            Ok(target) => target,
            Err(e) => {
                log::warn!(target: "archive", "Begleitdatei bleibt liegen ({}): {e}", candidate.display());
                if created_dir {
                    let _ = fs.remove_dir(&extras);
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
                if fs.move_file(&target, candidate).is_err() {
                    log::error!(target: "archive", "Begleitdatei liegt jetzt unter {}", target.display());
                }
            }
        }
    }
    (assigned, ignored)
}

/// Companion files that were moved along with their model, so the move can be undone.
#[derive(Default)]
pub struct Carried {
    moves: Vec<(i64, PathBuf, PathBuf)>,
    old_paths: Vec<(i64, String)>,
    created_dir: Option<PathBuf>,
    old_dir: Option<PathBuf>,
    /// The folder itself was renamed (a rename that only changes letter case).
    renamed_dir: Option<(PathBuf, PathBuf)>,
}

impl Carried {
    /// Everything went through: the old folder goes away if it is empty now.
    pub fn finish(self, fs: &dyn CompanionFs) {
        if let Some(dir) = self.old_dir.filter(|_| !self.moves.is_empty()) {
            let _ = fs.remove_dir(&dir);
        }
    }

    /// Puts files and rows back where they were (best effort, failures are logged).
    pub fn undo(self, fs: &dyn CompanionFs, conn: &Connection) {
        if let Some((from, to)) = &self.renamed_dir {
            if let Err(e) = fs.rename(to, from) {
                log::error!(target: "datei", "Begleitdateien-Ordner konnte nicht zurückbenannt werden ({}): {e}", to.display());
            }
        }
        if let Some(old_dir) = &self.old_dir {
            if !self.moves.is_empty() && !fs.exists(old_dir) {
                let _ = fs.create_dir(old_dir);
            }
        }
        for (_, from, to) in self.moves.iter().rev() {
            if let Err(e) = fs.move_file(to, from) {
                log::error!(target: "datei", "Begleitdatei konnte nicht zurückgelegt werden ({}): {e}", to.display());
            }
        }
        for (id, path) in &self.old_paths {
            if let Err(e) = db::companions::update_companion_path(conn, *id, path) {
                log::error!(target: "datei", "Pfad der Begleitdatei konnte nicht zurückgesetzt werden: {e}");
            }
        }
        if let Some(dir) = &self.created_dir {
            let _ = fs.remove_dir(dir);
        }
    }
}

/// A model moved or was renamed from `old_model` to `new_model` (the model file is
/// already at its new place): its companions follow into the new extras folder. Only
/// the model's own files move, never other files of a folder two models share. Nothing is
/// overwritten; on any failure everything done here is undone and an error returned, so
/// the caller can undo the model move. `case_insensitive` says that two names which differ
/// only in letter case are the same folder (macOS, Windows); the returned [`Carried`] must
/// be finished or undone with the same file system.
pub fn carry(
    fs: &dyn CompanionFs,
    conn: &Connection,
    file_id: i64,
    old_model: &Path,
    new_model: &Path,
    case_insensitive: bool,
) -> CmdResult<Carried> {
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
            let inside = Path::new(&row.path).parent().is_some_and(|dir| companion_read::same_dir_with(dir, &old_dir, case_insensitive));
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
        carried.undo(fs, conn);
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
            .filter(|path| Path::new(path).parent().is_some_and(|dir| companion_read::same_dir_with(dir, &old_dir, case_insensitive)))
            .count();
        if in_folder > own {
            return Ok(Carried::default());
        }
        if is_real_dir(fs, &old_dir) {
            fs.rename(&old_dir, &new_dir).map_err(|e| CmdError::from(e.to_string()))?;
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
    let movable: Vec<&CompanionRecord> = rows.iter().filter(|row| is_real_file(fs, Path::new(&row.path))).collect();
    if !movable.is_empty() && !is_real_dir(fs, &new_dir) {
        if fs.symlink_metadata(&new_dir).is_ok() {
            return Err(CmdError::coded(ErrorCode::CompanionTargetFolderBlocked));
        }
        if let Err(e) = fs.create_dir(&new_dir) {
            return Err(CmdError::coded_with(ErrorCode::CompanionTargetFolderCreateFailed, [e]));
        }
        carried.created_dir = Some(new_dir.clone());
    }
    for row in &rows {
        let old_path = PathBuf::from(&row.path);
        let target = new_dir.join(&row.name);
        carried.old_paths.push((row.id, row.path.clone()));
        if is_real_file(fs, &old_path) {
            if let Err(e) = fs.move_file(&old_path, &target) {
                let error = if e.kind() == io::ErrorKind::AlreadyExists {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::to_dto;
    use crate::unique_test_dir;
    use std::cell::{Cell, RefCell};
    use std::io::ErrorKind::{PermissionDenied, Unsupported};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    struct Fault {
        op: &'static str,
        /// Only calls whose first path contains this text count.
        needle: Option<String>,
        nth: usize,
        kind: io::ErrorKind,
        seen: Cell<usize>,
    }

    /// Real files in a temp dir; the n-th matching call of one operation fails.
    #[derive(Default)]
    struct FaultFs {
        faults: Vec<Fault>,
        calls: RefCell<Vec<(&'static str, PathBuf)>>,
    }

    impl FaultFs {
        fn fail(mut self, op: &'static str, needle: Option<&str>, nth: usize, kind: io::ErrorKind) -> Self {
            self.faults.push(Fault { op, needle: needle.map(str::to_owned), nth, kind, seen: Cell::new(0) });
            self
        }
        fn step(&self, op: &'static str, path: &Path) -> io::Result<()> {
            self.calls.borrow_mut().push((op, path.to_path_buf()));
            let text = path.to_string_lossy();
            let mut result = Ok(());
            for fault in &self.faults {
                if fault.op != op || fault.needle.as_deref().is_some_and(|n| !text.contains(n)) {
                    continue;
                }
                fault.seen.set(fault.seen.get() + 1);
                if fault.seen.get() == fault.nth && result.is_ok() {
                    result = Err(io::Error::new(fault.kind, format!("injected {op} #{} on {text}", fault.nth)));
                }
            }
            result
        }
        fn count(&self, op: &str) -> usize {
            self.calls.borrow().iter().filter(|c| c.0 == op).count()
        }
    }

    impl CompanionFs for FaultFs {
        fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
            self.step("symlink_metadata", path)?;
            StdCompanionFs.symlink_metadata(path)
        }
        fn exists(&self, path: &Path) -> bool {
            self.calls.borrow_mut().push(("exists", path.to_path_buf()));
            StdCompanionFs.exists(path)
        }
        fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
            self.step("canonicalize", path)?;
            StdCompanionFs.canonicalize(path)
        }
        fn create_dir(&self, path: &Path) -> io::Result<()> {
            self.step("create_dir", path)?;
            StdCompanionFs.create_dir(path)
        }
        fn remove_dir(&self, path: &Path) -> io::Result<()> {
            self.step("remove_dir", path)?;
            StdCompanionFs.remove_dir(path)
        }
        fn remove_file(&self, path: &Path) -> io::Result<()> {
            self.step("remove_file", path)?;
            StdCompanionFs.remove_file(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.step("rename", from)?;
            StdCompanionFs.rename(from, to)
        }
        fn create_new(&self, path: &Path) -> io::Result<File> {
            self.step("create_new", path)?;
            StdCompanionFs.create_new(path)
        }
        fn move_file(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.step("move_file", from)?;
            StdCompanionFs.move_file(from, to)
        }
    }

    struct Fx {
        base: PathBuf,
        root: PathBuf,
        trash: PathBuf,
        conn: Connection,
        folder: i64,
    }

    impl Drop for Fx {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    impl Fx {
        fn new(tag: &str) -> Self {
            let base = unique_test_dir(&format!("p3_core_{tag}"));
            let base = base.canonicalize().unwrap_or(base);
            let (root, trash) = (base.join("catalog"), base.join("trash"));
            std::fs::create_dir_all(&root).unwrap();
            std::fs::create_dir_all(&trash).unwrap();
            std::fs::create_dir_all(base.join("outside")).unwrap();
            let conn = db::connect_in_memory().unwrap();
            let folder = db::insert_folder_with_parent(&conn, "catalog", None, &root.to_string_lossy()).unwrap();
            Self { base, root, trash, conn, folder }
        }
        fn model(&self, name: &str) -> (i64, PathBuf) {
            let path = self.root.join(name);
            std::fs::write(&path, b"model").unwrap();
            (db::test_insert_minimal_file(&self.conn, &path.to_string_lossy(), Some(self.folder)).unwrap(), path)
        }
        fn add(&self, model: i64, name: &str) -> CompanionRecord {
            let source = self.base.join("outside").join(name);
            std::fs::write(&source, PNG).unwrap();
            add_companion(&self.conn, &StdCompanionFs, model, &source, &[]).unwrap()
        }
        fn row(&self, id: i64) -> CompanionRecord {
            db::companions::get_companion(&self.conn, id).unwrap().unwrap()
        }
        fn block(&self, trigger: &str) {
            self.conn.execute_batch(trigger).unwrap();
        }
    }

    const BLOCK_PATH: &str = "CREATE TRIGGER block_path BEFORE UPDATE OF path ON companion_files BEGIN SELECT RAISE(ABORT, 'blocked'); END;";
    const BLOCK_TRASH: &str = "CREATE TRIGGER block_trash BEFORE UPDATE OF deleted_at ON companion_files BEGIN SELECT RAISE(ABORT, 'blocked'); END;";
    const BLOCK_INSERT: &str = "CREATE TRIGGER block_insert BEFORE INSERT ON companion_files BEGIN SELECT RAISE(ABORT, 'blocked'); END;";

    #[test]
    fn candidate_names_count_up_before_the_extension() {
        assert_eq!(split_extension("pic.png"), ("pic", ".png".to_string()));
        assert_eq!(split_extension("noext"), ("noext", String::new()));
        assert_eq!(candidate_name("pic", ".png", "pic.png", 1), "pic.png");
        assert_eq!(candidate_name("pic", ".png", "pic.png", 3), "pic (3).png");
    }

    #[test]
    fn add_stops_when_the_extras_folder_cannot_be_created() {
        let f = Fx::new("add_mkdir");
        let (m, _) = f.model("m.3mf");
        let source = f.base.join("outside/a.png");
        std::fs::write(&source, PNG).unwrap();
        let fs = FaultFs::default().fail("create_dir", None, 1, PermissionDenied);
        let err = add_companion(&f.conn, &fs, m, &source, &[]).unwrap_err();
        assert!(err.message.starts_with("Begleitdateien-Ordner konnte nicht angelegt werden: "), "{}", err.message);
        assert!(db::companions::list_companions(&f.conn, m).unwrap().is_empty());
        assert!(!f.root.join("m_extras").exists());
    }

    #[test]
    fn add_with_a_failing_row_insert_removes_the_file_and_the_new_folder() {
        let f = Fx::new("add_insert");
        let (m, _) = f.model("m.3mf");
        f.block(BLOCK_INSERT);
        let source = f.base.join("outside/a.png");
        std::fs::write(&source, PNG).unwrap();
        let fs = FaultFs::default();
        assert!(add_companion(&f.conn, &fs, m, &source, &[]).is_err());
        assert!(!f.root.join("m_extras").exists());
        assert_eq!(fs.count("remove_file"), 1);
        assert_eq!(fs.count("remove_dir"), 1);
        assert!(source.exists(), "the source is only read");
    }

    #[test]
    fn restore_with_the_move_back_failing_removes_the_new_folder_and_keeps_the_row_trashed() {
        let f = Fx::new("restore_move");
        let (m, _) = f.model("m.3mf");
        let row = f.add(m, "a.png");
        remove_companion(&f.conn, &StdCompanionFs, row.id, &f.trash).unwrap();
        assert!(!f.root.join("m_extras").exists(), "the emptied folder went with the file");
        let before = f.row(row.id);
        let fs = FaultFs::default().fail("move_file", None, 1, PermissionDenied);
        let err = restore_companion(&f.conn, &fs, row.id, &[]).unwrap_err();
        assert!(err.message.contains("injected move_file"), "{}", err.message);
        assert!(!f.root.join("m_extras").exists(), "the folder created for it is removed again");
        assert_eq!(f.row(row.id), before);
        assert!(Path::new(before.trash_path.as_deref().unwrap()).exists());
    }

    #[test]
    fn restore_with_the_row_update_and_the_rollback_failing_names_the_file() {
        let f = Fx::new("restore_both");
        let (m, _) = f.model("m.3mf");
        let row = f.add(m, "a.png");
        remove_companion(&f.conn, &StdCompanionFs, row.id, &f.trash).unwrap();
        f.block(BLOCK_PATH);
        // Move calls: 1 restores the file, 2 is the rollback.
        let fs = FaultFs::default().fail("move_file", None, 2, PermissionDenied);
        let err = restore_companion(&f.conn, &fs, row.id, &[]).unwrap_err();
        assert!(err.message.contains("UND Rollback in den Papierkorb fehlgeschlagen"), "{}", err.message);
        assert!(f.root.join("m_extras/a.png").exists(), "the file stays where the message says");
    }

    #[test]
    fn trash_with_the_row_update_and_the_rollback_failing_names_the_file() {
        let f = Fx::new("trash_both");
        let (m, _) = f.model("m.3mf");
        let row = f.add(m, "a.png");
        f.block(BLOCK_TRASH);
        let fs = FaultFs::default().fail("move_file", None, 2, PermissionDenied);
        let err = remove_companion(&f.conn, &fs, row.id, &f.trash).unwrap_err();
        assert!(err.message.contains("UND Rollback aus dem Papierkorb fehlgeschlagen"), "{}", err.message);
        assert!(f.trash.join(format!("companion-{}-a.png", row.id)).exists());
    }

    #[test]
    fn a_locked_companion_stays_active_when_its_model_goes_to_the_trash() {
        let f = Fx::new("trash_locked");
        let (m, _) = f.model("m.3mf");
        let row = f.add(m, "a.png");
        let fs = FaultFs::default().fail("move_file", None, 1, PermissionDenied);
        trash_with_model(&fs, &f.conn, m, &f.trash, "2026-01-01T00:00:00Z");
        assert!(f.row(row.id).deleted_at.is_none());
        assert!(Path::new(&row.path).exists());
    }

    #[test]
    fn a_hard_link_that_is_not_possible_is_the_fs_decision_not_ours() {
        // The core asks for a move; how a volume change is handled is the file system's part.
        let f = Fx::new("trash_volume");
        let (m, _) = f.model("m.3mf");
        let row = f.add(m, "a.png");
        let fs = FaultFs::default().fail("move_file", None, 1, Unsupported);
        trash_with_model(&fs, &f.conn, m, &f.trash, "2026-01-01T00:00:00Z");
        assert!(f.row(row.id).deleted_at.is_none(), "an error from the move leaves the row active");
        assert_eq!(fs.count("move_file"), 1);
    }

    #[test]
    fn purge_keeps_the_row_when_the_trash_file_cannot_be_removed() {
        let f = Fx::new("purge");
        let (m, _) = f.model("m.3mf");
        let row = f.add(m, "a.png");
        remove_companion(&f.conn, &StdCompanionFs, row.id, &f.trash).unwrap();
        let fs = FaultFs::default().fail("remove_file", None, 1, PermissionDenied);
        purge_all_trashed(&fs, &f.conn, &f.trash);
        assert!(db::companions::get_companion(&f.conn, row.id).unwrap().is_some());
        assert!(f.trash.join(format!("companion-{}-a.png", row.id)).exists());
        assert!(purge_trash_files_of_model(&fs_fail_once(), &f.conn, m, &f.trash).is_err());
        purge_all_trashed(&StdCompanionFs, &f.conn, &f.trash);
        assert!(db::companions::get_companion(&f.conn, row.id).unwrap().is_none());
    }

    fn fs_fail_once() -> FaultFs {
        FaultFs::default().fail("remove_file", None, 1, PermissionDenied)
    }

    #[test]
    fn assign_from_archive_leaves_the_file_when_the_folder_cannot_be_created() {
        let f = Fx::new("assign");
        let (m, model) = f.model("pack.stl");
        let picture = f.root.join("front.png");
        std::fs::write(&picture, PNG).unwrap();
        let written = vec![model, picture.clone()];
        let imported = vec![to_dto(db::get_file(&f.conn, m).unwrap().unwrap(), &[])];
        let fs = FaultFs::default().fail("create_dir", None, 1, PermissionDenied);
        assert_eq!(assign_from_archive(&fs, &f.conn, &written, &imported, &[]), (0, 0));
        assert!(picture.exists());
        assert_eq!(assign_from_archive(&StdCompanionFs, &f.conn, &written, &imported, &[]), (1, 0));
        assert!(!picture.exists());
        assert!(f.root.join("pack_extras/front.png").exists());
    }

    #[test]
    fn carry_in_place_stops_when_the_folder_rename_is_refused() {
        let f = Fx::new("carry_case");
        let (m, old_model) = f.model("foo.stl");
        let row = f.add(m, "a.png");
        let new_model = f.root.join("FOO.stl");
        let fs = FaultFs::default().fail("rename", None, 1, PermissionDenied);
        let err = carry(&fs, &f.conn, m, &old_model, &new_model, true).err().unwrap();
        assert!(err.message.contains("injected rename"), "{}", err.message);
        assert_eq!(f.row(row.id).path, row.path);
        assert!(Path::new(&row.path).exists());
        // The same names on a case-sensitive volume are two folders: the file moves over
        // (not checkable where the real volume ignores case).
        if !cfg!(any(windows, target_os = "macos")) {
            let fs = FaultFs::default();
            carry(&fs, &f.conn, m, &old_model, &new_model, false).unwrap().finish(&fs);
            assert_eq!(fs.count("rename"), 0);
            assert!(f.root.join("FOO_extras/a.png").exists());
        }
    }

    #[test]
    fn carry_reports_a_folder_that_cannot_be_created_and_undoes_a_failed_row_update() {
        let f = Fx::new("carry_fail");
        let (m, old_model) = f.model("Foo.stl");
        let a = f.add(m, "a.png");
        let b = f.add(m, "b.png");
        let new_model = f.root.join("Bar.stl");
        let fs = FaultFs::default().fail("create_dir", None, 1, PermissionDenied);
        let err = carry(&fs, &f.conn, m, &old_model, &new_model, false).err().unwrap();
        assert!(err.expected && err.message == ErrorCode::CompanionTargetFolderCreateFailed.as_str(), "{err:?}");
        assert_eq!(f.row(a.id).path, a.path);
        // The row of the second file cannot be updated: the first one goes back.
        f.block("CREATE TRIGGER block_b BEFORE UPDATE OF path ON companion_files WHEN NEW.id = 2 BEGIN SELECT RAISE(ABORT, 'blocked'); END;");
        assert_eq!(b.id, 2);
        let fs = FaultFs::default();
        assert!(carry(&fs, &f.conn, m, &old_model, &new_model, false).is_err());
        assert_eq!(f.row(a.id).path, a.path);
        assert_eq!(f.row(b.id).path, b.path);
        assert!(Path::new(&a.path).exists() && Path::new(&b.path).exists());
        assert!(!f.root.join("Bar_extras").exists());
    }
}
