use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek};

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;
use zip::ZipArchive;

use super::error::ThreeMfError;
use super::model_xml::{parse_model_xml, Object, ParsedModel};

const RELS_PATH: &str = "_rels/.rels";
const MODEL_RELATIONSHIP_TYPE: &str =
    "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";
const THUMBNAIL_RELATIONSHIP_TYPE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail";
const DEFAULT_MODEL_PATH: &str = "3D/3dmodel.model";
const FALLBACK_THUMBNAIL_PATHS: [&str; 2] =
    ["Metadata/thumbnail.png", "3D/Thumbnails/thumbnail.png"];
// Limits against zip bombs (unpacked size per entry).
const MAX_MODEL_XML_BYTES: u64 = 256 * 1024 * 1024;
const MAX_THUMBNAIL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RELS_XML_BYTES: u64 = 16 * 1024 * 1024;
/// For `Metadata/model_settings.config` and `Metadata/slice_info.config`.
pub(super) const MAX_CONFIG_XML_BYTES: u64 = 16 * 1024 * 1024;
/// Limit for the sum of ALL unpacked resources, so many entries just below their
/// individual limit can't together use arbitrary memory.
const MAX_TOTAL_UNPACKED_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum number of referenced `.model` files (Production Extension), so many
/// small files neither bypass the budget nor make resolving take forever.
const MAX_REFERENCED_MODELS: usize = 32;

#[cfg(test)]
thread_local! {
    /// Lets tests use a small total budget without a 512 MB test file (thread-local).
    static TEST_MAX_TOTAL_UNPACKED_BYTES: std::cell::Cell<Option<u64>> =
        const { std::cell::Cell::new(None) };
}

fn max_total_unpacked_bytes() -> u64 {
    #[cfg(test)]
    {
        if let Some(max) = TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.get()) {
            return max;
        }
    }
    MAX_TOTAL_UNPACKED_BYTES
}

fn check_total_budget(total_unpacked: u64) -> Result<(), ThreeMfError> {
    let max = max_total_unpacked_bytes();
    if total_unpacked > max {
        return Err(ThreeMfError::ResourceLimitExceeded(format!(
            "Gesamtgroesse der entpackten Paket-Ressourcen ueberschreitet {max} Bytes"
        )));
    }
    Ok(())
}

pub struct PackageParts {
    pub root_model: ParsedModel,
    /// 3MF "Production Extension" files store extra objects in separate ZIP
    /// entries (referenced via p:path). The key is the normalized entry path
    /// (no leading '/').
    pub referenced_models: HashMap<String, ParsedModel>,
    pub thumbnail: Option<Vec<u8>>,
    pub plate_count: Option<u32>,
    pub slice_info: Option<super::slice_info::SliceInfo>,
}

impl PackageParts {
    /// Looks up an object by the file it was declared in (`None` = root model) and
    /// its local ID. Object IDs are only unique within a single file, hence the
    /// per-file lookup.
    pub fn lookup_object(&self, file: Option<&str>, object_id: &str) -> Option<&Object> {
        match file {
            None => self.root_model.objects.get(object_id),
            Some(path) => self.referenced_models.get(path)?.objects.get(object_id),
        }
    }
}

pub fn read_package<R: Read + Seek>(reader: R) -> Result<PackageParts, ThreeMfError> {
    let mut archive = ZipArchive::new(reader)?;

    // Counts EVERY resource read from the ZIP, including _rels/.rels and the slicer configs.
    let mut total_unpacked: u64 = 0;

    let (plate_count, plates_bytes) = super::plates::count_plates(&mut archive);
    total_unpacked += plates_bytes;
    check_total_budget(total_unpacked)?;

    let (slice_info, slice_info_bytes) = super::slice_info::parse_slice_info(&mut archive);
    total_unpacked += slice_info_bytes;
    check_total_budget(total_unpacked)?;

    let (model_path, thumbnail_path, rels_bytes) = resolve_relationships(&mut archive);
    total_unpacked += rels_bytes;
    check_total_budget(total_unpacked)?;
    let model_path = model_path.unwrap_or_else(|| DEFAULT_MODEL_PATH.to_string());

    let root_xml = read_entry_to_string(&mut archive, &model_path, MAX_MODEL_XML_BYTES)
        .or_else(|_| read_entry_to_string(&mut archive, DEFAULT_MODEL_PATH, MAX_MODEL_XML_BYTES))
        .map_err(|_| ThreeMfError::MissingRootModel)?;
    total_unpacked += root_xml.len() as u64;
    check_total_budget(total_unpacked)?;
    let root_model = parse_model_xml(&root_xml)?;

    let mut referenced_models: HashMap<String, ParsedModel> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = referenced_paths(&root_model);
    let mut referenced_count: usize = 0;

    // Resolve iteratively until a fixpoint instead of a single pass: every newly
    // read file can contain new, unknown p:path references itself (multi-stage
    // slicer output). `visited` prevents endless loops on circular references.
    while let Some(path) = queue.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        referenced_count += 1;
        if referenced_count > MAX_REFERENCED_MODELS {
            return Err(ThreeMfError::ResourceLimitExceeded(format!(
                "Zu viele referenzierte Modelldateien (> {MAX_REFERENCED_MODELS})"
            )));
        }
        // Missing or broken references are tolerated, an oversized entry is not:
        // otherwise the limit could be bypassed with references just too large, which
        // would then silently be missing.
        let xml = match read_entry_to_string(&mut archive, &path, MAX_MODEL_XML_BYTES) {
            Ok(xml) => xml,
            Err(err @ ThreeMfError::EntryTooLarge { .. }) => return Err(err),
            Err(_) => {
                eprintln!(
                    "[3mf] referenzierte Modell-Datei nicht gefunden, wird uebersprungen: {path}"
                );
                continue;
            }
        };
        total_unpacked += xml.len() as u64;
        check_total_budget(total_unpacked)?;
        let Ok(parsed) = parse_model_xml(&xml) else {
            eprintln!("[3mf] referenzierte Modell-Datei nicht parsbar, wird uebersprungen: {path}");
            continue;
        };
        for referenced in referenced_paths(&parsed) {
            if !visited.contains(&referenced) {
                queue.push(referenced);
            }
        }
        referenced_models.insert(path, parsed);
    }

    let mut thumbnail = thumbnail_path.and_then(|p| read_entry_to_bytes(&mut archive, &p, MAX_THUMBNAIL_BYTES).ok());
    if thumbnail.is_none() {
        for candidate in FALLBACK_THUMBNAIL_PATHS {
            if let Ok(bytes) = read_entry_to_bytes(&mut archive, candidate, MAX_THUMBNAIL_BYTES) {
                thumbnail = Some(bytes);
                break;
            }
        }
    }
    if let Some(thumb) = &thumbnail {
        total_unpacked += thumb.len() as u64;
        check_total_budget(total_unpacked)?;
    }

    Ok(PackageParts {
        root_model,
        referenced_models,
        plate_count,
        slice_info,
        thumbnail,
    })
}

/// Collects all p:path references of a parsed model file (from
/// `<build><item p:path=".."/>` as well as nested `<component p:path=".."/>`).
fn referenced_paths(model: &ParsedModel) -> Vec<String> {
    let mut paths = Vec::new();
    for item in &model.build_items {
        if let Some(path) = &item.path {
            paths.push(path.clone());
        }
    }
    for object in model.objects.values() {
        for component in &object.components {
            if let Some(path) = &component.path {
                paths.push(path.clone());
            }
        }
    }
    paths
}

fn resolve_relationships<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> (Option<String>, Option<String>, u64) {
    let Ok(rels_xml) = read_entry_to_string(archive, RELS_PATH, MAX_RELS_XML_BYTES) else {
        return (None, None, 0);
    };
    let rels_bytes = rels_xml.len() as u64;

    let mut model_path = None;
    let mut thumbnail_path = None;

    let mut reader = Reader::from_str(&rels_xml);
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == b"Relationship" => {
                let mut rel_type = None;
                let mut target = None;
                for attr in e.attributes().flatten() {
                    match attr.key.as_ref() {
                        b"Type" => rel_type = attr.normalized_value(XmlVersion::Implicit1_0).ok().map(|v| v.into_owned()),
                        b"Target" => target = attr.normalized_value(XmlVersion::Implicit1_0).ok().map(|v| v.into_owned()),
                        _ => {}
                    }
                }
                if let (Some(rel_type), Some(target)) = (rel_type, target) {
                    let normalized = target.trim_start_matches('/').to_string();
                    if rel_type == MODEL_RELATIONSHIP_TYPE {
                        model_path = Some(normalized);
                    } else if rel_type == THUMBNAIL_RELATIONSHIP_TYPE {
                        thumbnail_path = Some(normalized);
                    }
                }
            }
            _ => {}
        }
    }

    (model_path, thumbnail_path, rels_bytes)
}

pub(super) fn read_entry_to_string<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
    max_bytes: u64,
) -> Result<String, ThreeMfError> {
    let file = archive.by_name(path)?;
    reject_oversized_entry(path, file.size(), max_bytes)?;
    let mut contents = String::new();
    file.take(max_bytes).read_to_string(&mut contents)?;
    Ok(contents)
}

fn read_entry_to_bytes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, ThreeMfError> {
    let file = archive.by_name(path)?;
    reject_oversized_entry(path, file.size(), max_bytes)?;
    let mut contents = Vec::new();
    file.take(max_bytes).read_to_end(&mut contents)?;
    Ok(contents)
}

/// The declared size isn't trustworthy, the real barrier is `Read::take` while
/// reading. This check gives honestly oversized entries a clear error message
/// instead of a truncated file.
fn reject_oversized_entry(path: &str, size: u64, max_bytes: u64) -> Result<(), ThreeMfError> {
    if size > max_bytes {
        return Err(ThreeMfError::EntryTooLarge {
            path: path.to_string(),
            size,
            max: max_bytes,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    const ROOT_MODEL_XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06">
  <build>
    <item p:path="/3D/Objects/object_1.model" objectid="1"/>
  </build>
</model>"##;

    const CHILD_MODEL_XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <resources>
    <object id="1" type="model">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0"/>
          <vertex x="1" y="0" z="0"/>
          <vertex x="0" y="1" z="0"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="1" v3="2"/>
        </triangles>
      </mesh>
    </object>
  </resources>
</model>"##;

    const CHILD_WITH_NESTED_REF_XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06">
  <resources>
    <object id="1" type="model">
      <components>
        <component p:path="/3D/Objects/object_2.model" objectid="9"/>
      </components>
    </object>
  </resources>
</model>"##;

    const GRANDCHILD_MODEL_XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <resources>
    <object id="9" type="model">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0"/>
          <vertex x="1" y="0" z="0"/>
          <vertex x="0" y="1" z="0"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="1" v3="2"/>
        </triangles>
      </mesh>
    </object>
  </resources>
</model>"##;

    fn build_multi_file_zip(extra_files: &[(&str, &str)]) -> Vec<u8> {
        let rels_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/>
</Relationships>"#;

        let content_types_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
</Types>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();

            zip.start_file("[Content_Types].xml", options).unwrap();
            zip.write_all(content_types_xml.as_bytes()).unwrap();

            zip.start_file("_rels/.rels", options).unwrap();
            zip.write_all(rels_xml.as_bytes()).unwrap();

            zip.start_file("3D/3dmodel.model", options).unwrap();
            zip.write_all(ROOT_MODEL_XML.as_bytes()).unwrap();

            for (path, xml) in extra_files {
                zip.start_file(*path, options).unwrap();
                zip.write_all(xml.as_bytes()).unwrap();
            }

            zip.finish().unwrap();
        }
        buf
    }

    /// ZIP with one highly compressible entry (mini zip bomb).
    fn build_zip_with_one_entry(path: &str, uncompressed_len: usize) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(path, SimpleFileOptions::default()).unwrap();
            zip.write_all(&vec![b'A'; uncompressed_len]).unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn read_entry_rejects_an_entry_exceeding_the_size_limit() {
        let bytes = build_zip_with_one_entry("big.model", 4096);
        let mut archive = ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();

        let too_big = read_entry_to_string(&mut archive, "big.model", 1024);
        assert!(
            matches!(too_big, Err(ThreeMfError::EntryTooLarge { .. })),
            "an entry above the cap must be rejected, got {too_big:?}"
        );

        let bytes_too_big = read_entry_to_bytes(&mut archive, "big.model", 1024);
        assert!(matches!(bytes_too_big, Err(ThreeMfError::EntryTooLarge { .. })));

        // Below the limit everything stays as before.
        let ok = read_entry_to_bytes(&mut archive, "big.model", 8192).expect("within the cap");
        assert_eq!(ok.len(), 4096);
    }

    #[test]
    fn resolves_a_single_p_path_reference() {
        let bytes = build_multi_file_zip(&[("3D/Objects/object_1.model", CHILD_MODEL_XML)]);
        let package = read_package(std::io::Cursor::new(bytes)).expect("read should succeed");

        assert!(package
            .referenced_models
            .contains_key("3D/Objects/object_1.model"));
        let child = &package.referenced_models["3D/Objects/object_1.model"];
        assert!(child.objects.contains_key("1"));
    }

    #[test]
    fn skips_missing_referenced_file_without_failing() {
        let bytes = build_multi_file_zip(&[]);
        let package =
            read_package(std::io::Cursor::new(bytes)).expect("read should still succeed");

        assert!(package.referenced_models.is_empty());
        assert_eq!(package.root_model.build_items.len(), 1);
    }

    #[test]
    fn resolves_transitive_p_path_references_to_a_fixpoint() {
        let bytes = build_multi_file_zip(&[
            ("3D/Objects/object_1.model", CHILD_WITH_NESTED_REF_XML),
            ("3D/Objects/object_2.model", GRANDCHILD_MODEL_XML),
        ]);
        let package = read_package(std::io::Cursor::new(bytes)).expect("read should succeed");

        assert!(package
            .referenced_models
            .contains_key("3D/Objects/object_1.model"));
        assert!(package
            .referenced_models
            .contains_key("3D/Objects/object_2.model"));
    }

    /// 3MF with `count` tiny referenced models that together exceed a total budget
    /// shrunk for the test.
    fn build_multi_model_3mf_exceeding_total_budget(count: usize) -> Vec<u8> {
        let root_items: String = (0..count)
            .map(|i| format!(r#"<item p:path="/3D/Objects/object_{i}.model" objectid="1"/>"#))
            .collect::<Vec<_>>()
            .join("\n");
        let root_xml = format!(
            r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06">
  <build>
    {root_items}
  </build>
</model>"##
        );

        let extra_files: Vec<(String, String)> = (0..count)
            .map(|i| (format!("3D/Objects/object_{i}.model"), CHILD_MODEL_XML.to_string()))
            .collect();
        let extra_files_ref: Vec<(&str, &str)> = extra_files
            .iter()
            .map(|(p, x)| (p.as_str(), x.as_str()))
            .collect();

        build_multi_file_zip_with_root(&root_xml, &extra_files_ref)
    }

    /// Like `build_multi_file_zip`, but allows a different root model XML (the fixed
    /// `ROOT_MODEL_XML` constant only covers exactly one referenced file).
    fn build_multi_file_zip_with_root(root_xml: &str, extra_files: &[(&str, &str)]) -> Vec<u8> {
        let rels_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/>
</Relationships>"#;

        let content_types_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
</Types>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();

            zip.start_file("[Content_Types].xml", options).unwrap();
            zip.write_all(content_types_xml.as_bytes()).unwrap();

            zip.start_file("_rels/.rels", options).unwrap();
            zip.write_all(rels_xml.as_bytes()).unwrap();

            zip.start_file("3D/3dmodel.model", options).unwrap();
            zip.write_all(root_xml.as_bytes()).unwrap();

            for (path, xml) in extra_files {
                zip.start_file(*path, options).unwrap();
                zip.write_all(xml.as_bytes()).unwrap();
            }

            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn read_package_rejects_many_referenced_models_that_together_exceed_the_total_budget() {
        // Each model is far below the per-entry limit, the sum above 2000 bytes.
        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(Some(2000)));
        let bytes = build_multi_model_3mf_exceeding_total_budget(10);
        let result = read_package(std::io::Cursor::new(bytes));
        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(None));

        assert!(
            result.is_err(),
            "die Summe aller entpackten Ressourcen ueberschreitet das Gesamtbudget, das muss abgelehnt werden"
        );
    }

    #[test]
    fn read_package_still_accepts_a_legitimate_multi_part_3mf_within_budget() {
        let bytes = build_multi_file_zip(&[("3D/Objects/object_1.model", CHILD_MODEL_XML)]);
        assert!(read_package(std::io::Cursor::new(bytes)).is_ok());
    }

    const MINIMAL_ROOT_MODEL_XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <build></build>
</model>"##;

    const SMALL_RELS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/>
</Relationships>"#;

    /// Minimal 3MF with a custom `_rels/.rels` and extra entries, to inflate exactly
    /// one kind of resource.
    fn build_minimal_3mf_with_entries(rels_xml: &str, extra_entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();

            zip.start_file("_rels/.rels", options).unwrap();
            zip.write_all(rels_xml.as_bytes()).unwrap();

            zip.start_file("3D/3dmodel.model", options).unwrap();
            zip.write_all(MINIMAL_ROOT_MODEL_XML.as_bytes()).unwrap();

            for (path, content) in extra_entries {
                zip.start_file(*path, options).unwrap();
                zip.write_all(content.as_bytes()).unwrap();
            }

            zip.finish().unwrap();
        }
        buf
    }

    /// A `_rels/.rels` below its own limit but above the total budget must be
    /// counted and rejected.
    #[test]
    fn read_package_rejects_when_rels_alone_pushes_total_over_budget() {
        let padded_rels = format!("<!--{}-->{SMALL_RELS_XML}", "A".repeat(5000));
        assert!((padded_rels.len() as u64) < MAX_RELS_XML_BYTES);

        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(Some(1000)));
        let bytes = build_minimal_3mf_with_entries(&padded_rels, &[]);
        let result = read_package(std::io::Cursor::new(bytes));
        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(None));

        assert!(
            result.is_err(),
            "ein ueberdimensionierter _rels/.rels-Eintrag allein muss das Gesamtbudget ueberschreiten koennen"
        );
    }

    /// Same as above, but for `Metadata/model_settings.config` (`count_plates()`).
    #[test]
    fn read_package_rejects_when_model_settings_config_alone_pushes_total_over_budget() {
        let padded_config = "A".repeat(5000);
        assert!((padded_config.len() as u64) < MAX_CONFIG_XML_BYTES);

        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(Some(1000)));
        let bytes = build_minimal_3mf_with_entries(
            SMALL_RELS_XML,
            &[("Metadata/model_settings.config", &padded_config)],
        );
        let result = read_package(std::io::Cursor::new(bytes));
        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(None));

        assert!(
            result.is_err(),
            "ein ueberdimensionierter model_settings.config-Eintrag allein muss das Gesamtbudget ueberschreiten koennen"
        );
    }

    /// Same as above, but for `Metadata/slice_info.config` (`parse_slice_info()`).
    #[test]
    fn read_package_rejects_when_slice_info_config_alone_pushes_total_over_budget() {
        let padded_config = "A".repeat(5000);
        assert!((padded_config.len() as u64) < MAX_CONFIG_XML_BYTES);

        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(Some(1000)));
        let bytes = build_minimal_3mf_with_entries(
            SMALL_RELS_XML,
            &[("Metadata/slice_info.config", &padded_config)],
        );
        let result = read_package(std::io::Cursor::new(bytes));
        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(None));

        assert!(
            result.is_err(),
            "ein ueberdimensionierter slice_info.config-Eintrag allein muss das Gesamtbudget ueberschreiten koennen"
        );
    }

    /// Counter-check: without padding the same package is accepted.
    #[test]
    fn read_package_accepts_a_tiny_package_under_the_same_small_test_budget() {
        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(Some(1000)));
        let bytes = build_minimal_3mf_with_entries(SMALL_RELS_XML, &[]);
        let result = read_package(std::io::Cursor::new(bytes));
        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(None));

        assert!(result.is_ok(), "ein winziges Paket muss unter dem Testbudget akzeptiert werden");
    }

    /// 3MF with an existing referenced model 1 byte over `MAX_MODEL_XML_BYTES`
    /// (`Stored`, so writing it is fast).
    fn build_3mf_with_oversized_referenced_model() -> Vec<u8> {
        let root_xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06">
  <build>
    <item p:path="/3D/Objects/big.model" objectid="1"/>
  </build>
</model>"##;

        let oversized_len = (MAX_MODEL_XML_BYTES + 1) as usize;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();
            let stored = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            zip.start_file("_rels/.rels", options).unwrap();
            zip.write_all(SMALL_RELS_XML.as_bytes()).unwrap();

            zip.start_file("3D/3dmodel.model", options).unwrap();
            zip.write_all(root_xml.as_bytes()).unwrap();

            zip.start_file("3D/Objects/big.model", stored).unwrap();
            zip.write_all(&vec![b'A'; oversized_len]).unwrap();

            zip.finish().unwrap();
        }
        buf
    }

    /// An existing but oversized reference must fail with `EntryTooLarge` instead of being skipped.
    #[test]
    fn read_package_hard_errors_on_an_oversized_referenced_model_instead_of_skipping_it() {
        let bytes = build_3mf_with_oversized_referenced_model();
        let result = read_package(std::io::Cursor::new(bytes));

        assert!(
            matches!(result, Err(ThreeMfError::EntryTooLarge { .. })),
            "ein referenziertes Modell ueber dem Pro-Entry-Limit muss als EntryTooLarge hart fehlschlagen"
        );
    }

    #[test]
    fn lookup_object_resolves_root_and_referenced_files() {
        let bytes = build_multi_file_zip(&[("3D/Objects/object_1.model", CHILD_MODEL_XML)]);
        let package = read_package(std::io::Cursor::new(bytes)).expect("read should succeed");

        assert!(package
            .lookup_object(Some("3D/Objects/object_1.model"), "1")
            .is_some());
        assert!(package.lookup_object(None, "1").is_none());
        assert!(package
            .lookup_object(Some("does/not/exist.model"), "1")
            .is_none());
    }
}
