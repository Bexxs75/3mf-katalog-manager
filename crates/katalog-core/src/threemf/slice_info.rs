use std::io::{Read, Seek};

use quick_xml::events::{BytesStart, Event};
use quick_xml::XmlVersion;
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceInfo {
    pub total_weight_g: f64,
    pub plates: Vec<PlateFilamentUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlateFilamentUsage {
    pub plate_index: u32,
    pub weight_g: f64,
    pub filaments: Vec<FilamentUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilamentUsage {
    pub filament_type: String,
    pub color: Option<String>,
    pub used_g: f64,
    pub used_m: f64,
}

fn local_name(qname: &[u8]) -> &str {
    let s = std::str::from_utf8(qname).unwrap_or("");
    match s.rfind(':') {
        Some(idx) => &s[idx + 1..],
        None => s,
    }
}

fn get_attr(e: &BytesStart, name: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if local_name(a.key.as_ref()) == name {
            a.normalized_value(XmlVersion::Implicit1_0).ok().map(|v| v.into_owned())
        } else {
            None
        }
    })
}

/// What the file says about slicing, told apart (a plain `None` cannot do that).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceState {
    /// Readable, well-formed, at least one `<plate>`.
    Sliced,
    /// Entry present and well-formed but without a `<plate>`: the file holds no print
    /// data. That does not say the model was never sliced (a project saved after
    /// slicing looks the same).
    NoSliceData,
    /// The entry does not exist: nothing proves anything about the file.
    NotRecognized,
    /// The entry exists but is too large, not well-formed, or could not be read.
    Unreadable,
}

impl SliceState {
    pub fn as_str(self) -> &'static str {
        match self {
            SliceState::Sliced => "sliced",
            SliceState::NoSliceData => "no_slice_data",
            SliceState::NotRecognized => "not_recognized",
            SliceState::Unreadable => "unreadable",
        }
    }
}

/// Per-plate data of `slice_info.config` that is kept apart from [`SliceInfo`], so the
/// stored `slice_info_json` stays byte-identical.
#[derive(Debug, Clone, PartialEq)]
pub struct PlateSliceExtra {
    /// As in [`PlateFilamentUsage::plate_index`]: the stated index or the position; may be 0.
    pub plate_index: u32,
    /// Raw (not yet validated) value of the first matching print time source, in seconds.
    pub print_seconds: Option<f64>,
    /// `id` of every `<filament>` of the plate, in file order.
    pub used_filament_ids: Vec<u32>,
    /// False when a `<filament>` had no usable `id`: the list is then not the whole truth,
    /// so nothing derived from it (temperatures) may be trusted.
    pub filament_ids_complete: bool,
}

#[derive(Debug, Clone)]
pub struct SliceDetection {
    pub state: SliceState,
    /// Same value `parse_slice_info` returns.
    pub info: Option<SliceInfo>,
    /// One entry per entry of `info.plates`, same order; empty without `info`.
    pub plate_extras: Vec<PlateSliceExtra>,
}

fn detection(state: SliceState) -> SliceDetection {
    SliceDetection { state, info: None, plate_extras: Vec::new() }
}

/// Reads `Metadata/slice_info.config` (Bambu Studio/OrcaSlicer, not part of the
/// 3MF standard). Missing, broken or plate-less file: `None`. Read with a size
/// limit (zip bomb); the bytes read are returned, so `read_package()` adds them to
/// the total budget.
pub fn parse_slice_info<R: Read + Seek>(archive: &mut ZipArchive<R>) -> (Option<SliceInfo>, u64) {
    // Lenient scan: the old behaviour (a cut-off document still yields what was read before
    // the end; an empty `<plate/>` is not a plate) stays exactly as it was.
    let (detection, bytes_read) = scan_slice_info(archive, &[], false);
    (detection.info, bytes_read)
}

/// Like [`parse_slice_info`], but tells the four cases apart and also returns the
/// per-plate extras. Print time keys come from the (currently empty) shipped table.
pub fn read_slice_info_detailed<R: Read + Seek>(archive: &mut ZipArchive<R>) -> (SliceDetection, u64) {
    read_slice_info_detailed_with(archive, super::print_info::PRINT_TIME_SOURCES)
}

/// `print_time_keys`: `<metadata key>` names inside a `<plate>` that hold the print time in seconds.
pub fn read_slice_info_detailed_with<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    print_time_keys: &[&str],
) -> (SliceDetection, u64) {
    scan_slice_info(archive, print_time_keys, true)
}

/// `strict`: only a complete, well-formed document without `<plate>` is `NoSliceData`;
/// an empty, cut-off or otherwise incomplete one is `Unreadable`, and a self-closing
/// `<plate/>` counts as a plate for the state (it adds no entry to `info`, which keeps
/// the stored slice info as it always was).
fn scan_slice_info<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    print_time_keys: &[&str],
    strict: bool,
) -> (SliceDetection, u64) {
    let xml = super::container::read_entry_to_string(
        archive,
        "Metadata/slice_info.config",
        super::container::MAX_CONFIG_XML_BYTES,
    );
    let bytes_read = xml.as_ref().map(|s| s.len() as u64).unwrap_or(0);
    let xml = match xml {
        Ok(xml) => xml,
        // Only a missing entry is "not recognised"; every other failure is a read problem.
        Err(super::error::ThreeMfError::Zip(zip::result::ZipError::FileNotFound)) => {
            return (detection(SliceState::NotRecognized), bytes_read);
        }
        Err(_) => return (detection(SliceState::Unreadable), bytes_read),
    };

    let mut reader = quick_xml::Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    let mut plates: Vec<PlateFilamentUsage> = Vec::new();
    let mut extras: Vec<PlateSliceExtra> = Vec::new();
    let mut plate_counter: u32 = 0;
    let mut in_plate = false;
    let mut current_index: Option<u32> = None;
    // Whether an `index` was stated at all: a stated but unusable one is not replaced by the position.
    let mut index_stated = false;
    let mut depth: usize = 0;
    let mut seen_element = false;
    let mut empty_plates: usize = 0;
    // Set at the end of a cut-off or empty document (strict only). The plates read before the
    // cut stay in `info`, exactly as the lenient scan returns them.
    let incomplete: bool;
    let mut current_weight: Option<f64> = None;
    let mut current_seconds: Option<f64> = None;
    let mut current_ids: Vec<u32> = Vec::new();
    let mut ids_complete = true;
    let mut current_filaments: Vec<FilamentUsage> = Vec::new();

    let mut buf = Vec::new();
    loop {
        let event = reader.read_event_into(&mut buf);
        // Nesting depth, to tell a complete document from a cut-off one at the end.
        match &event {
            Ok(Event::Start(_)) => {
                depth += 1;
                seen_element = true;
            }
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Empty(_)) => seen_element = true,
            _ => {}
        }
        match event {
            Ok(Event::Start(e)) if local_name(e.name().as_ref()) == "plate" => {
                in_plate = true;
                index_stated = false;
                plate_counter += 1;
                current_index = None;
                current_weight = None;
                current_seconds = None;
                current_ids = Vec::new();
                ids_complete = true;
                current_filaments = Vec::new();
            }
            Ok(Event::Empty(e)) if local_name(e.name().as_ref()) == "plate" => {
                empty_plates += 1;
            }
            Ok(Event::End(e)) if local_name(e.name().as_ref()) == "plate" => {
                if in_plate {
                    let weight_g = current_weight
                        .unwrap_or_else(|| current_filaments.iter().map(|f| f.used_g).sum());
                    let plate_index = current_index.unwrap_or(plate_counter);
                    // Extras: a stated index that is unusable (not a number, out of range) makes
                    // the plate unusable (0 = no row), it is not renumbered.
                    let extra_index = match (index_stated, current_index) {
                        (false, _) => plate_counter,
                        (true, Some(index)) => index,
                        (true, None) => 0,
                    };
                    plates.push(PlateFilamentUsage {
                        plate_index,
                        weight_g,
                        filaments: std::mem::take(&mut current_filaments),
                    });
                    extras.push(PlateSliceExtra {
                        plate_index: extra_index,
                        print_seconds: current_seconds.take(),
                        used_filament_ids: std::mem::take(&mut current_ids),
                        filament_ids_complete: ids_complete,
                    });
                }
                in_plate = false;
            }
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if in_plate => {
                match local_name(e.name().as_ref()) {
                    "metadata" => {
                        let key = get_attr(&e, "key");
                        let value = get_attr(&e, "value");
                        match key.as_deref() {
                            Some("index") => {
                                index_stated = true;
                                current_index = value.and_then(|v| v.parse().ok());
                            }
                            Some("weight") => current_weight = value.and_then(|v| v.parse().ok()),
                            Some(other) if current_seconds.is_none() && print_time_keys.contains(&other) => {
                                current_seconds = value.and_then(|v| v.trim().parse().ok());
                            }
                            _ => {}
                        }
                    }
                    "filament" => {
                        match get_attr(&e, "id").and_then(|v| v.trim().parse().ok()) {
                            Some(id) => current_ids.push(id),
                            None => ids_complete = false,
                        }
                        current_filaments.push(FilamentUsage {
                            filament_type: get_attr(&e, "type").unwrap_or_default(),
                            color: get_attr(&e, "color"),
                            used_g: get_attr(&e, "used_g").and_then(|v| v.parse().ok()).unwrap_or(0.0),
                            used_m: get_attr(&e, "used_m").and_then(|v| v.parse().ok()).unwrap_or(0.0),
                        });
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => {
                incomplete = strict && (depth != 0 || !seen_element);
                break;
            }
            Err(_) => return (detection(SliceState::Unreadable), bytes_read),
            _ => {}
        }
        buf.clear();
    }

    if plates.is_empty() {
        if incomplete {
            return (detection(SliceState::Unreadable), bytes_read);
        }
        let state = if strict && empty_plates > 0 { SliceState::Sliced } else { SliceState::NoSliceData };
        return (detection(state), bytes_read);
    }

    let total_weight_g = plates.iter().map(|p| p.weight_g).sum();
    (
        SliceDetection {
            state: if incomplete { SliceState::Unreadable } else { SliceState::Sliced },
            info: Some(SliceInfo { total_weight_g, plates }),
            plate_extras: extras,
        },
        bytes_read,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn build_zip(entries: &[(&str, &str)]) -> ZipArchive<Cursor<Vec<u8>>> {
        let mut buf = Vec::new();
        {
            let mut writer = ZipWriter::new(Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            for (name, content) in entries {
                writer.start_file(*name, opts).unwrap();
                writer.write_all(content.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        ZipArchive::new(Cursor::new(buf)).unwrap()
    }

    const SINGLE_PLATE_SINGLE_FILAMENT: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="weight" value="15.83"/>
    <object identify_id="1" name="Shape.stl" skipped="false"/>
    <filament id="1" tray_info_idx="GFA00" type="PLA" color="#FFFFFFFF" used_m="14.5" used_g="43.24"/>
  </plate>
</config>"##;

    /// Archive with an oversized config entry (mini zip bomb).
    fn build_zip_with_oversized_config(name: &str) -> ZipArchive<Cursor<Vec<u8>>> {
        let oversized = (super::super::container::MAX_CONFIG_XML_BYTES + 1) as usize;
        let mut buf = Vec::new();
        {
            let mut writer = ZipWriter::new(Cursor::new(&mut buf));
            writer.start_file(name, SimpleFileOptions::default()).unwrap();
            writer.write_all(&vec![b'A'; oversized]).unwrap();
            writer.finish().unwrap();
        }
        ZipArchive::new(Cursor::new(buf)).unwrap()
    }

    #[test]
    fn rejects_an_oversized_slice_info_config_instead_of_reading_it() {
        let mut archive = build_zip_with_oversized_config("Metadata/slice_info.config");
        assert!(
            parse_slice_info(&mut archive).0.is_none(),
            "ein ueberlanger Config-Eintrag darf nicht komplett eingelesen werden"
        );
    }

    #[test]
    fn parses_single_plate_single_filament() {
        let mut archive = build_zip(&[("Metadata/slice_info.config", SINGLE_PLATE_SINGLE_FILAMENT)]);
        let info = parse_slice_info(&mut archive).0.expect("slice info present");

        assert!((info.total_weight_g - 15.83).abs() < 1e-6);
        assert_eq!(info.plates.len(), 1);
        assert_eq!(info.plates[0].plate_index, 1);
        assert!((info.plates[0].weight_g - 15.83).abs() < 1e-6);
        assert_eq!(info.plates[0].filaments.len(), 1);
        assert_eq!(info.plates[0].filaments[0].filament_type, "PLA");
        assert_eq!(info.plates[0].filaments[0].color.as_deref(), Some("#FFFFFFFF"));
        assert!((info.plates[0].filaments[0].used_g - 43.24).abs() < 1e-6);
        assert!((info.plates[0].filaments[0].used_m - 14.5).abs() < 1e-6);
    }

    const MULTICOLOR_PLATE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="weight" value="15.20"/>
    <filament id="1" type="PLA" color="#FFFFFFFF" used_m="10.5" used_g="10.43"/>
    <filament id="2" type="PLA" color="#000000FF" used_m="4.8" used_g="4.77"/>
  </plate>
</config>"##;

    #[test]
    fn parses_multicolor_plate_with_multiple_filaments() {
        let mut archive = build_zip(&[("Metadata/slice_info.config", MULTICOLOR_PLATE)]);
        let info = parse_slice_info(&mut archive).0.expect("slice info present");

        assert_eq!(info.plates[0].filaments.len(), 2);
        assert_eq!(info.plates[0].filaments[1].color.as_deref(), Some("#000000FF"));
    }

    const TWO_PLATES: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="weight" value="10.00"/>
    <filament id="1" type="PLA" color="#FFFFFFFF" used_m="4.0" used_g="10.00"/>
  </plate>
  <plate>
    <metadata key="index" value="2"/>
    <metadata key="weight" value="5.00"/>
    <filament id="1" type="PETG" color="#FF0000FF" used_m="2.0" used_g="5.00"/>
  </plate>
</config>"##;

    #[test]
    fn parses_multiple_plates_and_sums_total_weight() {
        let mut archive = build_zip(&[("Metadata/slice_info.config", TWO_PLATES)]);
        let info = parse_slice_info(&mut archive).0.expect("slice info present");

        assert_eq!(info.plates.len(), 2);
        assert!((info.total_weight_g - 15.00).abs() < 1e-6);
        assert_eq!(info.plates[1].plate_index, 2);
        assert_eq!(info.plates[1].filaments[0].filament_type, "PETG");
    }

    #[test]
    fn falls_back_to_summed_filament_weight_when_weight_metadata_missing() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <filament id="1" type="PLA" color="#FFFFFFFF" used_m="4.0" used_g="12.5"/>
  </plate>
</config>"##;
        let mut archive = build_zip(&[("Metadata/slice_info.config", xml)]);
        let info = parse_slice_info(&mut archive).0.expect("slice info present");

        assert!((info.plates[0].weight_g - 12.5).abs() < 1e-6);
    }

    #[test]
    fn returns_none_when_config_missing() {
        let mut archive = build_zip(&[("3D/3dmodel.model", "<model></model>")]);
        assert!(parse_slice_info(&mut archive).0.is_none());
    }

    #[test]
    fn returns_none_when_config_is_not_valid_xml() {
        let mut archive = build_zip(&[("Metadata/slice_info.config", "not xml at all <<<")]);
        assert!(parse_slice_info(&mut archive).0.is_none());
    }

    #[test]
    fn returns_none_when_no_plate_elements_present() {
        let mut archive = build_zip(&[("Metadata/slice_info.config", "<config></config>")]);
        assert!(parse_slice_info(&mut archive).0.is_none());
    }

    #[test]
    fn ignores_namespace_prefix_on_plate_elements() {
        let xml = r#"<config><p:plate xmlns:p="urn:x">
          <p:metadata key="weight" value="1.5"/>
          <p:filament type="PLA" used_g="1.5" used_m="0.5"/>
        </p:plate></config>"#;
        let mut archive = build_zip(&[("Metadata/slice_info.config", xml)]);
        let info = parse_slice_info(&mut archive).0.expect("slice info present");
        assert_eq!(info.plates.len(), 1);
    }
}
