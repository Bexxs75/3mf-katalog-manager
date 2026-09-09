use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use super::error::DbError;
use super::models::{CloudAccountRecord, FileRecord, FileType, FolderRecord, MaterialRecord, NewFile, TagCount};

const SCHEMA_SQL: &str = include_str!("schema.sql");

pub fn connect(path: &Path) -> Result<Connection, DbError> {
    let conn = Connection::open(path)?;
    init(&conn)?;
    Ok(conn)
}

#[allow(dead_code)]
pub fn connect_in_memory() -> Result<Connection, DbError> {
    let conn = Connection::open_in_memory()?;
    init(&conn)?;
    Ok(conn)
}

fn init(conn: &Connection) -> Result<(), DbError> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(())
}

pub fn insert_folder(conn: &Connection, name: &str) -> Result<i64, DbError> {
    conn.execute("INSERT INTO folders (name) VALUES (?1)", params![name])?;
    Ok(conn.last_insert_rowid())
}

pub fn list_folders(conn: &Connection) -> Result<Vec<FolderRecord>, DbError> {
    let mut stmt = conn.prepare("SELECT id, name FROM folders ORDER BY name")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(FolderRecord {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Deterministic hue in [0, 360) derived from the tag name, so a tag keeps
/// the same color across sessions without persisting a user choice for it.
fn hue_for_tag(name: &str) -> i64 {
    let mut hash: u32 = 2166136261;
    for b in name.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(16777619);
    }
    (hash % 360) as i64
}

fn get_or_create_tag(conn: &Connection, name: &str) -> Result<i64, DbError> {
    let existing: Option<i64> = conn
        .query_row("SELECT id FROM tags WHERE name = ?1", params![name], |row| {
            row.get(0)
        })
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO tags (name, color_hue) VALUES (?1, ?2)",
        params![name, hue_for_tag(name)],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn add_tag_to_file(conn: &Connection, file_id: i64, tag_name: &str) -> Result<(), DbError> {
    let tag_id = get_or_create_tag(conn, tag_name)?;
    conn.execute(
        "INSERT OR IGNORE INTO file_tags (file_id, tag_id) VALUES (?1, ?2)",
        params![file_id, tag_id],
    )?;
    Ok(())
}

pub fn remove_tag_from_file(conn: &Connection, file_id: i64, tag_name: &str) -> Result<(), DbError> {
    let tag_id: Option<i64> = conn
        .query_row("SELECT id FROM tags WHERE name = ?1", params![tag_name], |row| {
            row.get(0)
        })
        .optional()?;

    conn.execute(
        "DELETE FROM file_tags
         WHERE file_id = ?1 AND tag_id = (SELECT id FROM tags WHERE name = ?2)",
        params![file_id, tag_name],
    )?;

    if let Some(tag_id) = tag_id {
        delete_tag_if_unused(conn, tag_id)?;
    }
    Ok(())
}

/// Loescht einen Tag, wenn ihm nach einer Aenderung keine Datei mehr
/// zugeordnet ist - verhindert verwaiste, sinnfreie Tags (z.B. aus geloeschten
/// Dateien) im Sidebar-Tag-Filter.
fn delete_tag_if_unused(conn: &Connection, tag_id: i64) -> Result<(), DbError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM file_tags WHERE tag_id = ?1",
        params![tag_id],
        |row| row.get(0),
    )?;
    if count == 0 {
        conn.execute("DELETE FROM tags WHERE id = ?1", params![tag_id])?;
    }
    Ok(())
}

/// Entfernt alle Tags, die aktuell keiner Datei zugeordnet sind. Wird beim
/// App-Start aufgerufen, um bereits vorhandene verwaiste Tags aus frueheren
/// Sitzungen (vor dieser Aufraeum-Logik) zu bereinigen.
pub fn delete_unused_tags(conn: &Connection) -> Result<usize, DbError> {
    Ok(conn.execute(
        "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM file_tags)",
        [],
    )?)
}

pub fn list_tag_counts(conn: &Connection) -> Result<Vec<TagCount>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT t.name, t.color_hue, COUNT(ft.file_id)
         FROM tags t
         LEFT JOIN file_tags ft ON ft.tag_id = t.id
         GROUP BY t.id
         ORDER BY t.name",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(TagCount {
                name: row.get(0)?,
                color_hue: row.get(1)?,
                count: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn insert_file(conn: &mut Connection, file: &NewFile) -> Result<i64, DbError> {
    let tx = conn.transaction()?;

    let [dim_x, dim_y, dim_z] = match file.dimensions_mm {
        Some(d) => [Some(d[0]), Some(d[1]), Some(d[2])],
        None => [None, None, None],
    };

    tx.execute(
        "INSERT INTO files (
            name, path, file_type, folder_id, origin, cloud_id, sync_status,
            file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
            volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            file.name,
            file.path,
            file.file_type.as_str(),
            file.folder_id,
            file.origin,
            file.cloud_id,
            file.sync_status,
            file.file_size_bytes,
            dim_x,
            dim_y,
            dim_z,
            file.volume_cm3,
            file.object_count,
            file.thumbnail_png,
            file.imported_at,
            file.file_modified_at,
        ],
    )?;
    let file_id = tx.last_insert_rowid();

    for material in &file.materials {
        tx.execute(
            "INSERT INTO file_materials (file_id, name, display_color) VALUES (?1, ?2, ?3)",
            params![file_id, material.name, material.display_color],
        )?;
    }

    for (label, value) in &file.metadata {
        tx.execute(
            "INSERT INTO file_metadata (file_id, label, value) VALUES (?1, ?2, ?3)",
            params![file_id, label, value],
        )?;
    }

    for tag_name in &file.tags {
        let tag_id = get_or_create_tag(&tx, tag_name)?;
        tx.execute(
            "INSERT OR IGNORE INTO file_tags (file_id, tag_id) VALUES (?1, ?2)",
            params![file_id, tag_id],
        )?;
    }

    tx.commit()?;
    Ok(file_id)
}

pub fn delete_file(conn: &Connection, id: i64) -> Result<(), DbError> {
    // Tag-IDs vorher merken, da file_tags per ON DELETE CASCADE mitgeloescht
    // wird und danach nicht mehr bekannt ist, welche Tags betroffen waren.
    let mut stmt = conn.prepare("SELECT tag_id FROM file_tags WHERE file_id = ?1")?;
    let tag_ids: Vec<i64> = stmt
        .query_map(params![id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    conn.execute("DELETE FROM files WHERE id = ?1", params![id])?;

    for tag_id in tag_ids {
        delete_tag_if_unused(conn, tag_id)?;
    }
    Ok(())
}

pub fn file_exists_by_path(conn: &Connection, path: &str) -> Result<bool, DbError> {
    let exists: Option<i64> = conn
        .query_row("SELECT 1 FROM files WHERE path = ?1", params![path], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(exists.is_some())
}

pub fn get_file(conn: &Connection, id: i64) -> Result<Option<FileRecord>, DbError> {
    let row = conn
        .query_row(
            "SELECT id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
                    file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
                    volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at
             FROM files WHERE id = ?1",
            params![id],
            row_to_file,
        )
        .optional()?;

    let Some(mut file) = row else {
        return Ok(None);
    };
    file.materials = load_materials(conn, id)?;
    file.metadata = load_metadata(conn, id)?;
    file.tags = load_tags(conn, id)?;
    Ok(Some(file))
}

pub fn list_files(conn: &Connection) -> Result<Vec<FileRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
                file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
                volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at
         FROM files ORDER BY name",
    )?;
    let mut files = stmt
        .query_map([], row_to_file)?
        .collect::<Result<Vec<_>, _>>()?;

    for file in &mut files {
        file.materials = load_materials(conn, file.id)?;
        file.metadata = load_metadata(conn, file.id)?;
        file.tags = load_tags(conn, file.id)?;
    }
    Ok(files)
}

fn row_to_file(row: &rusqlite::Row) -> rusqlite::Result<FileRecord> {
    let file_type_str: String = row.get(3)?;
    let dim_x: Option<f64> = row.get(9)?;
    let dim_y: Option<f64> = row.get(10)?;
    let dim_z: Option<f64> = row.get(11)?;
    let dimensions_mm = match (dim_x, dim_y, dim_z) {
        (Some(x), Some(y), Some(z)) => Some([x, y, z]),
        _ => None,
    };

    Ok(FileRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        file_type: FileType::parse(&file_type_str).unwrap_or(FileType::ThreeMf),
        folder_id: row.get(4)?,
        origin: row.get(5)?,
        sync_status: row.get(6)?,
        cloud_id: row.get(7)?,
        file_size_bytes: row.get(8)?,
        dimensions_mm,
        volume_cm3: row.get(12)?,
        object_count: row.get(13)?,
        thumbnail_png: row.get(14)?,
        imported_at: row.get(15)?,
        file_modified_at: row.get(16)?,
        materials: Vec::new(),
        metadata: BTreeMap::new(),
        tags: Vec::new(),
    })
}

fn load_materials(conn: &Connection, file_id: i64) -> Result<Vec<MaterialRecord>, DbError> {
    let mut stmt =
        conn.prepare("SELECT name, display_color FROM file_materials WHERE file_id = ?1")?;
    let rows = stmt
        .query_map(params![file_id], |row| {
            Ok(MaterialRecord {
                name: row.get(0)?,
                display_color: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_metadata(conn: &Connection, file_id: i64) -> Result<BTreeMap<String, String>, DbError> {
    let mut stmt = conn.prepare("SELECT label, value FROM file_metadata WHERE file_id = ?1")?;
    let rows = stmt
        .query_map(params![file_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    Ok(rows)
}

fn load_tags(conn: &Connection, file_id: i64) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM tags t
         JOIN file_tags ft ON ft.tag_id = t.id
         WHERE ft.file_id = ?1
         ORDER BY t.name",
    )?;
    let rows = stmt
        .query_map(params![file_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn upsert_cloud_account(
    conn: &Connection,
    provider: &str,
    account_label: &str,
    connected_at: &str,
) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO cloud_accounts (provider, account_label, status, connected_at)
         VALUES (?1, ?2, 'connected', ?3)
         ON CONFLICT(provider) DO UPDATE SET
             account_label = excluded.account_label,
             status = 'connected',
             connected_at = excluded.connected_at",
        params![provider, account_label, connected_at],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM cloud_accounts WHERE provider = ?1",
        params![provider],
        |row| row.get(0),
    )?)
}

pub fn list_cloud_accounts(conn: &Connection) -> Result<Vec<CloudAccountRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, account_label, status, connected_at FROM cloud_accounts ORDER BY provider",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CloudAccountRecord {
            id: row.get(0)?,
            provider: row.get(1)?,
            account_label: row.get(2)?,
            status: row.get(3)?,
            connected_at: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn set_cloud_account_status(conn: &Connection, provider: &str, status: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE cloud_accounts SET status = ?1 WHERE provider = ?2",
        params![status, provider],
    )?;
    Ok(())
}

pub fn set_file_sync_status(conn: &Connection, file_id: i64, status: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET sync_status = ?1 WHERE id = ?2",
        params![status, file_id],
    )?;
    Ok(())
}

pub fn set_file_modified_at(conn: &Connection, file_id: i64, modified_at: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET file_modified_at = ?1 WHERE id = ?2",
        params![modified_at, file_id],
    )?;
    Ok(())
}
