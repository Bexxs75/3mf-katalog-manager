use std::io::{Read, Seek};
use zip::ZipArchive;

/// Counts the `<plate>` elements in `Metadata/model_settings.config` (Bambu
/// Studio/OrcaSlicer, not part of the 3MF standard). Missing or broken file:
/// `None`. Read with a size limit (zip bomb); the bytes read are returned, so
/// `read_package()` adds them to the total budget.
pub fn read_plate_metadata<R: Read + Seek>(archive: &mut ZipArchive<R>) -> (Option<u32>, u64, PlateAssignments) {
    let xml = super::container::read_entry_to_string(
        archive,
        "Metadata/model_settings.config",
        super::container::MAX_CONFIG_XML_BYTES,
    );
    let bytes_read = xml.as_ref().map(|s| s.len() as u64).unwrap_or(0);
    let Ok(xml) = xml else {
        return (None, bytes_read, PlateAssignments::default());
    };

    let mut reader = quick_xml::Reader::from_str(&xml);
    reader.config_mut().trim_text(true);
    let mut count = 0u32;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e)) => {
                let local_name = e.local_name();
                if local_name.as_ref() == b"plate" {
                    count += 1;
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => return (None, bytes_read, PlateAssignments::default()),
            _ => {}
        }
        buf.clear();
    }

    if count == 0 {
        (None, bytes_read, PlateAssignments::default())
    } else {
        (Some(count), bytes_read, parse_assignments(&xml))
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlateAssignments {
    pub plates: std::collections::BTreeMap<u32, Option<String>>,
    pub objects: std::collections::HashMap<String, u32>,
}

#[derive(Default)]
struct PendingPlate {
    number: Option<u32>,
    name: Option<String>,
    objects: Vec<String>,
}

/// Slicer metadata is optional; malformed XML must not invalidate geometry.
pub fn parse_assignments(xml: &str) -> PlateAssignments {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut result = PlateAssignments::default();
    let mut plate: Option<PendingPlate> = None;
    let mut in_instance = false;
    let mut depth = 0usize;
    loop {
        match reader.read_event() {
            Ok(event @ (Event::Start(_) | Event::Empty(_))) => {
                let (element, opens) = match event {
                    Event::Start(e) => (e, true),
                    Event::Empty(e) => (e, false),
                    _ => unreachable!(),
                };
                if opens { depth += 1; }
                match element.local_name().as_ref() {
                    b"plate" if opens => plate = Some(PendingPlate::default()),
                    b"model_instance" => {
                        in_instance = opens;
                        if let Some(plate) = &mut plate {
                            for attr in element.attributes().flatten() {
                                if attr.key.as_ref() == b"object_id" {
                                    if let Ok(value) = attr.decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder()) {
                                        if !value.is_empty() { plate.objects.push(value.into_owned()); }
                                    }
                                }
                            }
                        }
                    }
                    b"metadata" => {
                        let mut key = None;
                        let mut value = None;
                        for attr in element.attributes().flatten() {
                            let Ok(text) = attr.decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder()) else { continue; };
                            match attr.key.as_ref() {
                                b"key" => key = Some(text.into_owned()),
                                b"value" => value = Some(text.into_owned()),
                                _ => {}
                            }
                        }
                        if let (Some(plate), Some(key), Some(value)) = (&mut plate, key, value) {
                            match key.as_str() {
                                "plater_id" if !in_instance => plate.number = value.parse::<u32>().ok().filter(|n| *n > 0),
                                "plater_name" if !in_instance => plate.name = (!value.trim().is_empty()).then_some(value),
                                "object_id" if in_instance && !value.is_empty() => plate.objects.push(value),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                depth = depth.saturating_sub(1);
                match e.local_name().as_ref() {
                    b"model_instance" => in_instance = false,
                    b"plate" => {
                        if let Some(PendingPlate { number: Some(number), name, objects }) = plate.take() {
                            result.plates.insert(number, name);
                            for object in objects { result.objects.insert(object, number); }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) if depth == 0 => return result,
            Ok(Event::Eof) | Err(_) => return PlateAssignments::default(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn assignments_use_object_ids_and_ignore_incomplete_fields() {
        let result = parse_assignments(r#"<config>
          <plate><metadata key="plater_id" value="1"/><metadata key="plater_name" value=""/>
            <model_instance><metadata key="object_id" value="4"/></model_instance></plate>
          <plate><metadata key="plater_id" value="2"/><metadata key="plater_name" value="Regal &amp; Böden"></metadata>
            <model_instance object_id="18"/><model_instance><metadata key="object_id" value="20"/></model_instance></plate>
          <plate><metadata key="plater_id" value=""/><model_instance object_id="27"/></plate>
          <plate/><plate><metadata key="plater_id" value="3"/></plate>
        </config>"#);
        assert_eq!(result.objects.get("4"), Some(&1));
        assert_eq!(result.objects.get("18"), Some(&2));
        assert_eq!(result.objects.get("20"), Some(&2));
        assert!(!result.objects.contains_key("27"));
        assert_eq!(result.plates.get(&1), Some(&None));
        assert_eq!(result.plates.get(&2), Some(&Some("Regal & Böden".into())));
    }

    #[test]
    fn broken_or_absent_plate_metadata_has_no_assignments() {
        for xml in ["", "<config/>", "<<<", "<config><plate>",
            r#"<config><plate><metadata key="plater_id" value="1"/></plate></wrong>"#] {
            let result = parse_assignments(xml);
            assert!(result.objects.is_empty());
            assert!(result.plates.is_empty());
        }
    }

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

    const MODEL_SETTINGS_TWO_PLATES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="plater_id" value="1"/>
  </plate>
  <plate>
    <metadata key="plater_id" value="2"/>
  </plate>
</config>"#;

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
    fn rejects_an_oversized_model_settings_config_instead_of_reading_it() {
        let mut archive = build_zip_with_oversized_config("Metadata/model_settings.config");
        assert_eq!(
            read_plate_metadata(&mut archive).0,
            None,
            "ein ueberlanger Config-Eintrag darf nicht komplett eingelesen werden"
        );
    }

    #[test]
    fn counts_plate_elements_when_config_present() {
        let mut archive = build_zip(&[
            ("Metadata/model_settings.config", MODEL_SETTINGS_TWO_PLATES),
        ]);
        assert_eq!(read_plate_metadata(&mut archive).0, Some(2));
    }

    #[test]
    fn returns_none_when_config_missing() {
        let mut archive = build_zip(&[("3D/3dmodel.model", "<model></model>")]);
        assert_eq!(read_plate_metadata(&mut archive).0, None);
    }

    #[test]
    fn returns_none_when_config_is_not_valid_xml() {
        let mut archive = build_zip(&[
            ("Metadata/model_settings.config", "not xml at all <<<"),
        ]);
        assert_eq!(read_plate_metadata(&mut archive).0, None);
    }

    #[test]
    fn ignores_namespace_prefix_on_plate_elements() {
        let mut archive = build_zip(&[(
            "Metadata/model_settings.config",
            r#"<config><p:plate xmlns:p="urn:x"></p:plate></config>"#,
        )]);
        assert_eq!(read_plate_metadata(&mut archive).0, Some(1));
    }

    #[test]
    fn reports_bytes_read_when_config_present() {
        let mut archive = build_zip(&[
            ("Metadata/model_settings.config", MODEL_SETTINGS_TWO_PLATES),
        ]);
        let (_, bytes_read, _) = read_plate_metadata(&mut archive);
        assert_eq!(bytes_read, MODEL_SETTINGS_TWO_PLATES.len() as u64);
    }

    #[test]
    fn reports_zero_bytes_read_when_config_missing() {
        let mut archive = build_zip(&[("3D/3dmodel.model", "<model></model>")]);
        let (_, bytes_read, _) = read_plate_metadata(&mut archive);
        assert_eq!(bytes_read, 0);
    }
}
