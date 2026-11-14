//! What the trash dialog and "empty trash" see: one classification of the trashed
//! models, shared by the summary, the snapshot and the server's read-only summary,
//! so the three always agree on how many entries can be emptied.

use super::*;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One trashed model as far as emptying the trash is concerned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrashEntry {
    pub id: i64,
    pub deleted_at: String,
    /// A companion row would be stranded by a permanent delete (it sits in the desktop's
    /// trash or is still active), so the entry is left for the desktop.
    pub blocked: bool,
    /// Bytes of the model plus its companion files in the web trash.
    pub bytes: u64,
    /// Companion files in the web trash that go with the model.
    pub companions: usize,
}

/// Where a companion's `trash_path` points, judged like the delete itself judges it
/// (right root, right name, plain file on the root's device) but without file handles.
enum Place {
    Present,
    Gone,
    Unsafe,
    Outside,
}

fn place(root: &Path, path: &str, row_id: i64) -> Place {
    use std::os::unix::fs::MetadataExt;
    let p = Path::new(path);
    let (Some(dir), Some(name)) = (p.parent(), p.file_name().and_then(|n| n.to_str())) else {
        return Place::Outside;
    };
    if !name.starts_with(&companion_trash_prefix(row_id)) {
        return Place::Outside;
    }
    let Ok(dir) = std::fs::canonicalize(dir) else { return Place::Outside };
    if !std::fs::canonicalize(root.join(TRASH)).is_ok_and(|t| t == dir) {
        return Place::Outside;
    }
    let Ok(root_dev) = std::fs::metadata(root).map(|m| m.dev()) else { return Place::Unsafe };
    match std::fs::symlink_metadata(p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Place::Gone,
        Ok(m) if m.is_file() && m.nlink() == 1 && m.dev() == root_dev => Place::Present,
        _ => Place::Unsafe,
    }
}

/// Whether the web can bring a removed companion back: its file sits in this root's web
/// trash under its own name, or is gone / was never moved (only the row is restored).
/// A desktop-trash path or anything unsafe is left to the desktop.
pub fn web_restorable(roots: &[PathBuf], model_path: &str, row_id: i64, trash_path: Option<&str>) -> bool {
    let Some(root) = roots
        .iter()
        .filter(|r| Path::new(model_path).starts_with(r))
        .max_by_key(|r| r.components().count())
    else {
        return false;
    };
    match trash_path {
        None => true,
        Some(p) => matches!(place(root, p, row_id), Place::Present | Place::Gone),
    }
}

/// A companion file that was removed on its own while its model is still in the
/// catalog. Emptying the trash deletes these for good.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LooseCompanion {
    pub id: i64,
    pub deleted_at: String,
    /// Bytes of the file when it sits in this root's web trash, else 0.
    pub bytes: u64,
}

/// Removed companion files of live models whose trash file is ours (or already gone).
/// A row in the desktop's trash or under a foreign name is left to the desktop.
pub fn loose_companions(c: &Connection, roots: &[PathBuf]) -> Result<Vec<LooseCompanion>> {
    let mut stmt = c.prepare(
        "SELECT c.id,c.deleted_at,c.trash_path,c.size_bytes,f.path,c.name,c.path,c.kind FROM companion_files c
         JOIN files f ON f.id=c.file_id WHERE c.deleted_at IS NOT NULL AND f.deleted_at IS NULL ORDER BY c.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<i64>>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, String>(7)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, deleted_at, trash_path, size, model_path, name, path, kind) = row?;
        // The same checks the removal itself makes: a row it would refuse is neither counted nor offered.
        if crate::companions::validate_stored_fields(&name, &path, &kind).is_err()
            || chrono::DateTime::parse_from_rfc3339(&deleted_at).is_err()
        {
            continue;
        }
        let Some(root) = roots
            .iter()
            .filter(|r| Path::new(&model_path).starts_with(r))
            .max_by_key(|r| r.components().count())
        else {
            continue;
        };
        let bytes = match trash_path {
            None => 0,
            Some(p) => match place(root, &p, id) {
                Place::Present => size.unwrap_or(0).max(0) as u64,
                Place::Gone => 0,
                Place::Unsafe | Place::Outside => continue,
            },
        };
        out.push(LooseCompanion { id, deleted_at, bytes });
    }
    Ok(out)
}

pub fn trash_entries(c: &Connection, roots: &[PathBuf]) -> Result<Vec<TrashEntry>> {
    let mut companions: HashMap<i64, (bool, usize, u64)> = HashMap::new();
    let mut stmt = c.prepare(
        "SELECT c.file_id,c.id,c.deleted_at IS NULL,c.trash_path,c.size_bytes,f.path FROM companion_files c
         JOIN files f ON f.id=c.file_id WHERE f.deleted_at IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, bool>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<i64>>(4)?,
            r.get::<_, String>(5)?,
        ))
    })?;
    for row in rows {
        let (file_id, id, active, trash_path, size, model_path) = row?;
        let entry = companions.entry(file_id).or_default();
        // An active row still has its file in the catalog, whatever its trash_path says.
        if active {
            entry.0 = true;
            continue;
        }
        let Some(p) = trash_path else { continue };
        // Only the trash of the root the model lives in is its own.
        let root = roots
            .iter()
            .filter(|r| Path::new(&model_path).starts_with(r))
            .max_by_key(|r| r.components().count());
        match root.map_or(Place::Outside, |root| place(root, &p, id)) {
            Place::Present => {
                entry.1 += 1;
                entry.2 = entry.2.saturating_add(size.unwrap_or(0).max(0) as u64);
            }
            Place::Gone | Place::Unsafe => (),
            Place::Outside => entry.0 = true,
        }
    }
    let mut stmt = c.prepare("SELECT id,deleted_at,file_size_bytes FROM files WHERE deleted_at IS NOT NULL ORDER BY id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (id, deleted_at, size) = row?;
        let (blocked, count, bytes) = companions.get(&id).copied().unwrap_or_default();
        out.push(TrashEntry {
            id,
            deleted_at,
            blocked,
            bytes: (size.max(0) as u64).saturating_add(bytes),
            companions: count,
        });
    }
    Ok(out)
}

/// Counts the entries that "empty trash" would remove. Blocked entries are listed
/// but neither counted nor sized, so limits and backups are not spent on refusals.
pub fn trash_summary(c: &Connection, roots: &[PathBuf]) -> Result<TrashSummary> {
    let mut summary = TrashSummary {
        count: 0,
        blocked_by_companions: 0,
        companion_count: 0,
        loose_companion_count: 0,
        loose_companion_bytes: 0,
        bytes: 0,
        max_deleted_at: None,
    };
    // The newest stamp is handed back verbatim, so the client's cutoff matches the stored text.
    let mut max: Option<(chrono::DateTime<chrono::FixedOffset>, String)> = None;
    for entry in trash_entries(c, roots)? {
        let time = chrono::DateTime::parse_from_rfc3339(&entry.deleted_at).map_err(|_| Error::Changed)?;
        if entry.blocked {
            summary.blocked_by_companions += 1;
            continue;
        }
        summary.count += 1;
        summary.companion_count += entry.companions;
        summary.bytes = summary.bytes.saturating_add(entry.bytes);
        if max.as_ref().is_none_or(|(old, _)| time > *old) {
            max = Some((time, entry.deleted_at));
        }
    }
    for loose in loose_companions(c, roots)? {
        let time = chrono::DateTime::parse_from_rfc3339(&loose.deleted_at).map_err(|_| Error::Changed)?;
        summary.loose_companion_count += 1;
        summary.loose_companion_bytes = summary.loose_companion_bytes.saturating_add(loose.bytes);
        if max.as_ref().is_none_or(|(old, _)| time > *old) {
            max = Some((time, loose.deleted_at));
        }
    }
    summary.max_deleted_at = max.map(|(_, text)| text);
    Ok(summary)
}
