//! Re-reading a cataloged file from disk and overwriting everything derived from it.
//! The desktop reads the stored path as is; the web variant only reads files below the
//! configured roots and applies the same parser budgets as the upload import.
use crate::db::models::{FileRecord, FileType, MaterialRecord, NewFile, ScannedMetadataUpdate};
use crate::import::files::{
    compute_content_hash, disk_modified_at, read_web_model, step_metadata, WebImportLimits,
};
use crate::read::{to_dto, ModelFileDto};
use crate::{db, obj, safe_file, stl, threemf};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Reasons a rescan fails. None of the variants for the web carries a filesystem path, so
/// the transport can hand them out unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RescanError {
    NotInCatalog,
    /// The stored path no longer exists (desktop keeps it for the user's message).
    Missing(String),
    Unsupported,
    /// STEP needs the OCCT preview, which the web server does not ship.
    StepUnavailable,
    /// The stored path is not below a configured root, or is a link/special file.
    Outside,
    TooLarge,
    Invalid,
    /// The file changed while it was read; a second attempt may succeed.
    Changed,
    Failed(String),
}

impl From<db::error::DbError> for RescanError {
    fn from(e: db::error::DbError) -> Self {
        Self::Failed(e.to_string())
    }
}

type Result<T> = std::result::Result<T, RescanError>;

fn load(conn: &Connection, id: i64) -> Result<FileRecord> {
    db::get_file(conn, id)?.ok_or(RescanError::NotInCatalog)
}

fn store(conn: &mut Connection, id: i64, update: &ScannedMetadataUpdate) -> Result<ModelFileDto> {
    db::update_scanned_metadata(conn, id, update)?;
    let file = load(conn, id)?;
    let spools = db::list_filament_spools(conn)?;
    Ok(to_dto(file, &spools))
}

fn materials(doc_materials: Vec<threemf::ThreeMfMaterial>) -> Vec<MaterialRecord> {
    doc_materials
        .into_iter()
        .map(|m| MaterialRecord { name: m.name, display_color: m.display_color })
        .collect()
}

/// Re-reads a cataloged file and overwrites all columns derived from it, e.g.
/// after it was re-sliced. If the file is missing, it aborts before the DB is
/// changed.
pub fn rescan_file(conn: &mut Connection, id: i64) -> Result<ModelFileDto> {
    let existing = load(conn, id)?;
    let path = Path::new(&existing.path);
    if !path.exists() {
        return Err(RescanError::Missing(existing.path.clone()));
    }
    let failed = |e: &dyn std::fmt::Display| RescanError::Failed(e.to_string());
    let extension = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase());

    let file_size_bytes = std::fs::metadata(path).map_err(|e| failed(&e))?.len() as i64;
    let content_hash = compute_content_hash(path).ok();
    let scanned = |dimensions_mm, volume_cm3, object_count| ScannedMetadataUpdate {
        dimensions_mm,
        volume_cm3,
        object_count,
        thumbnail_png: None,
        plate_count: None,
        slice_info_json: None,
        materials: Vec::new(),
        metadata: BTreeMap::new(),
        source: Default::default(),
        file_size_bytes,
        file_modified_at: disk_modified_at(path),
        content_hash: content_hash.clone(),
    };

    let update = match extension.as_deref() {
        Some("3mf") => {
            let doc = threemf::parse_3mf_file(path).map_err(|e| failed(&e))?;
            let source = threemf::source::detect_source(&doc.metadata);
            ScannedMetadataUpdate {
                source,
                thumbnail_png: doc.thumbnail_png,
                plate_count: doc.plate_count.map(|c| c as i64),
                slice_info_json: doc.slice_info.and_then(|s| serde_json::to_string(&s).ok()),
                materials: materials(doc.materials),
                metadata: doc.metadata,
                ..scanned(doc.dimensions_mm, doc.volume_cm3, Some(doc.object_count as i64))
            }
        }
        Some("stl") => {
            let doc = stl::parse_stl_file(path).map_err(|e| failed(&e))?;
            scanned(doc.dimensions_mm, doc.volume_cm3, None)
        }
        Some("stp") | Some("step") => {
            let (dimensions_mm, volume_cm3, object_count) = step_metadata(path);
            scanned(dimensions_mm, volume_cm3, object_count)
        }
        Some("obj") => {
            let doc = obj::parse_obj_file(path).map_err(|e| failed(&e))?;
            scanned(doc.dimensions_mm, doc.volume_cm3, None)
        }
        _ => return Err(RescanError::Unsupported),
    };
    store(conn, id, &update)
}

/// Opens the stored path only if its real location is below one of `roots`. The final
/// component must not be a link, and the handle is compared with the admitted
/// entry so a swap between check and open is detected.
fn open_below_roots(stored: &str, roots: &[PathBuf]) -> Result<(std::fs::File, PathBuf)> {
    let stored = Path::new(stored);
    let canonical = stored.canonicalize().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => RescanError::Missing(String::new()),
        _ => RescanError::Outside,
    })?;
    if !roots.iter().any(|root| root.canonicalize().is_ok_and(|root| canonical.starts_with(root))) {
        return Err(RescanError::Outside);
    }
    let linked = std::fs::symlink_metadata(stored).map_err(|_| RescanError::Outside)?;
    if linked.file_type().is_symlink() {
        return Err(RescanError::Outside);
    }
    let admitted = std::fs::symlink_metadata(&canonical).map_err(|_| RescanError::Outside)?;
    if !admitted.is_file() {
        return Err(RescanError::Outside);
    }
    let file = safe_file::open_regular(&canonical).map_err(|_| RescanError::Outside)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata().map_err(|_| RescanError::Outside)?;
        if admitted.dev() != opened.dev() || admitted.ino() != opened.ino() {
            return Err(RescanError::Changed);
        }
    }
    Ok((file, canonical))
}

fn update_from(new_file: NewFile) -> ScannedMetadataUpdate {
    ScannedMetadataUpdate {
        file_modified_at: new_file.file_modified_at,
        dimensions_mm: new_file.dimensions_mm,
        volume_cm3: new_file.volume_cm3,
        object_count: new_file.object_count,
        thumbnail_png: new_file.thumbnail_png,
        plate_count: new_file.plate_count,
        slice_info_json: new_file.slice_info_json,
        materials: new_file.materials,
        metadata: new_file.metadata,
        file_size_bytes: new_file.file_size_bytes,
        content_hash: new_file.content_hash,
        source: new_file.source,
    }
}

/// Identity of the file as it was parsed. The short write step compares it again so a file
/// replaced or rewritten in between is never stored under stale metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    dev_ino: (u64, u64),
}

impl PreparedRescan {
    /// The resolved location that was read; for diagnostics and tests only, never sent out.
    pub fn canonical_path(&self) -> &Path {
        &self.canonical
    }
}

impl FileStamp {
    fn of(meta: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            len: meta.len(),
            modified: meta.modified().ok(),
            #[cfg(unix)]
            dev_ino: (meta.dev(), meta.ino()),
        }
    }
}

/// Everything read from disk for a web rescan, ready to be written without touching the disk
/// under the write lock.
pub struct PreparedRescan {
    id: i64,
    stored_path: String,
    canonical: PathBuf,
    stamp: FileStamp,
    update: ScannedMetadataUpdate,
}

/// Step 1 of the web rescan, run without the write connection: only the catalog id is client
/// input. The path comes from the database and is admitted like a download, then parsed with
/// the upload budgets. Errors never include a path.
pub fn prepare_web_rescan(
    conn: &Connection,
    id: i64,
    roots: &[PathBuf],
    limits: WebImportLimits,
) -> Result<PreparedRescan> {
    let existing = load(conn, id)?;
    if existing.deleted_at.is_some() {
        return Err(RescanError::NotInCatalog);
    }
    let extension = Path::new(&existing.path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match extension.as_deref() {
        Some("3mf" | "stl" | "obj") => {}
        // Reading it without OCCT would silently wipe the dimensions of a good record.
        Some("stp" | "step") => return Err(RescanError::StepUnavailable),
        _ => return Err(RescanError::Unsupported),
    }
    let (file, canonical) = open_below_roots(&existing.path, roots).map_err(|e| match e {
        RescanError::Missing(_) => RescanError::Missing(String::new()),
        other => other,
    })?;
    let stamp = FileStamp::of(&file.metadata().map_err(|_| RescanError::Outside)?);
    let new_file = read_web_model(file, &canonical, &existing.name, limits).map_err(|kind| match kind.as_str() {
        "resource_limit" => RescanError::TooLarge,
        "changed" => RescanError::Changed,
        "read_failed" => RescanError::Failed("read_failed".into()),
        _ => RescanError::Invalid,
    })?;
    debug_assert!(matches!(new_file.file_type, FileType::ThreeMf | FileType::Stl | FileType::Obj));
    Ok(PreparedRescan { id, stored_path: existing.path, canonical, stamp, update: update_from(new_file) })
}

/// Step 2, short and under the write lock: the entry must still be the same live row with the
/// same path, and the file must still be the one that was parsed. Anything else is `Changed`.
pub fn apply_web_rescan(conn: &mut Connection, prepared: PreparedRescan) -> Result<ModelFileDto> {
    let current = load(conn, prepared.id)?;
    if current.deleted_at.is_some() || current.path != prepared.stored_path {
        return Err(RescanError::Changed);
    }
    let now = std::fs::symlink_metadata(&prepared.canonical).map_err(|_| RescanError::Changed)?;
    if !now.is_file() || FileStamp::of(&now) != prepared.stamp {
        return Err(RescanError::Changed);
    }
    store(conn, prepared.id, &prepared.update)
}

/// Both steps in a row, for callers that do not need the split (tests).
pub fn rescan_file_in_roots(
    conn: &mut Connection,
    id: i64,
    roots: &[PathBuf],
    limits: WebImportLimits,
) -> Result<ModelFileDto> {
    let prepared = prepare_web_rescan(conn, id, roots, limits)?;
    apply_web_rescan(conn, prepared)
}

#[cfg(test)]
#[path = "rescan_tests.rs"]
mod tests;
