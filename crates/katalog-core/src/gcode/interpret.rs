//! Turns the collected `key -> value` pairs into print data. The key tables below hold only
//! keys that real files from the slicers named in the module documentation proved; the text
//! and the binary files use the same names (text `key = value`, binary `key=value`).

use std::collections::BTreeMap;

use crate::threemf::print_info::{PlatePrintInfo, PrintInfo, Reduced, SettingsState, SliceState, MAX_FILAMENT_COLORS};
use crate::threemf::slice_info::{FilamentUsage, PlateFilamentUsage};
use crate::threemf::source::sanitize_source_value;
use crate::threemf::SliceInfo;

use super::{FilamentFacts, GcodeRead, Raw};

/// Print time keys, strongest first. `total estimated time` (Bambu Studio, Orca) is what the
/// slicer shows as the print time; it includes preparation, `model printing time` does not.
const TIME_KEYS: &[&str] = &["total estimated time", "estimated printing time (normal mode)", "model printing time"];
const FIRST_LAYER_TIME_KEY: &str = "estimated first layer printing time (normal mode)";
const LENGTH_KEYS: &[&str] = &["filament used [mm]", "total filament length [mm]"];
const WEIGHT_KEYS: &[&str] = &["filament used [g]", "total filament weight [g]"];
const VOLUME_CM3_KEYS: &[&str] = &["filament used [cm3]"];
/// Bambu Studio labels this total "[cm^3]" but the numbers are cubic millimetres (checked against
/// the size of a 20 mm cube); the unit is therefore converted.
const VOLUME_MM3_KEYS: &[&str] = &["total filament volume [cm^3]"];
const TOTAL_WEIGHT_KEY: &str = "total filament used [g]";
const NOZZLE_TEMP_KEYS: &[&str] = &["nozzle_temperature", "temperature"];
/// Bed temperature key of the Orca family by `curr_bed_type`. Only the two plates real files
/// proved are listed; another plate name leaves the bed temperature unknown.
const BED_TEMP_BY_PLATE: &[(&str, &str)] = &[("Cool Plate", "cool_plate_temp"), ("High Temp Plate", "hot_plate_temp")];
/// PrusaSlicer family (no plate types).
const PRUSA_BED_TEMP_KEY: &str = "bed_temperature";

/// CuraEngine writes this before the user interface replaces it by the estimate.
const CURA_TIME_PLACEHOLDER: f64 = 6666.0;

pub(super) fn interpret(raw: Raw) -> GcodeRead {
    let thumbnail_png = raw.thumbs.iter().max_by_key(|t| u64::from(t.width) * u64::from(t.height)).map(|t| t.png.clone());
    let kv = &raw.kv;

    if !raw.recognized {
        return GcodeRead {
            slicer: None,
            print_info: PrintInfo {
                slice_state: SliceState::NotRecognized,
                settings_state: SettingsState::NotAttempted,
                printer_model: None,
                bed_type: None,
                layer_height_mm: None,
                nozzle_mm: Reduced::Unknown,
                bed_temp_c: Reduced::Unknown,
                nozzle_temp_c: Reduced::Unknown,
                plates: Vec::new(),
                filament_colors: Vec::new(),
            },
            filaments: Vec::new(),
            slice_info: None,
            thumbnail_png,
        };
    }

    let types = text_list(kv, "filament_type");
    let colours = text_list(kv, "filament_colour");
    let filaments = filament_use(kv, &types, &colours);

    // Slots the values below are read from: the used filaments, else the ones the header names.
    let ids = id_list(kv, "filament");
    let slots: Vec<usize> = if !filaments.is_empty() {
        filaments.iter().map(|f| f.slot).collect()
    } else {
        ids.clone()
    };

    let plate_key = get(kv, "curr_bed_type").and_then(|t| BED_TEMP_BY_PLATE.iter().find(|(name, _)| *name == t).map(|(_, key)| *key));
    let bed_temps = match (get(kv, "curr_bed_type"), plate_key) {
        (Some(_), Some(key)) => number_list(kv, key),
        (Some(_), None) => Vec::new(),
        (None, _) => number_list(kv, PRUSA_BED_TEMP_KEY),
    };
    let nozzle_temps = NOZZLE_TEMP_KEYS.iter().map(|k| number_list(kv, k)).find(|l| !l.is_empty()).unwrap_or_default();

    let print_seconds = TIME_KEYS.iter().find_map(|k| get(kv, k).and_then(parse_duration)).or_else(|| cura_seconds(kv));
    let first_layer_seconds = get(kv, FIRST_LAYER_TIME_KEY).and_then(parse_duration).map(|s| s as f64);

    let filament_colors = {
        let from_slots: Vec<usize> = if !slots.is_empty() || colours.len() != 1 { slots.clone() } else { vec![0] };
        from_slots.iter().filter_map(|s| colours.get(*s)).filter_map(|c| normalize_color(c)).collect::<Vec<_>>()
    };

    let print_info = PrintInfo {
        slice_state: SliceState::Sliced,
        settings_state: if raw.config { SettingsState::Read } else { SettingsState::Missing },
        printer_model: get(kv, "printer_model").or_else(|| get(kv, "TARGET_MACHINE.NAME")).and_then(sanitize_source_value),
        bed_type: get(kv, "curr_bed_type").and_then(sanitize_source_value),
        layer_height_mm: get(kv, "layer_height").or_else(|| get(kv, "Layer height")).and_then(parse_number),
        nozzle_mm: reduce(number_list(kv, "nozzle_diameter").into_iter().flatten()),
        bed_temp_c: reduce(pick(&bed_temps, &slots)),
        nozzle_temp_c: reduce(pick(&nozzle_temps, &slots)),
        plates: vec![PlatePrintInfo { plate_index: 1, plate_name: None, print_seconds, first_layer_seconds, plate_bed_type: None }],
        filament_colors: dedupe_colors(filament_colors),
    };

    let slice_info = slice_info(kv, &filaments);
    GcodeRead { slicer: raw.slicer.clone(), print_info, filaments, slice_info, thumbnail_png }
}

fn get<'a>(kv: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    kv.get(key).map(|v| v.trim()).filter(|v| !v.is_empty())
}

fn parse_number(text: &str) -> Option<f64> {
    text.trim().trim_matches('"').parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Items of a list value: `1372.73, 1371.79`, `PLA;PETG`, `0.4,0.4`, `#E4002B;#00AA00`.
fn items(value: &str) -> Vec<&str> {
    value.split([',', ';']).map(|p| p.trim().trim_matches('"').trim()).collect()
}

fn number_list(kv: &BTreeMap<String, String>, key: &str) -> Vec<Option<f64>> {
    get(kv, key).map(|v| items(v).into_iter().map(parse_number).collect()).unwrap_or_default()
}

fn text_list(kv: &BTreeMap<String, String>, key: &str) -> Vec<String> {
    get(kv, key).map(|v| items(v).into_iter().map(str::to_string).collect()).unwrap_or_default()
}

/// One-based filament numbers of the header (`; filament: 1,2`) as zero-based slots.
fn id_list(kv: &BTreeMap<String, String>, key: &str) -> Vec<usize> {
    get(kv, key)
        .map(|v| items(v).into_iter().filter_map(|n| n.parse::<usize>().ok()).filter(|n| (1..=1024).contains(n)).map(|n| n - 1).collect())
        .unwrap_or_default()
}

/// The entries of a per-filament list for the used slots; a one-entry list applies to all.
fn pick(list: &[Option<f64>], slots: &[usize]) -> Vec<f64> {
    if list.len() == 1 || slots.is_empty() {
        return list.iter().flatten().copied().collect();
    }
    slots.iter().filter_map(|s| list.get(*s).copied().flatten()).collect()
}

fn reduce(values: impl IntoIterator<Item = f64>) -> Reduced<f64> {
    let values: Vec<f64> = values.into_iter().filter(|v| v.is_finite()).collect();
    match values.first() {
        None => Reduced::Unknown,
        Some(first) if values.iter().all(|v| (v - first).abs() < 1e-9) => Reduced::Value(*first),
        Some(_) => Reduced::Mixed,
    }
}

/// `1h 9m 37s`, `39m 55s`, `10s`, `2d 3h 1m 2s`. Anything else is not a time.
pub(super) fn parse_duration(text: &str) -> Option<u64> {
    let mut total = 0u64;
    let mut seen = false;
    for part in text.split_whitespace() {
        let (digits, unit) = part.split_at(part.len().checked_sub(1)?);
        let n: u64 = digits.parse().ok()?;
        let factor = match unit {
            "d" => 86_400,
            "h" => 3_600,
            "m" => 60,
            "s" => 1,
            _ => return None,
        };
        total = total.checked_add(n.checked_mul(factor)?)?;
        seen = true;
    }
    (seen && total > 0).then_some(total)
}

/// CuraEngine: `;TIME:<seconds>` once the interface filled it in; as written by the engine alone
/// the value is a placeholder, and the last `;TIME_ELAPSED:` of the file is the estimate the engine
/// itself wrote.
fn cura_seconds(kv: &BTreeMap<String, String>) -> Option<u64> {
    let seconds = |key: &str| get(kv, key).and_then(parse_number).filter(|s| *s > 0.0 && *s < 1e9);
    let real = seconds("TIME").filter(|s| (*s - CURA_TIME_PLACEHOLDER).abs() > 0.5);
    real.or_else(|| seconds("TIME_ELAPSED")).map(|s| s.round() as u64)
}

/// `#RRGGBB` or `#RRGGBBAA` to `#rrggbb`.
fn normalize_color(text: &str) -> Option<String> {
    let hex = text.trim().strip_prefix('#')?;
    if (hex.len() == 6 || hex.len() == 8) && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(format!("#{}", hex[..6].to_ascii_lowercase()))
    } else {
        None
    }
}

fn dedupe_colors(colors: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for color in colors {
        if !out.contains(&color) && out.len() < MAX_FILAMENT_COLORS {
            out.push(color);
        }
    }
    out
}

/// Filaments the slicer reports a positive use for. The lists of the sums run over all filaments
/// (PrusaSlicer, Orca: unused ones as 0) or over the used ones only (Bambu Studio); the
/// `; filament: 1,2` header tells which.
fn filament_use(kv: &BTreeMap<String, String>, types: &[String], colours: &[String]) -> Vec<FilamentFacts> {
    let first_list = |keys: &[&str]| keys.iter().map(|k| number_list(kv, k)).find(|l| !l.is_empty()).unwrap_or_default();
    let lengths = first_list(LENGTH_KEYS);
    let weights = first_list(WEIGHT_KEYS);
    let volumes_cm3 = first_list(VOLUME_CM3_KEYS);
    let volumes_mm3 = first_list(VOLUME_MM3_KEYS);
    let mut cura_length: Vec<Option<f64>> = Vec::new();
    if lengths.is_empty() {
        // `;Filament used: 1.23m` or `1.2m, 0.3m`
        if let Some(v) = get(kv, "Filament used") {
            cura_length = items(v).into_iter().map(|p| p.strip_suffix('m').and_then(parse_number).map(|m| m * 1000.0)).collect();
        }
    }
    let lengths = if lengths.is_empty() { cura_length } else { lengths };

    let count = lengths.len().max(weights.len()).max(volumes_cm3.len()).max(volumes_mm3.len());
    if count == 0 {
        return Vec::new();
    }
    let ids = id_list(kv, "filament");
    let slot_of = |i: usize| if ids.len() == count { ids[i] } else { i };
    let at = |list: &[Option<f64>], i: usize| list.get(i).copied().flatten();

    let mut out = Vec::new();
    for i in 0..count.min(1024) {
        let length_mm = at(&lengths, i);
        let weight_g = at(&weights, i);
        let volume_cm3 = at(&volumes_cm3, i).or_else(|| at(&volumes_mm3, i).map(|mm3| mm3 / 1000.0));
        let positive = |v: Option<f64>| v.is_some_and(|v| v > 0.0);
        if !(positive(length_mm) || positive(weight_g) || positive(volume_cm3)) {
            continue;
        }
        let slot = slot_of(i);
        out.push(FilamentFacts {
            slot,
            filament_type: types.get(slot).and_then(|t| sanitize_source_value(t)),
            color: colours.get(slot).and_then(|c| normalize_color(c)),
            length_mm,
            weight_g,
            volume_cm3,
        });
    }
    out
}

/// Only when every used filament has a weight: the file then holds what "is there enough
/// filament" needs. Length alone (CuraEngine) is not turned into a weight.
fn slice_info(kv: &BTreeMap<String, String>, filaments: &[FilamentFacts]) -> Option<SliceInfo> {
    if filaments.is_empty() || filaments.iter().any(|f| !f.weight_g.is_some_and(|g| g > 0.0)) {
        return None;
    }
    let sum: f64 = filaments.iter().filter_map(|f| f.weight_g).sum();
    let total = get(kv, TOTAL_WEIGHT_KEY).and_then(parse_number).filter(|t| *t > 0.0).unwrap_or(sum);
    Some(SliceInfo {
        total_weight_g: total,
        plates: vec![PlateFilamentUsage {
            plate_index: 1,
            weight_g: total,
            filaments: filaments
                .iter()
                .map(|f| FilamentUsage {
                    filament_type: f.filament_type.clone().unwrap_or_default(),
                    color: f.color.clone(),
                    used_g: f.weight_g.unwrap_or(0.0),
                    used_m: f.length_mm.map_or(0.0, |mm| mm / 1000.0),
                })
                .collect(),
        }],
    })
}
