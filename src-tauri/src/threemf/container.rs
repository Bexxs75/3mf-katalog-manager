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
// Obergrenzen fuer die entpackte Groesse einzelner Paket-Eintraege: ein
// wenige Kilobyte grosses 3MF kann sich sonst beim Entpacken auf mehrere
// Gigabyte aufblaehen und die App per OOM beenden ("Zip-Bombe",
// Security-Review 2026-09-19, Finding A-1).
const MAX_MODEL_XML_BYTES: u64 = 256 * 1024 * 1024; // 256 MB
const MAX_THUMBNAIL_BYTES: u64 = 16 * 1024 * 1024; // 16 MB
const MAX_RELS_XML_BYTES: u64 = 16 * 1024 * 1024; // 16 MB
/// Gilt fuer die slicer-spezifischen Config-Eintraege
/// (`Metadata/model_settings.config`, `Metadata/slice_info.config`), die
/// `plates.rs`/`slice_info.rs` lesen - dieselbe Groessenordnung wie die
/// uebrigen Nicht-Modell-Eintraege.
pub(super) const MAX_CONFIG_XML_BYTES: u64 = 16 * 1024 * 1024; // 16 MB
/// Obergrenze fuer die Summe ALLER tatsaechlich aus dem ZIP entpackten
/// Paket-Ressourcen (_rels/.rels, die beiden Slicer-Config-Eintraege,
/// Root-Modell-XML, referenzierte Modell-XML-Dateien und Thumbnail) - ein
/// wenige Kilobyte grosses 3MF darf sich in Summe nicht zu einem beliebig
/// grossen Speicherabdruck aufblaehen, selbst wenn jede einzelne Ressource
/// unter ihrem jeweiligen Pro-Entry-Limit bleibt (M-05, Senior-Code-Review
/// 2026-09-19, korrigiert in der fuenften Review-Runde: die Fassung nach
/// der zweiten Runde zaehlte nur root_xml + referenzierte Modell-XMLs +
/// Thumbnail, nicht aber _rels/.rels und die beiden Slicer-Configs).
const MAX_TOTAL_UNPACKED_BYTES: u64 = 512 * 1024 * 1024; // 512 MB
/// Obergrenze fuer die Anzahl referenzierter `.model`-Dateien (3MF
/// "Production Extension"), unabhaengig davon, dass jede einzelne Datei
/// unter `MAX_MODEL_XML_BYTES` bleibt - verhindert, dass sehr viele kleine
/// referenzierte Modelle zusammen das Gesamtbudget umgehen bzw. die
/// Aufloesung unzumutbar lange dauert.
const MAX_REFERENCED_MODELS: usize = 32;

#[cfg(test)]
thread_local! {
    /// Erlaubt Tests, `MAX_TOTAL_UNPACKED_BYTES` fuer die Dauer eines
    /// einzelnen Tests durch einen kleinen Wert zu ersetzen, ohne eine
    /// tatsaechlich 512-MB-grosse Testdatei anlegen zu muessen. Jeder Test
    /// laeuft in seinem eigenen Thread, daher keine Interferenz zwischen
    /// Tests.
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
    /// 3MF-"Production-Extension"-Dateien lagern zusaetzliche Objekte in
    /// separaten ZIP-Eintraegen aus (referenziert via p:path). Schluessel
    /// ist der normalisierte (kein fuehrendes '/') Eintragspfad.
    pub referenced_models: HashMap<String, ParsedModel>,
    pub thumbnail: Option<Vec<u8>>,
    pub plate_count: Option<u32>,
    pub slice_info: Option<super::slice_info::SliceInfo>,
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

    // `total_unpacked` erfasst JEDE aus dem ZIP tatsaechlich gelesene
    // Ressource, in der Reihenfolge, in der read_package sie tatsaechlich
    // liest - nicht nur root_xml/referenzierte Modelle/Thumbnail (zweite
    // Review-Runde), sondern auch die beiden Slicer-Config-Dateien und
    // _rels/.rels (fuenfte Review-Runde), damit der Name
    // MAX_TOTAL_UNPACKED_BYTES ehrlich das gesamte Paket abdeckt.
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

    // Iterative Aufloesung bis zum Fixpunkt statt eines einzelnen
    // Durchlaufs: jede neu gelesene Datei kann selbst wieder neue,
    // noch unbekannte p:path-Referenzen enthalten (mehrstufige
    // Slicer-Ausgaben). `visited` verhindert Endlosschleifen bei
    // zirkulaeren Referenzen.
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
        // "Datei nicht gefunden"/"nicht parsbar" bleiben bewusst tolerant
        // (unveraendertes Verhalten, siehe Tests
        // `skips_missing_referenced_file_without_failing` und
        // `extract_render_meshes_skips_missing_referenced_object_without_failing`)
        // - NUR ein Verstoss gegen das Pro-Entry-Groessenlimit
        // (`EntryTooLarge`) wird hart propagiert (P1-Korrektur, zweite
        // Review-Runde): ein bereits bestehendes Groessenlimit darf nicht
        // durch stilles Ueberspringen unterlaufen werden - ein boesartiges
        // Paket koennte sonst gezielt knapp-zu-grosse referenzierte Modelle
        // einschleusen, die unbemerkt fehlen, waehrend das restliche Paket
        // scheinbar normal importiert wird.
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

/// Lehnt einen ZIP-Eintrag anhand seiner im Archiv deklarierten entpackten
/// Groesse ab. Die Angabe stammt aus dem Archiv selbst und ist damit nicht
/// vertrauenswuerdig - die eigentliche Schranke ist das `Read::take` beim
/// Lesen; diese Pruefung meldet nur den ehrlich deklarierten Fall mit einer
/// brauchbaren Fehlermeldung, statt eine stillschweigend abgeschnittene
/// (und dadurch kaputte) XML-/Bilddatei weiterzureichen.
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

    /// Baut ein ZIP mit einem einzelnen, stark komprimierbaren Eintrag -
    /// Miniatur-Nachbau einer "Zip-Bombe" (Security-Review 2026-09-19,
    /// Finding A-1).
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

        // Unterhalb der Grenze bleibt alles wie bisher.
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

    /// Baut ein 3MF mit `count` referenzierten `.model`-Dateien, die jede
    /// einzeln winzig sind (weit unter `MAX_MODEL_XML_BYTES`), deren Summe
    /// mit dem Root-Modell aber `total_bytes_target` Bytes ueberschreitet -
    /// zusammen mit einem per Testthread ueberschriebenen, kleinen
    /// `MAX_TOTAL_UNPACKED_BYTES` (siehe `TEST_MAX_TOTAL_UNPACKED_BYTES`)
    /// reicht das, um das Gesamtbudget zu ueberschreiten, ohne eine
    /// tatsaechlich riesige Testdatei anlegen zu muessen.
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

    /// Wie `build_multi_file_zip`, erlaubt aber ein abweichendes
    /// Root-Modell-XML (die feste `ROOT_MODEL_XML`-Konstante deckt nur
    /// genau eine referenzierte Datei ab).
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
        // 10 referenzierte Modelle liegen jede einzeln weit unter
        // MAX_MODEL_XML_BYTES (256 MB), ihre Summe (+ Root-Modell)
        // ueberschreitet aber das per Test auf 2000 Bytes verkleinerte
        // Gesamtbudget.
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

    /// Baut ein minimales 3MF (kleines Root-Modell ohne Build-Items, also
    /// ohne referenzierte Dateien) mit frei waehlbarem `_rels/.rels`-Inhalt
    /// und beliebigen zusaetzlichen Eintraegen (z. B. die beiden
    /// Slicer-Configs) - dient dazu, GENAU EINE Ressourcenart gezielt
    /// aufzublaehen, waehrend alle anderen winzig bleiben.
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

    /// Dies ist genau der urspruengliche Bug (M-05, fuenfte Review-Runde):
    /// ein einzelner `_rels/.rels`-Eintrag, der fuer sich genommen weit
    /// unter `MAX_RELS_XML_BYTES` bleibt, aber gross genug ist, um allein
    /// das (im Test verkleinerte) Gesamtbudget zu ueberschreiten, wurde
    /// vorher gar nicht mitgezaehlt und daher stillschweigend akzeptiert.
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

    /// Analog zu oben, aber fuer `Metadata/model_settings.config`
    /// (`count_plates()`).
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

    /// Analog zu oben, aber fuer `Metadata/slice_info.config`
    /// (`parse_slice_info()`).
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

    /// Gegenprobe zu den drei Tests oben: dasselbe minimale Paket ohne
    /// Polsterung wird unter demselben kleinen Testbudget weiterhin
    /// akzeptiert - beweist, dass die Ablehnung oben wirklich am
    /// Gesamtbudget liegt und nicht an einem unabhaengigen Parsing-Fehler
    /// oder generell zu knappen Testbudget.
    #[test]
    fn read_package_accepts_a_tiny_package_under_the_same_small_test_budget() {
        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(Some(1000)));
        let bytes = build_minimal_3mf_with_entries(SMALL_RELS_XML, &[]);
        let result = read_package(std::io::Cursor::new(bytes));
        TEST_MAX_TOTAL_UNPACKED_BYTES.with(|c| c.set(None));

        assert!(result.is_ok(), "ein winziges Paket muss unter dem Testbudget akzeptiert werden");
    }

    /// Baut ein 3MF, dessen referenziertes Modell `3D/Objects/big.model`
    /// tatsaechlich existiert (kein Missing-Referenzfall), dessen Inhalt
    /// aber `MAX_MODEL_XML_BYTES` um 1 Byte ueberschreitet - `Stored`
    /// (unkomprimiert) statt der Standard-Kompression, damit das Schreiben
    /// des Fixtures in vertretbarer Zeit passiert.
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

    /// Finding 2 (Review): eine referenzierte Modell-Datei, die
    /// tatsaechlich EXISTIERT, aber ihr Pro-Entry-Groessenlimit
    /// (`MAX_MODEL_XML_BYTES`) ueberschreitet, muss hart fehlschlagen statt
    /// stillschweigend uebersprungen zu werden - im Gegensatz zu einer
    /// fehlenden/kaputten Datei (siehe
    /// `skips_missing_referenced_file_without_failing`, die weiterhin
    /// toleriert). Der Abgleich auf `EntryTooLarge` (statt nur `is_err()`)
    /// stellt sicher, dass tatsaechlich der neue `EntryTooLarge`-Zweig
    /// getroffen wird und nicht z. B. die Gesamtbudget-Pruefung.
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
