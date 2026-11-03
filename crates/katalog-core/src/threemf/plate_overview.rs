//! Objects and preview pictures per plate for the plate overview dialog. Read on demand when the
//! dialog opens, never stored. Everything is optional: a value the file does not give stays `None`
//! (never 0 as a stand-in), and a damaged or oversized entry only means "no data".

use std::io::{Read, Seek};

use zip::ZipArchive;

use super::container::{read_entry_to_string, MAX_CONFIG_XML_BYTES};
use super::print_info::MAX_PLATES;

const MODEL_SETTINGS_PATH: &str = "Metadata/model_settings.config";
/// A larger picture is skipped; a plate preview is a few KiB in practice.
pub const MAX_PREVIEW_BYTES: u64 = 512 * 1024;
/// All pictures of one answer together (raw bytes).
pub const MAX_PREVIEW_TOTAL_BYTES: u64 = 8 * 1024 * 1024;
/// Only the first plates get a picture, so a file with hundreds of plates cannot fill the answer.
pub const MAX_PREVIEW_PLATES: usize = 64;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlateOverviewRow {
    pub plate_index: u32,
    /// `<model_instance>` elements of the plate.
    pub object_count: Option<u32>,
    /// Raw PNG bytes, only when previews were asked for and the entry passed the checks.
    pub preview: Option<Vec<u8>>,
}

/// One row per plate of `Metadata/model_settings.config`, ordered by plate number. No such entry,
/// an oversized or broken one: no rows.
pub fn read_plate_overview<R: Read + Seek>(archive: &mut ZipArchive<R>, with_previews: bool) -> Vec<PlateOverviewRow> {
    let Ok(xml) = read_entry_to_string(archive, MODEL_SETTINGS_PATH, MAX_CONFIG_XML_BYTES) else {
        return Vec::new();
    };
    let mut rows: Vec<PlateOverviewRow> = count_instances(&xml)
        .into_iter()
        .map(|(plate_index, count)| PlateOverviewRow { plate_index, object_count: Some(count), preview: None })
        .collect();
    if with_previews {
        let mut budget = MAX_PREVIEW_TOTAL_BYTES;
        for row in rows.iter_mut().take(MAX_PREVIEW_PLATES) {
            // The names are built from the plate number; a path stored in the file is never followed.
            // The small picture comes first, the full-size one only stands in when it is missing.
            let small = format!("Metadata/plate_{}_small.png", row.plate_index);
            let full = format!("Metadata/plate_{}.png", row.plate_index);
            if let Some(bytes) = read_png(archive, &small, budget).or_else(|| read_png(archive, &full, budget)) {
                budget -= bytes.len() as u64;
                row.preview = Some(bytes);
            }
        }
    }
    rows
}

/// `(plate number, instance count)` per plate, ordered by number; the first plate with a number wins.
fn count_instances(xml: &str) -> Vec<(u32, u32)> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut plates: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    // (number, instances) of the open plate.
    let mut open: Option<(Option<u32>, u32)> = None;
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
                if opens {
                    depth += 1;
                }
                match element.local_name().as_ref() {
                    b"plate" if opens => open = Some((None, 0)),
                    b"model_instance" => {
                        in_instance = opens;
                        if let Some((_, count)) = &mut open {
                            *count += 1;
                        }
                    }
                    b"metadata" if !in_instance => {
                        let (mut key, mut value) = (None, None);
                        for attr in element.attributes().flatten() {
                            let Ok(text) = attr.decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder()) else { continue };
                            match attr.key.as_ref() {
                                b"key" => key = Some(text.into_owned()),
                                b"value" => value = Some(text.into_owned()),
                                _ => {}
                            }
                        }
                        if let (Some((number, _)), Some("plater_id"), Some(value)) = (&mut open, key.as_deref(), value) {
                            *number = value.parse::<u32>().ok().filter(|n| *n > 0);
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
                        if let Some((Some(number), count)) = open.take() {
                            if plates.len() < MAX_PLATES {
                                plates.entry(number).or_insert(count);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) if depth == 0 => return plates.into_iter().collect(),
            Ok(Event::Eof) | Err(_) => return Vec::new(),
            _ => {}
        }
    }
}

/// The entry's bytes when it starts with the PNG signature and fits both limits; nothing is decoded.
fn read_png<R: Read + Seek>(archive: &mut ZipArchive<R>, path: &str, budget: u64) -> Option<Vec<u8>> {
    let limit = MAX_PREVIEW_BYTES.min(budget);
    let file = archive.by_name(path).ok()?;
    if file.size() > limit {
        return None;
    }
    let mut data = Vec::new();
    // One byte more than allowed, so a size field that lies is noticed.
    file.take(limit + 1).read_to_end(&mut data).ok()?;
    (data.len() as u64 <= limit && data.starts_with(PNG_SIGNATURE)).then_some(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn png(extra: usize) -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend(std::iter::repeat_n(7u8, extra));
        bytes
    }

    fn archive(entries: &[(&str, Vec<u8>)]) -> ZipArchive<Cursor<Vec<u8>>> {
        let mut buf = Vec::new();
        {
            let mut writer = ZipWriter::new(Cursor::new(&mut buf));
            // Stored: the sizes in the test are the sizes on disk.
            let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            for (name, data) in entries {
                writer.start_file(*name, options).unwrap();
                writer.write_all(data).unwrap();
            }
            writer.finish().unwrap();
        }
        ZipArchive::new(Cursor::new(buf)).unwrap()
    }

    fn config(plates: &[(u32, usize)]) -> Vec<u8> {
        let mut xml = String::from("<config>");
        for (number, instances) in plates {
            xml.push_str(&format!(r#"<plate><metadata key="plater_id" value="{number}"/>"#));
            for _ in 0..*instances {
                xml.push_str(r#"<model_instance><metadata key="object_id" value="1"/></model_instance>"#);
            }
            xml.push_str("</plate>");
        }
        xml.push_str("</config>");
        xml.into_bytes()
    }

    fn summary(rows: &[PlateOverviewRow]) -> Vec<(u32, Option<u32>, bool)> {
        rows.iter().map(|r| (r.plate_index, r.object_count, r.preview.is_some())).collect()
    }

    #[test]
    fn counts_the_instances_of_each_plate_and_orders_by_number() {
        let mut a = archive(&[(MODEL_SETTINGS_PATH, config(&[(3, 1), (1, 2), (2, 0)]))]);
        assert_eq!(summary(&read_plate_overview(&mut a, false)), [(1, Some(2), false), (2, Some(0), false), (3, Some(1), false)]);
    }

    #[test]
    fn plates_without_a_valid_number_are_skipped_and_the_first_duplicate_wins() {
        let xml = br#"<config>
          <plate><metadata key="plater_id" value="1"/><model_instance object_id="5"/><model_instance object_id="6"/></plate>
          <plate><metadata key="plater_id" value="1"/><model_instance object_id="7"/></plate>
          <plate><metadata key="plater_id" value="0"/><model_instance object_id="8"/></plate>
          <plate><metadata key="plater_id" value="x"/></plate>
          <plate><model_instance object_id="9"/></plate>
        </config>"#;
        let mut a = archive(&[(MODEL_SETTINGS_PATH, xml.to_vec())]);
        assert_eq!(summary(&read_plate_overview(&mut a, false)), [(1, Some(2), false)]);
    }

    #[test]
    fn a_metadata_key_inside_an_instance_does_not_rename_the_plate() {
        let xml = br#"<config><plate><metadata key="plater_id" value="2"/>
          <model_instance><metadata key="plater_id" value="9"/></model_instance></plate></config>"#;
        let mut a = archive(&[(MODEL_SETTINGS_PATH, xml.to_vec())]);
        assert_eq!(summary(&read_plate_overview(&mut a, false)), [(2, Some(1), false)]);
    }

    #[test]
    fn no_settings_entry_or_broken_xml_gives_no_rows() {
        assert!(read_plate_overview(&mut archive(&[("other.txt", vec![1])]), true).is_empty());
        let broken = br#"<config><plate><metadata key="plater_id" value="1"/></config>"#.to_vec();
        assert!(read_plate_overview(&mut archive(&[(MODEL_SETTINGS_PATH, broken)]), true).is_empty());
    }

    #[test]
    fn an_oversized_settings_entry_is_refused_instead_of_read() {
        let oversized = vec![b'A'; (MAX_CONFIG_XML_BYTES + 1) as usize];
        assert!(read_plate_overview(&mut archive(&[(MODEL_SETTINGS_PATH, oversized)]), true).is_empty());
    }

    #[test]
    fn previews_are_only_read_when_asked_for() {
        let entries = [(MODEL_SETTINGS_PATH, config(&[(1, 1)])), ("Metadata/plate_1_small.png", png(10))];
        assert!(read_plate_overview(&mut archive(&entries), false)[0].preview.is_none());
        assert_eq!(read_plate_overview(&mut archive(&entries), true)[0].preview, Some(png(10)));
    }

    #[test]
    fn the_full_size_picture_stands_in_for_a_missing_small_one() {
        let entries = [(MODEL_SETTINGS_PATH, config(&[(1, 1), (2, 1)])), ("Metadata/plate_1.png", png(4)), ("Metadata/plate_2_small.png", png(5)), ("Metadata/plate_2.png", png(6))];
        let rows = read_plate_overview(&mut archive(&entries), true);
        assert_eq!(rows[0].preview, Some(png(4)));
        assert_eq!(rows[1].preview, Some(png(5)));
    }

    #[test]
    fn a_wrong_signature_or_an_oversized_picture_gives_no_picture() {
        let big = png(MAX_PREVIEW_BYTES as usize);
        let exact = png(MAX_PREVIEW_BYTES as usize - PNG_SIGNATURE.len());
        let entries = [
            (MODEL_SETTINGS_PATH, config(&[(1, 1), (2, 1), (3, 1)])),
            ("Metadata/plate_1_small.png", b"GIF89a-not-a-png".to_vec()),
            ("Metadata/plate_2_small.png", big),
            ("Metadata/plate_3_small.png", exact.clone()),
        ];
        let rows = read_plate_overview(&mut archive(&entries), true);
        assert_eq!(summary(&rows), [(1, Some(1), false), (2, Some(1), false), (3, Some(1), true)]);
        assert_eq!(rows[2].preview, Some(exact), "a picture of exactly the limit still passes");
    }

    #[test]
    fn a_path_stored_in_the_file_is_never_followed() {
        // The plate names a picture elsewhere in the archive; only the name built from the number counts.
        let xml = br#"<config><plate><metadata key="plater_id" value="1"/>
          <metadata key="thumbnail_file" value="../x.png"/><model_instance object_id="1"/></plate></config>"#;
        let entries = [(MODEL_SETTINGS_PATH, xml.to_vec()), ("../x.png", png(3)), ("Metadata/evil.png", png(3))];
        let rows = read_plate_overview(&mut archive(&entries), true);
        assert_eq!(summary(&rows), [(1, Some(1), false)]);
    }

    #[test]
    fn only_the_first_plates_get_a_picture_and_the_total_is_bounded() {
        let numbers: Vec<(u32, usize)> = (1..=70).map(|n| (n, 1)).collect();
        let mut entries = vec![(MODEL_SETTINGS_PATH.to_string(), config(&numbers))];
        for n in 1..=70 {
            entries.push((format!("Metadata/plate_{n}_small.png"), png(100)));
        }
        let refs: Vec<(&str, Vec<u8>)> = entries.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
        let rows = read_plate_overview(&mut archive(&refs), true);
        assert_eq!(rows.len(), 70);
        assert_eq!(rows.iter().filter(|r| r.preview.is_some()).count(), MAX_PREVIEW_PLATES);
        assert!(rows[MAX_PREVIEW_PLATES].preview.is_none());

        // 20 pictures of the largest allowed size: only as many as fit into the total.
        let full = png(MAX_PREVIEW_BYTES as usize - PNG_SIGNATURE.len());
        let mut entries = vec![(MODEL_SETTINGS_PATH.to_string(), config(&(1..=20).map(|n| (n, 1)).collect::<Vec<_>>()))];
        for n in 1..=20 {
            entries.push((format!("Metadata/plate_{n}_small.png"), full.clone()));
        }
        let refs: Vec<(&str, Vec<u8>)> = entries.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
        let rows = read_plate_overview(&mut archive(&refs), true);
        let delivered: u64 = rows.iter().filter_map(|r| r.preview.as_ref()).map(|p| p.len() as u64).sum();
        assert_eq!(rows.iter().filter(|r| r.preview.is_some()).count() as u64, MAX_PREVIEW_TOTAL_BYTES / MAX_PREVIEW_BYTES);
        assert!(delivered <= MAX_PREVIEW_TOTAL_BYTES);
        assert_eq!(rows.len(), 20, "the rows themselves are never dropped for the picture budget");
    }

    #[test]
    fn the_plate_count_is_bounded() {
        let numbers: Vec<(u32, usize)> = (1..=(MAX_PLATES as u32 + 5)).map(|n| (n, 1)).collect();
        let rows = read_plate_overview(&mut archive(&[(MODEL_SETTINGS_PATH, config(&numbers))]), false);
        assert_eq!(rows.len(), MAX_PLATES);
    }
}
