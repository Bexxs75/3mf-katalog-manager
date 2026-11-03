use std::collections::BTreeMap;

use crate::threemf::print_info::{
    PrintInfo, Reduced, SettingsState, SliceState, MAX_LAYER_HEIGHT_MM, MAX_NOZZLE_MM, MAX_PLATES, MAX_SECONDS,
    MAX_TEMP_C, MAX_FILAMENT_COLORS, MIN_LAYER_HEIGHT_MM, MIN_NOZZLE_MM, MIN_SECONDS, MIN_TEMP_C,
    PRINT_INFO_READER_VERSION,
};
use crate::threemf::source::sanitize_source_value;

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

/// Where a model was downloaded from, as far as the file itself proves it.
/// All three fields are `None` when the file carries no such proof.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceInfo {
    /// Stable lowercase key (`makerworld`); the UI maps it to a display name.
    pub platform: Option<String>,
    pub model_id: Option<String>,
    pub profile_id: Option<String>,
}

/// One plate row of `file_print_plates`.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintPlateRecord {
    pub plate_index: i64,
    pub plate_name: Option<String>,
    pub print_seconds: Option<i64>,
    /// Time of the first layer only; never a print time.
    pub first_layer_seconds: Option<f64>,
    pub plate_bed_type: Option<String>,
}

/// Database mirror of [`PrintInfo`]: one `file_print_info` row plus its plate rows.
/// Build it with [`PrintInfoRecord::from_print_info`]; the writer validates again, so no
/// path can hand the table a value its CHECK or primary key would reject.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintInfoRecord {
    pub reader_version: u32,
    pub read_at: String,
    pub slice_state: SliceState,
    pub settings_state: SettingsState,
    pub printer_model: Option<String>,
    pub bed_type: Option<String>,
    pub layer_height_mm: Option<f64>,
    pub nozzle_mm: Option<f64>,
    pub nozzle_mixed: bool,
    pub bed_temp_c: Option<f64>,
    pub bed_temp_mixed: bool,
    pub nozzle_temp_c: Option<f64>,
    pub nozzle_temp_mixed: bool,
    pub plates: Vec<PrintPlateRecord>,
    /// Distinct `#rrggbb` colors (stored as one space separated text); empty = none known.
    pub filament_colors: Vec<String>,
}

fn in_range(value: Option<f64>, min: f64, max: f64) -> Option<f64> {
    value.filter(|v| v.is_finite() && (min..=max).contains(v))
}

/// A reduced value as the pair of columns: a mixed value has no single number.
fn split_reduced(value: Reduced<f64>) -> (Option<f64>, bool) {
    match value {
        Reduced::Value(v) => (Some(v), false),
        Reduced::Mixed => (None, true),
        Reduced::Unknown => (None, false),
    }
}

impl PrintInfoRecord {
    pub fn from_print_info(info: &PrintInfo, read_at: String) -> Self {
        let (nozzle_mm, nozzle_mixed) = split_reduced(info.nozzle_mm);
        let (bed_temp_c, bed_temp_mixed) = split_reduced(info.bed_temp_c);
        let (nozzle_temp_c, nozzle_temp_mixed) = split_reduced(info.nozzle_temp_c);
        PrintInfoRecord {
            reader_version: PRINT_INFO_READER_VERSION,
            read_at,
            slice_state: info.slice_state,
            settings_state: info.settings_state,
            printer_model: info.printer_model.clone(),
            bed_type: info.bed_type.clone(),
            layer_height_mm: info.layer_height_mm,
            nozzle_mm,
            nozzle_mixed,
            bed_temp_c,
            bed_temp_mixed,
            nozzle_temp_c,
            nozzle_temp_mixed,
            plates: info
                .plates
                .iter()
                .map(|p| PrintPlateRecord {
                    plate_index: i64::from(p.plate_index),
                    plate_name: p.plate_name.clone(),
                    print_seconds: p.print_seconds.and_then(|s| i64::try_from(s).ok()),
                    first_layer_seconds: p.first_layer_seconds,
                    plate_bed_type: p.plate_bed_type.clone(),
                })
                .collect(),
            filament_colors: info.filament_colors.clone(),
        }
        .validated()
    }

    /// Applies every rule the table and the restore check enforce:
    /// out-of-range numbers become unknown (never 0), texts are cleaned, a mixed value has no
    /// number, temperatures and print time exist only for a sliced file, plate rows follow the
    /// the plate rules (index >= 1, the first plate with an index wins, at most `MAX_PLATES`) and exist only
    /// for the two states that recognised a slicer project. Idempotent.
    pub fn validated(mut self) -> Self {
        self.reader_version = self.reader_version.max(1);
        self.printer_model = self.printer_model.as_deref().and_then(sanitize_source_value);
        self.bed_type = self.bed_type.as_deref().and_then(sanitize_source_value);
        self.layer_height_mm = in_range(self.layer_height_mm, MIN_LAYER_HEIGHT_MM, MAX_LAYER_HEIGHT_MM);
        self.nozzle_mm = in_range(self.nozzle_mm, MIN_NOZZLE_MM, MAX_NOZZLE_MM);
        self.bed_temp_c = in_range(self.bed_temp_c, MIN_TEMP_C, MAX_TEMP_C);
        self.nozzle_temp_c = in_range(self.nozzle_temp_c, MIN_TEMP_C, MAX_TEMP_C);
        if self.slice_state != SliceState::Sliced {
            self.bed_temp_c = None;
            self.bed_temp_mixed = false;
            self.nozzle_temp_c = None;
            self.nozzle_temp_mixed = false;
        }
        if self.nozzle_mixed {
            self.nozzle_mm = None;
        }
        if self.bed_temp_mixed {
            self.bed_temp_c = None;
        }
        if self.nozzle_temp_mixed {
            self.nozzle_temp_c = None;
        }

        if !matches!(self.slice_state, SliceState::Sliced | SliceState::NoSliceData) {
            self.plates.clear();
        }
        let mut seen = std::collections::BTreeSet::new();
        let sliced = self.slice_state == SliceState::Sliced;
        let mut kept = Vec::with_capacity(self.plates.len().min(MAX_PLATES));
        for mut plate in std::mem::take(&mut self.plates) {
            if plate.plate_index < 1 || !seen.insert(plate.plate_index) {
                log::debug!(target: "3mf", "print data: plate index {} dropped", plate.plate_index);
                continue;
            }
            if kept.len() >= MAX_PLATES {
                log::debug!(target: "3mf", "print data: more than {MAX_PLATES} plates, rest dropped");
                break;
            }
            plate.plate_name = plate.plate_name.as_deref().and_then(sanitize_source_value);
            plate.plate_bed_type = plate.plate_bed_type.as_deref().and_then(sanitize_source_value);
            plate.print_seconds = plate
                .print_seconds
                .filter(|s| sliced && (MIN_SECONDS as i64..=MAX_SECONDS as i64).contains(s));
            plate.first_layer_seconds = in_range(plate.first_layer_seconds, MIN_SECONDS, MAX_SECONDS);
            kept.push(plate);
        }
        self.plates = kept;

        // Same canonical form the reader produces: usable `#rrggbb`, distinct, at most 32.
        let mut colors: Vec<String> = Vec::with_capacity(self.filament_colors.len().min(MAX_FILAMENT_COLORS));
        for color in std::mem::take(&mut self.filament_colors) {
            let usable = color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit());
            let color = color.to_ascii_lowercase();
            if usable && !colors.contains(&color) && colors.len() < MAX_FILAMENT_COLORS {
                colors.push(color);
            }
        }
        self.filament_colors = colors;
        self
    }

    /// Rough size of what the record adds to the catalog, for the web upload budget.
    pub fn approx_bytes(&self) -> usize {
        let text = |t: &Option<String>| t.as_ref().map_or(0, String::len);
        256 + text(&self.printer_model)
            + text(&self.bed_type)
            + self.read_at.len()
            + self.plates.iter().map(|p| 64 + text(&p.plate_name) + text(&p.plate_bed_type)).sum::<usize>()
            + self.filament_colors.len() * 8
    }
}

/// What the summary list needs of the print data: just enough to say "unread / state".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintInfoSummary {
    pub reader_version: u32,
    pub slice_state: String,
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
    pub license: Option<String>,
    pub source: SourceInfo,
    pub queue_position: Option<i64>,
    pub favorite: bool,
    pub plate_count: Option<i64>,
    pub slice_info_json: Option<String>,
    /// 3MF only (`None` for every other type); written in the same transaction as the file.
    pub print_info: Option<PrintInfoRecord>,
}

/// Fields `rescan_file` always overwrites when re-reading (full refresh, no merge).
pub struct ScannedMetadataUpdate {
    pub file_modified_at: Option<String>,
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
    /// Overwritten like the other scanned fields: a file that no longer carries
    /// the data clears the columns.
    pub source: SourceInfo,
    /// Replaces the stored print data when `Some`; `None` leaves it untouched (non-3MF files).
    pub print_info: Option<PrintInfoRecord>,
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
    pub license: Option<String>,
    pub queue_position: Option<i64>,
    pub favorite: bool,
    pub plate_count: Option<i64>,
    pub slice_info_json: Option<String>,
    pub deleted_at: Option<String>,
    pub trash_path: Option<String>,
    pub source_platform: Option<String>,
    pub source_model_id: Option<String>,
    pub source_profile_id: Option<String>,
    pub print_info: Option<PrintInfoRecord>,
}

impl FileRecord {
    /// Where the file is right now: the trash copy for a trashed file, otherwise
    /// the catalog path. `trash_path` is only a path while `deleted_at` is set.
    pub fn current_path(&self) -> &str {
        match (&self.deleted_at, &self.trash_path) {
            (Some(_), Some(trash_path)) => trash_path,
            _ => &self.path,
        }
    }
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
    /// Up to four colours of a multi-colour spool; empty means single-colour. When not
    /// empty, `color_hex` is always its first entry (the slicer colour match reads that).
    pub color_hexes: Vec<String>,
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
    /// See `FilamentSpoolRecord::color_hexes`.
    pub color_hexes: Vec<String>,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct PrinterRecord {
    pub id: i64,
    pub name: String,
    /// "filament" or "resin".
    pub kind: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub nozzle_mm: Option<f64>,
    pub bed_x_mm: Option<f64>,
    pub bed_y_mm: Option<f64>,
    pub bed_z_mm: Option<f64>,
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

/// An extra file of a model (picture, PDF, text note). `deleted_at`/`trash_path`
/// are set while it sits in the app trash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionRecord {
    pub id: i64,
    pub file_id: i64,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size_bytes: Option<i64>,
    pub added_at: String,
    pub deleted_at: Option<String>,
    pub trash_path: Option<String>,
}

/// A folder watched for new model files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFolderRecord {
    pub id: i64,
    pub path: String,
    pub added_at: String,
}

/// A source-folder file the user declined; matched by path and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedSourceRecord {
    pub id: i64,
    pub path: String,
    pub size_bytes: i64,
    pub rejected_at: String,
}
