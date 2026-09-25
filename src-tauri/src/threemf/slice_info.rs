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

/// Liest `Metadata/slice_info.config` (Bambu Studio/OrcaSlicer, kein
/// 3MF-Standard). Fehlende, kaputte oder plattenlose Datei: `None`. Gelesen mit
/// Groessenlimit (Zip-Bombe); die gelesenen Bytes kommen zurueck, damit
/// `read_package()` sie ins Gesamtbudget einrechnet.
pub fn parse_slice_info<R: Read + Seek>(archive: &mut ZipArchive<R>) -> (Option<SliceInfo>, u64) {
    let xml = super::container::read_entry_to_string(
        archive,
        "Metadata/slice_info.config",
        super::container::MAX_CONFIG_XML_BYTES,
    );
    let bytes_read = xml.as_ref().map(|s| s.len() as u64).unwrap_or(0);
    let Ok(xml) = xml else {
        return (None, bytes_read);
    };

    let mut reader = quick_xml::Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    let mut plates: Vec<PlateFilamentUsage> = Vec::new();
    let mut plate_counter: u32 = 0;
    let mut in_plate = false;
    let mut current_index: Option<u32> = None;
    let mut current_weight: Option<f64> = None;
    let mut current_filaments: Vec<FilamentUsage> = Vec::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if local_name(e.name().as_ref()) == "plate" => {
                in_plate = true;
                plate_counter += 1;
                current_index = None;
                current_weight = None;
                current_filaments = Vec::new();
            }
            Ok(Event::End(e)) if local_name(e.name().as_ref()) == "plate" => {
                if in_plate {
                    let weight_g = current_weight
                        .unwrap_or_else(|| current_filaments.iter().map(|f| f.used_g).sum());
                    plates.push(PlateFilamentUsage {
                        plate_index: current_index.unwrap_or(plate_counter),
                        weight_g,
                        filaments: std::mem::take(&mut current_filaments),
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
                            Some("index") => current_index = value.and_then(|v| v.parse().ok()),
                            Some("weight") => current_weight = value.and_then(|v| v.parse().ok()),
                            _ => {}
                        }
                    }
                    "filament" => {
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
            Ok(Event::Eof) => break,
            Err(_) => return (None, bytes_read),
            _ => {}
        }
        buf.clear();
    }

    if plates.is_empty() {
        return (None, bytes_read);
    }

    let total_weight_g = plates.iter().map(|p| p.weight_g).sum();
    (Some(SliceInfo { total_weight_g, plates }), bytes_read)
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

    /// Archiv mit zu grossem Config-Eintrag (Mini-Zip-Bombe).
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
