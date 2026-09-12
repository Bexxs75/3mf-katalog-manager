use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    ThreeMf,
    Stl,
}

impl FileType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileType::ThreeMf => "3mf",
            FileType::Stl => "stl",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "3mf" => Some(FileType::ThreeMf),
            "stl" => Some(FileType::Stl),
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
    pub deleted_at: Option<String>,
    pub trash_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FolderRecord {
    pub id: i64,
    pub name: String,
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

#[derive(Debug, Clone)]
pub struct FilamentSpoolRecord {
    pub id: i64,
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub location: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: i64,
    pub remaining_weight_g: i64,
    pub price: Option<f64>,
    pub image_png: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct NewFilamentSpool {
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub location: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: i64,
    pub remaining_weight_g: i64,
    pub price: Option<f64>,
    pub image_png: Option<Vec<u8>>,
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
