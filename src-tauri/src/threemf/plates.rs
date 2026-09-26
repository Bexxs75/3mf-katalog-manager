use std::io::{Read, Seek};
use zip::ZipArchive;

/// Counts the `<plate>` elements in `Metadata/model_settings.config` (Bambu
/// Studio/OrcaSlicer, not part of the 3MF standard). Missing or broken file:
/// `None`. Read with a size limit (zip bomb); the bytes read are returned, so
/// `read_package()` adds them to the total budget.
pub fn count_plates<R: Read + Seek>(archive: &mut ZipArchive<R>) -> (Option<u32>, u64) {
    let xml = super::container::read_entry_to_string(
        archive,
        "Metadata/model_settings.config",
        super::container::MAX_CONFIG_XML_BYTES,
    );
    let bytes_read = xml.as_ref().map(|s| s.len() as u64).unwrap_or(0);
    let Ok(xml) = xml else {
        return (None, bytes_read);
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
            Err(_) => return (None, bytes_read),
            _ => {}
        }
        buf.clear();
    }

    if count == 0 {
        (None, bytes_read)
    } else {
        (Some(count), bytes_read)
    }
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
            count_plates(&mut archive).0,
            None,
            "ein ueberlanger Config-Eintrag darf nicht komplett eingelesen werden"
        );
    }

    #[test]
    fn counts_plate_elements_when_config_present() {
        let mut archive = build_zip(&[
            ("Metadata/model_settings.config", MODEL_SETTINGS_TWO_PLATES),
        ]);
        assert_eq!(count_plates(&mut archive).0, Some(2));
    }

    #[test]
    fn returns_none_when_config_missing() {
        let mut archive = build_zip(&[("3D/3dmodel.model", "<model></model>")]);
        assert_eq!(count_plates(&mut archive).0, None);
    }

    #[test]
    fn returns_none_when_config_is_not_valid_xml() {
        let mut archive = build_zip(&[
            ("Metadata/model_settings.config", "not xml at all <<<"),
        ]);
        assert_eq!(count_plates(&mut archive).0, None);
    }

    #[test]
    fn ignores_namespace_prefix_on_plate_elements() {
        let mut archive = build_zip(&[(
            "Metadata/model_settings.config",
            r#"<config><p:plate xmlns:p="urn:x"></p:plate></config>"#,
        )]);
        assert_eq!(count_plates(&mut archive).0, Some(1));
    }

    #[test]
    fn reports_bytes_read_when_config_present() {
        let mut archive = build_zip(&[
            ("Metadata/model_settings.config", MODEL_SETTINGS_TWO_PLATES),
        ]);
        let (_, bytes_read) = count_plates(&mut archive);
        assert_eq!(bytes_read, MODEL_SETTINGS_TWO_PLATES.len() as u64);
    }

    #[test]
    fn reports_zero_bytes_read_when_config_missing() {
        let mut archive = build_zip(&[("3D/3dmodel.model", "<model></model>")]);
        let (_, bytes_read) = count_plates(&mut archive);
        assert_eq!(bytes_read, 0);
    }
}
