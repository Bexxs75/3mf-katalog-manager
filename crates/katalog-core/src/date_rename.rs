//! Date-prefix rename planning and apply-time validation, matching the desktop command.
//! Filesystem reads and the reversible rename operation are supplied by the host.

use crate::import::{
    file_names as folders, rename as import_rename, ImportError as CmdError,
    ImportResult as CmdResult,
};
use crate::{
    db::{self, models::FileRecord},
    error_code::ErrorCode,
};
use chrono::{DateTime, Local};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    time::SystemTime,
};
use unicode_normalization::UnicodeNormalization;

mod std_fs;
pub use std_fs::StdDateRenameFs;

/// Reads needed for preview and revalidation. A server adapter must enforce its
/// handle-relative confinement on every operation. `entry` must not follow the
/// final symlink; `modified` follows it, matching the desktop date source.
pub trait DateRenameFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    fn entry(&self, path: &Path) -> io::Result<()>;
    fn modified(&self, path: &Path) -> io::Result<SystemTime>;
    /// Hosts that confine access themselves (the web server) say here whether the path
    /// lies inside the catalog. `Some` replaces the `catalog_base_dir` rule, `None`
    /// keeps it (desktop).
    fn within_catalog(&self, _path: &Path) -> Option<bool> {
        None
    }
}

/// A row that passed every check of `apply`; only the rename itself is left.
#[derive(Debug, Clone)]
pub struct VerifiedRename {
    pub file: FileRecord,
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    /// New name without the extension of the old name, for hosts that re-attach it.
    pub stem: String,
}

/// Host-owned reversible file operation, including companion carry and cleanup.
/// Implementations must recheck the destination and never overwrite it. After
/// moving files, call `commit` exactly once. Return success only after it succeeds;
/// on failure undo companion changes and the rename, preserving rollback errors.
/// The callback owns the model-row SQL; the host must not update that row itself.
/// This boundary lets desktop and handle-based hosts keep their own file semantics.
pub trait DateRenameEffect {
    fn rename(
        &self,
        file: &FileRecord,
        old: &Path,
        new: &Path,
        commit: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<(), String>;
}

fn normalized_lowercase(name: &str) -> String {
    name.nfc()
        .collect::<String>()
        .to_lowercase()
        .nfc()
        .collect()
}

fn target_exists(fs: &dyn DateRenameFs, path: &Path) -> io::Result<bool> {
    // Adding a date prefix can never be a case-only rename: every existing entry
    // is a conflict, including dangling symlinks and distinct hard links.
    match fs.entry(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn catalog_base(fs: &dyn DateRenameFs, conn: &Connection) -> CmdResult<Option<PathBuf>> {
    Ok(
        db::printer_link::get_setting(conn, folders::SETTING_CATALOG_BASE_DIR)
            .map_err(|e| format!("database: {e}"))?
            .and_then(|p| fs.canonicalize(Path::new(&p)).ok()),
    )
}

fn reject_if_sensitive_path(
    fs: &dyn DateRenameFs,
    path: &Path,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<()> {
    reject_if_sensitive_path_expanded(fs, path, &expand_sensitive_dirs(fs, sensitive_dirs))
}
/// Adds the canonical spelling of each protected folder (e.g. /bin -> /usr/bin).
/// Compute once when many paths are checked.
fn expand_sensitive_dirs(fs: &dyn DateRenameFs, sensitive_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut expanded = Vec::with_capacity(sensitive_dirs.len());
    for dir in sensitive_dirs {
        if let Ok(canonical) = fs.canonicalize(dir) {
            if canonical != *dir {
                expanded.push(canonical);
            }
        }
        expanded.push(dir.clone());
    }
    expanded
}
fn reject_if_sensitive_path_expanded(
    fs: &dyn DateRenameFs,
    path: &Path,
    expanded_dirs: &[PathBuf],
) -> CmdResult<()> {
    let resolved = resolve_path_for_sensitivity_check(fs, path)?;
    for dir in expanded_dirs {
        if resolved == *dir || resolved.starts_with(dir) {
            return Err(format!(
                "Zielpfad liegt in einem geschuetzten Systemverzeichnis ({}) und wird abgelehnt",
                dir.display()
            )
            .into());
        }
    }
    Ok(())
}
fn resolve_path_for_sensitivity_check(fs: &dyn DateRenameFs, path: &Path) -> CmdResult<PathBuf> {
    if let Ok(canonical) = fs.canonicalize(path) {
        return Ok(canonical);
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Pfad enthaelt \"..\"-Komponenten und wird abgelehnt".into());
    }

    let mut trailing: Vec<std::ffi::OsString> = Vec::new();
    let mut cursor = path;
    while let (Some(name), Some(parent)) = (cursor.file_name(), cursor.parent()) {
        trailing.push(name.to_os_string());
        if let Ok(canonical) = fs.canonicalize(parent) {
            let mut resolved = canonical;
            for component in trailing.iter().rev() {
                resolved.push(component);
            }
            return Ok(resolved);
        }
        cursor = parent;
    }
    Ok(path.to_path_buf())
}

/// Upper bound per call so one request cannot hold the catalog gate and the
/// database lock for an unbounded time.
const MAX_ITEMS: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateSource {
    /// Modification time of the file on disk.
    Modified,
    /// `imported_at` of the catalog entry.
    Imported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStatus {
    Rename,
    SkipHasPrefix,
    SkipConflict,
    SkipMissing,
    SkipTrash,
    SkipTooLong,
    /// File lies outside the catalog folders (adopted in place).
    SkipExternal,
    /// The chosen date source has no usable value for this file.
    SkipNoDate,
    /// New name or location is not allowed (forbidden characters, protected folder).
    SkipInvalid,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DateRenamePreview {
    pub id: String,
    pub old_name: String,
    pub new_name: String,
    pub status: PreviewStatus,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateRenameItem {
    pub id: String,
    pub new_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyStatus {
    Renamed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DateRenameResult {
    pub id: String,
    pub status: ApplyStatus,
    /// Short code for the checks done here ("missing", "external", "trash",
    /// "hasPrefix", "nameRule", "targetTaken"), otherwise the backend message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn disk_name(file: &FileRecord) -> String {
    Path::new(&file.path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.name.clone())
}

fn source_time(
    fs: &dyn DateRenameFs,
    file: &FileRecord,
    source: DateSource,
) -> Option<DateTime<Local>> {
    match source {
        DateSource::Modified => fs
            .modified(Path::new(&file.path))
            .ok()
            .map(DateTime::<Local>::from),
        // Stored as UTC RFC 3339; the prefix is shown in the local zone like at import.
        DateSource::Imported => DateTime::parse_from_rfc3339(&file.imported_at)
            .ok()
            .map(|t| t.with_timezone(&Local)),
    }
}

/// True when the target is taken on disk or by another catalog entry. Catalog
/// entries whose file is missing still own their path.
fn target_taken(fs: &dyn DateRenameFs, conn: &Connection, id: i64, new: &Path) -> CmdResult<bool> {
    if target_exists(fs, new).map_err(|e| e.to_string())? {
        return Ok(true);
    }
    let in_catalog: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM files WHERE path = ?1 AND id != ?2)",
            rusqlite::params![new.to_string_lossy(), id],
            |row| row.get(0),
        )
        .map_err(|e| format!("database: {e}"))?;
    Ok(in_catalog)
}

/// Checks that hold for preview and apply alike. `Err` carries the skip reason.
fn eligibility(
    fs: &dyn DateRenameFs,
    file: &FileRecord,
    base: Option<&Path>,
) -> Result<(), PreviewStatus> {
    if file.deleted_at.is_some() {
        return Err(PreviewStatus::SkipTrash);
    }
    if fs.entry(Path::new(&file.path)).is_err() {
        return Err(PreviewStatus::SkipMissing);
    }
    let inside = match fs.within_catalog(Path::new(&file.path)) {
        Some(inside) => inside,
        None => base
            .zip(fs.canonicalize(Path::new(&file.path)).ok())
            .is_some_and(|(base, path)| path.starts_with(base)),
    };
    if !inside {
        return Err(PreviewStatus::SkipExternal);
    }
    if import_rename::has_date_prefix(&disk_name(file)) {
        return Err(PreviewStatus::SkipHasPrefix);
    }
    Ok(())
}

fn plan_file(
    fs: &dyn DateRenameFs,
    conn: &Connection,
    file: &FileRecord,
    source: DateSource,
    base: Option<&Path>,
    sensitive: &[PathBuf],
    claimed: &mut HashSet<String>,
) -> CmdResult<(String, PreviewStatus)> {
    let old_name = disk_name(file);
    let skip = |status| Ok((old_name.clone(), status));
    if let Err(status) = eligibility(fs, file, base) {
        return skip(status);
    }
    let Some(time) = source_time(fs, file, source) else {
        return skip(PreviewStatus::SkipNoDate);
    };
    let new_name = format!("{}{old_name}", import_rename::date_prefix(time));
    // A forbidden character in the old name must not be silently rewritten here.
    if new_name.len() > folders::MAX_DIR_NAME_BYTES {
        return skip(PreviewStatus::SkipTooLong);
    }
    if folders::validate_name(&new_name, true, folders::NameKind::File).is_err() {
        return skip(PreviewStatus::SkipInvalid);
    }
    let old_path = PathBuf::from(&file.path);
    let new_path = old_path.with_file_name(&new_name);
    if reject_if_sensitive_path(fs, &old_path, sensitive).is_err()
        || reject_if_sensitive_path(fs, &new_path, sensitive).is_err()
    {
        return skip(PreviewStatus::SkipInvalid);
    }
    // Case-insensitive file systems treat these spellings as one entry; the first
    // file of the selection keeps the name.
    let key = normalized_lowercase(&new_path.to_string_lossy());
    if target_taken(fs, conn, file.id, &new_path)? || !claimed.insert(key) {
        return skip(PreviewStatus::SkipConflict);
    }
    Ok((new_name, PreviewStatus::Rename))
}

/// Plans names without changing files or catalog rows. Selection order breaks conflicts.
pub fn preview(
    conn: &Connection,
    file_ids: &[String],
    source: DateSource,
    sensitive: &[PathBuf],
    fs: &dyn DateRenameFs,
) -> CmdResult<Vec<DateRenamePreview>> {
    if file_ids.len() > MAX_ITEMS {
        return Err(CmdError::coded_with(
            ErrorCode::MoveTooManyFiles,
            [file_ids.len().to_string(), MAX_ITEMS.to_string()],
        ));
    }
    let base = catalog_base(fs, conn)?;
    let mut claimed = HashSet::new();
    let mut out = Vec::with_capacity(file_ids.len());
    for raw in file_ids {
        let file = raw
            .parse::<i64>()
            .ok()
            .map(|id| db::get_file(conn, id).map_err(|e| format!("database: {e}")))
            .transpose()?
            .flatten();
        let (old_name, new_name, status) = match file {
            None => (String::new(), String::new(), PreviewStatus::SkipMissing),
            Some(file) => {
                let (new_name, status) = plan_file(
                    fs,
                    conn,
                    &file,
                    source,
                    base.as_deref(),
                    sensitive,
                    &mut claimed,
                )?;
                (disk_name(&file), new_name, status)
            }
        };
        out.push(DateRenamePreview {
            id: raw.clone(),
            old_name,
            new_name,
            status,
        });
    }
    Ok(out)
}

/// A name the preview could have produced: a real `JJJJ-MM-TT_HH-MM_` prefix
/// followed by the current name, nothing else.
fn follows_rule(new_name: &str, old_name: &str) -> bool {
    let Some((prefix, rest)) = new_name
        .get(..import_rename::PREFIX_LEN)
        .zip(new_name.get(import_rename::PREFIX_LEN..))
    else {
        return false;
    };
    rest == old_name
        && import_rename::has_date_prefix(prefix)
        && chrono::NaiveDate::parse_from_str(&prefix[..10], "%Y-%m-%d").is_ok()
        && chrono::NaiveTime::parse_from_str(&prefix[11..16], "%H-%M").is_ok()
}

/// Same extension rule as the rename operation (`.gcode.3mf` counts as one).
fn extension_of(name: &str) -> &str {
    if name.to_ascii_lowercase().ends_with(".gcode.3mf") {
        &name[name.len() - 10..]
    } else {
        name.rfind('.').map(|i| &name[i..]).unwrap_or("")
    }
}

/// Everything `apply` checks before it renames: state, trash, presence, location,
/// name rule, free target, name validity and protected folders. Errors are the short
/// reason codes of `DateRenameResult::reason`. `base` is only used when the host's
/// `within_catalog` returns `None`.
pub fn verify_item(
    fs: &dyn DateRenameFs,
    conn: &Connection,
    item: &DateRenameItem,
    base: Option<&Path>,
    sensitive: &[PathBuf],
) -> Result<VerifiedRename, String> {
    let id: i64 = item.id.parse().map_err(|_| "missing".to_string())?;
    let file = db::get_file(conn, id)
        .map_err(|e| format!("database: {e}"))?
        .ok_or_else(|| "missing".to_string())?;
    eligibility(fs, &file, base).map_err(|status| {
        match status {
            PreviewStatus::SkipTrash => "trash",
            PreviewStatus::SkipMissing => "missing",
            PreviewStatus::SkipExternal => "external",
            _ => "hasPrefix",
        }
        .to_string()
    })?;
    // The UI may only send names the preview produced, never free input.
    let old_name = disk_name(&file);
    if !follows_rule(&item.new_name, &old_name) {
        return Err("nameRule".into());
    }
    let old_path = PathBuf::from(&file.path);
    let new_path = old_path.with_file_name(&item.new_name);
    if target_taken(fs, conn, id, &new_path).map_err(|e| e.message)? {
        return Err("targetTaken".into());
    }
    folders::validate_name(&item.new_name, true, folders::NameKind::File)
        .map_err(|e| e.message)?;
    reject_if_sensitive_path(fs, &old_path, sensitive).map_err(|e| e.message)?;
    reject_if_sensitive_path(fs, &new_path, sensitive).map_err(|e| e.message)?;
    let stem = item
        .new_name
        .strip_suffix(extension_of(&old_name))
        .unwrap_or(&item.new_name)
        .to_string();
    Ok(VerifiedRename {
        file,
        old_path,
        new_path,
        stem,
    })
}

fn apply_one(
    fs: &dyn DateRenameFs,
    effect: &dyn DateRenameEffect,
    conn: &Connection,
    item: &DateRenameItem,
    base: Option<&Path>,
    sensitive: &[PathBuf],
) -> Result<(), String> {
    let VerifiedRename {
        file,
        old_path,
        new_path,
        ..
    } = verify_item(fs, conn, item, base, sensitive)?;
    let id = file.id;
    let outcome = effect
        .rename(&file, &old_path, &new_path, &mut || {
            db::rename_file(conn, id, &item.new_name, &new_path.to_string_lossy())
                .map_err(|e| e.to_string())
        })
        .map_err(CmdError::from);
    outcome.map_err(|e| {
        // The target can appear between the check above and the rename.
        if target_taken(fs, conn, id, &new_path).unwrap_or(false) {
            "targetTaken".to_string()
        } else {
            e.message
        }
    })
}

/// Rechecks every item against current state and continues after individual failures.
/// The caller must hold its exclusive catalog gate and database lock for the whole call.
pub fn apply(
    conn: &Connection,
    items: Vec<DateRenameItem>,
    sensitive: &[PathBuf],
    fs: &dyn DateRenameFs,
    effect: &dyn DateRenameEffect,
) -> CmdResult<Vec<DateRenameResult>> {
    if items.len() > MAX_ITEMS {
        return Err(CmdError::coded_with(
            ErrorCode::MoveTooManyFiles,
            [items.len().to_string(), MAX_ITEMS.to_string()],
        ));
    }
    let base = catalog_base(fs, conn)?;
    Ok(items
        .into_iter()
        .map(|item| {
            // One failing file must not stop the others.
            let outcome = apply_one(fs, effect, conn, &item, base.as_deref(), sensitive);
            DateRenameResult {
                id: item.id,
                status: if outcome.is_ok() {
                    ApplyStatus::Renamed
                } else {
                    ApplyStatus::Failed
                },
                reason: outcome.err(),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests;
