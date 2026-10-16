//! Database-only metadata commands. Desktop functions retain their historical contract;
//! `web` adds transactional admission and strict input validation.
mod error;
pub mod web;
use crate::read::{print_log_entry_to_dto, CollectionDto, PrintLogEntryDto};
use crate::{db, tagging};
pub use error::WriteError;
pub use rusqlite::Connection;
pub type WriteResult<T> = Result<T, WriteError>;
pub const MAX_CUSTOM_IMAGE_BYTES: usize = 5 * 1024 * 1024;
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuePositionUpdate {
    pub file_id: String,
    pub position: i64,
}
pub type CollectionPositionUpdate = QueuePositionUpdate;

pub fn add_print_log_entry(
    conn: &Connection,
    file_id: String,
    printed_at: String,
    note: Option<String>,
    photo_base64: Option<String>,
) -> WriteResult<PrintLogEntryDto> {
    let entry = prepare_print_log_entry(file_id, printed_at, note, photo_base64)?;
    insert_prepared_print_log_entry(conn, entry)
}
pub fn prepare_print_log_entry(
    file_id: String,
    printed_at: String,
    note: Option<String>,
    photo_base64: Option<String>,
) -> WriteResult<db::models::NewPrintLogEntry> {
    use base64::Engine;
    let fid: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;

    let photo_png = photo_base64
        .map(|b64| {
            base64::engine::general_purpose::STANDARD
                .decode(&b64)
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    if let Some(bytes) = &photo_png {
        if bytes.len() > MAX_CUSTOM_IMAGE_BYTES {
            return Err(format!(
                "Bild ist zu groß ({:.1} MB) - maximal {} MB erlaubt",
                bytes.len() as f64 / (1024.0 * 1024.0),
                MAX_CUSTOM_IMAGE_BYTES / (1024 * 1024)
            )
            .into());
        }
    }

    Ok(db::models::NewPrintLogEntry {
        file_id: fid,
        printed_at,
        note,
        photo_png,
    })
}
pub fn insert_prepared_print_log_entry(
    conn: &Connection,
    new_entry: db::models::NewPrintLogEntry,
) -> WriteResult<PrintLogEntryDto> {
    let fid = new_entry.file_id;
    let id = db::insert_print_log_entry(conn, &new_entry).map_err(|e| e.to_string())?;
    let entries = db::list_print_log_entries(conn, fid).map_err(|e| e.to_string())?;
    let entry = entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| "entry not found after insert".to_string())?;
    Ok(print_log_entry_to_dto(entry))
}

pub fn delete_print_log_entry(conn: &Connection, entry_id: String) -> WriteResult<()> {
    let id: i64 = entry_id
        .parse()
        .map_err(|_| "invalid entry id".to_string())?;
    db::delete_print_log_entry(conn, id).map_err(|e| e.to_string().into())
}

pub fn remove_tag(conn: &Connection, file_id: String, tag: String) -> WriteResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    // No normalization here (unlike add_tag): the frontend always sends the name
    // actually stored - normalizing would make an alias tag (e.g. the ambiguous
    // "mini") impossible to remove once it no longer matches the key.
    db::remove_tag_from_file(conn, id, &tag).map_err(|e| e.to_string().into())
}

pub fn set_print_status(conn: &Connection, file_id: String, status: String) -> WriteResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    db::set_print_status(conn, id, &status).map_err(|e| e.to_string().into())
}

pub fn set_favorite(conn: &Connection, file_id: String, favorite: bool) -> WriteResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    db::set_favorite(conn, id, favorite).map_err(|e| e.to_string().into())
}

pub fn mark_file_viewed(conn: &Connection, file_id: String) -> WriteResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    db::mark_file_viewed(conn, id).map_err(|e| e.to_string().into())
}

pub fn add_to_queue(conn: &Connection, file_id: String) -> WriteResult<i64> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let next = db::max_queue_position(conn)
        .map_err(|e| e.to_string())?
        .unwrap_or(0)
        + 1;
    db::set_queue_position(conn, id, Some(next)).map_err(|e| e.to_string())?;
    Ok(next)
}

pub fn remove_from_queue(conn: &Connection, file_id: String) -> WriteResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    db::set_queue_position(conn, id, None).map_err(|e| e.to_string().into())
}

pub fn set_source_url(conn: &Connection, file_id: String, url: Option<String>) -> WriteResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let url = validate_source_url(url)?;
    db::set_source_url(conn, id, url.as_deref()).map_err(|e| e.to_string().into())
}

pub fn set_license(
    conn: &mut Connection,
    file_ids: Vec<String>,
    license: Option<String>,
) -> WriteResult<()> {
    let ids = file_ids
        .iter()
        .map(|id| id.parse::<i64>().map_err(|_| "invalid file id".to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    db::licenses::set_license(conn, &ids, license.as_deref()).map_err(|e| e.to_string().into())
}

pub fn add_tag_with_conn(conn: &Connection, file_id: i64, tag: &str) -> WriteResult<()> {
    db::error::validate_text_length(tag, 100, "Tag")
        .map_err(|e| WriteError::expected(e.to_string()))?;
    db::add_tag_to_file(conn, file_id, &tagging::canonical_tag(tag)).map_err(|e| match e {
        db::error::DbError::Invalid(message) => WriteError::expected(message),
        other => other.to_string().into(),
    })
}

pub fn reorder_queue_with_conn(
    conn: &mut Connection,
    updates: Vec<QueuePositionUpdate>,
) -> WriteResult<()> {
    let mut parsed = Vec::with_capacity(updates.len());
    for update in updates {
        let id: i64 = update
            .file_id
            .parse()
            .map_err(|_| "invalid file id".to_string())?;
        if db::get_file(conn, id).map_err(|e| e.to_string())?.is_none() {
            return Err(
                format!("Datei mit id {id} nicht gefunden - Batch wird nicht angewendet").into(),
            );
        }
        parsed.push((id, update.position));
    }

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for (id, position) in parsed {
        db::set_queue_position(&tx, id, Some(position)).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn create_collection(conn: &Connection, name: String) -> WriteResult<CollectionDto> {
    let created_at = chrono::Utc::now().to_rfc3339();
    let id = db::create_collection(conn, &name, &created_at)?;
    Ok(CollectionDto {
        id: id.to_string(),
        name,
        model_count: 0,
    })
}

pub fn rename_collection(
    conn: &Connection,
    collection_id: String,
    name: String,
) -> WriteResult<()> {
    let id: i64 = collection_id
        .parse()
        .map_err(|_| "invalid collection id".to_string())?;
    db::rename_collection(conn, id, &name)?;
    Ok(())
}

pub fn delete_collection(conn: &Connection, collection_id: String) -> WriteResult<()> {
    let id: i64 = collection_id
        .parse()
        .map_err(|_| "invalid collection id".to_string())?;
    db::delete_collection(conn, id).map_err(|e| e.to_string().into())
}

pub fn add_files_to_collection(
    conn: &Connection,
    collection_id: String,
    file_ids: Vec<String>,
) -> WriteResult<()> {
    let cid: i64 = collection_id
        .parse()
        .map_err(|_| "invalid collection id".to_string())?;
    let start = db::max_collection_position(conn, cid)
        .map_err(|e| e.to_string())?
        .unwrap_or(-1)
        + 1;
    for (next, file_id) in (start..).zip(file_ids) {
        let fid: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
        db::add_file_to_collection(conn, cid, fid, next).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn remove_file_from_collection(
    conn: &Connection,
    collection_id: String,
    file_id: String,
) -> WriteResult<()> {
    let cid: i64 = collection_id
        .parse()
        .map_err(|_| "invalid collection id".to_string())?;
    let fid: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    db::remove_file_from_collection(conn, cid, fid).map_err(|e| e.to_string().into())
}

pub fn reorder_collection_with_conn(
    conn: &mut Connection,
    collection_id: i64,
    updates: Vec<CollectionPositionUpdate>,
) -> WriteResult<()> {
    let member_ids =
        db::list_collection_file_ids(conn, collection_id).map_err(|e| e.to_string())?;

    let mut parsed = Vec::with_capacity(updates.len());
    for update in updates {
        let fid: i64 = update
            .file_id
            .parse()
            .map_err(|_| "invalid file id".to_string())?;
        if !member_ids.contains(&fid) {
            return Err(format!(
                "Datei mit id {fid} ist nicht Teil der Collection {collection_id} - Batch wird nicht angewendet"
            )
            .into());
        }
        parsed.push((fid, update.position));
    }

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for (fid, position) in parsed {
        db::set_collection_position(&tx, collection_id, fid, position)
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn validate_source_url(url: Option<String>) -> WriteResult<Option<String>> {
    let Some(trimmed) = url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty()) else {
        return Ok(None);
    };
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(Some(trimmed))
    } else {
        Err(WriteError::expected(
            "source URL must start with http:// or https://",
        ))
    }
}

/// Match the desktop image picker: PNG, JPEG, or WebP signatures.
pub fn validate_image_format(bytes: &[u8]) -> WriteResult<()> {
    let supported = bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"\xff\xd8\xff")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(&b"WEBP"[..]));
    if supported {
        Ok(())
    } else {
        Err(WriteError::expected("imageUploadUnsupported"))
    }
}

pub mod manager;
pub mod printers;
mod web_manager;
#[cfg(test)]
mod web_manager_tests;
