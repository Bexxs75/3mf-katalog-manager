//! The disk sync of the desktop catalog (issue #20, package P8): the scan that compares the
//! catalog with its roots on disk (new and missing files, files renamed or moved outside the app
//! by content hash, copies of present models, remembered declined copies, folders gone from disk,
//! case-insensitive comparison) and the apply step that takes over what the user chose. Desktop
//! semantics: absolute paths as stored, catalog roots are the topmost folder rows. No `fsops` /
//! `upload_fs` dependency, so it builds on Windows and macOS.
//!
//! The disk access of the scan goes through [`DiskScanFs`]; the desktop implements it on top of
//! its file-system boundary (`Fs`, P0a) so tests can fail step n. The checks and effects whose
//! desktop variants log their own errors go through [`DiskSyncEffects`], so the log lines stay
//! the desktop's. Reading file content (hash, import) is not part of the trait. See
//! `docs/plans/issue20-p8-zuschnitt.md`.
use std::collections::{HashMap, HashSet};
use std::fs::{FileType, Metadata};
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db;
use crate::import::file_names::SETTING_CATALOG_BASE_DIR;
use crate::import::placement::expand_sensitive_dirs;

/// The disk operations of the scan and the checks of the apply step.
pub trait DiskScanFs {
    /// Same as `std::fs::canonicalize`.
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    /// Same as `std::fs::metadata`.
    fn metadata(&self, path: &Path) -> io::Result<Metadata>;
    /// Same as `std::fs::symlink_metadata`.
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata>;
    /// Entries of `dir` with their types, as `std::fs::read_dir` lists them. An entry that
    /// vanishes while listing (`NotFound`) is skipped, `ignore`d paths are skipped before
    /// their type is read; any other error fails the whole listing.
    fn read_dir_entries(&self, dir: &Path, ignore: &dyn Fn(&Path) -> bool) -> io::Result<Vec<(PathBuf, FileType)>>;
    /// Same as `Path::is_dir` (follows links, any error is `false`).
    fn is_dir(&self, path: &Path) -> bool {
        self.metadata(path).map(|m| m.is_dir()).unwrap_or(false)
    }
}

/// `std::fs`, 1:1.
pub struct StdDiskScanFs;

impl DiskScanFs for StdDiskScanFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
    fn metadata(&self, path: &Path) -> io::Result<Metadata> {
        std::fs::metadata(path)
    }
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
        std::fs::symlink_metadata(path)
    }
    fn read_dir_entries(&self, dir: &Path, ignore: &dyn Fn(&Path) -> bool) -> io::Result<Vec<(PathBuf, FileType)>> {
        let mut entries = Vec::new();
        for item in std::fs::read_dir(dir)? {
            let item = match item {
                Ok(item) => item,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            if ignore(&item.path()) {
                continue;
            }
            match item.file_type() {
                Ok(kind) => entries.push((item.path(), kind)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(entries)
    }
}

/// Checks and effects of the sync whose desktop variants log their own failures. Errors are
/// plain texts: the sync only shows them as the reason of a failed path (or maps them to a code).
pub trait DiskSyncEffects {
    /// Content hash of a file, `None` when it cannot be read (import hash function).
    fn content_hash(&self, path: &Path) -> Option<String>;
    /// A managed path must neither lie in nor contain a protected directory.
    fn check_managed_location(&self, path: &Path, expanded_sensitive: &[PathBuf]) -> Result<(), String>;
    /// False when moving the folder at `path` would move the catalog base directory.
    fn may_relocate_folder(&self, conn: &Connection, path: &Path) -> bool;
    /// Moves the row of a file that is gone from disk to the trash (no disk access).
    fn delete_missing_file(&self, conn: &Connection, id: i64) -> Result<(), String>;
}

/// The core's own variants (they do not log).
pub struct StdDiskSyncEffects;

impl DiskSyncEffects for StdDiskSyncEffects {
    fn content_hash(&self, path: &Path) -> Option<String> {
        crate::import::files::compute_content_hash(path).ok()
    }
    fn check_managed_location(&self, path: &Path, expanded_sensitive: &[PathBuf]) -> Result<(), String> {
        crate::folder_move::check_managed_location(path, expanded_sensitive).map_err(|e| e.message)
    }
    fn may_relocate_folder(&self, conn: &Connection, path: &Path) -> bool {
        crate::folder_move::reject_catalog_root_relocation(conn, path).is_ok()
    }
    fn delete_missing_file(&self, conn: &Connection, id: i64) -> Result<(), String> {
        crate::model_trash::delete_missing_file(conn, id).map_err(|e| e.message)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskEntry {
    pub path: String,
    pub relative_path: String,
    pub root: String,
    pub id: Option<String>,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskScan {
    pub new_folders: Vec<DiskEntry>,
    pub new_files: Vec<DiskEntry>,
    /// Missing catalog rows that reappear as exactly one new file with the same content.
    pub renamed_files: Vec<DiskRename>,
    /// New files whose content already belongs to a present catalog row.
    pub duplicate_files: Vec<DiskDuplicate>,
    pub missing_files: Vec<DiskEntry>,
    /// Topmost catalog folders that are gone from disk although their catalog root is
    /// reachable. Only reported: the sync never removes one on its own.
    pub missing_folders: Vec<DiskFolderEntry>,
    pub unreachable_roots: Vec<String>,
    pub skipped_paths: Vec<String>,
    /// Supported files on disk that would be new but whose path is still held by a trash
    /// row. The sync never restores them; the count only lets the dialog say why they
    /// are not offered.
    pub trash_blocked_count: usize,
    /// New files left out because the user declined them as copies earlier and nothing
    /// changed since. The dialog only names the number.
    pub known_copy_count: usize,
    #[serde(skip)]
    pub roots: Vec<DiskRoot>,
}
/// A catalog folder row whose directory no longer exists on disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskFolderEntry {
    pub id: String,
    pub path: String,
    pub relative_path: String,
    pub root: String,
    /// Models below the folder, subfolders included, trash excluded.
    pub model_count: i64,
}
/// A catalog row whose file was renamed or moved outside the app.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskRename {
    pub id: String,
    pub old_path: String,
    pub old_relative_path: String,
    pub old_root: String,
    pub new_path: String,
    pub new_relative_path: String,
    pub root: String,
}
impl DiskRename {
    pub fn old_entry(&self) -> DiskEntry {
        DiskEntry {
            path: self.old_path.clone(),
            relative_path: self.old_relative_path.clone(),
            root: self.old_root.clone(),
            id: Some(self.id.clone()),
        }
    }
    pub fn new_entry(&self) -> DiskEntry {
        DiskEntry {
            path: self.new_path.clone(),
            relative_path: self.new_relative_path.clone(),
            root: self.root.clone(),
            id: None,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskDuplicate {
    #[serde(flatten)]
    pub entry: DiskEntry,
    pub duplicate_of_name: String,
    pub duplicate_of_path: String,
    // What the content match saw, kept so a declined copy can be remembered without
    // reading the file again. Never sent to the webview.
    #[serde(skip)]
    pub content_hash: String,
    #[serde(skip)]
    pub size_bytes: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameChoice {
    pub id: String,
    pub new_path: String,
}
// Every field has a default so that callers written before renames existed still work.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DiskSelection {
    pub new_paths: Vec<String>,
    pub missing_ids: Vec<String>,
    pub remove_missing: bool,
    pub renames: Vec<RenameChoice>,
    /// Copies of present models the user left unticked; they are remembered so the next
    /// sync does not offer them again. Only paths the re-scan lists as copies count.
    pub declined_copies: Vec<String>,
    // Filled by the command from app state, never taken from the webview.
    #[serde(skip)]
    pub sensitive_dirs: Vec<PathBuf>,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskApplied {
    pub file_ids: Vec<String>,
    pub folder_ids: Vec<String>,
    pub renamed_ids: Vec<String>,
    pub renamed_count: usize,
    pub removed_count: usize,
    /// Copies remembered by this run, and all remembered copies afterwards.
    pub noted_copy_count: usize,
    pub known_copy_count: i64,
    pub failed_paths: Vec<DiskFailedPath>,
}

#[derive(Serialize)]
pub struct DiskFailedPath {
    pub path: String,
    pub reason: String,
}

/// A listed entry: the stored spelling, its path relative to `root` and its catalog id.
pub fn entry(root: &Path, path: &Path, id: Option<String>) -> DiskEntry {
    DiskEntry {
        path: path.to_string_lossy().into_owned(),
        relative_path: path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned(),
        root: root.to_string_lossy().into_owned(),
        id,
    }
}
/// How the desktop compares paths on this system (macOS and Windows fold case).
pub const CASE_INSENSITIVE_PATHS: bool = cfg!(any(windows, target_os = "macos"));

/// Comparison key of a path: folded to lower case when `case_insensitive`.
pub fn path_key(path: &Path, case_insensitive: bool) -> PathBuf {
    if case_insensitive {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path.to_path_buf()
    }
}

/// Compares paths by what they point to, not by how they are spelled. A catalog row may
/// hold a symlink spelling of a file (the import stores whatever path it was given)
/// while the scan walks the real tree; both must yield the same key or the same file
/// is offered as new. Stored paths are never rewritten, only the comparison key is.
pub struct KeyResolver<'a> {
    fs: &'a dyn DiskScanFs,
    roots: &'a [DiskRoot],
    case_insensitive: bool,
    // One lookup per directory instead of per file; `None` remembers a failed lookup.
    dirs: std::cell::RefCell<HashMap<PathBuf, Option<PathBuf>>>,
}
impl<'a> KeyResolver<'a> {
    /// On the real file system.
    pub fn new(roots: &'a [DiskRoot], case_insensitive: bool) -> Self {
        Self::with_fs(&StdDiskScanFs, roots, case_insensitive)
    }
    pub fn with_fs(fs: &'a dyn DiskScanFs, roots: &'a [DiskRoot], case_insensitive: bool) -> Self {
        Self {
            fs,
            roots,
            case_insensitive,
            dirs: Default::default(),
        }
    }
    /// Maps a stored path below a configured root onto the canonical root.
    pub fn normalize(&self, path: &Path) -> PathBuf {
        self.roots
            .iter()
            .find_map(|r| r.resolve(path))
            .unwrap_or_else(|| path.to_path_buf())
    }
    fn canonical_dir(&self, dir: &Path) -> Option<PathBuf> {
        if let Some(known) = self.dirs.borrow().get(dir) {
            return known.clone();
        }
        let resolved = self.fs.canonicalize(dir).ok();
        self.dirs
            .borrow_mut()
            .insert(dir.to_path_buf(), resolved.clone());
        resolved
    }
    /// Falls back to the plain spelling whenever resolving fails (missing file or
    /// directory, no permission, symlink loop), so a broken alias never hides a file.
    pub fn key(&self, path: &Path) -> PathBuf {
        let normalized = self.normalize(path);
        let mut resolved = match (normalized.parent(), normalized.file_name()) {
            (Some(parent), Some(name)) => self
                .canonical_dir(parent)
                .map_or_else(|| normalized.clone(), |dir| dir.join(name)),
            _ => normalized,
        };
        // The file itself may be a symlink; the scan lists its target instead.
        if self.fs.symlink_metadata(&resolved).is_ok_and(|m| m.file_type().is_symlink()) {
            if let Ok(target) = self.fs.canonicalize(&resolved) {
                resolved = target;
            }
        }
        path_key(&resolved, self.case_insensitive)
    }
}

/// A configured catalog root: the spelling stored in the catalog and where it really is.
#[derive(Debug)]
pub struct DiskRoot {
    pub stored: PathBuf,
    pub canonical: PathBuf,
}
impl DiskRoot {
    /// `path` (below the stored or the canonical spelling) on the canonical root.
    pub fn resolve(&self, path: &Path) -> Option<PathBuf> {
        let relative = path
            .strip_prefix(&self.stored)
            .ok()
            .or_else(|| path.strip_prefix(&self.canonical).ok())?;
        if relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return None;
        }
        Some(self.canonical.join(relative))
    }
    /// The stored spelling still resolves to the same directory.
    pub fn unchanged(&self, fs: &dyn DiskScanFs) -> bool {
        fs.canonicalize(&self.stored).ok().as_ref() == Some(&self.canonical)
            && fs.is_dir(&self.canonical)
    }
}

#[derive(Debug, Serialize)]
pub struct DiskSyncError {
    pub code: &'static str,
    pub paths: Vec<String>,
}
pub type SyncResult<T> = Result<T, DiskSyncError>;
impl DiskSyncError {
    pub fn new(code: &'static str, paths: Vec<String>) -> Self {
        Self { code, paths }
    }
    pub fn path(code: &'static str, path: &Path) -> Self {
        Self::new(code, vec![path.to_string_lossy().into_owned()])
    }
}
impl From<String> for DiskSyncError {
    fn from(_: String) -> Self {
        Self::new("diskSyncFailed", vec![])
    }
}
impl DiskSyncError {
    /// What the desktop made of a command error: `importActive` keeps its code, anything
    /// else is `diskSyncFailed` (the text is not passed on).
    pub fn from_message(message: &str) -> Self {
        Self::new(if message == "importActive" { "importActive" } else { "diskSyncFailed" }, vec![])
    }
}

/// Same as `backup::is_missing` (only `NotFound` counts), on an injected file system.
fn is_missing_fs(fs: &dyn DiskScanFs, path: &Path) -> std::io::Result<bool> {
    match fs.metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e),
    }
}

// The canonical root is trusted; only descendants must be free of symlinks.
pub fn no_symlinks(fs: &dyn DiskScanFs, root: &Path, path: &Path) -> std::io::Result<bool> {
    let Ok(relative) = path.strip_prefix(root) else {
        return Ok(false);
    };
    if relative
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Ok(false);
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs.symlink_metadata(&current) {
            Ok(m) if m.file_type().is_symlink() => return Ok(false),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}
/// Entries of one directory with their types.
pub type DirectoryEntries = Vec<(PathBuf, std::fs::FileType)>;

/// Lists a directory for the scan; paths the import ignores are left out.
pub fn read_directory(fs: &dyn DiskScanFs, dir: &Path) -> std::io::Result<DirectoryEntries> {
    fs.read_dir_entries(dir, &crate::import::files::is_ignored_import_path)
}

fn record_skipped(scan: &mut DiskScan, root: &DiskRoot, dir: &Path) {
    let stored = root.stored.join(dir.strip_prefix(&root.canonical).unwrap());
    scan.skipped_paths
        .push(stored.to_string_lossy().into_owned());
}

/// What the catalog already knows, as comparison keys.
struct KnownPaths<'a> {
    folders: &'a HashSet<PathBuf>,
    files: &'a HashSet<PathBuf>,
    /// Keys of files that exist only as trash rows; a subset of `files`.
    trashed: &'a HashSet<PathBuf>,
    /// Companion folders of catalog models.
    extras: &'a HashSet<PathBuf>,
}

fn walk(
    fs: &dyn DiskScanFs,
    root: &DiskRoot,
    known: &KnownPaths,
    scan: &mut DiskScan,
    case_insensitive: bool,
    seen_files: &mut HashSet<PathBuf>,
    read: &mut impl FnMut(&Path) -> std::io::Result<DirectoryEntries>,
) -> std::io::Result<()> {
    let mut pending = vec![root.canonical.clone()];
    let mut new_folders = HashSet::new();
    while let Some(dir) = pending.pop() {
        let entries = no_symlinks(fs, &root.canonical, &dir).and_then(|safe| {
            if safe {
                read(&dir)
            } else {
                Ok(Vec::new())
            }
        });
        let entries = match entries {
            Ok(entries) => entries,
            Err(e) if dir == root.canonical => return Err(e),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                record_skipped(scan, root, &dir);
                continue;
            }
        };
        for (path, kind) in entries {
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                // The companion folder of a catalog model is managed through the
                // database; its contents are neither new files nor a new folder.
                if known.extras.contains(&path_key(&path, case_insensitive)) {
                    continue;
                }
                pending.push(path);
            } else if kind.is_file() && crate::import::files::is_model_extension(&path) {
                let key = path_key(&path, case_insensitive);
                seen_files.insert(key.clone());
                if known.files.contains(&key) {
                    if known.trashed.contains(&key) {
                        scan.trash_blocked_count += 1;
                    }
                    continue;
                }
                let stored = root
                    .stored
                    .join(path.strip_prefix(&root.canonical).unwrap());
                scan.new_files.push(entry(&root.stored, &stored, None));
                for parent in path
                    .ancestors()
                    .skip(1)
                    .take_while(|p| *p != root.canonical)
                {
                    if !known.folders.contains(&path_key(parent, case_insensitive)) {
                        new_folders.insert(parent.to_path_buf());
                    }
                }
            }
        }
    }
    for folder in new_folders {
        let stored = root
            .stored
            .join(folder.strip_prefix(&root.canonical).unwrap());
        scan.new_folders.push(entry(&root.stored, &stored, None));
    }
    Ok(())
}
/// Compares the catalog with the disk below its roots; read-only except for forgetting stale
/// remembered copies. `read` lists a directory (normally [`read_directory`]), `hash` hashes a
/// new file (normally [`hash_for_match`]).
pub fn scan(
    fs: &dyn DiskScanFs,
    conn: &Connection,
    read: &mut impl FnMut(&Path) -> std::io::Result<DirectoryEntries>,
    case_insensitive: bool,
    hash: &dyn Fn(&Path) -> Option<String>,
) -> SyncResult<DiskScan> {
    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let known_folders: HashSet<PathBuf> = folders
        .iter()
        .filter(|f| !f.path.is_empty())
        .map(|f| PathBuf::from(&f.path))
        .collect();
    let folder_keys: HashSet<_> = known_folders
        .iter()
        .map(|p| path_key(p, case_insensitive))
        .collect();
    let mut configured: Vec<_> = known_folders
        .iter()
        .filter(|p| {
            !p.ancestors()
                .skip(1)
                .any(|a| folder_keys.contains(&path_key(a, case_insensitive)))
        })
        .cloned()
        .collect();
    configured.sort();
    let mut result = DiskScan::default();
    for stored in &configured {
        match fs.canonicalize(stored) {
            Ok(canonical) if fs.is_dir(&canonical) => result.roots.push(DiskRoot {
                stored: stored.clone(),
                canonical,
            }),
            _ => result
                .unreachable_roots
                .push(stored.to_string_lossy().into_owned()),
        }
    }
    // Aliases and overlapping configured roots must not produce duplicate entries.
    result
        .roots
        .sort_by_key(|r| r.canonical.components().count());
    let roots = std::mem::take(&mut result.roots);
    let resolver = KeyResolver::with_fs(fs, &roots, case_insensitive);
    let normalize = |p: &Path| resolver.normalize(p);
    let known_folders = known_folders
        .iter()
        .map(|p| path_key(&normalize(p), case_insensitive))
        .collect();
    let mut stmt = conn
        .prepare("SELECT id, path, deleted_at IS NULL, folder_id IS NULL FROM files")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, bool>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let known_files = rows
        .iter()
        .map(|(_, p, _, _)| resolver.key(Path::new(p)))
        .collect();
    let active_files: HashSet<PathBuf> = rows
        .iter()
        .filter(|(_, _, active, _)| *active)
        .map(|(_, p, _, _)| resolver.key(Path::new(p)))
        .collect();
    let trashed_files: HashSet<PathBuf> = rows
        .iter()
        .filter(|(_, _, active, _)| !*active)
        .map(|(_, p, _, _)| resolver.key(Path::new(p)))
        .filter(|k| !active_files.contains(k))
        .collect();
    // Companion folders of catalog models, by the model's name and by where the
    // companion rows actually are.
    let known_extras: HashSet<PathBuf> = rows
        .iter()
        .filter_map(|(_, p, _, _)| crate::companions::extras_dir_for(Path::new(p)))
        .chain(
            db::companions::list_active_companion_paths(conn)
                .map_err(|e| e.to_string())?
                .iter()
                .filter_map(|p| Path::new(p).parent().map(Path::to_path_buf)),
        )
        .map(|dir| resolver.key(&dir))
        .collect();
    let mut seen_files = HashSet::new();
    for (index, root) in roots.iter().enumerate() {
        let checked = (|| -> std::io::Result<()> {
            // An empty directory holding catalogued files is most likely an unmounted
            // volume or a container without its model volume, not a real deletion.
            if read(&root.canonical)?.is_empty()
                && rows
                    .iter()
                    .any(|(_, path, active, _)| *active && root.resolve(Path::new(path)).is_some())
            {
                return Err(std::io::Error::other("diskSyncRootEmpty"));
            }
            if !roots[..index]
                .iter()
                .any(|r| root.canonical.starts_with(&r.canonical))
            {
                walk(
                    fs,
                    root,
                    &KnownPaths { folders: &known_folders, files: &known_files, trashed: &trashed_files, extras: &known_extras },
                    &mut result,
                    case_insensitive,
                    &mut seen_files,
                    read,
                )?;
            }
            for (id, path, active, _) in &rows {
                let Some(resolved) = root.resolve(Path::new(path)) else {
                    continue;
                };
                if !active
                    || result
                        .skipped_paths
                        .iter()
                        .any(|p| resolved.starts_with(normalize(Path::new(p))))
                {
                    continue;
                }
                let missing = no_symlinks(fs, &root.canonical, &resolved).and_then(|safe| {
                    if safe {
                        is_missing_fs(fs, &resolved)
                    } else {
                        Ok(false)
                    }
                });
                if missing.is_err() {
                    record_skipped(
                        &mut result,
                        root,
                        resolved.parent().unwrap_or(&root.canonical),
                    );
                }
                if missing.unwrap_or(false) {
                    result.missing_files.push(entry(
                        &root.stored,
                        Path::new(path),
                        Some(id.to_string()),
                    ));
                }
            }
            if !root.unchanged(fs) {
                return Err(std::io::Error::other("diskSyncPathChanged"));
            }
            Ok(())
        })();
        if checked.is_err() {
            result
                .unreachable_roots
                .push(root.stored.to_string_lossy().into_owned());
        }
    }
    for (id, path, active, unassigned) in &rows {
        let path = Path::new(path);
        if *active
            && *unassigned
            && !configured.iter().any(|root| path.starts_with(root))
            && !roots.iter().any(|root| root.resolve(path).is_some())
        {
            // Without its parent folder (unplugged drive) a loose file is unreachable, not missing.
            if !parent_exists(fs, path) {
                result
                    .unreachable_roots
                    .push(path.parent().unwrap_or(path).to_string_lossy().into_owned());
            } else if is_missing_fs(fs, path).unwrap_or(false) {
                result
                    .missing_files
                    .push(entry(Path::new(""), path, Some(id.to_string())));
            }
        }
    }
    // A later I/O error may have made an earlier missing result unsafe.
    result.missing_files.retain(|e| {
        !result
            .skipped_paths
            .iter()
            .any(|p| normalize(Path::new(&e.path)).starts_with(normalize(Path::new(p))))
    });
    if case_insensitive {
        // A directory entry proves presence even when the stored spelling cannot be opened.
        result.missing_files.retain(|e| {
            !seen_files.contains(&resolver.key(Path::new(&e.path)))
        });
        suppress_case_only_new_files(&mut result, case_insensitive);
    }
    result.roots = roots;
    if !result.unreachable_roots.is_empty() {
        result.new_folders.clear();
        result.new_files.clear();
        result.missing_files.clear();
        result.trash_blocked_count = 0;
    }
    result.new_folders.sort_by(|a, b| a.path.cmp(&b.path));
    result.new_files.sort_by(|a, b| a.path.cmp(&b.path));
    result.missing_files.sort_by(|a, b| a.path.cmp(&b.path));
    result.missing_files.dedup_by(|a, b| a.id == b.id);
    if result.unreachable_roots.is_empty() {
        skip_known_copies(fs, conn, &mut result, case_insensitive)?;
        classify_by_content(fs, conn, &mut result, case_insensitive, hash)?;
        // After the content match: a folder whose files were recognised as moved is not gone.
        result.missing_folders = find_missing_folders(fs, conn, &result, case_insensitive)?;
    }
    result.unreachable_roots.sort();
    result.unreachable_roots.dedup();
    result.skipped_paths.sort();
    result.skipped_paths.dedup();
    Ok(result)
}

/// Hash used to recognise a file again. Same function as the import and the stored
/// `content_hash`, so equal content always yields equal hashes. Files above the import
/// size limit and unreadable files get no hash and simply stay "new".
pub fn hash_for_match(fs: &dyn DiskScanFs, effects: &dyn DiskSyncEffects, path: &Path) -> Option<String> {
    let meta = fs.metadata(path).ok()?;
    if !meta.is_file() || meta.len() > crate::safe_file::MAX_MODEL_FILE_BYTES {
        return None;
    }
    effects.content_hash(path)
}

fn extension_key(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_lowercase())
}

/// Where a listed entry lies on disk (its stored spelling resolved onto the real root).
fn disk_path_of(scan: &DiskScan, e: &DiskEntry) -> Option<PathBuf> {
    scan.roots
        .iter()
        .find(|r| r.stored == Path::new(&e.root))
        .and_then(|r| r.resolve(Path::new(&e.path)))
}

/// Takes the copies the user declined earlier out of the new files without reading them.
/// A remembered copy counts only while the file lies at the same path with the same size
/// and a present, not missing catalog row still has its content hash; otherwise the row
/// is forgotten and the file is judged like any new file, so a copy is offered again as
/// soon as its original is gone. Size instead of a new hash is what saves the time; a
/// file that changes its content at the same size stays remembered.
pub fn skip_known_copies(fs: &dyn DiskScanFs, conn: &Connection, scan: &mut DiskScan, case_insensitive: bool) -> SyncResult<()> {
    let known = db::known_copies::list_known_copies(conn).map_err(|e| e.to_string())?;
    if known.is_empty() {
        return Ok(());
    }
    let missing_ids: HashSet<&str> = scan.missing_files.iter().filter_map(|e| e.id.as_deref()).collect();
    let mut live_hashes: HashSet<String> = HashSet::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, content_hash FROM files WHERE deleted_at IS NULL AND content_hash IS NOT NULL")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?.to_string(), r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, hash) = row.map_err(|e| e.to_string())?;
            if !missing_ids.contains(id.as_str()) {
                live_hashes.insert(hash);
            }
        }
    }
    let new_by_key: HashMap<PathBuf, usize> = scan
        .new_files
        .iter()
        .enumerate()
        .map(|(i, e)| (path_key(Path::new(&e.path), case_insensitive), i))
        .collect();
    let mut skipped: HashSet<usize> = HashSet::new();
    let mut stale: Vec<i64> = Vec::new();
    for row in &known {
        let index = new_by_key.get(&path_key(Path::new(&row.path), case_insensitive)).copied();
        let still_a_copy = index.is_some_and(|i| {
            live_hashes.contains(&row.content_hash)
                && disk_path_of(scan, &scan.new_files[i])
                    .and_then(|p| fs.metadata(&p).ok())
                    .is_some_and(|m| m.is_file() && i64::try_from(m.len()).ok() == Some(row.size_bytes))
        });
        match index {
            Some(i) if still_a_copy => {
                skipped.insert(i);
            }
            _ => stale.push(row.id),
        }
    }
    // Housekeeping only; a failure here must not stop the scan.
    if !stale.is_empty() {
        let _ = db::known_copies::forget_known_copies(conn, &stale);
    }
    if skipped.is_empty() {
        return Ok(());
    }
    let skipped_dirs: Vec<PathBuf> = skipped
        .iter()
        .filter_map(|&i| Path::new(&scan.new_files[i].path).parent().map(|p| path_key(p, case_insensitive)))
        .collect();
    let mut index = 0;
    scan.new_files.retain(|_| {
        index += 1;
        !skipped.contains(&(index - 1))
    });
    scan.known_copy_count = skipped.len();
    // A folder that only held remembered copies has nothing left to offer.
    let remaining: Vec<PathBuf> = scan
        .new_files
        .iter()
        .map(|e| path_key(Path::new(&e.path), case_insensitive))
        .collect();
    scan.new_folders.retain(|folder| {
        let folder = path_key(Path::new(&folder.path), case_insensitive);
        !skipped_dirs.iter().any(|dir| dir.starts_with(&folder)) || remaining.iter().any(|file| file.starts_with(&folder))
    });
    Ok(())
}

/// Splits the new files by content: a clear rename partner among the missing rows, or a
/// copy of a row that is still there. Only new files are hashed (never the whole
/// catalog), so a scan of a large, unchanged catalog costs nothing extra.
pub fn classify_by_content(
    fs: &dyn DiskScanFs,
    conn: &Connection,
    scan: &mut DiskScan,
    case_insensitive: bool,
    hash: &dyn Fn(&Path) -> Option<String>,
) -> SyncResult<()> {
    if scan.new_files.is_empty() {
        return Ok(());
    }
    let missing_ids: HashSet<&str> = scan
        .missing_files
        .iter()
        .filter_map(|e| e.id.as_deref())
        .collect();
    // Rows without a stored hash can never be matched.
    let mut missing_by_hash: HashMap<String, Vec<String>> = HashMap::new();
    let mut present_by_hash: HashMap<String, (String, String)> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, path, content_hash FROM files
                 WHERE deleted_at IS NULL AND content_hash IS NOT NULL ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?.to_string(),
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, name, path, content_hash) = row.map_err(|e| e.to_string())?;
            if missing_ids.contains(id.as_str()) {
                missing_by_hash.entry(content_hash).or_default().push(id);
            } else {
                // The lowest id wins, so the hint is stable between scans.
                present_by_hash.entry(content_hash).or_insert((name, path));
            }
        }
    }
    if missing_by_hash.is_empty() && present_by_hash.is_empty() {
        return Ok(());
    }
    let mut new_by_hash: HashMap<String, Vec<usize>> = HashMap::new();
    let mut sizes: HashMap<usize, i64> = HashMap::new();
    for (index, e) in scan.new_files.iter().enumerate() {
        let disk_path = disk_path_of(scan, e);
        // Size first: it is what a remembered copy is later compared with.
        let size = disk_path.as_ref().and_then(|p| fs.metadata(p).ok()).map_or(0, |m| m.len() as i64);
        if let Some(content_hash) = disk_path.and_then(|p| hash(&p)) {
            sizes.insert(index, size);
            new_by_hash.entry(content_hash).or_default().push(index);
        }
    }
    let mut moved = HashSet::new();
    let mut renamed_ids = HashSet::new();
    for (content_hash, indices) in &new_by_hash {
        if let Some((name, path)) = present_by_hash.get(content_hash) {
            // Even when a missing row has this content too: a copy of a present model is
            // never silently treated as the move of another one.
            for &i in indices {
                moved.insert(i);
                scan.duplicate_files.push(DiskDuplicate {
                    entry: scan.new_files[i].clone(),
                    duplicate_of_name: name.clone(),
                    duplicate_of_path: path.clone(),
                    content_hash: content_hash.clone(),
                    size_bytes: sizes.get(&i).copied().unwrap_or(0),
                });
            }
            continue;
        }
        let (Some(ids), [index]) = (missing_by_hash.get(content_hash), indices.as_slice()) else {
            continue;
        };
        let [id] = ids.as_slice() else { continue };
        let Some(old) = scan
            .missing_files
            .iter()
            .find(|e| e.id.as_deref() == Some(id.as_str()))
        else {
            continue;
        };
        let new = &scan.new_files[*index];
        // A changed extension would also change the file type; leave that to the user.
        if extension_key(Path::new(&old.path)) != extension_key(Path::new(&new.path)) {
            continue;
        }
        moved.insert(*index);
        renamed_ids.insert(id.clone());
        scan.renamed_files.push(DiskRename {
            id: id.clone(),
            old_path: old.path.clone(),
            old_relative_path: old.relative_path.clone(),
            old_root: old.root.clone(),
            new_path: new.path.clone(),
            new_relative_path: new.relative_path.clone(),
            root: new.root.clone(),
        });
    }
    if moved.is_empty() {
        return Ok(());
    }
    let mut index = 0;
    scan.new_files.retain(|_| {
        index += 1;
        !moved.contains(&(index - 1))
    });
    scan.missing_files
        .retain(|e| !e.id.as_ref().is_some_and(|id| renamed_ids.contains(id)));
    // A folder that now only holds renamed files is not new: the files bring their
    // catalog folder with them.
    let remaining: Vec<PathBuf> = scan
        .new_files
        .iter()
        .chain(scan.duplicate_files.iter().map(|d| &d.entry))
        .map(|e| path_key(Path::new(&e.path), case_insensitive))
        .collect();
    scan.new_folders.retain(|folder| {
        let folder = path_key(Path::new(&folder.path), case_insensitive);
        remaining.iter().any(|file| file.starts_with(&folder))
    });
    scan.renamed_files.sort_by(|a, b| a.new_path.cmp(&b.new_path));
    scan.duplicate_files.sort_by(|a, b| a.entry.path.cmp(&b.entry.path));
    Ok(())
}

/// Topmost catalog folders (never a catalog root) whose directory is gone while the
/// root they belong to was readable. A root that cannot be read is reported through
/// `unreachable_roots` instead, so an unplugged drive never looks like deleted folders.
/// Folders holding files that were recognised as renamed or moved are left out: they most
/// likely moved with their content, and the apply step follows them.
fn find_missing_folders(
    fs: &dyn DiskScanFs,
    conn: &Connection,
    scan: &DiskScan,
    case_insensitive: bool,
) -> SyncResult<Vec<DiskFolderEntry>> {
    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let sep = std::path::MAIN_SEPARATOR;
    let moved_from: Vec<PathBuf> = scan
        .renamed_files
        .iter()
        .map(|r| path_key(Path::new(&r.old_path), case_insensitive))
        .collect();
    let skipped: Vec<&Path> = scan.skipped_paths.iter().map(Path::new).collect();
    let mut gone: HashSet<i64> = HashSet::new();
    let mut moved: HashSet<i64> = HashSet::new();
    for folder in &folders {
        let path = Path::new(&folder.path);
        if folder.path.is_empty() || scan.roots.iter().any(|r| r.stored == path) {
            continue;
        }
        let Some((root, resolved)) = scan.roots.iter().find_map(|r| r.resolve(path).map(|p| (r, p)))
        else {
            continue;
        };
        let missing = no_symlinks(fs, &root.canonical, &resolved)
            .and_then(|safe| if safe { is_missing_fs(fs, &resolved) } else { Ok(false) });
        if !missing.unwrap_or(false) || skipped.iter().any(|s| path.starts_with(s)) {
            continue;
        }
        gone.insert(folder.id);
        let prefix = path_key(Path::new(&format!("{}{sep}", folder.path)), case_insensitive);
        if moved_from.iter().any(|old| old.starts_with(&prefix)) {
            moved.insert(folder.id);
        }
    }
    let parent_of: HashMap<i64, Option<i64>> = folders.iter().map(|f| (f.id, f.parent_id)).collect();
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    for f in &folders {
        if let Some(parent) = f.parent_id {
            children.entry(parent).or_default().push(f.id);
        }
    }
    let subtree = |id: i64| {
        let mut ids = vec![id];
        let mut i = 0;
        while i < ids.len() {
            ids.extend(children.get(&ids[i]).into_iter().flatten().copied());
            i += 1;
        }
        ids
    };
    // Everything below a folder that moved went with it, even subfolders without files.
    let moved_subtrees: HashSet<i64> = moved.iter().flat_map(|id| subtree(*id)).collect();
    let reportable: HashSet<i64> = gone.difference(&moved_subtrees).copied().collect();
    let mut per_folder: HashMap<i64, i64> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT folder_id, COUNT(*) FROM files
                 WHERE folder_id IS NOT NULL AND deleted_at IS NULL GROUP BY folder_id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, n) = row.map_err(|e| e.to_string())?;
            per_folder.insert(id, n);
        }
    }
    let mut found = Vec::new();
    for folder in &folders {
        let parent_reported = parent_of
            .get(&folder.id)
            .copied()
            .flatten()
            .is_some_and(|p| reportable.contains(&p));
        if !reportable.contains(&folder.id) || parent_reported {
            continue;
        }
        let path = Path::new(&folder.path);
        let Some(root) = scan.roots.iter().find(|r| r.resolve(path).is_some()) else {
            continue;
        };
        let DiskEntry { path, relative_path, root, .. } = entry(&root.stored, path, None);
        found.push(DiskFolderEntry {
            id: folder.id.to_string(),
            path,
            relative_path,
            root,
            model_count: subtree(folder.id).iter().filter_map(|id| per_folder.get(id)).sum(),
        });
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

/// Drops the rows of folders reported as missing, after their missing models went to the
/// trash. A folder that still holds an active model (one the user did not see as missing)
/// or contains the catalog base directory is kept.
fn remove_missing_folders(tx: &Connection, current: &DiskScan) -> SyncResult<()> {
    if current.missing_folders.is_empty() {
        return Ok(());
    }
    let folders = db::list_folders(tx).map_err(|e| e.to_string())?;
    let base: Option<String> = tx
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [SETTING_CATALOG_BASE_DIR],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    for reported in &current.missing_folders {
        let Ok(id) = reported.id.parse::<i64>() else {
            continue;
        };
        let mut ids = vec![id];
        let mut i = 0;
        while i < ids.len() {
            let parent = ids[i];
            ids.extend(folders.iter().filter(|f| f.parent_id == Some(parent)).map(|f| f.id));
            i += 1;
        }
        let in_subtree = |f: &&db::models::FolderRecord| ids.contains(&f.id);
        if !folders.iter().filter(in_subtree).any(|f| f.id == id)
            || folders.iter().filter(in_subtree).any(|f| Some(&f.path) == base.as_ref())
        {
            continue;
        }
        let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        let active: i64 = tx
            .query_row(
                &format!("SELECT COUNT(*) FROM files WHERE folder_id IN ({list}) AND deleted_at IS NULL"),
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if active == 0 {
            tx.execute(&format!("DELETE FROM folders WHERE id IN ({list})"), [])
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn parent_exists(fs: &dyn DiskScanFs, path: &Path) -> bool {
    path.parent()
        .is_some_and(|p| fs.metadata(p).is_ok_and(|m| m.is_dir()))
}

pub fn suppress_case_only_new_files(scan: &mut DiskScan, case_insensitive: bool) {
    let missing: HashSet<_> = scan
        .missing_files
        .iter()
        .map(|e| path_key(Path::new(&e.path), case_insensitive))
        .collect();
    scan.new_files
        .retain(|e| !missing.contains(&path_key(Path::new(&e.path), case_insensitive)));
    scan.new_folders.retain(|folder| {
        scan.new_files.iter().any(|file| {
            path_key(Path::new(&file.path), case_insensitive)
                .starts_with(path_key(Path::new(&folder.path), case_insensitive))
        })
    });
}

fn checked_path(fs: &dyn DiskScanFs, scan: &DiskScan, e: &DiskEntry, kind: &str) -> SyncResult<PathBuf> {
    let changed = || DiskSyncError::path("diskSyncPathChanged", Path::new(&e.path));
    if kind == "missing" && e.root.is_empty() {
        let path = Path::new(&e.path);
        return if parent_exists(fs, path) && is_missing_fs(fs, path).unwrap_or(false) {
            Ok(path.to_path_buf())
        } else {
            Err(changed())
        };
    }
    let root = scan
        .roots
        .iter()
        .find(|r| r.stored == Path::new(&e.root))
        .ok_or_else(changed)?;
    if !root.unchanged(fs) {
        return Err(DiskSyncError::path("diskSyncRootUnreachable", &root.stored));
    }
    let path = root.resolve(Path::new(&e.path)).ok_or_else(changed)?;
    if !no_symlinks(fs, &root.canonical, &path).map_err(|_| changed())? {
        return Err(changed());
    }
    let valid = match kind {
        "missing" => is_missing_fs(fs, &path).unwrap_or(false),
        "folder" => fs.symlink_metadata(&path).is_ok_and(|m| m.is_dir()),
        _ => fs.symlink_metadata(&path).is_ok_and(|m| m.is_file()),
    };
    if !valid {
        return Err(changed());
    }
    Ok(path)
}
// Reuse existing folder spellings (including aliases) without rewriting rows.
fn ensure_sync_folder(
    conn: &Connection,
    scan: &DiskScan,
    root: &Path,
    dir: &Path,
    case_insensitive: bool,
) -> SyncResult<i64> {
    let resolve = |p: &Path| scan.roots.iter().find_map(|r| r.resolve(p));
    let canonical = resolve(dir).ok_or_else(|| DiskSyncError::path("diskSyncPathChanged", dir))?;
    let folders: HashMap<_, _> = db::list_folders(conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|folder| {
            resolve(Path::new(&folder.path)).map(|path| (path_key(&path, case_insensitive), folder))
        })
        .collect();
    for ancestor in canonical.ancestors() {
        if let Some(folder) = folders.get(&path_key(ancestor, case_insensitive)) {
            let relative = canonical.strip_prefix(ancestor).unwrap();
            if relative.as_os_str().is_empty() {
                return Ok(folder.id);
            }
            let stored = Path::new(&folder.path);
            return db::ensure_folder_path(conn, stored, &stored.join(relative))
                .map_err(|e| e.to_string().into());
        }
    }
    db::ensure_folder_path(conn, root, dir).map_err(|e| e.to_string().into())
}
/// Checks a chosen pairing against the disk as it is now. The scan matched hashes a
/// moment ago, but parsing other files can take minutes and the user may have edited the
/// target in between. Quick pre-filter only: `confirm_rename_target` decides right before
/// the row is switched.
pub fn verify_rename(
    fs: &dyn DiskScanFs,
    effects: &dyn DiskSyncEffects,
    conn: &Connection,
    scan: &DiskScan,
    rename: &DiskRename,
    expanded_sensitive: &[PathBuf],
) -> Result<(), String> {
    confirm_rename_target(fs, effects, conn, scan, rename)?;
    let new_path = Path::new(&rename.new_path);
    let parent = new_path.parent().unwrap_or(new_path);
    effects.check_managed_location(parent, expanded_sensitive)
}

/// The target must still hold the content stored for the row. Without this a different
/// model put there in the meantime would inherit the row's tags, favourite and print log.
fn confirm_rename_target(
    fs: &dyn DiskScanFs,
    effects: &dyn DiskSyncEffects,
    conn: &Connection,
    scan: &DiskScan,
    rename: &DiskRename,
) -> Result<(), String> {
    let id: i64 = rename.id.parse().map_err(|_| "diskSyncStale".to_string())?;
    let stored: Option<String> = conn
        .query_row(
            "SELECT content_hash FROM files WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let target = scan
        .roots
        .iter()
        .find(|r| r.stored == Path::new(&rename.root))
        .and_then(|r| r.resolve(Path::new(&rename.new_path)))
        .ok_or("diskSyncPathChanged")?;
    match (stored, hash_for_match(fs, effects, &target)) {
        (Some(stored), Some(current)) if stored == current => {}
        _ => return Err("Inhalt der Datei hat sich geändert".into()),
    }
    Ok(())
}

/// Points an existing catalog row at its new location. Everything attached to the row
/// (tags, favourite, notes, print log, collections) hangs on `id` and stays.
pub fn apply_rename(
    tx: &Connection,
    scan: &DiskScan,
    rename: &DiskRename,
    known_files: &mut HashSet<PathBuf>,
    key: &dyn Fn(&Path) -> PathBuf,
    case_insensitive: bool,
) -> Result<(), String> {
    let id: i64 = rename.id.parse().map_err(|_| "diskSyncStale".to_string())?;
    let current: String = tx
        .query_row(
            "SELECT path FROM files WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let (old_key, new_key) = (key(Path::new(&current)), key(Path::new(&rename.new_path)));
    // A folder rename may already have moved the row; otherwise the target must be free.
    if current != rename.new_path && old_key != new_key && known_files.contains(&new_key) {
        return Err("Zielpfad ist schon im Katalog".into());
    }
    let new_path = Path::new(&rename.new_path);
    let name = new_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("invalid path")?;
    let folder_id = ensure_sync_folder(
        tx,
        scan,
        Path::new(&rename.root),
        new_path.parent().ok_or("invalid path")?,
        case_insensitive,
    )
    .map_err(|e| e.code.to_string())?;
    tx.execute(
        "UPDATE files SET name = ?1, path = ?2, folder_id = ?3 WHERE id = ?4",
        rusqlite::params![name, rename.new_path, folder_id, id],
    )
    .map_err(|e| e.to_string())?;
    known_files.remove(&old_key);
    known_files.insert(new_key);
    Ok(())
}

/// When every file below a catalog folder was renamed together with the folder (same
/// new parent, same layout beneath), the folder row itself follows instead of leaving an
/// empty old row beside a new one. Anything less clear leaves the folder alone: the old
/// row then stays and is treated as before. Best effort, never fails the sync.
fn move_emptied_folders(
    fs: &dyn DiskScanFs,
    effects: &dyn DiskSyncEffects,
    tx: &Connection,
    scan: &DiskScan,
    renames: &[&DiskRename],
    expanded_sensitive: &[PathBuf],
) {
    let sep = std::path::MAIN_SEPARATOR;
    let Ok(folders) = db::list_folders(tx) else {
        return;
    };
    let by_id: HashMap<&str, &DiskRename> = renames.iter().map(|r| (r.id.as_str(), *r)).collect();
    let mut candidates: Vec<&db::models::FolderRecord> = folders
        .iter()
        .filter(|f| {
            f.parent_id.is_some()
                && renames
                    .iter()
                    .any(|r| r.old_path.starts_with(&format!("{}{sep}", f.path)))
        })
        .collect();
    candidates.sort_by_key(|f| f.path.len());
    let mut moved: Vec<&str> = Vec::new();
    for folder in candidates {
        // Rows below a folder that was just moved no longer carry their old paths.
        if moved
            .iter()
            .any(|m| folder.path.starts_with(&format!("{m}{sep}")))
        {
            continue;
        }
        let Some(target) = emptied_folder_target(fs, effects, tx, scan, &folders, folder, &by_id, expanded_sensitive)
        else {
            continue;
        };
        if tx.execute_batch("SAVEPOINT disk_sync_folder").is_err() {
            return;
        }
        let done = (|| -> Result<(), String> {
            let (parent_dir, name) = (
                target.parent().ok_or("no parent")?,
                target.file_name().ok_or("no name")?.to_string_lossy(),
            );
            let stored_root = scan
                .roots
                .iter()
                .find(|r| r.resolve(&target).is_some())
                .ok_or("outside roots")?
                .stored
                .clone();
            let parent_id = ensure_sync_folder(tx, scan, &stored_root, parent_dir, CASE_INSENSITIVE_PATHS)
                .map_err(|e| e.code.to_string())?;
            db::rename_folder_name(tx, folder.id, &name).map_err(|e| e.to_string())?;
            db::set_folder_parent(tx, folder.id, Some(parent_id)).map_err(|e| e.to_string())?;
            db::update_paths_under_folder(tx, folder.id, &folder.path, &target.to_string_lossy())
                .map_err(|e| e.to_string())
        })();
        let finish = if done.is_ok() {
            "RELEASE disk_sync_folder"
        } else {
            "ROLLBACK TO disk_sync_folder; RELEASE disk_sync_folder"
        };
        if tx.execute_batch(finish).is_err() {
            return;
        }
        if done.is_ok() {
            moved.push(&folder.path);
        }
    }
}

/// True when something still exists at `old` that is not just `new` under another
/// spelling (a case-only rename on a case-insensitive file system).
fn remains_at_old_location(fs: &dyn DiskScanFs, old: &Path, new: &Path) -> bool {
    fs.symlink_metadata(old).is_ok()
        && match (fs.canonicalize(old), fs.canonicalize(new)) {
            (Ok(old), Ok(new)) => old != new,
            _ => true,
        }
}

/// The new location of `folder` if the renames describe a pure folder rename or move.
// The two traits plus the state of the step; a struct would only rename the parameters.
#[allow(clippy::too_many_arguments)]
fn emptied_folder_target(
    fs: &dyn DiskScanFs,
    effects: &dyn DiskSyncEffects,
    tx: &Connection,
    scan: &DiskScan,
    folders: &[db::models::FolderRecord],
    folder: &db::models::FolderRecord,
    by_id: &HashMap<&str, &DiskRename>,
    expanded_sensitive: &[PathBuf],
) -> Option<PathBuf> {
    let sep = std::path::MAIN_SEPARATOR;
    let prefix = format!("{}{sep}", folder.path);
    // Trash rows and rows of other states count too: they would move with the folder.
    let mut stmt = tx
        .prepare("SELECT id, path FROM files WHERE substr(path, 1, length(?1)) = ?1")
        .ok()?;
    let below: Vec<(i64, String)> = stmt
        .query_map([&prefix], |r| Ok((r.get(0).ok().unwrap_or(0), r.get(1)?)))
        .ok()?
        .collect::<Result<_, _>>()
        .ok()?;
    let mut new_prefix: Option<String> = None;
    for (id, path) in &below {
        let rename = by_id.get(id.to_string().as_str())?;
        if rename.old_path != *path {
            return None;
        }
        let suffix = path.strip_prefix(&prefix)?;
        let start = rename.new_path.strip_suffix(suffix)?;
        if new_prefix.get_or_insert_with(|| start.to_string()) != start {
            return None;
        }
    }
    let target = PathBuf::from(new_prefix?.strip_suffix(sep)?);
    // Rows assigned to the folder (or below) whose path points elsewhere would be mangled.
    let mut subtree = vec![folder.id];
    let mut i = 0;
    while i < subtree.len() {
        let parent = subtree[i];
        subtree.extend(
            folders
                .iter()
                .filter(|f| f.parent_id == Some(parent))
                .map(|f| f.id),
        );
        i += 1;
    }
    for id in &subtree {
        let mut stmt = tx.prepare("SELECT path FROM files WHERE folder_id = ?1").ok()?;
        let paths: Vec<String> = stmt
            .query_map([id], |r| r.get(0))
            .ok()?
            .collect::<Result<_, _>>()
            .ok()?;
        if paths.iter().any(|p| !p.starts_with(&prefix)) {
            return None;
        }
    }
    let old = Path::new(&folder.path);
    // Every file moved away, but the folder (or a catalogued subfolder, possibly empty)
    // may still be there. Its row then describes something that did not move.
    let subfolder_remains = folders
        .iter()
        .filter(|f| subtree.contains(&f.id))
        .any(|f| {
            let stored = Path::new(&f.path);
            let relocated = stored
                .strip_prefix(old)
                .map_or_else(|_| target.clone(), |rest| target.join(rest));
            remains_at_old_location(fs, stored, &relocated)
        });
    let usable = !subfolder_remains
        && target != old
        && !target.starts_with(old)
        && !folders.iter().any(|f| Path::new(&f.path) == target)
        && scan
            .roots
            .iter()
            .any(|r| r.resolve(&target).is_some() && r.stored != target && r.canonical != target)
        && effects.may_relocate_folder(tx, old)
        && effects.check_managed_location(&target, expanded_sensitive).is_ok();
    usable.then_some(target)
}

/// Takes over what the user chose: new folders and files, detected renames, missing rows
/// (with `remove_missing`) and declined copies. Re-scans first; the selection is only a
/// choice among what that scan offers. `read` reads a model for import (its error text ends
/// up in `failed_paths`).
pub fn apply(
    fs: &dyn DiskScanFs,
    effects: &dyn DiskSyncEffects,
    conn: &mut Connection,
    selection: DiskSelection,
    mut read: impl FnMut(&Path) -> Result<db::models::NewFile, String>,
    case_insensitive: bool,
) -> SyncResult<DiskApplied> {
    // Re-scan under the exclusive catalog gate; the webview is not authoritative.
    let current = scan(fs, conn, &mut |dir| read_directory(fs, dir), case_insensitive, &|p| hash_for_match(fs, effects, p))?;
    if !current.unreachable_roots.is_empty() {
        return Err(DiskSyncError::new(
            "diskSyncRootUnreachable",
            current.unreachable_roots,
        ));
    }
    let chosen: HashSet<_> = selection.new_paths.into_iter().collect();
    // Copies of present models are offered separately but are imported like any new file.
    let importable: Vec<&DiskEntry> = current
        .new_files
        .iter()
        .chain(current.duplicate_files.iter().map(|d| &d.entry))
        .collect();
    let available: HashSet<_> = current
        .new_folders
        .iter()
        .chain(importable.iter().copied())
        .map(|e| &e.path)
        .collect();
    let missing: HashSet<_> = selection.missing_ids.into_iter().collect();
    let missing_now: HashSet<_> = current
        .missing_files
        .iter()
        .filter_map(|e| e.id.as_ref())
        .collect();
    let chosen_renames: Vec<&DiskRename> = selection
        .renames
        .iter()
        .map(|choice| {
            current
                .renamed_files
                .iter()
                .find(|r| r.id == choice.id && r.new_path == choice.new_path)
        })
        .collect::<Option<_>>()
        .ok_or_else(|| DiskSyncError::new("diskSyncStale", vec![]))?;
    if chosen.iter().any(|p| !available.contains(p))
        || (selection.remove_missing && missing.iter().any(|id| !missing_now.contains(id)))
    {
        return Err(DiskSyncError::new("diskSyncStale", vec![]));
    }
    // Parsing, thumbnails and hashing can take minutes. Keep SQLite writable.
    let mut prepared = Vec::new();
    let mut result = DiskApplied::default();
    let mut renames: Vec<&DiskRename> = Vec::new();
    let expanded_sensitive = expand_sensitive_dirs(&selection.sensitive_dirs);
    for r in chosen_renames {
        if renames.iter().any(|known| known.id == r.id) {
            continue;
        }
        match verify_rename(fs, effects, conn, &current, r, &expanded_sensitive) {
            Ok(()) => renames.push(r),
            Err(reason) => result.failed_paths.push(DiskFailedPath {
                path: r.new_path.clone(),
                reason,
            }),
        }
    }
    for e in importable.iter().copied().filter(|e| chosen.contains(&e.path)) {
        let path = checked_path(fs, &current, e, "file")?;
        let mut file = match read(&path) {
            Ok(file) => file,
            Err(error) => {
                result.failed_paths.push(DiskFailedPath {
                    path: e.path.clone(),
                    reason: error.chars().take(160).collect(),
                });
                continue;
            }
        };
        file.path = e.path.clone();
        prepared.push((e, file));
    }
    let validate = || -> SyncResult<()> {
        for root in &current.roots {
            if !root.unchanged(fs) {
                return Err(DiskSyncError::path("diskSyncRootUnreachable", &root.stored));
            }
        }
        for e in current
            .new_folders
            .iter()
            .filter(|e| chosen.contains(&e.path))
        {
            checked_path(fs, &current, e, "folder")?;
        }
        for e in importable.iter().copied().filter(|e| chosen.contains(&e.path)) {
            checked_path(fs, &current, e, "file")?;
        }
        for r in &renames {
            checked_path(fs, &current, &r.new_entry(), "file")?;
            checked_path(fs, &current, &r.old_entry(), "missing")?;
        }
        if selection.remove_missing {
            for e in current
                .missing_files
                .iter()
                .filter(|e| e.id.as_ref().is_some_and(|id| missing.contains(id)))
            {
                checked_path(fs, &current, e, "missing")?;
            }
        }
        Ok(())
    };
    validate()?;
    let previous_folders: HashSet<_> = db::list_folders(conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|f| f.id)
        .collect();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    // Authoritative content check: all long reads are done, so this is the last look at
    // the targets before rows are switched. It runs before the folder step, which
    // rewrites paths for the pairings it covers.
    let mut confirmed: Vec<&DiskRename> = Vec::new();
    for r in &renames {
        match confirm_rename_target(fs, effects, &tx, &current, r) {
            Ok(()) => confirmed.push(r),
            Err(reason) => result.failed_paths.push(DiskFailedPath {
                path: r.new_path.clone(),
                reason,
            }),
        }
    }
    let renames = confirmed;
    // Before the path set is read: a renamed folder rewrites the paths below it.
    move_emptied_folders(fs, effects, &tx, &current, &renames, &expanded_sensitive);
    let resolver = KeyResolver::with_fs(fs, &current.roots, case_insensitive);
    let key = |p: &Path| resolver.key(p);
    let mut known_files: HashSet<_> = {
        let mut stmt = tx
            .prepare("SELECT path FROM files")
            .map_err(|e| e.to_string())?;
        let paths = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        paths.iter().map(|p| key(Path::new(p))).collect()
    };
    for r in &renames {
        // One savepoint per file: a collision fails this pairing only.
        tx.execute_batch("SAVEPOINT disk_sync_rename")
            .map_err(|e| e.to_string())?;
        match apply_rename(&tx, &current, r, &mut known_files, &key, case_insensitive) {
            Ok(()) => {
                tx.execute_batch("RELEASE disk_sync_rename")
                    .map_err(|e| e.to_string())?;
                result.renamed_ids.push(r.id.clone());
            }
            Err(reason) => {
                tx.execute_batch("ROLLBACK TO disk_sync_rename; RELEASE disk_sync_rename")
                    .map_err(|e| e.to_string())?;
                result.failed_paths.push(DiskFailedPath {
                    path: r.new_path.clone(),
                    reason,
                });
            }
        }
    }
    result.renamed_count = result.renamed_ids.len();
    for e in current
        .new_folders
        .iter()
        .filter(|e| chosen.contains(&e.path))
    {
        // Do not leave empty catalog folders when every contained import failed.
        if importable.iter().any(|file| {
            chosen.contains(&file.path)
                && Path::new(&file.path).starts_with(&e.path)
                && !result
                    .failed_paths
                    .iter()
                    .any(|failed| failed.path == file.path)
        }) {
            ensure_sync_folder(
                &tx,
                &current,
                Path::new(&e.root),
                Path::new(&e.path),
                case_insensitive,
            )?;
        }
    }
    for (e, mut file) in prepared {
        if !known_files.insert(key(Path::new(&file.path))) {
            continue;
        }
        file.folder_id = Some(ensure_sync_folder(
            &tx,
            &current,
            Path::new(&e.root),
            Path::new(&e.path).parent().unwrap(),
            case_insensitive,
        )?);
        // Each real disk path belongs in the catalog, even when its content hash already exists.
        result.file_ids.push(
            db::insert_file_within_tx(&tx, &file)
                .map_err(|e| e.to_string())?
                .to_string(),
        );
    }
    if selection.remove_missing {
        for e in current
            .missing_files
            .iter()
            .filter(|e| e.id.as_ref().is_some_and(|id| missing.contains(id)))
        {
            checked_path(fs, &current, e, "missing")?;
            let id =
                e.id.as_ref()
                    .unwrap()
                    .parse()
                    .map_err(|_| "diskSyncStale".to_string())?;
            effects.delete_missing_file(&tx, id).map_err(|message| DiskSyncError::from_message(&message))?;
            result.removed_count += 1;
        }
        remove_missing_folders(&tx, &current)?;
    }
    remember_declined_copies(&tx, &current, &selection.declined_copies, &chosen, case_insensitive, &mut result)?;
    result.folder_ids = db::list_folders(&tx)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|f| !previous_folders.contains(&f.id))
        .map(|f| f.id.to_string())
        .collect();
    validate()?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}

/// Remembers the copies the user left unticked. Only paths the fresh scan lists as copies
/// are taken (never one the webview merely names), so nothing outside the catalog roots
/// can end up in the table.
fn remember_declined_copies(
    tx: &Connection,
    current: &DiskScan,
    declined: &[String],
    chosen: &HashSet<String>,
    case_insensitive: bool,
    result: &mut DiskApplied,
) -> SyncResult<()> {
    if !declined.is_empty() {
        let declined: HashSet<&str> = declined.iter().map(String::as_str).collect();
        let existing = db::known_copies::list_known_copies(tx).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().to_rfc3339();
        for d in &current.duplicate_files {
            let path = d.entry.path.as_str();
            if !declined.contains(path) || chosen.contains(path) || d.content_hash.is_empty() {
                continue;
            }
            // The table is unique by spelling; where paths are case-folded another spelling is the same file.
            let key = path_key(Path::new(path), case_insensitive);
            let other_spellings: Vec<i64> = existing
                .iter()
                .filter(|row| row.path != path && path_key(Path::new(&row.path), case_insensitive) == key)
                .map(|row| row.id)
                .collect();
            db::known_copies::forget_known_copies(tx, &other_spellings).map_err(|e| e.to_string())?;
            db::known_copies::note_known_copy(tx, path, d.size_bytes, &d.content_hash, &now)
                .map_err(|e| e.to_string())?;
            result.noted_copy_count += 1;
        }
        db::known_copies::trim_known_copies(tx).map_err(|e| e.to_string())?;
    }
    result.known_copy_count = db::known_copies::count_known_copies(tx).map_err(|e| e.to_string())?;
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    const OBJ_A: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";

    fn read(path: &Path) -> Result<db::models::NewFile, String> {
        let mut file = crate::import::files::read_model_file(path, None)?;
        file.content_hash = Some(crate::import::files::compute_content_hash(path).map_err(|e| e.message)?);
        Ok(file)
    }

    fn scan_std(conn: &Connection, case_insensitive: bool) -> DiskScan {
        let fs = &StdDiskScanFs;
        scan(fs, conn, &mut |dir| read_directory(fs, dir), case_insensitive, &|p| hash_for_match(fs, &StdDiskSyncEffects, p)).unwrap()
    }

    #[test]
    fn path_keys_fold_only_when_requested() {
        let lower = Path::new("Spielzeug/Grün.3mf");
        let upper = Path::new("SPIELZEUG/GRÜN.3MF");
        assert_eq!(path_key(lower, true), path_key(upper, true));
        assert_ne!(path_key(lower, false), path_key(upper, false));
    }

    /// New file taken over, then renamed outside the app: the row follows, nothing is new or missing.
    #[test]
    fn std_scan_apply_and_rename_round_trip() {
        let dir = crate::unique_test_dir("core_disk_sync").canonicalize().unwrap();
        let root = dir.join("models");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.obj"), OBJ_A).unwrap();
        let mut conn = db::connect_in_memory().unwrap();
        db::ensure_folder_path(&conn, &root, &root).unwrap();
        let plan = scan_std(&conn, false);
        assert_eq!(plan.new_files.len(), 1);
        let selection = DiskSelection { new_paths: plan.new_files.iter().map(|e| e.path.clone()).collect(), ..Default::default() };
        let applied = apply(&StdDiskScanFs, &StdDiskSyncEffects, &mut conn, selection, read, false).unwrap();
        assert_eq!(applied.file_ids.len(), 1);
        std::fs::rename(root.join("a.obj"), root.join("b.obj")).unwrap();
        let plan = scan_std(&conn, false);
        assert_eq!((plan.new_files.len(), plan.missing_files.len(), plan.renamed_files.len()), (0, 0, 1));
        let renames = plan.renamed_files.iter().map(|r| RenameChoice { id: r.id.clone(), new_path: r.new_path.clone() }).collect();
        let applied = apply(&StdDiskScanFs, &StdDiskSyncEffects, &mut conn, DiskSelection { renames, ..Default::default() }, read, false).unwrap();
        assert_eq!(applied.renamed_count, 1);
        let after = scan_std(&conn, false);
        assert!(after.new_files.is_empty() && after.missing_files.is_empty() && after.renamed_files.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn error_codes_from_messages() {
        assert_eq!(DiskSyncError::from_message("importActive").code, "importActive");
        assert_eq!(DiskSyncError::from_message("database error: x").code, "diskSyncFailed");
        assert_eq!(DiskSyncError::from(String::from("x")).code, "diskSyncFailed");
    }
}
