use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use super::*;
use crate::threemf::container::MAX_CONFIG_XML_BYTES;
use crate::threemf::plates::read_plate_metadata;
use crate::threemf::slice_info::{parse_slice_info, read_slice_info_detailed_with};

const CREALITY: &[u8] = include_bytes!("../../../tests/fixtures/creality-3plates.3mf");
const BAMBU_SYNTHETIC: &[u8] = include_bytes!("../../../tests/fixtures/synthetic-bambu-project.3mf");

type Entries = Vec<(String, Vec<u8>)>;

fn build(entries: &Entries) -> ZipArchive<Cursor<Vec<u8>>> {
    let mut buf = Vec::new();
    {
        let mut writer = ZipWriter::new(Cursor::new(&mut buf));
        for (name, content) in entries {
            writer.start_file(name.as_str(), SimpleFileOptions::default()).unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
    }
    ZipArchive::new(Cursor::new(buf)).unwrap()
}

fn entries_of(bytes: &[u8]) -> Entries {
    let mut archive = ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut file = archive.by_index(i).unwrap();
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut data).unwrap();
            (file.name().unwrap().to_string(), data)
        })
        .collect()
}

fn set(entries: &mut Entries, name: &str, content: &[u8]) {
    match entries.iter_mut().find(|(n, _)| n == name) {
        Some(entry) => entry.1 = content.to_vec(),
        None => entries.push((name.to_string(), content.to_vec())),
    }
}

fn remove(entries: &mut Entries, name: &str) {
    entries.retain(|(n, _)| n != name);
}

/// Reads the archive the way the package reader does, with explicit source tables.
fn read_with(archive: &mut ZipArchive<Cursor<Vec<u8>>>, time_keys: &[&str], sources: &Sources) -> (SliceDetection, PrintInfo) {
    let (_, _, assignments) = read_plate_metadata(archive);
    let (slice, _) = read_slice_info_detailed_with(archive, time_keys);
    let (info, _) = read_print_info_with(archive, &slice, &assignments, sources);
    (slice, info)
}

fn read(archive: &mut ZipArchive<Cursor<Vec<u8>>>) -> PrintInfo {
    read_with(archive, PRINT_TIME_SOURCES, &Sources::SHIPPED).1
}

fn settings_json(entries: &mut Entries, json: &str) {
    set(entries, "Metadata/project_settings.config", json.as_bytes());
}

const HEADER_ONLY: &str = r#"<config><header><header_item key="a" value="b"/></header></config>"#;

fn slice_plate(index: &str, filament_ids: &[u32], extra: &str) -> String {
    let filaments: String = filament_ids
        .iter()
        .map(|id| format!(r##"<filament id="{id}" type="PLA" color="#FFFFFF" used_m="1" used_g="2"/>"##))
        .collect();
    format!(r#"<plate><metadata key="index" value="{index}"/>{extra}{filaments}</plate>"#)
}

fn slice_xml(plates: &[String]) -> String {
    format!("<config>{}</config>", plates.concat())
}

/// A small slicer project: only what a test needs.
fn base_entries() -> Entries {
    let mut entries = Entries::new();
    set(&mut entries, "Metadata/slice_info.config", HEADER_ONLY.as_bytes());
    settings_json(&mut entries, "{}");
    entries
}

// Fixtures

#[test]
fn creality_project_has_no_print_data() {
    let mut archive = ZipArchive::new(Cursor::new(CREALITY.to_vec())).unwrap();
    let info = read(&mut archive);
    assert_eq!(info.slice_state, SliceState::NoSliceData);
    assert_eq!(info.settings_state, SettingsState::Read);
    assert_eq!(info.printer_model.as_deref(), Some("Creality K1C"));
    assert_eq!(info.bed_type.as_deref(), Some("High Temp Plate"));
    assert_eq!(info.layer_height_mm, Some(0.2));
    assert_eq!(info.nozzle_mm, Reduced::Value(0.4));
    assert_eq!(info.bed_temp_c, Reduced::Unknown);
    assert_eq!(info.nozzle_temp_c, Reduced::Unknown, "not sliced: no temperature");
    // The plate rows come from model_settings.config; none of them holds a time or a print time.
    assert_eq!(info.plates.iter().map(|p| p.plate_index).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert!(info.plates.iter().all(|p| p.print_seconds.is_none() && p.first_layer_seconds.is_none() && p.plate_bed_type.is_none()));
}

#[test]
fn synthetic_bambu_style_project_has_settings_and_first_layer_times_but_no_print_data() {
    // SYNTHETIC: a package built by tests/fixtures/make_synthetic_bambu_project.py, all values invented.
    let mut archive = ZipArchive::new(Cursor::new(BAMBU_SYNTHETIC.to_vec())).unwrap();
    let info = read(&mut archive);
    assert_eq!(info.slice_state, SliceState::NoSliceData);
    assert_eq!(info.settings_state, SettingsState::Read);
    assert_eq!(info.printer_model.as_deref(), Some("Bambu Lab X1 Carbon"));
    assert_eq!(info.bed_type.as_deref(), Some("Supertack Plate"));
    assert_eq!(info.layer_height_mm, Some(0.2));
    assert_eq!(info.nozzle_mm, Reduced::Value(0.4));
    assert_eq!(info.bed_temp_c, Reduced::Unknown);
    assert_eq!(info.nozzle_temp_c, Reduced::Unknown);

    assert_eq!(info.plates.len(), 4);
    let times: Vec<_> = info.plates.iter().map(|p| p.first_layer_seconds).collect();
    assert_eq!(times, vec![Some(123.5), Some(88.0), None, Some(251.0)], "rounded to one decimal; plate 3 has no JSON");
    let beds: Vec<_> = info.plates.iter().map(|p| p.plate_bed_type.as_deref()).collect();
    assert_eq!(beds, vec![Some("supertack_plate"), Some("textured_plate"), None, Some("supertack_plate")]);
    assert!(info.plates.iter().all(|p| p.print_seconds.is_none()), "the first layer time is never a print time");
}

// Slice states

fn state_of(entries: &Entries) -> SliceState {
    let mut archive = build(entries);
    read_slice_info_detailed_with(&mut archive, &[]).0.state
}

#[test]
fn the_four_slice_states_are_told_apart() {
    let mut sliced = base_entries();
    set(&mut sliced, "Metadata/slice_info.config", slice_xml(&[slice_plate("1", &[1], "")]).as_bytes());
    assert_eq!(state_of(&sliced), SliceState::Sliced);

    assert_eq!(state_of(&base_entries()), SliceState::NoSliceData);

    let mut missing = base_entries();
    remove(&mut missing, "Metadata/slice_info.config");
    assert_eq!(state_of(&missing), SliceState::NotRecognized);

    let mut too_big = base_entries();
    set(&mut too_big, "Metadata/slice_info.config", &vec![b'A'; MAX_CONFIG_XML_BYTES as usize + 1]);
    assert_eq!(state_of(&too_big), SliceState::Unreadable);

    let mut broken = base_entries();
    set(&mut broken, "Metadata/slice_info.config", b"<config><plate><metadata key=\"index\" value=\"1\"></config>");
    assert_eq!(state_of(&broken), SliceState::Unreadable);

    // Cut off, empty or without any element: incomplete, not "no data".
    for incomplete in ["", "   \n", "<config><plate>", "<config><header>", "<config><plate><metadata key=\"index\" value=\"1\"/>"] {
        let mut entries = base_entries();
        set(&mut entries, "Metadata/slice_info.config", incomplete.as_bytes());
        assert_eq!(state_of(&entries), SliceState::Unreadable, "{incomplete:?}");
    }
    // A self-closing plate is a plate for the state.
    let mut empty_plate = base_entries();
    set(&mut empty_plate, "Metadata/slice_info.config", b"<config><plate/></config>");
    assert_eq!(state_of(&empty_plate), SliceState::Sliced);
    // Complete and well-formed without a plate stays "no data", also with other elements.
    let mut other = base_entries();
    set(&mut other, "Metadata/slice_info.config", b"<?xml version=\"1.0\"?><config><header><header_item key=\"a\" value=\"b\"/></header></config>");
    assert_eq!(state_of(&other), SliceState::NoSliceData);

    let mut not_utf8 = base_entries();
    set(&mut not_utf8, "Metadata/slice_info.config", &[0xff, 0xfe, 0xfd]);
    assert_eq!(state_of(&not_utf8), SliceState::Unreadable);
}

#[test]
fn parse_slice_info_still_answers_none_for_all_four_cases() {
    // The old function cannot tell the cases apart; its answers must not change.
    let mut sliced = base_entries();
    set(&mut sliced, "Metadata/slice_info.config", slice_xml(&[slice_plate("1", &[1], "")]).as_bytes());
    assert!(parse_slice_info(&mut build(&sliced)).0.is_some());
    assert!(parse_slice_info(&mut build(&base_entries())).0.is_none());
    let mut missing = base_entries();
    remove(&mut missing, "Metadata/slice_info.config");
    assert!(parse_slice_info(&mut build(&missing)).0.is_none());
    let mut too_big = base_entries();
    set(&mut too_big, "Metadata/slice_info.config", &vec![b'A'; MAX_CONFIG_XML_BYTES as usize + 1]);
    assert!(parse_slice_info(&mut build(&too_big)).0.is_none());
    // The old function stays lenient: a cut-off document still yields the plates read before the cut,
    // and a self-closing `<plate/>` is no plate. Only the detailed reading is strict.
    let mut cut = base_entries();
    set(&mut cut, "Metadata/slice_info.config", format!("<config>{}<config>", slice_plate("1", &[1], "")).as_bytes());
    assert!(parse_slice_info(&mut build(&cut)).0.is_some(), "lenient scan keeps the plate");
    let (detail, _) = read_slice_info_detailed_with(&mut build(&cut), &[]);
    assert_eq!(detail.state, SliceState::Unreadable);
    assert!(detail.info.is_some(), "the stored slice info is the same as before");
    let mut empty_plate = base_entries();
    set(&mut empty_plate, "Metadata/slice_info.config", b"<config><plate/></config>");
    assert!(parse_slice_info(&mut build(&empty_plate)).0.is_none());
    assert!(read_slice_info_detailed_with(&mut build(&empty_plate), &[]).0.info.is_none());
}

#[test]
fn files_other_than_slicer_projects_get_no_settings_reading() {
    let mut entries = base_entries();
    settings_json(&mut entries, r#"{"printer_model":"X"}"#);
    remove(&mut entries, "Metadata/slice_info.config");
    let info = read(&mut build(&entries));
    assert_eq!(info.slice_state, SliceState::NotRecognized);
    assert_eq!(info.settings_state, SettingsState::NotAttempted);
    assert_eq!(info.printer_model, None);
    assert!(info.plates.is_empty());
}

#[test]
fn a_gcode_default_in_the_content_types_does_not_make_a_file_sliced() {
    let mut entries = base_entries();
    set(
        &mut entries,
        "[Content_Types].xml",
        br#"<Types><Default Extension="gcode" ContentType="text/x.gcode"/></Types>"#,
    );
    assert_eq!(read(&mut build(&entries)).slice_state, SliceState::NoSliceData);
}

// Settings

#[test]
fn missing_empty_and_broken_settings_leave_every_value_unknown() {
    let mut entries = base_entries();
    remove(&mut entries, "Metadata/project_settings.config");
    let info = read(&mut build(&entries));
    assert_eq!(info.settings_state, SettingsState::Missing);
    assert_eq!(info.slice_state, SliceState::NoSliceData);

    settings_json(&mut entries, "{}");
    let info = read(&mut build(&entries));
    assert_eq!(info.settings_state, SettingsState::Read);
    assert_eq!(info.slice_state, SliceState::NoSliceData);
    assert_eq!((info.printer_model, info.bed_type, info.layer_height_mm), (None, None, None));
    assert_eq!((info.nozzle_mm, info.bed_temp_c, info.nozzle_temp_c), (Reduced::Unknown, Reduced::Unknown, Reduced::Unknown));

    for broken in ["not json", "{\"printer_model\": \"X\"", "[]", "\"text\"", ""] {
        settings_json(&mut entries, broken);
        let info = read(&mut build(&entries));
        assert_eq!(info.settings_state, SettingsState::Unreadable, "{broken:?}");
        assert_eq!(info.printer_model, None);
    }
    set(&mut entries, "Metadata/project_settings.config", &[0xff, 0xfe, b'{', b'}']);
    assert_eq!(read(&mut build(&entries)).settings_state, SettingsState::Unreadable, "invalid UTF-8");
}

#[test]
fn an_oversized_settings_entry_is_not_read() {
    let mut entries = base_entries();
    set(&mut entries, "Metadata/project_settings.config", &vec![b'A'; MAX_CONFIG_XML_BYTES as usize + 1]);
    let info = read(&mut build(&entries));
    assert_eq!(info.settings_state, SettingsState::Unreadable);
    assert_eq!(info.printer_model, None);
    assert_eq!(info.slice_state, SliceState::NoSliceData);
}

#[test]
fn wrong_types_and_implausible_numbers_become_unknown_never_zero() {
    for bad in [
        r#""abc""#, r#""0""#, r#""-1""#, r#""1e999""#, r#""NaN""#, r#""inf""#, r#""""#, "null", "[]", "{}", "true", "0", "-2", "0.0004", "1e999",
    ] {
        let mut entries = base_entries();
        settings_json(&mut entries, &format!(r#"{{"layer_height":{bad},"nozzle_diameter":{bad}}}"#));
        let info = read(&mut build(&entries));
        assert_eq!(info.layer_height_mm, None, "layer_height {bad}");
        assert_eq!(info.nozzle_mm, Reduced::Unknown, "nozzle_diameter {bad}");
    }
}

#[test]
fn float_noise_is_rounded_to_three_places() {
    let mut entries = base_entries();
    settings_json(&mut entries, r#"{"layer_height":"0.2","nozzle_diameter":0.4000000059604645}"#);
    let info = read(&mut build(&entries));
    assert_eq!(info.layer_height_mm, Some(0.2));
    assert_eq!(info.nozzle_mm, Reduced::Value(0.4));
    settings_json(&mut entries, r#"{"nozzle_diameter":"0.4000000059604645"}"#);
    assert_eq!(read(&mut build(&entries)).nozzle_mm, Reduced::Value(0.4));
    settings_json(&mut entries, r#"{"layer_height":0.0004}"#);
    assert_eq!(read(&mut build(&entries)).layer_height_mm, None, "rounds to 0.0: implausible, not zero");
}

#[test]
fn arrays_are_reduced_and_bounded() {
    let mut entries = base_entries();
    settings_json(&mut entries, r#"{"nozzle_diameter":["0.4","0.4"]}"#);
    assert_eq!(read(&mut build(&entries)).nozzle_mm, Reduced::Value(0.4));
    settings_json(&mut entries, r#"{"nozzle_diameter":["0.4","0.6"]}"#);
    assert_eq!(read(&mut build(&entries)).nozzle_mm, Reduced::Mixed);
    settings_json(&mut entries, r#"{"nozzle_diameter":["0.4","abc","0"]}"#);
    assert_eq!(read(&mut build(&entries)).nozzle_mm, Reduced::Value(0.4), "unknown entries do not take part");

    // Only the first MAX_FILAMENTS entries count: the 257th value must not turn the result into "mixed".
    let mut list = vec!["0.4".to_string(); MAX_FILAMENTS];
    list.extend(std::iter::repeat_n("0.6".to_string(), 10_000));
    settings_json(&mut entries, &format!(r#"{{"nozzle_diameter":{}}}"#, serde_json::to_string(&list).unwrap()));
    assert_eq!(read(&mut build(&entries)).nozzle_mm, Reduced::Value(0.4));
}

#[test]
fn texts_are_cleaned_and_capped() {
    let mut entries = base_entries();
    let long = "x".repeat(10_000);
    settings_json(
        &mut entries,
        &format!(r#"{{"printer_model":"{long}","curr_bed_type":"Bed\u0007Type​"}}"#),
    );
    set(
        &mut entries,
        "Metadata/model_settings.config",
        format!(r#"<config><plate><metadata key="plater_id" value="1"/><metadata key="plater_name" value="{long}"/></plate></config>"#).as_bytes(),
    );
    let info = read(&mut build(&entries));
    assert_eq!(info.printer_model.as_deref().map(|s| s.chars().count()), Some(128));
    assert_eq!(info.bed_type.as_deref(), Some("BedType"));
    assert_eq!(info.plates[0].plate_name.as_deref().map(|s| s.chars().count()), Some(128));
    for text in [info.printer_model.unwrap(), info.bed_type.unwrap(), info.plates[0].plate_name.clone().unwrap()] {
        crate::threemf::source::validate_source_value(&text).unwrap();
    }
}

#[test]
fn a_truncated_or_mutated_settings_file_never_panics() {
    let original = entries_of(BAMBU_SYNTHETIC);
    let settings = original.iter().find(|(n, _)| n == "Metadata/project_settings.config").unwrap().1.clone();
    for cut in (0..settings.len()).step_by(37) {
        let mut entries = original.clone();
        set(&mut entries, "Metadata/project_settings.config", &settings[..cut]);
        let info = read(&mut build(&entries));
        assert!(matches!(info.settings_state, SettingsState::Read | SettingsState::Unreadable));
    }
    for position in (0..settings.len()).step_by(53) {
        let mut mutated = settings.clone();
        mutated[position] = b'\xc3';
        let mut entries = original.clone();
        set(&mut entries, "Metadata/project_settings.config", &mutated);
        let _ = read(&mut build(&entries));
    }
}

// Temperatures: SYNTHETIC tests, structure assumed, not taken from a real file; they prove the mechanism only.

const BED_SOURCES: Sources<'static> = Sources {
    bed_temp_keys: &[("Synthetic Bed", "synthetic_bed_temp")],
    nozzle_temp_keys: NOZZLE_TEMP_KEYS,
};

fn temperature_entries(plates: &[String], settings: &str) -> Entries {
    let mut entries = base_entries();
    set(&mut entries, "Metadata/slice_info.config", slice_xml(plates).as_bytes());
    settings_json(&mut entries, settings);
    entries
}

fn nozzle_of(entries: &Entries) -> Reduced<f64> {
    read(&mut build(entries)).nozzle_temp_c
}

#[test]
fn synthetic_creality_style_sliced_file_reads_the_nozzle_temperature() {
    // The real Creality package with a plate that uses filament 1 added.
    let mut entries = entries_of(CREALITY);
    set(&mut entries, "Metadata/slice_info.config", slice_xml(&[slice_plate("1", &[1], "")]).as_bytes());
    let info = read(&mut build(&entries));
    assert_eq!(info.slice_state, SliceState::Sliced);
    assert_eq!(info.printer_model.as_deref(), Some("Creality K1C"));
    assert_eq!(info.layer_height_mm, Some(0.2));
    assert_eq!(info.nozzle_temp_c, Reduced::Value(220.0));
    assert_eq!(info.bed_temp_c, Reduced::Unknown, "the bed type mapping is not proven: shipped table is empty");
}

#[test]
fn synthetic_temperatures_are_reduced_over_the_used_filaments() {
    let settings = r#"{"filament_type":["PLA","PLA","PLA"],"nozzle_temperature":["200","210","0"]}"#;
    let one = |ids: &[u32]| nozzle_of(&temperature_entries(&[slice_plate("1", ids, "")], settings));
    assert_eq!(one(&[1]), Reduced::Value(200.0));
    assert_eq!(one(&[1, 1]), Reduced::Value(200.0), "all equal");
    assert_eq!(one(&[1, 2]), Reduced::Mixed, "two different values");
    assert_eq!(one(&[2, 3]), Reduced::Value(210.0), "a 0 entry does not count");
    assert_eq!(one(&[3]), Reduced::Unknown, "only a 0 entry: unknown, not 0");
    // Over the plates: equal -> value, different -> mixed.
    let two = |a: &[u32], b: &[u32]| nozzle_of(&temperature_entries(&[slice_plate("1", a, ""), slice_plate("2", b, "")], settings));
    assert_eq!(two(&[1], &[1]), Reduced::Value(200.0));
    assert_eq!(two(&[1], &[2]), Reduced::Mixed);
    assert_eq!(two(&[1], &[3]), Reduced::Unknown, "one plate without a known value makes the file unknown");
    assert_eq!(two(&[1], &[]), Reduced::Unknown, "a plate without filament ids too");
    // A filament without a usable id: the list is incomplete, the plate stays unknown.
    let bad_id = |id: &str| {
        let plate = format!(r#"<plate><metadata key="index" value="1"/><filament id="{id}" type="PLA"/><filament id="1" type="PLA"/></plate>"#);
        nozzle_of(&temperature_entries(&[plate], settings))
    };
    assert_eq!(bad_id("1"), Reduced::Value(200.0));
    assert_eq!(bad_id("x"), Reduced::Unknown);
    assert_eq!(bad_id(""), Reduced::Unknown);
}

#[test]
fn synthetic_gates_t1_to_t4_each_keep_the_temperature_unknown() {
    let good = r#"{"filament_type":["PLA","PLA","PLA"],"nozzle_temperature":["200","210","220"]}"#;
    let sliced = |ids: &[u32], settings: &str| nozzle_of(&temperature_entries(&[slice_plate("1", ids, "")], settings));
    assert_eq!(sliced(&[2], good), Reduced::Value(210.0), "all gates hold");

    // T1: not sliced, or a plate without filament ids.
    let mut not_sliced = temperature_entries(&[], good);
    set(&mut not_sliced, "Metadata/slice_info.config", HEADER_ONLY.as_bytes());
    assert_eq!(nozzle_of(&not_sliced), Reduced::Unknown);
    assert_eq!(sliced(&[], good), Reduced::Unknown);
    // T2: list length differs from the filament type list (the shape of a real project: 17 against 9).
    let long_list = format!(
        r#"{{"filament_type":{},"nozzle_temperature":{}}}"#,
        serde_json::to_string(&vec!["PLA"; 9]).unwrap(),
        serde_json::to_string(&(0..17).map(|i| (200 + i).to_string()).collect::<Vec<_>>()).unwrap()
    );
    assert_eq!(sliced(&[1], &long_list), Reduced::Unknown);
    assert_eq!(sliced(&[1], r#"{"nozzle_temperature":["200"]}"#), Reduced::Unknown, "no filament type list");
    // T4: id outside 1..=len.
    assert_eq!(sliced(&[0], good), Reduced::Unknown);
    assert_eq!(sliced(&[4], good), Reduced::Unknown);
    assert_eq!(sliced(&[4, 2], good), Reduced::Value(210.0), "only the id in range counts");
}

#[test]
fn synthetic_bed_temperature_needs_a_proven_key_for_the_bed_type_t3() {
    let settings = r#"{"curr_bed_type":"Synthetic Bed","filament_type":["PLA","PLA"],"synthetic_bed_temp":["55","60"],"hot_plate_temp":["70","70"]}"#;
    let entries = temperature_entries(&[slice_plate("1", &[1], ""), slice_plate("2", &[1], "")], settings);
    let (_, shipped) = read_with(&mut build(&entries), PRINT_TIME_SOURCES, &Sources::SHIPPED);
    assert_eq!(shipped.bed_temp_c, Reduced::Unknown, "shipped table is empty");
    let (_, mounted) = read_with(&mut build(&entries), PRINT_TIME_SOURCES, &BED_SOURCES);
    assert_eq!(mounted.bed_temp_c, Reduced::Value(55.0));
    // A different bed type text has no entry: unknown even though a key exists in the file.
    let other = temperature_entries(&[slice_plate("1", &[1], "")], &settings.replace("Synthetic Bed", "Other Bed"));
    assert_eq!(read_with(&mut build(&other), PRINT_TIME_SOURCES, &BED_SOURCES).1.bed_temp_c, Reduced::Unknown);
    // Both filaments used with different values: mixed.
    let mixed = temperature_entries(&[slice_plate("1", &[1, 2], "")], settings);
    assert_eq!(read_with(&mut build(&mixed), PRINT_TIME_SOURCES, &BED_SOURCES).1.bed_temp_c, Reduced::Mixed);
}

// Print time: SYNTHETIC, structure assumed, not taken from a real file; proves the mechanism only.

const TIME_KEY: &str = "synthetic_print_time";

fn print_seconds_of(value: &str, keys: &[&str]) -> Option<u64> {
    let extra = format!(r#"<metadata key="{TIME_KEY}" value="{value}"/>"#);
    let mut entries = base_entries();
    set(&mut entries, "Metadata/slice_info.config", slice_xml(&[slice_plate("1", &[1], &extra)]).as_bytes());
    let (_, info) = read_with(&mut build(&entries), keys, &Sources::SHIPPED);
    info.plates[0].print_seconds
}

#[test]
fn synthetic_print_time_is_read_from_a_mounted_source_and_validated() {
    assert_eq!(print_seconds_of("5400", &[TIME_KEY]), Some(5400));
    assert_eq!(print_seconds_of("3600.4", &[TIME_KEY]), Some(3600));
    for bad in ["0", "-5", "abc", "", "NaN", "1e30", "0.2"] {
        assert_eq!(print_seconds_of(bad, &[TIME_KEY]), None, "{bad:?}");
    }
}

#[test]
fn the_shipped_table_reads_no_print_time_not_even_from_the_hypothesis_key() {
    assert!(PRINT_TIME_SOURCES.is_empty() && BED_TEMP_KEYS.is_empty());
    assert_eq!(print_seconds_of("5400", PRINT_TIME_SOURCES), None, "no source: unknown");
    let extra = r#"<metadata key="prediction" value="5400"/>"#;
    let mut entries = base_entries();
    set(&mut entries, "Metadata/slice_info.config", slice_xml(&[slice_plate("1", &[1], extra)]).as_bytes());
    let info = read(&mut build(&entries));
    assert_eq!(info.plates[0].print_seconds, None);
}

#[test]
fn the_first_layer_time_is_never_taken_as_print_time() {
    let mut entries = base_entries();
    set(&mut entries, "Metadata/slice_info.config", slice_xml(&[slice_plate("1", &[1], "")]).as_bytes());
    set(&mut entries, "Metadata/plate_1.json", br#"{"first_layer_time": 400.0, "bed_type": "x"}"#);
    let info = read(&mut build(&entries));
    assert_eq!(info.plates[0].first_layer_seconds, Some(400.0));
    assert_eq!(info.plates[0].print_seconds, None);
}

// Plates

fn plate_indexes(info: &PrintInfo) -> Vec<u32> {
    info.plates.iter().map(|p| p.plate_index).collect()
}

#[test]
fn invalid_duplicate_and_missing_plate_indexes_are_dropped_or_numbered() {
    let mut entries = base_entries();
    let plates = [
        slice_plate("0", &[1], r#"<metadata key="synthetic_print_time" value="100"/>"#),
        slice_plate("2", &[1], r#"<metadata key="synthetic_print_time" value="200"/>"#),
        slice_plate("2", &[1], r#"<metadata key="synthetic_print_time" value="999"/>"#),
        "<plate><metadata key=\"weight\" value=\"1\"/></plate>".to_string(),
    ];
    set(&mut entries, "Metadata/slice_info.config", slice_xml(&plates).as_bytes());
    let (_, info) = read_with(&mut build(&entries), &[TIME_KEY], &Sources::SHIPPED);
    // Index 0 gets no row (see also the unusable-index test), the first plate with index 2 wins, the plate without an index
    // takes its position (4).
    assert_eq!(plate_indexes(&info), vec![2, 4]);
    assert_eq!(info.plates[0].print_seconds, Some(200));
}

#[test]
fn more_than_the_maximum_of_plates_keep_only_the_first_rows() {
    let mut entries = base_entries();
    let plates: Vec<String> = (0..5000).map(|_| "<plate/>".to_string()).collect();
    set(&mut entries, "Metadata/slice_info.config", slice_xml(&plates).as_bytes());
    let info = read(&mut build(&entries));
    // Self-closing plates count for the state but carry no data and make no rows.
    assert_eq!(info.slice_state, SliceState::Sliced);
    assert!(info.plates.is_empty());
    let plates: Vec<String> = (0..5000).map(|_| "<plate></plate>".to_string()).collect();
    set(&mut entries, "Metadata/slice_info.config", slice_xml(&plates).as_bytes());
    let info = read(&mut build(&entries));
    assert_eq!(info.slice_state, SliceState::Sliced);
    assert_eq!(info.plates.len(), MAX_PLATES);
    assert_eq!(info.plates.last().map(|p| p.plate_index), Some(MAX_PLATES as u32));
}

#[test]
fn plate_json_entries_beyond_the_maximum_are_not_rows() {
    let mut entries = base_entries();
    for n in 1..=(MAX_PLATES + 5) {
        set(&mut entries, &format!("Metadata/plate_{n}.json"), br#"{"first_layer_time":5.0}"#);
    }
    // Names that only look similar are no plates.
    set(&mut entries, "Metadata/plate_0.json", b"{}");
    set(&mut entries, "Metadata/plate_x.json", b"{}");
    set(&mut entries, "Metadata/plate_no_light_1.json", b"{}");
    let info = read(&mut build(&entries));
    assert_eq!(info.plates.len(), MAX_PLATES);
    assert_eq!(info.plates[0].first_layer_seconds, Some(5.0));
}

fn padded_plate_json(bytes: usize) -> Vec<u8> {
    let mut text = String::from(r#"{"first_layer_time":7.0,"pad":""#);
    text.push_str(&"A".repeat(bytes));
    text.push_str("\"}");
    text.into_bytes()
}

#[test]
fn an_oversized_plate_json_is_ignored_without_an_error() {
    let mut entries = base_entries();
    set(&mut entries, "Metadata/plate_1.json", &padded_plate_json(5 * 1024 * 1024));
    set(&mut entries, "Metadata/plate_2.json", br#"{"first_layer_time":9.0}"#);
    let info = read(&mut build(&entries));
    assert_eq!(info.plates[0].first_layer_seconds, None);
    assert_eq!(info.plates[1].first_layer_seconds, Some(9.0), "the next plate is still read");
}

#[test]
fn plate_json_reading_stops_at_the_total_limit() {
    let mut entries = base_entries();
    for n in 1..=6 {
        set(&mut entries, &format!("Metadata/plate_{n}.json"), &padded_plate_json(3_900_000));
    }
    let mut archive = build(&entries);
    let (_, _, assignments) = read_plate_metadata(&mut archive);
    let (slice, _) = read_slice_info_detailed_with(&mut archive, &[]);
    let (info, bytes) = read_print_info_with(&mut archive, &slice, &assignments, &Sources::SHIPPED);
    let read_plates: Vec<bool> = info.plates.iter().map(|p| p.first_layer_seconds.is_some()).collect();
    assert_eq!(read_plates, vec![true, true, true, true, false, false], "4 x 3.9 MB fit into 16 MiB, the fifth does not");
    assert!(bytes <= MAX_PLATE_JSON_TOTAL_BYTES + 1024, "{bytes} bytes read");
}

#[test]
fn broken_plate_json_only_means_no_values_for_that_plate() {
    let mut entries = base_entries();
    set(&mut entries, "Metadata/plate_1.json", b"not json");
    set(&mut entries, "Metadata/plate_2.json", b"[1,2]");
    set(&mut entries, "Metadata/plate_3.json", br#"{"first_layer_time":"abc","bed_type":5}"#);
    set(&mut entries, "Metadata/plate_4.json", br#"{"first_layer_time":0.2}"#);
    set(&mut entries, "Metadata/plate_5.json", br#"{"first_layer_time":"12.34","bed_type":"b\u0007ed"}"#);
    let info = read(&mut build(&entries));
    assert_eq!(plate_indexes(&info), vec![1, 2, 3, 4, 5]);
    assert!(info.plates[..4].iter().all(|p| p.first_layer_seconds.is_none() && p.plate_bed_type.is_none()));
    assert_eq!(info.plates[4].first_layer_seconds, Some(12.3));
    assert_eq!(info.plates[4].plate_bed_type.as_deref(), Some("bed"));
}

#[test]
fn plate_names_come_from_the_model_settings() {
    let mut entries = base_entries();
    set(
        &mut entries,
        "Metadata/model_settings.config",
        br#"<config><plate><metadata key="plater_id" value="1"/><metadata key="plater_name" value="Front"/></plate><plate><metadata key="plater_id" value="2"/><metadata key="plater_name" value=""/></plate></config>"#,
    );
    let info = read(&mut build(&entries));
    let names: Vec<_> = info.plates.iter().map(|p| p.plate_name.as_deref()).collect();
    assert_eq!(names, vec![Some("Front"), None]);
}

#[test]
fn a_big_package_with_big_configs_is_read_unchanged() {
    // Sizes like a real project with 200 objects: 237 entries, about 10 MB unpacked, a 1 MB model_settings.config.
    let mut entries = entries_of(BAMBU_SYNTHETIC);
    let reference = read(&mut build(&entries));
    for n in 0..200 {
        set(&mut entries, &format!("3D/Objects/object_{n}.model"), &vec![b' '; 40_000]);
    }
    let mut settings = String::from("<config>");
    for n in 1..=4 {
        settings.push_str(&format!(r#"<plate><metadata key="plater_id" value="{n}"/></plate>"#));
    }
    settings.push_str(&format!("<!--{}-->", "x".repeat(1_000_000)));
    settings.push_str("</config>");
    set(&mut entries, "Metadata/model_settings.config", settings.as_bytes());
    let info = read(&mut build(&entries));
    assert_eq!(info, reference);
}

#[test]
fn a_stated_but_unusable_plate_index_is_dropped_not_renumbered() {
    for bad in ["4294967296", "-1", "abc", "", " ", "1.5"] {
        let plate = format!(r#"<plate><metadata key="index" value="{bad}"/><metadata key="synthetic_print_time" value="100"/></plate>"#);
        let mut entries = base_entries();
        set(&mut entries, "Metadata/slice_info.config", slice_xml(&[plate]).as_bytes());
        let (slice, info) = read_with(&mut build(&entries), &[TIME_KEY], &Sources::SHIPPED);
        assert_eq!(slice.state, SliceState::Sliced);
        assert!(info.plates.is_empty(), "{bad:?} must not become plate 1");
        // The stored slice info is the old one: the plate is there with its position.
        assert_eq!(slice.info.unwrap().plates[0].plate_index, 1);
    }
    // A missing index still takes the position.
    let mut entries = base_entries();
    set(&mut entries, "Metadata/slice_info.config", slice_xml(&["<plate><filament id=\"1\"/></plate>".to_string()]).as_bytes());
    assert_eq!(read(&mut build(&entries)).plates.iter().map(|p| p.plate_index).collect::<Vec<_>>(), vec![1]);
}

#[test]
fn the_first_layer_time_is_rounded_once_directly_to_one_place() {
    let mut entries = base_entries();
    set(&mut entries, "Metadata/plate_1.json", br#"{"first_layer_time":1.0496}"#);
    set(&mut entries, "Metadata/plate_2.json", br#"{"first_layer_time":"2.0499"}"#);
    set(&mut entries, "Metadata/plate_3.json", br#"{"first_layer_time":0.96}"#);
    set(&mut entries, "Metadata/plate_4.json", br#"{"first_layer_time":123.4567}"#);
    let times: Vec<_> = read(&mut build(&entries)).plates.iter().map(|p| p.first_layer_seconds).collect();
    assert_eq!(times, vec![Some(1.0), Some(2.0), None, Some(123.5)]);
}

#[test]
fn plate_json_names_with_leading_zeros_are_no_plates() {
    let mut entries = base_entries();
    set(&mut entries, "Metadata/plate_01.json", br#"{"first_layer_time":5.0}"#);
    set(&mut entries, "Metadata/plate_2.json", br#"{"first_layer_time":6.0}"#);
    let info = read(&mut build(&entries));
    assert_eq!(plate_indexes(&info), vec![2]);
}

/// Stored (not compressed) entries so a big test archive builds fast.
fn build_stored(entries: &Entries) -> ZipArchive<Cursor<Vec<u8>>> {
    let mut buf = Vec::new();
    {
        let mut writer = ZipWriter::new(Cursor::new(&mut buf));
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, content) in entries {
            writer.start_file(name.as_str(), options).unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
    }
    ZipArchive::new(Cursor::new(buf)).unwrap()
}

#[test]
fn unusable_plate_json_entries_count_against_the_budget_too() {
    // Invalid UTF-8, just below the single-entry limit, many plates: without counting, every
    // one of them would be decompressed in full.
    let mut entries = base_entries();
    let bad = vec![0xffu8; MAX_PLATE_JSON_BYTES as usize - 16];
    for n in 1..=8 {
        set(&mut entries, &format!("Metadata/plate_{n}.json"), &bad);
    }
    let mut archive = build_stored(&entries);
    let (_, _, assignments) = read_plate_metadata(&mut archive);
    let (slice, _) = read_slice_info_detailed_with(&mut archive, &[]);
    let (info, bytes) = read_print_info_with(&mut archive, &slice, &assignments, &Sources::SHIPPED);
    assert!(bytes > MAX_PLATE_JSON_TOTAL_BYTES / 2, "the failed entries are counted: {bytes}");
    assert!(bytes <= MAX_PLATE_JSON_TOTAL_BYTES + 64, "never more than the total budget: {bytes}");
    assert!(info.plates.iter().all(|p| p.first_layer_seconds.is_none()));
}

#[test]
fn an_entry_is_limited_by_the_rest_of_the_budget_not_only_by_its_own_limit() {
    // Four valid-looking entries of 3.9 MB use 15.6 MB; the fifth, whose size field is honest but
    // larger than the rest, is skipped, and a lying one would be cut at the rest.
    let mut entries = base_entries();
    for n in 1..=5 {
        set(&mut entries, &format!("Metadata/plate_{n}.json"), &padded_plate_json(3_900_000));
    }
    let mut archive = build_stored(&entries);
    let (_, _, assignments) = read_plate_metadata(&mut archive);
    let (slice, _) = read_slice_info_detailed_with(&mut archive, &[]);
    let (info, bytes) = read_print_info_with(&mut archive, &slice, &assignments, &Sources::SHIPPED);
    assert_eq!(info.plates.iter().filter(|p| p.first_layer_seconds.is_some()).count(), 4);
    assert!(bytes <= MAX_PLATE_JSON_TOTAL_BYTES);
}

#[test]
fn an_invalid_utf8_settings_entry_is_counted_as_read() {
    let mut entries = base_entries();
    set(&mut entries, "Metadata/project_settings.config", &vec![0xffu8; 100_000]);
    let mut archive = build(&entries);
    let (_, _, assignments) = read_plate_metadata(&mut archive);
    let (slice, _) = read_slice_info_detailed_with(&mut archive, &[]);
    let (info, bytes) = read_print_info_with(&mut archive, &slice, &assignments, &Sources::SHIPPED);
    assert_eq!(info.settings_state, SettingsState::Unreadable);
    assert_eq!(bytes, 100_000);
}
