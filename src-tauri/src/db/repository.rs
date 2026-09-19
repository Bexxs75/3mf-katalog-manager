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
    Ok(())
}

// Nur von Tests genutzt: legt eine minimale files-Zeile an (Name wird aus
// dem letzten Pfadsegment abgeleitet), fuer Kompensationstests in
// commands.rs, die einen kollidierenden `path`-Wert oder einen bereits
// vorhandenen file_id-Datensatz brauchen, ohne den vollen `insert_file`-Weg
// mit einem kompletten `NewFile` zu gehen (siehe Task-1-Brief, C-01/H-01).
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

// Nur von Tests genutzt: einfacher Test-Helfer, um schnell eine
// folders-Zeile ohne parent_id/echte Verzeichnisstruktur anzulegen (fuer
// Faelle, in denen der volle `insert_folder_with_parent`-Aufruf mit
// physischem Pfad nicht noetig ist, z.B. list_folders()-Tests).
#[cfg(test)]
pub fn insert_folder(conn: &Connection, name: &str) -> Result<i64, DbError> {
    // path wird hier synthetisch aus dem Namen gebildet, nur damit bestehende
    // Tests (die diese 1-Parameter-Signatur nutzen) weiterhin gueltige
    // FolderRecord-Zeilen erzeugen (path ist in Rust ein non-optionales
    // String-Feld). Eine echte parent_id/path-Vergabe kommt erst mit der
    // erweiterten Signatur in Task 2.
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

/// Legt fuer jede Verzeichnisebene zwischen `import_root` (inklusive) und
/// `dir` (inklusive) einen folders-Eintrag an, sofern er noch nicht
/// existiert (Lookup per `path`-Spalte, idempotent bei wiederholtem
/// Import desselben Baums). Gibt die id der tiefsten Ebene (= `dir`)
/// zurueck.
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

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// Duenner Insert-Wrapper fuer `create_folder`: legt IMMER eine neue Zeile
/// an (anders als `find_or_insert_folder`, das bei bereits existierendem
/// `path` still die bestehende id zurueckgibt). Ein `create_folder`-Aufruf
/// mit bereits existierendem Zielpfad soll fehlschlagen statt den
/// bestehenden Ordner zurueckzugeben - in der Praxis schlaegt in diesem
/// Fall aber schon `std::fs::create_dir` vorher mit `AlreadyExists` fehl,
/// bevor diese Funktion ueberhaupt erreicht wird.
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

/// Legt einen neuen Eintrag in der maschinenlokalen Slicer-Registry an
/// (M-06, Task 11). `executable_path` ist `UNIQUE` im Schema - ein erneuter
/// Insert desselben Pfads (z.B. bei jedem App-Start erneut auto-erkannt)
/// schlaegt mit einem echten `DbError` fehl; Aufrufer, die das best-effort
/// tolerieren wollen (Autoerkennung), pruefen vorher selbst gegen
/// `list_registered_slicers`.
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

/// Aktualisiert `folder_id` und `path` einer Datei nach einem physischen
/// Verschieben (siehe `move_file_to_folder`-Command in `commands.rs`).
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

/// Aktualisiert nur die `name`-Spalte eines Ordners (der physische
/// `std::fs::rename` und das rekursive Pfad-Update via
/// `update_paths_under_folder` passieren getrennt, siehe `rename_folder`-
/// Command in `commands.rs`).
pub fn rename_folder_name(conn: &Connection, folder_id: i64, name: &str) -> Result<(), DbError> {
    conn.execute("UPDATE folders SET name = ?1 WHERE id = ?2", params![name, folder_id])?;
    Ok(())
}

/// Aktualisiert nur die `parent_id`-Spalte eines Ordners (der physische
/// `std::fs::rename` und das rekursive Pfad-Update via
/// `update_paths_under_folder` passieren getrennt, siehe `move_folder`-
/// Command in `commands.rs`).
pub fn set_folder_parent(conn: &Connection, folder_id: i64, parent_id: Option<i64>) -> Result<(), DbError> {
    conn.execute("UPDATE folders SET parent_id = ?1 WHERE id = ?2", params![parent_id, folder_id])?;
    Ok(())
}

/// Rekursives Praefix-Update fuer `folders.path` UND `files.path`
/// unterhalb eines Ordners, nachdem sich dessen eigener Pfad geaendert hat
/// (Umbenennen oder Verschieben, siehe `rename_folder`/`move_folder`-
/// Commands in `commands.rs`). `old_path`/`new_path` sind der alte bzw.
/// neue absolute Pfad von `folder_id` selbst; Kind-Ordner und -Dateien
/// werden per Praefix-Ersetzung mitgezogen, beliebig tief verschachtelt.
pub fn update_paths_under_folder(
    conn: &Connection,
    folder_id: i64,
    old_path: &str,
    new_path: &str,
) -> Result<(), DbError> {
    conn.execute("UPDATE folders SET path = ?1 WHERE id = ?2", params![new_path, folder_id])?;
    // length()/substr() auf TEXT-Werten zaehlen in SQLite in UTF-8-Zeichen,
    // nicht in Bytes - old_path.len() (Rust, Byte-Laenge) waere bei
    // Pfaden mit Nicht-ASCII-Zeichen (Umlaute etc.) ein falscher Offset.
    // Indem length() hier ebenfalls von SQLite auf dem TEXT-Wert berechnet
    // wird, stimmen beide Seiten in derselben Einheit (Zeichen) ueberein.
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
        let suffix = &child_old_path[old_path.len()..];
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
    let id = insert_file_within_tx(&tx, file)?;
    tx.commit()?;
    Ok(id)
}

/// Core of [`insert_file`], operating on an already-open transaction/connection
/// instead of opening its own. Callers that import many files in one batch
/// (see `commands::import_many_with_conn`) use this directly so the whole
/// batch commits once instead of once per file - SQLite fsyncs on every
/// commit, so one-transaction-per-file made large imports take a very long
/// time (Finding, Review 2026-09-13).
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

/// Schlanke Projektion von `files` fuer die Katalog-Uebersicht (Grid/Liste):
/// bewusst OHNE `render_snapshot_png`/`custom_image_png` (grosse
/// Zusatzbilder, nur auf der Detailseite gebraucht) und OHNE
/// Materials/Metadata/Tags (bislang pro Zeile per `load_materials`/
/// `load_metadata`/`load_tags` nachgeladen - genau das N+1-Problem aus
/// Finding M-01). `thumbnail_png` bleibt enthalten, da die Kachel-/
/// Grid-Vorschau ohne ein kleines Bild pro Zeile nicht sinnvoll waere
/// (Variante (a) aus dem Task-6-Brief).
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
}

pub fn list_file_summaries(conn: &Connection) -> Result<Vec<FileSummary>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, path, file_type, folder_id, file_size_bytes,
                dimension_x_mm, dimension_y_mm, dimension_z_mm, volume_cm3,
                object_count, imported_at, print_status, favorite,
                queue_position, thumbnail_png
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
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Alle Datei->Tag-Zuordnungen in EINER Abfrage (Nachtrag zu Finding M-01:
/// die Sidebar-Tag-Filterung (`m.tags.includes(activeTag)`) braucht pro
/// Datei die Tag-Liste, die `list_file_summaries` bewusst nicht mehr
/// mitliefert - ein Nachladen ueber diese eine Aggregat-Abfrage haelt die
/// Anzahl der Statements weiterhin O(1) statt O(N), im Gegensatz zu einem
/// erneuten `load_tags`-Aufruf pro Zeile).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_file_summaries_omits_large_blob_columns() {
        let conn = connect_in_memory().unwrap();
        let file_id = test_insert_minimal_file(&conn, "/tmp/x.3mf", None).unwrap();
        conn.execute(
            "UPDATE files SET render_snapshot_png = ?1, custom_image_png = ?2 WHERE id = ?3",
            params![vec![0u8; 1024], vec![0u8; 1024], file_id],
        ).unwrap();

        let summaries = list_file_summaries(&conn).unwrap();

        assert_eq!(summaries.len(), 1);
        // FileSummary hat schlicht KEIN Feld fuer render_snapshot_png/custom_image_png -
        // dieser Test dokumentiert die Absicht ueber die Feldliste des Typs selbst
        // (Compile-Zeit-Garantie: FileSummary { .. } ohne diese Felder).
        assert_eq!(summaries[0].thumbnail_png, None);
    }

    // `Connection::trace` (rusqlite 0.40) nimmt nur einen reinen
    // Funktionszeiger (`fn(&str)`), keine capturing Closure - der Zaehler
    // muss deshalb ausserhalb der Closure leben. Ein thread-lokaler Zaehler
    // reicht hier aus: der SQLite-Trace-Callback laeuft synchron auf
    // demselben Thread wie der `list_file_summaries`-Aufruf, und cargo test
    // fuehrt jeden Testfall auf einem eigenen Thread aus, wodurch Tests sich
    // nicht gegenseitig verfaelschen (anders als bei einem globalen Static).
    thread_local! {
        static QUERY_TRACE_COUNT: std::cell::Cell<usize> = std::cell::Cell::new(0);
    }

    // `Connection::trace` ist in dieser rusqlite-Version als deprecated
    // markiert (siehe Doc-Kommentar oben) - `trace_v2` mit dem
    // `SQLITE_TRACE_STMT`-Event ist der empfohlene Nachfolger und zaehlt
    // exakt dieselben "eine Query beginnt"-Ereignisse.
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
        // Bewusst KEINE harte Zeit-Assertion (P2-Korrektur) - dieser Test dient
        // nur der manuellen Beobachtung via --nocapture, nicht als CI-Gate.
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
    fn list_files_returns_an_error_for_a_row_with_an_unparseable_file_type() {
        let conn = connect_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        // PRAGMA ignore_check_constraints deaktiviert CHECK-Constraints GEZIELT
        // fuer diese Verbindung (offizielle SQLite-Pragma, siehe
        // https://www.sqlite.org/pragma.html#pragma_ignore_check_constraints) -
        // damit ist garantiert, dass der folgende INSERT gelingt, unabhaengig
        // vom bestehenden `CHECK (file_type IN ('3mf', 'stl'))` in schema.sql.
        // Kein bedingtes "return" mehr moeglich wie in der urspruenglichen
        // Testfassung - der Test prueft den Fix immer tatsaechlich.
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
        // Regressionsschutz: normale 3mf/stl-Zeilen (der weit ueberwiegende
        // Normalfall) duerfen durch die Aenderung nicht beeintraechtigt werden.
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
        // Regressionsschutz: normale 3mf/stl-Zeilen (der weit ueberwiegende
        // Normalfall) duerfen durch die Aenderung nicht beeintraechtigt werden.
        let conn = connect_in_memory().unwrap();
        test_insert_minimal_file(&conn, "/tmp/a.3mf", None).unwrap();
        let result = list_file_summaries(&conn);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }
}
