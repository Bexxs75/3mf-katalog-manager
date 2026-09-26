use rusqlite::{Connection, OptionalExtension};
use super::error::DbError;

type MigrationFn = fn(&Connection) -> Result<(), DbError>;

/// `Simple` steps run in a transaction opened by the runner. `Rebuild` steps
/// (CHECK change via table rebuild) get the full connection and manage pragma,
/// transaction and user_version themselves: `PRAGMA foreign_keys` is a no-op
/// inside a transaction.
enum MigrationStep {
    Simple(MigrationFn),
    Rebuild(fn(&mut Connection, i64) -> Result<(), DbError>),
}

/// Migrations are additive: never change or renumber shipped steps.
const MIGRATIONS: &[MigrationStep] = &[
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE filament_spools ADD COLUMN image_png BLOB")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE filament_spools ADD COLUMN location TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN print_status TEXT NOT NULL DEFAULT 'not_printed' CHECK (print_status IN ('not_printed', 'printed'))")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN last_viewed_at TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN creator TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE folders ADD COLUMN parent_id INTEGER REFERENCES folders(id) ON DELETE CASCADE")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE folders ADD COLUMN path TEXT")),
    MigrationStep::Simple(|c| exec(c, "CREATE UNIQUE INDEX IF NOT EXISTS idx_folders_path ON folders (path)")),
    MigrationStep::Simple(|c| exec(c, "UPDATE files SET creator = (SELECT value FROM file_metadata WHERE file_id = files.id AND label = 'Designer') WHERE creator IS NULL")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN content_hash TEXT")),
    MigrationStep::Simple(|c| exec(c, "CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files (content_hash)")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN render_snapshot_png BLOB")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN custom_image_png BLOB")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN source_url TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN queue_position INTEGER")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN plate_count INTEGER")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN slice_info_json TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN deleted_at TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN trash_path TEXT")),
    // Machine-local slicer registry, never adopted on restore (see replace_catalog_db).
    MigrationStep::Simple(|c| exec(c, "CREATE TABLE IF NOT EXISTS registered_slicers (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        executable_path TEXT NOT NULL UNIQUE,
        is_auto_detected INTEGER NOT NULL DEFAULT 0
    )")),
    // 'stp' in the file_type CHECK; changing a CHECK needs a rebuild.
    MigrationStep::Rebuild(add_stp_to_file_type_check),
    // 'obj' as a separate step, because the STP step has already shipped.
    MigrationStep::Rebuild(add_obj_to_file_type_check),
    // Printers, units and the slot columns of spools. The unique index lives only
    // here, not in schema.sql: schema.sql runs on old DBs before the migrations,
    // where the columns don't exist yet.
    MigrationStep::Simple(|c| exec(c, "CREATE TABLE IF NOT EXISTS printers (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        position INTEGER NOT NULL DEFAULT 0
    )")),
    MigrationStep::Simple(|c| exec(c, "CREATE TABLE IF NOT EXISTS material_units (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        printer_id INTEGER NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        kind TEXT NOT NULL CHECK (kind IN ('bambu_ams', 'bambu_ams_lite', 'bambu_ams_ht', 'creality_cfs',
                                           'prusa_mmu3', 'anycubic_ace', 'external', 'custom')),
        slot_count INTEGER NOT NULL CHECK (slot_count BETWEEN 1 AND 16),
        bambu_ams_index INTEGER CHECK (bambu_ams_index BETWEEN 0 AND 3),
        position INTEGER NOT NULL DEFAULT 0
    )")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE filament_spools ADD COLUMN unit_id INTEGER REFERENCES material_units(id) ON DELETE SET NULL")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE filament_spools ADD COLUMN slot_index INTEGER")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE filament_spools ADD COLUMN home_location TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE filament_spools ADD COLUMN color_hex TEXT")),
    MigrationStep::Simple(|c| exec(c, "CREATE UNIQUE INDEX IF NOT EXISTS idx_filament_spools_slot ON filament_spools (unit_id, slot_index) WHERE unit_id IS NOT NULL")),
    MigrationStep::Simple(super::printers::backfill_color_hex),
    // Kind per entry; for 'resin' the weight columns hold milliliters.
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE filament_spools ADD COLUMN kind TEXT NOT NULL DEFAULT 'filament' CHECK (kind IN ('filament', 'resin'))")),
    // Printer connection: settings, one connection per printer, fetched prints.
    MigrationStep::Simple(|c| exec(c, "CREATE TABLE IF NOT EXISTS app_settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    )")),
    MigrationStep::Simple(|c| exec(c, "CREATE TABLE IF NOT EXISTS printer_connections (
        printer_id INTEGER PRIMARY KEY REFERENCES printers(id) ON DELETE CASCADE,
        kind TEXT NOT NULL CHECK (kind IN ('moonraker')),
        address TEXT NOT NULL,
        base_url TEXT,
        remote_version TEXT,
        connected_since REAL NOT NULL,
        last_synced_at REAL,
        last_error TEXT,
        error_since REAL,
        paused INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0, 1))
    )")),
    MigrationStep::Simple(|c| exec(c, "CREATE TABLE IF NOT EXISTS printer_jobs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        printer_id INTEGER NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
        remote_id TEXT NOT NULL,
        file_name TEXT NOT NULL,
        outcome TEXT NOT NULL CHECK (outcome IN ('completed', 'partial')),
        raw_status TEXT NOT NULL,
        ended_at REAL NOT NULL,
        print_duration_s REAL NOT NULL,
        used_mm REAL NOT NULL,
        slicer_total_mm REAL,
        slicer_weight_g REAL,
        material TEXT,
        thumbnail_path TEXT,
        state TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'confirmed', 'ignored')),
        booked_spool_id INTEGER REFERENCES filament_spools(id) ON DELETE SET NULL,
        booked_file_id INTEGER REFERENCES files(id) ON DELETE SET NULL,
        booked_g REAL,
        decided_at TEXT,
        UNIQUE (printer_id, remote_id)
    )")),
    // Adds `kind` for development DBs that reached 34 with the old numbering
    // (printer connection before `kind`). Otherwise a no-op thanks to the tolerated
    // "duplicate column name".
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE filament_spools ADD COLUMN kind TEXT NOT NULL DEFAULT 'filament' CHECK (kind IN ('filament', 'resin'))")),
    // Resin printers: `printers.kind` and 'resin_vat' in the CHECK of `material_units.kind` (rebuild, one transaction).
    MigrationStep::Rebuild(add_resin_printers),
    // Saved filters have had no UI since the GUI redesign; the table goes away.
    MigrationStep::Simple(|c| exec(c, "DROP TABLE IF EXISTS saved_filters")),
    // Printer clock minus local clock, measured on every test and sync (printers without NTP).
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE printer_connections ADD COLUMN clock_offset_s REAL NOT NULL DEFAULT 0")),
];

/// Derived from [`MIGRATIONS`] so the two can never drift apart.
pub const CURRENT_SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Schema version right before the first printer step, for the migration test in `db/printers.rs`.
#[cfg(test)]
pub(crate) const FIRST_PRINTER_MIGRATION_VERSION: i64 = 23;

/// Schema version after the `kind` step shipped in v0.13.1 (resin). Must stay
/// 32: that's what every database created with v0.13.1 contains.
#[cfg(test)]
pub(crate) const KIND_MIGRATION_VERSION: i64 = 32;

/// Schema version after the resin printer step.
#[cfg(test)]
pub(crate) const RESIN_PRINTER_MIGRATION_VERSION: i64 = 37;

/// Schema version after dropping `saved_filters`.
#[cfg(test)]
const DROP_SAVED_FILTERS_MIGRATION_VERSION: i64 = 38;

/// Schema version after adding `printer_connections.clock_offset_s`.
#[cfg(test)]
const CLOCK_OFFSET_MIGRATION_VERSION: i64 = 39;

/// Runs a single statement. The only tolerated error is "duplicate column name"
/// (column already exists); everything else propagates.
fn exec(conn: &Connection, sql: &str) -> Result<(), DbError> {
    match conn.execute(sql, []) {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("duplicate column name") => Ok(()),
        Err(e) => Err(DbError::from(e)),
    }
}

/// Extends the `file_type` CHECK of `files`. SQLite can't alter CHECKs, hence the
/// recommended rebuild (create-copy-drop-rename, see
/// https://www.sqlite.org/lang_altertable.html). Every new file type gets its own
/// step; shipped steps are never changed.
///
/// CRITICAL: the child tables of `files` use `ON DELETE CASCADE`. With
/// `foreign_keys` on, `DROP TABLE files` deletes all tags, metadata, materials
/// and assignments. The pragma can only be switched outside a transaction, so
/// this step manages both itself.
fn rebuild_files_table_with_check(
    conn: &mut Connection,
    step_version: i64,
    allowed_file_types: &[&str],
) -> Result<(), DbError> {
    let check_values = allowed_file_types
        .iter()
        .map(|t| format!("'{t}'"))
        .collect::<Vec<_>>()
        .join(", ");

    // Fresh DBs already have the CHECK (schema.sql): only bump user_version.
    let current_sql: String = conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'files'",
        [],
        |r| r.get(0),
    )?;
    let already_migrated = allowed_file_types
        .iter()
        .all(|t| current_sql.contains(&format!("'{t}'")));

    // Remember the previous value: tests also call run_migrations without foreign_keys=ON.
    let previously_enabled: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    conn.pragma_update(None, "foreign_keys", false)?;

    let result = (|| -> Result<(), DbError> {
        let tx = conn.transaction()?;
        if !already_migrated {
            // Leftovers from the removed Google Drive integration violate the stricter
            // `origin` CHECK and made the copy fail (the app wouldn't start). The files are
            // local, so keep them as local entries.
            tx.execute(
                "UPDATE files SET origin = 'local', sync_status = 'local-only', cloud_id = NULL
                 WHERE origin <> 'local'",
                [],
            )?;
            tx.execute_batch(&format!(
                "CREATE TABLE files_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    path TEXT NOT NULL UNIQUE,
                    file_type TEXT NOT NULL CHECK (file_type IN ({check_values})),
                    folder_id INTEGER REFERENCES folders (id) ON DELETE SET NULL,
                    origin TEXT NOT NULL DEFAULT 'local'
                        CHECK (origin IN ('local')),
                    sync_status TEXT NOT NULL DEFAULT 'local-only'
                        CHECK (sync_status IN ('synced', 'outdated', 'local-only', 'cloud-only')),
                    cloud_id TEXT,
                    file_size_bytes INTEGER NOT NULL,
                    dimension_x_mm REAL,
                    dimension_y_mm REAL,
                    dimension_z_mm REAL,
                    volume_cm3 REAL,
                    object_count INTEGER,
                    thumbnail_png BLOB,
                    imported_at TEXT NOT NULL,
                    file_modified_at TEXT,
                    print_status TEXT NOT NULL DEFAULT 'not_printed'
                        CHECK (print_status IN ('not_printed', 'printed')),
                    last_viewed_at TEXT,
                    creator TEXT,
                    content_hash TEXT,
                    render_snapshot_png BLOB,
                    custom_image_png BLOB,
                    source_url TEXT,
                    queue_position INTEGER,
                    favorite INTEGER NOT NULL DEFAULT 0,
                    plate_count INTEGER,
                    slice_info_json TEXT,
                    deleted_at TEXT,
                    trash_path TEXT
                );
                INSERT INTO files_new SELECT * FROM files;
                DROP TABLE files;
                ALTER TABLE files_new RENAME TO files;
                CREATE INDEX IF NOT EXISTS idx_files_folder_id ON files (folder_id);
                CREATE INDEX IF NOT EXISTS idx_files_file_type ON files (file_type);
                CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files (content_hash);"
            ))?;
            // Mandatory check per the SQLite docs: no child row points nowhere.
            let has_violation = tx
                .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
                .optional()?
                .is_some();
            if has_violation {
                return Err(DbError::Other(
                    "file_type-CHECK-Migration: foreign_key_check fand verwaiste Referenzen nach dem Rebuild".into(),
                ));
            }
        }
        tx.pragma_update(None, "user_version", step_version)?;
        tx.commit()?;
        Ok(())
    })();

    // Always restore, even after an error.
    conn.pragma_update(None, "foreign_keys", previously_enabled)?;
    result
}

fn add_stp_to_file_type_check(conn: &mut Connection, step_version: i64) -> Result<(), DbError> {
    rebuild_files_table_with_check(conn, step_version, &["3mf", "stl", "stp"])
}

fn add_obj_to_file_type_check(conn: &mut Connection, step_version: i64) -> Result<(), DbError> {
    rebuild_files_table_with_check(conn, step_version, &["3mf", "stl", "stp", "obj"])
}

/// `printers.kind` and 'resin_vat' in the CHECK of `material_units.kind`, via a
/// rebuild like `rebuild_files_table_with_check` and with the same trap:
/// `filament_spools.unit_id` has `ON DELETE SET NULL`, so with `foreign_keys` on
/// the rebuild would throw every spool out of its slot.
fn add_resin_printers(conn: &mut Connection, step_version: i64) -> Result<(), DbError> {
    // Fresh DBs already have 'resin_vat'. Missing tables only occur in tests with
    // partial DBs; there's nothing to do there.
    let table_sql = |conn: &Connection, name: &str| -> Result<Option<String>, DbError> {
        Ok(conn
            .query_row("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1", [name], |r| r.get(0))
            .optional()?)
    };
    let has_printers = table_sql(conn, "printers")?.is_some();
    let already_migrated = table_sql(conn, "material_units")?.is_none_or(|sql| sql.contains("'resin_vat'"));

    let previously_enabled: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    conn.pragma_update(None, "foreign_keys", false)?;

    let result = (|| -> Result<(), DbError> {
        let tx = conn.transaction()?;
        // `exec` tolerates "duplicate column name" (fresh DB or already present);
        // everything else propagates.
        if has_printers {
            exec(
                &tx,
                "ALTER TABLE printers ADD COLUMN kind TEXT NOT NULL DEFAULT 'filament' CHECK (kind IN ('filament', 'resin'))",
            )?;
        }
        if !already_migrated {
            // Explicit columns instead of `SELECT *`: the column order of an imported
            // backup need not match ours.
            tx.execute_batch(
                "CREATE TABLE material_units_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    printer_id INTEGER NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL CHECK (kind IN ('bambu_ams', 'bambu_ams_lite', 'bambu_ams_ht', 'creality_cfs',
                                                       'prusa_mmu3', 'anycubic_ace', 'external', 'custom', 'resin_vat')),
                    slot_count INTEGER NOT NULL CHECK (slot_count BETWEEN 1 AND 16),
                    bambu_ams_index INTEGER CHECK (bambu_ams_index BETWEEN 0 AND 3),
                    position INTEGER NOT NULL DEFAULT 0
                );
                INSERT INTO material_units_new (id, printer_id, name, kind, slot_count, bambu_ams_index, position)
                    SELECT id, printer_id, name, kind, slot_count, bambu_ams_index, position FROM material_units;
                DROP TABLE material_units;
                ALTER TABLE material_units_new RENAME TO material_units;",
            )?;
            // Mandatory check per the SQLite docs, limited to the tables the rebuild
            // touches: no spool points to a missing unit, no unit to a missing printer.
            for table in ["filament_spools", "material_units"] {
                let has_violation = tx
                    .query_row(&format!("PRAGMA foreign_key_check({table})"), [], |_| Ok(()))
                    .optional()?
                    .is_some();
                if has_violation {
                    return Err(DbError::Other(format!(
                        "Resin-Drucker-Migration: foreign_key_check fand verwaiste Referenzen in {table}"
                    )));
                }
            }
        }
        tx.pragma_update(None, "user_version", step_version)?;
        tx.commit()?;
        Ok(())
    })();

    conn.pragma_update(None, "foreign_keys", previously_enabled)?;
    result
}

/// Migrates `conn` from its current `PRAGMA user_version` to
/// [`CURRENT_SCHEMA_VERSION`]. Each step runs on its own; if one fails,
/// `user_version` is NOT bumped (the error propagates immediately, no partial
/// progress is silently kept).
pub fn run_migrations(conn: &mut Connection) -> Result<(), DbError> {
    run_migrations_with(conn, MIGRATIONS, CURRENT_SCHEMA_VERSION)
}

/// Core of [`run_migrations`] with its own step list, so tests can inject failing steps.
fn run_migrations_with(conn: &mut Connection, migrations: &[MigrationStep], target_version: i64) -> Result<(), DbError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    // A DB from a newer app version would otherwise skip every step and silently
    // count as compatible.
    if current > target_version {
        return Err(DbError::Other(format!(
            "Datenbank-Schemaversion {current} ist neuer als die von dieser App-Version unterstuetzte Version {target_version}"
        )));
    }
    for (index, migration) in migrations.iter().enumerate() {
        let step_version = (index + 1) as i64;
        if step_version <= current || step_version > target_version {
            continue;
        }
        match migration {
            MigrationStep::Simple(f) => {
                // Statement and user_version bump in the same transaction: never an applied
                // but unrecorded migration.
                let tx = conn.transaction()?;
                f(&tx)?;
                tx.pragma_update(None, "user_version", step_version)?;
                tx.commit()?;
            }
            // Manages transaction and user_version itself (see MigrationStep).
            MigrationStep::Rebuild(f) => f(conn, step_version)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migrating_a_fresh_in_memory_db_reaches_current_schema_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn migrating_twice_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap(); // must not run ALTER TABLE again / must not fail
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn migration_drops_the_saved_filters_table_of_older_databases() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE saved_filters (id INTEGER PRIMARY KEY, name TEXT NOT NULL, sort TEXT NOT NULL);
             INSERT INTO saved_filters (name, sort) VALUES ('Vases', 'name');
             INSERT INTO tags (name, color_hue) VALUES ('keep', 0);",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", DROP_SAVED_FILTERS_MIGRATION_VERSION - 1).unwrap();

        run_migrations(&mut conn).unwrap();

        let tables: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE name = 'saved_filters'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tables, 0);
        let tags: i64 = conn.query_row("SELECT COUNT(*) FROM tags WHERE name = 'keep'", [], |r| r.get(0)).unwrap();
        assert_eq!(tags, 1);
    }

    #[test]
    fn migrating_a_genuinely_partially_upgraded_db_only_applies_remaining_steps() {
        // Real base schema (without migration columns), run only the first 3 steps;
        // run_migrations must catch up on the rest.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        for migration in &MIGRATIONS[0..3] {
            match migration {
                MigrationStep::Simple(f) => f(&conn).unwrap(),
                MigrationStep::Rebuild(_) => unreachable!("erste 3 MIGRATIONS-Eintraege sind alle Simple"),
            }
        }
        conn.pragma_update(None, "user_version", 3).unwrap();

        run_migrations(&mut conn).unwrap();

        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        // Column from a late step (trash_path, step 20).
        conn.query_row("SELECT COUNT(trash_path) FROM files", [], |_| Ok(())).unwrap();
        // Step 6 (folders.parent_id) ran as well, so it continued from step 4.
        conn.query_row("SELECT COUNT(parent_id) FROM folders", [], |_| Ok(())).unwrap();
    }

    #[test]
    fn a_failing_migration_step_does_not_advance_user_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        // Make the database read-only to force a real I/O/write error.
        conn.execute("PRAGMA query_only = ON", []).unwrap();
        let result = run_migrations(&mut conn);
        assert!(result.is_err(), "a real SQLite error must propagate, not be swallowed");
    }

    #[test]
    fn a_failing_step_does_not_leave_a_half_applied_schema_change_committed() {
        // The error must happen in the same closure as the ALTER TABLE; a separate
        // second step would have its own transaction.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        let failing_migrations: &[MigrationStep] = &[
            MigrationStep::Simple(|c| {
                exec(c, "ALTER TABLE files ADD COLUMN test_marker_column TEXT")?;
                Err(DbError::Other("simulierter Fehler nach ALTER TABLE".into()))
            }),
        ];
        let result = run_migrations_with(&mut conn, failing_migrations, 1);
        assert!(result.is_err());
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, 0, "user_version darf nicht erhoeht werden, wenn der Schritt insgesamt fehlschlaegt");
        // Rolled back: the column must not exist.
        let column_exists = conn
            .query_row("SELECT test_marker_column FROM files LIMIT 0", [], |_| Ok(()))
            .is_ok();
        assert!(!column_exists, "ALTER TABLE muss mit user_version zusammen zurueckgerollt werden");
    }

    /// Cascade trap from `rebuild_files_table_with_check`: old schema, foreign_keys
    /// on (as in repository::init), rows in every child table. None of them may be
    /// missing after the migration.
    #[test]
    fn migrating_preserves_all_child_rows_and_widens_the_file_type_check() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE folders (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);
             CREATE TABLE tags (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, color_hue INTEGER NOT NULL);
             CREATE TABLE files (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL,
                 path TEXT NOT NULL UNIQUE,
                 file_type TEXT NOT NULL CHECK (file_type IN ('3mf', 'stl')),
                 folder_id INTEGER REFERENCES folders (id) ON DELETE SET NULL,
                 origin TEXT NOT NULL DEFAULT 'local' CHECK (origin IN ('local')),
                 sync_status TEXT NOT NULL DEFAULT 'local-only' CHECK (sync_status IN ('synced', 'outdated', 'local-only', 'cloud-only')),
                 cloud_id TEXT,
                 file_size_bytes INTEGER NOT NULL,
                 dimension_x_mm REAL, dimension_y_mm REAL, dimension_z_mm REAL,
                 volume_cm3 REAL, object_count INTEGER, thumbnail_png BLOB,
                 imported_at TEXT NOT NULL, file_modified_at TEXT
             );
             CREATE TABLE file_tags (
                 file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                 tag_id INTEGER NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
                 PRIMARY KEY (file_id, tag_id)
             );
             CREATE TABLE file_metadata (
                 file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                 label TEXT NOT NULL, value TEXT NOT NULL,
                 PRIMARY KEY (file_id, label)
             );
             CREATE TABLE file_materials (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                 name TEXT NOT NULL, display_color TEXT
             );
             CREATE TABLE collections (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, created_at TEXT NOT NULL);
             CREATE TABLE collection_files (
                 collection_id INTEGER NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
                 file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                 position INTEGER NOT NULL,
                 UNIQUE (collection_id, file_id)
             );
             CREATE TABLE print_log (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                 printed_at TEXT NOT NULL, note TEXT, photo_png BLOB, created_at TEXT NOT NULL
             );

             INSERT INTO folders (id, name) VALUES (1, 'Root');
             INSERT INTO tags (id, name, color_hue) VALUES (1, 'Deko', 30);
             INSERT INTO files (id, name, path, file_type, folder_id, file_size_bytes, imported_at)
                 VALUES (1, 'a.3mf', '/tmp/a.3mf', '3mf', 1, 100, '2020-01-01T00:00:00Z');
             INSERT INTO files (id, name, path, file_type, folder_id, file_size_bytes, imported_at)
                 VALUES (2, 'b.stl', '/tmp/b.stl', 'stl', 1, 200, '2020-01-01T00:00:00Z');
             INSERT INTO file_tags (file_id, tag_id) VALUES (1, 1);
             INSERT INTO file_metadata (file_id, label, value) VALUES (1, 'Designer', 'Alice');
             INSERT INTO file_materials (id, file_id, name, display_color) VALUES (1, 1, 'PLA', '#ffffff');
             INSERT INTO collections (id, name, created_at) VALUES (1, 'Favoriten', '2020-01-01T00:00:00Z');
             INSERT INTO collection_files (collection_id, file_id, position) VALUES (1, 1, 0);
             INSERT INTO print_log (id, file_id, printed_at, created_at) VALUES (1, 2, '2020-01-02T00:00:00Z', '2020-01-02T00:00:00Z');",
        )
        .unwrap();
        // As in repository::init: SCHEMA_SQL first (creates missing tables).
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        // As in repository::init; the trap is only armed with foreign_keys=ON.
        conn.pragma_update(None, "foreign_keys", true).unwrap();

        run_migrations(&mut conn).unwrap();

        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);

        let fk_enabled: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert!(fk_enabled, "foreign_keys muss nach der Migration wieder eingeschaltet sein");

        let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(count("SELECT COUNT(*) FROM files"), 2, "beide Dateien muessen den Rebuild ueberleben");
        assert_eq!(count("SELECT COUNT(*) FROM file_tags WHERE file_id = 1 AND tag_id = 1"), 1, "Tag-Zuordnung darf nicht kaskadierend geloescht worden sein");
        assert_eq!(count("SELECT COUNT(*) FROM file_metadata WHERE file_id = 1 AND label = 'Designer'"), 1, "Metadaten duerfen nicht verloren gehen");
        assert_eq!(count("SELECT COUNT(*) FROM file_materials WHERE file_id = 1"), 1, "Materialien duerfen nicht verloren gehen");
        assert_eq!(count("SELECT COUNT(*) FROM collection_files WHERE collection_id = 1 AND file_id = 1"), 1, "Collection-Zuordnung darf nicht verloren gehen");
        assert_eq!(count("SELECT COUNT(*) FROM print_log WHERE file_id = 2"), 1, "Druck-Log darf nicht verloren gehen");

        // The ids are kept, otherwise the child rows would be orphaned.
        let file1_path: String = conn.query_row("SELECT path FROM files WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(file1_path, "/tmp/a.3mf");

        conn.execute(
            "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('c.stp', '/tmp/c.stp', 'stp', 1, '2020-01-01T00:00:00Z')",
            [],
        )
        .expect("file_type='stp' muss nach der Migration erlaubt sein");

        // Second rebuild (obj) on the already rebuilt table.
        conn.execute(
            "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('e.obj', '/tmp/e.obj', 'obj', 1, '2020-01-01T00:00:00Z')",
            [],
        )
        .expect("file_type='obj' muss nach der Migration erlaubt sein");
        assert_eq!(count("SELECT COUNT(*) FROM file_tags WHERE file_id = 1 AND tag_id = 1"), 1, "Tag-Zuordnung darf auch den zweiten Rebuild ueberleben");
        assert_eq!(count("SELECT COUNT(*) FROM print_log WHERE file_id = 2"), 1, "Druck-Log darf auch den zweiten Rebuild ueberleben");

        let rejected = conn.execute(
            "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('d.xyz', '/tmp/d.xyz', 'xyz', 1, '2020-01-01T00:00:00Z')",
            [],
        );
        assert!(rejected.is_err(), "ein weiterhin ungueltiger file_type muss am CHECK scheitern");

        let index_names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'files'")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(index_names.contains(&"idx_files_folder_id".to_string()));
        assert!(index_names.contains(&"idx_files_file_type".to_string()));
    }

    /// Catalogs from the Google Drive era can contain `origin = 'gdrive'`; the
    /// rebuild with `CHECK (origin IN ('local'))` otherwise stopped the app from starting.
    #[test]
    fn migrating_a_catalog_with_leftover_cloud_rows_turns_them_into_local_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE folders (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);
             CREATE TABLE tags (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, color_hue INTEGER NOT NULL);
             CREATE TABLE files (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL,
                 path TEXT NOT NULL UNIQUE,
                 file_type TEXT NOT NULL CHECK (file_type IN ('3mf', 'stl')),
                 folder_id INTEGER REFERENCES folders (id) ON DELETE SET NULL,
                 origin TEXT NOT NULL DEFAULT 'local'
                     CHECK (origin IN ('local', 'gdrive', 'onedrive', 'dropbox', 'proton')),
                 sync_status TEXT NOT NULL DEFAULT 'local-only' CHECK (sync_status IN ('synced', 'outdated', 'local-only', 'cloud-only')),
                 cloud_id TEXT,
                 file_size_bytes INTEGER NOT NULL,
                 dimension_x_mm REAL, dimension_y_mm REAL, dimension_z_mm REAL,
                 volume_cm3 REAL, object_count INTEGER, thumbnail_png BLOB,
                 imported_at TEXT NOT NULL, file_modified_at TEXT
             );
             CREATE TABLE file_tags (
                 file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                 tag_id INTEGER NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
                 PRIMARY KEY (file_id, tag_id)
             );

             INSERT INTO tags (id, name, color_hue) VALUES (1, 'Deko', 30);
             INSERT INTO files (id, name, path, file_type, origin, sync_status, cloud_id, file_size_bytes, imported_at)
                 VALUES (1, 'cloud.3mf', '/home/u/.cache/app/abc.3mf', '3mf', 'gdrive', 'synced', 'abc', 100, '2020-01-01T00:00:00Z');
             INSERT INTO files (id, name, path, file_type, file_size_bytes, imported_at)
                 VALUES (2, 'lokal.stl', '/tmp/lokal.stl', 'stl', 200, '2020-01-01T00:00:00Z');
             INSERT INTO file_tags (file_id, tag_id) VALUES (1, 1);",
        )
        .unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();

        run_migrations(&mut conn).expect("Migration darf an alten Cloud-Zeilen nicht scheitern");

        let (origin, sync_status, cloud_id): (String, String, Option<String>) = conn
            .query_row("SELECT origin, sync_status, cloud_id FROM files WHERE id = 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(origin, "local");
        assert_eq!(sync_status, "local-only");
        assert_eq!(cloud_id, None);

        let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(count("SELECT COUNT(*) FROM files"), 2, "keine Zeile darf verloren gehen");
        assert_eq!(count("SELECT COUNT(*) FROM file_tags WHERE file_id = 1"), 1, "Tags der ehemaligen Cloud-Datei bleiben erhalten");
    }

    #[test]
    fn a_database_from_a_newer_schema_version_is_rejected() {
        // A DB from a newer app version must be rejected.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1).unwrap();

        let result = run_migrations(&mut conn);

        assert!(result.is_err(), "eine Datenbank mit einer neueren Schemaversion als der App muss abgelehnt werden");
    }

    #[test]
    fn the_kind_migration_marks_existing_spools_as_filament_and_checks_the_value() {
        let mut conn = Connection::open_in_memory().unwrap();
        // State before the kind step: filament_spools without the kind column.
        conn.execute_batch(
            "CREATE TABLE filament_spools (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                material TEXT NOT NULL, manufacturer TEXT, color TEXT, location TEXT,
                diameter_mm REAL NOT NULL, original_weight_g INTEGER NOT NULL,
                remaining_weight_g INTEGER NOT NULL, price REAL, image_png BLOB,
                created_at TEXT NOT NULL, unit_id INTEGER, slot_index INTEGER,
                home_location TEXT, color_hex TEXT
            );
            INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                VALUES ('PLA', 1.75, 1000, 600, '2026-01-01');",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", KIND_MIGRATION_VERSION - 1).unwrap();

        run_migrations(&mut conn).unwrap();

        let kind: String = conn.query_row("SELECT kind FROM filament_spools", [], |r| r.get(0)).unwrap();
        assert_eq!(kind, "filament");
        let bad = conn.execute(
            "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, kind)
             VALUES ('X', 1.75, 1, 1, '2026-01-01', 'pla')",
            [],
        );
        assert!(bad.is_err(), "CHECK erlaubt nur filament/resin");
    }

    /// `kind` stays at 32 (as shipped in v0.13.1), the printer connection follows.
    #[test]
    fn the_kind_step_stays_at_the_shipped_position_32() {
        assert_eq!(KIND_MIGRATION_VERSION, 32);
        assert_eq!(CURRENT_SCHEMA_VERSION, CLOCK_OFFSET_MIGRATION_VERSION);
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        // Only run the steps up to and including 32.
        run_migrations_with(&mut conn, &MIGRATIONS[..KIND_MIGRATION_VERSION as usize], KIND_MIGRATION_VERSION).unwrap();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, 32);
    }

    /// Development DBs with the old numbering were at 34, without `kind`. Step 36
    /// adds the column, also via `init`.
    #[test]
    fn a_master_dev_db_at_version_34_without_kind_gets_the_column_on_init() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch(
            "DROP INDEX IF EXISTS idx_filament_spools_slot;
             DROP TABLE filament_spools;
             CREATE TABLE filament_spools (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                material TEXT NOT NULL, manufacturer TEXT, color TEXT, location TEXT,
                diameter_mm REAL NOT NULL, original_weight_g REAL NOT NULL,
                remaining_weight_g REAL NOT NULL, price REAL, image_png BLOB,
                created_at TEXT NOT NULL,
                unit_id INTEGER REFERENCES material_units(id) ON DELETE SET NULL,
                slot_index INTEGER, home_location TEXT, color_hex TEXT
             );
             INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                VALUES ('PLA', 1.75, 1000, 612.4, '2026-09-24');",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 34).unwrap();

        crate::db::repository::init(&mut conn).unwrap();

        let kind: String = conn.query_row("SELECT kind FROM filament_spools", [], |r| r.get(0)).unwrap();
        assert_eq!(kind, "filament");
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        let bad = conn.execute(
            "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, kind)
             VALUES ('X', 1.75, 1, 1, '2026-01-01', 'pla')",
            [],
        );
        assert!(bad.is_err(), "auch der nachgeholte Schritt bringt den CHECK mit");
    }

    /// State before step 37: printers without `kind`, old unit CHECK, one spool in a slot.
    fn db_before_resin_printers() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch(
            "DROP INDEX IF EXISTS idx_filament_spools_slot;
             DROP TABLE printer_jobs;
             DROP TABLE printer_connections;
             DROP TABLE material_units;
             DROP TABLE printers;
             CREATE TABLE printers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                position INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE material_units (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                printer_id INTEGER NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                kind TEXT NOT NULL CHECK (kind IN ('bambu_ams', 'bambu_ams_lite', 'bambu_ams_ht', 'creality_cfs',
                                                   'prusa_mmu3', 'anycubic_ace', 'external', 'custom')),
                slot_count INTEGER NOT NULL CHECK (slot_count BETWEEN 1 AND 16),
                bambu_ams_index INTEGER CHECK (bambu_ams_index BETWEEN 0 AND 3),
                position INTEGER NOT NULL DEFAULT 0
             );
             CREATE UNIQUE INDEX idx_filament_spools_slot ON filament_spools (unit_id, slot_index) WHERE unit_id IS NOT NULL;
             INSERT INTO printers (id, name, position) VALUES (1, 'X1C', 0), (2, 'A1', 1);
             INSERT INTO material_units (id, printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (10, 1, 'AMS A', 'bambu_ams', 4, 0, 0), (11, 2, 'Spulenhalter', 'external', 1, NULL, 0);
             INSERT INTO filament_spools (id, material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index, home_location)
                 VALUES (100, 'PLA', 1.75, 1000, 600, '2026-09-01', 10, 2, 'Regal 2'),
                        (101, 'PETG', 1.75, 1000, 900, '2026-09-01', NULL, NULL, NULL);",
        )
        .unwrap();
        // Recreate printer_connections/printer_jobs as in the real state before 37.
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        conn.pragma_update(None, "user_version", RESIN_PRINTER_MIGRATION_VERSION - 1).unwrap();
        conn
    }

    #[test]
    fn the_resin_printer_step_keeps_units_and_loaded_spools_and_marks_printers_as_filament() {
        let mut conn = db_before_resin_printers();
        // As in repository::init: foreign_keys is on BEFORE the migrations - otherwise
        // the cascade trap (ON DELETE SET NULL) wouldn't be armed.
        conn.pragma_update(None, "foreign_keys", true).unwrap();

        run_migrations(&mut conn).unwrap();

        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        let fk: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert!(fk, "foreign_keys muss danach wieder an sein");

        let kinds: Vec<String> = conn
            .prepare("SELECT kind FROM printers ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(kinds, vec!["filament", "filament"]);
        let units: i64 = conn.query_row("SELECT COUNT(*) FROM material_units", [], |r| r.get(0)).unwrap();
        assert_eq!(units, 2);
        let slot: (Option<i64>, Option<i64>, Option<String>) = conn
            .query_row("SELECT unit_id, slot_index, home_location FROM filament_spools WHERE id = 100", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(slot, (Some(10), Some(2), Some("Regal 2".into())), "Spule bleibt im Fach");
        let ams_index: Option<i64> =
            conn.query_row("SELECT bambu_ams_index FROM material_units WHERE id = 10", [], |r| r.get(0)).unwrap();
        assert_eq!(ams_index, Some(0));

        // New values allowed, invalid ones still rejected.
        conn.execute("INSERT INTO printers (id, name, kind, position) VALUES (3, 'Saturn', 'resin', 2)", []).unwrap();
        conn.execute(
            "INSERT INTO material_units (printer_id, name, kind, slot_count, position) VALUES (3, 'Harzwanne', 'resin_vat', 1, 0)",
            [],
        )
        .expect("resin_vat ist nach Schritt 37 erlaubt");
        assert!(conn.execute("INSERT INTO printers (name, kind, position) VALUES ('X', 'toast', 3)", []).is_err());
        assert!(conn
            .execute("INSERT INTO material_units (printer_id, name, kind, slot_count, position) VALUES (3, 'X', 'toaster', 1, 1)", [])
            .is_err());
        // Die Kaskaden von material_units funktionieren weiter.
        conn.execute("DELETE FROM material_units WHERE id = 10", []).unwrap();
        let unit_id: Option<i64> =
            conn.query_row("SELECT unit_id FROM filament_spools WHERE id = 100", [], |r| r.get(0)).unwrap();
        assert_eq!(unit_id, None, "ON DELETE SET NULL gilt weiter");
        let slot_index_exists: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE name = 'idx_filament_spools_slot'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(slot_index_exists, 1);
    }

    #[test]
    fn the_resin_printer_step_is_a_no_op_rebuild_on_a_fresh_db() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        conn.execute("INSERT INTO printers (id, name, kind) VALUES (1, 'Saturn', 'resin')", []).unwrap();
        run_migrations(&mut conn).unwrap();
        let kind: String = conn.query_row("SELECT kind FROM printers WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(kind, "resin");
        let sql: String = conn
            .query_row("SELECT sql FROM sqlite_master WHERE name = 'material_units'", [], |r| r.get(0))
            .unwrap();
        assert!(sql.contains("'resin_vat'"));
    }
}
