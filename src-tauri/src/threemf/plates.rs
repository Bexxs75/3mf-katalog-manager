use std::io::{Read, Seek};
use zip::ZipArchive;

/// Liest `Metadata/model_settings.config` (Bambu Studio/OrcaSlicer-
/// spezifisch, kein Teil des offiziellen 3MF-Standards) aus dem bereits
/// geoeffneten Zip-Archiv und zaehlt die enthaltenen `<plate>`-Elemente
/// (Namespace-Praefix wird ignoriert, gleiche Toleranz wie der
/// bestehende Model-Parser). Existiert die Datei nicht oder laesst sie
/// sich nicht als XML lesen, wird `None` zurueckgegeben - kein
/// Fehlerfall, gleiches Verhalten wie das bestehende Thumbnail-Fallback.
///
/// Gelesen wird ueber `container::read_entry_to_string`, also mit derselben
/// Groessen-Obergrenze wie alle anderen Paket-Eintraege - ohne sie koennte
/// dieser Eintrag als Zip-Bombe den gesamten Arbeitsspeicher belegen
/// (Security-Review 2026-09-19, Finding A-1; Nachtrag zu Commit 800e373,
/// der genau diese beiden Config-Eintraege noch ausgelassen hatte).
pub fn count_plates<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Option<u32> {
    let xml = super::container::read_entry_to_string(
        archive,
        "Metadata/model_settings.config",
        super::container::MAX_CONFIG_XML_BYTES,
    )
    .ok()?;

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
            Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }

    if count == 0 {
        None
    } else {
        Some(count)
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

    /// Baut ein Archiv, dessen Config-Eintrag entpackt groesser als die
    /// erlaubte Obergrenze ist - Miniatur-"Zip-Bombe" (Security-Review
    /// 2026-09-19, Finding A-1).
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
            count_plates(&mut archive),
            None,
            "ein ueberlanger Config-Eintrag darf nicht komplett eingelesen werden"
        );
    }

    #[test]
    fn counts_plate_elements_when_config_present() {
        let mut archive = build_zip(&[
            ("Metadata/model_settings.config", MODEL_SETTINGS_TWO_PLATES),
        ]);
        assert_eq!(count_plates(&mut archive), Some(2));
    }

    #[test]
    fn returns_none_when_config_missing() {
        let mut archive = build_zip(&[("3D/3dmodel.model", "<model></model>")]);
        assert_eq!(count_plates(&mut archive), None);
    }

    #[test]
    fn returns_none_when_config_is_not_valid_xml() {
        let mut archive = build_zip(&[
            ("Metadata/model_settings.config", "not xml at all <<<"),
        ]);
        assert_eq!(count_plates(&mut archive), None);
    }

    #[test]
    fn ignores_namespace_prefix_on_plate_elements() {
        let mut archive = build_zip(&[(
            "Metadata/model_settings.config",
            r#"<config><p:plate xmlns:p="urn:x"></p:plate></config>"#,
        )]);
        assert_eq!(count_plates(&mut archive), Some(1));
    }
}
