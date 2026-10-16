//! Strict operations. Checks and mutations share an IMMEDIATE transaction.
use super::{Connection, WriteError, WriteResult, MAX_CUSTOM_IMAGE_BYTES};
use crate::{db, read};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
#[derive(Debug)]
pub struct Outcome<T> {
    pub value: T,
    pub changes: u64,
}
pub(super) fn outcome<T>(value: T, changes: u64) -> Outcome<T> {
    Outcome { value, changes }
}
pub(super) fn invalid(message: &str) -> WriteError {
    WriteError::expected(message)
}
fn missing() -> WriteError {
    WriteError::NotFound {
        message: "Eintrag nicht gefunden".into(),
    }
}
pub(super) fn transaction<T>(
    c: &mut Connection,
    f: impl FnOnce(&Connection) -> WriteResult<Outcome<T>>,
) -> WriteResult<Outcome<T>> {
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = f(&tx)?;
    tx.commit()?;
    Ok(result)
}
pub(super) fn exists(c: &Connection, table: &str, id: i64) -> WriteResult<()> {
    let sql = match table {
        "files" => "SELECT 1 FROM files WHERE id=?1 AND deleted_at IS NULL",
        "all_files" => "SELECT 1 FROM files WHERE id=?1",
        "collections" => "SELECT 1 FROM collections WHERE id=?1",
        "print_log" => "SELECT 1 FROM print_log WHERE id=?1",
        "filament_spools" => "SELECT 1 FROM filament_spools WHERE id=?1",
        "printers" => "SELECT 1 FROM printers WHERE id=?1",
        "material_units" => "SELECT 1 FROM material_units WHERE id=?1",
        _ => unreachable!(),
    };
    if c.query_row(sql, [id], |_| Ok(())).optional()?.is_none() {
        return Err(missing());
    }
    Ok(())
}
fn nonempty(value: &str) -> WriteResult<()> {
    if value.trim().is_empty() {
        return Err(invalid("Text darf nicht leer sein"));
    }
    Ok(())
}
fn length(value: &str, max: usize) -> WriteResult<()> {
    if value.chars().count() > max {
        return Err(invalid("Text ist zu lang"));
    }
    Ok(())
}
fn count(ids: usize) -> WriteResult<()> {
    if ids > 500 {
        return Err(invalid("Höchstens 500 IDs erlaubt"));
    }
    Ok(())
}
fn file_update(
    c: &mut Connection,
    id: i64,
    f: impl FnOnce(&Connection) -> Result<(), db::error::DbError>,
) -> WriteResult<Outcome<()>> {
    transaction(c, |c| {
        exists(c, "files", id)?;
        f(c)?;
        Ok(outcome((), c.changes()))
    })
}
pub fn set_favorite(c: &mut Connection, id: i64, favorite: bool) -> WriteResult<Outcome<()>> {
    file_update(c, id, |c| db::set_favorite(c, id, favorite))
}
pub fn set_print_status(c: &mut Connection, id: i64, status: &str) -> WriteResult<Outcome<()>> {
    if !matches!(status, "not_printed" | "printed") {
        return Err(invalid("Ungültiger Druckstatus"));
    }
    file_update(c, id, |c| db::set_print_status(c, id, status))
}
pub fn mark_file_viewed(c: &mut Connection, id: i64) -> WriteResult<Outcome<()>> {
    file_update(c, id, |c| db::mark_file_viewed(c, id))
}
pub fn remove_from_queue(c: &mut Connection, id: i64) -> WriteResult<Outcome<()>> {
    transaction(c, |c| {
        exists(c, "all_files", id)?;
        db::set_queue_position(c, id, None)?;
        Ok(outcome((), c.changes()))
    })
}
pub fn add_to_queue(c: &mut Connection, id: i64) -> WriteResult<Outcome<i64>> {
    transaction(c, |c| {
        exists(c, "files", id)?;
        let current: Option<i64> =
            c.query_row("SELECT queue_position FROM files WHERE id=?1", [id], |r| {
                r.get(0)
            })?;
        if let Some(position) = current {
            return Ok(outcome(position, 0));
        }
        let next = db::max_queue_position(c)?
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid("Position zu groß"))?;
        db::set_queue_position(c, id, Some(next))?;
        Ok(outcome(next, c.changes()))
    })
}
pub fn add_tag(c: &mut Connection, id: i64, tag: &str) -> WriteResult<Outcome<()>> {
    nonempty(tag)?;
    db::error::validate_text_length(tag, 100, "Tag")?;
    file_update(c, id, |c| {
        db::add_tag_to_file(c, id, &crate::tagging::canonical_tag(tag))
    })
}
pub fn remove_tag(c: &mut Connection, id: i64, tag: &str) -> WriteResult<Outcome<()>> {
    nonempty(tag)?;
    transaction(c, |c| {
        exists(c, "files", id)?;
        c.execute("DELETE FROM file_tags WHERE file_id=?1 AND tag_id IN (SELECT id FROM tags WHERE name=?2)", params![id,tag])?;
        let changes = c.changes();
        c.execute(
            "DELETE FROM tags WHERE name=?1 AND id NOT IN (SELECT tag_id FROM file_tags)",
            [tag],
        )?;
        Ok(outcome((), changes))
    })
}
pub fn set_source_url(
    c: &mut Connection,
    id: i64,
    url: Option<String>,
) -> WriteResult<Outcome<()>> {
    if url
        .as_ref()
        .is_some_and(|u| u.chars().any(char::is_control))
    {
        return Err(invalid("Ungültige Quell-URL"));
    }
    let url = url.map(|u| u.trim().to_owned()).filter(|u| !u.is_empty());
    if let Some(url) = &url {
        length(url, 2048)?;
        let scheme = url.split_once("://").map(|(scheme, _)| scheme);
        if !scheme
            .is_some_and(|s| s.eq_ignore_ascii_case("http") || s.eq_ignore_ascii_case("https"))
        {
            return Err(invalid("Ungültige Quell-URL"));
        }
    }
    file_update(c, id, |c| db::set_source_url(c, id, url.as_deref()))
}
/// The existing license operation owns its transaction and checks every UPDATE.
/// Do not wrap it: its rollback guarantee covers races after these prechecks too.
pub fn set_license(
    c: &mut Connection,
    ids: &[i64],
    license: Option<&str>,
) -> WriteResult<Outcome<()>> {
    count(ids.len())?;
    db::licenses::validate_license(license).map_err(|e| invalid(&e.to_string()))?;
    for &id in ids {
        exists(c, "files", id)?;
    }
    db::licenses::set_license(c, ids, license)?;
    // Each successful main UPDATE affects exactly one row (enforced by licenses).
    Ok(outcome((), ids.len() as u64))
}
pub fn add_print_log_entry(
    c: &mut Connection,
    id: i64,
    printed_at: String,
    note: Option<String>,
    photo_base64: Option<String>,
) -> WriteResult<Outcome<read::PrintLogEntryDto>> {
    use base64::Engine;
    length(&printed_at, 64)?;
    if chrono::DateTime::parse_from_rfc3339(&printed_at).is_err() {
        return Err(invalid("Ungültiger Druckzeitpunkt"));
    }
    if let Some(note) = &note {
        length(note, 4000)?;
    }
    let photo_png = photo_base64
        .map(|s| {
            base64::engine::general_purpose::STANDARD
                .decode(s)
                .map_err(|_| invalid("Ungültiges Foto"))
        })
        .transpose()?;
    if photo_png
        .as_ref()
        .is_some_and(|p| p.len() > MAX_CUSTOM_IMAGE_BYTES)
    {
        return Err(invalid("Foto ist zu groß"));
    }
    if let Some(photo) = &photo_png {
        super::validate_image_format(photo)?;
    }
    transaction(c, |c| {
        exists(c, "files", id)?;
        let entry = db::models::NewPrintLogEntry {
            file_id: id,
            printed_at,
            note,
            photo_png,
        };
        let key = db::insert_print_log_entry(c, &entry)?;
        let changes = c.changes();
        let entry = db::list_print_log_entries(c, id)?
            .into_iter()
            .find(|e| e.id == key)
            .ok_or_else(missing)?;
        Ok(outcome(read::print_log_entry_to_dto(entry), changes))
    })
}
pub fn delete_print_log_entry(c: &mut Connection, id: i64) -> WriteResult<Outcome<()>> {
    transaction(c, |c| {
        exists(c, "print_log", id)?;
        db::delete_print_log_entry(c, id)?;
        Ok(outcome((), c.changes()))
    })
}
pub fn create_collection(
    c: &mut Connection,
    name: &str,
) -> WriteResult<Outcome<read::CollectionDto>> {
    nonempty(name)?;
    length(name, 120)?;
    transaction(c, |c| {
        let id = db::create_collection(c, name, &chrono::Utc::now().to_rfc3339())?;
        Ok(outcome(
            read::CollectionDto {
                id: id.to_string(),
                name: name.into(),
                model_count: 0,
            },
            c.changes(),
        ))
    })
}
pub fn rename_collection(c: &mut Connection, id: i64, name: &str) -> WriteResult<Outcome<()>> {
    nonempty(name)?;
    length(name, 120)?;
    transaction(c, |c| {
        exists(c, "collections", id)?;
        db::rename_collection(c, id, name)?;
        Ok(outcome((), c.changes()))
    })
}
pub fn delete_collection(c: &mut Connection, id: i64) -> WriteResult<Outcome<()>> {
    transaction(c, |c| {
        exists(c, "collections", id)?;
        db::delete_collection(c, id)?;
        Ok(outcome((), c.changes()))
    })
}
pub fn add_files_to_collection(
    c: &mut Connection,
    cid: i64,
    ids: &[i64],
) -> WriteResult<Outcome<()>> {
    count(ids.len())?;
    transaction(c, |c| {
        exists(c, "collections", cid)?;
        for &id in ids {
            exists(c, "files", id)?;
        }
        let mut next = db::max_collection_position(c, cid)?.unwrap_or(-1);
        let mut changes = 0;
        for &id in ids {
            next = next
                .checked_add(1)
                .ok_or_else(|| invalid("Position zu groß"))?;
            db::add_file_to_collection(c, cid, id, next)?;
            changes += c.changes();
        }
        Ok(outcome((), changes))
    })
}
pub fn remove_file_from_collection(
    c: &mut Connection,
    cid: i64,
    id: i64,
) -> WriteResult<Outcome<()>> {
    transaction(c, |c| {
        exists(c, "collections", cid)?;
        exists(c, "all_files", id)?;
        db::remove_file_from_collection(c, cid, id)?;
        Ok(outcome((), c.changes()))
    })
}
fn reorder(
    c: &mut Connection,
    cid: Option<i64>,
    updates: &[(i64, i64)],
) -> WriteResult<Outcome<()>> {
    count(updates.len())?;
    transaction(c, |c| {
        if let Some(cid) = cid {
            exists(c, "collections", cid)?;
        }
        for &(id, _) in updates {
            let member: bool = if let Some(cid) = cid {
                c.query_row("SELECT EXISTS(SELECT 1 FROM collection_files JOIN files ON files.id=file_id WHERE collection_id=?1 AND file_id=?2 AND deleted_at IS NULL)", params![cid,id], |r| r.get(0))?
            } else {
                c.query_row("SELECT EXISTS(SELECT 1 FROM files WHERE id=?1 AND queue_position IS NOT NULL AND deleted_at IS NULL)", [id], |r| r.get(0))?
            };
            if !member {
                return Err(invalid("Datei ist kein Mitglied der Liste"));
            }
        }
        let mut changes = 0;
        for &(id, position) in updates {
            if let Some(cid) = cid {
                db::set_collection_position(c, cid, id, position)?;
            } else {
                db::set_queue_position(c, id, Some(position))?;
            }
            changes += c.changes();
        }
        Ok(outcome((), changes))
    })
}
pub fn reorder_queue(c: &mut Connection, updates: &[(i64, i64)]) -> WriteResult<Outcome<()>> {
    reorder(c, None, updates)
}
pub fn reorder_collection(
    c: &mut Connection,
    cid: i64,
    updates: &[(i64, i64)],
) -> WriteResult<Outcome<()>> {
    reorder(c, Some(cid), updates)
}
#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;

pub use super::web_manager::*;

/// Maximum padded Base64 encoding of a permitted snapshot, checked before decoding.
pub const MAX_RENDER_SNAPSHOT_BASE64_BYTES: usize = MAX_CUSTOM_IMAGE_BYTES.div_ceil(3) * 4;

pub fn set_render_snapshot(
    c: &mut Connection,
    id: i64,
    image_base64: &str,
) -> WriteResult<Outcome<()>> {
    use base64::Engine;
    transaction(c, |c| {
        exists(c, "files", id)?;
        if image_base64.len() > MAX_RENDER_SNAPSHOT_BASE64_BYTES {
            return Err(invalid("Snapshot-Base64 ist zu groß"));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(image_base64)
            .map_err(|_| invalid("Ungültiges Snapshot-Base64"))?;
        if bytes.len() > MAX_CUSTOM_IMAGE_BYTES {
            return Err(invalid("Snapshot ist zu groß"));
        }
        super::validate_image_format(&bytes)?;
        db::set_render_snapshot_png(c, id, &bytes)?;
        Ok(outcome((), c.changes()))
    })
}

/// Remove catalog records only; filesystem ownership remains with the upload journal.
pub fn remove_files_from_catalog(c: &mut Connection, ids: &[i64]) -> WriteResult<Outcome<usize>> {
    if ids.len() > 200 {
        return Err(invalid("Höchstens 200 IDs erlaubt"));
    }
    if ids.iter().any(|id| *id <= 0) {
        return Err(invalid("Ungültige Katalog-ID."));
    }
    let ids: std::collections::BTreeSet<_> = ids.iter().copied().collect();
    transaction(c, |c| {
        for id in &ids {
            let active: bool = c.query_row(
                "SELECT EXISTS(SELECT 1 FROM files WHERE id=?1 AND deleted_at IS NULL)",
                [id], |r| r.get(0),
            )?;
            if !active {
                return Err(WriteError::NotFound { message: "Modell nicht gefunden oder bereits im Papierkorb.".into() });
            }
        }
        let mut changes = 0;
        for id in &ids {
            changes += c.execute("DELETE FROM files WHERE id=?1", [id])?;
        }
        Ok(outcome(changes, changes as u64))
    })
}
