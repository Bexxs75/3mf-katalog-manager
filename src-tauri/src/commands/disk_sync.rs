use super::*;
use serde::Deserialize;
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskEntry {
    path: String,
    relative_path: String,
    root: String,
    id: Option<String>,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskScan {
    new_folders: Vec<DiskEntry>,
    new_files: Vec<DiskEntry>,
    /// Missing catalog rows that reappear as exactly one new file with the same content.
    renamed_files: Vec<DiskRename>,
    /// New files whose content already belongs to a present catalog row.
    duplicate_files: Vec<DiskDuplicate>,
    missing_files: Vec<DiskEntry>,
    unreachable_roots: Vec<String>,
    skipped_paths: Vec<String>,
    #[serde(skip)]
    roots: Vec<DiskRoot>,
}
/// A catalog row whose file was renamed or moved outside the app.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskRename {
    id: String,
    old_path: String,
    old_relative_path: String,
    old_root: String,
    new_path: String,
    new_relative_path: String,
    root: String,
}
impl DiskRename {
    fn old_entry(&self) -> DiskEntry {
        DiskEntry {
            path: self.old_path.clone(),
            relative_path: self.old_relative_path.clone(),
            root: self.old_root.clone(),
            id: Some(self.id.clone()),
        }
    }
    fn new_entry(&self) -> DiskEntry {
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
    entry: DiskEntry,
    duplicate_of_name: String,
    duplicate_of_path: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameChoice {
    id: String,
    new_path: String,
}
// Every field has a default so that callers written before renames existed still work.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DiskSelection {
    new_paths: Vec<String>,
    missing_ids: Vec<String>,
    remove_missing: bool,
    renames: Vec<RenameChoice>,
    // Filled by the command from app state, never taken from the webview.
    #[serde(skip)]
    sensitive_dirs: Vec<PathBuf>,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskApplied {
    file_ids: Vec<String>,
    folder_ids: Vec<String>,
    renamed_ids: Vec<String>,
    renamed_count: usize,
    removed_count: usize,
    failed_paths: Vec<DiskFailedPath>,
}

#[derive(Serialize)]
pub struct DiskFailedPath {
    path: String,
    reason: String,
}

fn entry(root: &Path, path: &Path, id: Option<String>) -> DiskEntry {
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
const CASE_INSENSITIVE_PATHS: bool = cfg!(any(windows, target_os = "macos"));

fn path_key(path: &Path, case_insensitive: bool) -> PathBuf {
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
struct KeyResolver<'a> {
    roots: &'a [DiskRoot],
    case_insensitive: bool,
    // One lookup per directory instead of per file; `None` remembers a failed lookup.
    dirs: std::cell::RefCell<HashMap<PathBuf, Option<PathBuf>>>,
}
impl<'a> KeyResolver<'a> {
    fn new(roots: &'a [DiskRoot], case_insensitive: bool) -> Self {
        Self {
            roots,
            case_insensitive,
            dirs: Default::default(),
        }
    }
    /// Maps a stored path below a configured root onto the canonical root.
    fn normalize(&self, path: &Path) -> PathBuf {
        self.roots
            .iter()
            .find_map(|r| r.resolve(path))
            .unwrap_or_else(|| path.to_path_buf())
    }
    fn canonical_dir(&self, dir: &Path) -> Option<PathBuf> {
        if let Some(known) = self.dirs.borrow().get(dir) {
            return known.clone();
        }
        let resolved = std::fs::canonicalize(dir).ok();
        self.dirs
            .borrow_mut()
            .insert(dir.to_path_buf(), resolved.clone());
        resolved
    }
    /// Falls back to the plain spelling whenever resolving fails (missing file or
    /// directory, no permission, symlink loop), so a broken alias never hides a file.
    fn key(&self, path: &Path) -> PathBuf {
        let normalized = self.normalize(path);
        let mut resolved = match (normalized.parent(), normalized.file_name()) {
            (Some(parent), Some(name)) => self
                .canonical_dir(parent)
                .map_or_else(|| normalized.clone(), |dir| dir.join(name)),
            _ => normalized,
        };
        // The file itself may be a symlink; the scan lists its target instead.
        if std::fs::symlink_metadata(&resolved).is_ok_and(|m| m.file_type().is_symlink()) {
            if let Ok(target) = std::fs::canonicalize(&resolved) {
                resolved = target;
            }
        }
        path_key(&resolved, self.case_insensitive)
    }
}

#[derive(Debug)]
struct DiskRoot {
    stored: PathBuf,
    canonical: PathBuf,
}
impl DiskRoot {
    fn resolve(&self, path: &Path) -> Option<PathBuf> {
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
    fn unchanged(&self) -> bool {
        std::fs::canonicalize(&self.stored).ok().as_ref() == Some(&self.canonical)
            && self.canonical.is_dir()
    }
}

#[derive(Debug, Serialize)]
pub struct DiskSyncError {
    code: &'static str,
    paths: Vec<String>,
}
type SyncResult<T> = Result<T, DiskSyncError>;
impl DiskSyncError {
    fn new(code: &'static str, paths: Vec<String>) -> Self {
        Self { code, paths }
    }
    fn path(code: &'static str, path: &Path) -> Self {
        Self::new(code, vec![path.to_string_lossy().into_owned()])
    }
}
impl From<String> for DiskSyncError {
    fn from(_: String) -> Self {
        Self::new("diskSyncFailed", vec![])
    }
}
impl From<CmdError> for DiskSyncError {
    fn from(error: CmdError) -> Self {
        Self::new(
            if error.message == "importActive" {
                "importActive"
            } else {
                "diskSyncFailed"
            },
            vec![],
        )
    }
}

// The canonical root is trusted; only descendants must be free of symlinks.
fn no_symlinks(root: &Path, path: &Path) -> std::io::Result<bool> {
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
        match std::fs::symlink_metadata(&current) {
            Ok(m) if m.file_type().is_symlink() => return Ok(false),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}
type DirectoryEntries = Vec<(PathBuf, std::fs::FileType)>;

fn read_directory(dir: &Path) -> std::io::Result<DirectoryEntries> {
    let mut entries = Vec::new();
    for item in std::fs::read_dir(dir)? {
        let item = match item {
            Ok(item) => item,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        if files::is_ignored_import_path(&item.path()) {
            continue;
        }
        match item.file_type() {
            Ok(kind) => entries.push((item.path(), kind)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(entries)
}

fn record_skipped(scan: &mut DiskScan, root: &DiskRoot, dir: &Path) {
    let stored = root.stored.join(dir.strip_prefix(&root.canonical).unwrap());
    scan.skipped_paths
        .push(stored.to_string_lossy().into_owned());
}

fn walk(
    root: &DiskRoot,
    known_folders: &HashSet<PathBuf>,
    known_files: &HashSet<PathBuf>,
    scan: &mut DiskScan,
    case_insensitive: bool,
    seen_files: &mut HashSet<PathBuf>,
    read: &mut impl FnMut(&Path) -> std::io::Result<DirectoryEntries>,
) -> std::io::Result<()> {
    let mut pending = vec![root.canonical.clone()];
    let mut new_folders = HashSet::new();
    while let Some(dir) = pending.pop() {
        let entries = no_symlinks(&root.canonical, &dir).and_then(|safe| {
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
                pending.push(path);
            } else if kind.is_file() && files::is_supported_extension(&path) {
                let key = path_key(&path, case_insensitive);
                seen_files.insert(key.clone());
                if known_files.contains(&key) {
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
                    if !known_folders.contains(&path_key(parent, case_insensitive)) {
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
fn scan(conn: &Connection) -> SyncResult<DiskScan> {
    scan_with_reader(conn, &mut read_directory)
}

fn scan_with_reader(
    conn: &Connection,
    read: &mut impl FnMut(&Path) -> std::io::Result<DirectoryEntries>,
) -> SyncResult<DiskScan> {
    scan_with_path_mode(conn, read, CASE_INSENSITIVE_PATHS)
}

fn scan_with_path_mode(
    conn: &Connection,
    read: &mut impl FnMut(&Path) -> std::io::Result<DirectoryEntries>,
    case_insensitive: bool,
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
        match std::fs::canonicalize(stored) {
            Ok(canonical) if canonical.is_dir() => result.roots.push(DiskRoot {
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
    let resolver = KeyResolver::new(&roots, case_insensitive);
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
                    root,
                    &known_folders,
                    &known_files,
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
                let missing = no_symlinks(&root.canonical, &resolved).and_then(|safe| {
                    if safe {
                        backup::is_missing(&resolved)
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
            if !root.unchanged() {
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
            if !parent_exists(path) {
                result
                    .unreachable_roots
                    .push(path.parent().unwrap_or(path).to_string_lossy().into_owned());
            } else if backup::is_missing(path).unwrap_or(false) {
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
    }
    result.new_folders.sort_by(|a, b| a.path.cmp(&b.path));
    result.new_files.sort_by(|a, b| a.path.cmp(&b.path));
    result.missing_files.sort_by(|a, b| a.path.cmp(&b.path));
    result.missing_files.dedup_by(|a, b| a.id == b.id);
    if result.unreachable_roots.is_empty() {
        classify_by_content(conn, &mut result, case_insensitive, &hash_for_match)?;
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
fn hash_for_match(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > crate::safe_file::MAX_MODEL_FILE_BYTES {
        return None;
    }
    files::compute_content_hash(path).ok()
}

fn extension_key(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_lowercase())
}

/// Splits the new files by content: a clear rename partner among the missing rows, or a
/// copy of a row that is still there. Only new files are hashed (never the whole
/// catalog), so a scan of a large, unchanged catalog costs nothing extra.
fn classify_by_content(
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
    for (index, e) in scan.new_files.iter().enumerate() {
        let disk_path = scan
            .roots
            .iter()
            .find(|r| r.stored == Path::new(&e.root))
            .and_then(|r| r.resolve(Path::new(&e.path)));
        if let Some(content_hash) = disk_path.and_then(|p| hash(&p)) {
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

fn parent_exists(path: &Path) -> bool {
    path.parent()
        .is_some_and(|p| std::fs::metadata(p).is_ok_and(|m| m.is_dir()))
}

fn suppress_case_only_new_files(scan: &mut DiskScan, case_insensitive: bool) {
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

fn checked_path(scan: &DiskScan, e: &DiskEntry, kind: &str) -> SyncResult<PathBuf> {
    let changed = || DiskSyncError::path("diskSyncPathChanged", Path::new(&e.path));
    if kind == "missing" && e.root.is_empty() {
        let path = Path::new(&e.path);
        return if parent_exists(path) && backup::is_missing(path).unwrap_or(false) {
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
    if !root.unchanged() {
        return Err(DiskSyncError::path("diskSyncRootUnreachable", &root.stored));
    }
    let path = root.resolve(Path::new(&e.path)).ok_or_else(changed)?;
    if !no_symlinks(&root.canonical, &path).map_err(|_| changed())? {
        return Err(changed());
    }
    let valid = match kind {
        "missing" => backup::is_missing(&path).unwrap_or(false),
        "folder" => std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()),
        _ => std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()),
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
/// target in between.
fn verify_rename(
    conn: &Connection,
    scan: &DiskScan,
    rename: &DiskRename,
    expanded_sensitive: &[PathBuf],
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
    match (stored, hash_for_match(&target)) {
        (Some(stored), Some(current)) if stored == current => {}
        _ => return Err("Inhalt der Datei hat sich geändert".into()),
    }
    let parent = Path::new(&rename.new_path).parent().unwrap_or(&target);
    archives::check_managed_location(parent, expanded_sensitive).map_err(|e| e.message)
}

/// Points an existing catalog row at its new location. Everything attached to the row
/// (tags, favourite, notes, print log, collections) hangs on `id` and stays.
fn apply_rename(
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
        let Some(target) = emptied_folder_target(tx, scan, &folders, folder, &by_id, expanded_sensitive)
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

/// The new location of `folder` if the renames describe a pure folder rename or move.
fn emptied_folder_target(
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
    let usable = target != old
        && !target.starts_with(old)
        && !folders.iter().any(|f| Path::new(&f.path) == target)
        && scan
            .roots
            .iter()
            .any(|r| r.resolve(&target).is_some() && r.stored != target && r.canonical != target)
        && folders::reject_catalog_root_relocation(tx, old).is_ok()
        && archives::check_managed_location(&target, expanded_sensitive).is_ok();
    usable.then_some(target)
}

fn apply(conn: &mut Connection, selection: DiskSelection) -> SyncResult<DiskApplied> {
    apply_with_reader(conn, selection, |path| {
        let mut file = files::read_import_model_file(path, None)?;
        file.content_hash = Some(files::compute_content_hash(path)?);
        Ok(file)
    })
}
fn apply_with_reader(
    conn: &mut Connection,
    selection: DiskSelection,
    read: impl FnMut(&Path) -> CmdResult<db::models::NewFile>,
) -> SyncResult<DiskApplied> {
    apply_with_path_mode(conn, selection, read, CASE_INSENSITIVE_PATHS)
}

fn apply_with_path_mode(
    conn: &mut Connection,
    selection: DiskSelection,
    mut read: impl FnMut(&Path) -> CmdResult<db::models::NewFile>,
    case_insensitive: bool,
) -> SyncResult<DiskApplied> {
    // Re-scan under the exclusive catalog gate; the webview is not authoritative.
    let current = scan_with_path_mode(conn, &mut read_directory, case_insensitive)?;
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
        match verify_rename(conn, &current, r, &expanded_sensitive) {
            Ok(()) => renames.push(r),
            Err(reason) => result.failed_paths.push(DiskFailedPath {
                path: r.new_path.clone(),
                reason,
            }),
        }
    }
    for e in importable.iter().copied().filter(|e| chosen.contains(&e.path)) {
        let path = checked_path(&current, e, "file")?;
        let mut file = match read(&path) {
            Ok(file) => file,
            Err(error) => {
                result.failed_paths.push(DiskFailedPath {
                    path: e.path.clone(),
                    reason: error.message.chars().take(160).collect(),
                });
                continue;
            }
        };
        file.path = e.path.clone();
        prepared.push((e, file));
    }
    let validate = || -> SyncResult<()> {
        for root in &current.roots {
            if !root.unchanged() {
                return Err(DiskSyncError::path("diskSyncRootUnreachable", &root.stored));
            }
        }
        for e in current
            .new_folders
            .iter()
            .filter(|e| chosen.contains(&e.path))
        {
            checked_path(&current, e, "folder")?;
        }
        for e in importable.iter().copied().filter(|e| chosen.contains(&e.path)) {
            checked_path(&current, e, "file")?;
        }
        for r in &renames {
            checked_path(&current, &r.new_entry(), "file")?;
            checked_path(&current, &r.old_entry(), "missing")?;
        }
        if selection.remove_missing {
            for e in current
                .missing_files
                .iter()
                .filter(|e| e.id.as_ref().is_some_and(|id| missing.contains(id)))
            {
                checked_path(&current, e, "missing")?;
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
    // Before the path set is read: a renamed folder rewrites the paths below it.
    move_emptied_folders(&tx, &current, &renames, &expanded_sensitive);
    let resolver = KeyResolver::new(&current.roots, case_insensitive);
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
            checked_path(&current, e, "missing")?;
            let id =
                e.id.as_ref()
                    .unwrap()
                    .parse()
                    .map_err(|_| "diskSyncStale".to_string())?;
            trash::delete_missing_file_with_conn(&tx, id)?;
            result.removed_count += 1;
        }
    }
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

fn connection(state: &AppState) -> CmdResult<Connection> {
    let conn =
        Connection::open_with_flags(&state.db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    Ok(conn)
}
#[tauri::command]
pub async fn scan_catalog_disk(app: tauri::AppHandle) -> SyncResult<DiskScan> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _guard = state.import_jobs.gate.exclusive()?;
        scan(&connection(&state)?)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn apply_catalog_disk_sync(
    app: tauri::AppHandle,
    selection: DiskSelection,
) -> SyncResult<DiskApplied> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _guard = state.import_jobs.gate.exclusive()?;
        let mut selection = selection;
        selection.sensitive_dirs = state.sensitive_dirs.clone();
        apply(&mut connection(&state)?, selection)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestDir(PathBuf);
    impl TestDir {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    struct Fixture {
        dir: TestDir,
        conn: Connection,
        root: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = TestDir(
                std::fs::canonicalize(std::env::temp_dir())
                    .unwrap()
                    .join(format!(
                        "disk-sync-{}-{}",
                        std::process::id(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_nanos()
                    )),
            );
            std::fs::create_dir(&dir.0).unwrap();
            let root = dir.path().join("models");
            std::fs::create_dir(&root).unwrap();
            let conn = db::connect_in_memory().unwrap();
            db::ensure_folder_path(&conn, &root, &root).unwrap();
            Self { dir, conn, root }
        }
        fn select(&self, remove_missing: bool) -> DiskSelection {
            let preview = scan(&self.conn).unwrap();
            let renames = preview
                .renamed_files
                .iter()
                .map(|r| RenameChoice {
                    id: r.id.clone(),
                    new_path: r.new_path.clone(),
                })
                .collect();
            DiskSelection {
                renames,
                new_paths: preview
                    .new_folders
                    .into_iter()
                    .chain(preview.new_files)
                    // Copies of present models are opt-in in the dialog; these helpers
                    // model a user who accepts everything.
                    .chain(preview.duplicate_files.into_iter().map(|d| d.entry))
                    .map(|e| e.path)
                    .collect(),
                missing_ids: preview
                    .missing_files
                    .into_iter()
                    .filter_map(|e| e.id)
                    .collect(),
                remove_missing,
                ..Default::default()
            }
        }
    }
    #[test]
    fn path_keys_fold_only_when_requested() {
        let lower = Path::new("Spielzeug/Grün.3mf");
        let upper = Path::new("SPIELZEUG/GRÜN.3MF");
        assert_eq!(path_key(lower, true), path_key(upper, true));
        assert_ne!(path_key(lower, false), path_key(upper, false));
        assert_eq!(path_key(lower, false), lower);
        assert_eq!(
            CASE_INSENSITIVE_PATHS,
            cfg!(any(windows, target_os = "macos"))
        );
    }

    #[test]
    fn renamed_folder_is_reused_for_new_files_without_rewriting_paths() {
        let mut f = Fixture::new();
        let stored = f.root.join("Haushalt");
        let actual = f.root.join("HAUSHALT");
        std::fs::create_dir(&actual).unwrap();
        let folder_id = db::ensure_folder_path(&f.conn, &f.root, &stored).unwrap();
        let old_file = stored.join("Kreisel.obj");
        db::test_insert_minimal_file(&f.conn, old_file.to_str().unwrap(), Some(folder_id)).unwrap();
        std::fs::write(actual.join("KREISEL.OBJ"), "mesh").unwrap();
        let new_file = actual.join("new.obj");
        std::fs::write(&new_file, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let preview = scan_with_path_mode(&f.conn, &mut read_directory, true).unwrap();
        assert!(preview.new_folders.is_empty());
        assert!(preview.missing_files.is_empty());
        assert_eq!(preview.new_files.len(), 1);
        assert_eq!(preview.new_files[0].path, new_file.to_string_lossy());
        let selection = DiskSelection {
            new_paths: vec![new_file.to_string_lossy().into_owned()],
            missing_ids: vec![],
            remove_missing: false,
            ..Default::default()
        };
        let applied = apply_with_path_mode(
            &mut f.conn,
            selection,
            |path| files::read_import_model_file(path, None),
            true,
        )
        .unwrap();
        assert_eq!(applied.file_ids.len(), 1);
        assert!(applied.folder_ids.is_empty());
        let files = db::list_files(&f.conn).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files
            .iter()
            .any(|file| file.path == old_file.to_string_lossy()));
        assert_eq!(
            files
                .iter()
                .find(|file| file.path == new_file.to_string_lossy())
                .unwrap()
                .folder_id,
            Some(folder_id)
        );
        assert!(db::list_folders(&f.conn)
            .unwrap()
            .iter()
            .any(|folder| folder.path == stored.to_string_lossy()));
    }

    #[test]
    fn apply_rechecks_case_folded_duplicates_after_parsing() {
        let mut f = Fixture::new();
        let database = f.dir.path().join("catalog.db");
        f.conn = db::connect(&database).unwrap();
        db::ensure_folder_path(&f.conn, &f.root, &f.root).unwrap();
        let path = f.root.join("KREISEL.OBJ");
        let stored = f.root.join("Kreisel.obj");
        std::fs::write(&path, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let other = Connection::open(&database).unwrap();
        let selection = f.select(false);
        let applied = apply_with_path_mode(
            &mut f.conn,
            selection,
            |path| {
                db::test_insert_minimal_file(&other, stored.to_str().unwrap(), None).unwrap();
                files::read_import_model_file(path, None)
            },
            true,
        )
        .unwrap();
        assert!(applied.file_ids.is_empty());
        let files = db::list_files(&f.conn).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, stored.to_string_lossy());
    }

    #[test]
    fn scan_case_only_rename_is_neither_new_nor_missing() {
        let f = Fixture::new();
        let folder = f.root.join("Spielzeug");
        std::fs::create_dir(&folder).unwrap();
        db::ensure_folder_path(&f.conn, &f.root, &folder).unwrap();
        let stored = folder.join("Kreisel.3mf");
        std::fs::write(&stored, "mesh").unwrap();
        db::test_insert_minimal_file(&f.conn, stored.to_str().unwrap(), None).unwrap();
        // Simulate read_dir returning a new spelling while metadata still finds the old one.
        let preview = scan_with_path_mode(
            &f.conn,
            &mut |dir| {
                let mut entries = read_directory(dir)?;
                for (path, kind) in &mut entries {
                    if kind.is_file() {
                        *path = dir.join("KREISEL.3MF");
                    }
                }
                Ok(entries)
            },
            true,
        )
        .unwrap();
        assert!(preview.new_files.is_empty());
        assert!(preview.new_folders.is_empty());
        assert!(preview.missing_files.is_empty());
    }

    #[test]
    fn scan_is_read_only_and_apply_preserves_missing_until_selected() {
        let mut f = Fixture::new();
        let missing = f.root.join("missing.stl");
        let id = db::test_insert_minimal_file(&f.conn, missing.to_str().unwrap(), None).unwrap();
        let nested = f.root.join("nested");
        std::fs::create_dir(&nested).unwrap();
        let model = nested.join("new.obj");
        std::fs::write(&model, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        for name in [".hidden.obj", "~$temp.obj", "skip.part"] {
            std::fs::write(f.root.join(name), "ignored").unwrap();
        }
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.new_files.len(), 1);
        assert_eq!(preview.new_folders.len(), 1);
        assert_eq!(preview.missing_files.len(), 1);
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 1);
        assert_eq!(db::list_folders(&f.conn).unwrap().len(), 1);
        let selection = f.select(false);
        let result = apply(&mut f.conn, selection).unwrap();
        assert_eq!(result.file_ids.len(), 1);
        assert_eq!(result.folder_ids.len(), 1);
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 2);
        let imported = db::get_file(&f.conn, result.file_ids[0].parse().unwrap())
            .unwrap()
            .unwrap();
        assert!(imported.content_hash.is_some());
        assert!(imported.folder_id.is_some());
        assert!(scan(&f.conn).unwrap().new_files.is_empty());
        let selection = f.select(true);
        assert_eq!(apply(&mut f.conn, selection).unwrap().removed_count, 1);
        assert!(db::list_trash(&f.conn)
            .unwrap()
            .iter()
            .any(|file| file.id == id));
        assert!(model.exists());
    }
    #[test]
    fn missing_only_apply_requires_selection_and_moves_entries_to_trash() {
        let mut f = Fixture::new();
        let missing = f.root.join("missing.stl");
        // A non-empty root is needed, otherwise it counts as an unmounted volume.
        std::fs::write(f.root.join("notes.txt"), "present").unwrap();
        let id = db::test_insert_minimal_file(&f.conn, missing.to_str().unwrap(), None).unwrap();
        let preview = scan(&f.conn).unwrap();
        assert!(preview.new_files.is_empty());
        assert!(preview.new_folders.is_empty());
        assert_eq!(preview.missing_files.len(), 1);

        let selection = f.select(false);
        assert_eq!(apply(&mut f.conn, selection).unwrap().removed_count, 0);
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 1);
        assert!(db::list_trash(&f.conn).unwrap().is_empty());

        let selection = f.select(true);
        assert!(selection.new_paths.is_empty());
        let result = apply_with_reader(&mut f.conn, selection, |_| {
            panic!("Removing missing entries must not attempt to import a file")
        })
        .unwrap();
        assert_eq!(result.removed_count, 1);
        assert!(result.file_ids.is_empty());
        assert!(result.folder_ids.is_empty());
        assert!(db::list_files(&f.conn).unwrap().is_empty());
        let trash = db::list_trash(&f.conn).unwrap();
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].id, id);
        assert_eq!(trash[0].path, missing.to_string_lossy());
        assert!(!missing.exists());
    }

    #[test]
    fn unavailable_root_suppresses_all_changes_and_apply() {
        let mut f = Fixture::new();
        db::test_insert_minimal_file(&f.conn, f.root.join("missing.stl").to_str().unwrap(), None)
            .unwrap();
        let selection = f.select(true);
        std::fs::remove_dir(&f.root).unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.unreachable_roots.len(), 1);
        assert!(preview.missing_files.is_empty());
        assert!(apply(&mut f.conn, selection).is_err());
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 1);
    }
    #[test]
    fn empty_root_with_catalogued_files_is_unreachable_and_apply_changes_nothing() {
        let mut f = Fixture::new();
        for name in ["a.stl", "b.stl"] {
            db::test_insert_minimal_file(&f.conn, f.root.join(name).to_str().unwrap(), None)
                .unwrap();
        }
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.unreachable_roots, vec![f.root.to_string_lossy()]);
        assert!(preview.missing_files.is_empty());
        let selection = DiskSelection {
            new_paths: vec![],
            missing_ids: db::list_files(&f.conn)
                .unwrap()
                .iter()
                .map(|file| file.id.to_string())
                .collect(),
            remove_missing: true,
            ..Default::default()
        };
        assert!(apply(&mut f.conn, selection).is_err());
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 2);
        assert!(db::list_trash(&f.conn).unwrap().is_empty());
    }

    #[test]
    fn root_with_only_some_files_missing_still_offers_them() {
        let f = Fixture::new();
        std::fs::write(f.root.join("here.stl"), "mesh").unwrap();
        db::test_insert_minimal_file(&f.conn, f.root.join("here.stl").to_str().unwrap(), None)
            .unwrap();
        db::test_insert_minimal_file(&f.conn, f.root.join("gone.stl").to_str().unwrap(), None)
            .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert!(preview.unreachable_roots.is_empty());
        assert_eq!(preview.missing_files.len(), 1);
        assert_eq!(
            preview.missing_files[0].path,
            f.root.join("gone.stl").to_string_lossy()
        );
    }

    #[test]
    fn empty_root_without_catalogued_files_is_reachable() {
        let f = Fixture::new();
        let preview = scan(&f.conn).unwrap();
        assert!(preview.unreachable_roots.is_empty());
        assert!(preview.missing_files.is_empty());
    }

    #[test]
    fn loose_file_with_existing_parent_is_offered_as_missing() {
        let f = Fixture::new();
        let outside = f.dir.path().join("loose");
        std::fs::create_dir(&outside).unwrap();
        db::test_insert_minimal_file(&f.conn, outside.join("gone.stl").to_str().unwrap(), None)
            .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert!(preview.unreachable_roots.is_empty());
        assert_eq!(preview.missing_files.len(), 1);
    }

    #[test]
    fn loose_file_with_absent_parent_is_unreachable_not_missing() {
        let mut f = Fixture::new();
        let drive = f.dir.path().join("stick");
        db::test_insert_minimal_file(&f.conn, drive.join("gone.stl").to_str().unwrap(), None)
            .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.unreachable_roots, vec![drive.to_string_lossy()]);
        assert!(preview.missing_files.is_empty());
        let selection = DiskSelection {
            new_paths: vec![],
            missing_ids: vec![],
            remove_missing: true,
            ..Default::default()
        };
        assert!(apply(&mut f.conn, selection).is_err());
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 1);
    }

    #[test]
    fn stale_missing_selection_and_outside_paths_are_rejected() {
        let mut f = Fixture::new();
        let missing = f.root.join("missing.stl");
        std::fs::write(f.root.join("notes.txt"), "present").unwrap();
        db::test_insert_minimal_file(&f.conn, missing.to_str().unwrap(), None).unwrap();
        let selection = f.select(true);
        std::fs::write(&missing, "returned").unwrap();
        assert!(apply(&mut f.conn, selection).is_err());
        let selection = DiskSelection {
            new_paths: vec![f.dir.path().join("outside.obj").to_string_lossy().into()],
            missing_ids: vec![],
            remove_missing: false,
            ..Default::default()
        };
        assert!(apply(&mut f.conn, selection).is_err());
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 1);
    }
    #[test]
    fn invalid_import_is_reported_without_creating_empty_folder_rows() {
        let mut f = Fixture::new();
        std::fs::create_dir(f.root.join("new")).unwrap();
        std::fs::write(f.root.join("new/broken.stl"), "broken").unwrap();
        let selection = f.select(false);
        let result = apply(&mut f.conn, selection).unwrap();
        assert_eq!(result.failed_paths.len(), 1);
        assert!(result.file_ids.is_empty());
        assert!(result.folder_ids.is_empty());
        assert_eq!(result.removed_count, 0);
        assert_eq!(db::list_folders(&f.conn).unwrap().len(), 1);
        assert!(db::list_files(&f.conn).unwrap().is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_and_overlapping_roots_are_not_followed_or_duplicated() {
        let f = Fixture::new();
        let nested = f.root.join("nested");
        std::fs::create_dir(&nested).unwrap();
        db::ensure_folder_path(&f.conn, &nested, &nested).unwrap();
        std::fs::write(nested.join("new.obj"), "mesh").unwrap();
        std::os::unix::fs::symlink(&nested, f.root.join("link")).unwrap();
        std::os::unix::fs::symlink(f.dir.path().join("absent"), f.root.join("broken.obj")).unwrap();
        db::test_insert_minimal_file(&f.conn, f.root.join("broken.obj").to_str().unwrap(), None)
            .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.new_files.len(), 1);
        assert!(preview.missing_files.is_empty());
        assert!(preview.new_folders.is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn non_not_found_io_error_is_not_missing() {
        let f = Fixture::new();
        let file = f.root.join("not_a_directory");
        std::fs::write(&file, "x").unwrap();
        assert!(backup::is_missing(&file.join("child.stl")).is_err());
        db::test_insert_minimal_file(&f.conn, file.join("child.stl").to_str().unwrap(), None)
            .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert!(preview.missing_files.is_empty());
        assert!(preview.unreachable_roots.is_empty());
    }
    #[test]
    fn scan_ignores_paths_reserved_by_trash_and_empty_or_unsupported_folders() {
        let mut f = Fixture::new();
        let path = f.root.join("trashed.obj");
        std::fs::write(&path, "mesh").unwrap();
        let id = db::test_insert_minimal_file(&f.conn, path.to_str().unwrap(), None).unwrap();
        trash::delete_missing_file_with_conn(&f.conn, id).unwrap();
        std::fs::create_dir(f.root.join("empty")).unwrap();
        std::fs::create_dir_all(f.root.join("docs/images")).unwrap();
        std::fs::write(f.root.join("docs/readme.txt"), "docs").unwrap();
        assert!(scan(&f.conn).unwrap().new_files.is_empty());
        assert!(scan(&f.conn).unwrap().new_folders.is_empty());
        let selection = f.select(false);
        let result = apply(&mut f.conn, selection).unwrap();
        assert!(result.folder_ids.is_empty());
        assert!(result.file_ids.is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn root_with_symlinked_ancestor_works_without_rewriting_catalog_paths() {
        let mut f = Fixture::new();
        let actual = f.dir.path().join("actual");
        std::fs::create_dir_all(actual.join("nested/known")).unwrap();
        let alias = f.dir.path().join("alias");
        std::os::unix::fs::symlink(&actual, &alias).unwrap();
        let root = alias.join("nested");
        let folder_id = db::ensure_folder_path(&f.conn, &root, &root.join("known")).unwrap();
        let known = root.join("known/old.obj");
        std::fs::write(&known, "mesh").unwrap();
        let known_id =
            db::test_insert_minimal_file(&f.conn, known.to_str().unwrap(), Some(folder_id))
                .unwrap();
        // A canonical file path in an older catalog must also be recognized.
        let canonical = actual.join("nested/canonical.obj");
        std::fs::write(&canonical, "mesh").unwrap();
        db::test_insert_minimal_file(&f.conn, canonical.to_str().unwrap(), None).unwrap();
        let gone = root.join("gone.obj");
        db::test_insert_minimal_file(&f.conn, gone.to_str().unwrap(), None).unwrap();
        std::fs::create_dir_all(root.join("new/deep")).unwrap();
        std::fs::write(
            root.join("new/deep/new.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(&f.root, root.join("link")).unwrap();
        std::fs::write(f.root.join("outside.obj"), "mesh").unwrap();
        let preview = scan(&f.conn).unwrap();
        assert!(preview.unreachable_roots.is_empty());
        assert_eq!(preview.new_files.len(), 2);
        assert_eq!(preview.new_folders.len(), 2);
        assert_eq!(preview.missing_files[0].path, gone.to_string_lossy());
        assert!(
            !no_symlinks(&actual.join("nested"), &actual.join("nested/../escape.obj")).unwrap()
        );
        assert!(!no_symlinks(&actual.join("nested"), &f.root.join("outside.obj")).unwrap());
        let mut selection = f.select(false);
        selection
            .new_paths
            .retain(|p| p.starts_with(root.to_str().unwrap()));
        let result = apply(&mut f.conn, selection).unwrap();
        assert_eq!(result.file_ids.len(), 1);
        assert_eq!(result.folder_ids.len(), 2);
        assert_eq!(
            db::get_file(&f.conn, known_id).unwrap().unwrap().path,
            known.to_string_lossy()
        );
        assert_eq!(
            db::list_folders(&f.conn)
                .unwrap()
                .iter()
                .find(|folder| folder.id == folder_id)
                .unwrap()
                .path,
            root.join("known").to_string_lossy()
        );
        assert!(scan(&f.conn)
            .unwrap()
            .new_files
            .iter()
            .all(|e| !e.path.starts_with(root.to_str().unwrap())));
    }

    #[test]
    fn unreadable_files_do_not_prevent_valid_imports() {
        let mut f = Fixture::new();
        let broken = f.root.join("broken.3mf");
        std::fs::write(&broken, "incomplete archive").unwrap();
        let valid = f.root.join("valid.obj");
        std::fs::write(&valid, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let selection = f.select(false);
        let result = apply(&mut f.conn, selection).unwrap();
        assert_eq!(result.file_ids.len(), 1);
        assert_eq!(result.failed_paths.len(), 1);
        assert_eq!(result.failed_paths[0].path, broken.to_string_lossy());
        assert!(!result.failed_paths[0].reason.is_empty());
        assert_eq!(
            db::list_files(&f.conn).unwrap()[0].path,
            valid.to_string_lossy()
        );
        let serialized = serde_json::to_value(&result).unwrap();
        assert_eq!(
            serialized["failedPaths"][0]["path"],
            broken.to_string_lossy().as_ref()
        );
    }

    #[cfg(unix)]
    #[test]
    fn failed_read_does_not_bypass_path_validation() {
        let mut f = Fixture::new();
        let path = f.root.join("broken.3mf");
        std::fs::write(&path, "incomplete archive").unwrap();
        let selection = f.select(false);
        let result = apply_with_reader(&mut f.conn, selection, |path| {
            std::fs::remove_file(path).unwrap();
            std::os::unix::fs::symlink(f.dir.path().join("elsewhere.3mf"), path).unwrap();
            Err("Cannot read file".to_string().into())
        });
        assert_eq!(result.err().unwrap().code, "diskSyncPathChanged");
        assert!(db::list_files(&f.conn).unwrap().is_empty());
    }

    #[test]
    fn parsing_all_files_happens_before_any_sqlite_transaction() {
        let mut f = Fixture::new();
        let database = f.dir.path().join("catalog.db");
        f.conn = db::connect(&database).unwrap();
        db::ensure_folder_path(&f.conn, &f.root, &f.root).unwrap();
        std::fs::create_dir(f.root.join("new")).unwrap();
        for name in ["a.obj", "b.obj"] {
            std::fs::write(
                f.root.join("new").join(name),
                "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
            )
            .unwrap();
        }
        let other = Connection::open(&database).unwrap();
        other.busy_timeout(std::time::Duration::ZERO).unwrap();
        let selection = f.select(false);
        let mut reads = 0;
        let result = apply_with_reader(&mut f.conn, selection, |path| {
            // A second writer must succeed even while preparing the second file.
            other
                .execute_batch("BEGIN IMMEDIATE; UPDATE folders SET name = name; COMMIT;")
                .unwrap();
            assert_eq!(db::list_folders(&other).unwrap().len(), 1);
            assert!(db::list_files(&other).unwrap().is_empty());
            reads += 1;
            let mut file = files::read_import_model_file(path, None)?;
            file.content_hash = Some(files::compute_content_hash(path)?);
            Ok(file)
        })
        .unwrap();
        assert_eq!(reads, 2);
        assert_eq!(result.file_ids.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_swap_during_parsing_rolls_back_everything() {
        let mut f = Fixture::new();
        std::fs::create_dir(f.root.join("new")).unwrap();
        let path = f.root.join("new/model.obj");
        std::fs::write(&path, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let selection = f.select(false);
        let result = apply_with_reader(&mut f.conn, selection, |path| {
            let file = files::read_import_model_file(path, None)?;
            std::fs::remove_file(path).unwrap();
            std::os::unix::fs::symlink(f.dir.path().join("elsewhere.obj"), path).unwrap();
            Ok(file)
        });
        assert_eq!(result.err().unwrap().code, "diskSyncPathChanged");
        assert_eq!(db::list_folders(&f.conn).unwrap().len(), 1);
        assert!(db::list_files(&f.conn).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn root_retargeted_during_parsing_is_rejected() {
        let mut f = Fixture::new();
        let alias = f.dir.path().join("alias");
        std::os::unix::fs::symlink(&f.root, &alias).unwrap();
        f.conn
            .execute("UPDATE folders SET path = ?1", [alias.to_str().unwrap()])
            .unwrap();
        std::fs::write(
            f.root.join("model.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let elsewhere = f.dir.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let selection = f.select(false);
        let error = apply_with_reader(&mut f.conn, selection, |path| {
            let file = files::read_import_model_file(path, None)?;
            std::fs::remove_file(&alias).unwrap();
            std::os::unix::fs::symlink(&elsewhere, &alias).unwrap();
            Ok(file)
        })
        .err()
        .unwrap();
        assert_eq!(error.code, "diskSyncRootUnreachable");
        assert!(db::list_files(&f.conn).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn canonical_folder_rows_and_duplicate_root_aliases_are_reused() {
        let mut f = Fixture::new();
        let alias = f.dir.path().join("alias");
        std::os::unix::fs::symlink(&f.root, &alias).unwrap();
        db::ensure_folder_path(&f.conn, &alias, &alias).unwrap();
        std::fs::create_dir(f.root.join("known")).unwrap();
        let id = db::ensure_folder_path(&f.conn, &f.root, &f.root.join("known")).unwrap();
        let known = alias.join("old.obj");
        std::fs::write(&known, "mesh").unwrap();
        db::test_insert_minimal_file(&f.conn, known.to_str().unwrap(), None).unwrap();
        std::fs::write(
            f.root.join("known/new.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.new_files.len(), 1);
        assert!(preview.new_folders.is_empty());
        let selection = f.select(false);
        let result = apply(&mut f.conn, selection).unwrap();
        assert!(result.folder_ids.is_empty());
        let file = db::get_file(&f.conn, result.file_ids[0].parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(file.folder_id, Some(id));
        assert_eq!(db::list_folders(&f.conn).unwrap().len(), 3);
    }

    #[test]
    fn unassigned_files_outside_roots_are_checked_and_can_be_removed() {
        let mut f = Fixture::new();
        let gone = f.dir.path().join("gone.obj");
        let id = db::test_insert_minimal_file(&f.conn, gone.to_str().unwrap(), None).unwrap();
        let present = f.dir.path().join("present.obj");
        std::fs::write(&present, "mesh").unwrap();
        db::test_insert_minimal_file(&f.conn, present.to_str().unwrap(), None).unwrap();
        let trashed = db::test_insert_minimal_file(
            &f.conn,
            f.dir.path().join("trashed.obj").to_str().unwrap(),
            None,
        )
        .unwrap();
        trash::delete_missing_file_with_conn(&f.conn, trashed).unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.missing_files.len(), 1);
        assert_eq!(preview.missing_files[0].id, Some(id.to_string()));
        assert!(preview.missing_files[0].root.is_empty());
        let selection = f.select(false);
        assert_eq!(apply(&mut f.conn, selection).unwrap().removed_count, 0);
        let selection = f.select(true);
        assert_eq!(apply(&mut f.conn, selection).unwrap().removed_count, 1);
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 1);
    }

    #[test]
    fn sync_imports_distinct_paths_with_identical_content_hashes() {
        let mut f = Fixture::new();
        let original = f.root.join("original.obj");
        std::fs::write(&original, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let selection = f.select(false);
        let first = apply(&mut f.conn, selection).unwrap();
        std::fs::copy(&original, f.root.join("copy.obj")).unwrap();
        let selection = f.select(false);
        let second = apply(&mut f.conn, selection).unwrap();
        assert_eq!(second.file_ids.len(), 1);
        let first = db::get_file(&f.conn, first.file_ids[0].parse().unwrap())
            .unwrap()
            .unwrap();
        let second = db::get_file(&f.conn, second.file_ids[0].parse().unwrap())
            .unwrap()
            .unwrap();
        assert!(first.content_hash.is_some());
        assert_eq!(first.content_hash, second.content_hash);
        assert_ne!(first.path, second.path);
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 2);
    }

    #[test]
    fn child_read_errors_preserve_siblings_and_protect_missing_files() {
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
        ] {
            let f = Fixture::new();
            let child = f.root.join("child");
            std::fs::create_dir(&child).unwrap();
            std::fs::write(f.root.join("new.obj"), "mesh").unwrap();
            db::test_insert_minimal_file(&f.conn, child.join("gone.obj").to_str().unwrap(), None)
                .unwrap();
            db::test_insert_minimal_file(&f.conn, f.root.join("gone.obj").to_str().unwrap(), None)
                .unwrap();
            let preview = scan_with_reader(&f.conn, &mut |dir| {
                if dir == child {
                    Err(std::io::Error::from(kind))
                } else {
                    read_directory(dir)
                }
            })
            .unwrap();
            assert!(preview.unreachable_roots.is_empty());
            assert_eq!(preview.new_files.len(), 1);
            if kind == std::io::ErrorKind::NotFound {
                assert!(preview.skipped_paths.is_empty());
                assert_eq!(preview.missing_files.len(), 2);
            } else {
                assert_eq!(preview.skipped_paths, vec![child.to_string_lossy()]);
                assert_eq!(preview.missing_files.len(), 1);
                assert_eq!(
                    preview.missing_files[0].path,
                    f.root.join("gone.obj").to_string_lossy()
                );
            }
        }
    }

    #[test]
    fn root_read_errors_still_suppress_all_changes() {
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
        ] {
            let f = Fixture::new();
            db::test_insert_minimal_file(&f.conn, f.root.join("gone.obj").to_str().unwrap(), None)
                .unwrap();
            let preview =
                scan_with_reader(&f.conn, &mut |_| Err(std::io::Error::from(kind))).unwrap();
            assert_eq!(preview.unreachable_roots, vec![f.root.to_string_lossy()]);
            assert!(preview.skipped_paths.is_empty());
            assert!(preview.missing_files.is_empty());
            assert!(preview.new_files.is_empty());
        }
    }

    #[test]
    fn case_only_filter_preserves_missing_and_unrelated_new_files() {
        let root = Path::new("models");
        let mut preview = DiskScan {
            new_files: vec![
                entry(root, Path::new("models/Part/A.obj"), None),
                entry(root, Path::new("models/other/b.obj"), None),
            ],
            new_folders: vec![
                entry(root, Path::new("models/Part"), None),
                entry(root, Path::new("models/other"), None),
            ],
            missing_files: vec![entry(
                root,
                Path::new("models/part/a.OBJ"),
                Some("1".into()),
            )],
            ..DiskScan::default()
        };
        suppress_case_only_new_files(&mut preview, true);
        assert_eq!(preview.new_files.len(), 1);
        assert_eq!(preview.new_files[0].path, "models/other/b.obj");
        assert_eq!(preview.new_folders.len(), 1);
        assert_eq!(preview.new_folders[0].path, "models/other");
        assert_eq!(preview.missing_files.len(), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_keeps_case_distinct_paths() {
        let f = Fixture::new();
        std::fs::write(f.root.join("A.obj"), "mesh").unwrap();
        db::test_insert_minimal_file(&f.conn, f.root.join("a.obj").to_str().unwrap(), None)
            .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.new_files.len(), 1);
        assert_eq!(preview.missing_files.len(), 1);
    }

    #[test]
    fn error_contract_keeps_codes_and_paths_separate() {
        let error = DiskSyncError::path("diskSyncPathChanged", Path::new("/models/a.obj"));
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            serde_json::json!({"code": "diskSyncPathChanged", "paths": ["/models/a.obj"]})
        );
    }
}

#[cfg(test)]
mod roadmap_1g_tests;

#[cfg(test)]
mod rename_tests;
