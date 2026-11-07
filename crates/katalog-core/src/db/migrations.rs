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
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE printers ADD COLUMN manufacturer TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE printers ADD COLUMN model TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE printers ADD COLUMN nozzle_mm REAL")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE printers ADD COLUMN bed_x_mm REAL")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE printers ADD COLUMN bed_y_mm REAL")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE printers ADD COLUMN bed_z_mm REAL")),
    // Catalogs created by development builds between the trash columns and
    // `slice_info_json` hold those three columns in a different order; the
    // positional table rebuilds above then moved their values.
    MigrationStep::Simple(repair_shifted_file_columns),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN license TEXT")),
    // Platform and IDs of downloaded models. Only the columns are created here:
    // existing entries get their values on the next metadata rescan, never in a
    // bulk run while the catalog is being upgraded.
    MigrationStep::Simple(add_source_columns),
    // Companion files of a model and the remembered source folders (with the files
    // the user declined). New tables only, no existing data is touched.
    MigrationStep::Simple(add_companion_and_source_tables),
    // Colour list of a spool (JSON array of up to four `#rrggbb` values); NULL means single-colour.
    MigrationStep::Simple(add_spool_color_list),
    // Blocked automatic tags. New table only, no existing data is touched.
    MigrationStep::Simple(add_blocked_auto_tags_table),
    // Marks models trashed by "folder to trash". New table only, no existing data is touched.
    MigrationStep::Simple(add_folder_trash_origin_table),
    // Print data read from the slicer files of a 3MF. New tables only, no existing data is touched.
    MigrationStep::Simple(add_print_info_tables),
    // Filament colors of a 3MF (space separated `#rrggbb`); NULL = none known. New nullable column only.
    MigrationStep::Simple(add_print_info_filament_colors),
    // Copies the disk sync offered and the user declined, so they are not hashed and offered
    // again. New table only, no existing data is touched.
    MigrationStep::Simple(add_known_copies_table),
    // New file types `gcode` (.gcode and .bgcode) and `resin` (resin slicer files). Only the CHECK of
    // `files.file_type` changes; the columns, indexes and triggers of the table stay as they are.
    MigrationStep::Rebuild(add_gcode_and_resin_to_file_type_check),
    // Bundles of files (variants or parts of one model). New tables only, no existing data is touched.
    MigrationStep::Simple(add_bundle_tables),
    // Failed attempts in a row and the time of the last attempt per printer connection.
    // New columns only; stored connections start at 0 and NULL.
    MigrationStep::Simple(add_printer_connection_failures),
    // API key of the printer connection. New nullable column only; stored connections have no key.
    MigrationStep::Simple(add_printer_connection_api_key),
    // New companion file kind `other` (CAD sources and documents that are stored but never opened).
    // Only the CHECK of `companion_files.kind` changes; every row stays as it is.
    MigrationStep::Rebuild(add_other_to_companion_kind_check),
    // Failure reason and note per fetched print, and the flag for prints the program closed itself.
    // New columns only; stored prints start without a reason and not auto-closed.
    MigrationStep::Simple(add_printer_job_failure_columns),
    // New unit kind `bambu_ams_2_pro`. Only the CHECK of `material_units.kind` changes (rebuild);
    // no existing unit is switched over, every row and every spool in its slot stays as it is.
    MigrationStep::Rebuild(add_ams_2_pro_unit_kind),
    // Reports of sliced files that the slicer sent to the program (post-processing script). New table
    // only; nothing existing is touched.
    MigrationStep::Simple(add_slicer_reports_table),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN plate_dimensions_json TEXT")),
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

/// Schema version after the source columns of `files` (the step before the companion tables).
#[cfg(any(test, feature = "test-support"))]
pub const SOURCE_COLUMNS_MIGRATION_VERSION: i64 = 48;

/// Schema version after the companion and source-folder tables. Must stay 49: that is
/// what every catalog written by the version that introduced them contains.
#[cfg(test)]
pub(crate) const COMPANION_TABLES_MIGRATION_VERSION: i64 = 49;

/// Schema version after the spool colour list. Must stay 50: that is what every catalog
/// written by the version that introduced the column contains.
#[cfg(any(test, feature = "test-support"))]
pub const SPOOL_COLOR_LIST_MIGRATION_VERSION: i64 = 50;

/// Schema version after the blocked automatic tags table. Must stay 51: that is what
/// every catalog written by the version that introduced the table contains.
#[cfg(any(test, feature = "test-support"))]
pub const BLOCKED_AUTO_TAGS_MIGRATION_VERSION: i64 = 51;

/// Schema version after the folder trash origin table. Must stay 52: that is what every
/// catalog written by the version that introduced the table contains.
#[cfg(test)]
pub(crate) const FOLDER_TRASH_ORIGIN_MIGRATION_VERSION: i64 = 52;

/// Schema version after the print data tables. Must stay 53 for the same reason as the
/// versions above: it is what catalogs of builds between the tables and the colors contain.
#[cfg(test)]
pub(crate) const PRINT_INFO_TABLES_MIGRATION_VERSION: i64 = 53;

/// Schema version after the filament color column. Must stay 54: it is what catalogs of builds
/// between the color column and the remembered copies contain.
#[cfg(test)]
pub(crate) const FILAMENT_COLORS_MIGRATION_VERSION: i64 = 54;

/// Schema version after the file types `gcode` and `resin`.
#[cfg(test)]
pub(crate) const GCODE_RESIN_FILE_TYPES_MIGRATION_VERSION: i64 = 56;

/// Schema version after the bundle tables.
#[cfg(test)]
pub(crate) const BUNDLE_TABLES_MIGRATION_VERSION: i64 = 57;

/// New tables only; `IF NOT EXISTS` so a catalog that already got them from the base schema is left
/// alone. `files` is not touched on purpose: other tools read it with fixed column lists. Keep the
/// statements in step with `schema.sql`.
fn add_bundle_tables(conn: &Connection) -> Result<(), DbError> {
    exec(conn, "CREATE TABLE IF NOT EXISTS bundles (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
        kind TEXT NOT NULL CHECK (kind IN ('variants', 'parts')),
        created_at TEXT NOT NULL
    )")?;
    exec(conn, "CREATE TABLE IF NOT EXISTS bundle_members (
        file_id INTEGER PRIMARY KEY REFERENCES files (id) ON DELETE CASCADE,
        bundle_id INTEGER NOT NULL REFERENCES bundles (id) ON DELETE CASCADE,
        position INTEGER NOT NULL CHECK (position >= 0),
        label TEXT CHECK (label IS NULL OR length(label) <= 60)
    )")?;
    exec(conn, "CREATE INDEX IF NOT EXISTS idx_bundle_members_bundle_id ON bundle_members (bundle_id)")
}

/// Schema version after the failure counter of the printer connection.
#[cfg(test)]
pub(crate) const PRINTER_FAILURES_MIGRATION_VERSION: i64 = 58;

/// New columns only. `exec` tolerates a duplicate column, so a catalog that already got them from
/// the base schema is left alone. Keep the statements in step with `schema.sql`.
fn add_printer_connection_failures(conn: &Connection) -> Result<(), DbError> {
    exec(conn, "ALTER TABLE printer_connections ADD COLUMN failures INTEGER NOT NULL DEFAULT 0 CHECK (failures >= 0)")?;
    exec(conn, "ALTER TABLE printer_connections ADD COLUMN last_attempt_at REAL")
}

/// Schema version after the API key of the printer connection.
#[cfg(test)]
pub(crate) const PRINTER_API_KEY_MIGRATION_VERSION: i64 = 59;

/// New nullable column only (`exec` tolerates a duplicate column). Keep in step with `schema.sql`.
fn add_printer_connection_api_key(conn: &Connection) -> Result<(), DbError> {
    exec(conn, "ALTER TABLE printer_connections ADD COLUMN api_key TEXT")
}

/// Schema version after the failure columns of the fetched prints.
#[cfg(test)]
pub(crate) const PRINTER_JOB_FAILURE_MIGRATION_VERSION: i64 = 61;

/// New columns only (`exec` tolerates a duplicate column). The reason is not constrained here: the
/// list of codes lives in code and grows without a migration. Keep in step with `schema.sql`.
fn add_printer_job_failure_columns(conn: &Connection) -> Result<(), DbError> {
    exec(conn, "ALTER TABLE printer_jobs ADD COLUMN failure_reason TEXT")?;
    exec(conn, "ALTER TABLE printer_jobs ADD COLUMN failure_note TEXT")?;
    exec(conn, "ALTER TABLE printer_jobs ADD COLUMN auto_closed INTEGER NOT NULL DEFAULT 0 CHECK (auto_closed IN (0, 1))")
}

/// Schema version after the unit kind `bambu_ams_2_pro`.
#[cfg(test)]
pub(crate) const AMS_2_PRO_MIGRATION_VERSION: i64 = 62;

/// Schema version after the slicer reports.
#[cfg(test)]
pub(crate) const SLICER_REPORTS_MIGRATION_VERSION: i64 = 63;

/// New table only; `IF NOT EXISTS` so a catalog that already got it from the base schema is left alone.
/// `files` is not touched. The links to files are `ON DELETE SET NULL`: a deleted model leaves the
/// report standing. Keep the statements in step with `schema.sql`.
fn add_slicer_reports_table(conn: &Connection) -> Result<(), DbError> {
    exec(conn, "CREATE TABLE IF NOT EXISTS slicer_reports (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        reported_at TEXT NOT NULL,
        reported_path TEXT NOT NULL,
        file_name TEXT NOT NULL,
        file_size_bytes INTEGER NOT NULL CHECK (file_size_bytes >= 0),
        slicer TEXT,
        summary_json TEXT NOT NULL,
        state TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'linked', 'dismissed')),
        suggested_file_id INTEGER REFERENCES files (id) ON DELETE SET NULL,
        model_file_id INTEGER REFERENCES files (id) ON DELETE SET NULL,
        decided_at TEXT
    )")?;
    exec(conn, "CREATE INDEX IF NOT EXISTS idx_slicer_reports_name ON slicer_reports (file_name)")
}

/// Adds 'bambu_ams_2_pro' to the CHECK of `material_units.kind`, via a rebuild like
/// [`add_resin_printers`] and with the same trap: `filament_spools.unit_id` has `ON DELETE SET NULL`,
/// so with `foreign_keys` on the rebuild would throw every spool out of its slot. Existing units are
/// not converted (the program cannot know whether someone owns an AMS or an AMS 2 Pro); the user
/// changes the type of a unit by hand. Keep the CHECK in step with `schema.sql`.
fn add_ams_2_pro_unit_kind(conn: &mut Connection, step_version: i64) -> Result<(), DbError> {
    // Fresh DBs already have the kind. A missing table only occurs in tests with partial DBs.
    let already_migrated = conn
        .query_row("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'material_units'", [], |r| r.get::<_, String>(0))
        .optional()?
        .is_none_or(|sql| sql.contains("'bambu_ams_2_pro'"));

    let previously_enabled: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    conn.pragma_update(None, "foreign_keys", false)?;

    let result = (|| -> Result<(), DbError> {
        let tx = conn.transaction()?;
        if !already_migrated {
            // Explicit columns instead of `SELECT *`: the column order of an imported backup need not match ours.
            tx.execute_batch(
                "CREATE TABLE material_units_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    printer_id INTEGER NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL CHECK (kind IN ('bambu_ams', 'bambu_ams_lite', 'bambu_ams_2_pro', 'bambu_ams_ht', 'creality_cfs',
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
            for table in ["filament_spools", "material_units"] {
                let has_violation = tx
                    .query_row(&format!("PRAGMA foreign_key_check({table})"), [], |_| Ok(()))
                    .optional()?
                    .is_some();
                if has_violation {
                    return Err(DbError::Other(format!(
                        "AMS-2-Pro-Migration: foreign_key_check fand verwaiste Referenzen in {table}"
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

/// Schema version after the companion kind `other`.
#[cfg(test)]
pub(crate) const COMPANION_OTHER_KIND_MIGRATION_VERSION: i64 = 60;

/// Widens the `kind` CHECK of `companion_files` by `other`, via a rebuild (SQLite cannot alter a
/// CHECK). Nothing references the table, but the rebuild still runs with `foreign_keys` off and
/// ends with a `foreign_key_check`, because the table itself points at `files` with
/// `ON DELETE CASCADE`. Columns are named in the copy: an imported backup need not have our order.
fn add_other_to_companion_kind_check(conn: &mut Connection, step_version: i64) -> Result<(), DbError> {
    let current_sql: Option<String> = conn
        .query_row("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'companion_files'", [], |r| r.get(0))
        .optional()?;
    // A fresh database already has the new CHECK from schema.sql, and a database without the
    // table (a stripped-down table of an old test) has nothing to widen: only the version is bumped.
    let needs_rebuild = current_sql.is_some_and(|sql| !sql.contains("'other'"));

    let previously_enabled: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    conn.pragma_update(None, "foreign_keys", false)?;

    let result = (|| -> Result<(), DbError> {
        let tx = conn.transaction()?;
        if needs_rebuild {
            tx.execute_batch(
                "CREATE TABLE companion_files_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                    name TEXT NOT NULL,
                    path TEXT NOT NULL UNIQUE,
                    kind TEXT NOT NULL CHECK (kind IN ('image', 'pdf', 'text', 'other')),
                    size_bytes INTEGER,
                    added_at TEXT NOT NULL,
                    deleted_at TEXT,
                    trash_path TEXT
                );
                INSERT INTO companion_files_new (id, file_id, name, path, kind, size_bytes, added_at, deleted_at, trash_path)
                    SELECT id, file_id, name, path, kind, size_bytes, added_at, deleted_at, trash_path FROM companion_files;
                DROP TABLE companion_files;
                ALTER TABLE companion_files_new RENAME TO companion_files;
                CREATE INDEX IF NOT EXISTS idx_companion_files_file_id ON companion_files (file_id);",
            )?;
            let has_violation = tx
                .query_row("PRAGMA foreign_key_check(companion_files)", [], |_| Ok(()))
                .optional()?
                .is_some();
            if has_violation {
                return Err(DbError::Other(
                    "Begleitdatei-Migration: foreign_key_check fand verwaiste Referenzen in companion_files".into(),
                ));
            }
        }
        tx.pragma_update(None, "user_version", step_version)?;
        tx.commit()?;
        Ok(())
    })();

    conn.pragma_update(None, "foreign_keys", previously_enabled)?;
    result
}

/// New tables only (a row per read 3MF and a row per plate); `IF NOT EXISTS` so a catalog
/// that already got them from the base schema is left alone. `files` is not touched on purpose:
/// other tools read it with fixed column lists.
fn add_print_info_tables(conn: &Connection) -> Result<(), DbError> {
    exec(conn, "CREATE TABLE IF NOT EXISTS file_print_info (
        file_id INTEGER PRIMARY KEY REFERENCES files (id) ON DELETE CASCADE,
        reader_version INTEGER NOT NULL CHECK (reader_version >= 1),
        read_at TEXT NOT NULL,
        slice_state TEXT NOT NULL
            CHECK (slice_state IN ('sliced', 'no_slice_data', 'not_recognized', 'unreadable')),
        settings_state TEXT NOT NULL
            CHECK (settings_state IN ('read', 'missing', 'unreadable', 'not_attempted')),
        printer_model TEXT,
        bed_type TEXT,
        layer_height_mm REAL CHECK (layer_height_mm IS NULL OR layer_height_mm > 0),
        nozzle_mm REAL CHECK (nozzle_mm IS NULL OR nozzle_mm > 0),
        nozzle_mixed INTEGER NOT NULL DEFAULT 0 CHECK (nozzle_mixed IN (0, 1)),
        bed_temp_c REAL CHECK (bed_temp_c IS NULL OR bed_temp_c > 0),
        bed_temp_mixed INTEGER NOT NULL DEFAULT 0 CHECK (bed_temp_mixed IN (0, 1)),
        nozzle_temp_c REAL CHECK (nozzle_temp_c IS NULL OR nozzle_temp_c > 0),
        nozzle_temp_mixed INTEGER NOT NULL DEFAULT 0 CHECK (nozzle_temp_mixed IN (0, 1))
    )")?;
    exec(conn, "CREATE TABLE IF NOT EXISTS file_print_plates (
        file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
        plate_index INTEGER NOT NULL CHECK (plate_index >= 1),
        plate_name TEXT,
        print_seconds INTEGER CHECK (print_seconds IS NULL OR print_seconds > 0),
        first_layer_seconds REAL CHECK (first_layer_seconds IS NULL OR first_layer_seconds > 0),
        plate_bed_type TEXT,
        PRIMARY KEY (file_id, plate_index)
    )")
}

/// New nullable column only, so every stored row keeps its values. Rows written by an earlier
/// reader version count as unread anyway until they are read again. A catalog that already got
/// the column from the base schema is left alone (`exec` tolerates the duplicate column).
fn add_print_info_filament_colors(conn: &Connection) -> Result<(), DbError> {
    exec(conn, "ALTER TABLE file_print_info ADD COLUMN filament_colors TEXT")
}

/// New table only; `IF NOT EXISTS` so a catalog that already got it from the base schema is left alone.
/// Keep the statement in step with `schema.sql`.
fn add_known_copies_table(conn: &Connection) -> Result<(), DbError> {
    exec(conn, "CREATE TABLE IF NOT EXISTS disk_sync_known_copies (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        path TEXT NOT NULL UNIQUE,
        size_bytes INTEGER NOT NULL,
        content_hash TEXT NOT NULL,
        noted_at TEXT NOT NULL
    )")
}

/// New table only; `IF NOT EXISTS` so a catalog that already got it from the base schema is left alone.
fn add_blocked_auto_tags_table(conn: &Connection) -> Result<(), DbError> {
    exec(conn, "CREATE TABLE IF NOT EXISTS blocked_auto_tags (
        name TEXT PRIMARY KEY COLLATE NOCASE,
        blocked_at TEXT NOT NULL
    )")
}

/// New table only; `IF NOT EXISTS` so a catalog that already got it from the base schema is left alone.
fn add_folder_trash_origin_table(conn: &Connection) -> Result<(), DbError> {
    exec(conn, "CREATE TABLE IF NOT EXISTS folder_trash_origin (
        file_id INTEGER PRIMARY KEY REFERENCES files (id) ON DELETE CASCADE
    )")
}

/// New nullable column only, so existing spools stay single-colour and keep every value.
fn add_spool_color_list(conn: &Connection) -> Result<(), DbError> {
    exec(conn, "ALTER TABLE filament_spools ADD COLUMN color_hexes TEXT")
}

fn add_companion_and_source_tables(conn: &Connection) -> Result<(), DbError> {
    exec(conn, "CREATE TABLE IF NOT EXISTS companion_files (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        path TEXT NOT NULL UNIQUE,
        kind TEXT NOT NULL CHECK (kind IN ('image', 'pdf', 'text')),
        size_bytes INTEGER,
        added_at TEXT NOT NULL,
        deleted_at TEXT,
        trash_path TEXT
    )")?;
    exec(conn, "CREATE INDEX IF NOT EXISTS idx_companion_files_file_id ON companion_files (file_id)")?;
    exec(conn, "CREATE TABLE IF NOT EXISTS source_folders (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        path TEXT NOT NULL UNIQUE,
        added_at TEXT NOT NULL
    )")?;
    exec(conn, "CREATE TABLE IF NOT EXISTS source_rejected (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        path TEXT NOT NULL UNIQUE,
        size_bytes INTEGER NOT NULL,
        rejected_at TEXT NOT NULL
    )")
}

fn add_source_columns(conn: &Connection) -> Result<(), DbError> {
    for column in ["source_platform", "source_model_id", "source_profile_id"] {
        exec(conn, &format!("ALTER TABLE files ADD COLUMN {column} TEXT"))?;
    }
    Ok(())
}

/// Runs a single statement. The only tolerated error is "duplicate column name"
/// (column already exists); everything else propagates.
/// Puts `slice_info_json`, `deleted_at` and `trash_path` back where they belong.
///
/// Catalogs that went through an older column order (`deleted_at`, `trash_path`,
/// `slice_info_json`) lost the order when a table rebuild copied the rows by
/// position: the slicer data ended up in `trash_path`, and a trashed file's
/// timestamp and path moved one column to the right. Both patterns are
/// unmistakable (JSON object in a path column, a path where a timestamp
/// belongs), so rows that match neither are left alone.
fn repair_shifted_file_columns(conn: &Connection) -> Result<(), DbError> {
    // Trashed files: timestamp in `slice_info_json`, path in `deleted_at`,
    // slicer data (or nothing) in `trash_path`.
    conn.execute(
        "UPDATE files SET
            deleted_at = slice_info_json,
            trash_path = deleted_at,
            slice_info_json = CASE
                WHEN trash_path IS NOT NULL AND substr(ltrim(trash_path), 1, 1) = '{' AND json_valid(trash_path)
                THEN trash_path END
         WHERE deleted_at IS NOT NULL
           AND deleted_at NOT GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T*'
           AND slice_info_json GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T*'",
        [],
    )?;
    // Other files: slicer data in `trash_path`. A value written later by a
    // metadata rescan in `slice_info_json` wins.
    conn.execute(
        "UPDATE files SET
            slice_info_json = COALESCE(slice_info_json, trash_path),
            trash_path = NULL
         WHERE deleted_at IS NULL
           AND trash_path IS NOT NULL
           AND substr(ltrim(trash_path), 1, 1) = '{'
           AND json_valid(trash_path)",
        [],
    )?;
    Ok(())
}

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

/// Adds `gcode` and `resin` to the `file_type` CHECK of `files`. Unlike
/// [`rebuild_files_table_with_check`] (its column list ends at `trash_path`, so it cannot be run
/// again) this copy takes the table definition, the columns, the indexes and the triggers from
/// the database itself and replaces only the value list of the CHECK.
fn add_gcode_and_resin_to_file_type_check(conn: &mut Connection, step_version: i64) -> Result<(), DbError> {
    const NEW_VALUES: &str = "'3mf', 'stl', 'stp', 'obj', 'gcode', 'resin'";
    let current_sql: String = conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'files'",
        [],
        |r| r.get(0),
    )?;
    // A fresh database already has the new CHECK from schema.sql, and a table without a `file_type`
    // CHECK (a stripped-down table of an old test) has nothing to widen: only the version is bumped.
    let already_migrated = current_sql.contains("'gcode'") && current_sql.contains("'resin'");
    let new_sql = if already_migrated { None } else { files_sql_with_check(&current_sql, NEW_VALUES) };

    // Remember the previous value: tests also call run_migrations without foreign_keys=ON.
    let previously_enabled: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    // Must be off before the transaction starts, or DROP TABLE would cascade into every child table.
    conn.pragma_update(None, "foreign_keys", false)?;

    let result = (|| -> Result<(), DbError> {
        let tx = conn.transaction()?;
        if let Some(create_sql) = new_sql {
            let columns: Vec<String> = {
                let mut stmt = tx.prepare("SELECT name FROM pragma_table_info('files') ORDER BY cid")?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                rows.collect::<Result<_, _>>()?
            };
            let column_list = columns.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
            // Indexes and triggers of `files` vanish with the table: keep their definitions.
            let extras: Vec<String> = {
                let mut stmt = tx.prepare(
                    "SELECT sql FROM sqlite_master
                     WHERE tbl_name = 'files' AND type IN ('index', 'trigger') AND sql IS NOT NULL
                     ORDER BY rowid",
                )?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                rows.collect::<Result<_, _>>()?
            };
            tx.execute_batch(&create_sql)?;
            tx.execute_batch(&format!(
                "INSERT INTO files_new ({column_list}) SELECT {column_list} FROM files;
                 DROP TABLE files;
                 ALTER TABLE files_new RENAME TO files;"
            ))?;
            for sql in extras {
                tx.execute_batch(&sql)?;
            }
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

/// The `CREATE TABLE` text of `files` as stored by SQLite, renamed to `files_new` and with the
/// value list of the `file_type` CHECK replaced.
fn files_sql_with_check(current_sql: &str, values: &str) -> Option<String> {
    let body = &current_sql[current_sql.find('(')?..];
    let list_start = body.find("file_type IN (")? + "file_type IN (".len();
    let list_len = body[list_start..].find(')')?;
    Some(format!(
        "CREATE TABLE files_new {}{}{}",
        &body[..list_start],
        values,
        &body[list_start + list_len..]
    ))
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
        return Err(DbError::NewerSchema { found: current, supported: target_version });
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
    fn license_migrates_existing_rows_and_exists_in_fresh_schema() {
        let mut conn = crate::db::connect_in_memory().unwrap();
        let id = crate::db::test_insert_minimal_file(&conn, "/model.3mf", None).unwrap();
        conn.execute_batch("ALTER TABLE files DROP COLUMN license").unwrap();
        // The license step is the one before the source-column step.
        conn.pragma_update(None, "user_version", SOURCE_COLUMNS_MIGRATION_VERSION - 2).unwrap();
        run_migrations(&mut conn).unwrap();
        assert_eq!(crate::db::get_file(&conn, id).unwrap().unwrap().license, None);
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        fresh.prepare("SELECT license FROM files").unwrap();
    }

    #[test]
    fn printer_details_upgrade_from_44_preserves_data() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations_with(&mut conn, &MIGRATIONS[..44], 44).unwrap();
        conn.execute("INSERT INTO printers (id, name, position, kind) VALUES (7, 'Old printer', 3, 'filament')", []).unwrap();
        run_migrations(&mut conn).unwrap();
        let old: (String, i64, String) = conn.query_row("SELECT name, position, kind FROM printers WHERE id=7", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!(old, ("Old printer".into(), 3, "filament".into()));
        for column in ["manufacturer", "model", "nozzle_mm", "bed_x_mm", "bed_y_mm", "bed_z_mm"] {
            let value: Option<String> = conn.query_row(&format!("SELECT {column} FROM printers WHERE id=7"), [], |r| r.get(0)).unwrap();
            assert!(value.is_none());
        }
    }

    #[test]
    fn upgrade_from_45_repairs_columns_moved_by_an_old_table_order() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations_with(&mut conn, &MIGRATIONS[..45], 45).unwrap();
        let rows = [
            // 1 live file, slicer data in trash_path
            ("p1", None, None, Some(r#"{"total_weight_g":1.0,"plates":[]}"#)),
            // 2 live file rescanned after the move: the newer slicer data wins
            ("p2", Some(r#"{"total_weight_g":2.0}"#), None, Some(r#"{"total_weight_g":9.0}"#)),
            // 3 trashed file, everything one column to the right
            ("p3", Some("2026-02-02T10:00:00+00:00"), Some("/trash/3-c.3mf"), Some(r#"{"total_weight_g":3.0}"#)),
            // 4 trashed file without slicer data
            ("p4", Some("2026-02-03T10:00:00+00:00"), Some("/trash/4-d.stl"), None),
            // 5 correct trashed file stays as it is
            ("p5", None, Some("2026-02-04T10:00:00+00:00"), Some("/trash/5-e.3mf")),
            // 6 correct live file with slicer data stays as it is
            ("p6", Some(r#"{"total_weight_g":6.0}"#), None, None),
            // 7 a path-like value in trash_path of a live file is not slicer data
            ("p7", None, None, Some("/not/json")),
            // 8 broken JSON is left alone
            ("p8", None, None, Some(r#"{"total_weight_g":"#)),
        ];
        for (name, slice, deleted, trash) in rows {
            conn.execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at, slice_info_json, deleted_at, trash_path)
                 VALUES (?1, '/x/' || ?1 || '.3mf', '3mf', 1, '2026-01-01T00:00:00Z', ?2, ?3, ?4)",
                rusqlite::params![name, slice, deleted, trash],
            ).unwrap();
        }
        run_migrations(&mut conn).unwrap();
        let read = |name: &str| -> (Option<String>, Option<String>, Option<String>) {
            conn.query_row(
                "SELECT slice_info_json, deleted_at, trash_path FROM files WHERE name = ?1", [name],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            ).unwrap()
        };
        let s = |v: &str| Some(v.to_string());
        assert_eq!(read("p1"), (s(r#"{"total_weight_g":1.0,"plates":[]}"#), None, None));
        assert_eq!(read("p2"), (s(r#"{"total_weight_g":2.0}"#), None, None));
        assert_eq!(read("p3"), (s(r#"{"total_weight_g":3.0}"#), s("2026-02-02T10:00:00+00:00"), s("/trash/3-c.3mf")));
        assert_eq!(read("p4"), (None, s("2026-02-03T10:00:00+00:00"), s("/trash/4-d.stl")));
        assert_eq!(read("p5"), (None, s("2026-02-04T10:00:00+00:00"), s("/trash/5-e.3mf")));
        assert_eq!(read("p6"), (s(r#"{"total_weight_g":6.0}"#), None, None));
        assert_eq!(read("p7"), (None, None, s("/not/json")));
        assert_eq!(read("p8"), (None, None, s(r#"{"total_weight_g":"#)));
    }

    /// The columns the Docker companion reads; they must survive every migration unchanged.
    const COMPANION_COLUMNS: &str = "id, name, path, file_type, file_size_bytes, thumbnail_png, folder_id, deleted_at";

    #[test]
    fn source_columns_are_added_to_a_previous_version_catalog_without_touching_its_data() {
        let mut conn = crate::db::connect_in_memory().unwrap();
        let id = crate::db::test_insert_minimal_file(&conn, "/model.3mf", None).unwrap();
        conn.execute("UPDATE files SET creator = 'Jane', license = 'cc-by', source_url = 'https://example.com/m', favorite = 1 WHERE id = ?1", [id]).unwrap();
        let snapshot = |conn: &Connection| -> String {
            conn.query_row(
                &format!("SELECT {COMPANION_COLUMNS}, creator, license, source_url, favorite, print_status, imported_at FROM files WHERE id = ?1"),
                [id],
                |r| {
                    let mut parts = Vec::new();
                    for i in 0..14 {
                        parts.push(format!("{:?}", r.get_ref(i)?));
                    }
                    Ok(parts.join("|"))
                },
            ).unwrap()
        };
        let before = snapshot(&conn);

        // The previous version's catalog: same data, none of the three columns, one step behind.
        conn.execute_batch("ALTER TABLE files DROP COLUMN source_platform; ALTER TABLE files DROP COLUMN source_model_id; ALTER TABLE files DROP COLUMN source_profile_id").unwrap();
        conn.pragma_update(None, "user_version", SOURCE_COLUMNS_MIGRATION_VERSION - 1).unwrap();
        assert!(conn.prepare("SELECT source_platform FROM files").is_err(), "precondition: column is gone");

        run_migrations(&mut conn).unwrap();

        let row: (Option<String>, Option<String>, Option<String>) = conn
            .query_row("SELECT source_platform, source_model_id, source_profile_id FROM files WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!(row, (None, None, None), "existing entries stay empty until the next rescan");
        assert_eq!(snapshot(&conn), before, "no existing value may change");
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn source_columns_exist_in_a_fresh_schema_and_the_step_is_repeatable() {
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        fresh.prepare("SELECT source_platform, source_model_id, source_profile_id FROM files").unwrap();
        // A development catalog that already has the columns must not fail on the step.
        add_source_columns(&fresh).unwrap();
        fresh.prepare(&format!("SELECT {COMPANION_COLUMNS} FROM files")).unwrap();
    }

    /// A catalog as the previous version left it: the full old schema, none of the new
    /// tables, one step behind, with data in several tables.
    fn previous_version_catalog() -> (Connection, i64) {
        let conn = crate::db::connect_in_memory().unwrap();
        let folder = crate::db::insert_folder(&conn, "root").unwrap();
        let id = crate::db::test_insert_minimal_file(&conn, "/models/a.3mf", Some(folder)).unwrap();
        conn.execute("UPDATE files SET creator = 'Jane', favorite = 1, license = 'cc-by' WHERE id = ?1", [id]).unwrap();
        conn.execute("INSERT INTO tags (id, name, color_hue) VALUES (1, 'deko', 10)", []).unwrap();
        conn.execute("INSERT INTO file_tags VALUES (?1, 1)", [id]).unwrap();
        conn.execute("INSERT INTO app_settings (key, value) VALUES ('catalog_base_dir', '/models')", []).unwrap();
        conn.execute_batch("DROP TABLE companion_files; DROP TABLE source_folders; DROP TABLE source_rejected;").unwrap();
        conn.pragma_update(None, "user_version", COMPANION_TABLES_MIGRATION_VERSION - 1).unwrap();
        (conn, id)
    }

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [name], |r| r.get::<_, i64>(0)).unwrap() == 1
    }

    #[test]
    fn the_companion_and_source_step_stays_at_49_and_adds_only_tables() {
        assert_eq!(COMPANION_TABLES_MIGRATION_VERSION, 49, "the step must stay where it shipped");
        assert_eq!(COMPANION_TABLES_MIGRATION_VERSION, SOURCE_COLUMNS_MIGRATION_VERSION + 1);
        assert_eq!(SPOOL_COLOR_LIST_MIGRATION_VERSION, COMPANION_TABLES_MIGRATION_VERSION + 1);
    }

    #[test]
    fn a_previous_version_catalog_gets_the_new_tables_and_keeps_all_its_data() {
        let (mut conn, id) = previous_version_catalog();
        for table in ["companion_files", "source_folders", "source_rejected"] {
            assert!(!table_exists(&conn, table), "precondition: {table} is not there yet");
        }
        let snapshot = |conn: &Connection| -> Vec<String> {
            let mut rows = Vec::new();
            for sql in [
                "SELECT id || '|' || name || '|' || path || '|' || ifnull(creator, '') || '|' || favorite || '|' || ifnull(license, '') || '|' || ifnull(folder_id, '') FROM files",
                "SELECT id || '|' || name || '|' || color_hue FROM tags",
                "SELECT file_id || '|' || tag_id FROM file_tags",
                "SELECT key || '|' || value FROM app_settings",
                "SELECT id || '|' || name || '|' || path FROM folders",
            ] {
                let mut stmt = conn.prepare(sql).unwrap();
                rows.extend(stmt.query_map([], |r| r.get::<_, String>(0)).unwrap().map(|r| r.unwrap()));
            }
            rows
        };
        let before = snapshot(&conn);

        run_migrations(&mut conn).unwrap();

        assert_eq!(snapshot(&conn), before, "no existing value may change");
        for table in ["companion_files", "source_folders", "source_rejected"] {
            assert!(table_exists(&conn, table), "{table}");
            let rows: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(rows, 0, "{table} starts empty");
        }
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        // The new tables work: kinds are checked, paths are unique, and the rows follow their model.
        conn.execute("INSERT INTO companion_files (file_id, name, path, kind, added_at) VALUES (?1, 'p.png', '/models/a_extras/p.png', 'image', 'now')", [id]).unwrap();
        assert!(conn.execute("INSERT INTO companion_files (file_id, name, path, kind, added_at) VALUES (?1, 'x', '/other', 'exe', 'now')", [id]).is_err(), "kind is checked");
        assert!(conn.execute("INSERT INTO companion_files (file_id, name, path, kind, added_at) VALUES (?1, 'p.png', '/models/a_extras/p.png', 'image', 'now')", [id]).is_err(), "path is unique");
        conn.execute("DELETE FROM files WHERE id = ?1", [id]).unwrap();
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM companion_files", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0, "deleting the model removes its companion rows");
    }

    #[test]
    fn the_new_tables_exist_in_a_fresh_schema_and_the_step_is_repeatable() {
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        for table in ["companion_files", "source_folders", "source_rejected"] {
            assert!(table_exists(&fresh, table), "{table}");
        }
        add_companion_and_source_tables(&fresh).unwrap();
        add_companion_and_source_tables(&fresh).unwrap();
    }

    /// A catalog one step behind the colour list: spools with all their values, no `color_hexes` yet.
    fn catalog_before_spool_color_list() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch("ALTER TABLE filament_spools DROP COLUMN color_hexes").unwrap();
        conn.execute(
            "INSERT INTO filament_spools (material, manufacturer, color, diameter_mm, original_weight_g, remaining_weight_g, created_at, color_hex)
             VALUES ('PLA', 'Bambu', 'Rot', 1.75, 1000, 640.5, 'now', '#b03020')",
            [],
        ).unwrap();
        conn.pragma_update(None, "user_version", SPOOL_COLOR_LIST_MIGRATION_VERSION - 1).unwrap();
        conn
    }

    fn spool_snapshot(conn: &Connection) -> String {
        conn.query_row(
            "SELECT id || '|' || material || '|' || manufacturer || '|' || color || '|' || diameter_mm || '|' || original_weight_g
                    || '|' || remaining_weight_g || '|' || color_hex || '|' || kind FROM filament_spools",
            [],
            |r| r.get(0),
        ).unwrap()
    }

    #[test]
    fn the_spool_color_list_step_adds_a_nullable_column_and_keeps_the_spools_unchanged() {
        let mut conn = catalog_before_spool_color_list();
        assert!(conn.prepare("SELECT color_hexes FROM filament_spools").is_err(), "precondition: no column yet");
        let before = spool_snapshot(&conn);

        run_migrations(&mut conn).unwrap();

        assert_eq!(spool_snapshot(&conn), before, "no existing value may change");
        let list: Option<String> = conn.query_row("SELECT color_hexes FROM filament_spools", [], |r| r.get(0)).unwrap();
        assert_eq!(list, None, "old spools stay single-colour");
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert!(version >= SPOOL_COLOR_LIST_MIGRATION_VERSION, "the step ran");
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    /// A catalog one step behind the blocked-tags table, with tags and spools to prove they stay as they were.
    fn catalog_before_blocked_auto_tags() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch("DROP TABLE blocked_auto_tags").unwrap();
        conn.execute_batch("INSERT INTO tags (name, color_hue) VALUES ('miniatur', 12), ('mein-projekt', 200)").unwrap();
        conn.pragma_update(None, "user_version", BLOCKED_AUTO_TAGS_MIGRATION_VERSION - 1).unwrap();
        conn
    }

    fn tag_snapshot(conn: &Connection) -> String {
        conn.query_row("SELECT group_concat(id || '|' || name || '|' || color_hue, ';') FROM (SELECT * FROM tags ORDER BY id)", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn the_blocked_tags_step_adds_an_empty_table_and_keeps_the_tags_unchanged() {
        let mut conn = catalog_before_blocked_auto_tags();
        assert!(!table_exists(&conn, "blocked_auto_tags"), "precondition: no table yet");
        let before = tag_snapshot(&conn);

        run_migrations(&mut conn).unwrap();

        assert_eq!(tag_snapshot(&conn), before, "no existing value may change");
        assert!(table_exists(&conn, "blocked_auto_tags"));
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM blocked_auto_tags", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert_eq!(FOLDER_TRASH_ORIGIN_MIGRATION_VERSION, BLOCKED_AUTO_TAGS_MIGRATION_VERSION + 1, "the folder trash origin table is the step after");
    }

    #[test]
    fn the_folder_trash_origin_step_adds_an_empty_table_and_is_repeatable() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        conn.execute_batch("DROP TABLE folder_trash_origin").unwrap();
        conn.pragma_update(None, "user_version", BLOCKED_AUTO_TAGS_MIGRATION_VERSION).unwrap();
        run_migrations(&mut conn).unwrap();
        assert!(table_exists(&conn, "folder_trash_origin"));
        add_folder_trash_origin_table(&conn).unwrap();
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM folder_trash_origin", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    /// Rows of the tables other tools read, to prove a step leaves them as they were.
    fn catalog_snapshot(conn: &Connection) -> String {
        let one = |sql: &str| -> String { conn.query_row(sql, [], |r| r.get::<_, Option<String>>(0)).unwrap().unwrap_or_default() };
        [
            one("SELECT group_concat(id || '|' || name || '|' || path || '|' || file_type || '|' || file_size_bytes, ';') FROM (SELECT * FROM files ORDER BY id)"),
            one("SELECT group_concat(id || '|' || name || '|' || color_hue, ';') FROM (SELECT * FROM tags ORDER BY id)"),
            one("SELECT group_concat(file_id || '|' || tag_id, ';') FROM (SELECT * FROM file_tags ORDER BY file_id, tag_id)"),
            one("SELECT group_concat(id || '|' || file_id || '|' || name, ';') FROM (SELECT * FROM file_materials ORDER BY id)"),
        ]
        .join("#")
    }

    #[test]
    fn migration_adds_print_tables_without_touching_data() {
        let mut conn = crate::db::connect_in_memory().unwrap();
        // A catalog at the version before the step: no print tables, but files, tags and materials.
        conn.execute_batch("DROP TABLE file_print_plates; DROP TABLE file_print_info").unwrap();
        let a = crate::db::test_insert_minimal_file(&conn, "/a.3mf", None).unwrap();
        let b = crate::db::test_insert_minimal_file(&conn, "/b.3mf", None).unwrap();
        conn.execute_batch(&format!(
            "INSERT INTO tags (name, color_hue) VALUES ('miniatur', 12), ('mein-projekt', 200);
             INSERT INTO file_tags (file_id, tag_id) VALUES ({a}, 1), ({a}, 2), ({b}, 2);
             INSERT INTO file_materials (file_id, name, display_color) VALUES ({a}, 'PLA', '#ff0000'), ({b}, 'PETG', NULL)"
        ))
        .unwrap();
        conn.pragma_update(None, "user_version", FOLDER_TRASH_ORIGIN_MIGRATION_VERSION).unwrap();
        assert!(!table_exists(&conn, "file_print_info") && !table_exists(&conn, "file_print_plates"), "precondition");
        let before = catalog_snapshot(&conn);
        assert!(before.contains("/a.3mf") && before.contains("miniatur") && before.contains("PETG"), "precondition: data present");

        run_migrations(&mut conn).unwrap();

        assert_eq!(catalog_snapshot(&conn), before, "no existing row may change");
        for table in ["file_print_info", "file_print_plates"] {
            assert!(table_exists(&conn, table), "{table}");
            let rows: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(rows, 0, "{table} starts empty");
        }
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert_eq!(PRINT_INFO_TABLES_MIGRATION_VERSION, FOLDER_TRASH_ORIGIN_MIGRATION_VERSION + 1, "the print tables are the step after");
    }

    #[test]
    fn the_print_tables_step_is_repeatable_and_part_of_a_fresh_schema() {
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        assert!(table_exists(&fresh, "file_print_info") && table_exists(&fresh, "file_print_plates"));
        fresh.pragma_update(None, "foreign_keys", true).unwrap();
        let id = crate::db::test_insert_minimal_file(&fresh, "/a.3mf", None).unwrap();
        fresh.execute("INSERT INTO file_print_plates (file_id, plate_index, first_layer_seconds) VALUES (?1, 1, 5.5)", [id]).unwrap();
        add_print_info_tables(&fresh).unwrap();
        add_print_info_tables(&fresh).unwrap();
        let rows: i64 = fresh.query_row("SELECT COUNT(*) FROM file_print_plates", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1, "repeating the step keeps the rows");
    }

    #[test]
    fn the_print_tables_refuse_invalid_rows_and_follow_their_file() {
        let conn = crate::db::connect_in_memory().unwrap();
        let mut counter = 0;
        // A fresh file per statement, so a refusal can only come from the CHECK under test
        // and never from a primary key that is already taken.
        let mut fresh = || {
            counter += 1;
            crate::db::test_insert_minimal_file(&conn, &format!("/f{counter}.3mf"), None).unwrap()
        };
        let info_cols = "file_id, reader_version, read_at, slice_state, settings_state";
        let ok_values = |id: i64| format!("{id}, 1, 'n', 'sliced', 'read'");
        // The valid row each case is derived from must be accepted.
        let id = fresh();
        conn.execute(&format!("INSERT INTO file_print_info ({info_cols}) VALUES ({})", ok_values(id)), []).unwrap();
        assert!(conn.execute(&format!("INSERT INTO file_print_info ({info_cols}) VALUES ({})", ok_values(id)), []).is_err(), "one row per file");
        let id = fresh();
        conn.execute(&format!("INSERT INTO file_print_plates (file_id, plate_index) VALUES ({id}, 1)"), []).unwrap();
        assert!(conn.execute(&format!("INSERT INTO file_print_plates (file_id, plate_index) VALUES ({id}, 1)"), []).is_err(), "one row per plate");

        let bad_info = [
            "{id}, 0, 'n', 'sliced', 'read'",
            "{id}, 1, 'n', 'x', 'read'",
            "{id}, 1, 'n', 'sliced', 'x'",
        ];
        for values in bad_info {
            let sql = format!("INSERT INTO file_print_info ({info_cols}) VALUES ({})", values.replace("{id}", &fresh().to_string()));
            assert!(conn.execute(&sql, []).is_err(), "{sql}");
        }
        for column in ["layer_height_mm", "nozzle_mm", "bed_temp_c", "nozzle_temp_c"] {
            let sql = format!("INSERT INTO file_print_info ({info_cols}, {column}) VALUES ({}, -1)", ok_values(fresh()));
            assert!(conn.execute(&sql, []).is_err(), "{sql}");
        }
        for column in ["nozzle_mixed", "bed_temp_mixed", "nozzle_temp_mixed"] {
            let sql = format!("INSERT INTO file_print_info ({info_cols}, {column}) VALUES ({}, 2)", ok_values(fresh()));
            assert!(conn.execute(&sql, []).is_err(), "{sql}");
        }
        for (column, value) in [("plate_index", "0"), ("print_seconds", "0"), ("first_layer_seconds", "-2")] {
            let id = fresh();
            let sql = if column == "plate_index" {
                format!("INSERT INTO file_print_plates (file_id, plate_index) VALUES ({id}, {value})")
            } else {
                format!("INSERT INTO file_print_plates (file_id, plate_index, {column}) VALUES ({id}, 1, {value})")
            };
            assert!(conn.execute(&sql, []).is_err(), "{sql}");
        }
        // No such file.
        assert!(conn.execute(&format!("INSERT INTO file_print_info ({info_cols}) VALUES ({})", ok_values(9999)), []).is_err());
        assert!(conn.execute("INSERT INTO file_print_plates (file_id, plate_index) VALUES (9999, 1)", []).is_err());

        let id = fresh();
        conn.execute(&format!("INSERT INTO file_print_info ({info_cols}) VALUES ({})", ok_values(id)), []).unwrap();
        conn.execute(&format!("INSERT INTO file_print_plates (file_id, plate_index) VALUES ({id}, 1)"), []).unwrap();
        conn.execute("DELETE FROM files WHERE id = ?1", [id]).unwrap();
        for table in ["file_print_info", "file_print_plates"] {
            let rows: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table} WHERE file_id = ?1"), [id], |r| r.get(0)).unwrap();
            assert_eq!(rows, 0, "{table} is cleaned up with the file");
        }
    }

    /// `PRAGMA table_info` of the four tables the Docker companion reads, as produced on
    /// commit 2a4a74c8 (the base of the print data work). A new column or a changed type in
    /// any of them would break tools that read the catalog with fixed column lists.
    const COMPANION_TABLE_SHAPES: [(&str, &[&str]); 4] = [
        ("files", &[
            "id|INTEGER|0|-|1", "name|TEXT|1|-|0", "path|TEXT|1|-|0", "file_type|TEXT|1|-|0", "folder_id|INTEGER|0|-|0",
            "origin|TEXT|1|'local'|0", "sync_status|TEXT|1|'local-only'|0", "cloud_id|TEXT|0|-|0",
            "file_size_bytes|INTEGER|1|-|0", "dimension_x_mm|REAL|0|-|0", "dimension_y_mm|REAL|0|-|0",
            "dimension_z_mm|REAL|0|-|0", "volume_cm3|REAL|0|-|0", "object_count|INTEGER|0|-|0",
            "thumbnail_png|BLOB|0|-|0", "imported_at|TEXT|1|-|0", "file_modified_at|TEXT|0|-|0",
            "print_status|TEXT|1|'not_printed'|0", "last_viewed_at|TEXT|0|-|0", "creator|TEXT|0|-|0",
            "content_hash|TEXT|0|-|0", "render_snapshot_png|BLOB|0|-|0", "custom_image_png|BLOB|0|-|0",
            "source_url|TEXT|0|-|0", "license|TEXT|0|-|0", "queue_position|INTEGER|0|-|0", "favorite|INTEGER|1|0|0",
            "plate_count|INTEGER|0|-|0", "slice_info_json|TEXT|0|-|0", "deleted_at|TEXT|0|-|0", "trash_path|TEXT|0|-|0",
            "source_platform|TEXT|0|-|0", "source_model_id|TEXT|0|-|0", "source_profile_id|TEXT|0|-|0",
        ]),
        ("folders", &["id|INTEGER|0|-|1", "name|TEXT|1|-|0", "parent_id|INTEGER|0|-|0", "path|TEXT|0|-|0"]),
        ("tags", &["id|INTEGER|0|-|1", "name|TEXT|1|-|0", "color_hue|INTEGER|1|-|0"]),
        ("file_tags", &["file_id|INTEGER|1|-|1", "tag_id|INTEGER|1|-|2"]),
    ];

    /// Indexes, foreign keys and triggers of the same four tables on 2a4a74c8 (sorted lines).
    const COMPANION_TABLE_EXTRAS: [(&str, &[&str]); 4] = [
        ("files", &[
            "fk folders|folder_id|id|NO ACTION|SET NULL",
            "index idx_files_content_hash|unique=0|origin=c|partial=0|content_hash",
            "index idx_files_file_type|unique=0|origin=c|partial=0|file_type",
            "index idx_files_folder_id|unique=0|origin=c|partial=0|folder_id",
            "index sqlite_autoindex_files_1|unique=1|origin=u|partial=0|path",
        ]),
        ("folders", &["fk folders|parent_id|id|NO ACTION|CASCADE", "index idx_folders_path|unique=1|origin=c|partial=0|path"]),
        ("tags", &["index sqlite_autoindex_tags_1|unique=1|origin=u|partial=0|name"]),
        ("file_tags", &[
            "fk files|file_id|id|NO ACTION|CASCADE",
            "fk tags|tag_id|id|NO ACTION|CASCADE",
            "index idx_file_tags_tag_id|unique=0|origin=c|partial=0|tag_id",
            "index sqlite_autoindex_file_tags_1|unique=1|origin=pk|partial=0|file_id,tag_id",
        ]),
    ];

    fn table_shape(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})")).unwrap();
        stmt.query_map([], |r| {
            let default: Option<String> = r.get(4)?;
            Ok(format!(
                "{}|{}|{}|{}|{}",
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                default.unwrap_or_else(|| "-".into()),
                r.get::<_, i64>(5)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
    }

    /// Indexes (with their columns), foreign keys and triggers of a table, as text lines.
    fn table_extras(conn: &Connection, table: &str) -> Vec<String> {
        let mut lines = Vec::new();
        let mut stmt = conn.prepare(&format!("PRAGMA index_list({table})")).unwrap();
        let indexes: Vec<(String, i64, String, i64)> = stmt
            .query_map([], |r| Ok((r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        for (name, unique, origin, partial) in indexes {
            let mut cols = conn.prepare(&format!("PRAGMA index_info({name})")).unwrap();
            let columns: Vec<String> = cols.query_map([], |r| r.get::<_, Option<String>>(2)).unwrap().map(|r| r.unwrap().unwrap_or_default()).collect();
            lines.push(format!("index {name}|unique={unique}|origin={origin}|partial={partial}|{}", columns.join(",")));
        }
        let mut stmt = conn.prepare(&format!("PRAGMA foreign_key_list({table})")).unwrap();
        let keys: Vec<String> = stmt
            .query_map([], |r| {
                Ok(format!("fk {}|{}|{}|{}|{}", r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?.unwrap_or_default(), r.get::<_, String>(5)?, r.get::<_, String>(6)?))
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        lines.extend(keys);
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'trigger' AND tbl_name = ?1 ORDER BY name").unwrap();
        let triggers: Vec<String> = stmt.query_map([table], |r| Ok(format!("trigger {}", r.get::<_, String>(0)?))).unwrap().map(|r| r.unwrap()).collect();
        lines.extend(triggers);
        lines.sort();
        lines
    }

    #[test]
    fn plate_dimensions_migrate_existing_rows_to_null() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        let id = crate::db::test_insert_minimal_file(&conn, "/legacy.3mf", None).unwrap();
        conn.execute_batch("ALTER TABLE files DROP COLUMN plate_dimensions_json; PRAGMA user_version = 63;").unwrap();
        run_migrations(&mut conn).unwrap();
        let file = crate::db::get_file(&conn, id).unwrap().unwrap();
        assert!(file.plate_dimensions_json.is_none());
        assert_eq!(file.path, "/legacy.3mf");
        conn.execute("UPDATE files SET plate_dimensions_json = '[]' WHERE id = ?1", [id]).unwrap();
        run_migrations(&mut conn).unwrap();
        assert_eq!(crate::db::get_file(&conn, id).unwrap().unwrap().plate_dimensions_json.as_deref(), Some("[]"));
    }

    #[test]
    fn companion_tables_keep_their_shape() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        for (table, expected) in COMPANION_TABLE_SHAPES {
            let shape: Vec<_> = table_shape(&conn, table).into_iter().filter(|column| !column.starts_with("plate_dimensions_json|")).collect();
            assert_eq!(shape, expected, "table {table} changed: the Docker companion reads it with fixed columns");
        }
        for (table, expected) in COMPANION_TABLE_EXTRAS {
            assert_eq!(table_extras(&conn, table), expected, "indexes, keys or triggers of {table} changed");
        }
    }

    #[test]
    fn the_blocked_tags_step_is_repeatable_and_part_of_a_fresh_schema() {
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        assert!(table_exists(&fresh, "blocked_auto_tags"));
        fresh.execute("INSERT INTO blocked_auto_tags (name, blocked_at) VALUES ('miniatur', 'x')", []).unwrap();
        add_blocked_auto_tags_table(&fresh).unwrap();
        add_blocked_auto_tags_table(&fresh).unwrap();
        let rows: i64 = fresh.query_row("SELECT COUNT(*) FROM blocked_auto_tags", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1, "repeating the step keeps the entries");
        assert!(
            fresh.execute("INSERT INTO blocked_auto_tags (name, blocked_at) VALUES ('MINIATUR', 'y')", []).is_err(),
            "names are unique without regard to case"
        );
    }

    #[test]
    fn the_spool_color_list_step_is_repeatable_and_part_of_a_fresh_schema() {
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        fresh.prepare("SELECT color_hexes FROM filament_spools").unwrap();
        add_spool_color_list(&fresh).unwrap();
        add_spool_color_list(&fresh).unwrap();
    }

    /// A catalog at version 53: print data tables without the color column, one stored row.
    fn catalog_before_filament_colors() -> (Connection, i64) {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch("ALTER TABLE file_print_info DROP COLUMN filament_colors").unwrap();
        let id = crate::db::test_insert_minimal_file(&conn, "/a.3mf", None).unwrap();
        conn.execute(
            "INSERT INTO file_print_info (file_id, reader_version, read_at, slice_state, settings_state, printer_model, layer_height_mm)
             VALUES (?1, 2, '2026-10-31T10:00:00+00:00', 'sliced', 'read', 'Bambu Lab X1 Carbon', 0.2)",
            [id],
        ).unwrap();
        conn.pragma_update(None, "user_version", PRINT_INFO_TABLES_MIGRATION_VERSION).unwrap();
        (conn, id)
    }

    #[test]
    fn the_filament_colors_step_adds_a_nullable_column_and_keeps_every_row_unchanged() {
        let (mut conn, id) = catalog_before_filament_colors();
        assert!(conn.prepare("SELECT filament_colors FROM file_print_info").is_err(), "precondition: no column yet");
        let before = catalog_snapshot(&conn);

        run_migrations(&mut conn).unwrap();

        assert_eq!(catalog_snapshot(&conn), before, "files, tags, file_tags and materials stay as they were");
        let (model, layer, version, colors): (String, f64, i64, Option<String>) = conn
            .query_row(
                "SELECT printer_model, layer_height_mm, reader_version, filament_colors FROM file_print_info WHERE file_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!((model.as_str(), layer, version, colors), ("Bambu Lab X1 Carbon", 0.2, 2, None), "old rows keep their values, colors unknown");
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert_eq!(FILAMENT_COLORS_MIGRATION_VERSION, PRINT_INFO_TABLES_MIGRATION_VERSION + 1, "the color column is the step after the print tables");
    }

    #[test]
    fn the_filament_colors_step_is_repeatable_and_part_of_a_fresh_schema() {
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        fresh.prepare("SELECT filament_colors FROM file_print_info").unwrap();
        let id = crate::db::test_insert_minimal_file(&fresh, "/a.3mf", None).unwrap();
        fresh.execute(
            "INSERT INTO file_print_info (file_id, reader_version, read_at, slice_state, settings_state, filament_colors)
             VALUES (?1, 3, 'n', 'sliced', 'read', '#ff0000 #00ff00')",
            [id],
        ).unwrap();
        add_print_info_filament_colors(&fresh).unwrap();
        add_print_info_filament_colors(&fresh).unwrap();
        let colors: String = fresh.query_row("SELECT filament_colors FROM file_print_info", [], |r| r.get(0)).unwrap();
        assert_eq!(colors, "#ff0000 #00ff00", "repeating the step keeps the value");
    }

    /// A catalog at version 54: everything but the table of remembered copies, with rows in the
    /// tables other tools read.
    fn catalog_before_known_copies() -> (Connection, i64) {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch("DROP TABLE disk_sync_known_copies").unwrap();
        let folder = crate::db::insert_folder(&conn, "root").unwrap();
        let id = crate::db::test_insert_minimal_file(&conn, "/models/a.3mf", Some(folder)).unwrap();
        conn.execute("INSERT INTO tags (id, name, color_hue) VALUES (1, 'deko', 10)", []).unwrap();
        conn.execute("INSERT INTO file_tags VALUES (?1, 1)", [id]).unwrap();
        conn.execute("INSERT INTO source_rejected (path, size_bytes, rejected_at) VALUES ('/d/x.stl', 5, 't')", []).unwrap();
        conn.pragma_update(None, "user_version", FILAMENT_COLORS_MIGRATION_VERSION).unwrap();
        (conn, id)
    }

    #[test]
    fn the_known_copies_step_adds_an_empty_table_and_keeps_every_row_unchanged() {
        let (mut conn, _) = catalog_before_known_copies();
        assert!(!table_exists(&conn, "disk_sync_known_copies"), "precondition: no table yet");
        let before = catalog_snapshot(&conn);

        run_migrations(&mut conn).unwrap();

        assert_eq!(catalog_snapshot(&conn), before, "files, tags, file_tags and materials stay byte for byte as they were");
        assert!(table_exists(&conn, "disk_sync_known_copies"));
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM disk_sync_known_copies", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0, "the table starts empty");
        let declined: i64 = conn.query_row("SELECT COUNT(*) FROM source_rejected", [], |r| r.get(0)).unwrap();
        assert_eq!(declined, 1, "the declined source files are another table and stay");
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert_eq!(CURRENT_SCHEMA_VERSION, SLICER_REPORTS_MIGRATION_VERSION + 1, "plate dimensions follow slicer reports");
        assert_eq!(GCODE_RESIN_FILE_TYPES_MIGRATION_VERSION, FILAMENT_COLORS_MIGRATION_VERSION + 2, "after the table of remembered copies, which follows the color column");
        assert_eq!(BUNDLE_TABLES_MIGRATION_VERSION, GCODE_RESIN_FILE_TYPES_MIGRATION_VERSION + 1, "the bundle tables follow the file type step");
        assert_eq!(PRINTER_FAILURES_MIGRATION_VERSION, BUNDLE_TABLES_MIGRATION_VERSION + 1, "the failure counter follows the bundle tables");
        assert_eq!(PRINTER_API_KEY_MIGRATION_VERSION, PRINTER_FAILURES_MIGRATION_VERSION + 1, "the API key follows the failure counter");
        assert_eq!(COMPANION_OTHER_KIND_MIGRATION_VERSION, PRINTER_API_KEY_MIGRATION_VERSION + 1, "the companion kind follows the API key");
        assert_eq!(PRINTER_JOB_FAILURE_MIGRATION_VERSION, COMPANION_OTHER_KIND_MIGRATION_VERSION + 1, "the print failure step follows the companion kind");
        assert_eq!(AMS_2_PRO_MIGRATION_VERSION, PRINTER_JOB_FAILURE_MIGRATION_VERSION + 1, "the AMS 2 Pro step follows the print failure step");
        assert_eq!(SLICER_REPORTS_MIGRATION_VERSION, AMS_2_PRO_MIGRATION_VERSION + 1, "the slicer report step follows the AMS 2 Pro step");
        assert_eq!(CURRENT_SCHEMA_VERSION, 64);
    }

    #[test]
    fn the_known_copies_step_is_repeatable_and_gives_the_same_table_as_a_fresh_schema() {
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        fresh.execute(
            "INSERT INTO disk_sync_known_copies (path, size_bytes, content_hash, noted_at) VALUES ('/m/a.3mf', 1, 'h', 't')",
            [],
        ).unwrap();
        add_known_copies_table(&fresh).unwrap();
        add_known_copies_table(&fresh).unwrap();
        let rows: i64 = fresh.query_row("SELECT COUNT(*) FROM disk_sync_known_copies", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1, "repeating the step keeps the row");
        assert!(fresh.execute(
            "INSERT INTO disk_sync_known_copies (path, size_bytes, content_hash, noted_at) VALUES ('/m/a.3mf', 1, 'h', 't')",
            [],
        ).is_err(), "the path is unique");

        let shape = |conn: &Connection| -> Vec<(String, String, bool, bool)> {
            conn.prepare("SELECT name, type, \"notnull\", pk FROM pragma_table_info('disk_sync_known_copies') ORDER BY cid")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let by_step = Connection::open_in_memory().unwrap();
        add_known_copies_table(&by_step).unwrap();
        assert_eq!(shape(&by_step), shape(&fresh), "schema.sql and the step must describe the same table");
        assert_eq!(shape(&fresh).len(), 5);
    }

    /// A catalog at version 56 with a connection that failed before the counter existed.
    fn catalog_before_failure_counter() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        // The state of a 56 catalog: no counter columns yet.
        conn.execute_batch("ALTER TABLE printer_connections DROP COLUMN failures; ALTER TABLE printer_connections DROP COLUMN last_attempt_at;").unwrap();
        conn.execute("INSERT INTO printers (id, name, position) VALUES (1, 'Sovol SV08', 0)", []).unwrap();
        conn.execute(
            "INSERT INTO printer_connections (printer_id, kind, address, connected_since, last_error, error_since, paused)
             VALUES (1, 'moonraker', '192.168.1.60', 1000.0, 'unreachable', 2000.0, 0)",
            [],
        ).unwrap();
        conn.pragma_update(None, "user_version", GCODE_RESIN_FILE_TYPES_MIGRATION_VERSION).unwrap();
        conn
    }

    #[test]
    fn the_failure_counter_step_adds_two_columns_and_keeps_the_connections() {
        let mut conn = catalog_before_failure_counter();
        let before = catalog_snapshot(&conn);
        run_migrations(&mut conn).unwrap();

        assert_eq!(catalog_snapshot(&conn), before, "the catalog tables stay as they were");
        let (failures, last_attempt, error, since): (i64, Option<f64>, String, f64) = conn
            .query_row(
                "SELECT failures, last_attempt_at, last_error, error_since FROM printer_connections WHERE printer_id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!((failures, last_attempt, error.as_str(), since), (0, None, "unreachable", 2000.0));
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION, "the run goes on to the newest step");
        assert!(conn.execute("UPDATE printer_connections SET failures = -1", []).is_err(), "the counter cannot go negative");
    }

    #[test]
    fn the_failure_counter_step_is_repeatable_and_leaves_stored_values_alone() {
        let conn = catalog_before_failure_counter();
        add_printer_connection_failures(&conn).unwrap();
        conn.execute("UPDATE printer_connections SET failures = 3, last_attempt_at = 5.0", []).unwrap();
        add_printer_connection_failures(&conn).unwrap();
        let (failures, last): (i64, f64) = conn
            .query_row("SELECT failures, last_attempt_at FROM printer_connections", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((failures, last), (3, 5.0));
    }

    #[test]
    fn the_failure_counter_step_gives_the_same_columns_as_a_fresh_schema() {
        let shape = |conn: &Connection| -> Vec<(String, String, bool, Option<String>)> {
            conn.prepare("SELECT name, type, \"notnull\", dflt_value FROM pragma_table_info('printer_connections') ORDER BY name")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        let mut migrated = catalog_before_failure_counter();
        run_migrations(&mut migrated).unwrap();
        assert_eq!(shape(&migrated), shape(&fresh), "schema.sql and the step must describe the same columns");
    }

    /// A catalog at version 56: everything but the bundle tables, with rows in the tables around them.
    fn catalog_before_bundle_tables() -> (Connection, i64) {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch("DROP TABLE bundle_members; DROP TABLE bundles").unwrap();
        let id = crate::db::test_insert_minimal_file(&conn, "/a.3mf", None).unwrap();
        conn.execute("INSERT INTO tags (id, name, color_hue) VALUES (1, 'deko', 10)", []).unwrap();
        conn.execute("INSERT INTO file_tags VALUES (?1, 1)", [id]).unwrap();
        conn.execute("INSERT INTO collections (id, name, created_at) VALUES (1, 'Set', 't')", []).unwrap();
        conn.execute("INSERT INTO collection_files (collection_id, file_id, position) VALUES (1, ?1, 0)", [id]).unwrap();
        conn.pragma_update(None, "user_version", GCODE_RESIN_FILE_TYPES_MIGRATION_VERSION).unwrap();
        (conn, id)
    }

    #[test]
    fn the_bundle_step_adds_empty_tables_and_keeps_every_row_of_every_table() {
        let (mut conn, _) = catalog_before_bundle_tables();
        assert!(!table_exists(&conn, "bundles") && !table_exists(&conn, "bundle_members"), "precondition: no tables yet");
        let before = catalog_snapshot(&conn);
        let counts_before = row_counts(&conn);

        run_migrations(&mut conn).unwrap();

        assert_eq!(catalog_snapshot(&conn), before);
        let mut expected = counts_before;
        expected.push(("bundle_members".to_string(), 0));
        expected.push(("bundles".to_string(), 0));
        expected.sort();
        assert_eq!(row_counts(&conn), expected, "only the two new tables appear, both empty");
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert_eq!(BUNDLE_TABLES_MIGRATION_VERSION, 57, "the bundle step keeps its number");
    }

    /// A catalog at version 62: everything but the slicer report table, with rows around it.
    fn catalog_before_slicer_reports() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch("DROP TABLE slicer_reports").unwrap();
        let id = crate::db::test_insert_minimal_file(&conn, "/a.3mf", None).unwrap();
        conn.execute("INSERT INTO tags (id, name, color_hue) VALUES (1, 'deko', 10)", []).unwrap();
        conn.execute("INSERT INTO file_tags VALUES (?1, 1)", [id]).unwrap();
        conn.pragma_update(None, "user_version", AMS_2_PRO_MIGRATION_VERSION).unwrap();
        conn
    }

    #[test]
    fn the_slicer_report_step_adds_one_empty_table_and_keeps_every_row_of_every_table() {
        let mut conn = catalog_before_slicer_reports();
        assert!(!table_exists(&conn, "slicer_reports"), "precondition: no table yet");
        let before = catalog_snapshot(&conn);
        let counts_before = row_counts(&conn);

        run_migrations(&mut conn).unwrap();

        assert_eq!(catalog_snapshot(&conn), before);
        let mut expected = counts_before;
        expected.push(("slicer_reports".to_string(), 0));
        expected.sort();
        assert_eq!(row_counts(&conn), expected, "only the new table appears, and it is empty");
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert_eq!(SLICER_REPORTS_MIGRATION_VERSION, 63, "the slicer report step is number 63");
    }

    #[test]
    fn the_slicer_report_step_is_repeatable_and_gives_the_same_table_and_rules_as_a_fresh_schema() {
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        fresh.execute("INSERT INTO slicer_reports (reported_at, reported_path, file_name, file_size_bytes, summary_json) VALUES ('t', '/o/a.gcode', 'a.gcode', 1, '{}')", []).unwrap();
        add_slicer_reports_table(&fresh).unwrap();
        add_slicer_reports_table(&fresh).unwrap();
        let rows: i64 = fresh.query_row("SELECT COUNT(*) FROM slicer_reports", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1, "repeating the step keeps the rows");

        let by_step = Connection::open_in_memory().unwrap();
        by_step.execute_batch("CREATE TABLE files (id INTEGER PRIMARY KEY)").unwrap();
        add_slicer_reports_table(&by_step).unwrap();
        let shape = |conn: &Connection| -> Vec<(String, String, bool, Option<String>, bool)> {
            conn.prepare("SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info('slicer_reports') ORDER BY cid")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(shape(&by_step), shape(&fresh), "schema.sql and the step must describe the table the same way");
        assert_eq!(shape(&fresh).len(), 11);
        let index: i64 = by_step.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name = 'idx_slicer_reports_name'", [], |r| r.get(0)).unwrap();
        assert_eq!(index, 1);
        for conn in [&fresh, &by_step] {
            let bad = |sql: &str| conn.execute(sql, []).is_err();
            assert!(bad("INSERT INTO slicer_reports (reported_at, reported_path, file_name, file_size_bytes, summary_json, state) VALUES ('t', 'p', 'n', 1, '{}', 'weird')"));
            assert!(bad("INSERT INTO slicer_reports (reported_at, reported_path, file_name, file_size_bytes, summary_json) VALUES ('t', 'p', 'n', -1, '{}')"));
        }
    }

    #[test]
    fn the_bundle_step_is_repeatable_and_gives_the_same_tables_as_a_fresh_schema() {
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        let a = crate::db::test_insert_minimal_file(&fresh, "/a.3mf", None).unwrap();
        let b = crate::db::test_insert_minimal_file(&fresh, "/b.3mf", None).unwrap();
        fresh.execute("INSERT INTO bundles (id, name, kind, created_at) VALUES (1, 'Vase', 'parts', 't')", []).unwrap();
        fresh.execute("INSERT INTO bundle_members (file_id, bundle_id, position) VALUES (?1, 1, 0), (?2, 1, 1)", [a, b]).unwrap();
        add_bundle_tables(&fresh).unwrap();
        add_bundle_tables(&fresh).unwrap();
        let rows: i64 = fresh.query_row("SELECT COUNT(*) FROM bundle_members", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 2, "repeating the step keeps the rows");

        let sql = |conn: &Connection| -> Vec<(String, String)> {
            conn.prepare("SELECT name, sql FROM sqlite_master WHERE name IN ('bundles', 'bundle_members', 'idx_bundle_members_bundle_id') ORDER BY name")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let shape = |conn: &Connection, table: &str| -> Vec<(String, String, bool, bool)> {
            conn.prepare(&format!("SELECT name, type, \"notnull\", pk FROM pragma_table_info('{table}') ORDER BY cid"))
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let by_step = Connection::open_in_memory().unwrap();
        by_step.execute_batch("CREATE TABLE files (id INTEGER PRIMARY KEY)").unwrap();
        add_bundle_tables(&by_step).unwrap();
        for table in ["bundles", "bundle_members"] {
            assert_eq!(shape(&by_step, table), shape(&fresh, table), "schema.sql and the step must describe {table} the same way");
        }
        assert_eq!(sql(&by_step).len(), 3, "both tables and the index");
        assert_eq!(shape(&fresh, "bundles").len(), 4);
        assert_eq!(shape(&fresh, "bundle_members").len(), 4);
    }

    #[test]
    fn bundle_tables_enforce_their_rules_and_hang_on_files_only_by_a_foreign_key() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        let a = crate::db::test_insert_minimal_file(&conn, "/a.3mf", None).unwrap();
        let b = crate::db::test_insert_minimal_file(&conn, "/b.3mf", None).unwrap();
        for bad in [
            "INSERT INTO bundles (name, kind, created_at) VALUES ('', 'parts', 't')",
            "INSERT INTO bundles (name, kind, created_at) VALUES ('x', 'other', 't')",
        ] {
            assert!(conn.execute(bad, []).is_err(), "{bad}");
        }
        let long_name = "n".repeat(121);
        assert!(conn.execute("INSERT INTO bundles (name, kind, created_at) VALUES (?1, 'parts', 't')", [long_name]).is_err());
        conn.execute("INSERT INTO bundles (id, name, kind, created_at) VALUES (1, 'A', 'parts', 't'), (2, 'B', 'variants', 't')", []).unwrap();
        conn.execute("INSERT INTO bundle_members (file_id, bundle_id, position) VALUES (?1, 1, 0)", [a]).unwrap();
        assert!(
            conn.execute("INSERT INTO bundle_members (file_id, bundle_id, position) VALUES (?1, 2, 0)", [a]).is_err(),
            "a file is in at most one bundle"
        );
        assert!(conn.execute("INSERT INTO bundle_members (file_id, bundle_id, position) VALUES (?1, 1, -1)", [b]).is_err());
        assert!(conn.execute("INSERT INTO bundle_members (file_id, bundle_id, position) VALUES (?1, 99, 0)", [b]).is_err(), "bundle must exist");
        assert!(conn.execute("INSERT INTO bundle_members (file_id, bundle_id, position) VALUES (9999, 1, 0)", []).is_err(), "file must exist");
        let long_label = "l".repeat(61);
        assert!(conn.execute("INSERT INTO bundle_members (file_id, bundle_id, position, label) VALUES (?1, 1, 1, ?2)", rusqlite::params![b, long_label]).is_err());
        conn.execute("INSERT INTO bundle_members (file_id, bundle_id, position, label) VALUES (?1, 1, 1, 'Lid')", [b]).unwrap();

        // Deleting a file removes its membership; deleting a bundle removes its members only.
        conn.execute("DELETE FROM files WHERE id = ?1", [a]).unwrap();
        let members: i64 = conn.query_row("SELECT COUNT(*) FROM bundle_members", [], |r| r.get(0)).unwrap();
        assert_eq!(members, 1);
        conn.execute("DELETE FROM bundles WHERE id = 1", []).unwrap();
        let members: i64 = conn.query_row("SELECT COUNT(*) FROM bundle_members", [], |r| r.get(0)).unwrap();
        assert_eq!(members, 0);
        let files: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0)).unwrap();
        assert_eq!(files, 1, "dissolving a bundle never touches a file");

        // Bundles must not need a trigger: the backup check refuses every trigger.
        let triggers: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'", [], |r| r.get(0)).unwrap();
        assert_eq!(triggers, 0);
        // Only the two bundle tables point at `files` from this change; `files` itself keeps no key to them.
        let keys: Vec<String> = conn
            .prepare("SELECT \"table\" FROM pragma_foreign_key_list('bundle_members') ORDER BY \"table\"")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(keys, vec!["bundles".to_string(), "files".to_string()]);
        let files_keys: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_list('files') WHERE \"table\" LIKE 'bundle%'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(files_keys, 0);
    }

    /// A catalog at version 58: connections exist, the key column does not.
    fn catalog_before_api_key() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch("ALTER TABLE printer_connections DROP COLUMN api_key;").unwrap();
        conn.execute("INSERT INTO printers (id, name, position) VALUES (1, 'Sovol SV08', 0)", []).unwrap();
        conn.execute(
            "INSERT INTO printer_connections (printer_id, kind, address, connected_since, failures) VALUES (1, 'moonraker', '192.168.1.60', 1000.0, 3)",
            [],
        ).unwrap();
        conn.pragma_update(None, "user_version", PRINTER_FAILURES_MIGRATION_VERSION).unwrap();
        conn
    }

    #[test]
    fn the_api_key_step_adds_an_empty_column_and_keeps_the_connections() {
        let mut conn = catalog_before_api_key();
        let has_column = |c: &Connection| -> bool {
            c.query_row("SELECT COUNT(*) FROM pragma_table_info('printer_connections') WHERE name = 'api_key'", [], |r| r.get::<_, i64>(0)).unwrap() == 1
        };
        assert!(!has_column(&conn), "precondition: no column yet");
        run_migrations(&mut conn).unwrap();
        assert!(has_column(&conn));
        let (address, failures, key): (String, i64, Option<String>) = conn
            .query_row("SELECT address, failures, api_key FROM printer_connections WHERE printer_id = 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!((address.as_str(), failures, key), ("192.168.1.60", 3, None));
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION, "later steps run as well");
    }

    #[test]
    fn the_api_key_step_is_repeatable_and_leaves_a_stored_key_alone() {
        let conn = catalog_before_api_key();
        add_printer_connection_api_key(&conn).unwrap();
        conn.execute("UPDATE printer_connections SET api_key = 'k'", []).unwrap();
        add_printer_connection_api_key(&conn).unwrap();
        let key: String = conn.query_row("SELECT api_key FROM printer_connections", [], |r| r.get(0)).unwrap();
        assert_eq!(key, "k");
    }

    #[test]
    fn the_api_key_step_gives_the_same_columns_as_a_fresh_schema() {
        let shape = |conn: &Connection| -> Vec<(String, String, bool, Option<String>)> {
            conn.prepare("SELECT name, type, \"notnull\", dflt_value FROM pragma_table_info('printer_connections') ORDER BY name")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        let mut migrated = catalog_before_api_key();
        run_migrations(&mut migrated).unwrap();
        assert_eq!(shape(&migrated), shape(&fresh));
    }

    /// A catalog at version 59 with the old `kind` CHECK and companion rows in it.
    fn catalog_before_other_kind() -> (Connection, i64) {
        let conn = crate::db::connect_in_memory().unwrap();
        let id = crate::db::test_insert_minimal_file(&conn, "/models/a.3mf", None).unwrap();
        conn.execute_batch(
            "DROP TABLE companion_files;
             CREATE TABLE companion_files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL CHECK (kind IN ('image', 'pdf', 'text')),
                size_bytes INTEGER,
                added_at TEXT NOT NULL,
                deleted_at TEXT,
                trash_path TEXT
             );
             CREATE INDEX idx_companion_files_file_id ON companion_files (file_id);",
        ).unwrap();
        conn.execute("INSERT INTO companion_files (id, file_id, name, path, kind, size_bytes, added_at) VALUES (7, ?1, 'p.png', '/models/a_extras/p.png', 'image', 12, 't1')", [id]).unwrap();
        conn.execute("INSERT INTO companion_files (id, file_id, name, path, kind, added_at, deleted_at, trash_path) VALUES (9, ?1, 'n.txt', '/models/a_extras/n.txt', 'text', 't2', 't3', '/trash/9')", [id]).unwrap();
        conn.pragma_update(None, "user_version", PRINTER_API_KEY_MIGRATION_VERSION).unwrap();
        // The unmigrated table really refuses the new kind.
        assert!(conn.execute("INSERT INTO companion_files (file_id, name, path, kind, added_at) VALUES (?1, 'x.fcstd', '/x', 'other', 't')", [id]).is_err());
        (conn, id)
    }

    #[test]
    fn the_other_kind_step_keeps_every_companion_row_and_accepts_only_the_new_kind() {
        let (mut conn, id) = catalog_before_other_kind();
        let rows = |c: &Connection| -> Vec<String> {
            c.prepare("SELECT id || '|' || file_id || '|' || name || '|' || path || '|' || kind || '|' || ifnull(size_bytes, '-') || '|' || added_at || '|' || ifnull(deleted_at, '-') || '|' || ifnull(trash_path, '-') FROM companion_files ORDER BY id")
                .unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
        };
        let before = rows(&conn);
        run_migrations(&mut conn).unwrap();
        assert_eq!(rows(&conn), before, "no value of an existing row may change");
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        conn.execute("INSERT INTO companion_files (file_id, name, path, kind, added_at) VALUES (?1, 'x.fcstd', '/models/a_extras/x.fcstd', 'other', 't')", [id]).unwrap();
        assert!(conn.execute("INSERT INTO companion_files (file_id, name, path, kind, added_at) VALUES (?1, 'y', '/y', 'exe', 't')", [id]).is_err(), "the rest stays invalid");
        assert!(conn.execute("INSERT INTO companion_files (file_id, name, path, kind, added_at) VALUES (?1, 'p.png', '/models/a_extras/p.png', 'image', 't')", [id]).is_err(), "the path is still unique");
        let index: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_companion_files_file_id'", [], |r| r.get(0)).unwrap();
        assert_eq!(index, 1, "the index comes back with the table");
        // The table keeps following its model, and new ids continue after the old ones.
        let next: i64 = conn.query_row("SELECT MAX(id) FROM companion_files", [], |r| r.get(0)).unwrap();
        assert_eq!(next, 10);
        conn.execute("DELETE FROM files WHERE id = ?1", [id]).unwrap();
        let left: i64 = conn.query_row("SELECT COUNT(*) FROM companion_files", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0, "deleting the model removes its companion rows");
    }

    #[test]
    fn the_other_kind_step_is_repeatable_and_gives_the_same_table_as_a_fresh_schema() {
        let shape = |conn: &Connection| -> Vec<String> {
            let mut lines: Vec<String> = conn
                .prepare("SELECT name || '|' || type || '|' || \"notnull\" || '|' || ifnull(dflt_value, '-') || '|' || pk FROM pragma_table_info('companion_files') ORDER BY cid")
                .unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
            lines.extend(
                conn.prepare("SELECT name FROM sqlite_master WHERE tbl_name = 'companion_files' ORDER BY name")
                    .unwrap().query_map([], |r| r.get::<_, String>(0)).unwrap().collect::<Result<Vec<_>, _>>().unwrap(),
            );
            lines.extend(
                conn.prepare("SELECT \"table\" || '|' || \"from\" || '|' || \"to\" || '|' || on_delete FROM pragma_foreign_key_list('companion_files')")
                    .unwrap().query_map([], |r| r.get::<_, String>(0)).unwrap().collect::<Result<Vec<_>, _>>().unwrap(),
            );
            lines
        };
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        let (mut migrated, _) = catalog_before_other_kind();
        run_migrations(&mut migrated).unwrap();
        assert_eq!(shape(&migrated), shape(&fresh));
        // A second run (the step again, on a table that already has the kind) changes nothing.
        let before = shape(&migrated);
        migrated.pragma_update(None, "user_version", PRINTER_API_KEY_MIGRATION_VERSION).unwrap();
        run_migrations(&mut migrated).unwrap();
        assert_eq!(shape(&migrated), before);
        let rows: i64 = migrated.query_row("SELECT COUNT(*) FROM companion_files", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 2);
    }

    /// A catalog at version 60: prints exist, the failure columns do not.
    fn catalog_before_job_failure() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch(
            "ALTER TABLE printer_jobs DROP COLUMN failure_reason;
             ALTER TABLE printer_jobs DROP COLUMN failure_note;
             ALTER TABLE printer_jobs DROP COLUMN auto_closed;",
        )
        .unwrap();
        conn.execute("INSERT INTO printers (id, name, position) VALUES (1, 'Sovol SV08', 0)", []).unwrap();
        conn.execute_batch(
            "INSERT INTO printer_jobs (printer_id, remote_id, file_name, outcome, raw_status, ended_at, print_duration_s, used_mm, state, decided_at)
                 VALUES (1, 'A@1', 'a.gcode', 'completed', 'completed', 2000.0, 10.0, 5.0, 'open', NULL),
                        (1, 'B@2', 'b.gcode', 'partial', 'cancelled', 3000.0, 10.0, 7.0, 'ignored', 't');",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", COMPANION_OTHER_KIND_MIGRATION_VERSION).unwrap();
        conn
    }

    #[test]
    fn the_job_failure_step_adds_empty_columns_and_keeps_the_prints() {
        let mut conn = catalog_before_job_failure();
        let columns = |c: &Connection| -> i64 {
            c.query_row("SELECT COUNT(*) FROM pragma_table_info('printer_jobs') WHERE name IN ('failure_reason', 'failure_note', 'auto_closed')", [], |r| r.get(0)).unwrap()
        };
        assert_eq!(columns(&conn), 0, "precondition: no columns yet");
        run_migrations(&mut conn).unwrap();
        assert_eq!(columns(&conn), 3);
        let rows: Vec<String> = conn
            .prepare("SELECT remote_id || '|' || state || '|' || ifnull(failure_reason, '-') || '|' || ifnull(failure_note, '-') || '|' || auto_closed FROM printer_jobs ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            ["A@1|open|-|-|0", "B@2|ignored|-|-|0"],
            "every old print stays as it was: not closed by the program, no reason"
        );
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION, "the later steps run as well");
        assert!(conn.execute("UPDATE printer_jobs SET auto_closed = 2", []).is_err(), "the flag is 0 or 1");
    }

    #[test]
    fn the_job_failure_step_is_repeatable_and_leaves_stored_values_alone() {
        let conn = catalog_before_job_failure();
        add_printer_job_failure_columns(&conn).unwrap();
        conn.execute("UPDATE printer_jobs SET failure_reason = 'clog', failure_note = 'n', auto_closed = 1 WHERE remote_id = 'B@2'", []).unwrap();
        add_printer_job_failure_columns(&conn).unwrap();
        let row: (String, String, i64) = conn
            .query_row("SELECT failure_reason, failure_note, auto_closed FROM printer_jobs WHERE remote_id = 'B@2'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!(row, ("clog".into(), "n".into(), 1));
    }

    #[test]
    fn the_job_failure_step_gives_the_same_columns_as_a_fresh_schema() {
        let shape = |conn: &Connection| -> Vec<(String, String, bool, Option<String>)> {
            conn.prepare("SELECT name, type, \"notnull\", dflt_value FROM pragma_table_info('printer_jobs') ORDER BY name")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        let mut migrated = catalog_before_job_failure();
        run_migrations(&mut migrated).unwrap();
        assert_eq!(shape(&migrated), shape(&fresh));
    }

    /// Measurement for the schema 60 step (spec P4, 6.2): 50 000 companion rows in a catalog
    /// file on disk. Run on purpose, it prints the time:
    /// `cargo test -p katalog-core --lib measure_other_kind_step -- --ignored --nocapture`.
    #[test]
    #[ignore = "measurement, not a check"]
    fn measure_other_kind_step_with_50000_companion_rows() {
        const MODELS: i64 = 1_000;
        const PER_MODEL: i64 = 50;
        let dir = std::env::temp_dir().join(format!("mfk-mig60-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("catalog.db");
        let mut conn = Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        conn.execute_batch(
            "DROP TABLE companion_files;
             CREATE TABLE companion_files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL CHECK (kind IN ('image', 'pdf', 'text')),
                size_bytes INTEGER,
                added_at TEXT NOT NULL,
                deleted_at TEXT,
                trash_path TEXT
             );
             CREATE INDEX idx_companion_files_file_id ON companion_files (file_id);",
        )
        .unwrap();
        {
            let tx = conn.transaction().unwrap();
            for model in 0..MODELS {
                let id = crate::db::test_insert_minimal_file(&tx, &format!("/models/m{model}.3mf"), None).unwrap();
                for n in 0..PER_MODEL {
                    tx.execute(
                        "INSERT INTO companion_files (file_id, name, path, kind, size_bytes, added_at) VALUES (?1, ?2, ?3, 'image', 1000, 't')",
                        rusqlite::params![id, format!("p{n}.png"), format!("/models/m{model}_extras/p{n}.png")],
                    )
                    .unwrap();
                }
            }
            tx.commit().unwrap();
        }
        conn.pragma_update(None, "user_version", PRINTER_API_KEY_MIGRATION_VERSION).unwrap();
        let started = std::time::Instant::now();
        add_other_to_companion_kind_check(&mut conn, COMPANION_OTHER_KIND_MIGRATION_VERSION).unwrap();
        let elapsed = started.elapsed();
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM companion_files", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, MODELS * PER_MODEL, "every row survives");
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, COMPANION_OTHER_KIND_MIGRATION_VERSION);
        println!("MIGRATION60_MEASURE rows={rows} elapsed_ms={}", elapsed.as_millis());
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A catalog at version 55 with the old `file_type` CHECK, rows in the tables that point at `files`.
    fn catalog_with_old_file_type_check() -> (Connection, i64) {
        let conn = crate::db::connect_in_memory().unwrap();
        let folder = crate::db::insert_folder(&conn, "root").unwrap();
        let id = crate::db::test_insert_minimal_file(&conn, "/models/a.3mf", Some(folder)).unwrap();
        let id_b = crate::db::test_insert_minimal_file(&conn, "/models/b.stl", Some(folder)).unwrap();
        conn.execute("INSERT INTO tags (id, name, color_hue) VALUES (1, 'deko', 10)", []).unwrap();
        conn.execute("INSERT INTO bundles (id, name, kind, created_at) VALUES (1, 'Vase', 'variants', 't')", []).unwrap();
        conn.execute("INSERT INTO bundle_members (file_id, bundle_id, position) VALUES (?1, 1, 0), (?2, 1, 1)", [id, id_b]).unwrap();
        conn.execute("INSERT INTO file_tags VALUES (?1, 1)", [id]).unwrap();
        conn.execute("INSERT INTO print_log (file_id, printed_at, created_at) VALUES (?1, 't', 't')", [id]).unwrap();
        conn.execute(
            "INSERT INTO companion_files (file_id, name, path, kind, added_at) VALUES (?1, 'a.png', '/models/a_extras/a.png', 'image', 't')",
            [id],
        ).unwrap();
        conn.execute(
            "INSERT INTO file_print_info (file_id, reader_version, read_at, slice_state, settings_state)
             VALUES (?1, 3, 'n', 'no_slice_data', 'missing')",
            [id],
        ).unwrap();
        let old_sql: String = conn
            .query_row("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'files'", [], |r| r.get(0))
            .unwrap();
        let old_create = files_sql_with_check(&old_sql, "'3mf', 'stl', 'stp', 'obj'").expect("the CHECK is found");
        conn.pragma_update(None, "foreign_keys", false).unwrap();
        conn.execute_batch(&format!(
            "{old_create};
             INSERT INTO files_new SELECT * FROM files;
             DROP TABLE files;
             ALTER TABLE files_new RENAME TO files;
             CREATE INDEX idx_files_folder_id ON files (folder_id);
             CREATE INDEX idx_files_file_type ON files (file_type);
             CREATE INDEX idx_files_content_hash ON files (content_hash);"
        ))
        .unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.pragma_update(None, "user_version", GCODE_RESIN_FILE_TYPES_MIGRATION_VERSION - 1).unwrap();
        // Precondition: the old CHECK really is in force.
        assert!(conn.execute("UPDATE files SET file_type = 'gcode' WHERE id = ?1", [id]).is_err());
        (conn, id)
    }

    fn row_counts(conn: &Connection) -> Vec<(String, i64)> {
        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        names
            .into_iter()
            .map(|n| {
                let c: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM \"{n}\""), [], |r| r.get(0)).unwrap();
                (n, c)
            })
            .collect()
    }

    #[test]
    fn the_gcode_resin_step_widens_the_check_and_keeps_every_row_of_every_table() {
        let (mut conn, id) = catalog_with_old_file_type_check();
        let counts_before = row_counts(&conn);
        let snapshot_before = catalog_snapshot(&conn);
        let columns = |conn: &Connection| -> Vec<(String, String, bool, bool)> {
            conn.prepare("SELECT name, type, \"notnull\", pk FROM pragma_table_info('files') ORDER BY cid")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let shape_before = columns(&conn);

        run_migrations(&mut conn).unwrap();

        assert_eq!(row_counts(&conn), counts_before, "no row of any table is lost or added");
        assert_eq!(catalog_snapshot(&conn), snapshot_before);
        assert_eq!(columns(&conn), shape_before, "columns, types and nullability stay the same");
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        let on: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert!(on, "foreign keys are switched on again");
        let indexes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND tbl_name = 'files'
                   AND name IN ('idx_files_folder_id', 'idx_files_file_type', 'idx_files_content_hash')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(indexes, 3);
        let violations: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0)).unwrap();
        assert_eq!(violations, 0);
        // The children still follow their parent.
        conn.execute("DELETE FROM files WHERE id = ?1", [id]).unwrap();
        let tags: i64 = conn.query_row("SELECT COUNT(*) FROM file_tags", [], |r| r.get(0)).unwrap();
        assert_eq!(tags, 0, "ON DELETE CASCADE still works after the rebuild");
        let members: i64 = conn.query_row("SELECT COUNT(*) FROM bundle_members", [], |r| r.get(0)).unwrap();
        assert_eq!(members, 1, "the bundle membership follows its file after the rebuild, the other one stays");
    }

    #[test]
    fn after_the_gcode_resin_step_both_new_types_are_accepted_and_others_are_not() {
        let (mut conn, _) = catalog_with_old_file_type_check();
        run_migrations(&mut conn).unwrap();
        for (i, kind) in ["gcode", "resin"].iter().enumerate() {
            let inserted = conn.execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('x', ?1, ?2, 1, 't')",
                rusqlite::params![format!("/p/{i}"), kind],
            );
            assert!(inserted.is_ok(), "{kind} is accepted");
        }
        let rejected = conn.execute(
            "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('x', '/p/z', 'xyz', 1, 't')",
            [],
        );
        assert!(rejected.is_err(), "an unknown type is still refused");
    }

    #[test]
    fn the_gcode_resin_step_is_repeatable_and_a_fresh_schema_needs_no_rebuild() {
        let (mut conn, _) = catalog_with_old_file_type_check();
        run_migrations(&mut conn).unwrap();
        let counts = row_counts(&conn);
        // Run the step again on the migrated table (schema.sql has the same CHECK).
        add_gcode_and_resin_to_file_type_check(&mut conn, GCODE_RESIN_FILE_TYPES_MIGRATION_VERSION).unwrap();
        assert_eq!(row_counts(&conn), counts);
        let mut fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut fresh).unwrap();
        let sql = |c: &Connection| -> String {
            c.query_row("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'files'", [], |r| r.get(0)).unwrap()
        };
        assert!(sql(&fresh).contains("'gcode', 'resin'"));
        assert!(sql(&conn).contains("'gcode', 'resin'"));
    }

    /// Counter-proof for the trap the step avoids: the same rebuild with foreign keys on deletes the
    /// children of every file.
    #[test]
    fn a_rebuild_of_files_with_foreign_keys_on_would_lose_the_child_rows() {
        let (conn, _) = catalog_with_old_file_type_check();
        conn.execute_batch(
            "CREATE TABLE files_new AS SELECT * FROM files;
             DROP TABLE files;
             ALTER TABLE files_new RENAME TO files;",
        )
        .unwrap();
        let tags: i64 = conn.query_row("SELECT COUNT(*) FROM file_tags", [], |r| r.get(0)).unwrap();
        assert_eq!(tags, 0, "dropping `files` with foreign keys on cascades into file_tags");
    }

    #[test]
    fn opening_a_version_54_catalog_through_init_migrates_it() {
        let (mut conn, id) = catalog_before_known_copies();
        crate::db::repository::init(&mut conn).unwrap();
        assert!(table_exists(&conn, "disk_sync_known_copies"));
        assert!(crate::db::get_file(&conn, id).unwrap().is_some());
    }

    #[test]
    fn opening_a_previous_version_catalog_through_init_migrates_it() {
        let (mut conn, id) = previous_version_catalog();
        crate::db::repository::init(&mut conn).unwrap();
        assert!(table_exists(&conn, "companion_files"));
        assert!(crate::db::get_file(&conn, id).unwrap().is_some());
    }

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
        // Build the complete historical schema, including printer tables created before `kind`.
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations_with(&mut conn, &MIGRATIONS[..(KIND_MIGRATION_VERSION - 1) as usize], KIND_MIGRATION_VERSION - 1).unwrap();
        conn.execute("INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at) VALUES ('PLA', 1.75, 1000, 600, '2026-01-01')", []).unwrap();

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
        assert_eq!(CURRENT_SCHEMA_VERSION, CLOCK_OFFSET_MIGRATION_VERSION + 25);
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

    /// State before step 62: the unit CHECK of step 37, two units, one spool in a slot.
    fn db_before_ams_2_pro() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP TABLE material_units;
             CREATE TABLE material_units (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                printer_id INTEGER NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                kind TEXT NOT NULL CHECK (kind IN ('bambu_ams', 'bambu_ams_lite', 'bambu_ams_ht', 'creality_cfs',
                                                   'prusa_mmu3', 'anycubic_ace', 'external', 'custom', 'resin_vat')),
                slot_count INTEGER NOT NULL CHECK (slot_count BETWEEN 1 AND 16),
                bambu_ams_index INTEGER CHECK (bambu_ams_index BETWEEN 0 AND 3),
                position INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO printers (id, name, position) VALUES (1, 'X1C', 0);
             INSERT INTO material_units (id, printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (10, 1, 'AMS A', 'bambu_ams', 4, 0, 0), (11, 1, 'AMS lite', 'bambu_ams_lite', 4, 1, 1);
             INSERT INTO filament_spools (id, material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index, home_location)
                 VALUES (100, 'PLA', 1.75, 1000, 600, '2026-09-01', 10, 2, 'Regal 2');",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", AMS_2_PRO_MIGRATION_VERSION - 1).unwrap();
        conn
    }

    #[test]
    fn the_ams_2_pro_step_allows_the_new_kind_and_keeps_every_unit_and_loaded_spool() {
        let mut conn = db_before_ams_2_pro();
        assert!(
            conn.execute("INSERT INTO material_units (printer_id, name, kind, slot_count) VALUES (1, 'X', 'bambu_ams_2_pro', 4)", []).is_err(),
            "precondition: the old CHECK refuses the kind"
        );
        // As in repository::init: foreign_keys is on BEFORE the migrations, which arms the cascade trap.
        conn.pragma_update(None, "foreign_keys", true).unwrap();

        run_migrations(&mut conn).unwrap();

        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION, "the later steps run as well");
        let fk: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert!(fk, "foreign_keys is back on afterwards");

        let units: Vec<(i64, String, String, i64, Option<i64>, i64)> = conn
            .prepare("SELECT id, name, kind, slot_count, bambu_ams_index, position FROM material_units ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            units,
            vec![
                (10, "AMS A".into(), "bambu_ams".into(), 4, Some(0), 0),
                (11, "AMS lite".into(), "bambu_ams_lite".into(), 4, Some(1), 1),
            ],
            "no unit is converted automatically"
        );
        let slot: (Option<i64>, Option<i64>, Option<String>) = conn
            .query_row("SELECT unit_id, slot_index, home_location FROM filament_spools WHERE id = 100", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(slot, (Some(10), Some(2), Some("Regal 2".into())), "the spool stays in its slot");

        conn.execute("INSERT INTO material_units (printer_id, name, kind, slot_count, position) VALUES (1, 'AMS 2 Pro', 'bambu_ams_2_pro', 4, 2)", [])
            .expect("the new kind is allowed");
        assert!(conn
            .execute("INSERT INTO material_units (printer_id, name, kind, slot_count, position) VALUES (1, 'X', 'toaster', 1, 3)", [])
            .is_err());
        conn.execute("DELETE FROM material_units WHERE id = 10", []).unwrap();
        let unit_id: Option<i64> = conn.query_row("SELECT unit_id FROM filament_spools WHERE id = 100", [], |r| r.get(0)).unwrap();
        assert_eq!(unit_id, None, "ON DELETE SET NULL still applies");
    }

    #[test]
    fn the_ams_2_pro_step_is_a_no_op_rebuild_on_a_fresh_db_and_repeatable() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        conn.execute("INSERT INTO printers (id, name) VALUES (1, 'X1C')", []).unwrap();
        conn.execute("INSERT INTO material_units (id, printer_id, name, kind, slot_count) VALUES (5, 1, 'AMS 2 Pro', 'bambu_ams_2_pro', 4)", []).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.pragma_update(None, "user_version", AMS_2_PRO_MIGRATION_VERSION - 1).unwrap();
        run_migrations(&mut conn).unwrap();
        let kind: String = conn.query_row("SELECT kind FROM material_units WHERE id = 5", [], |r| r.get(0)).unwrap();
        assert_eq!(kind, "bambu_ams_2_pro");
    }
}
