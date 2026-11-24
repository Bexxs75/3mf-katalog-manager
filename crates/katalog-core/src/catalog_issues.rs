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
    /// Like `is_missing`, with the reason of an unclear answer and (web) the gone folder.
    fn judge(&self, path: &Path) -> Verdict {
        match self.is_missing(path) {
            Ok(true) => Verdict::Missing { folder: None },
            Ok(false) => Verdict::Present,
            Err(e) => Verdict::Unclear { reason: classify(&e), place: None },
        }
    }
}

/// Why a row could not be judged. Shown to the user, never acted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UnclearReason {
    /// The root (or a folder) is not reachable right now: not mounted, empty, replaced, outside.
    Unreachable,
    /// A link, or something that is not a folder, lies on the way.
    Link,
    /// Any other read error (permissions, I/O).
    Read,
}

/// Marker carried inside an `io::Error` so the probe's own decisions keep their reason.
#[derive(Debug)]
struct Why(UnclearReason);
impl std::fmt::Display for Why {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unclear: {:?}", self.0)
    }
}
impl std::error::Error for Why {}
fn why(reason: UnclearReason) -> std::io::Error {
    std::io::Error::other(Why(reason))
}
/// The reason behind a probe error: the probe's own marker, else by OS error.
pub fn classify(error: &std::io::Error) -> UnclearReason {
    if let Some(Why(reason)) = error.get_ref().and_then(|e| e.downcast_ref::<Why>()) {
        return *reason;
    }
    #[cfg(unix)]
    if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) {
        return UnclearReason::Link;
    }
    UnclearReason::Read
}

/// Where below which root (by index) a row lies; never an absolute path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub root: usize,
    pub relative: String,
}

/// What a probe says about one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Present,
    /// Certainly gone. `folder` is the topmost folder on the way that is itself gone (the
    /// whole chain before it consists of real directories), when the missing part is a folder.
    Missing { folder: Option<Place> },
    Unclear { reason: UnclearReason, place: Option<Place> },
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
    /// Whether the root holds no entry at all (checked once, on first need).
    empty: std::sync::OnceLock<bool>,
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
                    empty: std::sync::OnceLock::new(),
                }
            })
            .collect();
        Self { roots }
    }

    /// `step(i)` runs right before component `i` is looked at (tests swap entries there).
    fn probe(&self, path: &Path, step: &mut dyn FnMut(usize)) -> std::io::Result<bool> {
        self.probe_detail(path, step).map(|(missing, _)| missing)
    }

    /// The root a path lies below (by index) and its plain components.
    fn locate<'p>(&self, path: &'p Path) -> Option<(usize, &ProbeRoot, Vec<&'p std::ffi::OsStr>)> {
        let (index, root, rel) = self.roots.iter().enumerate().find_map(|(index, root)| {
            let rel = path.strip_prefix(&root.stored).ok().or_else(|| path.strip_prefix(root.canonical.as_deref()?).ok())?;
            Some((index, root, rel))
        })?;
        let mut parts = Vec::new();
        for component in rel.components() {
            match component {
                std::path::Component::Normal(name) => parts.push(name),
                _ => return None,
            }
        }
        Some((index, root, parts))
    }

    /// `Ok((true, absent))`: gone; `absent` is the index of the first absent component when it is
    /// a folder on the way (not the last component). Errors carry their reason (see `classify`).
    fn probe_detail(&self, path: &Path, step: &mut dyn FnMut(usize)) -> std::io::Result<(bool, Option<usize>)> {
        let (_, root, parts) = match self.locate(path) {
            Some(found) => found,
            None if self.roots.iter().any(|r| path.starts_with(&r.stored) || r.canonical.as_deref().is_some_and(|c| path.starts_with(c))) => {
                return Err(why(UnclearReason::Read));
            }
            None => return Err(why(UnclearReason::Unreachable)),
        };
        if parts.is_empty() {
            return Err(why(UnclearReason::Read));
        }
        let canonical = root.canonical.as_deref().ok_or_else(|| why(UnclearReason::Unreachable))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let mut current = crate::upload_fs::Directory::open(canonical).map_err(|_| why(UnclearReason::Unreachable))?;
            let meta = current.metadata().map_err(|_| why(UnclearReason::Unreachable))?;
            if Some((meta.dev(), meta.ino())) != root.identity {
                return Err(why(UnclearReason::Unreachable));
            }
            let last = parts.len() - 1;
            for (index, part) in parts.iter().enumerate() {
                step(index);
                let name = part.to_str().ok_or_else(|| why(UnclearReason::Read))?;
                if index < last {
                    match current.open_dir(name) {
                        Ok(next) => current = next,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((true, Some(index))),
                        Err(e) => return Err(e),
                    }
                } else {
                    return match current.read(name) {
                        Ok(_) => Ok((false, None)),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((true, None)),
                        // Present, but not a regular file (folder, device): not missing.
                        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => Ok((false, None)),
                        Err(e) => Err(e),
                    };
                }
            }
            Err(why(UnclearReason::Read))
        }
        #[cfg(not(unix))]
        {
            let is_real_dir = |p: &Path| std::fs::symlink_metadata(p).map(|m| m.is_dir() && !m.file_type().is_symlink());
            if !is_real_dir(canonical).map_err(|_| why(UnclearReason::Unreachable))? {
                return Err(why(UnclearReason::Unreachable));
            }
            let mut current = canonical.to_path_buf();
            let last = parts.len() - 1;
            for (index, part) in parts.iter().enumerate() {
                step(index);
                current.push(part);
                match std::fs::symlink_metadata(&current) {
                    Ok(meta) if index < last => {
                        if !(meta.is_dir() && !meta.file_type().is_symlink()) {
                            return Err(why(UnclearReason::Link));
                        }
                    }
                    Ok(meta) => return if meta.file_type().is_symlink() { Err(why(UnclearReason::Link)) } else { Ok((false, None)) },
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((true, (index < last).then_some(index))),
                    Err(e) => return Err(e),
                }
            }
            Err(why(UnclearReason::Read))
        }
    }

    /// True when the root has no entry at all although a row lies below it: most likely a
    /// volume that is not mounted. Nothing below such a root counts as gone.
    fn root_is_empty(root: &ProbeRoot) -> bool {
        *root.empty.get_or_init(|| match root.canonical.as_deref().map(std::fs::read_dir) {
            Some(Ok(mut entries)) => entries.next().is_none(),
            _ => true,
        })
    }
}
impl MissingProbe for RootsMissingProbe {
    fn is_missing(&self, path: &Path) -> std::io::Result<bool> {
        self.probe(path, &mut |_| {})
    }
    fn judge(&self, path: &Path) -> Verdict {
        let place = |parts: &[&std::ffi::OsStr], root: usize| Place {
            root,
            relative: parts.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join("/"),
        };
        let located = self.locate(path);
        match self.probe_detail(path, &mut |_| {}) {
            Ok((false, _)) => Verdict::Present,
            Ok((true, absent)) => match &located {
                // An empty root is not trusted: its rows are not "gone", they are unclear.
                Some((root, r, parts)) if Self::root_is_empty(r) => {
                    Verdict::Unclear { reason: UnclearReason::Unreachable, place: Some(place(parts, *root)) }
                }
                Some((root, _, parts)) => Verdict::Missing { folder: absent.map(|i| place(&parts[..=i], *root)) },
                None => Verdict::Missing { folder: None },
            },
            Err(e) => Verdict::Unclear {
                reason: classify(&e),
                place: located.map(|(root, _, parts)| place(&parts, root)),
            },
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Issues {
    pub orphaned: Vec<i64>,
    pub duplicate_groups: Vec<Vec<i64>>,
    pub truncated: bool,
    /// Web only: rows the probe could not judge, with a place below a root (rows outside every
    /// root have none and are not listed). Display only: never missing, never an anchor.
    pub unclear: Vec<UnclearRow>,
    /// Web only: folders that are gone as a whole (the chain up to the first absent folder is
    /// made of real directories) and the listed orphaned rows below them.
    pub missing_folders: Vec<GoneFolder>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnclearRow {
    pub place: Place,
    pub reason: UnclearReason,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoneFolder {
    pub place: Place,
    pub models: Vec<i64>,
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

/// `strict` (web): only rows the probe positively found are grouped as duplicates. A row the
/// probe could not judge (an `Err`) is neither missing nor safe: it is kept out of every group,
/// so it can never be the "keep" anchor of a group whose only reachable copy would then be
/// preselected for deletion. The desktop (`strict == false`) keeps its established behaviour.
fn scan_rows(rows: &[Row], probe: &dyn MissingProbe, limits: IssueLimits, started: Instant, strict: bool) -> Issues {
    let mut issues = Issues::default();
    let mut orphaned_ids: HashSet<i64> = HashSet::new();
    let mut ungrouped_ids: HashSet<i64> = HashSet::new();
    let mut folders: BTreeMap<(usize, String), Vec<i64>> = BTreeMap::new();
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
        let verdict = probe.judge(Path::new(row.path));
        if strict && matches!(verdict, Verdict::Unclear { .. }) {
            ungrouped_ids.insert(row.id);
        }
        match verdict {
            Verdict::Missing { folder } => {
                if issues.orphaned.len() >= limits.max_orphaned {
                    issues.truncated = true;
                    checked = index;
                    break;
                }
                orphaned_ids.insert(row.id);
                issues.orphaned.push(row.id);
                if let (true, Some(place)) = (strict, folder) {
                    folders.entry((place.root, place.relative)).or_default().push(row.id);
                }
            }
            Verdict::Unclear { reason, place: Some(place) } if strict => {
                if issues.unclear.len() >= limits.max_orphaned {
                    issues.truncated = true;
                } else {
                    issues.unclear.push(UnclearRow { place, reason });
                }
            }
            _ => {}
        }
    }
    issues.missing_folders = folders
        .into_iter()
        .map(|((root, relative), models)| GoneFolder { place: Place { root, relative }, models })
        .collect();
    ungrouped_ids.extend(orphaned_ids.iter().copied());
    let (mut groups, timed_out) = group_rows(&rows[..checked], &ungrouped_ids, started, limits.max_duration);
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
    scan_rows(&rows, probe, limits, started, false)
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
    Ok(scan_rows(&rows, probe, limits, started, true))
}

#[cfg(test)]
#[path = "catalog_issues_tests.rs"]
mod tests;
