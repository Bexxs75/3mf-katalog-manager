//! Validation and preparation of foreign catalog copies. Activation belongs to the host.
use crate::import::{ImportError as CmdError, ImportResult as CmdResult};
use crate::{companions, db, error_code::ErrorCode};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

mod imported_paths;
mod inspection;
pub use inspection::{inspect_backup, PathWarning, RestoreReport};
mod std_fs;
pub use std_fs::StdRestoreFs;

/// Owns an isolated database and removes it (including sidecars) on drop.
/// The host must not expose the copy as its active database until preparation succeeds.
pub trait TemporaryCatalog {
    fn connection(&mut self) -> &mut Connection;
}

/// Platform boundary. Implementations must isolate copies from the active catalog.
/// Resolve must canonicalize existing ancestors and reject unresolved parent traversal.
/// Server implementations must enforce their handle-relative filesystem policy here.
pub trait RestoreFs {
    fn resolve(&self, path: &Path) -> CmdResult<PathBuf>;
    fn temporary_catalog(&self, bytes: &[u8]) -> CmdResult<Box<dyn TemporaryCatalog>>;
}

/// Only these explicit trash locations may be restored. Roots are host configuration,
/// never roots obtained from the untrusted backup.
pub struct TrashPolicy<'a> {
    pub desktop_trash: Option<&'a Path>,
    pub catalog_roots: &'a [PathBuf],
}

// Restoring existing names must preserve legal host-platform names; only path
// components that could escape a joined directory are unsafe here.
fn validate_file_name(name: &str) -> CmdResult<()> {
    let mut components = Path::new(name).components();
    if name.trim().is_empty()
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
        || !matches!(
            (components.next(), components.next()),
            (Some(std::path::Component::Normal(_)), None)
        )
    {
        return Err(CmdError::coded(ErrorCode::BackupEntryNameInvalid));
    }
    #[cfg(windows)]
    {
        let stem = name
            .split('.')
            .next()
            .unwrap_or(name)
            .trim_end()
            .to_uppercase();
        if name.contains(':')
            || crate::import::file_names::WINDOWS_RESERVED_NAMES.contains(&stem.as_str())
        {
            return Err(CmdError::coded(ErrorCode::BackupEntryNameUnsafeOnWindows));
        }
    }
    Ok(())
}

use imported_paths::validate_path as validate_imported_local_path;

fn sanitize_imported_file_identity(conn: &Connection) -> Result<(), String> {
    // Duplicate decisions need local hashes; import dates retain the catalog history.
    conn.execute("UPDATE files SET content_hash = NULL", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn validate_imported_path_length(path: &str, known_folders: &[String]) -> Result<(), String> {
    if path.chars().take(4097).count() > 4096 {
        return Err("Importierter Pfad darf höchstens 4096 Zeichen enthalten".into());
    }
    validate_imported_local_path(path, known_folders)
}

const MAX_IMPORT_DB_BYTES: u64 = 256 * 1024 * 1024;
const MAX_IMPORT_SETTINGS_BYTES: u64 = 1024 * 1024;

/// Web uploads accept exactly the portable backup pair. Header consistency also
/// rejects duplicate central-directory names before the ZIP reader deduplicates them.
pub fn read_catalog_archive_strict<R: std::io::Read + std::io::Seek>(
    reader: R,
) -> CmdResult<(Vec<u8>, String)> {
    use std::io::Read;
    let invalid = || CmdError::coded(ErrorCode::BackupNotZip);
    let mut archive = zip::ZipArchive::new(reader).map_err(|_| invalid())?;
    if archive.len() != 2 {
        return Err(invalid());
    }
    let mut database = None;
    let mut settings = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_with_options(
                index,
                zip::read::ZipReadOptions::new().enforce_header_consistency(true),
            )
            .map_err(|_| invalid())?;
        let name = entry.name().map_err(|_| invalid())?.into_owned();
        let limit = match name.as_str() {
            "catalog.db" => MAX_IMPORT_DB_BYTES,
            "settings.json" => MAX_IMPORT_SETTINGS_BYTES,
            _ => return Err(CmdError::coded(ErrorCode::BackupEntryNameInvalid)),
        };
        if entry.is_dir() || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err(invalid());
        }
        reject_oversized_zip_entry(&name, entry.size(), limit)?;
        let mut bytes = Vec::new();
        entry
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid())?;
        reject_oversized_zip_entry(&name, bytes.len() as u64, limit)?;
        if name == "catalog.db" {
            database = Some(bytes);
        } else {
            settings = Some(bytes);
        }
    }
    let database = database
        .ok_or_else(|| CmdError::coded_with(ErrorCode::BackupMissingEntry, ["catalog.db"]))?;
    let settings = settings
        .ok_or_else(|| CmdError::coded_with(ErrorCode::BackupMissingEntry, ["settings.json"]))?;
    Ok((
        database,
        String::from_utf8(settings)
            .map_err(|_| CmdError::coded(ErrorCode::BackupSettingsNotUtf8))?,
    ))
}

/// The declared size can lie; the hard limit is `Read::take` while reading.
/// This check only saves the read attempt for honestly oversized entries. An
/// oversized entry is about the archive the user picked, not a fault here.
fn reject_oversized_zip_entry(name: &str, size: u64, max: u64) -> CmdResult<()> {
    if size > max {
        // Whole megabytes, rounded up: a decimal separator would need the user's locale.
        return Err(CmdError::coded_with(
            ErrorCode::BackupEntryTooLarge,
            [
                name.to_string(),
                size.div_ceil(1024 * 1024).to_string(),
                (max / (1024 * 1024)).to_string(),
            ],
        ));
    }
    Ok(())
}
/// Reads the two entries a catalog backup ZIP must contain. Generic over the
/// reader so this is unit-testable with an in-memory `Cursor` instead of a real
/// file. Every failure here is about the archive the user picked (wrong file,
/// backup from an unrelated app, truncated download, ...), never a fault of
/// this app - hence `CmdError::expected` throughout, not a plain `?`.
pub fn read_catalog_archive<R: std::io::Read + std::io::Seek>(
    reader: R,
) -> CmdResult<(Vec<u8>, String)> {
    use std::io::Read as _;

    let mut archive =
        zip::ZipArchive::new(reader).map_err(|_| CmdError::coded(ErrorCode::BackupNotZip))?;

    let mut db_bytes = Vec::new();
    {
        let entry = archive
            .by_name("catalog.db")
            .map_err(|_| CmdError::coded_with(ErrorCode::BackupMissingEntry, ["catalog.db"]))?;
        reject_oversized_zip_entry("catalog.db", entry.size(), MAX_IMPORT_DB_BYTES)?;
        entry
            .take(MAX_IMPORT_DB_BYTES)
            .read_to_end(&mut db_bytes)
            .map_err(|e| e.to_string())?;
    }

    let mut settings_bytes = Vec::new();
    {
        let entry = archive
            .by_name("settings.json")
            .map_err(|_| CmdError::coded_with(ErrorCode::BackupMissingEntry, ["settings.json"]))?;
        reject_oversized_zip_entry("settings.json", entry.size(), MAX_IMPORT_SETTINGS_BYTES)?;
        entry
            .take(MAX_IMPORT_SETTINGS_BYTES)
            .read_to_end(&mut settings_bytes)
            .map_err(|e| e.to_string())?;
    }
    let settings_json = String::from_utf8(settings_bytes)
        .map_err(|_| CmdError::coded(ErrorCode::BackupSettingsNotUtf8))?;

    Ok((db_bytes, settings_json))
}
/// Tables and columns the imported schema must contain. `table` only comes from
/// this constant, so interpolating it into the PRAGMA is safe.
const REQUIRED_COLUMNS: &[(&str, &[&str])] = &[
    (
        "files",
        &[
            "id",
            "name",
            "path",
            "file_type",
            "folder_id",
            "file_size_bytes",
            "imported_at",
            "deleted_at",
            "trash_path",
            "license",
            "source_platform",
            "source_model_id",
            "source_profile_id",
        ],
    ),
    ("folders", &["id", "name", "parent_id", "path"]),
    ("tags", &["id", "name", "color_hue"]),
    ("file_tags", &["file_id", "tag_id"]),
    ("file_metadata", &["file_id", "label", "value"]),
    (
        "filament_spools",
        &[
            "id",
            "material",
            "remaining_weight_g",
            "unit_id",
            "slot_index",
            "home_location",
            "color_hex",
            "color_hexes",
            "kind",
            "nozzle_temp_min",
            "nozzle_temp_max",
            "bed_temp_min",
            "bed_temp_max",
        ],
    ),
    ("printers", &["id", "name", "position", "kind"]),
    (
        "material_units",
        &[
            "id",
            "printer_id",
            "name",
            "kind",
            "slot_count",
            "bambu_ams_index",
            "position",
        ],
    ),
    ("collections", &["id", "name", "created_at"]),
    (
        "collection_files",
        &["collection_id", "file_id", "position"],
    ),
    (
        "registered_slicers",
        &["id", "name", "executable_path", "is_auto_detected"],
    ),
    ("app_settings", &["key", "value"]),
    (
        "companion_files",
        &[
            "id",
            "file_id",
            "name",
            "path",
            "kind",
            "size_bytes",
            "added_at",
            "deleted_at",
            "trash_path",
        ],
    ),
    ("source_folders", &["id", "path", "added_at"]),
    (
        "source_rejected",
        &["id", "path", "size_bytes", "rejected_at"],
    ),
    ("blocked_auto_tags", &["name", "blocked_at"]),
    (
        "disk_sync_known_copies",
        &["id", "path", "size_bytes", "content_hash", "noted_at"],
    ),
    (
        "printer_connections",
        &["printer_id", "kind", "address", "connected_since", "paused"],
    ),
    (
        "printer_jobs",
        &[
            "id",
            "printer_id",
            "remote_id",
            "file_name",
            "outcome",
            "raw_status",
            "ended_at",
            "print_duration_s",
            "used_mm",
            "state",
        ],
    ),
    (
        "file_print_info",
        &[
            "file_id",
            "reader_version",
            "read_at",
            "slice_state",
            "settings_state",
        ],
    ),
    (
        "file_print_plates",
        &["file_id", "plate_index", "first_layer_seconds"],
    ),
    ("bundles", &["id", "name", "kind", "created_at"]),
    ("bundle_members", &["file_id", "bundle_id", "position"]),
    (
        "slicer_reports",
        &[
            "id",
            "reported_at",
            "reported_path",
            "file_name",
            "file_size_bytes",
            "summary_json",
            "state",
            "suggested_file_id",
            "model_file_id",
        ],
    ),
];

fn validate_expected_schema(conn: &Connection) -> Result<(), String> {
    for (table, required_columns) in REQUIRED_COLUMNS {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|e| e.to_string())?;
        let existing_columns: std::collections::HashSet<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        if existing_columns.is_empty() {
            return Err(format!(
                "Katalog-Datenbank enthaelt nicht die erforderliche Tabelle '{table}'"
            ));
        }
        for column in *required_columns {
            if !existing_columns.contains(*column) {
                return Err(format!(
                    "Katalog-Datenbank: Tabelle '{table}' fehlt die erforderliche Spalte '{column}'"
                ));
            }
        }
    }
    Ok(())
}
/// Allowed SQLite storage class(es) per Rust read type (NULL separately via `nullable`).
#[derive(Clone, Copy)]
enum ColType {
    /// `i64` or `bool`: 'integer' only.
    Integer,
    /// `f64`: 'integer' or 'real'.
    Real,
    /// `String`: accepts ONLY `typeof(...) = 'text'`.
    Text,
    /// `Vec<u8>`: accepts ONLY `typeof(...) = 'blob'`.
    Blob,
}

impl ColType {
    fn allowed_typeof(self, nullable: bool) -> Vec<&'static str> {
        let mut allowed: Vec<&'static str> = match self {
            ColType::Integer => vec!["integer"],
            ColType::Real => vec!["integer", "real"],
            ColType::Text => vec!["text"],
            ColType::Blob => vec!["blob"],
        };
        if nullable {
            allowed.push("null");
        }
        allowed
    }
}
/// Columns read as typed values in Rust, with their allowed storage class and
/// whether NULL is allowed. Columns only used in ORDER BY/WHERE/binds are left out
/// on purpose: they can't cause a `FromSql` error.
const COLUMN_TYPES: &[(&str, &str, ColType, bool)] = &[
    // files - see row_to_file/list_file_summaries (repository.rs).
    ("files", "id", ColType::Integer, false),
    ("files", "name", ColType::Text, false),
    ("files", "path", ColType::Text, false),
    ("files", "file_type", ColType::Text, false),
    ("files", "folder_id", ColType::Integer, true),
    ("files", "origin", ColType::Text, false),
    ("files", "sync_status", ColType::Text, false),
    ("files", "cloud_id", ColType::Text, true),
    ("files", "file_size_bytes", ColType::Integer, false),
    ("files", "dimension_x_mm", ColType::Real, true),
    ("files", "dimension_y_mm", ColType::Real, true),
    ("files", "dimension_z_mm", ColType::Real, true),
    ("files", "volume_cm3", ColType::Real, true),
    ("files", "object_count", ColType::Integer, true),
    ("files", "thumbnail_png", ColType::Blob, true),
    ("files", "imported_at", ColType::Text, false),
    ("files", "file_modified_at", ColType::Text, true),
    ("files", "print_status", ColType::Text, false),
    ("files", "last_viewed_at", ColType::Text, true),
    ("files", "creator", ColType::Text, true),
    ("files", "content_hash", ColType::Text, true),
    ("files", "render_snapshot_png", ColType::Blob, true),
    ("files", "custom_image_png", ColType::Blob, true),
    ("files", "source_url", ColType::Text, true),
    ("files", "license", ColType::Text, true),
    ("files", "source_platform", ColType::Text, true),
    ("files", "source_model_id", ColType::Text, true),
    ("files", "source_profile_id", ColType::Text, true),
    ("files", "queue_position", ColType::Integer, true),
    ("files", "favorite", ColType::Integer, false),
    ("files", "plate_count", ColType::Integer, true),
    ("files", "slice_info_json", ColType::Text, true),
    ("files", "plate_dimensions_json", ColType::Text, true),
    ("files", "deleted_at", ColType::Text, true),
    ("files", "trash_path", ColType::Text, true),
    // folders - see list_folders (repository.rs).
    ("folders", "id", ColType::Integer, false),
    ("folders", "name", ColType::Text, false),
    ("folders", "parent_id", ColType::Integer, true),
    ("folders", "path", ColType::Text, false),
    // tags - see list_tag_counts/load_tags/get_or_create_tag (repository.rs).
    ("tags", "id", ColType::Integer, false),
    ("tags", "name", ColType::Text, false),
    ("tags", "color_hue", ColType::Integer, false),
    // file_tags - see list_all_file_tags (repository.rs).
    ("file_tags", "file_id", ColType::Integer, false),
    // file_metadata - see load_metadata (repository.rs).
    ("file_metadata", "label", ColType::Text, false),
    ("file_metadata", "value", ColType::Text, false),
    // filament_spools - see list_filament_spools/get_filament_spool
    // (repository.rs) and load_spool/unload_spool (printers.rs).
    ("filament_spools", "id", ColType::Integer, false),
    ("filament_spools", "material", ColType::Text, false),
    ("filament_spools", "manufacturer", ColType::Text, true),
    ("filament_spools", "color", ColType::Text, true),
    ("filament_spools", "location", ColType::Text, true),
    ("filament_spools", "diameter_mm", ColType::Real, false),
    ("filament_spools", "original_weight_g", ColType::Real, false),
    (
        "filament_spools",
        "remaining_weight_g",
        ColType::Real,
        false,
    ),
    ("filament_spools", "price", ColType::Real, true),
    ("filament_spools", "image_png", ColType::Blob, true),
    ("filament_spools", "color_hex", ColType::Text, true),
    ("filament_spools", "color_hexes", ColType::Text, true),
    ("filament_spools", "home_location", ColType::Text, true),
    ("filament_spools", "unit_id", ColType::Integer, true),
    ("filament_spools", "slot_index", ColType::Integer, true),
    ("filament_spools", "kind", ColType::Text, false),
    ("filament_spools", "nozzle_temp_min", ColType::Integer, true),
    ("filament_spools", "nozzle_temp_max", ColType::Integer, true),
    ("filament_spools", "bed_temp_min", ColType::Integer, true),
    ("filament_spools", "bed_temp_max", ColType::Integer, true),
    // printers - see list_printers (printers.rs).
    ("printers", "id", ColType::Integer, false),
    ("printers", "name", ColType::Text, false),
    ("printers", "kind", ColType::Text, false),
    // material_units - see list_units (printers.rs).
    ("material_units", "id", ColType::Integer, false),
    ("material_units", "printer_id", ColType::Integer, false),
    ("material_units", "name", ColType::Text, false),
    ("material_units", "kind", ColType::Text, false),
    ("material_units", "slot_count", ColType::Integer, false),
    ("material_units", "bambu_ams_index", ColType::Integer, true),
    // collections - see list_collections (collections.rs).
    ("collections", "id", ColType::Integer, false),
    ("collections", "name", ColType::Text, false),
    // collection_files - see list_collection_file_ids.
    ("collection_files", "file_id", ColType::Integer, false),
    // registered_slicers - see list_registered_slicers/get_registered_slicer (repository.rs).
    ("registered_slicers", "id", ColType::Integer, false),
    ("registered_slicers", "name", ColType::Text, false),
    (
        "registered_slicers",
        "executable_path",
        ColType::Text,
        false,
    ),
    (
        "registered_slicers",
        "is_auto_detected",
        ColType::Integer,
        false,
    ),
    // companion_files, source_folders, source_rejected - see db/companions.rs and db/sources.rs.
    ("companion_files", "id", ColType::Integer, false),
    ("companion_files", "file_id", ColType::Integer, false),
    ("companion_files", "name", ColType::Text, false),
    ("companion_files", "path", ColType::Text, false),
    ("companion_files", "kind", ColType::Text, false),
    ("companion_files", "size_bytes", ColType::Integer, true),
    ("companion_files", "added_at", ColType::Text, false),
    ("companion_files", "deleted_at", ColType::Text, true),
    ("companion_files", "trash_path", ColType::Text, true),
    ("source_folders", "id", ColType::Integer, false),
    ("source_folders", "path", ColType::Text, false),
    ("source_folders", "added_at", ColType::Text, false),
    ("source_rejected", "id", ColType::Integer, false),
    ("source_rejected", "path", ColType::Text, false),
    ("source_rejected", "size_bytes", ColType::Integer, false),
    ("source_rejected", "rejected_at", ColType::Text, false),
    // blocked_auto_tags - see db/blocked_tags.rs.
    ("blocked_auto_tags", "name", ColType::Text, false),
    ("blocked_auto_tags", "blocked_at", ColType::Text, false),
    // disk_sync_known_copies - see db/known_copies.rs.
    ("disk_sync_known_copies", "id", ColType::Integer, false),
    ("disk_sync_known_copies", "path", ColType::Text, false),
    (
        "disk_sync_known_copies",
        "size_bytes",
        ColType::Integer,
        false,
    ),
    (
        "disk_sync_known_copies",
        "content_hash",
        ColType::Text,
        false,
    ),
    ("disk_sync_known_copies", "noted_at", ColType::Text, false),
    // Printer connection - see db/printer_link.rs (connection_from_row, job_from_row).
    ("app_settings", "key", ColType::Text, false),
    ("app_settings", "value", ColType::Text, false),
    ("printer_connections", "printer_id", ColType::Integer, false),
    ("printer_connections", "kind", ColType::Text, false),
    ("printer_connections", "address", ColType::Text, false),
    ("printer_connections", "base_url", ColType::Text, true),
    ("printer_connections", "remote_version", ColType::Text, true),
    (
        "printer_connections",
        "connected_since",
        ColType::Real,
        false,
    ),
    ("printer_connections", "last_synced_at", ColType::Real, true),
    ("printer_connections", "last_error", ColType::Text, true),
    ("printer_connections", "error_since", ColType::Real, true),
    ("printer_connections", "paused", ColType::Integer, false),
    ("printer_connections", "failures", ColType::Integer, false),
    (
        "printer_connections",
        "last_attempt_at",
        ColType::Real,
        true,
    ),
    ("printer_connections", "api_key", ColType::Text, true),
    ("printer_jobs", "id", ColType::Integer, false),
    ("printer_jobs", "printer_id", ColType::Integer, false),
    ("printer_jobs", "remote_id", ColType::Text, false),
    ("printer_jobs", "file_name", ColType::Text, false),
    ("printer_jobs", "outcome", ColType::Text, false),
    ("printer_jobs", "raw_status", ColType::Text, false),
    ("printer_jobs", "ended_at", ColType::Real, false),
    ("printer_jobs", "print_duration_s", ColType::Real, false),
    ("printer_jobs", "used_mm", ColType::Real, false),
    ("printer_jobs", "slicer_total_mm", ColType::Real, true),
    ("printer_jobs", "slicer_weight_g", ColType::Real, true),
    ("printer_jobs", "material", ColType::Text, true),
    ("printer_jobs", "thumbnail_path", ColType::Text, true),
    ("printer_jobs", "state", ColType::Text, false),
    ("printer_jobs", "booked_spool_id", ColType::Integer, true),
    ("printer_jobs", "booked_file_id", ColType::Integer, true),
    ("printer_jobs", "booked_g", ColType::Real, true),
    ("printer_jobs", "decided_at", ColType::Text, true),
    // Not in REQUIRED_COLUMNS on purpose: an older backup gets the columns from the migration that runs first.
    ("printer_jobs", "failure_reason", ColType::Text, true),
    ("printer_jobs", "failure_note", ColType::Text, true),
    ("printer_jobs", "auto_closed", ColType::Integer, false),
    // Print data of a 3MF - see db/repository.rs (load_print_info).
    ("file_print_info", "file_id", ColType::Integer, false),
    ("file_print_info", "reader_version", ColType::Integer, false),
    ("file_print_info", "read_at", ColType::Text, false),
    ("file_print_info", "slice_state", ColType::Text, false),
    ("file_print_info", "settings_state", ColType::Text, false),
    ("file_print_info", "printer_model", ColType::Text, true),
    ("file_print_info", "bed_type", ColType::Text, true),
    ("file_print_info", "layer_height_mm", ColType::Real, true),
    ("file_print_info", "nozzle_mm", ColType::Real, true),
    ("file_print_info", "nozzle_mixed", ColType::Integer, false),
    ("file_print_info", "bed_temp_c", ColType::Real, true),
    ("file_print_info", "bed_temp_mixed", ColType::Integer, false),
    ("file_print_info", "nozzle_temp_c", ColType::Real, true),
    (
        "file_print_info",
        "nozzle_temp_mixed",
        ColType::Integer,
        false,
    ),
    // Not in REQUIRED_COLUMNS on purpose: an older backup gets the column from the migration that runs first.
    ("file_print_info", "filament_colors", ColType::Text, true),
    ("file_print_plates", "file_id", ColType::Integer, false),
    ("file_print_plates", "plate_index", ColType::Integer, false),
    ("file_print_plates", "plate_name", ColType::Text, true),
    ("file_print_plates", "print_seconds", ColType::Integer, true),
    (
        "file_print_plates",
        "first_layer_seconds",
        ColType::Real,
        true,
    ),
    ("file_print_plates", "plate_bed_type", ColType::Text, true),
    ("bundles", "id", ColType::Integer, false),
    ("bundles", "name", ColType::Text, false),
    ("bundles", "kind", ColType::Text, false),
    ("bundles", "created_at", ColType::Text, false),
    ("bundle_members", "file_id", ColType::Integer, false),
    ("bundle_members", "bundle_id", ColType::Integer, false),
    ("bundle_members", "position", ColType::Integer, false),
    ("bundle_members", "label", ColType::Text, true),
    // slicer_reports - see db/slicer_reports.rs.
    ("slicer_reports", "id", ColType::Integer, false),
    ("slicer_reports", "reported_at", ColType::Text, false),
    ("slicer_reports", "reported_path", ColType::Text, false),
    ("slicer_reports", "file_name", ColType::Text, false),
    ("slicer_reports", "file_size_bytes", ColType::Integer, false),
    ("slicer_reports", "slicer", ColType::Text, true),
    ("slicer_reports", "summary_json", ColType::Text, false),
    ("slicer_reports", "state", ColType::Text, false),
    (
        "slicer_reports",
        "suggested_file_id",
        ColType::Integer,
        true,
    ),
    ("slicer_reports", "model_file_id", ColType::Integer, true),
    ("slicer_reports", "decided_at", ColType::Text, true),
];
/// Checks the storage classes from `COLUMN_TYPES`. A crafted DB can bring tables
/// without type affinity, e.g. `slot_count = 4.5`: it passes every value check but
/// makes `row.get::<_, i64>` fail on read. Therefore runs before
/// `validate_printer_invariants`.
fn validate_column_types(conn: &Connection) -> Result<(), String> {
    for &(table, column, col_type, nullable) in COLUMN_TYPES {
        let allowed = col_type.allowed_typeof(nullable);
        let placeholders = allowed.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT COUNT(*) FROM \"{table}\" WHERE typeof(\"{column}\") NOT IN ({placeholders})"
        );
        let bad: i64 = conn
            .query_row(
                &sql,
                rusqlite::params_from_iter(allowed.iter().copied()),
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if bad > 0 {
            return Err(format!(
                "Katalog-Datenbank enthaelt einen falschen Datentyp in Tabelle '{table}', Spalte '{column}'"
            ));
        }
    }
    Ok(())
}
/// Checks the printer and slot invariants directly on the data, because an
/// imported DB can omit its CHECK constraints and foreign keys.
///
/// Every condition must cover `IS NULL` explicitly: a comparison with NULL is
/// UNKNOWN, so `kind = NULL` does not satisfy `kind NOT IN (...)` and would slip
/// through, although `list_printers`/`list_units` can't read the row afterwards.
fn validate_printer_invariants(conn: &Connection) -> Result<(), String> {
    let bad_printers: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM printers
             WHERE id IS NULL OR name IS NULL OR position IS NULL
                OR kind IS NULL OR kind NOT IN ('filament', 'resin')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_printers > 0 {
        return Err("Katalog-Datenbank enthaelt Drucker mit fehlendem Pflichtwert".to_string());
    }

    let bad_units: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM material_units
                 WHERE id IS NULL
                    OR printer_id IS NULL
                    OR name IS NULL
                    OR kind IS NULL
                    OR slot_count IS NULL
                    OR position IS NULL
                    OR kind NOT IN ({})
                    OR slot_count NOT BETWEEN 1 AND 16
                    OR (bambu_ams_index IS NOT NULL AND bambu_ams_index NOT BETWEEN 0 AND 3)
                    OR printer_id NOT IN (SELECT id FROM printers)",
                db::printers::UNIT_KINDS
                    .iter()
                    .map(|_| "?")
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            rusqlite::params_from_iter(db::printers::UNIT_KINDS.iter().copied()),
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_units > 0 {
        return Err(
            "Katalog-Datenbank enthaelt ungueltige Drucker-Einheiten (fehlender Pflichtwert, Typ, Fachanzahl, AMS-Nummer oder Drucker-Zuordnung)"
                .to_string(),
        );
    }

    // COALESCE: if the subquery yields NULL, `slot_index >= NULL` would be UNKNOWN
    // and the row invisible. With 0, every index fails.
    let bad_spools: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM filament_spools
             WHERE (unit_id IS NULL) <> (slot_index IS NULL)
                OR (unit_id IS NOT NULL AND unit_id NOT IN (SELECT id FROM material_units))
                OR (unit_id IS NOT NULL AND (
                    slot_index < 0
                    OR slot_index >= COALESCE(
                        (SELECT slot_count FROM material_units m WHERE m.id = filament_spools.unit_id), 0
                    )
                ))",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_spools > 0 {
        return Err("Katalog-Datenbank enthaelt Spulen mit ungueltiger Fach-Zuordnung".to_string());
    }

    let duplicate_slots: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (
                SELECT unit_id, slot_index FROM filament_spools
                WHERE unit_id IS NOT NULL
                GROUP BY unit_id, slot_index HAVING COUNT(*) > 1
             )",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if duplicate_slots > 0 {
        return Err("Katalog-Datenbank enthaelt mehrere Spulen im selben Fach".to_string());
    }

    let bad_temperatures: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM filament_spools
             WHERE nozzle_temp_min NOT BETWEEN 0 AND 500
                OR nozzle_temp_max NOT BETWEEN 0 AND 500
                OR bed_temp_min NOT BETWEEN 0 AND 500
                OR bed_temp_max NOT BETWEEN 0 AND 500
                OR nozzle_temp_min > nozzle_temp_max
                OR bed_temp_min > bed_temp_max",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_temperatures > 0 {
        return Err("Katalog-Datenbank enthaelt ungueltige Drucktemperaturen".to_string());
    }

    let bad_colors: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM filament_spools
             WHERE color_hex IS NOT NULL
               AND color_hex NOT GLOB '#[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_colors > 0 {
        return Err("Katalog-Datenbank enthaelt ungueltige Farbwerte".to_string());
    }

    // The colour list is parsed again on every load; a foreign backup must not bring
    // oversized lists or text that is not a colour.
    let mut color_lists = conn
        .prepare("SELECT color_hexes, color_hex FROM filament_spools WHERE color_hexes IS NOT NULL")
        .map_err(|e| e.to_string())?;
    for row in color_lists
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| e.to_string())?
    {
        let (raw, single) = row.map_err(|e| e.to_string())?;
        // The single colour (slicer matching) must equal the first list entry, as the
        // app itself always stores it.
        let valid = serde_json::from_str::<Vec<String>>(&raw).is_ok_and(|list| {
            crate::db::printers::validate_color_list(&list).is_ok()
                && list.first().is_none_or(|first| {
                    single
                        .as_deref()
                        .is_some_and(|s| s.eq_ignore_ascii_case(first))
                })
        });
        if !valid {
            return Err("Katalog-Datenbank enthaelt ungueltige Farblisten".to_string());
        }
    }
    drop(color_lists);

    // Resin only in a resin vat, filament never. A crafted backup can bring the
    // column without its CHECK.
    let bad_kinds: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM filament_spools
             WHERE kind IS NULL OR kind NOT IN ('filament', 'resin')
                OR (unit_id IS NOT NULL AND (kind = 'resin') <> COALESCE(
                    (SELECT m.kind = 'resin_vat' FROM material_units m WHERE m.id = filament_spools.unit_id), 0
                ))",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_kinds > 0 {
        return Err(
            "Katalog-Datenbank enthaelt Eintraege mit ungueltiger Art (oder Resin/Filament im falschen Drucker)".to_string(),
        );
    }

    // Resin vats have exactly one slot and belong to resin printers only; a resin
    // printer has only its one vat.
    let bad_resin_units: i64 = conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM material_units m JOIN printers p ON p.id = m.printer_id
                 WHERE (m.kind = 'resin_vat') <> (p.kind = 'resin')
                    OR (m.kind = 'resin_vat' AND m.slot_count <> 1))
             + (SELECT COUNT(*) FROM (
                 SELECT printer_id FROM material_units WHERE kind = 'resin_vat'
                 GROUP BY printer_id HAVING COUNT(*) > 1))",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_resin_units > 0 {
        return Err(
            "Katalog-Datenbank enthaelt ungueltige Harzwannen oder Resin-Drucker-Einheiten"
                .to_string(),
        );
    }

    Ok(())
}
/// CHECK values of the printer connection (a crafted backup could bring tables
/// without CHECK if they were created before the migration). The home network
/// check of the address happens on restore in `sanitize_printer_connections`.
fn validate_printer_link_rows(conn: &Connection) -> Result<(), String> {
    // Resin printers have no printer connection.
    let bad: i64 = conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM printer_connections WHERE kind <> 'moonraker' OR paused NOT IN (0, 1)
                    OR failures < 0
                    OR length(api_key) > 256 OR api_key GLOB '*[^!-~]*'
                    OR printer_id IN (SELECT id FROM printers WHERE kind = 'resin'))
             + (SELECT COUNT(*) FROM printer_jobs WHERE outcome NOT IN ('completed', 'partial')
                    OR state NOT IN ('open', 'confirmed', 'ignored'))",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    // The failure fields: a code of the list (no CHECK in the database holds it), only on a print
    // that ended early, a short note, and the flag only on prints nobody has to decide. The codes
    // are constants of ours, so joining them into the statement is safe.
    let reasons = crate::printer_link::failure::FAILURE_REASONS
        .iter()
        .map(|r| format!("'{r}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let bad_jobs: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM printer_jobs WHERE auto_closed NOT IN (0, 1)
                    OR (auto_closed = 1 AND state <> 'ignored')
                    OR (failure_reason IS NOT NULL AND (failure_reason NOT IN ({reasons}) OR outcome <> 'partial'))
                    OR length(failure_note) > {}",
                crate::printer_link::failure::MAX_NOTE_CHARS
            ),
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad + bad_jobs > 0 {
        return Err(
            "Katalog-Datenbank enthaelt ungueltige Druckeranbindungs-Eintraege".to_string(),
        );
    }
    Ok(())
}

/// The print data tables are derived from the 3MF files, but a crafted backup can bring them
/// without primary key, foreign key and CHECK clauses, and `foreign_key_check` only knows
/// declared keys. So every rule is checked on the data itself. A violation rejects the whole
/// backup: nothing is dropped silently, because a damaged backup should not lose data unnoticed.
///
/// Every condition covers `IS NULL` explicitly (a comparison with NULL is UNKNOWN and would
/// slip through). Runs after `validate_column_types`, so the storage classes are known.
fn validate_print_info_rows(conn: &Connection) -> Result<(), String> {
    use crate::threemf::print_info::{
        MAX_LAYER_HEIGHT_MM, MAX_NOZZLE_MM, MAX_PLATES, MAX_SECONDS, MAX_TEMP_C,
        MIN_LAYER_HEIGHT_MM, MIN_NOZZLE_MM, MIN_SECONDS, MIN_TEMP_C, PRINT_INFO_READER_VERSION,
    };
    let count = |sql: &str, params: &[&dyn rusqlite::ToSql]| -> Result<i64, String> {
        conn.query_row(sql, params, |r| r.get(0))
            .map_err(|e| e.to_string())
    };
    let reject = |what: &str| format!("Katalog-Datenbank enthaelt ungueltige Druckdaten ({what})");

    // Reference: every row belongs to an existing 3MF file.
    let orphans = count(
        "SELECT (SELECT COUNT(*) FROM file_print_info i WHERE i.file_id IS NULL OR NOT EXISTS
                    (SELECT 1 FROM files f WHERE f.id = i.file_id AND f.file_type IN ('3mf', 'gcode')))
              + (SELECT COUNT(*) FROM file_print_plates p WHERE p.file_id IS NULL OR NOT EXISTS
                    (SELECT 1 FROM files f WHERE f.id = p.file_id AND f.file_type IN ('3mf', 'gcode')))",
        &[],
    )?;
    if orphans > 0 {
        return Err(reject("Verweis auf keine 3MF- oder G-Code-Datei"));
    }

    // Uniqueness: one info row per file, one plate row per (file, index).
    let duplicates = count(
        "SELECT (SELECT COUNT(*) - COUNT(DISTINCT file_id) FROM file_print_info)
              + (SELECT COUNT(*) FROM (SELECT 1 FROM file_print_plates GROUP BY file_id, plate_index HAVING COUNT(*) > 1))",
        &[],
    )?;
    if duplicates > 0 {
        return Err(reject("doppelte Zeilen"));
    }

    // Plate index and plate count per file.
    let max_plates = MAX_PLATES as i64;
    let plates = count(
        "SELECT (SELECT COUNT(*) FROM file_print_plates
                  WHERE plate_index IS NULL OR typeof(plate_index) <> 'integer' OR plate_index < 1)
              + (SELECT COUNT(*) FROM (SELECT 1 FROM file_print_plates GROUP BY file_id HAVING COUNT(*) > ?1))",
        &[&max_plates],
    )?;
    if plates > 0 {
        return Err(reject("Plattenindex"));
    }

    // States, version and flags.
    let max_version = i64::from(PRINT_INFO_READER_VERSION);
    let max_gcode_version = i64::from(crate::gcode::GCODE_READER_VERSION);
    let states = count(
        "SELECT COUNT(*) FROM file_print_info
         WHERE slice_state IS NULL OR slice_state NOT IN ('sliced', 'no_slice_data', 'not_recognized', 'unreadable')
            OR settings_state IS NULL OR settings_state NOT IN ('read', 'missing', 'unreadable', 'not_attempted')
            OR reader_version IS NULL OR typeof(reader_version) <> 'integer' OR reader_version < 1 OR reader_version > ?1
            OR (reader_version > ?2 AND EXISTS (SELECT 1 FROM files f WHERE f.id = file_print_info.file_id AND f.file_type = 'gcode'))
            OR nozzle_mixed IS NULL OR nozzle_mixed NOT IN (0, 1)
            OR bed_temp_mixed IS NULL OR bed_temp_mixed NOT IN (0, 1)
            OR nozzle_temp_mixed IS NULL OR nozzle_temp_mixed NOT IN (0, 1)",
        &[&max_version, &max_gcode_version],
    )?;
    if states > 0 {
        return Err(reject("Zustand, Version oder Kennzeichen"));
    }

    // Temperatures and print time exist only for a sliced file; a mixed value has no number.
    let inconsistent = count(
        "SELECT (SELECT COUNT(*) FROM file_print_info
                  WHERE (slice_state <> 'sliced' AND (nozzle_temp_c IS NOT NULL OR bed_temp_c IS NOT NULL
                                                      OR nozzle_temp_mixed <> 0 OR bed_temp_mixed <> 0))
                     OR (nozzle_mixed = 1 AND nozzle_mm IS NOT NULL)
                     OR (bed_temp_mixed = 1 AND bed_temp_c IS NOT NULL)
                     OR (nozzle_temp_mixed = 1 AND nozzle_temp_c IS NOT NULL))
              + (SELECT COUNT(*) FROM file_print_plates p JOIN file_print_info i ON i.file_id = p.file_id
                  WHERE p.print_seconds IS NOT NULL AND i.slice_state <> 'sliced')",
        &[],
    )?;
    if inconsistent > 0 {
        return Err(reject("Werte, die der Zustand ausschliesst"));
    }

    // Plausible numbers (never 0 as a placeholder, nothing negative or absurd).
    let numbers = count(
        "SELECT (SELECT COUNT(*) FROM file_print_info
                  WHERE (layer_height_mm IS NOT NULL AND NOT (layer_height_mm BETWEEN ?1 AND ?2))
                     OR (nozzle_mm IS NOT NULL AND NOT (nozzle_mm BETWEEN ?3 AND ?4))
                     OR (bed_temp_c IS NOT NULL AND NOT (bed_temp_c BETWEEN ?5 AND ?6))
                     OR (nozzle_temp_c IS NOT NULL AND NOT (nozzle_temp_c BETWEEN ?5 AND ?6)))
              + (SELECT COUNT(*) FROM file_print_plates
                  WHERE (print_seconds IS NOT NULL AND NOT (print_seconds BETWEEN ?7 AND ?8))
                     OR (first_layer_seconds IS NOT NULL AND NOT (first_layer_seconds BETWEEN ?7 AND ?8)))",
        &[
            &MIN_LAYER_HEIGHT_MM, &MAX_LAYER_HEIGHT_MM, &MIN_NOZZLE_MM, &MAX_NOZZLE_MM, &MIN_TEMP_C, &MAX_TEMP_C,
            &MIN_SECONDS, &MAX_SECONDS,
        ],
    )?;
    if numbers > 0 {
        return Err(reject("Zahl ausserhalb des erlaubten Bereichs"));
    }

    // Texts are shown in the UI: same limits as freshly read ones; the read time must parse.
    let mut info = conn
        .prepare("SELECT read_at, printer_model, bed_type FROM file_print_info")
        .map_err(|e| e.to_string())?;
    let rows = info
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (read_at, printer_model, bed_type) = row.map_err(|e| e.to_string())?;
        if chrono::DateTime::parse_from_rfc3339(&read_at).is_err() {
            return Err(reject("Lesezeitpunkt"));
        }
        for text in [printer_model, bed_type].into_iter().flatten() {
            crate::threemf::source::validate_source_value(&text).map_err(|_| reject("Text"))?;
        }
    }
    drop(info);

    // Filament colors: one text of `#rrggbb` entries separated by single spaces, at most 32
    // (lowercase, no alpha, no empty text); NULL when none are known.
    let mut colors = conn
        .prepare("SELECT filament_colors FROM file_print_info WHERE filament_colors IS NOT NULL")
        .map_err(|e| e.to_string())?;
    let rows = colors
        .query_map([], |row| row.get::<_, rusqlite::types::Value>(0))
        .map_err(|e| e.to_string())?;
    for row in rows {
        let valid = match row.map_err(|e| e.to_string())? {
            rusqlite::types::Value::Text(text) => {
                crate::threemf::print_info::parse_color_list_text(&text).is_some()
            }
            _ => false,
        };
        if !valid {
            return Err(reject("Filamentfarben"));
        }
    }
    drop(colors);
    let mut plate_texts = conn
        .prepare("SELECT plate_name, plate_bed_type FROM file_print_plates")
        .map_err(|e| e.to_string())?;
    let rows = plate_texts
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (name, bed_type) = row.map_err(|e| e.to_string())?;
        for text in [name, bed_type].into_iter().flatten() {
            crate::threemf::source::validate_source_value(&text).map_err(|_| reject("Text"))?;
        }
    }
    Ok(())
}

/// Slicer reports are shown in the UI and their paths are shown to the user (never opened by the app
/// before a decision), so a foreign backup must not bring oversized or control text; the rules the CHECK
/// clauses would enforce are checked on the data itself (a crafted backup can omit them).
fn validate_slicer_report_rows(conn: &Connection) -> Result<(), String> {
    use crate::db::slicer_reports::MAX_SUMMARY_BYTES;
    let reject =
        |what: &str| format!("Katalog-Datenbank enthaelt ungueltige Slicer-Meldungen ({what})");
    let bad_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM slicer_reports
              WHERE state IS NULL OR state NOT IN ('open', 'linked', 'dismissed')
                 OR file_size_bytes IS NULL OR file_size_bytes < 0
                 OR length(CAST(summary_json AS BLOB)) > ?1",
            [MAX_SUMMARY_BYTES as i64],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_rows > 0 {
        return Err(reject("Zustand, Groesse oder Zusammenfassung"));
    }
    let mut stmt = conn
        .prepare("SELECT reported_at, reported_path, file_name, slicer, summary_json, decided_at FROM slicer_reports")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (reported_at, path, name, slicer, summary, decided_at) =
            row.map_err(|e| e.to_string())?;
        if chrono::DateTime::parse_from_rfc3339(&reported_at).is_err()
            || decided_at.is_some_and(|d| chrono::DateTime::parse_from_rfc3339(&d).is_err())
        {
            return Err(reject("Zeitpunkt"));
        }
        let texts_fit = path.chars().count() <= 1024
            && name.chars().count() <= 255
            && slicer.as_deref().is_none_or(|s| s.chars().count() <= 200)
            && [Some(path.as_str()), Some(name.as_str()), slicer.as_deref()]
                .into_iter()
                .flatten()
                .all(|t| !t.chars().any(|c| c.is_control()));
        if !texts_fit || name.is_empty() {
            return Err(reject("Text"));
        }
        if serde_json::from_str::<crate::slicer_report::Summary>(&summary).is_err() {
            return Err(reject("Zusammenfassung"));
        }
    }
    Ok(())
}

/// Bundle names and labels are shown in the UI, so a foreign backup must not bring oversized or
/// control text, and the rules the CHECK clauses would enforce are checked on the data itself
/// (a crafted backup can omit them). A bundle with fewer than two members is accepted on purpose:
/// the app can leave one behind and removes it at the next start, so a backup of the app's own
/// would otherwise be refused. Runs after `validate_column_types`, so the storage classes are known.
fn validate_bundle_rows(conn: &Connection) -> Result<(), String> {
    use crate::db::bundles::{MAX_BUNDLE_LABEL_CHARS, MAX_BUNDLE_MEMBERS, MAX_BUNDLE_NAME_CHARS};
    let reject = |what: &str| format!("Katalog-Datenbank enthaelt ungueltige Buendel ({what})");
    let count = |sql: &str, params: &[&dyn rusqlite::ToSql]| -> Result<i64, String> {
        conn.query_row(sql, params, |r| r.get(0))
            .map_err(|e| e.to_string())
    };

    if count(
        "SELECT COUNT(*) FROM bundles WHERE kind IS NULL OR kind NOT IN ('variants', 'parts')",
        &[],
    )? > 0
    {
        return Err(reject("unbekannte Art"));
    }
    // Reference: a member belongs to an existing bundle and an existing file.
    let orphans = count(
        "SELECT COUNT(*) FROM bundle_members m
          WHERE m.file_id IS NULL OR m.bundle_id IS NULL
             OR NOT EXISTS (SELECT 1 FROM bundles b WHERE b.id = m.bundle_id)
             OR NOT EXISTS (SELECT 1 FROM files f WHERE f.id = m.file_id)",
        &[],
    )?;
    if orphans > 0 {
        return Err(reject("Mitglied ohne Buendel oder ohne Datei"));
    }
    // A file is a member once, and the bundles are not larger than the app allows.
    let duplicates = count(
        "SELECT COUNT(*) - COUNT(DISTINCT file_id) FROM bundle_members",
        &[],
    )?;
    if duplicates > 0 {
        return Err(reject("Datei in mehreren Buendeln"));
    }
    let too_large = count(
        "SELECT COUNT(*) FROM (SELECT 1 FROM bundle_members GROUP BY bundle_id HAVING COUNT(*) > ?1)",
        &[&(MAX_BUNDLE_MEMBERS as i64)],
    )?;
    if too_large > 0 {
        return Err(reject("zu viele Mitglieder"));
    }
    if count(
        "SELECT COUNT(*) FROM bundle_members WHERE position < 0",
        &[],
    )? > 0
    {
        return Err(reject("negative Position"));
    }

    let valid_text =
        |text: &str, max: usize| text.chars().count() <= max && !text.chars().any(char::is_control);
    let mut names = conn
        .prepare("SELECT name FROM bundles")
        .map_err(|e| e.to_string())?;
    let rows = names
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    for row in rows {
        let name = row.map_err(|e| e.to_string())?;
        if name.trim().is_empty() || !valid_text(&name, MAX_BUNDLE_NAME_CHARS) {
            return Err(reject("Name"));
        }
    }
    drop(names);
    let mut labels = conn
        .prepare("SELECT label FROM bundle_members WHERE label IS NOT NULL")
        .map_err(|e| e.to_string())?;
    let rows = labels
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    for row in rows {
        if !valid_text(&row.map_err(|e| e.to_string())?, MAX_BUNDLE_LABEL_CHARS) {
            return Err(reject("Beschriftung"));
        }
    }
    Ok(())
}

/// On restore: drop connections with addresses outside the home network and
/// pause all others until the user tests them again.
fn sanitize_printer_connections(conn: &Connection) -> Result<(), String> {
    let rows: Vec<(i64, String)> = conn
        .prepare("SELECT printer_id, address FROM printer_connections")
        .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
        .map_err(|e| e.to_string())?;
    for (printer_id, address) in rows {
        if !crate::printer_link::address::looks_valid(&address) {
            conn.execute(
                "DELETE FROM printer_connections WHERE printer_id = ?1",
                [printer_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    // The count of failed attempts belongs to the machine the backup came from.
    conn.execute(
        "UPDATE printer_connections SET paused = 1, failures = 0, last_attempt_at = NULL",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
/// Checks that `bytes` are a usable, safe catalog DB BEFORE the existing
/// catalog.db is touched: opens, has the expected schema, no `folders.path` in
/// system directories and every trash path allowed by the explicit policy.
pub fn validate_catalog_db_bytes(
    bytes: &[u8],
    sensitive_dirs: &[PathBuf],
    policy: &TrashPolicy<'_>,
    known_folders: &[String],
    fs: &dyn RestoreFs,
) -> CmdResult<()> {
    let mut temporary = fs.temporary_catalog(bytes)?;
    validate_connection(
        temporary.connection(),
        sensitive_dirs,
        policy,
        known_folders,
        fs,
        None,
    )
}

/// True when `path` lies lexically below one of the host-configured roots. Pure
/// string work: used to decide whether an imported path may touch the filesystem.
pub(super) fn import_path_in_boundary(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| path.starts_with(root))
}

/// `reject_sensitive` for an imported path, behind the optional host boundary.
/// Outside the boundary the path is never resolved (a dead network mount must not
/// stall the check); only a lexical match against the unresolved protected
/// directories still rejects it. Without a boundary (desktop) nothing changes.
fn reject_sensitive_bounded(
    fs: &dyn RestoreFs,
    path: &Path,
    expanded: &[PathBuf],
    lexical: &[PathBuf],
    boundary: Option<&[PathBuf]>,
) -> CmdResult<()> {
    match boundary {
        Some(roots) if !import_path_in_boundary(path, roots) => {
            if lexical.iter().any(|dir| path.starts_with(dir)) {
                Err(CmdError::coded(ErrorCode::BackupInvalidDatabase))
            } else {
                Ok(())
            }
        }
        _ => reject_sensitive(fs, path, expanded),
    }
}

fn validate_connection(
    conn: &mut Connection,
    sensitive_dirs: &[PathBuf],
    policy: &TrashPolicy<'_>,
    known_folders: &[String],
    fs: &dyn RestoreFs,
    boundary: Option<&[PathBuf]>,
) -> CmdResult<()> {
    let result = (|| -> Result<(), String> {
        let mut expanded_dirs = sensitive_dirs.to_vec();
        for dir in sensitive_dirs {
            if let Ok(resolved) = fs.resolve(dir) {
                expanded_dirs.push(resolved);
            }
        }
        // Views, triggers and virtual tables run SQL or code from the foreign file
        // as soon as they are queried - a recursive view named `files` never
        // finishes. The app creates none of them, so they are rejected by
        // looking only at sqlite_schema, before any other statement touches the data.
        conn.pragma_update(None, "trusted_schema", false)
            .map_err(|e| e.to_string())?;
        let mut schema_stmt = conn
            .prepare(
                "SELECT type, name FROM sqlite_schema
                     WHERE type IN ('trigger', 'view')
                        OR (type = 'table' AND sql LIKE 'CREATE VIRTUAL TABLE%')",
            )
            .map_err(|e| e.to_string())?;
        let unexpected_schema_objects: Vec<(String, String)> = schema_stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if !unexpected_schema_objects.is_empty() {
            let names: Vec<String> = unexpected_schema_objects
                .iter()
                .map(|(kind, name)| format!("{kind} '{name}'"))
                .collect();
            return Err(format!(
                "Katalog-Datenbank enthaelt nicht erlaubte Schema-Objekte: {}",
                names.join(", ")
            ));
        }
        drop(schema_stmt);

        conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;

        // quick_check detects structural corruption; before any migration.
        let quick_check: String = conn
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if quick_check != "ok" {
            return Err(format!(
                "Katalog-Datenbank ist beschaedigt (quick_check: {quick_check})"
            ));
        }

        // Migrate first, so older legitimate backups (e.g. without
        // folders.parent_id) don't fail the following checks.
        crate::db::run_migrations(conn).map_err(|e| e.to_string())?;

        // quick_check says nothing about whether the tables and columns the app needs exist.
        validate_expected_schema(conn)?;

        // Existence isn't enough, the storage classes must match too (see `validate_column_types`).
        validate_column_types(conn)?;
        let mut licenses = conn
            .prepare("SELECT license FROM files WHERE license IS NOT NULL")
            .map_err(|e| e.to_string())?;
        for value in licenses
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
        {
            crate::db::licenses::validate_license(Some(&value.map_err(|e| e.to_string())?))
                .map_err(|e| e.to_string())?;
        }
        drop(licenses);
        // Shown in the UI, so a foreign backup must not smuggle in oversized or control text.
        let mut sources = conn.prepare(
                "SELECT source_platform, source_model_id, source_profile_id FROM files
                 WHERE source_platform IS NOT NULL OR source_model_id IS NOT NULL OR source_profile_id IS NOT NULL")
                .map_err(|e| e.to_string())?;
        for row in sources
            .query_map([], |row| {
                Ok([row.get::<_, Option<String>>(0)?, row.get(1)?, row.get(2)?])
            })
            .map_err(|e| e.to_string())?
        {
            for value in row.map_err(|e| e.to_string())?.into_iter().flatten() {
                crate::threemf::source::validate_source_value(&value).map_err(|e| e.to_string())?;
            }
        }
        drop(sources);

        // A DB with a current user_version skips every migration and can bring tables
        // without CHECK, FK and unique index. Without this check e.g.
        // `slot_count = 1000000000` could hang the frontend while rendering, or two
        // spools could end up in the same slot.
        validate_printer_invariants(conn)?;
        validate_printer_link_rows(conn)?;
        validate_print_info_rows(conn)?;
        validate_bundle_rows(conn)?;
        validate_slicer_report_rows(conn)?;
        // quick_check doesn't catch FK violations; check them after the migration.
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(|e| e.to_string())?;
        let mut fk_stmt = conn
            .prepare("PRAGMA foreign_key_check")
            .map_err(|e| e.to_string())?;
        let has_violation = fk_stmt.exists([]).map_err(|e| e.to_string())?;
        if has_violation {
            return Err("Katalog-Datenbank enthaelt Fremdschluessel-Verletzungen".to_string());
        }
        drop(fk_stmt);

        // Validate every stored path lexically before resolving any imported value.
        let mut paths = conn.prepare("SELECT path, 0 FROM folders UNION ALL SELECT path, 0 FROM files UNION ALL SELECT trash_path, 1 FROM files WHERE trash_path IS NOT NULL")
                .map_err(|e| e.to_string())?;
        for row in paths
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
            })
            .map_err(|e| e.to_string())?
        {
            let (path, is_trash) = row.map_err(|e| e.to_string())?;
            if is_trash && path.trim().is_empty() {
                continue;
            }
            validate_imported_path_length(&path, known_folders)?;
        }
        drop(paths);

        let mut stmt = conn
            .prepare("SELECT name, path FROM folders")
            .map_err(|e| e.to_string())?;
        let paths = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (name, path) in paths {
            validate_file_name(&name).map_err(|e| e.message)?;
            validate_imported_path_length(&path, known_folders)?;
            reject_sensitive_bounded(fs, Path::new(&path), &expanded_dirs, sensitive_dirs, boundary)
                .map_err(|e| format!("Ordner-Eintrag im Archiv abgelehnt: {}", e.message))?;
        }

        // Cycles in folders.parent_id would send every path reconstruction into an endless loop.
        let mut folder_edges_stmt = conn
            .prepare("SELECT id, parent_id FROM folders")
            .map_err(|e| e.to_string())?;
        let edges: Vec<(i64, Option<i64>)> = folder_edges_stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let parent_of: std::collections::HashMap<i64, Option<i64>> = edges.into_iter().collect();
        for &start in parent_of.keys() {
            let mut current = start;
            let mut steps = 0usize;
            let mut seen = std::collections::HashSet::new();
            seen.insert(current);
            while let Some(Some(parent)) = parent_of.get(&current) {
                if !seen.insert(*parent) {
                    return Err(format!("Katalog-Datenbank enthaelt einen zyklischen Ordner-Graphen (beteiligt: Ordner-ID {parent})"));
                }
                current = *parent;
                steps += 1;
                if steps > parent_of.len() {
                    break; // defensive upper bound, `seen` should already cover this
                }
            }
        }
        drop(folder_edges_stmt);

        let mut stmt = conn
            .prepare("SELECT name, path, trash_path, id FROM files")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (name, path, trash_path, id) in rows {
            validate_imported_path_length(&path, known_folders)?;
            if let Some(path) = trash_path.as_ref().filter(|p| !p.trim().is_empty()) {
                validate_imported_path_length(path, known_folders)?;
            }
            validate_file_name(&name)
                .map_err(|e| format!("Datei-Eintrag im Archiv abgelehnt: {}", e.message))?;
            reject_sensitive_bounded(fs, Path::new(&path), &expanded_dirs, sensitive_dirs, boundary)
                .map_err(|e| format!("Datei-Eintrag im Archiv abgelehnt: {}", e.message))?;
            if let Some(trash_path) = trash_path.filter(|p| !p.trim().is_empty()) {
                validate_trash_path(
                    fs,
                    Path::new(&trash_path),
                    id,
                    false,
                    policy,
                    &expanded_dirs,
                    boundary,
                )
                .map_err(|e| e.message)?;
            }
        }
        // Companion rows decide which files the app moves, deletes or opens, so each
        // must look like what the app itself writes: allowed kind, an `_extras`
        // folder, no protected location, a trash file only inside the trash.
        let mut stmt = conn
            .prepare("SELECT name, path, kind, trash_path, id FROM companion_files")
            .map_err(|e| e.to_string())?;
        let companions = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (name, path, kind, trash_path, id) in companions {
            validate_imported_path_length(&path, known_folders)?;
            if let Some(path) = trash_path.as_ref().filter(|p| !p.trim().is_empty()) {
                validate_imported_path_length(path, known_folders)?;
            }
            companions::validate_stored_fields(&name, &path, &kind)
                .map_err(|why| format!("Begleitdatei-Eintrag im Archiv abgelehnt ({why})"))?;
            reject_sensitive_bounded(fs, Path::new(&path), &expanded_dirs, sensitive_dirs, boundary)
                .map_err(|e| format!("Begleitdatei-Eintrag im Archiv abgelehnt: {}", e.message))?;
            if let Some(trash_path) = trash_path.filter(|p| !p.trim().is_empty()) {
                validate_trash_path(
                    fs,
                    Path::new(&trash_path),
                    id,
                    true,
                    policy,
                    &expanded_dirs,
                    boundary,
                )
                    .map_err(|e| e.message)?;
            }
        }
        drop(stmt);
        // The paths show up in the sync settings and decide which files the sync skips,
        // so a foreign backup must not bring an endless list or protected locations.
        let copies_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM disk_sync_known_copies", [], |row| {
                row.get(0)
            })
            .map_err(|e| e.to_string())?;
        if copies_count as usize > crate::db::known_copies::MAX_KNOWN_COPIES {
            return Err("Katalog-Datenbank enthaelt zu viele gemerkte Kopien".to_string());
        }
        let mut stmt = conn
            .prepare("SELECT path, size_bytes, content_hash FROM disk_sync_known_copies")
            .map_err(|e| e.to_string())?;
        let copies = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (path, size_bytes, content_hash) in copies {
            crate::db::known_copies::validate_stored_fields(&path, size_bytes, &content_hash)
                .map_err(|why| format!("Eintrag gemerkter Kopien im Archiv abgelehnt ({why})"))?;
            validate_imported_path_length(&path, known_folders)?;
            reject_sensitive_bounded(fs, Path::new(&path), &expanded_dirs, sensitive_dirs, boundary).map_err(|e| {
                format!(
                    "Eintrag gemerkter Kopien im Archiv abgelehnt: {}",
                    e.message
                )
            })?;
        }
        drop(stmt);
        validate_blocked_auto_tags(conn)?;
        Ok(())
    })();
    // Whatever the check found stays in the log; the user only learns that the
    // file is not a usable catalog database.
    result.map_err(|e| {
        log::warn!(target: "backup", "invalid catalog database: {e}");
        CmdError::coded(ErrorCode::BackupInvalidDatabase)
    })
}
/// The names are shown in the settings and compared with every new tag, so a
/// foreign backup must not bring an endless list or oversized or control text.
fn validate_blocked_auto_tags(conn: &Connection) -> Result<(), String> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM blocked_auto_tags", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    if count as usize > crate::db::blocked_tags::MAX_BLOCKED_TAGS {
        return Err("Katalog-Datenbank enthaelt zu viele gesperrte Tags".to_string());
    }
    let names: Vec<String> = conn
        .prepare("SELECT name FROM blocked_auto_tags")
        .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
        .map_err(|e| e.to_string())?;
    for name in names {
        crate::db::blocked_tags::normalize_blocked_name(&name).map_err(|_| {
            "Katalog-Datenbank enthaelt einen ungueltigen gesperrten Tag".to_string()
        })?;
    }
    Ok(())
}

fn reject_sensitive(fs: &dyn RestoreFs, path: &Path, dirs: &[PathBuf]) -> CmdResult<()> {
    let resolved = fs.resolve(path)?;
    if dirs.iter().any(|dir| resolved.starts_with(dir)) {
        return Err(CmdError::coded(ErrorCode::BackupInvalidDatabase));
    }
    Ok(())
}

fn validate_trash_path(
    fs: &dyn RestoreFs,
    path: &Path,
    id: i64,
    companion: bool,
    policy: &TrashPolicy<'_>,
    sensitive: &[PathBuf],
    boundary: Option<&[PathBuf]>,
) -> CmdResult<()> {
    let invalid = || CmdError::coded(ErrorCode::BackupInvalidDatabase);
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(invalid());
    }
    // Only the explicit trash locations can pass below, and all of them lie inside
    // the host boundary; anything else is refused without resolving it.
    if boundary.is_some_and(|roots| !import_path_in_boundary(path, roots)) {
        return Err(invalid());
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(invalid)?;
    validate_file_name(name)?;
    let prefix = if companion {
        format!("companion-{id}-")
    } else {
        format!("{id}-")
    };
    let tail = name.strip_prefix(&prefix).ok_or_else(invalid)?;
    let parent = path.parent().ok_or_else(invalid)?;
    let resolved = fs.resolve(path)?;
    let resolved_parent = fs.resolve(parent)?;
    if resolved.parent() != Some(resolved_parent.as_path())
        || resolved.file_name() != path.file_name()
    {
        return Err(invalid());
    }
    if let Some(desktop_path) = policy.desktop_trash {
        let desktop = fs.resolve(desktop_path)?;
        if parent == desktop_path && resolved_parent == desktop {
            validate_file_name(tail)?;
            // Desktop trash deliberately lives below the application's protected data dir.
            let denylist: Vec<_> = sensitive
                .iter()
                .filter(|d| !desktop.starts_with(d))
                .cloned()
                .collect();
            return reject_sensitive(fs, path, &denylist);
        }
    }
    for root in policy.catalog_roots {
        let trash = root.join(".katalog-papierkorb");
        if parent == trash && resolved_parent == fs.resolve(&trash)? {
            // A web trash directory must itself remain immediately below its configured root.
            if resolved_parent.parent() != Some(fs.resolve(root)?.as_path()) {
                return Err(invalid());
            }
            let (op, disk_name) = tail.split_once('-').ok_or_else(invalid)?;
            if op.len() != 8 || !op.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(invalid());
            }
            validate_file_name(disk_name)?;
            return reject_sensitive(fs, path, sensitive);
        }
    }
    Err(invalid())
}

/// Validate, migrate and sanitize a private copy. The returned owner keeps it alive.
/// Capture local settings while holding the host's operation gate; keep that gate
/// until activation. This function never replaces or writes the active connection.
pub fn prepare_catalog(
    bytes: &[u8],
    sensitive_dirs: &[PathBuf],
    policy: &TrashPolicy<'_>,
    known_folders: &[String],
    fs: &dyn RestoreFs,
    local_slicers: &[db::models::RegisteredSlicer],
    local_thingiverse: &db::thingiverse::LocalSettings,
) -> CmdResult<Box<dyn TemporaryCatalog>> {
    let mut temporary = fs.temporary_catalog(bytes)?;
    let incoming = temporary.connection();
    validate_connection(incoming, sensitive_dirs, policy, known_folders, fs, None)?;
    // A key in a file somebody handed over must never take effect. Outside the
    // transaction because it vacuums the file; the on/off switch stays. Then the
    // key of this computer goes back in (it is the only one that takes effect).
    db::scrub_secrets(incoming).map_err(|e| e.to_string())?;
    db::thingiverse::keep_local(incoming, local_thingiverse).map_err(|e| e.to_string())?;
    let tx = incoming
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    sanitize_printer_connections(&tx)?;
    sanitize_imported_file_identity(&tx)?;
    // The base-dir preference is not portable, so settings restore leaves it untouched.
    // Allow startup to adopt that location instead of one from the backup.
    tx.execute(
        "DELETE FROM app_settings WHERE key = ?1",
        [crate::import::file_names::SETTING_CATALOG_BASE_DIR],
    )
    .map_err(|e| e.to_string())?;
    // DROP instead of DELETE: a malicious AFTER DELETE trigger would otherwise
    // re-insert the row; DROP TABLE removes the triggers too. Second barrier next
    // to the trigger rejection in validate_catalog_db_bytes. Schema as in
    // migrations.rs.
    tx.execute("DROP TABLE IF EXISTS registered_slicers", [])
        .map_err(|e| e.to_string())?;
    tx.execute(
        "CREATE TABLE registered_slicers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                executable_path TEXT NOT NULL UNIQUE,
                is_auto_detected INTEGER NOT NULL DEFAULT 0
            )",
        [],
    )
    .map_err(|e| e.to_string())?;
    for slicer in local_slicers {
        db::insert_registered_slicer(
            &tx,
            &slicer.name,
            &slicer.executable_path,
            slicer.is_auto_detected,
        )
        .map_err(|e| e.to_string())?;
    }
    // The count must match the saved local slicers exactly.
    let final_count: i64 = tx
        .query_row("SELECT COUNT(*) FROM registered_slicers", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    if final_count as usize != local_slicers.len() {
        return Err(format!(
                "Sanierung von registered_slicers ergab eine unerwartete Zeilenzahl ({final_count} statt {}) - Restore abgebrochen",
                local_slicers.len()
            )
            .into());
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(temporary)
}

#[cfg(test)]
mod tests;
