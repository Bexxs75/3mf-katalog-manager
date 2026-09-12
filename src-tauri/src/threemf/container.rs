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

pub struct PackageParts {
    pub root_model: ParsedModel,
    /// 3MF-"Production-Extension"-Dateien lagern zusaetzliche Objekte in
    /// separaten ZIP-Eintraegen aus (referenziert via p:path). Schluessel
    /// ist der normalisierte (kein fuehrendes '/') Eintragspfad.
    pub referenced_models: HashMap<String, ParsedModel>,
    pub thumbnail: Option<Vec<u8>>,
    pub plate_count: Option<u32>,
}

impl PackageParts {
    /// Sucht ein Objekt anhand der Datei, in der es deklariert wurde
    /// (`None` = Root-Modell) und seiner lokalen ID. Objekt-IDs sind nur
    /// innerhalb einer einzelnen Datei eindeutig, daher die dateibezogene
    /// Suche.
    pub fn lookup_object(&self, file: Option<&str>, object_id: &str) -> Option<&Object> {
        match file {
            None => self.root_model.objects.get(object_id),
            Some(path) => self.referenced_models.get(path)?.objects.get(object_id),
        }
    }
}

pub fn read_package<R: Read + Seek>(reader: R) -> Result<PackageParts, ThreeMfError> {
    let mut archive = ZipArchive::new(reader)?;

    let plate_count = super::plates::count_plates(&mut archive);

    let (model_path, thumbnail_path) = resolve_relationships(&mut archive);
    let model_path = model_path.unwrap_or_else(|| DEFAULT_MODEL_PATH.to_string());

    let root_xml = read_entry_to_string(&mut archive, &model_path)
        .or_else(|_| read_entry_to_string(&mut archive, DEFAULT_MODEL_PATH))
        .map_err(|_| ThreeMfError::MissingRootModel)?;
    let root_model = parse_model_xml(&root_xml)?;

    let mut referenced_models: HashMap<String, ParsedModel> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = referenced_paths(&root_model);

    // Iterative Aufloesung bis zum Fixpunkt statt eines einzelnen
    // Durchlaufs: jede neu gelesene Datei kann selbst wieder neue,
    // noch unbekannte p:path-Referenzen enthalten (mehrstufige
    // Slicer-Ausgaben). `visited` verhindert Endlosschleifen bei
    // zirkulaeren Referenzen.
    while let Some(path) = queue.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        let Ok(xml) = read_entry_to_string(&mut archive, &path) else {
            eprintln!(
                "[3mf] referenzierte Modell-Datei nicht gefunden, wird uebersprungen: {path}"
            );
            continue;
        };
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

    let mut thumbnail = thumbnail_path.and_then(|p| read_entry_to_bytes(&mut archive, &p).ok());
    if thumbnail.is_none() {
        for candidate in FALLBACK_THUMBNAIL_PATHS {
            if let Ok(bytes) = read_entry_to_bytes(&mut archive, candidate) {
                thumbnail = Some(bytes);
                break;
            }
        }
    }

    Ok(PackageParts {
        root_model,
        referenced_models,
        plate_count,
        thumbnail,
    })
}

/// Sammelt alle p:path-Referenzen einer geparsten Modell-Datei (sowohl aus
/// `<build><item p:path=".."/>` als auch aus verschachtelten `<component
/// p:path=".."/>`-Elementen).
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
) -> (Option<String>, Option<String>) {
    let Ok(rels_xml) = read_entry_to_string(archive, RELS_PATH) else {
        return (None, None);
    };

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

    (model_path, thumbnail_path)
}

fn read_entry_to_string<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Result<String, ThreeMfError> {
    let mut file = archive.by_name(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

fn read_entry_to_bytes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Result<Vec<u8>, ThreeMfError> {
    let mut file = archive.by_name(path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    Ok(contents)
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
