use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    ThreeMf,
    Stl,
    // Covers both .stp and .step files - the DB value is always "stp" (canonical), regardless of the original extension.
    Stp,
    Obj,
}

impl FileType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileType::ThreeMf => "3mf",
            FileType::Stl => "stl",
            FileType::Stp => "stp",
            FileType::Obj => "obj",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "3mf" => Some(FileType::ThreeMf),
            "stl" => Some(FileType::Stl),
            "stp" => Some(FileType::Stp),
            "obj" => Some(FileType::Obj),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MaterialRecord {
    pub name: String,
    pub display_color: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewFile {
    pub name: String,
    pub path: String,
    pub file_type: FileType,
    pub folder_id: Option<i64>,
    pub origin: String,
    pub cloud_id: Option<String>,
    pub sync_status: String,
    pub file_size_bytes: i64,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub thumbnail_png: Option<Vec<u8>>,
    pub imported_at: String,
    pub file_modified_at: Option<String>,
    pub materials: Vec<MaterialRecord>,
    pub metadata: BTreeMap<String, String>,
    pub tags: Vec<String>,
    pub print_status: String,
    pub last_viewed_at: Option<String>,
    pub creator: Option<String>,
    pub content_hash: Option<String>,
    pub render_snapshot_png: Option<Vec<u8>>,
    pub custom_image_png: Option<Vec<u8>>,
    pub source_url: Option<String>,
    pub queue_position: Option<i64>,
    pub favorite: bool,
    pub plate_count: Option<i64>,
    pub slice_info_json: Option<String>,
}

/// Fields `rescan_file` always overwrites when re-reading (full refresh, no merge).
pub struct ScannedMetadataUpdate {
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub thumbnail_png: Option<Vec<u8>>,
    pub plate_count: Option<i64>,
    pub slice_info_json: Option<String>,
    pub materials: Vec<MaterialRecord>,
    pub metadata: BTreeMap<String, String>,
    pub file_size_bytes: i64,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub id: i64,
    pub name: String,
    pub path: String,
    // Indexed in the DB (idx_files_file_type) for a planned, not yet built STL/3MF
    // filter - hence not removed, although currently never read.
    #[allow(dead_code)]
    pub file_type: FileType,
    pub folder_id: Option<i64>,
    pub origin: String,
    pub sync_status: String,
    // Leftover of the removed cloud sync; column deliberately not dropped.
    #[allow(dead_code)]
    pub cloud_id: Option<String>,
    pub file_size_bytes: i64,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub thumbnail_png: Option<Vec<u8>>,
    pub imported_at: String,
    // Never set on import (always None) - no real mtime tracking implemented, the column already exists in the schema.
    #[allow(dead_code)]
    pub file_modified_at: Option<String>,
    pub materials: Vec<MaterialRecord>,
    pub metadata: BTreeMap<String, String>,
    pub tags: Vec<String>,
    pub print_status: String,
    pub last_viewed_at: Option<String>,
    pub creator: Option<String>,
    pub content_hash: Option<String>,
    pub render_snapshot_png: Option<Vec<u8>>,
    pub custom_image_png: Option<Vec<u8>>,
    pub source_url: Option<String>,
    pub queue_position: Option<i64>,
    pub favorite: bool,
    pub plate_count: Option<i64>,
    pub slice_info_json: Option<String>,
    pub deleted_at: Option<String>,
    pub trash_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FolderRecord {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub path: String,
}

/// Machine-local registry of trusted slicers (see `replace_catalog_db`).
#[derive(Debug, Clone)]
pub struct RegisteredSlicer {
    pub id: i64,
    pub name: String,
    pub executable_path: String,
    pub is_auto_detected: bool,
}

#[derive(Debug, Clone)]
pub struct TagCount {
    pub name: String,
    pub color_hue: i64,
    pub count: i64,
}

/// Kind of a stock entry. For `SPOOL_KIND_RESIN`, `original_weight_g`/
/// `remaining_weight_g` are milliliters; resin never sits in a filament slot.
pub const SPOOL_KIND_FILAMENT: &str = "filament";
pub const SPOOL_KIND_RESIN: &str = "resin";
pub const SPOOL_KINDS: &[&str] = &[SPOOL_KIND_FILAMENT, SPOOL_KIND_RESIN];

#[derive(Debug, Clone)]
pub struct FilamentSpoolRecord {
    pub id: i64,
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub location: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: f64,
    pub remaining_weight_g: f64,
    pub price: Option<f64>,
    pub image_png: Option<Vec<u8>>,
    pub color_hex: Option<String>,
    /// Home location while the spool sits in a slot (otherwise `None`).
    pub home_location: Option<String>,
    pub unit_id: Option<i64>,
    pub slot_index: Option<i64>,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct NewFilamentSpool {
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub location: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: f64,
    pub remaining_weight_g: f64,
    pub price: Option<f64>,
    pub image_png: Option<Vec<u8>>,
    pub color_hex: Option<String>,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct PrinterRecord {
    pub id: i64,
    pub name: String,
    /// "filament" or "resin".
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct MaterialUnitRecord {
    pub id: i64,
    pub printer_id: i64,
    pub name: String,
    pub kind: String,
    pub slot_count: i64,
    pub bambu_ams_index: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct CollectionRecord {
    pub id: i64,
    pub name: String,
    pub model_count: i64,
}

pub struct NewPrintLogEntry {
    pub file_id: i64,
    pub printed_at: String,
    pub note: Option<String>,
    pub photo_png: Option<Vec<u8>>,
}

pub struct PrintLogEntryRecord {
    pub id: i64,
    pub printed_at: String,
    pub note: Option<String>,
    pub photo_png: Option<Vec<u8>>,
}
