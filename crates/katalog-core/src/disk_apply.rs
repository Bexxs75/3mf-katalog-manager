//! Applying the disk comparison (W2, Forgejo #39): what a row of the report may do, checked
//! again right before it is done. Pure core logic; the server decides when to call what and
//! holds the locks (the functions get a connection and hold nothing).
//!
//! Safety rules (spec 2.6):
//! - Every file access goes through [`Anchored`]: each path component is opened relative to the
//!   handle of the one before it (`openat` with `O_NOFOLLOW`), starting at a root handle. A link on
//!   any component is an error, the whole path is never canonicalized. Excluded folders and other
//!   roots are recognised by the identity (dev, ino) of the handles that were opened on the way.
//! - A row remembers the identity of its root. The root is opened again before the row is
//!   changed; a different, unreadable or empty (although the catalog holds models there) root
//!   makes the row `root_unavailable`. This holds for model rows and companion rows alike.
//! - Importing re-resolves the current path entry before the commit and rolls back on any
//!   deviation; the open handle alone would keep describing a replaced inode.
//!
//! Linux only: the anchored resolution is built on `fsops::FsContext` and `upload_fs::Directory`,
//! which exist only there. Other systems have no web write mode; the module is absent.
#![cfg(target_os = "linux")]

use crate::db;
use crate::db::models::NewFile;
use crate::disk_report::{self, Presence, Root as ReportRoot, RootStatus};
use crate::fsops::{Access, Error as FsError, FsContext, Identity, Protection, TRASH};
use crate::import::files::{is_ignored_import_path, is_model_extension, read_web_model, WebImportLimits};
use crate::rescan::{FileStamp, RescanError, StepReader};
use crate::upload_fs::Directory;
use rusqlite::{Connection, TransactionBehavior};
use serde::Serialize;
use std::collections::HashSet;
use std::fs::File;
use std::path::{Component, Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_COMPONENTS: usize = 64;

/// (device, inode)
pub type Ident = (u64, u64);

/// Stable reason codes (spec 2.8) for a row that is skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    Changed,
    ExistsAgain,
    AlreadyInCatalog,
    RootUnavailable,
    Outside,
    Invalid,
    TooLarge,
    StepUnavailable,
    HasCompanions,
    NotAFile,
    Unclear,
    Protected,
    Failed,
}
impl SkipReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::ExistsAgain => "exists_again",
            Self::AlreadyInCatalog => "already_in_catalog",
            Self::RootUnavailable => "root_unavailable",
            Self::Outside => "outside",
            Self::Invalid => "invalid",
            Self::TooLarge => "too_large",
            Self::StepUnavailable => "step_unavailable",
            Self::HasCompanions => "has_companions",
            Self::NotAFile => "not_a_file",
            Self::Unclear => "unclear",
            Self::Protected => "protected",
            Self::Failed => "failed",
        }
    }
}
type Res<T> = Result<T, SkipReason>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    New,
    MissingModel,
    DanglingCompanion,
}

#[derive(Debug, Clone)]
enum Detail {
    New { stamp: FileStamp, entry: Ident },
    Model { id: i64, path: String, content_hash: Option<String> },
    Companion { id: i64, file_id: i64, path: String },
}

/// One planned row: what the preview saw. Never leaves the server.
#[derive(Debug, Clone)]
pub struct PlanRow {
    pub kind: RowKind,
    /// `"<root>:<relative>"` for new files, the id for the others.
    pub key: String,
    pub root: usize,
    /// Identity of the root the row lies below, as seen by the preview.
    pub root_identity: Ident,
    pub relative: String,
    detail: Detail,
}
impl PlanRow {
    /// Model id (missing model) or companion id (dangling companion).
    pub fn id(&self) -> Option<i64> {
        match &self.detail {
            Detail::Model { id, .. } | Detail::Companion { id, .. } => Some(*id),
            Detail::New { .. } => None,
        }
    }
    pub fn file_id(&self) -> Option<i64> {
        match &self.detail {
            Detail::Companion { file_id, .. } => Some(*file_id),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// paths

/// Checks a client-supplied path below a root (spec 2.3) and returns it as a relative path.
/// Shape violations are `Outside`, an unsupported ending is `Invalid`. Backslash is an ordinary
/// character; only `/` separates.
pub fn validate_relative(path: &str) -> Res<PathBuf> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES || path.contains('\0') || path.starts_with('/') {
        return Err(SkipReason::Outside);
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > MAX_COMPONENTS {
        return Err(SkipReason::Outside);
    }
    let mut out = PathBuf::new();
    for part in &parts {
        if part.is_empty() || *part == "." || *part == ".." || is_ignored_import_path(Path::new(part)) {
            return Err(SkipReason::Outside);
        }
        out.push(part);
    }
    // `.part`/hidden/`~$` are already excluded above for every component, the folder rule is the rest.
    let (last, dirs) = parts.split_last().expect("split gives at least one part");
    if dirs.iter().any(|d| crate::companions::is_extras_dir_name(Path::new(d))) {
        return Err(SkipReason::Outside);
    }
    if !is_model_extension(Path::new(last)) {
        return Err(SkipReason::Invalid);
    }
    Ok(out)
}

fn normalized_lowercase(text: &str) -> String {
    text.nfc().collect::<String>().to_lowercase().nfc().collect()
}

/// Every path the catalog holds (active and in the trash), compared without regard to case and
/// NFC/NFD, because the catalog may be shared with systems that fold them.
#[derive(Debug, Default)]
pub struct CatalogPaths(HashSet<String>);
impl CatalogPaths {
    pub fn load(conn: &Connection) -> Res<Self> {
        let paths = conn
            .prepare("SELECT path FROM files")
            .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<HashSet<_>, _>>())
            .map_err(|_| SkipReason::Failed)?;
        Ok(Self(paths.iter().map(|p| normalized_lowercase(p)).collect()))
    }
    pub fn contains(&self, path: &Path) -> bool {
        self.0.contains(&normalized_lowercase(&path.to_string_lossy()))
    }
}

// ---------------------------------------------------------------------------------------------
// anchored resolution

enum Lookup {
    /// A component (or the entry) does not exist.
    Gone,
    /// Excluded folder, other root, trash, other device.
    Excluded,
    /// A link (or a non-directory) where a directory was needed.
    Link,
    Unclear,
}

/// An opened regular file together with what it looked like when it was opened.
pub struct Opened {
    pub file: File,
    pub(crate) stamp: FileStamp,
    /// Identity of the directory entry (not followed).
    pub entry: Ident,
}

/// All file access of W2. Built once per request from the configured roots; the root handles it
/// pins stay valid for that long, so callers that compare against a plan also use
/// [`Anchored::check_root`], which opens the root path again.
///
/// Building it creates the root trash folders (`.katalog-papierkorb`) if they are missing, like
/// `FsContext` does for the file operations. Roots that cannot be opened (or that lie below an
/// earlier root) get no handle; everything on them is `root_unavailable`.
pub struct Anchored {
    roots: Vec<ReportRoot>,
    slots: Vec<Option<usize>>,
    ctx: FsContext,
    excluded: Vec<Identity>,
}

fn errno(error: &std::io::Error) -> Option<i32> {
    error.raw_os_error()
}

impl Anchored {
    /// `configured` is the roots as configured; the report index of a root is its position in
    /// [`disk_report::ordered_roots`]. `excluded` are folders nothing may be read from (upload
    /// folder); ones that do not exist are ignored. Fails (`root_unavailable`) when the roots
    /// cannot be combined into a context at all (for example one lies inside an excluded folder).
    pub fn new(configured: &[PathBuf], excluded: &[PathBuf]) -> Res<Self> {
        let mut roots = disk_report::open_roots(configured);
        for root in roots.iter_mut().filter(|r| r.status == RootStatus::Ok) {
            if Directory::open(&root.canonical).is_err() {
                root.status = RootStatus::Unreachable;
            }
        }
        disk_report::mark_covered(&mut roots);
        let mut slots = vec![None; roots.len()];
        let mut paths = Vec::new();
        for (index, root) in roots.iter().enumerate() {
            if root.status == RootStatus::Ok && !root.covered {
                slots[index] = Some(paths.len());
                paths.push(root.canonical.clone());
            }
        }
        let protected: Vec<(PathBuf, Protection)> = excluded
            .iter()
            .filter_map(|p| std::fs::canonicalize(p).ok())
            .filter(|p| p.is_dir())
            .map(|p| (p, Protection::Upload))
            .collect();
        let excluded = protected
            .iter()
            .map(|(p, _)| Directory::open(p).ok().and_then(|d| Identity::of(&d).ok()))
            .collect::<Option<Vec<_>>>()
            .ok_or(SkipReason::RootUnavailable)?;
        let ctx = FsContext::new(&paths, &protected).map_err(|_| SkipReason::RootUnavailable)?;
        Ok(Self { roots, slots, ctx, excluded })
    }

    /// Root and relative path of a stored absolute path, indexed like the report.
    pub fn locate(&self, stored: &str) -> Option<(usize, String)> {
        disk_report::locate(&self.roots, stored).map(|(i, rel)| (i, disk_report::shown(&rel)))
    }

    fn stored_root(&self, root: usize) -> Res<&Path> {
        self.roots.get(root).map(|r| r.stored.as_path()).ok_or(SkipReason::RootUnavailable)
    }

    /// Identity of the pinned handle of `root`; `None` for a root without one.
    pub fn root_identity(&self, root: usize) -> Option<Ident> {
        let slot = (*self.slots.get(root)?)?;
        let id = self.ctx.roots[slot].identity;
        Some((id.dev, id.ino))
    }

    /// Opens the root path afresh (no earlier handle) and returns its identity.
    fn fresh_root_identity(&self, root: usize) -> Option<Ident> {
        use std::os::unix::fs::MetadataExt;
        let dir = Directory::open(&self.roots.get(root)?.canonical).ok()?;
        let meta = dir.metadata().ok()?;
        Some((meta.dev(), meta.ino()))
    }

    /// The root must still be the one the plan saw: the pinned handle and a fresh open of its
    /// path both have the planned identity.
    pub fn check_root_identity(&self, root: usize, expected: Ident) -> Res<()> {
        let pinned = self.root_identity(root).ok_or(SkipReason::RootUnavailable)?;
        let fresh = self.fresh_root_identity(root).ok_or(SkipReason::RootUnavailable)?;
        if pinned != expected || fresh != expected {
            return Err(SkipReason::RootUnavailable);
        }
        Ok(())
    }

    /// [`Self::check_root_identity`] plus the usability test for rows that conclude "gone":
    /// the root must be readable and not empty while the catalog holds active models below it
    /// (an unmounted volume, or a root replaced by an empty folder).
    pub fn root_usable(&self, conn: &Connection, root: usize) -> bool {
        let Some(slot) = self.slots.get(root).copied().flatten() else { return false };
        let Ok(names) = self.ctx.roots[slot].handle.names() else { return false };
        let has_content = names.iter().any(|n| n != "." && n != ".." && n != TRASH);
        has_content || !self.catalog_holds_models_below(conn, root)
    }

    pub fn check_root(&self, conn: &Connection, root: usize, expected: Ident) -> Res<()> {
        self.check_root_identity(root, expected)?;
        if self.root_usable(conn, root) {
            Ok(())
        } else {
            Err(SkipReason::RootUnavailable)
        }
    }

    fn catalog_holds_models_below(&self, conn: &Connection, root: usize) -> bool {
        let Some(r) = self.roots.get(root) else { return true };
        let prefix = |p: &Path| format!("{}/", p.to_string_lossy().trim_end_matches('/'));
        let (a, b) = (prefix(&r.stored), prefix(&r.canonical));
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM files WHERE deleted_at IS NULL \
             AND (substr(path, 1, length(?1)) = ?1 OR substr(path, 1, length(?2)) = ?2))",
            [&a, &b],
            |r| r.get::<_, bool>(0),
        )
        // When the catalog cannot be asked, assume it does: an unclear root is not trusted.
        .unwrap_or(true)
    }

    fn resolve(&self, root: usize, relative: &str) -> Result<crate::fsops::Resolved, Lookup> {
        let slot = self.slots.get(root).copied().flatten().ok_or(Lookup::Unclear)?;
        match self.ctx.resolve_relative(slot, relative, Access::Source) {
            Ok(resolved) => {
                let parent = Identity::of(&resolved.parent).map_err(|_| Lookup::Unclear)?;
                if self.excluded.contains(&parent) {
                    return Err(Lookup::Excluded);
                }
                Ok(resolved)
            }
            Err(FsError::NotFound) => Err(Lookup::Gone),
            Err(FsError::Protected | FsError::CrossDevice) => Err(Lookup::Excluded),
            Err(FsError::PathUnresolvable) => Err(Lookup::Link),
            Err(_) => Err(Lookup::Unclear),
        }
    }

    /// What is at `relative` below `root`, without following a link anywhere. `Missing` only when
    /// the path up to the first absent component ran over real directories only (no link, no
    /// non-directory, no other error) and that component is `NotFound` (a whole folder may be gone).
    pub(crate) fn presence(&self, root: usize, relative: &str) -> Presence {
        let resolved = match self.resolve(root, relative) {
            Ok(r) => r,
            Err(Lookup::Gone) => return Presence::Missing,
            Err(_) => return Presence::Unknown,
        };
        match resolved.parent.entry_identity(&resolved.name) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Presence::Missing,
            Err(_) => return Presence::Unknown,
        }
        match resolved.parent.read(&resolved.name) {
            Ok(_) => Presence::File,
            Err(e) if errno(&e) == Some(libc::ELOOP) || e.kind() == std::io::ErrorKind::InvalidData => Presence::Other,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Presence::Missing,
            Err(_) => Presence::Unknown,
        }
    }

    /// Opens the regular file at `relative` (no link anywhere on the way, not excluded).
    /// `NotFound` is `changed`; a link, an excluded place, a non-file or any other error is `outside`.
    pub fn open_file(&self, root: usize, relative: &str) -> Res<Opened> {
        use std::os::unix::fs::MetadataExt;
        let resolved = self.resolve(root, relative).map_err(|e| match e {
            Lookup::Gone => SkipReason::Changed,
            _ => SkipReason::Outside,
        })?;
        let entry = resolved.parent.entry_identity(&resolved.name).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound { SkipReason::Changed } else { SkipReason::Outside }
        })?;
        let file = resolved.parent.read(&resolved.name).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound { SkipReason::Changed } else { SkipReason::Outside }
        })?;
        let meta = file.metadata().map_err(|_| SkipReason::Outside)?;
        if (meta.dev(), meta.ino()) != entry {
            return Err(SkipReason::Changed);
        }
        Ok(Opened { file, stamp: FileStamp::of(&meta), entry })
    }

    /// The identity of the entry as it is now (re-resolved from the root, nothing cached).
    pub fn entry_now(&self, root: usize, relative: &str) -> Res<Ident> {
        self.open_file(root, relative).map(|o| o.entry)
    }
}

// ---------------------------------------------------------------------------------------------
// planning (preview) and re-checking (apply)

/// A file the report listed as new. Everything that can be seen without writing.
pub fn plan_new_file(anchored: &Anchored, known: &CatalogPaths, root: usize, relative: &str) -> Res<PlanRow> {
    let rel = validate_relative(relative)?;
    let root_identity = anchored.root_identity(root).ok_or(SkipReason::RootUnavailable)?;
    let r = &anchored.roots[root];
    if known.contains(&r.stored.join(&rel)) || known.contains(&r.canonical.join(&rel)) {
        return Err(SkipReason::AlreadyInCatalog);
    }
    let opened = anchored.open_file(root, relative)?;
    Ok(PlanRow {
        kind: RowKind::New,
        key: format!("{root}:{relative}"),
        root,
        root_identity,
        relative: relative.to_owned(),
        detail: Detail::New { stamp: opened.stamp, entry: opened.entry },
    })
}

struct ModelNow {
    root: usize,
    relative: String,
    root_identity: Ident,
    path: String,
    content_hash: Option<String>,
}
fn judge_missing(conn: &Connection, anchored: &Anchored, id: i64) -> Res<ModelNow> {
    let (path, content_hash, active): (String, Option<String>, bool) = conn
        .query_row("SELECT path, content_hash, deleted_at IS NULL FROM files WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|_| SkipReason::Changed)?;
    if !active {
        return Err(SkipReason::Changed);
    }
    let (root, relative) = anchored.locate(&path).ok_or(SkipReason::Outside)?;
    let root_identity = anchored.root_identity(root).ok_or(SkipReason::RootUnavailable)?;
    anchored.check_root(conn, root, root_identity)?;
    match anchored.presence(root, &relative) {
        Presence::Missing => Ok(ModelNow { root, relative, root_identity, path, content_hash }),
        Presence::File | Presence::Other => Err(SkipReason::ExistsAgain),
        Presence::Unknown => Err(SkipReason::Unclear),
    }
}

/// A model the report listed as missing.
pub fn plan_missing_model(conn: &Connection, anchored: &Anchored, id: i64) -> Res<PlanRow> {
    let now = judge_missing(conn, anchored, id)?;
    Ok(PlanRow {
        kind: RowKind::MissingModel,
        key: id.to_string(),
        root: now.root,
        root_identity: now.root_identity,
        relative: now.relative,
        detail: Detail::Model { id, path: now.path, content_hash: now.content_hash },
    })
}

/// Right before removing a missing model: the row, the root and the absence are checked again
/// against the plan. Writes nothing.
pub fn recheck_missing(row: &PlanRow, conn: &Connection, anchored: &Anchored) -> Res<()> {
    let Detail::Model { id, path, content_hash } = &row.detail else { return Err(SkipReason::Failed) };
    // The planned identity first: a replaced root must never reach the presence test.
    anchored.check_root(conn, row.root, row.root_identity)?;
    let now = judge_missing(conn, anchored, *id)?;
    if now.path != *path
        || now.content_hash != *content_hash
        || now.root != row.root
        || now.relative != row.relative
        || now.root_identity != row.root_identity
    {
        return Err(SkipReason::Changed);
    }
    Ok(())
}

struct CompanionNow {
    file_id: i64,
    path: String,
    root: usize,
    relative: String,
    root_identity: Ident,
    presence: Presence,
}
fn judge_companion(conn: &Connection, anchored: &Anchored, id: i64) -> Res<CompanionNow> {
    let (file_id, path, active): (i64, String, bool) = conn
        .query_row(
            "SELECT c.file_id, c.path, c.deleted_at IS NULL AND f.deleted_at IS NULL \
             FROM companion_files c JOIN files f ON f.id = c.file_id WHERE c.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| SkipReason::Changed)?;
    if !active {
        return Err(SkipReason::Changed);
    }
    let (root, relative) = anchored.locate(&path).ok_or(SkipReason::Outside)?;
    let root_identity = anchored.root_identity(root).ok_or(SkipReason::RootUnavailable)?;
    // The same root rules as for models: an emptied or replaced root would make every companion "missing".
    anchored.check_root(conn, root, root_identity)?;
    let presence = anchored.presence(root, &relative);
    Ok(CompanionNow { file_id, path, root, relative, root_identity, presence })
}

/// A companion row the report listed as dangling (nothing there, or something that is not a file).
pub fn plan_dangling_companion(conn: &Connection, anchored: &Anchored, id: i64) -> Res<PlanRow> {
    let now = judge_companion(conn, anchored, id)?;
    match now.presence {
        Presence::Missing | Presence::Other => {}
        Presence::File => return Err(SkipReason::ExistsAgain),
        Presence::Unknown => return Err(SkipReason::Unclear),
    }
    Ok(PlanRow {
        kind: RowKind::DanglingCompanion,
        key: id.to_string(),
        root: now.root,
        root_identity: now.root_identity,
        relative: now.relative,
        detail: Detail::Companion { id, file_id: now.file_id, path: now.path },
    })
}

/// Right before removing a dangling companion row. Writes nothing.
pub fn recheck_dangling(row: &PlanRow, conn: &Connection, anchored: &Anchored) -> Res<()> {
    let Detail::Companion { id, file_id, path } = &row.detail else { return Err(SkipReason::Failed) };
    anchored.check_root(conn, row.root, row.root_identity)?;
    let now = judge_companion(conn, anchored, *id)?;
    if now.path != *path
        || now.file_id != *file_id
        || now.root != row.root
        || now.relative != row.relative
        || now.root_identity != row.root_identity
    {
        return Err(SkipReason::Changed);
    }
    match now.presence {
        Presence::Missing => Ok(()),
        Presence::Other => Err(SkipReason::NotAFile),
        Presence::File => Err(SkipReason::ExistsAgain),
        Presence::Unknown => Err(SkipReason::Unclear),
    }
}

// ---------------------------------------------------------------------------------------------
// importing a new file

/// A new file read and parsed, ready for [`store_new_file`]. Holds the open handle it was parsed from.
pub struct PreparedNew {
    row: PlanRow,
    handle: File,
    new_file: NewFile,
    stored_root: PathBuf,
    logical: PathBuf,
}
impl PreparedNew {
    pub fn row(&self) -> &PlanRow {
        &self.row
    }
}

fn from_rescan(error: RescanError) -> SkipReason {
    match error {
        RescanError::TooLarge => SkipReason::TooLarge,
        RescanError::StepUnavailable => SkipReason::StepUnavailable,
        RescanError::Changed => SkipReason::Changed,
        RescanError::Outside => SkipReason::Outside,
        _ => SkipReason::Invalid,
    }
}

/// Opens the planned file (anchored), checks the stamp against the plan and parses it with the
/// web budgets. STEP needs `step`; without it the row is `step_unavailable`. Parsing happens
/// outside any database lock.
pub fn read_new_file(
    row: &PlanRow,
    anchored: &Anchored,
    limits: WebImportLimits,
    step: Option<StepReader<'_>>,
) -> Res<PreparedNew> {
    let Detail::New { stamp, entry } = &row.detail else { return Err(SkipReason::Failed) };
    anchored.check_root_identity(row.root, row.root_identity)?;
    let rel = validate_relative(&row.relative)?;
    let extension = rel.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    let is_step = matches!(extension.as_deref(), Some("stp" | "step"));
    if is_step && step.is_none() {
        return Err(SkipReason::StepUnavailable);
    }
    let opened = anchored.open_file(row.root, &row.relative)?;
    if opened.stamp != *stamp || opened.entry != *entry {
        return Err(SkipReason::Changed);
    }
    let stored_root = anchored.stored_root(row.root)?.to_path_buf();
    let logical = stored_root.join(&rel);
    let name = rel.file_name().and_then(|n| n.to_str()).ok_or(SkipReason::Outside)?.to_owned();
    let handle = opened.file.try_clone().map_err(|_| SkipReason::Failed)?;
    let step_metadata = match step {
        Some(read) if is_step => Some(read(&opened.file).map_err(from_rescan)?),
        _ => None,
    };
    let mut new_file = read_web_model(opened.file, &logical, &name, limits).map_err(|kind| match kind.as_str() {
        "resource_limit" => SkipReason::TooLarge,
        "changed" => SkipReason::Changed,
        _ => SkipReason::Invalid,
    })?;
    if let Some((dimensions_mm, volume_cm3, object_count)) = step_metadata {
        new_file.dimensions_mm = dimensions_mm;
        new_file.volume_cm3 = volume_cm3;
        new_file.object_count = object_count;
    }
    // Rewritten while it was read: the parsed data would not describe the file.
    if FileStamp::of(&handle.metadata().map_err(|_| SkipReason::Changed)?) != *stamp {
        return Err(SkipReason::Changed);
    }
    Ok(PreparedNew { row: row.clone(), handle, new_file, stored_root, logical })
}

/// Rows for every directory level from `root_dir` to `dir`, like the import does it (a root that is
/// new is hung below its cataloged parent folder). Only rows are written; nothing happens on disk,
/// and the caller has resolved `dir` as an existing directory.
pub fn ensure_folder_chain(conn: &Connection, root_dir: &Path, dir: &Path) -> Result<i64, String> {
    let inside = dir.strip_prefix(root_dir).is_ok_and(|rel| rel.components().all(|c| matches!(c, Component::Normal(_))));
    if !inside {
        return Err("folder outside the root".into());
    }
    let root_is_new = !db::folder_path_exists(conn, root_dir).map_err(|e| e.to_string())?;
    let id = db::ensure_folder_path(conn, root_dir, dir).map_err(|e| e.to_string())?;
    if root_is_new {
        db::attach_folder_to_parent_by_path(conn, root_dir).map_err(|e| e.to_string())?;
    }
    Ok(id)
}

/// The short write step: one transaction. Before the commit (transaction open, caller holds the
/// write lock) the file is looked at once more: the handle's stamp, the root identity, and the
/// current path entry resolved afresh must all still be what the preview saw and what was parsed.
/// Anything else rolls the transaction back (no folder row, no file row) and returns `changed`
/// (`outside` for a link). The window between that check and the COMMIT stays, as with the rescan.
pub fn store_new_file(conn: &mut Connection, anchored: &Anchored, prepared: PreparedNew) -> Res<i64> {
    let PreparedNew { row, handle, mut new_file, stored_root, logical } = prepared;
    let Detail::New { stamp, entry } = &row.detail else { return Err(SkipReason::Failed) };
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|_| SkipReason::Failed)?;
    let known = CatalogPaths::load(&tx)?;
    let canonical_logical = anchored.roots[row.root].canonical.join(logical.strip_prefix(&stored_root).map_err(|_| SkipReason::Failed)?);
    if known.contains(&logical) || known.contains(&canonical_logical) {
        return Err(SkipReason::AlreadyInCatalog);
    }
    let dir = logical.parent().ok_or(SkipReason::Outside)?;
    new_file.folder_id = Some(ensure_folder_chain(&tx, &stored_root, dir).map_err(|_| SkipReason::Failed)?);
    let id = db::insert_file_within_tx(&tx, &new_file).map_err(|_| SkipReason::Failed)?;
    anchored.check_root_identity(row.root, row.root_identity)?;
    let meta = handle.metadata().map_err(|_| SkipReason::Changed)?;
    if FileStamp::of(&meta) != *stamp {
        return Err(SkipReason::Changed);
    }
    let current = anchored.entry_now(row.root, &row.relative)?;
    {
        use std::os::unix::fs::MetadataExt;
        if current != (meta.dev(), meta.ino()) || current != *entry {
            return Err(SkipReason::Changed);
        }
    }
    tx.commit().map_err(|_| SkipReason::Failed)?;
    Ok(id)
}

#[cfg(test)]
#[path = "disk_apply_tests.rs"]
mod tests;
