use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Read, Seek};

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;
use zip::ZipArchive;

use super::error::ThreeMfError;
use super::model_xml::{parse_model_xml_reader, parse_model_xml_reader_with_preview, Object, ParsedModel};

const RELS_PATH: &str = "_rels/.rels";
const MODEL_RELATIONSHIP_TYPE: &str =
    "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";
const THUMBNAIL_RELATIONSHIP_TYPE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail";
const DEFAULT_MODEL_PATH: &str = "3D/3dmodel.model";
const FALLBACK_THUMBNAIL_PATHS: [&str; 2] =
    ["Metadata/thumbnail.png", "3D/Thumbnails/thumbnail.png"];
// Limits against zip bombs (unpacked size per entry).
// High-resolution slicer projects need more room; streaming enforces the cap without retaining XML.
const MAX_MODEL_XML_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_THUMBNAIL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RELS_XML_BYTES: u64 = 16 * 1024 * 1024;
/// For `Metadata/model_settings.config` and `Metadata/slice_info.config`.
pub(super) const MAX_CONFIG_XML_BYTES: u64 = 16 * 1024 * 1024;
/// Limit for the sum of ALL unpacked resources, so many entries just below their
/// individual limit can't together use arbitrary memory.
const MAX_TOTAL_UNPACKED_BYTES: u64 = 3 * 1024 * 1024 * 1024;
/// Maximum number of referenced `.model` files (Production Extension), so many
/// small files neither bypass the budget nor make resolving take forever.
/// Slicer projects with many parts keep one file per object: a real catalog
/// holds projects with up to 49 of them, so the limit leaves room above that.
const MAX_REFERENCED_MODELS: usize = 128;

#[cfg(test)]
thread_local! {
    /// Lets tests use a small total budget without a multi-gigabyte test file (thread-local).
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
    pub plate_assignments: super::plates::PlateAssignments,
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
    read_package_inner(reader, false, None)
}

/// Geometry needs the original limit error; metadata import retains its fallback.
pub fn read_geometry_package<R: Read + Seek>(reader: R) -> Result<PackageParts, ThreeMfError> {
    read_package_inner(reader, true, None)
}

pub(crate) fn read_web_package<R: Read + Seek>(reader: R, limits: crate::import::files::WebImportLimits) -> Result<PackageParts, ThreeMfError> {
    read_package_inner(reader, false, Some(limits))
}

fn read_package_inner<R: Read + Seek>(reader: R, geometry: bool, web: Option<crate::import::files::WebImportLimits>) -> Result<PackageParts, ThreeMfError> {
    let mut archive = ZipArchive::new(reader)?;

    // Validate actual decompression, including unused entries and lying ZIP size fields,
    // before parsing. This streams to a sink and never retains the unpacked archive.
    if let Some(limits) = web {
        let mut remaining = limits.max_unpacked_bytes;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            if entry.size() > remaining { return Err(ThreeMfError::ResourceLimitExceeded("unpacked limit".into())); }
            let count = std::io::copy(&mut (&mut entry).take(remaining.saturating_add(1)), &mut std::io::sink())?;
            remaining = remaining.checked_sub(count).ok_or_else(|| ThreeMfError::ResourceLimitExceeded("unpacked limit".into()))?;
        }
    }
    let mut mesh_remaining = web.map(|limits| (limits.max_vertices, limits.max_triangles));

    // Counts EVERY resource read from the ZIP, including _rels/.rels and the slicer configs.
    let mut total_unpacked: u64 = 0;

    let (plate_count, plates_bytes, plate_assignments) = super::plates::read_plate_metadata(&mut archive);
    total_unpacked += plates_bytes;
    check_total_budget(total_unpacked)?;

    let (slice_info, slice_info_bytes) = super::slice_info::parse_slice_info(&mut archive);
    total_unpacked += slice_info_bytes;
    check_total_budget(total_unpacked)?;

    let (model_path, thumbnail_path, rels_bytes) = resolve_relationships(&mut archive);
    total_unpacked += rels_bytes;
    check_total_budget(total_unpacked)?;
    let model_path = model_path.unwrap_or_else(|| DEFAULT_MODEL_PATH.to_string());

    let root_model = match read_model_entry_limited(&mut archive, &model_path, MAX_MODEL_XML_BYTES, &mut total_unpacked, geometry, &mut mesh_remaining) {
        Ok(model) => model,
        Err(err @ (ThreeMfError::Xml(_) | ThreeMfError::InvalidGeometry(_) | ThreeMfError::InvalidTransform(_) | ThreeMfError::ResourceLimitExceeded(_))) => return Err(err),
        Err(err @ ThreeMfError::EntryTooLarge { .. }) if geometry => return Err(err),
        Err(_) => match read_model_entry_limited(&mut archive, DEFAULT_MODEL_PATH, MAX_MODEL_XML_BYTES, &mut total_unpacked, geometry, &mut mesh_remaining) {
            Ok(model) => model,
            Err(err @ (ThreeMfError::Xml(_) | ThreeMfError::InvalidGeometry(_) | ThreeMfError::InvalidTransform(_) | ThreeMfError::ResourceLimitExceeded(_))) => return Err(err),
            Err(err @ ThreeMfError::EntryTooLarge { .. }) if geometry => return Err(err),
            Err(_) => return Err(ThreeMfError::MissingRootModel),
        },
    };

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
        let parsed = match read_model_entry_limited(&mut archive, &path, MAX_MODEL_XML_BYTES, &mut total_unpacked, geometry, &mut mesh_remaining) {
            Ok(model) => model,
            Err(err @ (ThreeMfError::EntryTooLarge { .. } | ThreeMfError::ResourceLimitExceeded(_))) => return Err(err),
            Err(_) => {
                if web.is_none() { log::warn!(target: "3mf", "referenced model file missing or not parseable, skipped: {path}"); }
                continue;
            }
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
        plate_assignments,
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

// Count decompressed bytes before the XML parser can allocate for them. One
// lookahead byte distinguishes a full entry from a silently truncated one.
struct ModelReader<'a, R> {
    inner: R,
    read: u64,
    max: u64,
    total: &'a mut u64,
    exceeded_entry: bool,
    exceeded_total: bool,
}

impl<R: Read> Read for ModelReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let remaining = self.max.saturating_sub(self.read).min(max_total_unpacked_bytes().saturating_sub(*self.total));
        let len = buf.len().min(remaining.saturating_add(1) as usize);
        let n = self.inner.read(&mut buf[..len])?;
        self.read += n as u64;
        *self.total += n as u64;
        self.exceeded_entry = self.read > self.max;
        self.exceeded_total = *self.total > max_total_unpacked_bytes();
        if self.exceeded_entry || self.exceeded_total {
            return Err(std::io::Error::other("model resource limit exceeded"));
        }
        Ok(n)
    }
}

#[cfg(test)]
fn read_model_entry<R: Read + Seek>(archive: &mut ZipArchive<R>, path: &str, max: u64, total: &mut u64, preview: bool) -> Result<ParsedModel, ThreeMfError> {
    read_model_entry_limited(archive, path, max, total, preview, &mut None)
}

fn read_model_entry_limited<R: Read + Seek>(
    archive: &mut ZipArchive<R>, path: &str, max: u64, total: &mut u64, preview: bool, mesh_remaining: &mut Option<(usize, usize)>,
) -> Result<ParsedModel, ThreeMfError> {
    let file = archive.by_name(path)?;
    reject_oversized_entry(path, file.size(), max)?;
    let mut source = ModelReader { inner: file, read: 0, max, total, exceeded_entry: false, exceeded_total: false };
    let result = if let Some(limits) = mesh_remaining {
        super::model_xml::parse_model_xml_reader_with_limits(BufReader::new(&mut source), *limits)
    } else if preview {
        parse_model_xml_reader_with_preview(BufReader::new(&mut source), true)
    } else {
        parse_model_xml_reader(BufReader::new(&mut source))
    };
    let result = match result {
        // Preserve ZIP read failures as I/O errors for the root fallback.
        Err(ThreeMfError::Xml(quick_xml::Error::Io(error))) =>
            Err(ThreeMfError::Io(std::io::Error::new(error.kind(), error.to_string()))),
        Err(error @ (ThreeMfError::Xml(_) | ThreeMfError::InvalidGeometry(_) | ThreeMfError::InvalidTransform(_))) => {
            // Broken references are tolerated, but their unread tail must not
            // bypass byte limits or ZIP integrity checks.
            match std::io::copy(&mut source, &mut std::io::sink()) {
                Ok(_) => Err(error),
                Err(error) => Err(ThreeMfError::Io(error)),
            }
        }
        other => other,
    };
    if source.exceeded_entry {
        return Err(ThreeMfError::EntryTooLarge { path: path.into(), size: source.read, max });
    }
    check_total_budget(*source.total)?;
    if let (Ok(model), Some(remaining)) = (&result, mesh_remaining) {
        for object in model.objects.values() {
            if let Some(mesh) = &object.mesh {
                remaining.0 = remaining.0.saturating_sub(mesh.vertices.len());
                remaining.1 = remaining.1.saturating_sub(mesh.triangles.len());
            }
        }
    }
    result
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

    fn build_zip_with_referenced_models(count: usize) -> Vec<u8> {
        let items: String = (1..=count)
            .map(|i| format!(r#"<item p:path="/3D/Objects/object_{i}.model" objectid="{i}"/>"#))
            .collect();
        let root = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06"><build>{items}</build></model>"#
        );
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();
            zip.start_file("3D/3dmodel.model", options).unwrap();
            zip.write_all(root.as_bytes()).unwrap();
            for i in 1..=count {
                zip.start_file(format!("3D/Objects/object_{i}.model"), options).unwrap();
                zip.write_all(CHILD_MODEL_XML.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn projects_with_many_part_files_load_up_to_the_limit() {
        let package = read_geometry_package(std::io::Cursor::new(build_zip_with_referenced_models(60))).unwrap();
        assert_eq!(package.referenced_models.len(), 60);
        let too_many = build_zip_with_referenced_models(MAX_REFERENCED_MODELS + 1);
        assert!(matches!(
            read_geometry_package(std::io::Cursor::new(too_many)),
            Err(ThreeMfError::ResourceLimitExceeded(_))
        ));
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
    fn oversized_root_model_keeps_its_size_error() {
        let mut bytes = build_zip_with_one_entry(DEFAULT_MODEL_PATH, 4);
        // Honest size rejection happens before decompression; no large allocation
        // is necessary to exercise the root-model fallback.
        let central = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        let local = bytes.windows(4).position(|w| w == b"PK\x03\x04").unwrap();
        let size = ((MAX_MODEL_XML_BYTES + 1) as u32).to_le_bytes();
        bytes[central + 24..central + 28].copy_from_slice(&size);
        bytes[local + 22..local + 26].copy_from_slice(&size);
        assert!(matches!(read_geometry_package(std::io::Cursor::new(&bytes)), Err(ThreeMfError::EntryTooLarge { .. })));
        assert!(matches!(read_package(std::io::Cursor::new(bytes)), Err(ThreeMfError::MissingRootModel)));
    }

    #[test]
    fn streaming_checks_remainder_after_malformed_xml() {
        let xml = format!("<model></broken>{}", " ".repeat(50_000));
        let mut bytes = build_multi_file_zip(&[("3D/Objects/object_1.model", &xml)]);
        let central = bytes.windows(4).rposition(|w| w == b"PK\x01\x02").unwrap();
        bytes[central + 24..central + 28].copy_from_slice(&4_u32.to_le_bytes());
        let mut archive = ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut total = 0;
        assert!(matches!(read_model_entry(&mut archive, "3D/Objects/object_1.model", 16_000, &mut total, false),
            Err(ThreeMfError::EntryTooLarge { .. })));
    }

    #[test]
    fn streaming_preserves_root_fallback_for_zip_read_errors() {
        let mut bytes = build_minimal_3mf_with_entries(SMALL_RELS_XML, &[]);
        let central = bytes.windows(4).rposition(|w| w == b"PK\x01\x02").unwrap();
        bytes[central + 16..central + 20].copy_from_slice(&0_u32.to_le_bytes());
        assert!(matches!(read_package(std::io::Cursor::new(bytes)), Err(ThreeMfError::MissingRootModel)));
    }

    #[test]
    fn streaming_total_budget_counts_actual_bytes_across_entries() {
        let bytes = build_multi_file_zip(&[("3D/Objects/object_1.model", CHILD_MODEL_XML)]);
        let mut archive = ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut total = 0;
        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(Some((ROOT_MODEL_XML.len() + CHILD_MODEL_XML.len() - 1) as u64)));
        let root = read_model_entry(&mut archive, DEFAULT_MODEL_PATH, MAX_MODEL_XML_BYTES, &mut total, false);
        let child = read_model_entry(&mut archive, "3D/Objects/object_1.model", MAX_MODEL_XML_BYTES, &mut total, false);
        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(None));
        assert!(root.is_ok());
        assert!(matches!(child, Err(ThreeMfError::ResourceLimitExceeded(_))));
    }

    #[test]
    fn streaming_entry_rejects_a_dishonest_header() {
        let mut bytes = build_zip_with_one_entry(DEFAULT_MODEL_PATH, 4096);
        let central = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        let local = bytes.windows(4).position(|w| w == b"PK\x03\x04").unwrap();
        bytes[central + 24..central + 28].copy_from_slice(&4_u32.to_le_bytes());
        bytes[local + 22..local + 26].copy_from_slice(&4_u32.to_le_bytes());
        let mut archive = ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut total = 0;
        assert!(matches!(read_model_entry(&mut archive, DEFAULT_MODEL_PATH, 1024, &mut total, false),
            Err(ThreeMfError::EntryTooLarge { .. })));
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

    /// Declare an oversized reference without allocating gigabytes in the test.
    fn build_3mf_with_oversized_referenced_model() -> Vec<u8> {
        let root_xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06">
  <build>
    <item p:path="/3D/Objects/big.model" objectid="1"/>
  </build>
</model>"##;

        let oversized_len = 4;

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
        let central = buf.windows(4).rposition(|w| w == b"PK\x01\x02").unwrap();
        buf[central + 24..central + 28].copy_from_slice(&((MAX_MODEL_XML_BYTES + 1) as u32).to_le_bytes());
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
