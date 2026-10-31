//! Read-only comparison of the catalog with the disk, for the web.
//!
//! Reports which cataloged models are gone from disk, which supported files below the
//! configured roots the catalog does not know, and which companion rows point at nothing.
//! It never changes the database or the disk. The database is read once into a
//! [`Snapshot`] (no transaction is held afterwards); the disk scan then works on that copy.
//!
//! Safety rules: only paths below the configured roots are looked at, links are never
//! followed (a link on the way makes a row "unknown", not "missing"), and nothing in the
//! result is an absolute path: every entry names its root by index and its path relative
//! to that root.
use crate::import::files::{is_ignored_import_path, is_supported_extension};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

const CASE_INSENSITIVE_PATHS: bool = cfg!(any(windows, target_os = "macos"));

/// Upper bounds of one report. Whatever is exceeded ends the work and marks the report
/// as truncated.
#[derive(Clone, Copy)]
pub struct Limits {
    /// Entries per list (missing models, new files, dangling companions).
    pub max_items: usize,
    /// Directory entries looked at while walking the roots.
    pub max_entries: usize,
    pub max_duration: Duration,
    /// Called with each directory right before it is read. For tests that need to act (or
    /// stall) while the scan is running; `None` in production.
    pub walk_hook: Option<&'static (dyn Fn(&Path) + Sync)>,
}
impl Default for Limits {
    fn default() -> Self {
        Self { max_items: 500, max_entries: 200_000, max_duration: Duration::from_secs(20), walk_hook: None }
    }
}

/// A root with at least this many active models, more than half of them gone, is probably
/// only partly mounted.
const MOSTLY_MISSING_MIN_MODELS: usize = 10;

#[derive(Debug, Clone)]
pub struct ModelRow {
    pub id: i64,
    pub path: String,
    /// Not in the trash.
    pub active: bool,
}
#[derive(Debug, Clone)]
pub struct CompanionRow {
    pub id: i64,
    pub file_id: i64,
    pub name: String,
    pub path: String,
}
/// What the catalog says, copied out of the database.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub models: Vec<ModelRow>,
    /// Active companions of active models.
    pub companions: Vec<CompanionRow>,
}

/// Reads the two tables with one statement each and returns; nothing stays open.
pub fn snapshot(conn: &Connection) -> Result<Snapshot, String> {
    let fail = |e: rusqlite::Error| e.to_string();
    let models = conn
        .prepare("SELECT id, path, deleted_at IS NULL FROM files")
        .and_then(|mut s| {
            s.query_map([], |r| Ok(ModelRow { id: r.get(0)?, path: r.get(1)?, active: r.get(2)? }))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(fail)?;
    let companions = conn
        .prepare(
            "SELECT c.id, c.file_id, c.name, c.path FROM companion_files c \
             JOIN files f ON f.id = c.file_id WHERE c.deleted_at IS NULL AND f.deleted_at IS NULL",
        )
        .and_then(|mut s| {
            s.query_map([], |r| Ok(CompanionRow { id: r.get(0)?, file_id: r.get(1)?, name: r.get(2)?, path: r.get(3)? }))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(fail)?;
    Ok(Snapshot { models, companions })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RootStatus {
    Ok,
    /// Not a directory or not readable.
    Unreachable,
    /// Empty although the catalog holds models there: most likely an unmounted volume,
    /// so nothing is reported for it.
    Empty,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RootInfo {
    /// Last path component only.
    pub name: String,
    pub status: RootStatus,
    /// More than half of the root's active models are missing (and there are at least ten):
    /// the volume may be only partly mounted, so the list below may be a symptom, not a cleanup list.
    pub mostly_missing: bool,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MissingModel {
    pub id: String,
    pub root: usize,
    pub relative_path: String,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NewFile {
    pub root: usize,
    pub relative_path: String,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DanglingReason {
    /// Nothing at the path.
    Missing,
    /// Something is there but not a regular file (folder, link, special file).
    NotFile,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DanglingCompanion {
    pub id: String,
    pub file_id: String,
    pub name: String,
    pub root: usize,
    pub relative_path: String,
    pub reason: DanglingReason,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TruncatedBy {
    Items,
    Entries,
    Time,
}
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiskReport {
    pub roots: Vec<RootInfo>,
    pub missing_models: Vec<MissingModel>,
    pub new_files: Vec<NewFile>,
    pub dangling_companions: Vec<DanglingCompanion>,
    /// Active models and companions whose stored path is below no configured root; they
    /// are counted, never listed (no path leaves the server).
    pub models_outside_roots: usize,
    pub companions_outside_roots: usize,
    pub scanned_entries: usize,
    /// The report is incomplete: a limit was hit.
    pub truncated: bool,
    pub truncated_by: Vec<TruncatedBy>,
}

struct Root {
    stored: PathBuf,
    canonical: PathBuf,
    status: RootStatus,
    /// Nested in an earlier root: that root's walk already covers it.
    covered: bool,
}
impl Root {
    /// Path relative to this root when `path` lies below it, spelled either way.
    fn relative(&self, path: &Path) -> Option<PathBuf> {
        let relative = path.strip_prefix(&self.stored).or_else(|_| path.strip_prefix(&self.canonical)).ok()?;
        if relative.as_os_str().is_empty() || relative.components().any(|c| !matches!(c, Component::Normal(_))) {
            return None;
        }
        Some(relative.to_path_buf())
    }
}

fn key(path: &Path) -> PathBuf {
    if CASE_INSENSITIVE_PATHS {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path.to_path_buf()
    }
}
fn shown(relative: &Path) -> String {
    relative.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

#[derive(Debug, PartialEq, Eq)]
enum Presence {
    Missing,
    File,
    /// A folder, a final link or a special file.
    Other,
    /// A link on the way or an I/O error: no statement possible.
    Unknown,
}
/// Looks at `relative` below the canonical root component by component without following links.
fn presence(root: &Path, relative: &Path) -> Presence {
    let mut current = root.to_path_buf();
    let count = relative.components().count();
    for (index, component) in relative.components().enumerate() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if index + 1 < count => {
                if !meta.is_dir() {
                    return if meta.file_type().is_symlink() { Presence::Unknown } else { Presence::Missing };
                }
            }
            Ok(meta) => return if meta.is_file() { Presence::File } else { Presence::Other },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Presence::Missing,
            Err(_) => return Presence::Unknown,
        }
    }
    Presence::Unknown
}

struct Budget {
    started: Instant,
    limits: Limits,
    truncated: Vec<TruncatedBy>,
}
impl Budget {
    fn mark(&mut self, by: TruncatedBy) {
        if !self.truncated.contains(&by) {
            self.truncated.push(by);
        }
    }
    fn out_of_time(&mut self) -> bool {
        if self.started.elapsed() >= self.limits.max_duration {
            self.mark(TruncatedBy::Time);
            return true;
        }
        false
    }
    /// Room for one more entry in a list that holds `len`; marks the report if not.
    fn room(&mut self, len: usize) -> bool {
        if len >= self.limits.max_items {
            self.mark(TruncatedBy::Items);
            return false;
        }
        true
    }
}

fn open_roots(configured: &[PathBuf]) -> Vec<Root> {
    let mut roots: Vec<Root> = configured
        .iter()
        .map(|stored| match std::fs::canonicalize(stored) {
            Ok(canonical) if canonical.is_dir() => Root { stored: stored.clone(), canonical, status: RootStatus::Ok, covered: false },
            _ => Root { stored: stored.clone(), canonical: stored.clone(), status: RootStatus::Unreachable, covered: false },
        })
        .collect();
    // Shallow roots first, so a nested or duplicate root is attributed to the one above it.
    roots.sort_by_key(|r| r.canonical.components().count());
    roots
}

/// Whether an earlier, usable root already walks each root. Run after the status checks:
/// a root that turned out empty or unreachable covers nothing.
fn mark_covered(roots: &mut [Root]) {
    for index in 0..roots.len() {
        let covered = roots[..index].iter().any(|r| r.status == RootStatus::Ok && roots[index].canonical.starts_with(&r.canonical));
        roots[index].covered = covered && roots[index].status == RootStatus::Ok;
    }
}

/// The root a stored path belongs to and its path below it.
fn locate(roots: &[Root], path: &str) -> Option<(usize, PathBuf)> {
    let path = Path::new(path);
    // A usable root wins; a row that lies only below an unreachable root still belongs to it
    // (so it is not mistaken for a row outside every root) and is skipped by the callers.
    let find = |usable: bool| {
        roots.iter().enumerate().filter(|(_, r)| (r.status != RootStatus::Unreachable) == usable).find_map(|(i, r)| r.relative(path).map(|rel| (i, rel)))
    };
    find(true).or_else(|| find(false))
}

/// Compares the snapshot with the disk below `configured`. Never fails: whatever cannot
/// be judged is left out and, where a limit caused it, `truncated` says so.
pub fn build_report(snapshot: &Snapshot, configured: &[PathBuf], limits: Limits) -> DiskReport {
    let mut budget = Budget { started: Instant::now(), limits, truncated: Vec::new() };
    let mut roots = open_roots(configured);
    let mut report = DiskReport::default();

    // A root that is empty although the catalog holds models in it is not trusted.
    for root in roots.iter_mut() {
        if root.status != RootStatus::Ok {
            continue;
        }
        let holds_models = snapshot.models.iter().any(|m| m.active && root.relative(Path::new(&m.path)).is_some());
        match std::fs::read_dir(&root.canonical) {
            Err(_) => root.status = RootStatus::Unreachable,
            Ok(mut entries) => {
                if holds_models && entries.next().is_none() {
                    root.status = RootStatus::Empty;
                }
            }
        }
    }
    mark_covered(&mut roots);

    // What the catalog knows is built first and without the clock: it is only database data,
    // and a walk must never list a known file as new because the checks below ran out of time.
    let mut known = HashSet::new();
    let mut companion_dirs = HashSet::new();
    let mut model_checks = Vec::new();
    for model in &snapshot.models {
        match locate(&roots, &model.path) {
            None => {
                if model.active {
                    report.models_outside_roots += 1;
                }
            }
            Some((index, relative)) => {
                known.insert(key(&roots[index].canonical.join(&relative)));
                if model.active && roots[index].status == RootStatus::Ok {
                    model_checks.push((model.id, index, relative));
                }
            }
        }
    }
    let mut companion_checks = Vec::new();
    for row in &snapshot.companions {
        let Some((index, relative)) = locate(&roots, &row.path) else {
            report.companions_outside_roots += 1;
            continue;
        };
        if let Some(dir) = roots[index].canonical.join(&relative).parent() {
            companion_dirs.insert(key(dir));
        }
        if roots[index].status == RootStatus::Ok {
            companion_checks.push((row, index, relative));
        }
    }

    let mut checked = vec![0usize; roots.len()];
    let mut gone = vec![0usize; roots.len()];
    let mut all_checked = true;
    for (id, index, relative) in model_checks {
        if budget.out_of_time() {
            all_checked = false;
            break;
        }
        checked[index] += 1;
        if presence(&roots[index].canonical, &relative) == Presence::Missing {
            gone[index] += 1;
            if budget.room(report.missing_models.len()) {
                report.missing_models.push(MissingModel { id: id.to_string(), root: index, relative_path: shown(&relative) });
            }
        }
    }
    let mostly_missing: Vec<bool> =
        (0..roots.len()).map(|i| all_checked && checked[i] >= MOSTLY_MISSING_MIN_MODELS && gone[i] * 2 > checked[i]).collect();
    report.roots = roots
        .iter()
        .enumerate()
        .map(|(i, r)| RootInfo {
            name: r.stored.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            status: r.status,
            mostly_missing: mostly_missing[i],
        })
        .collect();

    for (row, index, relative) in companion_checks {
        if budget.out_of_time() {
            break;
        }
        let reason = match presence(&roots[index].canonical, &relative) {
            Presence::Missing => DanglingReason::Missing,
            Presence::Other => DanglingReason::NotFile,
            Presence::File | Presence::Unknown => continue,
        };
        if budget.room(report.dangling_companions.len()) {
            report.dangling_companions.push(DanglingCompanion {
                id: row.id.to_string(),
                file_id: row.file_id.to_string(),
                name: row.name.clone(),
                root: index,
                relative_path: shown(&relative),
                reason,
            });
        }
    }

    // After a timeout the walk is skipped altogether; `truncated` already says so.
    let walkable = !budget.out_of_time();
    for (index, root) in roots.iter().enumerate().filter(|_| walkable) {
        if root.status != RootStatus::Ok || root.covered {
            continue;
        }
        if !walk(index, root, &known, &companion_dirs, &mut budget, &mut report) {
            break;
        }
    }

    report.missing_models.sort_by(|a, b| (a.root, &a.relative_path).cmp(&(b.root, &b.relative_path)));
    report.new_files.sort_by(|a, b| (a.root, &a.relative_path).cmp(&(b.root, &b.relative_path)));
    report.dangling_companions.sort_by(|a, b| (a.root, &a.relative_path).cmp(&(b.root, &b.relative_path)));
    report.truncated = !budget.truncated.is_empty();
    report.truncated_by = budget.truncated;
    report
}

/// Lists supported files the catalog does not hold. Returns false once the walk has to stop
/// for good (entry or time limit), so no further root is started.
fn walk(
    index: usize,
    root: &Root,
    known: &HashSet<PathBuf>,
    companion_dirs: &HashSet<PathBuf>,
    budget: &mut Budget,
    report: &mut DiskReport,
) -> bool {
    let mut pending = vec![root.canonical.clone()];
    while let Some(dir) = pending.pop() {
        if let Some(hook) = budget.limits.walk_hook {
            hook(&dir);
        }
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        // Never hold more than the remaining entry budget plus one (which trips the limit below).
        let room = budget.limits.max_entries.saturating_sub(report.scanned_entries);
        let mut entries: Vec<_> = read.filter_map(Result::ok).take(room.saturating_add(1)).collect();
        entries.sort_by_key(|e| e.file_name());
        let mut subdirs = Vec::new();
        for entry in entries {
            if report.scanned_entries >= budget.limits.max_entries {
                budget.mark(TruncatedBy::Entries);
                return false;
            }
            report.scanned_entries += 1;
            if report.scanned_entries.is_multiple_of(256) && budget.out_of_time() {
                return false;
            }
            let path = entry.path();
            if is_ignored_import_path(&path) {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                if crate::companions::is_extras_dir_name(&path) || companion_dirs.contains(&key(&path)) {
                    continue;
                }
                subdirs.push(path);
            } else if kind.is_file() && is_supported_extension(&path) && !known.contains(&key(&path)) {
                if !budget.room(report.new_files.len()) {
                    return false;
                }
                if let Ok(relative) = path.strip_prefix(&root.canonical) {
                    report.new_files.push(NewFile { root: index, relative_path: shown(relative) });
                }
            }
        }
        pending.extend(subdirs.into_iter().rev());
    }
    true
}

#[cfg(test)]
#[path = "disk_report_tests.rs"]
mod tests;
