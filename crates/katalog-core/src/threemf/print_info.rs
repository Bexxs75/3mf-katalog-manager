//! Print data of a 3MF: target printer, bed type, layer height, nozzle, per-plate
//! first-layer time and (only when a file provably holds them) print time and
//! temperatures. Everything here is optional: a missing, broken or odd entry only
//! means "unknown" (never 0, never a guessed default) and never fails an import.
//!
//! Only keys that a real file proved are read. The tables for print time and for
//! temperatures that are NOT proven yet ship empty (see [`PRINT_TIME_SOURCES`],
//! [`BED_TEMP_KEYS`]); the mechanism is complete and tested with synthetic archives.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek};

use serde::Deserialize;
use serde_json::Value;
use zip::result::ZipError;
use zip::ZipArchive;

use super::container::MAX_CONFIG_XML_BYTES;
use super::plates::PlateAssignments;
use super::slice_info::{PlateSliceExtra, SliceDetection};
pub use super::slice_info::SliceState;
use super::slicer_colors::MAX_FILAMENTS;
use super::source::sanitize_source_value;

/// Raise with every new source entry so a re-read is offered again for files read earlier.
/// 2: PrusaSlicer projects (`Metadata/Slic3r_PE.config`) are read as a source.
pub const PRINT_INFO_READER_VERSION: u32 = 2;

/// Upper bound of plate rows per file (protects against prepared archives; real files have 1 to 4).
pub const MAX_PLATES: usize = 1024;

/// Per `Metadata/plate_N.json` (real files: up to 35 KB with 210 objects).
pub const MAX_PLATE_JSON_BYTES: u64 = 4 * 1024 * 1024;
/// All `plate_N.json` of one archive together; further plates are read without these values.
pub const MAX_PLATE_JSON_TOTAL_BYTES: u64 = 16 * 1024 * 1024;

pub const MIN_LAYER_HEIGHT_MM: f64 = 0.01;
pub const MAX_LAYER_HEIGHT_MM: f64 = 5.0;
pub const MIN_NOZZLE_MM: f64 = 0.1;
pub const MAX_NOZZLE_MM: f64 = 5.0;
pub const MIN_TEMP_C: f64 = 1.0;
pub const MAX_TEMP_C: f64 = 500.0;
pub const MIN_SECONDS: f64 = 1.0;
/// Three years.
pub const MAX_SECONDS: f64 = 3.0 * 365.0 * 24.0 * 3600.0;

const SETTINGS_PATH: &str = "Metadata/project_settings.config";

/// Settings of a PrusaSlicer project (text, one `; key = value` per line).
///
/// Source table, entry proven by a real PrusaSlicer 2.9 project saved without slicing
/// (user sample `skoda_top_tether_hook_20260812.3mf`; the repository holds only the
/// synthetic `tests/fixtures/synthetic-prusa-project.3mf`). That package has no
/// `slice_info.config`, no `plate_N.json`, no G-code and no print time. Keys read:
/// `printer_model`, `layer_height`, `nozzle_diameter` (comma separated with several
/// extruders). Temperatures (`temperature`, `bed_temperature`, ...) and `bed_shape` are
/// present in the file but deliberately NOT read: no temperatures without print data, and
/// a bed type is never guessed from the bed shape.
const PRUSA_SETTINGS_PATH: &str = "Metadata/Slic3r_PE.config";

/// `<metadata key=".." value="seconds"/>` keys inside a `<plate>` of `slice_info.config`
/// that carry the print time. EMPTY until a real print file proves one (the key
/// `prediction` is only a hypothesis and must not be added without such a file).
pub const PRINT_TIME_SOURCES: &[&str] = &[];

/// Bed type text (`curr_bed_type`) to the key of its bed temperature list. EMPTY until a
/// real print file proves the mapping (gate T3).
pub const BED_TEMP_KEYS: &[(&str, &str)] = &[];

/// Keys of the nozzle temperature list. The key exists in real project files, but the
/// value is used only behind the gates T1, T2 and T4 (see [`reduce_temperature`]).
pub const NOZZLE_TEMP_KEYS: &[&str] = &["nozzle_temperature"];

/// Result of reading `project_settings.config`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsState {
    Read,
    Missing,
    Unreadable,
    /// Not read on purpose (file not recognised as a slicer project, or deferred
    /// reading). Reserved wording of the stored `settings_state` column.
    NotAttempted,
}

impl SettingsState {
    pub fn as_str(self) -> &'static str {
        match self {
            SettingsState::Read => "read",
            SettingsState::Missing => "missing",
            SettingsState::Unreadable => "unreadable",
            SettingsState::NotAttempted => "not_attempted",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "read" => Some(SettingsState::Read),
            "missing" => Some(SettingsState::Missing),
            "unreadable" => Some(SettingsState::Unreadable),
            "not_attempted" => Some(SettingsState::NotAttempted),
            _ => None,
        }
    }
}

/// A value reduced over several entries: all known entries equal, at least two
/// different known entries, or nothing known.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reduced<T> {
    Value(T),
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlatePrintInfo {
    pub plate_index: u32,
    pub plate_name: Option<String>,
    /// Only ever from [`PRINT_TIME_SOURCES`]; never derived from `first_layer_seconds`.
    pub print_seconds: Option<u64>,
    /// Time of the first layer only (one decimal), never a print time.
    pub first_layer_seconds: Option<f64>,
    /// Raw text of `plate_N.json` (other spelling than `bed_type`, e.g. `supertack_plate`).
    pub plate_bed_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintInfo {
    pub slice_state: SliceState,
    pub settings_state: SettingsState,
    pub printer_model: Option<String>,
    pub bed_type: Option<String>,
    pub layer_height_mm: Option<f64>,
    pub nozzle_mm: Reduced<f64>,
    pub bed_temp_c: Reduced<f64>,
    pub nozzle_temp_c: Reduced<f64>,
    pub plates: Vec<PlatePrintInfo>,
}

impl PrintInfo {
    fn empty(slice_state: SliceState, settings_state: SettingsState) -> Self {
        PrintInfo {
            slice_state,
            settings_state,
            printer_model: None,
            bed_type: None,
            layer_height_mm: None,
            nozzle_mm: Reduced::Unknown,
            bed_temp_c: Reduced::Unknown,
            nozzle_temp_c: Reduced::Unknown,
            plates: Vec::new(),
        }
    }
}

/// The temperature source tables the reader uses; a parameter so tests can mount
/// synthetic entries without a global switch. (The print time keys belong to the
/// `slice_info.config` reading and are a parameter of
/// `read_slice_info_detailed_with`.)
#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    pub bed_temp_keys: &'a [(&'a str, &'a str)],
    pub nozzle_temp_keys: &'a [&'a str],
}

impl Sources<'static> {
    pub const SHIPPED: Sources<'static> = Sources {
        bed_temp_keys: BED_TEMP_KEYS,
        nozzle_temp_keys: NOZZLE_TEMP_KEYS,
    };
}

/// Reads the slice state plus the print data of an archive (the entry point the package
/// reader uses). Returns the slice detection too, so `slice_info.config` is read once.
/// The second element of the result counts the bytes read here (settings and plate
/// JSON), for the package budget; the bytes of `slice_info.config` are in the detection call.
pub fn read_print_info<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    slice: &SliceDetection,
    assignments: &PlateAssignments,
) -> (PrintInfo, u64) {
    read_print_info_with(archive, slice, assignments, &Sources::SHIPPED)
}

pub fn read_print_info_with<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    slice: &SliceDetection,
    assignments: &PlateAssignments,
    sources: &Sources,
) -> (PrintInfo, u64) {
    // A PrusaSlicer project has no `slice_info.config`; its own settings entry proves what it
    // is. A file that has a `slice_info.config` (any state but "missing") is judged by that.
    if slice.state == SliceState::NotRecognized {
        if let Some(result) = read_prusa_project(archive) {
            return result;
        }
    }

    // Only files recognised as slicer projects get the extra reading: for the other
    // two states nothing proves what the file is, and no extra work is done for them.
    if !matches!(slice.state, SliceState::Sliced | SliceState::NoSliceData) {
        return (PrintInfo::empty(slice.state, SettingsState::NotAttempted), 0);
    }

    let mut bytes_read: u64 = 0;
    let mut info = PrintInfo::empty(slice.state, SettingsState::Missing);

    let (settings_text, settings_bytes) = read_entry_counted(archive, SETTINGS_PATH, MAX_CONFIG_XML_BYTES);
    bytes_read += settings_bytes;
    let settings = match settings_text {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) => Some(map),
            _ => {
                info.settings_state = SettingsState::Unreadable;
                None
            }
        },
        Err(EntryFailure::Missing) => None,
        Err(_) => {
            info.settings_state = SettingsState::Unreadable;
            None
        }
    };

    if let Some(settings) = &settings {
        info.settings_state = SettingsState::Read;
        info.printer_model = text_value(settings.get("printer_model"));
        info.bed_type = text_value(settings.get("curr_bed_type"));
        info.layer_height_mm = scalar_number(settings.get("layer_height"), MIN_LAYER_HEIGHT_MM, MAX_LAYER_HEIGHT_MM);
        info.nozzle_mm = reduce(number_list(settings.get("nozzle_diameter"), MIN_NOZZLE_MM, MAX_NOZZLE_MM).into_iter().flatten());
        if slice.state == SliceState::Sliced {
            info.nozzle_temp_c = sources
                .nozzle_temp_keys
                .iter()
                .find(|key| settings.contains_key(**key))
                .map(|key| reduce_temperature(settings, key, slice))
                .unwrap_or(Reduced::Unknown);
            // T3: only a bed type with a proven key is used.
            info.bed_temp_c = info
                .bed_type
                .as_deref()
                .and_then(|bed| sources.bed_temp_keys.iter().find(|(text, _)| *text == bed))
                .map(|(_, key)| reduce_temperature(settings, key, slice))
                .unwrap_or(Reduced::Unknown);
        }
    }

    let (plates, plate_bytes) = read_plates(archive, slice, assignments);
    bytes_read += plate_bytes;
    info.plates = plates;
    (info, bytes_read)
}

/// Reads a PrusaSlicer project (`Slic3r_PE.config`). `None` when the entry does not exist
/// (the caller goes on as before). Otherwise always a result: the file holds no print data
/// (`NoSliceData`), the settings are `Read`, or `Unreadable` when the entry is too large,
/// cut off, not text or has no `; key = value` line at all. No plate rows, no
/// temperatures, no print time.
fn read_prusa_project<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Option<(PrintInfo, u64)> {
    let (text, bytes) = read_entry_counted(archive, PRUSA_SETTINGS_PATH, MAX_CONFIG_XML_BYTES);
    let mut info = PrintInfo::empty(SliceState::NoSliceData, SettingsState::Unreadable);
    let text = match text {
        Ok(text) => text,
        Err(EntryFailure::Missing) => return None,
        Err(_) => return Some((info, bytes)),
    };

    let mut seen_pair = false;
    let (mut model, mut layer, mut nozzle) = (None, None, None);
    for line in text.lines() {
        let Some((key, value)) = line.strip_prefix(';').and_then(|rest| rest.split_once('=')) else { continue };
        let (key, value) = (key.trim(), value.trim());
        if key.is_empty() {
            continue;
        }
        seen_pair = true;
        // The first line of a key wins; other keys are not kept.
        let slot = match key {
            "printer_model" => &mut model,
            "layer_height" => &mut layer,
            "nozzle_diameter" => &mut nozzle,
            _ => continue,
        };
        slot.get_or_insert(value);
    }
    if !seen_pair {
        return Some((info, bytes));
    }

    info.settings_state = SettingsState::Read;
    info.printer_model = model.and_then(sanitize_source_value);
    info.layer_height_mm = layer.and_then(|v| number_of_text(v, MIN_LAYER_HEIGHT_MM, MAX_LAYER_HEIGHT_MM));
    info.nozzle_mm = nozzle
        .map(|v| reduce(v.split(',').take(MAX_FILAMENTS).filter_map(|n| number_of_text(n, MIN_NOZZLE_MM, MAX_NOZZLE_MM))))
        .unwrap_or(Reduced::Unknown);
    Some((info, bytes))
}

/// Temperature reduced over the plates (gates T1, T2, T4 of the specification). Anything
/// that is not provable stays `Unknown`; nothing is guessed.
fn reduce_temperature(settings: &serde_json::Map<String, Value>, key: &str, slice: &SliceDetection) -> Reduced<f64> {
    // T2: the list must have exactly one entry per filament type.
    let filament_count = match settings.get("filament_type") {
        Some(Value::Array(list)) => list.len().min(MAX_FILAMENTS + 1),
        _ => return Reduced::Unknown,
    };
    let Some(Value::Array(list)) = settings.get(key) else { return Reduced::Unknown };
    if filament_count == 0 || list.len() != filament_count || filament_count > MAX_FILAMENTS {
        return Reduced::Unknown;
    }
    let values = number_list(settings.get(key), MIN_TEMP_C, MAX_TEMP_C);

    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut per_plate: Vec<Reduced<f64>> = Vec::new();
    for extra in &slice.plate_extras {
        // First plate with an index wins, index 0 is not a plate (P1, P2).
        if extra.plate_index == 0 || !seen.insert(extra.plate_index) {
            continue;
        }
        // T1: a plate needs the complete list of filament ids it uses.
        if !extra.filament_ids_complete {
            per_plate.push(Reduced::Unknown);
            continue;
        }
        // T4: an id outside 1..=len is unknown.
        let known = extra.used_filament_ids.iter().filter_map(|id| {
            let position = (*id as usize).checked_sub(1)?;
            values.get(position).copied().flatten()
        });
        per_plate.push(reduce(known));
    }
    // One plate without a known value makes the file's value unknown: nothing is guessed.
    if per_plate.is_empty() || per_plate.iter().any(|p| matches!(p, Reduced::Unknown)) {
        return Reduced::Unknown;
    }
    reduce_reduced(per_plate.into_iter())
}

fn reduce_reduced(items: impl Iterator<Item = Reduced<f64>>) -> Reduced<f64> {
    let mut found: Option<f64> = None;
    for item in items {
        match item {
            Reduced::Unknown => {}
            Reduced::Mixed => return Reduced::Mixed,
            Reduced::Value(v) => match found {
                None => found = Some(v),
                Some(f) if f == v => {}
                Some(_) => return Reduced::Mixed,
            },
        }
    }
    found.map_or(Reduced::Unknown, Reduced::Value)
}

/// All known values equal: that value; two different ones: mixed; none: unknown.
fn reduce(values: impl Iterator<Item = f64>) -> Reduced<f64> {
    reduce_reduced(values.map(Reduced::Value))
}

/// Why an entry gave no text.
enum EntryFailure {
    Missing,
    /// Larger than the allowed size (declared, or more bytes than allowed came out).
    TooLarge,
    /// Other read problem, including text that is not valid UTF-8.
    Unreadable,
}

/// Reads one entry as text, at most `max_bytes`, and tells how many bytes were
/// decompressed ALSO when the result is a failure, so the caller can count them
/// against the budget (an entry that is not valid UTF-8 costs as much as a good one).
fn read_entry_counted<R: Read + Seek>(archive: &mut ZipArchive<R>, path: &str, max_bytes: u64) -> (Result<String, EntryFailure>, u64) {
    let file = match archive.by_name(path) {
        Ok(file) => file,
        Err(ZipError::FileNotFound) => return (Err(EntryFailure::Missing), 0),
        Err(_) => return (Err(EntryFailure::Unreadable), 0),
    };
    if file.size() > max_bytes {
        return (Err(EntryFailure::TooLarge), 0);
    }
    let mut data = Vec::new();
    // One byte more than allowed, so a size field that lies is noticed.
    let read = file.take(max_bytes.saturating_add(1)).read_to_end(&mut data);
    let consumed = data.len() as u64;
    if read.is_err() {
        return (Err(EntryFailure::Unreadable), consumed);
    }
    if consumed > max_bytes {
        return (Err(EntryFailure::TooLarge), max_bytes);
    }
    (String::from_utf8(data).map_err(|_| EntryFailure::Unreadable), consumed)
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Text or number to a plausible, rounded number; everything else is unknown.
fn number_of(value: &Value, min: f64, max: f64) -> Option<f64> {
    let parsed = match value {
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        Value::Number(n) => n.as_f64()?,
        _ => return None,
    };
    if !parsed.is_finite() {
        return None;
    }
    let rounded = round3(parsed);
    (min..=max).contains(&rounded).then_some(rounded)
}

/// Text to a plausible, rounded number (same rules as [`number_of`]).
fn number_of_text(text: &str, min: f64, max: f64) -> Option<f64> {
    let parsed = text.trim().parse::<f64>().ok().filter(|v| v.is_finite())?;
    let rounded = round3(parsed);
    (min..=max).contains(&rounded).then_some(rounded)
}

/// A finite number from text or a number, unrounded.
fn raw_number(value: &Value) -> Option<f64> {
    let parsed = match value {
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        Value::Number(n) => n.as_f64()?,
        _ => return None,
    };
    parsed.is_finite().then_some(parsed)
}

fn scalar_number(value: Option<&Value>, min: f64, max: f64) -> Option<f64> {
    number_of(value?, min, max)
}

/// A list (or a single text/number) of numbers; each entry `None` when not plausible.
/// Only the first `MAX_FILAMENTS` entries are looked at.
fn number_list(value: Option<&Value>, min: f64, max: f64) -> Vec<Option<f64>> {
    match value {
        Some(Value::Array(list)) => list.iter().take(MAX_FILAMENTS).map(|v| number_of(v, min, max)).collect(),
        Some(single @ (Value::String(_) | Value::Number(_))) => vec![number_of(single, min, max)],
        _ => Vec::new(),
    }
}

fn text_value(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => sanitize_source_value(text),
        _ => None,
    }
}

fn valid_seconds(seconds: f64) -> Option<f64> {
    (MIN_SECONDS..=MAX_SECONDS).contains(&seconds).then_some(seconds)
}

/// Only the two proven fields; `serde_json` skips every other key (also the object
/// lists) without keeping it.
#[derive(Deserialize)]
struct PlateJson {
    #[serde(default)]
    first_layer_time: Option<Value>,
    #[serde(default)]
    bed_type: Option<Value>,
}

/// `Metadata/plate_<n>.json` file names of the archive, as plate indexes (>= 1).
fn plate_json_indexes<R: Read + Seek>(archive: &ZipArchive<R>) -> BTreeSet<u32> {
    archive
        .file_names()
        .filter_map(|name| {
            let name = name.ok()?;
            let digits = name.strip_prefix("Metadata/plate_")?.strip_suffix(".json")?;
            // No leading zeros: the name read later is built from the number, so it must match.
            if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            digits.parse::<u32>().ok().filter(|n| *n >= 1)
        })
        .collect()
}

/// Plate rows: one per valid plate index of the union of `slice_info.config`, the
/// `<plate>` blocks of `model_settings.config` and the `plate_<n>.json` entries (P1 to P5).
fn read_plates<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    slice: &SliceDetection,
    assignments: &PlateAssignments,
) -> (Vec<PlatePrintInfo>, u64) {
    let json_indexes = plate_json_indexes(archive);

    // First plate in file order wins on a repeated index (P2); index 0 gets no row (P1).
    let mut extras: BTreeMap<u32, &PlateSliceExtra> = BTreeMap::new();
    for extra in &slice.plate_extras {
        if extra.plate_index >= 1 {
            extras.entry(extra.plate_index).or_insert(extra);
        }
    }

    let mut indexes: BTreeSet<u32> = BTreeSet::new();
    indexes.extend(extras.keys().copied());
    indexes.extend(assignments.plates.keys().copied().filter(|n| *n >= 1));
    indexes.extend(json_indexes.iter().copied());

    if indexes.len() > MAX_PLATES {
        log::debug!(target: "3mf", "{} plates found, keeping the first {MAX_PLATES}", indexes.len());
    }

    let mut bytes_read: u64 = 0;
    let mut json_total: u64 = 0;
    let mut plates = Vec::new();
    for index in indexes.into_iter().take(MAX_PLATES) {
        let mut first_layer_seconds = None;
        let mut plate_bed_type = None;
        let remaining = MAX_PLATE_JSON_TOTAL_BYTES.saturating_sub(json_total);
        if json_indexes.contains(&index) && remaining > 0 {
            let path = format!("Metadata/plate_{index}.json");
            // Each entry is limited by what is left of the total, not only by its own limit,
            // and what was decompressed counts even when the entry turns out unusable.
            let (text, consumed) = read_entry_counted(archive, &path, MAX_PLATE_JSON_BYTES.min(remaining));
            json_total += consumed;
            bytes_read += consumed;
            if let Ok(text) = text {
                // A derived struct would also accept a JSON array, so insist on an object.
                let parsed = text.trim_start().starts_with('{').then(|| serde_json::from_str::<PlateJson>(&text)).and_then(Result::ok);
                if let Some(parsed) = parsed {
                    first_layer_seconds = parsed
                        .first_layer_time
                        .as_ref()
                        .and_then(raw_number)
                        .and_then(valid_seconds)
                        .map(|s| (s * 10.0).round() / 10.0);
                    plate_bed_type = text_value(parsed.bed_type.as_ref());
                }
            }
        }
        plates.push(PlatePrintInfo {
            plate_index: index,
            plate_name: assignments.plates.get(&index).and_then(|n| n.as_deref()).and_then(sanitize_source_value),
            print_seconds: extras
                .get(&index)
                .and_then(|e| e.print_seconds)
                .and_then(valid_seconds)
                .map(|s| s.round() as u64),
            first_layer_seconds,
            plate_bed_type,
        });
    }
    (plates, bytes_read)
}

#[cfg(test)]
mod tests;
