use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    ThreeMf,
    Stl,
    // Deckt sowohl .stp- als auch .step-Dateien ab - der DB-Wert ist
    // unabhaengig von der urspruenglichen Endung immer "stp" (kanonisch).
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

/// Felder, die `rescan_file` (commands.rs) nach dem erneuten Einlesen einer
/// bereits katalogisierten Datei unbedingt ueberschreibt - "neu einlesen"
/// ist ein voller Refresh, kein Merge mit dem alten Zustand.
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
    // In DB indiziert (idx_files_file_type) fuer eine geplante, noch nicht
    // gebaute STL/3MF-Filterung - deshalb nicht entfernt, obwohl aktuell
    // nirgends gelesen.
    #[allow(dead_code)]
    pub file_type: FileType,
    pub folder_id: Option<i64>,
    pub origin: String,
    pub sync_status: String,
    // Ueberbleibsel der entfernten Cloud-Synchronisation (siehe
    // project_3mf_katalog_manager_major_features_20260912-Memory), Spalte
    // bewusst nicht per Migration gedroppt.
    #[allow(dead_code)]
    pub cloud_id: Option<String>,
    pub file_size_bytes: i64,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub thumbnail_png: Option<Vec<u8>>,
    pub imported_at: String,
    // Wird beim Import nie gesetzt (immer None) - keine echte mtime-Erfassung
    // implementiert, Spalte existiert bereits im Schema.
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

/// Maschinenlokale Registry vertrauenswuerdiger Slicer-Executables (M-06,
/// Task 11) - siehe `registered_slicers`-Migration in `migrations.rs` und
/// `replace_catalog_db` in `commands.rs`.
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

#[derive(Debug, Clone)]
pub struct CreatorCount {
    pub name: String,
    pub count: i64,
}

/// Art eines Lager-Eintrags (v0.13.1). Bei `SPOOL_KIND_RESIN` bedeuten
/// `original_weight_g`/`remaining_weight_g` Milliliter; Resin steckt nie in
/// einem Fach.
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
    /// Stammplatz, solange die Spule in einem Fach steckt (sonst `None`).
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
pub struct SavedFilterRecord {
    pub id: i64,
    pub name: String,
    pub folder_id: Option<i64>,
    pub tag: Option<String>,
    pub creator: Option<String>,
    pub query: Option<String>,
    pub sort: String,
    // Nach dem Laden im Rust-Code nie gelesen, aber die Spalte selbst treibt
    // "ORDER BY created_at" in list_saved_filters() - funktional nicht tot.
    #[allow(dead_code)]
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NewSavedFilter {
    pub name: String,
    pub folder_id: Option<i64>,
    pub tag: Option<String>,
    pub creator: Option<String>,
    pub query: Option<String>,
    pub sort: String,
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
