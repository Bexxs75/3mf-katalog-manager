use std::collections::BTreeMap;
use std::path::{Component, Path};

use rusqlite::{params, Connection, OptionalExtension};

use super::error::DbError;
use super::models::{
    FileRecord, FileType, FilamentSpoolRecord, FolderRecord, MaterialRecord,
    NewFile, NewFilamentSpool, NewPrintLogEntry, NewSavedFilter, PrintLogEntryRecord,
    RegisteredSlicer, SavedFilterRecord, TagCount, CreatorCount,
};

pub const SCHEMA_SQL: &str = include_str!("schema.sql");

pub fn connect(path: &Path) -> Result<Connection, DbError> {
    let mut conn = Connection::open(path)?;
    init(&mut conn)?;
    Ok(conn)
}

#[allow(dead_code)]
pub fn connect_in_memory() -> Result<Connection, DbError> {
    let mut conn = Connection::open_in_memory()?;
    init(&mut conn)?;
    Ok(conn)
}

pub(crate) fn init(conn: &mut Connection) -> Result<(), DbError> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.execute_batch(SCHEMA_SQL)?;
    super::migrations::run_migrations(conn)?;
    // Kein Abbruch: ein Fehler hier darf den Start nicht verhindern, die
    // Transaktion in merge_auto_tag_aliases rollt dann zurueck.
    if let Err(e) = merge_auto_tag_aliases(conn) {
        eprintln!("[tags] Zusammenlegen der Namen automatischer Tags fehlgeschlagen: {e}");
    }
    Ok(())
}

// Nur fuer Tests: minimale files-Zeile (Name aus dem letzten Pfadsegment),
// ohne den vollen `insert_file`-Weg.
#[cfg(test)]
pub fn test_insert_minimal_file(conn: &Connection, path: &str, folder_id: Option<i64>) -> Result<i64, DbError> {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());
    conn.execute(
        "INSERT INTO files (name, path, file_type, folder_id, file_size_bytes, imported_at)
         VALUES (?1, ?2, '3mf', ?3, 0, '2026-01-01T00:00:00Z')",
        params![name, path, folder_id],
    )?;
    Ok(conn.last_insert_rowid())
}

// Nur fuer Tests: folders-Zeile ohne parent_id und ohne echtes Verzeichnis.
#[cfg(test)]
pub fn insert_folder(conn: &Connection, name: &str) -> Result<i64, DbError> {
    // Synthetischer Pfad, weil `path` in Rust nicht optional ist.
    conn.execute(
        "INSERT INTO folders (name, path) VALUES (?1, ?1)",
        params![name],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_folders(conn: &Connection) -> Result<Vec<FolderRecord>, DbError> {
    let mut stmt = conn.prepare("SELECT id, name, parent_id, path FROM folders ORDER BY name")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(FolderRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                path: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Legt fuer jede Verzeichnisebene von `import_root` bis `dir` (beide inklusive)
/// einen folders-Eintrag an, falls er fehlt (idempotent ueber `path`). Gibt die
/// id von `dir` zurueck.
pub fn ensure_folder_path(conn: &Connection, import_root: &Path, dir: &Path) -> Result<i64, DbError> {
    let relative = dir.strip_prefix(import_root).map_err(|_| {
        DbError::Other(format!(
            "{} liegt nicht unter {}",
            dir.display(),
            import_root.display()
        ))
    })?;

    let mut current_path = import_root.to_path_buf();
    let mut parent_id: Option<i64> = None;
    parent_id = Some(find_or_insert_folder(conn, &current_path, parent_id, folder_name(&current_path))?);

    for component in relative.components() {
        if let Component::Normal(part) = component {
            current_path.push(part);
            parent_id = Some(find_or_insert_folder(
                conn,
                &current_path,
                parent_id,
                part.to_string_lossy().to_string(),
            )?);
        }
    }

    Ok(parent_id.expect("mindestens import_root wurde oben eingefuegt"))
}

/// Haengt einen bisher als Wurzel angelegten Ordner (`parent_id IS NULL`)
/// unter den katalogisierten Ordner, dessen `path` dem Elternverzeichnis
/// entspricht. Noetig nach dem Entpacken eines Archivs: `ensure_folder_path`
/// legt das Import-Wurzelverzeichnis immer als Wurzel an, auch wenn sein
/// Elternverzeichnis bereits ein Katalogordner ist. Ohne katalogisierten
/// Elternordner oder bei bereits eingehaengten Ordnern passiert nichts.
pub fn attach_folder_to_parent_by_path(conn: &Connection, dir: &Path) -> Result<(), DbError> {
    let Some(parent) = dir.parent() else {
        return Ok(());
    };
    conn.execute(
        "UPDATE folders SET parent_id = (SELECT id FROM folders WHERE path = ?2)
         WHERE path = ?1 AND parent_id IS NULL
           AND EXISTS (SELECT 1 FROM folders WHERE path = ?2)",
        params![dir.to_string_lossy().to_string(), parent.to_string_lossy().to_string()],
    )?;
    Ok(())
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// Legt IMMER eine neue Zeile an, anders als `find_or_insert_folder`. Einen
/// schon existierenden Zielpfad faengt `create_dir` vorher mit `AlreadyExists` ab.
pub fn insert_folder_with_parent(
    conn: &Connection,
    name: &str,
    parent_id: Option<i64>,
    path: &str,
) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO folders (name, parent_id, path) VALUES (?1, ?2, ?3)",
        params![name, parent_id, path],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Neuer Eintrag in der maschinenlokalen Slicer-Registry. `executable_path` ist
/// UNIQUE; die Autoerkennung prueft deshalb vorher gegen `list_registered_slicers`.
pub fn insert_registered_slicer(
    conn: &Connection,
    name: &str,
    executable_path: &str,
    is_auto_detected: bool,
) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO registered_slicers (name, executable_path, is_auto_detected) VALUES (?1, ?2, ?3)",
        params![name, executable_path, is_auto_detected],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_registered_slicer(conn: &Connection, id: i64) -> Result<Option<RegisteredSlicer>, DbError> {
    conn.query_row(
        "SELECT id, name, executable_path, is_auto_detected FROM registered_slicers WHERE id = ?1",
        params![id],
        |row| {
            Ok(RegisteredSlicer {
                id: row.get(0)?,
                name: row.get(1)?,
                executable_path: row.get(2)?,
                is_auto_detected: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(DbError::from)
}

pub fn list_registered_slicers(conn: &Connection) -> Result<Vec<RegisteredSlicer>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, executable_path, is_auto_detected FROM registered_slicers ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(RegisteredSlicer {
                id: row.get(0)?,
                name: row.get(1)?,
                executable_path: row.get(2)?,
                is_auto_detected: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Aktualisiert `folder_id` und `path` nach einem physischen Verschieben.
pub fn update_file_folder(conn: &Connection, file_id: i64, folder_id: Option<i64>, path: &str) -> Result<(), DbError> {
    let affected = conn.execute(
        "UPDATE files SET folder_id = ?1, path = ?2 WHERE id = ?3",
        params![folder_id, path, file_id],
    )?;
    if affected == 0 {
        return Err(DbError::Other(format!("keine Datei mit id {file_id} gefunden fuer update_file_folder")));
    }
    Ok(())
}

/// Aktualisiert `name` und `path` nach einem physischen Umbenennen.
pub fn rename_file(conn: &Connection, file_id: i64, name: &str, path: &str) -> Result<(), DbError> {
    let affected = conn.execute(
        "UPDATE files SET name = ?1, path = ?2 WHERE id = ?3",
        params![name, path, file_id],
    )?;
    if affected == 0 {
        return Err(DbError::Other(format!("keine Datei mit id {file_id} gefunden fuer rename_file")));
    }
    Ok(())
}

/// Aktualisiert nur `name`; Umbenennen auf der Platte und Pfad-Update
/// (`update_paths_under_folder`) passieren getrennt.
pub fn rename_folder_name(conn: &Connection, folder_id: i64, name: &str) -> Result<(), DbError> {
    conn.execute("UPDATE folders SET name = ?1 WHERE id = ?2", params![name, folder_id])?;
    Ok(())
}

/// Aktualisiert nur `parent_id`; Verschieben auf der Platte und Pfad-Update
/// (`update_paths_under_folder`) passieren getrennt.
pub fn set_folder_parent(conn: &Connection, folder_id: i64, parent_id: Option<i64>) -> Result<(), DbError> {
    conn.execute("UPDATE folders SET parent_id = ?1 WHERE id = ?2", params![parent_id, folder_id])?;
    Ok(())
}

/// Zieht `folders.path` und `files.path` unterhalb von `folder_id` per
/// Praefix-Ersetzung nach, nachdem sich dessen Pfad von `old_path` zu
/// `new_path` geaendert hat.
pub fn update_paths_under_folder(
    conn: &Connection,
    folder_id: i64,
    old_path: &str,
    new_path: &str,
) -> Result<(), DbError> {
    conn.execute("UPDATE folders SET path = ?1 WHERE id = ?2", params![new_path, folder_id])?;
    // length()/substr() zaehlen in SQLite Zeichen, nicht Bytes. Deshalb auch die
    // Laenge von SQLite berechnen lassen, sonst stimmt der Offset bei Umlauten nicht.
    conn.execute(
        "UPDATE files SET path = ?1 || substr(path, length(?2) + 1) WHERE folder_id = ?3",
        params![new_path, old_path, folder_id],
    )?;

    let mut stmt = conn.prepare("SELECT id, path FROM folders WHERE parent_id = ?1")?;
    let children: Vec<(i64, String)> = stmt
        .query_map(params![folder_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    for (child_id, child_old_path) in children {
        // Kein Byte-Slicing: passt die Hierarchie nicht zu den Pfaden (praepariertes
        // Backup), wuerde das paniken und mit panic = "abort" die App beenden.
        let Some(suffix) = child_old_path.strip_prefix(old_path) else {
            return Err(DbError::Other(format!(
                "Ordner {child_id} ({child_old_path}) liegt nicht unter {old_path}"
            )));
        };
        let child_new_path = format!("{new_path}{suffix}");
        update_paths_under_folder(conn, child_id, &child_old_path, &child_new_path)?;
    }
    Ok(())
}

fn find_or_insert_folder(
    conn: &Connection,
    path: &Path,
    parent_id: Option<i64>,
    name: String,
) -> Result<i64, DbError> {
    let path_str = path.to_string_lossy().to_string();
    if let Some(id) = conn
        .query_row("SELECT id FROM folders WHERE path = ?1", params![path_str], |r| r.get(0))
        .optional()?
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO folders (name, parent_id, path) VALUES (?1, ?2, ?3)",
        params![name, parent_id, path_str],
    )?;
    Ok(conn.last_insert_rowid())
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

/// Legt Tags, die in irgendeiner Sprache wie ein automatischer Tag heissen
/// (z.B. "Multipart"), mit der deutschen Kennung zusammen. Laeuft bei jedem
/// Oeffnen der DB, idempotent und in einer Transaktion.
pub fn merge_auto_tag_aliases(conn: &mut Connection) -> Result<usize, DbError> {
    let tx = conn.transaction()?;
    let tags: Vec<(i64, String)> = {
        let mut stmt = tx.prepare("SELECT id, name FROM tags")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut merged = 0;
    for (alias_id, name) in tags {
        // Mehrdeutige Aliase ("mini" wie in "Bambu A1 mini") werden beim Start
        // bewusst nicht zusammengelegt.
        let canonical = crate::tagging::canonical_tag_unambiguous(&name);
        if canonical == name {
            continue;
        }
        let canonical_id = get_or_create_tag(&tx, &canonical)?;
        tx.execute(
            "INSERT OR IGNORE INTO file_tags (file_id, tag_id)
             SELECT file_id, ?1 FROM file_tags WHERE tag_id = ?2",
            params![canonical_id, alias_id],
        )?;
        tx.execute("DELETE FROM tags WHERE id = ?1", params![alias_id])?;
        tx.execute(
            "UPDATE saved_filters SET tag = ?1 WHERE tag = ?2",
            params![canonical, name],
        )?;
        merged += 1;
    }
    tx.commit()?;
    Ok(merged)
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

/// Loescht einen Tag, wenn ihm keine Datei mehr zugeordnet ist.
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

/// Entfernt beim Start alle Tags ohne Datei.
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

// Nur fuer Tests; der Import nutzt `insert_file_within_tx` in einer Batch-Transaktion.
#[cfg(test)]
pub fn insert_file(conn: &mut Connection, file: &NewFile) -> Result<i64, DbError> {
    let tx = conn.transaction()?;
    let id = insert_file_within_tx(&tx, file)?;
    tx.commit()?;
    Ok(id)
}

/// Core of [`insert_file`] on an already open transaction, so a batch import
/// commits once instead of once per file (SQLite fsyncs on every commit).
pub fn insert_file_within_tx(conn: &Connection, file: &NewFile) -> Result<i64, DbError> {
    let [dim_x, dim_y, dim_z] = match file.dimensions_mm {
        Some(d) => [Some(d[0]), Some(d[1]), Some(d[2])],
        None => [None, None, None],
    };

    conn.execute(
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
    let file_id = conn.last_insert_rowid();

    for material in &file.materials {
        conn.execute(
            "INSERT INTO file_materials (file_id, name, display_color) VALUES (?1, ?2, ?3)",
            params![file_id, material.name, material.display_color],
        )?;
    }

    for (label, value) in &file.metadata {
        conn.execute(
            "INSERT INTO file_metadata (file_id, label, value) VALUES (?1, ?2, ?3)",
            params![file_id, label, value],
        )?;
    }

    for tag_name in &file.tags {
        let tag_id = get_or_create_tag(conn, tag_name)?;
        conn.execute(
            "INSERT OR IGNORE INTO file_tags (file_id, tag_id) VALUES (?1, ?2)",
            params![file_id, tag_id],
        )?;
    }

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
    let affected = conn.execute(
        "UPDATE files SET deleted_at = ?1, trash_path = ?2 WHERE id = ?3",
        params![deleted_at, trash_path, id],
    )?;
    if affected == 0 {
        return Err(DbError::Other(format!("keine Datei mit id {id} gefunden fuer soft_delete_file")));
    }
    Ok(())
}

pub fn restore_file(conn: &Connection, id: i64, new_path: Option<&str>) -> Result<(), DbError> {
    let affected = match new_path {
        Some(path) => conn.execute(
            "UPDATE files SET deleted_at = NULL, trash_path = NULL, path = ?1 WHERE id = ?2",
            params![path, id],
        )?,
        None => conn.execute(
            "UPDATE files SET deleted_at = NULL, trash_path = NULL WHERE id = ?1",
            params![id],
        )?,
    };
    if affected == 0 {
        return Err(DbError::Other(format!("keine Datei mit id {id} gefunden fuer restore_file")));
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

/// Laedt mehrere Dateien mit EINER Hauptabfrage statt einer pro id (z.B. fuer
/// grosse Sammlungen).
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

    // SQL "IN" garantiert keine Reihenfolge; die Sammlungs-Position haengt aber davon ab.
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

/// Schlanke Projektion von `files` fuer Grid und Liste: ohne
/// `custom_image_png` und ohne Materialien, Metadaten und Tags (sonst N+1
/// Abfragen). `thumbnail_png` und `render_snapshot_png` bleiben drin, das Grid
/// braucht ein Bild pro Zeile; der Snapshot ist im Schnitt sogar kleiner
/// (~9 KB gegenueber ~54 KB).
pub struct FileSummary {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub file_type: FileType,
    pub folder_id: Option<i64>,
    pub file_size_bytes: i64,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub imported_at: String,
    pub print_status: String,
    pub favorite: bool,
    pub queue_position: Option<i64>,
    pub thumbnail_png: Option<Vec<u8>>,
    pub render_snapshot_png: Option<Vec<u8>>,
    // Redundant zu `render_snapshot_png`, wird im Frontend aber noch fuer die Snapshot-Warteschlange genutzt.
    pub has_render_snapshot: bool,
    // Fuer den Creator-Filter der Seitenleiste.
    pub creator: Option<String>,
    // Fuer "Zuletzt angesehen", "Duplikate" und die Sortierung 'viewed'.
    pub last_viewed_at: Option<String>,
    pub content_hash: Option<String>,
}

pub fn list_file_summaries(conn: &Connection) -> Result<Vec<FileSummary>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, path, file_type, folder_id, file_size_bytes,
                dimension_x_mm, dimension_y_mm, dimension_z_mm, volume_cm3,
                object_count, imported_at, print_status, favorite,
                queue_position, thumbnail_png, render_snapshot_png,
                render_snapshot_png IS NOT NULL AS has_render_snapshot, creator,
                last_viewed_at, content_hash
         FROM files WHERE deleted_at IS NULL ORDER BY name",
    )?;
    let rows = stmt
        .query_map([], |row| {
            let dx: Option<f64> = row.get(6)?;
            let dy: Option<f64> = row.get(7)?;
            let dz: Option<f64> = row.get(8)?;
            let file_type_str: String = row.get(3)?;
            Ok(FileSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                file_type: FileType::parse(&file_type_str)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(3, "file_type".to_string(), rusqlite::types::Type::Text))?,
                folder_id: row.get(4)?,
                file_size_bytes: row.get(5)?,
                dimensions_mm: match (dx, dy, dz) {
                    (Some(x), Some(y), Some(z)) => Some([x, y, z]),
                    _ => None,
                },
                volume_cm3: row.get(9)?,
                object_count: row.get(10)?,
                imported_at: row.get(11)?,
                print_status: row.get(12)?,
                favorite: row.get::<_, i64>(13)? != 0,
                queue_position: row.get(14)?,
                thumbnail_png: row.get(15)?,
                render_snapshot_png: row.get(16)?,
                has_render_snapshot: row.get::<_, i64>(17)? != 0,
                creator: row.get(18)?,
                last_viewed_at: row.get(19)?,
                content_hash: row.get(20)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Alle Datei-Tag-Zuordnungen in EINER Abfrage, fuer die Tag-Filterung (die
/// Summaries enthalten keine Tags).
pub fn list_all_file_tags(conn: &Connection) -> Result<Vec<(i64, String)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT file_tags.file_id, tags.name
         FROM file_tags
         JOIN tags ON tags.id = file_tags.tag_id
         ORDER BY file_tags.file_id, tags.name",
    )?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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
        file_type: FileType::parse(&file_type_str)
            .ok_or_else(|| rusqlite::Error::InvalidColumnType(3, "file_type".to_string(), rusqlite::types::Type::Text))?,
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

/// (id, path) aller Dateien ohne content_hash, fuer `backfill_content_hashes`.
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
            (material, manufacturer, color, location, diameter_mm, original_weight_g, remaining_weight_g, price, image_png, created_at, color_hex, kind)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            spool.material,
            spool.manufacturer,
            spool.color,
            spool.location,
            spool.diameter_mm,
            crate::db::printers::round_tenth(spool.original_weight_g),
            crate::db::printers::round_tenth(spool.remaining_weight_g),
            spool.price,
            spool.image_png,
            chrono::Utc::now().to_rfc3339(),
            spool.color_hex,
            spool.kind,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_filament_spools(conn: &Connection) -> Result<Vec<FilamentSpoolRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, material, manufacturer, color, location, diameter_mm, original_weight_g, remaining_weight_g, price, image_png,
                color_hex, home_location, unit_id, slot_index, kind
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
                color_hex: row.get(10)?,
                home_location: row.get(11)?,
                unit_id: row.get(12)?,
                slot_index: row.get(13)?,
                kind: row.get(14)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Liest eine Spule inklusive Fach, damit `update_filament_spool` den echten DB-Stand zurueckgibt.
pub fn get_filament_spool(conn: &Connection, id: i64) -> Result<FilamentSpoolRecord, DbError> {
    conn.query_row(
        "SELECT id, material, manufacturer, color, location, diameter_mm, original_weight_g, remaining_weight_g, price, image_png,
                color_hex, home_location, unit_id, slot_index, kind
         FROM filament_spools WHERE id = ?1",
        params![id],
        |row| {
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
                color_hex: row.get(10)?,
                home_location: row.get(11)?,
                unit_id: row.get(12)?,
                slot_index: row.get(13)?,
                kind: row.get(14)?,
            })
        },
    )
    .map_err(DbError::from)
}

/// Aendert nie das Fach einer Spule (das tun nur `load_spool`/`unload_spool`
/// in `db::printers`). Steckt die Spule in einem Fach, ist der uebergebene
/// Lagerort ihr Stammplatz und landet in `home_location`; `location` bleibt
/// dann leer.
pub fn update_filament_spool(conn: &Connection, id: i64, spool: &NewFilamentSpool) -> Result<(), DbError> {
    // Ein eingelegter Eintrag behaelt seine Art: sonst laege Resin in einem
    // Filament-Fach oder Filament in einer Harzwanne.
    let loaded_kind: Option<String> = conn
        .query_row(
            "SELECT kind FROM filament_spools WHERE id = ?1 AND unit_id IS NOT NULL",
            params![id],
            |r| r.get(0),
        )
        .optional()?;
    if loaded_kind.is_some_and(|k| k != spool.kind) {
        return Err(DbError::Other(
            "Die Art eines Eintrags im Drucker kann nicht geaendert werden - erst herausnehmen".to_string(),
        ));
    }
    conn.execute(
        "UPDATE filament_spools
         SET material = ?1, manufacturer = ?2, color = ?3,
             location = CASE WHEN unit_id IS NULL THEN ?4 ELSE location END,
             home_location = CASE WHEN unit_id IS NULL THEN home_location ELSE ?4 END,
             diameter_mm = ?5, original_weight_g = ?6, remaining_weight_g = ?7, price = ?8, image_png = ?9,
             color_hex = ?11, kind = ?12
         WHERE id = ?10",
        params![
            spool.material,
            spool.manufacturer,
            spool.color,
            spool.location,
            spool.diameter_mm,
            crate::db::printers::round_tenth(spool.original_weight_g),
            crate::db::printers::round_tenth(spool.remaining_weight_g),
            spool.price,
            spool.image_png,
            id,
            spool.color_hex,
            spool.kind,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_file_summaries_includes_render_snapshot_but_omits_custom_image() {
        let conn = connect_in_memory().unwrap();
        let file_id = test_insert_minimal_file(&conn, "/tmp/x.3mf", None).unwrap();
        conn.execute(
            "UPDATE files SET render_snapshot_png = ?1, custom_image_png = ?2 WHERE id = ?3",
            params![vec![1u8; 1024], vec![2u8; 1024], file_id],
        ).unwrap();

        let summaries = list_file_summaries(&conn).unwrap();

        assert_eq!(summaries.len(), 1);
        // Der Snapshot muss mitkommen, sonst zeigt das Grid ihn erst nach dem Oeffnen des Modells.
        assert_eq!(summaries[0].render_snapshot_png, Some(vec![1u8; 1024]));
        assert!(summaries[0].has_render_snapshot);
    }

    #[test]
    fn list_file_summaries_includes_creator() {
        // creator muss in der Projektion sein, sonst greift der Creator-Filter nicht.
        let conn = connect_in_memory().unwrap();
        let file_id = test_insert_minimal_file(&conn, "/tmp/z.3mf", None).unwrap();
        conn.execute("UPDATE files SET creator = ?1 WHERE id = ?2", params!["CarlFromUp", file_id]).unwrap();

        let summaries = list_file_summaries(&conn).unwrap();

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].creator, Some("CarlFromUp".to_string()));
    }

    #[test]
    fn list_file_summaries_includes_last_viewed_at_and_content_hash() {
        let conn = connect_in_memory().unwrap();
        let viewed = test_insert_minimal_file(&conn, "/tmp/viewed.3mf", None).unwrap();
        let _plain = test_insert_minimal_file(&conn, "/tmp/plain.3mf", None).unwrap();
        conn.execute(
            "UPDATE files SET last_viewed_at = ?1, content_hash = ?2 WHERE id = ?3",
            params!["2026-09-24T08:00:00+00:00", "abc123", viewed],
        )
        .unwrap();

        let summaries = list_file_summaries(&conn).unwrap();
        let v = summaries.iter().find(|s| s.id == viewed).unwrap();
        let p = summaries.iter().find(|s| s.id != viewed).unwrap();

        assert_eq!(v.last_viewed_at.as_deref(), Some("2026-09-24T08:00:00+00:00"));
        assert_eq!(v.content_hash.as_deref(), Some("abc123"));
        assert_eq!(p.last_viewed_at, None);
        assert_eq!(p.content_hash, None);
    }

    #[test]
    fn list_file_summaries_has_render_snapshot_false_when_none_saved() {
        let conn = connect_in_memory().unwrap();
        test_insert_minimal_file(&conn, "/tmp/y.3mf", None).unwrap();

        let summaries = list_file_summaries(&conn).unwrap();

        assert_eq!(summaries.len(), 1);
        assert!(!summaries[0].has_render_snapshot);
    }

    // `Connection::trace` nimmt nur einen Funktionszeiger, der Zaehler lebt deshalb
    // ausserhalb. Thread-lokal reicht: der Callback laeuft synchron, und jeder Test
    // hat einen eigenen Thread.
    thread_local! {
        static QUERY_TRACE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    // `trace_v2` mit `SQLITE_TRACE_STMT` ersetzt das veraltete `trace`.
    fn count_traced_query(event: rusqlite::trace::TraceEvent<'_>) {
        if matches!(event, rusqlite::trace::TraceEvent::Stmt(_, _)) {
            QUERY_TRACE_COUNT.with(|c| c.set(c.get() + 1));
        }
    }

    #[test]
    fn list_file_summaries_executes_a_constant_number_of_queries_regardless_of_row_count() {
        let conn = connect_in_memory().unwrap();
        for i in 0..500 {
            test_insert_minimal_file(&conn, &format!("/tmp/model-{i}.3mf"), None).unwrap();
        }

        QUERY_TRACE_COUNT.with(|c| c.set(0));
        conn.trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
            Some(count_traced_query),
        );

        let summaries = list_file_summaries(&conn).unwrap();

        assert_eq!(summaries.len(), 500);
        let executed = QUERY_TRACE_COUNT.with(|c| c.get());
        assert!(
            executed <= 3,
            "list_file_summaries darf nicht pro Zeile eine zusaetzliche Query ausfuehren (gemessen: {executed} Statements fuer 500 Zeilen - muss unabhaengig von der Zeilenzahl konstant klein bleiben, nicht O(N))"
        );
    }

    #[test]
    #[ignore] // manuell ausfuehren: cargo test --lib -- --ignored list_file_summaries_benchmark
    fn list_file_summaries_benchmark_with_5000_files() {
        let conn = connect_in_memory().unwrap();
        for i in 0..5000 {
            test_insert_minimal_file(&conn, &format!("/tmp/model-{i}.3mf"), None).unwrap();
        }
        let start = std::time::Instant::now();
        let summaries = list_file_summaries(&conn).unwrap();
        let elapsed = start.elapsed();
        assert_eq!(summaries.len(), 5000);
        // Bewusst ohne Zeit-Assertion: nur zur Beobachtung mit --nocapture.
        eprintln!("list_file_summaries(5000 rows): {elapsed:?}");
    }

    #[test]
    fn list_all_file_tags_returns_every_file_tag_pair() {
        let conn = connect_in_memory().unwrap();
        let file_a = test_insert_minimal_file(&conn, "/tmp/a.3mf", None).unwrap();
        let file_b = test_insert_minimal_file(&conn, "/tmp/b.3mf", None).unwrap();
        add_tag_to_file(&conn, file_a, "vase").unwrap();
        add_tag_to_file(&conn, file_a, "red").unwrap();
        add_tag_to_file(&conn, file_b, "vase").unwrap();

        let mut pairs = list_all_file_tags(&conn).unwrap();
        pairs.sort();
        let mut expected = vec![
            (file_a, "red".to_string()),
            (file_a, "vase".to_string()),
            (file_b, "vase".to_string()),
        ];
        expected.sort();
        assert_eq!(pairs, expected);
    }

    #[test]
    fn list_all_file_tags_executes_a_constant_number_of_queries_regardless_of_row_count() {
        let conn = connect_in_memory().unwrap();
        for i in 0..500 {
            let file_id = test_insert_minimal_file(&conn, &format!("/tmp/tagged-{i}.3mf"), None).unwrap();
            add_tag_to_file(&conn, file_id, "bulk").unwrap();
        }

        QUERY_TRACE_COUNT.with(|c| c.set(0));
        conn.trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
            Some(count_traced_query),
        );

        let pairs = list_all_file_tags(&conn).unwrap();

        assert_eq!(pairs.len(), 500);
        let executed = QUERY_TRACE_COUNT.with(|c| c.get());
        assert!(
            executed <= 3,
            "list_all_file_tags darf nicht pro Zeile eine zusaetzliche Query ausfuehren (gemessen: {executed} Statements fuer 500 Zeilen - muss unabhaengig von der Zeilenzahl konstant klein bleiben, nicht O(N))"
        );
    }

    #[test]
    fn list_files_by_ids_returns_full_records_including_images_and_metadata() {
        let conn = connect_in_memory().unwrap();
        let file_id = test_insert_minimal_file(&conn, "/tmp/detail.3mf", None).unwrap();
        conn.execute(
            "UPDATE files SET render_snapshot_png = ?1 WHERE id = ?2",
            params![vec![0u8; 1024], file_id],
        ).unwrap();

        let files = list_files_by_ids(&conn, &[file_id]).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].render_snapshot_png, Some(vec![0u8; 1024]));
    }

    #[test]
    fn list_folders_returns_path_and_parent_id() {
        let conn = connect_in_memory().unwrap();
        conn.execute("INSERT INTO folders (name, path) VALUES ('Root', '/tmp/Root')", []).unwrap();
        let root_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO folders (name, path, parent_id) VALUES ('Child', '/tmp/Root/Child', ?1)",
            params![root_id],
        ).unwrap();

        let folders = list_folders(&conn).unwrap();
        assert_eq!(folders.len(), 2);
        let child = folders.iter().find(|f| f.name == "Child").unwrap();
        assert_eq!(child.parent_id, Some(root_id));
        assert_eq!(child.path, "/tmp/Root/Child");
    }

    #[test]
    fn ensure_folder_path_creates_missing_levels() {
        let conn = connect_in_memory().unwrap();
        let root = Path::new("/tmp/Tabletop");
        let dir = Path::new("/tmp/Tabletop/Reaper");

        let leaf_id = ensure_folder_path(&conn, root, dir).unwrap();
        let folders = list_folders(&conn).unwrap();
        assert_eq!(folders.len(), 2);
        let leaf = folders.iter().find(|f| f.id == leaf_id).unwrap();
        assert_eq!(leaf.name, "Reaper");
        assert_eq!(leaf.path, "/tmp/Tabletop/Reaper");
        let parent = folders.iter().find(|f| f.id == leaf.parent_id.unwrap()).unwrap();
        assert_eq!(parent.name, "Tabletop");
        assert_eq!(parent.parent_id, None);
    }

    #[test]
    fn ensure_folder_path_is_idempotent() {
        let conn = connect_in_memory().unwrap();
        let root = Path::new("/tmp/Tabletop");
        let dir = Path::new("/tmp/Tabletop/Reaper");

        let first = ensure_folder_path(&conn, root, dir).unwrap();
        let second = ensure_folder_path(&conn, root, dir).unwrap();
        assert_eq!(first, second);
        assert_eq!(list_folders(&conn).unwrap().len(), 2);
    }

    #[test]
    fn ensure_folder_path_file_directly_in_root() {
        let conn = connect_in_memory().unwrap();
        let root = Path::new("/tmp/Tabletop");

        let id = ensure_folder_path(&conn, root, root).unwrap();
        let folders = list_folders(&conn).unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].id, id);
        assert_eq!(folders[0].name, "Tabletop");
    }

    #[test]
    fn attach_folder_to_parent_by_path_moves_a_root_folder_under_its_catalogued_parent() {
        let conn = connect_in_memory().unwrap();
        let parent = Path::new("/tmp/Katalog/Tabletop");
        let child = Path::new("/tmp/Katalog/Tabletop/Drache");
        let parent_id = ensure_folder_path(&conn, parent, parent).unwrap();
        let child_id = ensure_folder_path(&conn, child, child).unwrap();

        attach_folder_to_parent_by_path(&conn, child).unwrap();

        let folders = list_folders(&conn).unwrap();
        let child_row = folders.iter().find(|f| f.id == child_id).unwrap();
        assert_eq!(child_row.parent_id, Some(parent_id));
    }

    #[test]
    fn attach_folder_to_parent_by_path_is_a_no_op_without_catalogued_parent_or_for_nested_folders() {
        let conn = connect_in_memory().unwrap();
        let lonely = Path::new("/tmp/Irgendwo/Drache");
        let lonely_id = ensure_folder_path(&conn, lonely, lonely).unwrap();
        attach_folder_to_parent_by_path(&conn, lonely).unwrap();
        let folders = list_folders(&conn).unwrap();
        assert_eq!(folders.iter().find(|f| f.id == lonely_id).unwrap().parent_id, None);

        // Bereits eingehaengte Ordner werden nicht umgehaengt.
        let root = Path::new("/tmp/A");
        let nested = Path::new("/tmp/A/B");
        let nested_id = ensure_folder_path(&conn, root, nested).unwrap();
        let parent_before = list_folders(&conn).unwrap().into_iter().find(|f| f.id == nested_id).unwrap().parent_id;
        attach_folder_to_parent_by_path(&conn, nested).unwrap();
        let parent_after = list_folders(&conn).unwrap().into_iter().find(|f| f.id == nested_id).unwrap().parent_id;
        assert!(parent_before.is_some());
        assert_eq!(parent_before, parent_after);
    }

    #[test]
    fn update_paths_under_folder_rejects_a_child_whose_path_is_not_below_the_parent() {
        let conn = connect_in_memory().unwrap();
        let parent = Path::new("/tmp/Katalog/A");
        let parent_id = ensure_folder_path(&conn, parent, parent).unwrap();
        // Inkonsistente Hierarchie wie aus einem praeparierten Backup:
        // parent_id zeigt auf A, der Pfad liegt aber woanders.
        insert_folder_with_parent(&conn, "fremd", Some(parent_id), "/x").unwrap();

        let result = update_paths_under_folder(&conn, parent_id, "/tmp/Katalog/A", "/tmp/Katalog/B");
        assert!(result.is_err());
    }

    #[test]
    fn list_files_returns_an_error_for_a_row_with_an_unparseable_file_type() {
        let conn = connect_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        // ignore_check_constraints, damit der INSERT trotz CHECK in schema.sql gelingt.
        conn.execute("PRAGMA ignore_check_constraints = 1", []).unwrap();
        conn.execute(
            "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('x', '/tmp/x.xyz', 'xyz', 1, '2026-01-01T00:00:00Z')",
            [],
        ).expect("insert must succeed with check constraints disabled");

        let result = list_files(&conn);

        assert!(result.is_err(), "ein nicht parsebarer file_type darf nicht still auf ThreeMf zurueckfallen");
    }

    #[test]
    fn list_files_still_succeeds_for_rows_with_valid_file_types() {
        // Normale 3mf/stl-Zeilen bleiben unberuehrt.
        let conn = connect_in_memory().unwrap();
        test_insert_minimal_file(&conn, "/tmp/a.3mf", None).unwrap();
        let result = list_files(&conn);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn list_file_summaries_returns_an_error_for_a_row_with_an_unparseable_file_type() {
        let conn = connect_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn.execute("PRAGMA ignore_check_constraints = 1", []).unwrap();
        conn.execute(
            "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('x', '/tmp/x.xyz', 'xyz', 1, '2026-01-01T00:00:00Z')",
            [],
        ).expect("insert must succeed with check constraints disabled");

        let result = list_file_summaries(&conn);

        assert!(result.is_err(), "ein nicht parsebarer file_type darf nicht still auf ThreeMf zurueckfallen");
    }

    #[test]
    fn list_file_summaries_still_succeeds_for_rows_with_valid_file_types() {
        // Normale 3mf/stl-Zeilen bleiben unberuehrt.
        let conn = connect_in_memory().unwrap();
        test_insert_minimal_file(&conn, "/tmp/a.3mf", None).unwrap();
        let result = list_file_summaries(&conn);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }
}
