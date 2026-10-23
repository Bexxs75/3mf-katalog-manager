use std::collections::BTreeMap;
use std::path::{Component, Path};
#[cfg(any(test, feature = "test-support"))]
use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension};

use super::error::DbError;
use super::models::{
    FileRecord, FileType, FilamentSpoolRecord, FolderRecord, MaterialRecord,
    NewFile, NewFilamentSpool, NewPrintLogEntry, PrintLogEntryRecord,
    RegisteredSlicer, TagCount,
};

pub const SCHEMA_SQL: &str = include_str!("schema.sql");

/// Opens an existing catalog without initialization, migrations or cleanup.
/// SQLite's read-only flag is the primary write barrier; query_only adds a
/// connection-local barrier. A hot rollback journal must be recovered by a writer.
pub fn open_read_only(path: &Path) -> Result<Connection, DbError> {
    if !path.try_exists().map_err(|e| DbError::Other(format!("Katalog nicht erreichbar: {e}")))? {
        return Err(DbError::Other("Katalogdatei fehlt".into()));
    }
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.pragma_update(None, "query_only", true)?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| match &e {
            rusqlite::Error::SqliteFailure(code, _) if code.code == rusqlite::ErrorCode::ReadOnly =>
                DbError::Other("Katalog kann nicht schreibgeschützt geöffnet werden: Ein heißes Journal muss zuerst von der Desktop-App (oder dem schreibenden Modus) wiederhergestellt werden".into()),
            _ => DbError::from(e),
        })?;
    let supported = super::migrations::CURRENT_SCHEMA_VERSION;
    if version < supported {
        return Err(DbError::Other("Katalog muss zuerst von der Desktop-App (oder dem schreibenden Modus) migriert werden".into()));
    }
    if version > supported {
        return Err(DbError::NewerSchema { found: version, supported });
    }
    Ok(conn)
}

/// Opens only an existing, current-schema catalog without migration or cleanup.
/// Hot-journal recovery can write while opening; enable only with KATALOG_WRITE=1.
pub fn open_read_write(path: &Path) -> Result<Connection, DbError> {
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let supported = super::migrations::CURRENT_SCHEMA_VERSION;
    if version != supported {
        return Err(DbError::Other(format!("Katalogschema {version}, erwartet {supported}; Migration nur in der Desktop-App")));
    }
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

pub fn connect(path: &Path) -> Result<Connection, DbError> {
    connect_with_warning(path).map(|(conn, _)| conn)
}

pub fn connect_with_warning(path: &Path) -> Result<(Connection, bool), DbError> {
    let mut conn = Connection::open(path)?;
    let backup_failed = match backup_before_migration(&conn, path) {
        Ok(()) => false,
        Err(e) => {
            log::warn!(target: "backup", "Sicherung vor der Schema-Migration fehlgeschlagen: {e}");
            true
        }
    };
    init(&mut conn)?;
    Ok((conn, backup_failed))
}

/// Directory next to the catalog database where copies taken before a schema
/// migration (and before in-app updates) are kept.
pub const BACKUP_DIR_NAME: &str = "update-backups";

/// A new app version migrates the catalog forward only; an older version cannot
/// read the result. Whoever installs a version by hand (no in-app update, which
/// has its own backup) would lose the way back, so an existing catalog is copied
/// right before the first migration. A failed copy must never block the start.
fn backup_before_migration(conn: &Connection, db_path: &Path) -> Result<(), DbError> {
    let Some((from, to)) = migration_backup_label(conn)? else { return Ok(()) };
    let dir = db_path.parent().ok_or_else(|| DbError::Other("Sicherungsordner nicht gefunden".into()))?.join(BACKUP_DIR_NAME);
    let path = crate::backup::create_schema(conn, &dir, from, to).map_err(DbError::Other)?;
    log::info!(target: "backup", "Sicherung vor der Schema-Migration: {}", path.display());
    Ok(())
}

/// `Some((old, new))` when the database already holds a catalog
/// (any table) and its schema version is older than this build's, else `None`.
fn migration_backup_label(conn: &Connection) -> Result<Option<(i64, i64)>, DbError> {
    let tables: i64 = conn
        .query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table'", [], |r| r.get(0))?;
    if tables == 0 {
        return Ok(None);
    }
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let target = super::migrations::CURRENT_SCHEMA_VERSION;
    Ok((version < target).then_some((version, target)))
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
    // Don't abort: an error here must not prevent startup; the transaction in
    // merge_auto_tag_aliases rolls back.
    if let Err(e) = merge_auto_tag_aliases(conn) {
        log::error!(target: "tags", "merging automatic tag names failed: {e}");
    }
    Ok(())
}

// Tests only: minimal files row (name from the last path segment), without the full `insert_file` path.
#[cfg(any(test, feature = "test-support"))]
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

// Tests only: folders row without parent_id and without a real directory.
#[cfg(any(test, feature = "test-support"))]
pub fn insert_folder(conn: &Connection, name: &str) -> Result<i64, DbError> {
    // Synthetic path, because `path` isn't optional in Rust.
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

/// Creates a folders row for every directory level from `import_root` to `dir`
/// (both inclusive) if missing (idempotent via `path`). Returns the id of `dir`.
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

/// Attaches a folder created as a root (`parent_id IS NULL`) below the cataloged
/// folder whose `path` is its parent directory. Needed after extracting an
/// archive: `ensure_folder_path` always creates the import root as a root, even
/// if its parent is already a catalog folder. Does nothing without a cataloged
/// parent or for folders that are already attached.
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

/// Highest folder id in use. Ids are AUTOINCREMENT, so every row inserted
/// afterwards has a larger id.
#[cfg(any(test, feature = "test-support"))]
pub fn max_folder_id(conn: &Connection) -> Result<i64, DbError> {
    Ok(conn.query_row("SELECT COALESCE(MAX(id), 0) FROM folders", [], |r| r.get(0))?)
}

/// Drops the catalog rows of the given folders, but only rows inserted after
/// `known_max_id` and without files. Used when an extraction is undone because
/// nothing new reached the catalog: an older row for the same path (e.g. a
/// folder deleted outside the app) and its subfolders must stay.
#[cfg(any(test, feature = "test-support"))]
pub fn remove_new_folder_rows(conn: &Connection, dirs: &[PathBuf], known_max_id: i64) -> Result<(), DbError> {
    for dir in dirs.iter().rev() {
        conn.execute(
            "DELETE FROM folders WHERE path = ?1 AND id > ?2
               AND NOT EXISTS (SELECT 1 FROM files WHERE files.folder_id = folders.id)",
            params![dir.to_string_lossy().to_string(), known_max_id],
        )?;
    }
    Ok(())
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// ALWAYS inserts a new row, unlike `find_or_insert_folder`. An existing target
/// path is caught earlier by `create_dir` with `AlreadyExists`.
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

/// New entry in the machine-local slicer registry. `executable_path` is UNIQUE;
/// auto-detection therefore checks against `list_registered_slicers` first.
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

/// Updates `folder_id` and `path` after a physical move.
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

/// Updates `name` and `path` after a physical rename.
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

/// Updates only `name`; renaming on disk and the path update (`update_paths_under_folder`) happen separately.
pub fn rename_folder_name(conn: &Connection, folder_id: i64, name: &str) -> Result<(), DbError> {
    conn.execute("UPDATE folders SET name = ?1 WHERE id = ?2", params![name, folder_id])?;
    Ok(())
}

/// Updates only `parent_id`; moving on disk and the path update (`update_paths_under_folder`) happen separately.
pub fn set_folder_parent(conn: &Connection, folder_id: i64, parent_id: Option<i64>) -> Result<(), DbError> {
    conn.execute("UPDATE folders SET parent_id = ?1 WHERE id = ?2", params![parent_id, folder_id])?;
    Ok(())
}

/// Rewrites `folders.path` and `files.path` below `folder_id` by prefix
/// replacement after its path changed from `old_path` to `new_path`.
///
/// Folders are followed through `parent_id`. Files are matched twice: by
/// `folder_id` (rows of the folder subtree) and by their path (everything below
/// `old_path` + separator). The path match is what keeps rows whose `folder_id` is
/// NULL or stale (single dropped files, damaged backups) from pointing at a
/// folder that no longer exists. It deliberately includes trash rows: `path` of a
/// trashed row is the place "restore" moves the file back to, so it has to follow
/// the folder too (the file itself sits at `trash_path` and is unaffected).
///
/// A new path that collides with an existing row fails on `UNIQUE(path)`; callers
/// run this in a transaction and undo the move on disk when it errors.
/// `new_path` must not lie below `old_path` (the callers rule that out).
pub fn update_paths_under_folder(
    conn: &Connection,
    folder_id: i64,
    old_path: &str,
    new_path: &str,
) -> Result<(), DbError> {
    update_folder_subtree_paths(conn, folder_id, old_path, new_path)?;

    // substr()/length() instead of LIKE: folder names may contain % and _, and
    // "/a/Deko" must not match "/a/Deko2/x.3mf". The separator is the one the app
    // writes into `path` (it stores native paths).
    let sep = std::path::MAIN_SEPARATOR.to_string();
    conn.execute(
        "UPDATE files SET path = ?1 || substr(path, length(?2) + 1)
         WHERE substr(path, 1, length(?2) + length(?3)) = ?2 || ?3",
        params![new_path, old_path, sep],
    )?;
    // Companion files sit in `_extras` folders below the model's folder and move
    // with it. Trashed rows follow too: their path is where a restore puts them.
    conn.execute(
        "UPDATE companion_files SET path = ?1 || substr(path, length(?2) + 1)
         WHERE substr(path, 1, length(?2) + length(?3)) = ?2 || ?3",
        params![new_path, old_path, sep],
    )?;
    Ok(())
}

fn update_folder_subtree_paths(
    conn: &Connection,
    folder_id: i64,
    old_path: &str,
    new_path: &str,
) -> Result<(), DbError> {
    conn.execute("UPDATE folders SET path = ?1 WHERE id = ?2", params![new_path, folder_id])?;
    // length()/substr() count characters in SQLite, not bytes. So let SQLite compute
    // the length too, otherwise the offset is wrong for non-ASCII names.
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
        // No byte slicing: if the hierarchy doesn't match the paths (crafted backup),
        // that would panic and, with panic = "abort", kill the app.
        let Some(suffix) = child_old_path.strip_prefix(old_path) else {
            return Err(DbError::Other(format!(
                "Ordner {child_id} ({child_old_path}) liegt nicht unter {old_path}"
            )));
        };
        let child_new_path = format!("{new_path}{suffix}");
        update_folder_subtree_paths(conn, child_id, &child_old_path, &child_new_path)?;
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

/// Merges tags named like an automatic tag in any language (e.g. "Multipart")
/// into the German key. Runs every time the DB is opened, idempotent and in one
/// transaction.
pub fn merge_auto_tag_aliases(conn: &mut Connection) -> Result<usize, DbError> {
    let tx = conn.transaction()?;
    let tags: Vec<(i64, String)> = {
        let mut stmt = tx.prepare("SELECT id, name FROM tags")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut merged = 0;
    for (alias_id, name) in tags {
        // Ambiguous aliases ("mini" as in "Bambu A1 mini") are deliberately not merged at startup.
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

/// Deletes a tag once no file is assigned to it.
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

/// Removes all tags without files at startup.
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

// Tests only; the import uses `insert_file_within_tx` in a batch transaction.
#[cfg(any(test, feature = "test-support"))]
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
            plate_count, slice_info_json, license,
            source_platform, source_model_id, source_profile_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31)",
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
            file.license,
            file.source.platform,
            file.source.model_id,
            file.source.profile_id,
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

    // Every automatic assignment ends up here (import, folder sync, source folders,
    // archives), so the block list is applied in this one place.
    let mut tags = file.tags.clone();
    super::blocked_tags::retain_unblocked(conn, &mut tags)?;
    for tag_name in &tags {
        // Derived tags must never prevent importing the file itself.
        if tag_name.encode_utf16().take(101).count() > 100 {
            continue;
        }
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
            plate_count = ?7, slice_info_json = ?8, file_size_bytes = ?9, content_hash = ?10, file_modified_at = ?12,
            source_platform = ?13, source_model_id = ?14, source_profile_id = ?15
         WHERE id = ?11",
        params![
            dim_x, dim_y, dim_z, update.volume_cm3, update.object_count,
            update.thumbnail_png, update.plate_count, update.slice_info_json,
            update.file_size_bytes, update.content_hash, file_id, update.file_modified_at,
            update.source.platform, update.source.model_id, update.source.profile_id,
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
    // Remember the tag ids first: file_tags is removed via ON DELETE CASCADE and
    // afterwards we'd no longer know which tags were affected.
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
     plate_count, slice_info_json, deleted_at, trash_path, license,
                    source_platform, source_model_id, source_profile_id";

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

#[cfg(any(test, feature = "test-support"))]
pub fn file_exists_by_path(conn: &Connection, path: &str) -> Result<bool, DbError> {
    let exists: Option<i64> = conn
        .query_row("SELECT 1 FROM files WHERE path = ?1 AND deleted_at IS NULL", params![path], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(exists.is_some())
}

#[cfg(any(test, feature = "test-support"))]
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

/// Identity of a catalog row that blocks an import as a duplicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingFileRef {
    pub id: i64,
    pub name: String,
}

// At most two rows are fetched: callers only need to tell "exactly one" from
// "several", and an ambiguous match must not be presented as a single file.
fn find_existing(conn: &Connection, column: &str, value: &str) -> Result<Vec<ExistingFileRef>, DbError> {
    let sql = format!("SELECT id, name FROM files WHERE {column} = ?1 AND deleted_at IS NULL ORDER BY id LIMIT 2");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![value], |row| Ok(ExistingFileRef { id: row.get(0)?, name: row.get(1)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn find_existing_by_path(conn: &Connection, path: &str) -> Result<Vec<ExistingFileRef>, DbError> {
    find_existing(conn, "path", path)
}

pub fn find_existing_by_hash(conn: &Connection, hash: &str) -> Result<Vec<ExistingFileRef>, DbError> {
    find_existing(conn, "content_hash", hash)
}

pub fn get_file(conn: &Connection, id: i64) -> Result<Option<FileRecord>, DbError> {
    let row = conn
        .query_row(
            "SELECT id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
                    file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
                    volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
                    print_status, last_viewed_at, creator, content_hash,
                    render_snapshot_png, custom_image_png, source_url, queue_position, favorite,
                    plate_count, slice_info_json, deleted_at, trash_path, license,
                    source_platform, source_model_id, source_profile_id
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

/// Loads several files with ONE main query instead of one per id (e.g. for large collections).
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
                plate_count, slice_info_json, deleted_at, trash_path, license,
                    source_platform, source_model_id, source_profile_id
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

    // SQL "IN" guarantees no order, but the collection position depends on it.
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
                plate_count, slice_info_json, deleted_at, trash_path, license,
                    source_platform, source_model_id, source_profile_id
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

/// Catalog metadata without image blobs or per-file child queries.
pub struct FileSummary {
    pub license: Option<String>,
    pub source_platform: Option<String>,
    pub source_model_id: Option<String>,
    pub source_profile_id: Option<String>,
    pub materials: Vec<MaterialRecord>,
    pub slice_info_json: Option<String>,
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
    pub file_modified_at: Option<String>,
    pub print_status: String,
    pub favorite: bool,
    pub queue_position: Option<i64>,
    pub has_thumbnail: bool,
    pub has_render_snapshot: bool,
    // For the sidebar's creator filter.
    pub creator: Option<String>,
    // For "Recently viewed", "Duplicates" and the 'viewed' sort order.
    pub last_viewed_at: Option<String>,
    pub content_hash: Option<String>,
}

pub fn list_file_summaries(conn: &Connection) -> Result<Vec<FileSummary>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, path, file_type, folder_id, file_size_bytes,
                dimension_x_mm, dimension_y_mm, dimension_z_mm, volume_cm3,
                object_count, imported_at, print_status, favorite,
                queue_position, thumbnail_png IS NOT NULL AS has_thumbnail,
                render_snapshot_png IS NOT NULL AS has_render_snapshot, creator,
                last_viewed_at, content_hash, file_modified_at, slice_info_json, license,
                source_platform, source_model_id, source_profile_id
         FROM files WHERE deleted_at IS NULL ORDER BY name",
    )?;
    let mut rows = stmt
        .query_map([], |row| {
            let dx: Option<f64> = row.get(6)?;
            let dy: Option<f64> = row.get(7)?;
            let dz: Option<f64> = row.get(8)?;
            let file_type_str: String = row.get(3)?;
            Ok(FileSummary {
                license: row.get(22)?,
                source_platform: row.get(23)?,
                source_model_id: row.get(24)?,
                source_profile_id: row.get(25)?,
                materials: Vec::new(),
                slice_info_json: row.get(21)?,
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
                has_thumbnail: row.get::<_, i64>(15)? != 0,
                has_render_snapshot: row.get::<_, i64>(16)? != 0,
                creator: row.get(17)?,
                last_viewed_at: row.get(18)?,
                content_hash: row.get(19)?,
                file_modified_at: row.get(20)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut materials = conn.prepare("SELECT m.file_id, m.name, m.display_color FROM file_materials m JOIN files f ON f.id = m.file_id WHERE f.deleted_at IS NULL ORDER BY m.rowid")?;
    let mut by_id: std::collections::HashMap<i64, Vec<MaterialRecord>> = std::collections::HashMap::new();
    for material in materials.query_map([], |row| Ok((row.get::<_, i64>(0)?, MaterialRecord { name: row.get(1)?, display_color: row.get(2)? })))? {
        let (id, material) = material?;
        by_id.entry(id).or_default().push(material);
    }
    for row in &mut rows { row.materials = by_id.remove(&row.id).unwrap_or_default(); }
    Ok(rows)
}

pub struct FileImages {
    pub id: i64,
    pub thumbnail_png: Option<Vec<u8>>,
    pub render_snapshot_png: Option<Vec<u8>>,
    pub custom_image_png: Option<Vec<u8>>,
}

pub fn list_file_images(conn: &Connection, ids: &[i64]) -> Result<Vec<FileImages>, DbError> {
    if ids.len() > 200 {
        return Err(DbError::Other("at most 200 image IDs per batch".to_string()));
    }
    if ids.is_empty() { return Ok(Vec::new()); }
    let placeholders = vec!["?"; ids.len()].join(",");
    let mut stmt = conn.prepare(&format!(
        "SELECT id, thumbnail_png, render_snapshot_png, custom_image_png FROM files WHERE id IN ({placeholders})"
    ))?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |row| Ok(FileImages {
        id: row.get(0)?, thumbnail_png: row.get(1)?, render_snapshot_png: row.get(2)?, custom_image_png: row.get(3)?,
    }))?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// All file-tag assignments in ONE query, for tag filtering (the summaries have no tags).
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
        license: row.get(30)?,
        source_platform: row.get(31)?,
        source_model_id: row.get(32)?,
        source_profile_id: row.get(33)?,
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

/// (id, path) of all files without content_hash. Only tests need it; production backfill queries the hashes itself.
#[cfg(any(test, feature = "test-support"))]
pub fn list_files_missing_content_hash(conn: &Connection) -> Result<Vec<(i64, String)>, DbError> {
    let mut stmt = conn.prepare("SELECT id, path FROM files WHERE content_hash IS NULL AND deleted_at IS NULL")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(any(test, feature = "test-support"))]
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
            (material, manufacturer, color, location, diameter_mm, original_weight_g, remaining_weight_g, price, image_png, created_at, color_hex, kind, color_hexes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
            crate::db::printers::primary_color_hex(&spool.color_hexes, spool.color_hex.as_deref()),
            spool.kind,
            crate::db::printers::color_list_to_json(&spool.color_hexes),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_filament_spools(conn: &Connection) -> Result<Vec<FilamentSpoolRecord>, DbError> {
    list_filament_spools_with_images(conn, true)
}

pub fn list_filament_spools_with_images(conn: &Connection, include_images: bool) -> Result<Vec<FilamentSpoolRecord>, DbError> {
    Ok(list_filament_spools_with_image_flags(conn, include_images)?.into_iter().map(|(spool, _)| spool).collect())
}

pub fn list_filament_spools_with_image_flags(conn: &Connection, include_images: bool) -> Result<Vec<(FilamentSpoolRecord, bool)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, material, manufacturer, color, location, diameter_mm, original_weight_g, remaining_weight_g, price, CASE WHEN ?1 THEN image_png ELSE NULL END,
                color_hex, home_location, unit_id, slot_index, kind, image_png IS NOT NULL AS has_image, color_hexes
         FROM filament_spools ORDER BY material, manufacturer",
    )?;
    let rows = stmt
        .query_map([include_images], |row| {
            Ok((FilamentSpoolRecord {
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
                color_hexes: crate::db::printers::color_list_from_json(row.get::<_, Option<String>>(16)?.as_deref()),
            }, row.get(15)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Reads a spool including its slot, so `update_filament_spool` returns the real DB state.
pub fn get_filament_spool(conn: &Connection, id: i64) -> Result<FilamentSpoolRecord, DbError> {
    conn.query_row(
        "SELECT id, material, manufacturer, color, location, diameter_mm, original_weight_g, remaining_weight_g, price, image_png,
                color_hex, home_location, unit_id, slot_index, kind, color_hexes
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
                color_hexes: crate::db::printers::color_list_from_json(row.get::<_, Option<String>>(15)?.as_deref()),
            })
        },
    )
    .map_err(DbError::from)
}

/// Never changes a spool's slot (only `load_spool`/`unload_spool` in
/// `db::printers` do). If the spool is loaded, the given location is its home
/// location and goes to `home_location`; `location` then stays empty.
pub fn update_filament_spool(conn: &Connection, id: i64, spool: &NewFilamentSpool) -> Result<(), DbError> {
    update_filament_spool_image_policy(conn, id, spool, true)
}

/// Web PATCH policy: when false the UPDATE does not mention image_png.
pub fn update_filament_spool_image_policy(conn: &Connection, id: i64, spool: &NewFilamentSpool, replace_image: bool) -> Result<(), DbError> {
    // A loaded entry keeps its kind: otherwise resin would sit in a filament slot
    // or filament in a resin vat.
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
    let image_assignment = if replace_image { ", image_png = ?13" } else { "" };
    let sql = format!("UPDATE filament_spools
         SET material = ?1, manufacturer = ?2, color = ?3,
             location = CASE WHEN unit_id IS NULL THEN ?4 ELSE location END,
             home_location = CASE WHEN unit_id IS NULL THEN home_location ELSE ?4 END,
             diameter_mm = ?5, original_weight_g = ?6, remaining_weight_g = ?7, price = ?8,
             color_hex = ?10, kind = ?11, color_hexes = ?12{image_assignment}
         WHERE id = ?9");
    let original_weight = crate::db::printers::round_tenth(spool.original_weight_g);
    let remaining_weight = crate::db::printers::round_tenth(spool.remaining_weight_g);
    let primary_hex = crate::db::printers::primary_color_hex(&spool.color_hexes, spool.color_hex.as_deref());
    let color_list = crate::db::printers::color_list_to_json(&spool.color_hexes);
    let mut values: Vec<&dyn rusqlite::ToSql> = vec![
            &spool.material,
            &spool.manufacturer,
            &spool.color,
            &spool.location,
            &spool.diameter_mm,
            &original_weight,
            &remaining_weight,
            &spool.price,
            &id,
            &primary_hex,
            &spool.kind,
            &color_list,
        ];
    if replace_image {
        values.push(&spool.image_png);
    }
    conn.execute(&sql, values.as_slice())?;
    Ok(())
}

pub fn delete_filament_spool(conn: &Connection, id: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM filament_spools WHERE id = ?1", params![id])?;
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
    fn failed_migration_backup_is_reported_but_startup_continues() {
        let dir = backup_test_dir("migration-warning");
        let path = dir.join("catalog.db");
        let conn = connect(&path).unwrap();
        conn.pragma_update(None, "user_version", super::super::migrations::CURRENT_SCHEMA_VERSION - 1).unwrap();
        drop(conn);
        std::fs::write(dir.join(BACKUP_DIR_NAME), b"blocks backup directory").unwrap();
        let (conn, warning) = connect_with_warning(&path).unwrap();
        assert!(warning);
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, super::super::migrations::CURRENT_SCHEMA_VERSION);
        drop(conn);
        assert!(!connect_with_warning(&path).unwrap().1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn list_file_images_limits_batches_and_ignores_unknown_ids() {
        let conn = connect_in_memory().unwrap();
        let id = test_insert_minimal_file(&conn, "/tmp/images.3mf", None).unwrap();
        conn.execute("UPDATE files SET thumbnail_png = ?1, render_snapshot_png = ?2, custom_image_png = ?3 WHERE id = ?4",
            params![vec![1u8; 10], vec![2u8; 20], vec![3u8; 30], id]).unwrap();
        let images = list_file_images(&conn, &[id, id + 999]).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].id, id);
        assert_eq!(images[0].thumbnail_png, Some(vec![1u8; 10]));
        assert_eq!(images[0].render_snapshot_png, Some(vec![2u8; 20]));
        assert_eq!(images[0].custom_image_png, Some(vec![3u8; 30]));
        assert!(list_file_images(&conn, &[]).unwrap().is_empty());
        assert!(list_file_images(&conn, &vec![id; 200]).is_ok());
        assert!(list_file_images(&conn, &vec![id; 201]).is_err());
    }

    #[test]
    fn list_file_summaries_reports_image_presence_without_blobs() {
        let conn = connect_in_memory().unwrap();
        let file_id = test_insert_minimal_file(&conn, "/tmp/x.3mf", None).unwrap();
        conn.execute(
            "UPDATE files SET render_snapshot_png = ?1, custom_image_png = ?2, thumbnail_png = ?3 WHERE id = ?4",
            params![vec![1u8; 1024], vec![2u8; 1024], vec![3u8; 1024], file_id],
        ).unwrap();

        let summaries = list_file_summaries(&conn).unwrap();

        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].has_thumbnail);
        assert!(summaries[0].has_render_snapshot);
    }

    #[test]
    fn list_file_summaries_includes_creator() {
        // creator must be in the projection, otherwise the creator filter doesn't work.
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
        assert!(!summaries[0].has_thumbnail);
    }

    // `Connection::trace` only takes a function pointer, so the counter lives
    // outside. Thread-local is enough: the callback runs synchronously and every
    // test has its own thread.
    thread_local! {
        static QUERY_TRACE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    // `trace_v2` with `SQLITE_TRACE_STMT` replaces the deprecated `trace`.
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
    #[ignore] // run manually: cargo test --lib -- --ignored list_file_summaries_benchmark
    fn list_file_summaries_benchmark_with_5000_files() {
        let conn = connect_in_memory().unwrap();
        for i in 0..5000 {
            test_insert_minimal_file(&conn, &format!("/tmp/model-{i}.3mf"), None).unwrap();
        }
        let start = std::time::Instant::now();
        let summaries = list_file_summaries(&conn).unwrap();
        let elapsed = start.elapsed();
        assert_eq!(summaries.len(), 5000);
        // Deliberately no timing assertion: only for watching with --nocapture.
        log::debug!(target: "db", "list_file_summaries(5000 rows): {elapsed:?}");
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

        // Folders that are already attached are not re-attached.
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
        // Inconsistent hierarchy as from a crafted backup: parent_id points to A, but
        // the path lies elsewhere.
        insert_folder_with_parent(&conn, "fremd", Some(parent_id), "/x").unwrap();

        let result = update_paths_under_folder(&conn, parent_id, "/tmp/Katalog/A", "/tmp/Katalog/B");
        assert!(result.is_err());
    }

    fn file_path(conn: &Connection, id: i64) -> String {
        conn.query_row("SELECT path FROM files WHERE id = ?1", params![id], |r| r.get(0)).unwrap()
    }

    #[test]
    fn update_paths_under_folder_also_carries_files_that_are_not_assigned_to_the_folder() {
        let conn = connect_in_memory().unwrap();
        let root = Path::new("/tmp/Katalog");
        let deko = root.join("Deko");
        let deko_sub = deko.join("Sub");
        let other = root.join("Anderes");
        let deko_id = insert_folder_with_parent(&conn, "Deko", None, &deko.to_string_lossy()).unwrap();
        insert_folder_with_parent(&conn, "Sub", Some(deko_id), &deko_sub.to_string_lossy()).unwrap();
        let other_id = insert_folder_with_parent(&conn, "Anderes", None, &other.to_string_lossy()).unwrap();

        let assigned = test_insert_minimal_file(&conn, &deko.join("a.3mf").to_string_lossy(), Some(deko_id)).unwrap();
        let loose = test_insert_minimal_file(&conn, &deko.join("loose.3mf").to_string_lossy(), None).unwrap();
        let loose_deep = test_insert_minimal_file(&conn, &deko_sub.join("deep").join("d.3mf").to_string_lossy(), None).unwrap();
        // folder_id points to an unrelated folder although the path lies below Deko.
        let stale = test_insert_minimal_file(&conn, &deko_sub.join("stale.3mf").to_string_lossy(), Some(other_id)).unwrap();
        let trashed = test_insert_minimal_file(&conn, &deko.join("gone.3mf").to_string_lossy(), None).unwrap();
        soft_delete_file(&conn, trashed, Some("/trash/1-gone.3mf"), "2026-01-01T00:00:00Z").unwrap();
        // Same name prefix, but a different folder: must stay untouched.
        let sibling = test_insert_minimal_file(&conn, &root.join("Deko2").join("x.3mf").to_string_lossy(), None).unwrap();
        let sibling_file = test_insert_minimal_file(&conn, &root.join("Deko.3mf").to_string_lossy(), None).unwrap();
        let unrelated = test_insert_minimal_file(&conn, &other.join("o.3mf").to_string_lossy(), Some(other_id)).unwrap();

        let new = root.join("Dekor");
        update_paths_under_folder(&conn, deko_id, &deko.to_string_lossy(), &new.to_string_lossy()).unwrap();

        assert_eq!(file_path(&conn, assigned), new.join("a.3mf").to_string_lossy());
        assert_eq!(file_path(&conn, loose), new.join("loose.3mf").to_string_lossy());
        assert_eq!(file_path(&conn, loose_deep), new.join("Sub").join("deep").join("d.3mf").to_string_lossy());
        assert_eq!(file_path(&conn, stale), new.join("Sub").join("stale.3mf").to_string_lossy());
        assert_eq!(file_path(&conn, trashed), new.join("gone.3mf").to_string_lossy());
        let trash_path: Option<String> =
            conn.query_row("SELECT trash_path FROM files WHERE id = ?1", params![trashed], |r| r.get(0)).unwrap();
        assert_eq!(trash_path.as_deref(), Some("/trash/1-gone.3mf"), "the trashed file itself does not move");
        assert_eq!(file_path(&conn, sibling), root.join("Deko2").join("x.3mf").to_string_lossy());
        assert_eq!(file_path(&conn, sibling_file), root.join("Deko.3mf").to_string_lossy());
        assert_eq!(file_path(&conn, unrelated), other.join("o.3mf").to_string_lossy());
    }

    #[test]
    fn update_paths_under_folder_treats_like_wildcards_in_the_folder_name_literally() {
        let conn = connect_in_memory().unwrap();
        let root = Path::new("/tmp/Katalog");
        let odd = root.join("100%_Deko");
        let lookalike = root.join("100xyDeko");
        let odd_id = insert_folder_with_parent(&conn, "100%_Deko", None, &odd.to_string_lossy()).unwrap();
        let inside = test_insert_minimal_file(&conn, &odd.join("a.3mf").to_string_lossy(), None).unwrap();
        let outside = test_insert_minimal_file(&conn, &lookalike.join("b.3mf").to_string_lossy(), None).unwrap();

        let new = root.join("Neu");
        update_paths_under_folder(&conn, odd_id, &odd.to_string_lossy(), &new.to_string_lossy()).unwrap();

        assert_eq!(file_path(&conn, inside), new.join("a.3mf").to_string_lossy());
        assert_eq!(file_path(&conn, outside), lookalike.join("b.3mf").to_string_lossy());
    }

    #[test]
    fn update_paths_under_folder_follows_non_ascii_folder_names_for_unassigned_files() {
        let conn = connect_in_memory().unwrap();
        let root = Path::new("/tmp/Katalog");
        let old = root.join("Dekoüß");
        let id = insert_folder_with_parent(&conn, "Dekoüß", None, &old.to_string_lossy()).unwrap();
        let loose = test_insert_minimal_file(&conn, &old.join("ä.3mf").to_string_lossy(), None).unwrap();

        let new = root.join("Z");
        update_paths_under_folder(&conn, id, &old.to_string_lossy(), &new.to_string_lossy()).unwrap();

        assert_eq!(file_path(&conn, loose), new.join("ä.3mf").to_string_lossy());
    }

    #[test]
    fn update_paths_under_folder_fails_without_skipping_when_a_new_path_is_already_taken() {
        let conn = connect_in_memory().unwrap();
        let root = Path::new("/tmp/Katalog");
        let old = root.join("Alt");
        let new = root.join("Neu");
        let id = insert_folder_with_parent(&conn, "Alt", None, &old.to_string_lossy()).unwrap();
        test_insert_minimal_file(&conn, &old.join("a.3mf").to_string_lossy(), None).unwrap();
        // A row that already occupies the target path of a.3mf.
        test_insert_minimal_file(&conn, &new.join("a.3mf").to_string_lossy(), None).unwrap();

        let tx = conn.unchecked_transaction().unwrap();
        let result = update_paths_under_folder(&tx, id, &old.to_string_lossy(), &new.to_string_lossy());
        assert!(result.is_err(), "UNIQUE(path) must surface as an error");
        drop(tx); // rollback

        let folder_path: String =
            conn.query_row("SELECT path FROM folders WHERE id = ?1", params![id], |r| r.get(0)).unwrap();
        assert_eq!(folder_path, old.to_string_lossy());
    }

    #[test]
    fn list_files_returns_an_error_for_a_row_with_an_unparseable_file_type() {
        let conn = connect_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        // ignore_check_constraints, so the INSERT succeeds despite the CHECK in schema.sql.
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
        // Normal 3mf/stl rows stay untouched.
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
        // Normal 3mf/stl rows stay untouched.
        let conn = connect_in_memory().unwrap();
        test_insert_minimal_file(&conn, "/tmp/a.3mf", None).unwrap();
        let result = list_file_summaries(&conn);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }
    fn backup_test_dir(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("mfk-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_older_catalog_is_copied_before_it_is_migrated() {
        let dir = backup_test_dir("migbackup-old");
        let db = dir.join("catalog.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch("CREATE TABLE marker (x INTEGER); INSERT INTO marker VALUES (7);").unwrap();
            conn.pragma_update(None, "user_version", 3).unwrap();
        }
        let target = crate::db::migrations_current_version();
        let _ = connect(&db); // migration of this stub may fail; the copy comes first
        let copies: Vec<_> = std::fs::read_dir(dir.join(BACKUP_DIR_NAME)).unwrap()
            .map(|entry| entry.unwrap().path()).collect();
        assert_eq!(copies.len(), 1);
        let copy = &copies[0];
        let name = copy.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("Katalog-Sicherung_"));
        assert!(name.ends_with(&format!("_vor-Schema-Migration_3_auf_{target}.db")));
        let saved = Connection::open(copy).unwrap();
        let x: i64 = saved.query_row("SELECT x FROM marker", [], |r| r.get(0)).unwrap();
        assert_eq!(x, 7, "the copy must hold the catalog as it was before the migration");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_or_current_catalog_is_not_copied() {
        let dir = backup_test_dir("migbackup-new");
        let fresh = dir.join("fresh.db");
        let _ = connect(&fresh).unwrap();
        assert!(!dir.join(BACKUP_DIR_NAME).exists(), "a brand-new catalog has nothing to protect");
        // Opened again at the current version: still no copy.
        let _ = connect(&fresh).unwrap();
        assert!(!dir.join(BACKUP_DIR_NAME).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

}
