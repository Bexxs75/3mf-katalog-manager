//! Catalog health check: models whose file is gone and groups of identical files.
//! The desktop app and the web server share this logic; only the way "is the file there"
//! is answered differs per platform (`MissingProbe`).
use crate::db::{self, models::FileRecord};
use rusqlite::Connection;
use std::borrow::Borrow;
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
pub struct RootsMissingProbe<'a> {
    roots: &'a [PathBuf],
}
impl<'a> RootsMissingProbe<'a> {
    pub fn new(roots: &'a [PathBuf]) -> Self {
        Self { roots }
    }
}
impl MissingProbe for RootsMissingProbe<'_> {
    fn is_missing(&self, path: &Path) -> std::io::Result<bool> {
        let unclear = |what: &str| std::io::Error::other(what.to_string());
        let (root, relative) = self
            .roots
            .iter()
            .find_map(|root| path.strip_prefix(root).ok().map(|rel| (root, rel)))
            .ok_or_else(|| unclear("outside the roots"))?;
        let count = relative.components().count();
        if count == 0 {
            return Err(unclear("is a root"));
        }
        let mut current = root.clone();
        for (index, component) in relative.components().enumerate() {
            if !matches!(component, std::path::Component::Normal(_)) {
                return Err(unclear("not a plain relative path"));
            }
            current.push(component);
            match std::fs::symlink_metadata(&current) {
                Ok(meta) if index + 1 < count => {
                    if !meta.is_dir() {
                        return if meta.file_type().is_symlink() { Err(unclear("link on the way")) } else { Ok(true) };
                    }
                }
                Ok(meta) => {
                    return if meta.file_type().is_symlink() { Err(unclear("link")) } else { Ok(false) };
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
                Err(e) => return Err(e),
            }
        }
        Err(unclear("empty path"))
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

/// Groups by `content_hash`, skipping `orphaned_ids` (a missing file must never become a
/// group's "keep the oldest" anchor). Only groups with 2+ members, each sorted oldest-first,
/// groups sorted by their oldest member.
pub fn group_duplicates(files: Vec<FileRecord>, orphaned_ids: &HashSet<i64>) -> Vec<Vec<FileRecord>> {
    group_by_hash(files, orphaned_ids)
}

fn group_by_hash<T: Borrow<FileRecord>>(files: Vec<T>, orphaned_ids: &HashSet<i64>) -> Vec<Vec<T>> {
    let mut by_hash: BTreeMap<String, Vec<T>> = BTreeMap::new();
    for file in files {
        let record: &FileRecord = file.borrow();
        if orphaned_ids.contains(&record.id) {
            continue;
        }
        if let Some(hash) = record.content_hash.clone() {
            by_hash.entry(hash).or_default().push(file);
        }
    }
    let mut groups: Vec<Vec<T>> = by_hash
        .into_values()
        .filter(|group| group.len() >= 2)
        .map(|mut group| {
            group.sort_by(|a, b| a.borrow().imported_at.cmp(&b.borrow().imported_at));
            group
        })
        .collect();
    groups.sort_by(|a, b| a[0].borrow().imported_at.cmp(&b[0].borrow().imported_at));
    groups
}

/// Same as `scan` for rows the caller already loaded (the desktop needs the full records
/// for its DTOs and must not read the catalog twice).
pub fn scan_records(files: &[FileRecord], probe: &dyn MissingProbe, limits: IssueLimits) -> Issues {
    let started = Instant::now();
    let mut issues = Issues::default();
    let mut orphaned_ids: HashSet<i64> = HashSet::new();
    for file in files {
        if started.elapsed() >= limits.max_duration {
            issues.truncated = true;
            break;
        }
        if probe.is_missing(Path::new(&file.path)).unwrap_or(false) {
            if issues.orphaned.len() >= limits.max_orphaned {
                issues.truncated = true;
                break;
            }
            orphaned_ids.insert(file.id);
            issues.orphaned.push(file.id);
        }
    }
    let mut groups = group_by_hash(files.iter().collect(), &orphaned_ids);
    if groups.len() > limits.max_groups {
        groups.truncate(limits.max_groups);
        issues.truncated = true;
    }
    issues.duplicate_groups = groups.into_iter().map(|g| g.into_iter().map(|f| f.id).collect()).collect();
    issues
}

pub fn scan(conn: &Connection, probe: &dyn MissingProbe, limits: IssueLimits) -> Result<Issues, String> {
    let files = db::list_files(conn).map_err(|e| e.to_string())?;
    Ok(scan_records(&files, probe, limits))
}

#[cfg(test)]
#[path = "catalog_issues_tests.rs"]
mod tests;
