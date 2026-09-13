use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use super::error::DbError;
use super::models::{
    FileRecord, FileType, FilamentSpoolRecord, FolderRecord, MaterialRecord,
    NewFile, NewFilamentSpool, NewPrintLogEntry, NewSavedFilter, PrintLogEntryRecord,
    SavedFilterRecord, TagCount, CreatorCount,
};

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

pub(crate) fn init(conn: &Connection) -> Result<(), DbError> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.execute_batch(SCHEMA_SQL)?;
    // filament_spools.image_png wurde nachtraeglich zur bereits bestehenden
    // Tabelle hinzugefuegt (kein Migrations-Framework in diesem Projekt) -
    // CREATE TABLE IF NOT EXISTS aendert eine schon vorhandene Tabelle nicht.
    // ALTER TABLE laeuft daher hier zusaetzlich und wird bewusst ignoriert,
    // falls die Spalte (auf einer frisch angelegten DB, wo CREATE TABLE sie
    // schon mitbringt) bereits existiert.
    let _ = conn.execute("ALTER TABLE filament_spools ADD COLUMN image_png BLOB", []);
    let _ = conn.execute("ALTER TABLE filament_spools ADD COLUMN location TEXT", []);
    // Gleiches Muster fuer vier neue files-Spalten (Druckstatus, Zuletzt-
    // angesehen, Creator, Inhalts-Hash) auf einer bereits befuellten
    // Produktions-DB.
    let _ = conn.execute(
        "ALTER TABLE files ADD COLUMN print_status TEXT NOT NULL DEFAULT 'not_printed'
            CHECK (print_status IN ('not_printed', 'printed'))",
        [],
    );
    let _ = conn.execute("ALTER TABLE files ADD COLUMN last_viewed_at TEXT", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN creator TEXT", []);
    // Backfill fuer Bestandsdaten: 'creator' wurde erst mit obiger ALTER TABLE
    // eingefuehrt und wird sonst nur beim Import gesetzt (import_one). Ohne
    // diesen Backfill bleibt 'creator' fuer jede vor diesem Upgrade bereits
    // importierte Datei fuer immer NULL, obwohl der Designer-Wert laengst in
    // file_metadata steht. Laeuft bei jedem Start, ist aber billig und
    // idempotent: WHERE creator IS NULL schliesst bereits befuellte Zeilen
    // bei kuenftigen Starts automatisch aus.
    let _ = conn.execute(
        "UPDATE files SET creator = (
             SELECT value FROM file_metadata
             WHERE file_id = files.id AND label = 'Designer'
         ) WHERE creator IS NULL",
        [],
    );
    let _ = conn.execute("ALTER TABLE files ADD COLUMN content_hash TEXT", []);
    // Index fuer content_hash wird hier ebenfalls als Migrations-Zeile hinzugefuegt,
    // NACH der ALTER TABLE, da es von der Spalte abhaengt. Auf frischen DBs ist die
    // Spalte bereits vorhanden (via CREATE TABLE), also ist diese Zeile hier auch auf
    // frischen DBs ein no-op.
    let _ = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files (content_hash)",
        [],
    );
    let _ = conn.execute("ALTER TABLE files ADD COLUMN render_snapshot_png BLOB", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN custom_image_png BLOB", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN source_url TEXT", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN queue_position INTEGER", []);
    let _ = conn.execute(
        "ALTER TABLE files ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute("ALTER TABLE files ADD COLUMN plate_count INTEGER", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN slice_info_json TEXT", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN deleted_at TEXT", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN trash_path TEXT", []);
    Ok(())
}

// Nur von Tests genutzt: es gibt aktuell keinen Command, der Ordner manuell
// anlegt (files.folder_id wird beim Import nie gesetzt, siehe commands.rs).
// list_folders() liest die Tabelle trotzdem aus, daher hier nur unter Test
// gehalten statt geloescht, um Testdaten fuer diese Abfrage anzulegen.
#[cfg(test)]
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
        "SELECT t.name, t.color_hue, COUNT(f.id)
         FROM tags t
         LEFT JOIN file_tags ft ON ft.tag_id = t.id
         LEFT JOIN files f ON f.id = ft.file_id AND f.deleted_at IS NULL
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

pub fn list_creator_counts(conn: &Connection) -> Result<Vec<CreatorCount>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT creator, COUNT(*) FROM files WHERE creator IS NOT NULL AND deleted_at IS NULL GROUP BY creator ORDER BY creator",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CreatorCount {
                name: row.get(0)?,
                count: row.get(1)?,
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
            volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
            print_status, last_viewed_at, creator, content_hash,
            render_snapshot_png, custom_image_png, source_url, queue_position, favorite,
            plate_count, slice_info_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)",
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
            file.print_status,
            file.last_viewed_at,
            file.creator,
            file.content_hash,
            file.render_snapshot_png,
            file.custom_image_png,
            file.source_url,
            file.queue_position,
            file.favorite,
            file.plate_count,
            file.slice_info_json,
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

pub fn update_scanned_metadata(
    conn: &mut Connection,
    file_id: i64,
    update: &super::models::ScannedMetadataUpdate,
) -> Result<(), DbError> {
    let tx = conn.transaction()?;

    let [dim_x, dim_y, dim_z] = match update.dimensions_mm {
        Some(d) => [Some(d[0]), Some(d[1]), Some(d[2])],
        None => [None, None, None],
    };

    tx.execute(
        "UPDATE files SET
            dimension_x_mm = ?1, dimension_y_mm = ?2, dimension_z_mm = ?3,
            volume_cm3 = ?4, object_count = ?5, thumbnail_png = ?6,
            plate_count = ?7, slice_info_json = ?8, file_size_bytes = ?9, content_hash = ?10
         WHERE id = ?11",
        params![
            dim_x, dim_y, dim_z, update.volume_cm3, update.object_count,
            update.thumbnail_png, update.plate_count, update.slice_info_json,
            update.file_size_bytes, update.content_hash, file_id,
        ],
    )?;

    tx.execute("DELETE FROM file_materials WHERE file_id = ?1", params![file_id])?;
    for material in &update.materials {
        tx.execute(
            "INSERT INTO file_materials (file_id, name, display_color) VALUES (?1, ?2, ?3)",
            params![file_id, material.name, material.display_color],
        )?;
    }

    tx.execute("DELETE FROM file_metadata WHERE file_id = ?1", params![file_id])?;
    for (label, value) in &update.metadata {
        tx.execute(
            "INSERT INTO file_metadata (file_id, label, value) VALUES (?1, ?2, ?3)",
            params![file_id, label, value],
        )?;
    }

    tx.commit()?;
    Ok(())
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

pub fn soft_delete_file(
    conn: &Connection,
    id: i64,
    trash_path: Option<&str>,
    deleted_at: &str,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET deleted_at = ?1, trash_path = ?2 WHERE id = ?3",
        params![deleted_at, trash_path, id],
    )?;
    Ok(())
}

pub fn restore_file(conn: &Connection, id: i64, new_path: Option<&str>) -> Result<(), DbError> {
    match new_path {
        Some(path) => {
            conn.execute(
                "UPDATE files SET deleted_at = NULL, trash_path = NULL, path = ?1 WHERE id = ?2",
                params![path, id],
            )?;
        }
        None => {
            conn.execute(
                "UPDATE files SET deleted_at = NULL, trash_path = NULL WHERE id = ?1",
                params![id],
            )?;
        }
    }
    Ok(())
}

const TRASH_SELECT_COLUMNS: &str = "id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
     file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
     volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
     print_status, last_viewed_at, creator, content_hash,
     render_snapshot_png, custom_image_png, source_url, queue_position, favorite,
     plate_count, slice_info_json, deleted_at, trash_path";

pub fn list_trash(conn: &Connection) -> Result<Vec<FileRecord>, DbError> {
    let sql = format!("SELECT {TRASH_SELECT_COLUMNS} FROM files WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC");
    let mut stmt = conn.prepare(&sql)?;
    let mut files = stmt.query_map([], row_to_file)?.collect::<Result<Vec<_>, _>>()?;
    for file in &mut files {
        file.materials = load_materials(conn, file.id)?;
        file.metadata = load_metadata(conn, file.id)?;
        file.tags = load_tags(conn, file.id)?;
    }
    Ok(files)
}

pub fn purge_expired_trash(conn: &Connection, older_than: &str) -> Result<Vec<FileRecord>, DbError> {
    let sql = format!("SELECT {TRASH_SELECT_COLUMNS} FROM files WHERE deleted_at IS NOT NULL AND deleted_at < ?1");
    let mut stmt = conn.prepare(&sql)?;
    let files = stmt.query_map(params![older_than], row_to_file)?.collect::<Result<Vec<_>, _>>()?;
    Ok(files)
}

pub fn file_exists_by_path(conn: &Connection, path: &str) -> Result<bool, DbError> {
    let exists: Option<i64> = conn
        .query_row("SELECT 1 FROM files WHERE path = ?1 AND deleted_at IS NULL", params![path], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(exists.is_some())
}

pub fn get_file_id_by_content_hash(conn: &Connection, hash: &str) -> Result<Option<i64>, DbError> {
    Ok(conn
        .query_row(
            "SELECT id FROM files WHERE content_hash = ?1 AND deleted_at IS NULL",
            params![hash],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn file_exists_by_hash(conn: &Connection, hash: &str) -> Result<bool, DbError> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM files WHERE content_hash = ?1 AND deleted_at IS NULL",
            params![hash],
            |row| row.get(0),
        )
        .optional()?;
    Ok(exists.is_some())
}

pub fn get_file(conn: &Connection, id: i64) -> Result<Option<FileRecord>, DbError> {
    let row = conn
        .query_row(
            "SELECT id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
                    file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
                    volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
                    print_status, last_viewed_at, creator, content_hash,
                    render_snapshot_png, custom_image_png, source_url, queue_position, favorite,
                    plate_count, slice_info_json, deleted_at, trash_path
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

/// Laedt mehrere Dateien anhand ihrer IDs in EINER Hauptabfrage statt einer
/// pro ID (wie es ein wiederholter get_file-Aufruf taete) - genutzt von
/// list_collection_files, wo eine Sammlung aus vielen Dateien bestehen kann.
/// Materials/Metadata/Tags werden weiterhin pro Datei nachgeladen (gleiches
/// Muster wie list_files/get_file), das war nicht der eigentliche N+1-Teil.
pub fn list_files_by_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<FileRecord>, DbError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
                file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
                volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
                print_status, last_viewed_at, creator, content_hash,
                render_snapshot_png, custom_image_png, source_url, queue_position, favorite,
                plate_count, slice_info_json, deleted_at, trash_path
         FROM files WHERE id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let params_vec: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
    let mut files = stmt
        .query_map(params_vec.as_slice(), row_to_file)?
        .collect::<Result<Vec<_>, _>>()?;

    for file in &mut files {
        file.materials = load_materials(conn, file.id)?;
        file.metadata = load_metadata(conn, file.id)?;
        file.tags = load_tags(conn, file.id)?;
    }

    // Reihenfolge der uebergebenen ids wiederherstellen - SQL "IN" garantiert
    // keine bestimmte Ergebnisreihenfolge, die Sammlungs-Position haengt aber
    // davon ab.
    let mut by_id: std::collections::HashMap<i64, FileRecord> =
        files.into_iter().map(|f| (f.id, f)).collect();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

pub fn list_files(conn: &Connection) -> Result<Vec<FileRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
                file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
                volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
                print_status, last_viewed_at, creator, content_hash,
                render_snapshot_png, custom_image_png, source_url, queue_position, favorite,
                plate_count, slice_info_json, deleted_at, trash_path
         FROM files WHERE deleted_at IS NULL ORDER BY name",
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
        print_status: row.get(17)?,
        last_viewed_at: row.get(18)?,
        creator: row.get(19)?,
        content_hash: row.get(20)?,
        render_snapshot_png: row.get(21)?,
        custom_image_png: row.get(22)?,
        source_url: row.get(23)?,
        queue_position: row.get(24)?,
        favorite: row.get(25)?,
        plate_count: row.get(26)?,
        slice_info_json: row.get(27)?,
        deleted_at: row.get(28)?,
        trash_path: row.get(29)?,
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

pub fn set_print_status(conn: &Connection, file_id: i64, status: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET print_status = ?1,
                queue_position = CASE WHEN ?1 = 'printed' THEN NULL ELSE queue_position END
         WHERE id = ?2",
        params![status, file_id],
    )?;
    Ok(())
}

pub fn set_favorite(conn: &Connection, file_id: i64, favorite: bool) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET favorite = ?1 WHERE id = ?2",
        params![favorite, file_id],
    )?;
    Ok(())
}

pub fn set_queue_position(conn: &Connection, file_id: i64, position: Option<i64>) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET queue_position = ?1 WHERE id = ?2",
        params![position, file_id],
    )?;
    Ok(())
}

pub fn max_queue_position(conn: &Connection) -> Result<Option<i64>, DbError> {
    Ok(conn.query_row("SELECT MAX(queue_position) FROM files WHERE deleted_at IS NULL", [], |row| row.get(0))?)
}

/// Liefert (id, path) fuer alle Dateien ohne content_hash - Grundlage fuer
/// den einmaligen Startup-Backfill in commands::backfill_content_hashes, der
/// fuer Bestandsdaten den Hash nachtraeglich per Datei-I/O berechnet. Bewusst
/// minimal (kein FileRecord), da nur diese zwei Felder gebraucht werden.
pub fn list_files_missing_content_hash(conn: &Connection) -> Result<Vec<(i64, String)>, DbError> {
    let mut stmt = conn.prepare("SELECT id, path FROM files WHERE content_hash IS NULL AND deleted_at IS NULL")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set_content_hash(conn: &Connection, file_id: i64, hash: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET content_hash = ?1 WHERE id = ?2",
        params![hash, file_id],
    )?;
    Ok(())
}

pub fn set_custom_image_png(conn: &Connection, file_id: i64, png: &[u8]) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET custom_image_png = ?1 WHERE id = ?2",
        params![png, file_id],
    )?;
    Ok(())
}

pub fn set_render_snapshot_png(conn: &Connection, file_id: i64, png: &[u8]) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET render_snapshot_png = ?1 WHERE id = ?2",
        params![png, file_id],
    )?;
    Ok(())
}

pub fn set_source_url(conn: &Connection, file_id: i64, url: Option<&str>) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET source_url = ?1 WHERE id = ?2",
        params![url, file_id],
    )?;
    Ok(())
}

pub fn mark_file_viewed(conn: &Connection, file_id: i64) -> Result<(), DbError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE files SET last_viewed_at = ?1 WHERE id = ?2",
        params![now, file_id],
    )?;
    Ok(())
}

pub fn insert_filament_spool(conn: &Connection, spool: &NewFilamentSpool) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO filament_spools
            (material, manufacturer, color, location, diameter_mm, original_weight_g, remaining_weight_g, price, image_png, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            spool.material,
            spool.manufacturer,
            spool.color,
            spool.location,
            spool.diameter_mm,
            spool.original_weight_g,
            spool.remaining_weight_g,
            spool.price,
            spool.image_png,
            chrono::Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_filament_spools(conn: &Connection) -> Result<Vec<FilamentSpoolRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, material, manufacturer, color, location, diameter_mm, original_weight_g, remaining_weight_g, price, image_png
         FROM filament_spools ORDER BY material, manufacturer",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(FilamentSpoolRecord {
                id: row.get(0)?,
                material: row.get(1)?,
                manufacturer: row.get(2)?,
                color: row.get(3)?,
                location: row.get(4)?,
                diameter_mm: row.get(5)?,
                original_weight_g: row.get(6)?,
                remaining_weight_g: row.get(7)?,
                price: row.get(8)?,
                image_png: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn update_filament_spool(conn: &Connection, id: i64, spool: &NewFilamentSpool) -> Result<(), DbError> {
    conn.execute(
        "UPDATE filament_spools
         SET material = ?1, manufacturer = ?2, color = ?3, location = ?4, diameter_mm = ?5,
             original_weight_g = ?6, remaining_weight_g = ?7, price = ?8, image_png = ?9
         WHERE id = ?10",
        params![
            spool.material,
            spool.manufacturer,
            spool.color,
            spool.location,
            spool.diameter_mm,
            spool.original_weight_g,
            spool.remaining_weight_g,
            spool.price,
            spool.image_png,
            id,
        ],
    )?;
    Ok(())
}

pub fn delete_filament_spool(conn: &Connection, id: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM filament_spools WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn insert_saved_filter(conn: &Connection, filter: &NewSavedFilter) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO saved_filters (name, folder_id, tag, creator, query, sort, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            filter.name,
            filter.folder_id,
            filter.tag,
            filter.creator,
            filter.query,
            filter.sort,
            chrono::Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_saved_filters(conn: &Connection) -> Result<Vec<SavedFilterRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, folder_id, tag, creator, query, sort, created_at
         FROM saved_filters ORDER BY created_at",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SavedFilterRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                folder_id: row.get(2)?,
                tag: row.get(3)?,
                creator: row.get(4)?,
                query: row.get(5)?,
                sort: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn delete_saved_filter(conn: &Connection, id: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM saved_filters WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn insert_print_log_entry(conn: &Connection, entry: &NewPrintLogEntry) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO print_log (file_id, printed_at, note, photo_png, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            entry.file_id,
            entry.printed_at,
            entry.note,
            entry.photo_png,
            chrono::Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_print_log_entries(conn: &Connection, file_id: i64) -> Result<Vec<PrintLogEntryRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, printed_at, note, photo_png FROM print_log
         WHERE file_id = ?1 ORDER BY printed_at DESC, id DESC",
    )?;
    let rows = stmt
        .query_map(params![file_id], |row| {
            Ok(PrintLogEntryRecord {
                id: row.get(0)?,
                printed_at: row.get(1)?,
                note: row.get(2)?,
                photo_png: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn delete_print_log_entry(conn: &Connection, id: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM print_log WHERE id = ?1", params![id])?;
    Ok(())
}
