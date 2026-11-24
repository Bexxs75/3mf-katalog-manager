//! Catalog health check: models whose file is gone and groups of identical files.
//! The desktop app and the web server share this logic; only the way "is the file there"
//! is answered differs per platform (`MissingProbe`).
use crate::db::{self, models::FileRecord};
use rusqlite::Connection;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// `Ok(true)` only for a certain "not there". Any `Err` means unclear and is never treated
/// as missing: a permission problem or an unmounted network drive must not mark files for
/// deletion.
pub trait MissingProbe {
    fn is_missing(&self, path: &Path) -> std::io::Result<bool>;
}

/// Desktop: asks the file system directly; only `NotFound` counts as missing.
pub struct StdMissingProbe;
impl MissingProbe for StdMissingProbe {
    fn is_missing(&self, path: &Path) -> std::io::Result<bool> {
        match std::fs::metadata(path) {
            Ok(_) => Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(e) => Err(e),
        }
    }
}

/// Web: only looks below the configured roots and never follows a link. A path outside
/// every root, behind a link or with any other I/O error is unclear.
///
/// On Unix the path is resolved from a freshly opened root handle, one component at a time
/// (`openat` with `O_NOFOLLOW` relative to the previous handle), so a component swapped for a
/// link between two steps is refused instead of followed. The root is opened by its canonical
/// path without following any link and must still be the directory seen in `new`. Elsewhere
/// every prefix is checked with `symlink_metadata` (no anchoring there).
///
/// "Missing" needs every component before the first absent one to be a real directory; a link,
/// a non-directory or any other error on the way is unclear.
pub struct RootsMissingProbe {
    roots: Vec<ProbeRoot>,
}
struct ProbeRoot {
    stored: PathBuf,
    canonical: Option<PathBuf>,
    /// (dev, ino) of the root directory when the probe was built.
    #[cfg(unix)]
    identity: Option<(u64, u64)>,
}
impl RootsMissingProbe {
    pub fn new(roots: &[PathBuf]) -> Self {
        let roots = roots
            .iter()
            .map(|stored| {
                let canonical = std::fs::canonicalize(stored).ok().filter(|c| c.is_dir());
                #[cfg(unix)]
                let identity = canonical.as_deref().and_then(|c| {
                    use std::os::unix::fs::MetadataExt;
                    let meta = crate::upload_fs::Directory::open(c).ok()?.metadata().ok()?;
                    Some((meta.dev(), meta.ino()))
                });
                ProbeRoot {
                    stored: stored.clone(),
                    canonical,
                    #[cfg(unix)]
                    identity,
                }
            })
            .collect();
        Self { roots }
    }

    /// `step(i)` runs right before component `i` is looked at (tests swap entries there).
    fn probe(&self, path: &Path, step: &mut dyn FnMut(usize)) -> std::io::Result<bool> {
        let unclear = |what: &str| std::io::Error::other(what.to_string());
        let (root, relative) = self
            .roots
            .iter()
            .find_map(|root| {
                let rel = path.strip_prefix(&root.stored).ok().or_else(|| path.strip_prefix(root.canonical.as_deref()?).ok())?;
                Some((root, rel))
            })
            .ok_or_else(|| unclear("outside the roots"))?;
        let mut parts: Vec<&std::ffi::OsStr> = Vec::new();
        for component in relative.components() {
            match component {
                std::path::Component::Normal(name) => parts.push(name),
                _ => return Err(unclear("not a plain relative path")),
            }
        }
        if parts.is_empty() {
            return Err(unclear("is a root"));
        }
        let canonical = root.canonical.as_deref().ok_or_else(|| unclear("root not usable"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let mut current = crate::upload_fs::Directory::open(canonical)?;
            let meta = current.metadata()?;
            if Some((meta.dev(), meta.ino())) != root.identity {
                return Err(unclear("root replaced"));
            }
            let last = parts.len() - 1;
            for (index, part) in parts.iter().enumerate() {
                step(index);
                let name = part.to_str().ok_or_else(|| unclear("name is not text"))?;
                if index < last {
                    match current.open_dir(name) {
                        Ok(next) => current = next,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
                        Err(e) => return Err(e),
                    }
                } else {
                    return match current.read(name) {
                        Ok(_) => Ok(false),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
                        // Present, but not a regular file (folder, device): not missing.
                        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => Ok(false),
                        Err(e) => Err(e),
                    };
                }
            }
            Err(unclear("empty path"))
        }
        #[cfg(not(unix))]
        {
            let is_real_dir = |p: &Path| std::fs::symlink_metadata(p).map(|m| m.is_dir() && !m.file_type().is_symlink());
            if !is_real_dir(canonical)? {
                return Err(unclear("root replaced"));
            }
            let mut current = canonical.to_path_buf();
            let last = parts.len() - 1;
            for (index, part) in parts.iter().enumerate() {
                step(index);
                current.push(part);
                match std::fs::symlink_metadata(&current) {
                    Ok(meta) if index < last => {
                        if !(meta.is_dir() && !meta.file_type().is_symlink()) {
                            return Err(unclear("link or non-directory on the way"));
                        }
                    }
                    Ok(meta) => return if meta.file_type().is_symlink() { Err(unclear("link")) } else { Ok(false) },
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
                    Err(e) => return Err(e),
                }
            }
            Err(unclear("empty path"))
        }
    }
}
impl MissingProbe for RootsMissingProbe {
    fn is_missing(&self, path: &Path) -> std::io::Result<bool> {
        self.probe(path, &mut |_| {})
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Issues {
    pub orphaned: Vec<i64>,
    pub duplicate_groups: Vec<Vec<i64>>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct IssueLimits {
    pub max_orphaned: usize,
    pub max_groups: usize,
    pub max_duration: Duration,
}
impl IssueLimits {
    /// No limit at all (desktop).
    pub const UNLIMITED: IssueLimits =
        IssueLimits { max_orphaned: usize::MAX, max_groups: usize::MAX, max_duration: Duration::MAX };
}

/// What the scan and the grouping need of a catalog row.
struct Row<'a> {
    id: i64,
    path: &'a str,
    hash: Option<&'a str>,
    imported_at: &'a str,
}
impl<'a> Row<'a> {
    fn of(file: &'a FileRecord) -> Self {
        Row { id: file.id, path: &file.path, hash: file.content_hash.as_deref(), imported_at: &file.imported_at }
    }
}

/// Groups by `content_hash`, skipping `orphaned_ids` (a missing file must never become a
/// group's "keep the oldest" anchor). Only groups with 2+ members, each sorted oldest-first,
/// groups sorted by their oldest member.
pub fn group_duplicates(files: Vec<FileRecord>, orphaned_ids: &HashSet<i64>) -> Vec<Vec<FileRecord>> {
    let rows: Vec<Row> = files.iter().map(Row::of).collect();
    let (groups, _) = group_rows(&rows, orphaned_ids, Instant::now(), Duration::MAX);
    let mut by_id: std::collections::HashMap<i64, FileRecord> = files.into_iter().map(|f| (f.id, f)).collect();
    groups.into_iter().map(|g| g.into_iter().filter_map(|id| by_id.remove(&id)).collect()).collect()
}

/// The second value is true when `max_duration` ran out while hashing; the groups then hold
/// only the rows seen until then (each member still a row the caller passed in).
fn group_rows(rows: &[Row], orphaned_ids: &HashSet<i64>, started: Instant, max_duration: Duration) -> (Vec<Vec<i64>>, bool) {
    let mut by_hash: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    let mut timed_out = false;
    for (index, row) in rows.iter().enumerate() {
        if index % 256 == 0 && started.elapsed() >= max_duration {
            timed_out = true;
            break;
        }
        if orphaned_ids.contains(&row.id) {
            continue;
        }
        if let Some(hash) = row.hash {
            by_hash.entry(hash).or_default().push(row);
        }
    }
    let mut groups: Vec<Vec<&Row>> = by_hash
        .into_values()
        .filter(|group| group.len() >= 2)
        .map(|mut group| {
            group.sort_by(|a, b| a.imported_at.cmp(b.imported_at));
            group
        })
        .collect();
    groups.sort_by(|a, b| a[0].imported_at.cmp(b[0].imported_at));
    (groups.into_iter().map(|g| g.into_iter().map(|r| r.id).collect()).collect(), timed_out)
}

fn scan_rows(rows: &[Row], probe: &dyn MissingProbe, limits: IssueLimits, started: Instant) -> Issues {
    let mut issues = Issues::default();
    let mut orphaned_ids: HashSet<i64> = HashSet::new();
    // Rows that were really looked at. A bounded scan that stops early must not form groups
    // from the rest (or from the row that hit the orphan limit): an unchecked missing model
    // could become the "keep" anchor of a group whose only living copy then gets deleted.
    let mut checked = rows.len();
    for (index, row) in rows.iter().enumerate() {
        if started.elapsed() >= limits.max_duration {
            issues.truncated = true;
            checked = index;
            break;
        }
        if probe.is_missing(Path::new(row.path)).unwrap_or(false) {
            if issues.orphaned.len() >= limits.max_orphaned {
                issues.truncated = true;
                checked = index;
                break;
            }
            orphaned_ids.insert(row.id);
            issues.orphaned.push(row.id);
        }
    }
    let (mut groups, timed_out) = group_rows(&rows[..checked], &orphaned_ids, started, limits.max_duration);
    if timed_out {
        issues.truncated = true;
    }
    if groups.len() > limits.max_groups {
        groups.truncate(limits.max_groups);
        issues.truncated = true;
    }
    issues.duplicate_groups = groups;
    issues
}

/// Same as `scan` for rows the caller already loaded (the desktop needs the full records
/// for its DTOs and must not read the catalog twice).
pub fn scan_records(files: &[FileRecord], probe: &dyn MissingProbe, limits: IssueLimits) -> Issues {
    let started = Instant::now();
    let rows: Vec<Row> = files.iter().map(Row::of).collect();
    scan_rows(&rows, probe, limits, started)
}

/// Web: the budget starts before the catalog is read, and only the few columns the scan needs
/// are loaded (no image blobs, no per-model child queries).
pub fn scan(conn: &Connection, probe: &dyn MissingProbe, limits: IssueLimits) -> Result<Issues, String> {
    let started = Instant::now();
    let loaded = db::list_issue_rows(conn).map_err(|e| e.to_string())?;
    let rows: Vec<Row> = loaded
        .iter()
        .map(|r| Row { id: r.id, path: &r.path, hash: r.content_hash.as_deref(), imported_at: &r.imported_at })
        .collect();
    Ok(scan_rows(&rows, probe, limits, started))
}

#[cfg(test)]
#[path = "catalog_issues_tests.rs"]
mod tests;
