// Repository functions for bundles: files shown as one card, either variants of one model or the
// parts of a multi-part model. A bundle is only a name, a kind and an ordered member list; tags,
// license, print status and everything else stay with the files, and no file is ever touched.
//
// The functions do not open a transaction of their own, so the caller decides how a group of
// them is made atomic (the web path already holds an immediate one).

use rusqlite::{params, Connection, OptionalExtension};

use super::error::DbError;
use crate::error_code::ErrorCode;

pub const MIN_BUNDLE_MEMBERS: usize = 2;
pub const MAX_BUNDLE_MEMBERS: usize = 200;
pub const MAX_BUNDLE_NAME_CHARS: usize = 120;
pub const MAX_BUNDLE_LABEL_CHARS: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleKind {
    Variants,
    Parts,
}

impl BundleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BundleKind::Variants => "variants",
            BundleKind::Parts => "parts",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "variants" => Some(BundleKind::Variants),
            "parts" => Some(BundleKind::Parts),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleMemberRecord {
    pub file_id: i64,
    pub position: i64,
    pub label: Option<String>,
    /// The file sits in the trash; it stays a member until it is deleted for good.
    pub in_trash: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleRecord {
    pub id: i64,
    pub name: String,
    pub kind: BundleKind,
    pub created_at: String,
    pub members: Vec<BundleMemberRecord>,
}

fn has_control_char(text: &str) -> bool {
    text.chars().any(char::is_control)
}

/// Returns the name as it is stored (trimmed).
pub fn validate_bundle_name(name: &str) -> Result<String, DbError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_BUNDLE_NAME_CHARS || has_control_char(name) {
        return Err(DbError::coded_with(ErrorCode::BundleNameInvalid, [MAX_BUNDLE_NAME_CHARS]));
    }
    Ok(name.to_string())
}

/// Returns the label as it is stored: trimmed, `None` when nothing is left.
pub fn validate_bundle_label(label: &str) -> Result<Option<String>, DbError> {
    let label = label.trim();
    if label.chars().count() > MAX_BUNDLE_LABEL_CHARS || has_control_char(label) {
        return Err(DbError::coded_with(ErrorCode::BundleLabelInvalid, [MAX_BUNDLE_LABEL_CHARS]));
    }
    Ok((!label.is_empty()).then(|| label.to_string()))
}

fn bundle_exists(conn: &Connection, bundle_id: i64) -> Result<(), DbError> {
    let found: Option<i64> = conn
        .query_row("SELECT 1 FROM bundles WHERE id = ?1", [bundle_id], |r| r.get(0))
        .optional()?;
    found.map(|_| ()).ok_or_else(|| DbError::coded(ErrorCode::BundleNotFound))
}

/// Only files that are in the catalog and not in the trash can join a bundle.
fn require_living_files(conn: &Connection, file_ids: &[i64]) -> Result<(), DbError> {
    let mut stmt = conn.prepare("SELECT 1 FROM files WHERE id = ?1 AND deleted_at IS NULL")?;
    for id in file_ids {
        if stmt.query_row([id], |r| r.get::<_, i64>(0)).optional()?.is_none() {
            return Err(DbError::coded(ErrorCode::EntryNotFound));
        }
    }
    Ok(())
}

/// Rejects a repeated id: a file is a member once.
fn require_distinct(file_ids: &[i64]) -> Result<(), DbError> {
    let mut seen = std::collections::HashSet::new();
    if file_ids.iter().all(|id| seen.insert(*id)) {
        Ok(())
    } else {
        Err(DbError::coded(ErrorCode::BundleDuplicateFiles))
    }
}

fn require_member_count(count: usize) -> Result<(), DbError> {
    if count < MIN_BUNDLE_MEMBERS {
        return Err(DbError::coded_with(ErrorCode::BundleTooFewFiles, [MIN_BUNDLE_MEMBERS]));
    }
    if count > MAX_BUNDLE_MEMBERS {
        return Err(DbError::coded_with(ErrorCode::BundleTooManyFiles, [MAX_BUNDLE_MEMBERS]));
    }
    Ok(())
}

/// Removes every bundle with fewer than two member rows (their last member goes with it).
/// Called wherever files are deleted for good, because the database has no trigger to do it:
/// the backup check refuses triggers. A bundle that slips through is invisible to
/// `list_bundles` and is removed here the next time.
pub fn prune_bundles(conn: &Connection) -> Result<usize, DbError> {
    Ok(conn.execute(
        "DELETE FROM bundles WHERE (SELECT COUNT(*) FROM bundle_members m WHERE m.bundle_id = bundles.id) < ?1",
        [MIN_BUNDLE_MEMBERS as i64],
    )?)
}

/// Creates a bundle from 2 to 200 distinct files, in the order given (position 0 is the title
/// model). A file that already belongs to another bundle moves over; a bundle that is left with
/// fewer than two members is dissolved.
pub fn create_bundle(
    conn: &Connection,
    name: &str,
    kind: BundleKind,
    file_ids: &[i64],
    created_at: &str,
) -> Result<i64, DbError> {
    let name = validate_bundle_name(name)?;
    require_member_count(file_ids.len())?;
    require_distinct(file_ids)?;
    require_living_files(conn, file_ids)?;

    let mut leave = conn.prepare("DELETE FROM bundle_members WHERE file_id = ?1")?;
    for id in file_ids {
        leave.execute([id])?;
    }
    drop(leave);
    conn.execute(
        "INSERT INTO bundles (name, kind, created_at) VALUES (?1, ?2, ?3)",
        params![name, kind.as_str(), created_at],
    )?;
    let bundle_id = conn.last_insert_rowid();
    let mut join = conn.prepare("INSERT INTO bundle_members (file_id, bundle_id, position) VALUES (?1, ?2, ?3)")?;
    for (position, id) in file_ids.iter().enumerate() {
        join.execute(params![id, bundle_id, position as i64])?;
    }
    drop(join);
    prune_bundles(conn)?;
    Ok(bundle_id)
}

/// Appends files at the end of a bundle. A file already in this bundle stays where it is; a file
/// of another bundle moves over. The bundle never exceeds 200 members.
pub fn add_files_to_bundle(conn: &Connection, bundle_id: i64, file_ids: &[i64]) -> Result<(), DbError> {
    bundle_exists(conn, bundle_id)?;
    require_distinct(file_ids)?;
    require_living_files(conn, file_ids)?;

    let mut here = std::collections::HashSet::new();
    {
        let mut stmt = conn.prepare("SELECT file_id FROM bundle_members WHERE bundle_id = ?1")?;
        for row in stmt.query_map([bundle_id], |r| r.get::<_, i64>(0))? {
            here.insert(row?);
        }
    }
    let new_ids: Vec<i64> = file_ids.iter().copied().filter(|id| !here.contains(id)).collect();
    if here.len() + new_ids.len() > MAX_BUNDLE_MEMBERS {
        return Err(DbError::coded_with(ErrorCode::BundleTooManyFiles, [MAX_BUNDLE_MEMBERS]));
    }
    let mut next: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), -1) FROM bundle_members WHERE bundle_id = ?1",
        [bundle_id],
        |r| r.get(0),
    )?;
    let mut leave = conn.prepare("DELETE FROM bundle_members WHERE file_id = ?1")?;
    let mut join = conn.prepare("INSERT INTO bundle_members (file_id, bundle_id, position) VALUES (?1, ?2, ?3)")?;
    for id in new_ids {
        leave.execute([id])?;
        next += 1;
        join.execute(params![id, bundle_id, next])?;
    }
    drop((leave, join));
    prune_bundles(conn)?;
    Ok(())
}

/// Takes files out of their bundles. Files that are in none are ignored. A bundle left with fewer
/// than two members is dissolved. Returns how many memberships were removed.
pub fn remove_files_from_bundle(conn: &Connection, file_ids: &[i64]) -> Result<usize, DbError> {
    let mut leave = conn.prepare("DELETE FROM bundle_members WHERE file_id = ?1")?;
    let mut removed = 0;
    for id in file_ids {
        removed += leave.execute([id])?;
    }
    drop(leave);
    prune_bundles(conn)?;
    Ok(removed)
}

/// Deletes the bundle and its memberships. No file is touched.
pub fn dissolve_bundle(conn: &Connection, bundle_id: i64) -> Result<(), DbError> {
    bundle_exists(conn, bundle_id)?;
    conn.execute("DELETE FROM bundles WHERE id = ?1", [bundle_id])?;
    Ok(())
}

/// Removes every bundle (catalog reset).
pub fn clear_bundles(conn: &Connection) -> Result<(), DbError> {
    conn.execute("DELETE FROM bundles", [])?;
    Ok(())
}

pub fn update_bundle(conn: &Connection, bundle_id: i64, name: Option<&str>, kind: Option<BundleKind>) -> Result<(), DbError> {
    let name = name.map(validate_bundle_name).transpose()?;
    bundle_exists(conn, bundle_id)?;
    if let Some(name) = name {
        conn.execute("UPDATE bundles SET name = ?1 WHERE id = ?2", params![name, bundle_id])?;
    }
    if let Some(kind) = kind {
        conn.execute("UPDATE bundles SET kind = ?1 WHERE id = ?2", params![kind.as_str(), bundle_id])?;
    }
    Ok(())
}

/// Sets the label of a member (an empty text clears it) and/or makes it the title model by
/// swapping positions with the current one.
pub fn update_bundle_member(
    conn: &Connection,
    file_id: i64,
    label: Option<&str>,
    make_title: bool,
) -> Result<(), DbError> {
    let label = label.map(validate_bundle_label).transpose()?;
    let membership: Option<(i64, i64)> = conn
        .query_row(
            "SELECT bundle_id, position FROM bundle_members WHERE file_id = ?1",
            [file_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((bundle_id, position)) = membership else {
        return Err(DbError::coded(ErrorCode::EntryNotFound));
    };
    if let Some(label) = label {
        conn.execute("UPDATE bundle_members SET label = ?1 WHERE file_id = ?2", params![label, file_id])?;
    }
    if make_title {
        let (title_id, title_position): (i64, i64) = conn.query_row(
            "SELECT file_id, position FROM bundle_members WHERE bundle_id = ?1 ORDER BY position, file_id LIMIT 1",
            [bundle_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if title_id != file_id {
            conn.execute("UPDATE bundle_members SET position = ?1 WHERE file_id = ?2", params![position, title_id])?;
            conn.execute("UPDATE bundle_members SET position = ?1 WHERE file_id = ?2", params![title_position, file_id])?;
        }
    }
    Ok(())
}

fn load_bundles(conn: &Connection, only: Option<i64>) -> Result<Vec<BundleRecord>, DbError> {
    // Bundles with fewer than two member rows are leftovers waiting to be pruned: not shown.
    let mut stmt = conn.prepare(
        "SELECT b.id, b.name, b.kind, b.created_at, m.file_id, m.position, m.label, f.deleted_at IS NOT NULL
         FROM bundles b
         JOIN bundle_members m ON m.bundle_id = b.id
         JOIN files f ON f.id = m.file_id
         WHERE (?1 IS NULL OR b.id = ?1)
           AND (SELECT COUNT(*) FROM bundle_members c WHERE c.bundle_id = b.id) >= 2
         ORDER BY b.id, m.position, m.file_id",
    )?;
    let mut rows = stmt.query([only])?;
    let mut bundles: Vec<BundleRecord> = Vec::new();
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        if bundles.last().map(|b| b.id) != Some(id) {
            let kind: String = row.get(2)?;
            bundles.push(BundleRecord {
                id,
                name: row.get(1)?,
                kind: BundleKind::parse(&kind).ok_or_else(|| DbError::Other(format!("unknown bundle kind {kind}")))?,
                created_at: row.get(3)?,
                members: Vec::new(),
            });
        }
        if let Some(bundle) = bundles.last_mut() {
            bundle.members.push(BundleMemberRecord {
                file_id: row.get(4)?,
                position: row.get(5)?,
                label: row.get(6)?,
                in_trash: row.get(7)?,
            });
        }
    }
    Ok(bundles)
}

/// All visible bundles (two or more member rows, trashed members included), oldest first.
pub fn list_bundles(conn: &Connection) -> Result<Vec<BundleRecord>, DbError> {
    load_bundles(conn, None)
}

pub fn get_bundle(conn: &Connection, bundle_id: i64) -> Result<Option<BundleRecord>, DbError> {
    Ok(load_bundles(conn, Some(bundle_id))?.pop())
}

#[cfg(test)]
#[path = "bundles_tests.rs"]
mod tests;
