//! Source folder validation, bounded read-only scans, and catalog selections.
//! Hosts retain their operation gate across mutations and imports. No file is changed here.

use super::{files, placement, ImportError as CmdError, ImportResult as CmdResult};
use crate::{
    db::{self, error::DbError, sources as store},
    error_code::ErrorCode,
};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    io,
    path::{Path, PathBuf},
    sync::Mutex,
};

mod std_fs;
pub use std_fs::StdSourceFs;

/// File kinds are obtained without following the final symlink when scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Copy)]
pub struct SourceMetadata {
    pub kind: SourceKind,
    pub len: u64,
}

/// An entry can be listed even when its kind cannot be read.
pub struct SourceEntry {
    pub path: PathBuf,
    pub kind: io::Result<SourceKind>,
}

/// Lazy enumeration keeps the entry limit effective for very large directories.
pub type SourceEntries<'a> = Box<dyn Iterator<Item = io::Result<SourceEntry>> + 'a>;

/// All filesystem reads used by source discovery, including path resolution and hashing.
/// Paths are host identifiers. Implementations must return immediate children from list.
/// A server adapter must enforce handle-relative confinement on every operation, including
/// hashing and canonicalization; the desktop adapter preserves std::fs behavior.
/// Following a link when registering a source is optional for stricter adapters.
pub trait SourceFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    fn stat(&self, path: &Path, follow_links: bool) -> io::Result<SourceMetadata>;
    fn list(&self, path: &Path) -> io::Result<SourceEntries<'_>>;
    /// Return the same SHA-256 representation as files::compute_reader_hash.
    fn content_hash(&self, path: &Path) -> CmdResult<String>;
}

/// Borrowed host state; scans release the database mutex before filesystem reads.
pub struct SourceState<'a> {
    pub db: &'a Mutex<Connection>,
    pub sensitive_dirs: &'a [PathBuf],
    pub fs: &'a dyn SourceFs,
}

/// Selected paths and the roots that the host must enforce while importing.
pub type ImportSelection = (Vec<PathBuf>, Vec<PathBuf>);

fn expand_sensitive_dirs(fs: &dyn SourceFs, dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut expanded = Vec::new();
    for dir in dirs {
        if let Ok(canonical) = fs.canonicalize(dir) {
            if canonical != *dir {
                expanded.push(canonical);
            }
        }
        expanded.push(dir.clone());
    }
    expanded
}

// Callers have already resolved the existing directory through the host adapter.
fn check_managed_location(path: &Path, expanded: &[PathBuf]) -> CmdResult<()> {
    if expanded
        .iter()
        .any(|dir| path.starts_with(dir) || dir.starts_with(path))
    {
        return Err(CmdError::coded(ErrorCode::SourceFolderProtected));
    }
    Ok(())
}

/// Subfolder levels below a source folder that are looked at.
pub const MAX_SCAN_DEPTH: usize = 3;
/// Directory entries inspected per source folder; a whole drive must not stall the check.
pub const MAX_SCAN_ENTRIES: usize = 5000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFolderDto {
    pub id: String,
    pub path: String,
    pub added_at: String,
    /// The folder exists and is the same folder as when it was added (a drive that is
    /// unplugged, or a path that now leads elsewhere, is not reachable).
    pub reachable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFileDto {
    pub path: String,
    /// Path below the source folder, with `/` separators.
    pub relative_path: String,
    pub size_bytes: i64,
    /// Catalog model with identical content, when the content could be compared.
    pub duplicate_of_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceScanDto {
    pub folder_id: String,
    pub folder_path: String,
    pub files: Vec<SourceFileDto>,
    /// Entries that could not be read (folders, files, links that were left out).
    pub skipped: u32,
    /// The inspection stopped at its entry limit; there may be more files.
    pub truncated: bool,
    /// The folder could not be checked (missing, moved, or no longer allowed).
    pub unreachable: bool,
}

/// Describes a stored source folder and checks its current reachability through the host.
pub fn source_dto(fs: &dyn SourceFs, record: db::models::SourceFolderRecord) -> SourceFolderDto {
    let reachable = resolve_source(fs, &record.path, &[]).is_ok();
    SourceFolderDto {
        id: record.id.to_string(),
        path: record.path,
        added_at: record.added_at,
        reachable,
    }
}

/// The folder itself as it stands now: an existing directory that is still the stored
/// (already resolved) path, and not in or around a protected place.
fn resolve_source(
    fs: &dyn SourceFs,
    stored: &str,
    expanded_sensitive: &[PathBuf],
) -> CmdResult<PathBuf> {
    let path = Path::new(stored);
    let meta = fs
        .stat(path, false)
        .map_err(|_| CmdError::coded(ErrorCode::SourceFolderMissing))?;
    if meta.kind != SourceKind::Directory {
        return Err(CmdError::coded(ErrorCode::SourceNotAFolder));
    }
    let canonical = fs
        .canonicalize(path)
        .map_err(|_| CmdError::coded(ErrorCode::SourceFolderUnreadable))?;
    if canonical != path {
        return Err(CmdError::coded(ErrorCode::SourceFolderMoved));
    }
    check_managed_location(&canonical, expanded_sensitive)?;
    Ok(canonical)
}

fn same_or_inside(candidate: &Path, base: &Path) -> bool {
    placement::comparison_path(candidate).starts_with(placement::comparison_path(base))
}

/// Everything that makes a path unfit as a new source folder.
pub fn validate_new_source(
    fs: &dyn SourceFs,
    conn: &Connection,
    requested: &Path,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<PathBuf> {
    if !requested.is_absolute()
        || requested
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(CmdError::coded(ErrorCode::SourcePathInvalid));
    }
    let meta = fs
        .stat(requested, true)
        .map_err(|_| CmdError::coded(ErrorCode::SourceFolderMissing))?;
    if meta.kind != SourceKind::Directory {
        return Err(CmdError::coded(ErrorCode::SourceNotAFolder));
    }
    let canonical = fs
        .canonicalize(requested)
        .map_err(|_| CmdError::coded(ErrorCode::SourceFolderUnreadable))?;
    check_managed_location(&canonical, &expand_sensitive_dirs(fs, sensitive_dirs))
        .map_err(|_| CmdError::coded(ErrorCode::SourceFolderProtected))?;

    // The catalog folder, and every folder tree the catalog already keeps in place: files
    // there are managed through the catalog, not "new".
    let mut catalog_roots: Vec<PathBuf> = Vec::new();
    if let Some(base) =
        db::printer_link::get_setting(conn, crate::import::file_names::SETTING_CATALOG_BASE_DIR)?
    {
        catalog_roots.push(PathBuf::from(base));
    }
    catalog_roots.extend(
        db::list_folders(conn)?
            .into_iter()
            .filter(|f| f.parent_id.is_none())
            .map(|f| PathBuf::from(f.path)),
    );
    for root in catalog_roots {
        let root = fs.canonicalize(&root).unwrap_or(root);
        if same_or_inside(&canonical, &root) || same_or_inside(&root, &canonical) {
            return Err(CmdError::coded(ErrorCode::SourceFolderInCatalog));
        }
    }
    for existing in store::list_source_folders(conn)? {
        let existing = PathBuf::from(&existing.path);
        if placement::comparison_path(&existing) == placement::comparison_path(&canonical) {
            return Err(CmdError::coded(ErrorCode::SourceFolderDuplicate));
        }
        if same_or_inside(&canonical, &existing) || same_or_inside(&existing, &canonical) {
            return Err(CmdError::coded(ErrorCode::SourceFolderOverlap));
        }
    }
    Ok(canonical)
}

pub fn add_source_folder_with_conn(
    fs: &dyn SourceFs,
    conn: &Connection,
    requested: &Path,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<SourceFolderDto> {
    let canonical = validate_new_source(fs, conn, requested, sensitive_dirs)?;
    let added_at = chrono::Utc::now().to_rfc3339();
    let id = store::insert_source_folder(conn, &canonical.to_string_lossy(), &added_at)?;
    Ok(source_dto(
        fs,
        db::models::SourceFolderRecord {
            id,
            path: canonical.to_string_lossy().into_owned(),
            added_at,
        },
    ))
}

/// Forgets the folder and the declined files below it. No file is touched.
pub fn remove_source_folder_with_conn(conn: &Connection, id: i64) -> CmdResult<()> {
    let folder = store::get_source_folder(conn, id)?
        .ok_or_else(|| CmdError::coded(ErrorCode::SourceFolderNotFound))?;
    let root = PathBuf::from(&folder.path);
    let tx = conn.unchecked_transaction().map_err(DbError::from)?;
    store::delete_source_folder(&tx, id)?;
    for rejected in store::list_rejected_sources(&tx)? {
        if same_or_inside(Path::new(&rejected.path), &root) {
            store::delete_rejected_source(&tx, rejected.id)?;
        }
    }
    tx.commit().map_err(DbError::from)?;
    Ok(())
}

/// Model files found by [`walk_source_folder`], before the catalog is consulted.
#[derive(Debug, Default)]
pub struct Walked {
    pub files: Vec<(PathBuf, u64)>,
    pub skipped: u32,
    pub truncated: bool,
}

/// Lists supported model files below `root` (depth and entry limits), never following a
/// symlink. Anything that cannot be read is counted and left out.
pub fn walk_source_folder(
    fs: &dyn SourceFs,
    root: &Path,
    max_depth: usize,
    max_entries: usize,
) -> Walked {
    let mut walked = Walked::default();
    let mut visited = 0usize;
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = pending.pop() {
        let entries = match fs.list(&dir) {
            Ok(entries) => entries,
            Err(_) => {
                walked.skipped += 1;
                continue;
            }
        };
        for entry in entries {
            if visited >= max_entries {
                walked.truncated = true;
                return walked;
            }
            visited += 1;
            let Ok(entry) = entry else {
                walked.skipped += 1;
                continue;
            };
            let path = entry.path;
            let Ok(kind) = entry.kind else {
                walked.skipped += 1;
                continue;
            };
            if kind == SourceKind::Symlink {
                // Only links that would have mattered are worth a note.
                if files::is_model_extension(&path) {
                    walked.skipped += 1;
                }
            } else if kind == SourceKind::Directory {
                if depth < max_depth {
                    pending.push((path, depth + 1));
                } else {
                    walked.truncated = true;
                }
            } else if kind == SourceKind::File
                && files::is_model_extension(&path)
                && !files::is_ignored_import_path(&path)
            {
                match fs.stat(&path, false) {
                    Ok(meta) if meta.kind == SourceKind::File => walked.files.push((path, meta.len)),
                    _ => walked.skipped += 1,
                }
            }
        }
    }
    walked.files.sort();
    walked
}

/// What the catalog knows, loaded once per scan so the disk walk runs without the lock.
struct CatalogView {
    /// Paths of active models.
    paths: HashSet<PathBuf>,
    /// Declined files: path -> size.
    rejected: HashMap<PathBuf, i64>,
    /// Active models by file size, with their stored content hash.
    by_size: HashMap<i64, Vec<(i64, Option<String>)>>,
}

fn load_catalog_view(conn: &Connection) -> CmdResult<CatalogView> {
    let mut paths = HashSet::new();
    let mut by_size: HashMap<i64, Vec<(i64, Option<String>)>> = HashMap::new();
    let mut stmt = conn
        .prepare(
            "SELECT id, path, file_size_bytes, content_hash FROM files WHERE deleted_at IS NULL",
        )
        .map_err(DbError::from)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(DbError::from)?;
    for row in rows {
        let (id, path, size, hash) = row.map_err(DbError::from)?;
        paths.insert(placement::comparison_path(Path::new(&path)));
        by_size.entry(size).or_default().push((id, hash));
    }
    let rejected = store::list_rejected_sources(conn)?
        .into_iter()
        .map(|r| (placement::comparison_path(Path::new(&r.path)), r.size_bytes))
        .collect();
    Ok(CatalogView {
        paths,
        rejected,
        by_size,
    })
}

/// Hash comparison for one new file; only when a model of the same size exists, so most
/// files are never read.
fn duplicate_of(fs: &dyn SourceFs, view: &CatalogView, path: &Path, size: i64) -> Option<String> {
    let candidates = view.by_size.get(&size)?;
    if size as u64 > crate::safe_file::MAX_MODEL_FILE_BYTES
        || !candidates.iter().any(|(_, hash)| hash.is_some())
    {
        return None;
    }
    let hash = fs.content_hash(path).ok()?;
    candidates
        .iter()
        .find(|(_, stored)| stored.as_deref() == Some(hash.as_str()))
        .map(|(id, _)| id.to_string())
}

fn scan_one(
    fs: &dyn SourceFs,
    folder: &db::models::SourceFolderRecord,
    view: &CatalogView,
    expanded_sensitive: &[PathBuf],
    max_entries: usize,
) -> SourceScanDto {
    let empty = |unreachable: bool| SourceScanDto {
        folder_id: folder.id.to_string(),
        folder_path: folder.path.clone(),
        files: Vec::new(),
        skipped: 0,
        truncated: false,
        unreachable,
    };
    let Ok(root) = resolve_source(fs, &folder.path, expanded_sensitive) else {
        return empty(true);
    };
    let walked = walk_source_folder(fs, &root, MAX_SCAN_DEPTH, max_entries);
    let mut files = Vec::new();
    for (path, size) in walked.files {
        let key = placement::comparison_path(&path);
        let size = size as i64;
        if view.paths.contains(&key) || view.rejected.get(&key) == Some(&size) {
            continue;
        }
        let relative_path = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let duplicate_of_id = duplicate_of(fs, view, &path, size);
        files.push(SourceFileDto {
            path: path.to_string_lossy().into_owned(),
            relative_path,
            size_bytes: size,
            duplicate_of_id,
        });
    }
    SourceScanDto {
        folder_id: folder.id.to_string(),
        folder_path: folder.path.clone(),
        files,
        skipped: walked.skipped,
        truncated: walked.truncated,
        unreachable: false,
    }
}

/// New model files in every source folder. Reads only; the database lock is not held
/// while the disk is walked.
pub fn scan_with_state(
    state: &SourceState<'_>,
    max_entries: usize,
) -> CmdResult<Vec<SourceScanDto>> {
    let (sources, view) = {
        let conn = lock(state.db)?;
        (
            store::list_source_folders(&conn)?,
            load_catalog_view(&conn)?,
        )
    };
    let expanded = expand_sensitive_dirs(state.fs, state.sensitive_dirs);
    Ok(sources
        .iter()
        .map(|folder| scan_one(state.fs, folder, &view, &expanded, max_entries))
        .collect())
}

/// Hosts that already serialize the connection can use the same scan logic.
pub fn scan_with_conn(
    fs: &dyn SourceFs,
    conn: &Connection,
    sensitive_dirs: &[PathBuf],
    max_entries: usize,
) -> CmdResult<Vec<SourceScanDto>> {
    let folders = store::list_source_folders(conn)?;
    let view = load_catalog_view(conn)?;
    let expanded = expand_sensitive_dirs(fs, sensitive_dirs);
    Ok(folders.iter().map(|folder| scan_one(fs, folder, &view, &expanded, max_entries)).collect())
}

pub fn reject_with_conn(
    fs: &dyn SourceFs,
    conn: &Connection,
    sensitive_dirs: &[PathBuf],
    requested: &[String],
    max_entries: usize,
) -> CmdResult<usize> {
    let scans = scan_with_conn(fs, conn, sensitive_dirs, max_entries)?;
    persist_rejected(conn, validate_requested(&scans, requested)?)
}

/// Every path of the request must be a file the scan just listed, which also means it
/// lies below a registered source folder. Returns the paths with their scanned sizes.
fn validate_requested(
    scans: &[SourceScanDto],
    requested: &[String],
) -> CmdResult<Vec<(PathBuf, i64)>> {
    let listed: HashMap<&str, i64> = scans
        .iter()
        .flat_map(|scan| scan.files.iter())
        .map(|file| (file.path.as_str(), file.size_bytes))
        .collect();
    let mut seen = HashSet::new();
    let mut accepted = Vec::new();
    for path in requested {
        let size = listed
            .get(path.as_str())
            .ok_or_else(|| CmdError::coded(ErrorCode::SourceFileNotNew))?;
        if seen.insert(path.as_str()) {
            accepted.push((PathBuf::from(path), *size));
        }
    }
    Ok(accepted)
}

pub fn reject_with_state(
    state: &SourceState<'_>,
    requested: &[String],
    max_entries: usize,
) -> CmdResult<usize> {
    let scans = scan_with_state(state, max_entries)?;
    let accepted = validate_requested(&scans, requested)?;
    let conn = lock(state.db)?;
    persist_rejected(&conn, accepted)
}

fn persist_rejected(conn: &Connection, accepted: Vec<(PathBuf, i64)>) -> CmdResult<usize> {
    let rejected_at = chrono::Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction().map_err(DbError::from)?;
    for (path, size) in &accepted {
        store::upsert_rejected_source(&tx, &path.to_string_lossy(), *size, &rejected_at)?;
    }
    tx.commit().map_err(DbError::from)?;
    Ok(accepted.len())
}

fn lock(db: &Mutex<Connection>) -> CmdResult<std::sync::MutexGuard<'_, Connection>> {
    db.lock().map_err(|_| "database lock poisoned".into())
}

/// Validates a fresh selection and returns its paths and confinement roots.
/// The host chooses copy or move semantics and revalidates confinement during import.
pub fn accepted_import_selection(
    state: &SourceState<'_>,
    requested: &[String],
    max_entries: usize,
) -> CmdResult<ImportSelection> {
    {
        let conn = lock(state.db)?;
        if db::printer_link::get_setting(
            &conn,
            crate::import::file_names::SETTING_CATALOG_BASE_DIR,
        )?
        .is_none()
        {
            return Err(CmdError::coded(ErrorCode::SourceNoCatalogFolder));
        }
    }
    let scans = scan_with_state(state, max_entries)?;
    let accepted = validate_requested(&scans, requested)?;
    if accepted.is_empty() {
        return Err(CmdError::coded(ErrorCode::SourceNoFilesSelected));
    }
    // The scan lists files below the resolved folder, so that is the root that matters;
    // the registered spelling is kept too in case it already was the resolved one.
    let mut roots = Vec::new();
    for scan in scans.iter().filter(|scan| !scan.unreachable) {
        let registered = PathBuf::from(&scan.folder_path);
        roots.extend(state.fs.canonicalize(&registered).ok());
        roots.push(registered);
    }
    Ok((accepted.into_iter().map(|(path, _)| path).collect(), roots))
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
