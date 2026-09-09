# Rust-seitige Geometrie-Extraktion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ZIP-Entpacken und Mesh-Parsing für 3MF/STL komplett nativ in Rust erledigen und dem Frontend nur noch fertige Vertex-/Index-Rohdaten übergeben, statt roher Dateibytes, die im Browser via three.js' `DOMParser`-abhängigen Loadern geparst werden mussten - das blockierte bei großen Multi-Part-3MF-Dateien den kompletten UI-Thread für mehrere Sekunden bis Minuten.

**Architecture:** `container.rs` löst 3MF-„Production Extension"-Dateireferenzen (`p:path`) iterativ bis zum Fixpunkt auf; eine neue, zu `accumulate_object` parallele Walk-Funktion sammelt weltraum-transformierte Dreiecksgeometrie statt Bounding-Box/Volumen; STL bekommt eine neue Vertex-Normalen-Berechnung (repliziert three.js' Verhalten für nicht-indizierte Geometrie exakt); ein neuer Tauri-Command liefert alles als einzelnen `tauri::ipc::Response`-Binärstrom; das Frontend decodiert das Format direkt in `THREE.BufferGeometry`, ohne `STLLoader`/`ThreeMFLoader`.

**Tech Stack:** Rust (`zip`, `quick-xml`, bereits vorhandene Crate-Abhängigkeiten), Tauri v2 (`tauri::async_runtime::spawn_blocking`, `tauri::ipc::Response`), TypeScript/React 19, three.js (nur noch für `BufferGeometry`/Rendering, nicht mehr für Datei-Parsing).

## Global Constraints

- Deutsche Kommentare nur wo das WARUM nicht aus dem Code ersichtlich ist - keine Kommentare, die nur beschreiben, was der Code offensichtlich tut.
- TDD: jeder Code-Schritt beginnt mit einem fehlschlagenden Test, der dann zum Bestehen gebracht wird.
- Git-Commits auf Deutsch, mit gezieltem `git add <Datei>` (nie `-A`/`.`), jeder Commit endet mit:
  ```
  Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
  ```
- Nach jedem Rust-Task: `cd src-tauri && cargo test --no-default-features` muss vollständig grün sein (inklusive aller bereits bestehenden Tests - keine Regression).
- Nach jedem Frontend-Task: `npx tsc --noEmit` muss fehlerfrei durchlaufen (das Projekt hat aktuell keinen JS/TS-Testrunner - Verifikation erfolgt über TypeScript-Kompilierung und den abschließenden manuellen Live-Test, siehe Task 8. Es wird bewusst **kein** neues Test-Framework wie Vitest eingeführt, um keinen ungefragten neuen Tooling-Fußabdruck zu setzen).
- Kein `git commit --amend`, keine `--no-verify`.

---

### Task 1: `RenderMesh`-Datentyp und STL-Normalenberechnung in `geometry.rs`

**Files:**
- Modify: `src-tauri/src/geometry.rs`

**Interfaces:**
- Consumes: nichts (Fundament-Task, keine Abhängigkeiten zu anderen Tasks dieses Plans).
- Produces:
  - `pub struct RenderMesh { pub positions: Vec<[f32; 3]>, pub indices: Vec<[u32; 3]>, pub normals: Option<Vec<[f32; 3]>> }` (derives `Debug, Clone, PartialEq`)
  - `pub fn compute_flat_normals(vertices: &[[f64; 3]], triangles: &[[u32; 3]]) -> Vec<[f32; 3]>`

Beide werden von Task 4 (3MF-Extraktion, nutzt `RenderMesh`), Task 5 (STL-Extraktion, nutzt `RenderMesh` + `compute_flat_normals`) und Task 6 (Wire-Format-Encoder, nutzt `RenderMesh`) verwendet.

- [ ] **Step 1: Failing tests schreiben**

Füge am Ende von `src-tauri/src/geometry.rs` an:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_flat_normals_returns_outward_unit_normal_for_single_triangle() {
        // Dreieck in der xy-Ebene (z=0); erwartete Normale gemaess der
        // (C-B) x (A-B) - Konvention (siehe Kommentar an compute_flat_normals).
        let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let triangles = [[0u32, 1, 2]];

        let normals = compute_flat_normals(&vertices, &triangles);

        assert_eq!(normals.len(), 3);
        for n in &normals {
            assert!(n[0].abs() < 1e-6, "unexpected x: {n:?}");
            assert!(n[1].abs() < 1e-6, "unexpected y: {n:?}");
            assert!((n[2] - 1.0).abs() < 1e-6, "unexpected z: {n:?}");
        }
    }

    #[test]
    fn compute_flat_normals_returns_zero_for_degenerate_triangle() {
        let vertices = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
        let triangles = [[0u32, 1, 2]];

        let normals = compute_flat_normals(&vertices, &triangles);

        assert_eq!(normals[0], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn render_mesh_is_constructible_and_comparable() {
        let a = RenderMesh {
            positions: vec![[0.0, 0.0, 0.0]],
            indices: vec![],
            normals: None,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 2: Tests laufen lassen, Fehlschlag verifizieren**

Run: `cd src-tauri && cargo test --no-default-features geometry:: -- --nocapture`
Expected: FAIL mit `cannot find function 'compute_flat_normals'` / `cannot find struct 'RenderMesh'`

- [ ] **Step 3: `RenderMesh` und `compute_flat_normals` implementieren**

Füge in `src-tauri/src/geometry.rs` VOR dem `#[cfg(test)]`-Block ein:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct RenderMesh {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<[u32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
}

/// Berechnet eine flache (nicht ueber Dreiecke gemittelte) Normale pro
/// Dreieck und weist sie allen drei Ecken zu. Das entspricht exakt three.js'
/// `BufferGeometry.computeVertexNormals()` fuer *nicht-indizierte* Geometrie
/// (siehe deren Quelltext: im nicht-indizierten Zweig wird das rohe
/// Kreuzprodukt ohne Mittelung ueber Dreiecke hinweg direkt auf alle drei
/// Ecken geschrieben, da ohne Index keine Information ueber geteilte
/// Vertices existiert). STL-Daten haben von Natur aus keine geteilten
/// Vertices, daher reproduziert das exakt das bisherige (three.js-basierte)
/// Shading der STL-Vorschau.
pub fn compute_flat_normals(vertices: &[[f64; 3]], triangles: &[[u32; 3]]) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0f32; 3]; vertices.len()];
    for tri in triangles {
        let a = vertices[tri[0] as usize];
        let b = vertices[tri[1] as usize];
        let c = vertices[tri[2] as usize];

        let cb = [c[0] - b[0], c[1] - b[1], c[2] - b[2]];
        let ab = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let cross = [
            cb[1] * ab[2] - cb[2] * ab[1],
            cb[2] * ab[0] - cb[0] * ab[2],
            cb[0] * ab[1] - cb[1] * ab[0],
        ];
        let len = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let n = if len > 0.0 {
            [
                (cross[0] / len) as f32,
                (cross[1] / len) as f32,
                (cross[2] / len) as f32,
            ]
        } else {
            [0.0, 0.0, 0.0]
        };

        for &idx in tri {
            normals[idx as usize] = n;
        }
    }
    normals
}
```

- [ ] **Step 4: Tests laufen lassen, Erfolg verifizieren**

Run: `cd src-tauri && cargo test --no-default-features geometry:: -- --nocapture`
Expected: PASS (3 Tests)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/geometry.rs
git commit -m "$(cat <<'EOF'
Rust: RenderMesh-Typ und STL-Normalenberechnung hinzufuegen

Fundament fuer die native Geometrie-Extraktion: RenderMesh ist das
gemeinsame Ausgabeformat fuer 3MF- und STL-Parsing (Positionen,
Indizes, optionale Normalen). compute_flat_normals repliziert three.js'
computeVertexNormals()-Verhalten fuer nicht-indizierte Geometrie exakt,
damit die STL-Vorschau nach der Umstellung gleich aussieht wie vorher.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

---

### Task 2: `p:path`-Attribut in `model_xml.rs` parsen

**Files:**
- Modify: `src-tauri/src/threemf/model_xml.rs`

**Interfaces:**
- Consumes: nichts.
- Produces:
  - `Component { object_id: String, path: Option<String>, transform: Option<Matrix3x4> }` (neues Feld `path`)
  - `BuildItem { object_id: String, path: Option<String>, transform: Option<Matrix3x4> }` (neues Feld `path`)

  `path` ist der Wert des `p:path`-Attributs (Namespace-Praefix wird wie bei allen anderen Attributen ignoriert, siehe `local_name`), normalisiert durch Entfernen eines fuehrenden `/` - konsistent mit der Normalisierung, die `container.rs`s `resolve_relationships` bereits fuer `.rels`-Ziele anwendet. Fehlt das Attribut, ist `path` `None` (Objekt liegt in derselben Datei wie das deklarierende Element).

  Task 3 baut auf diesen beiden Feldern auf (sammelt alle `Some(path)`-Werte, um referenzierte Dateien zu entdecken). Task 4 nutzt sie beim Aufloesen von Objekt-Referenzen ueber Dateigrenzen hinweg.

- [ ] **Step 1: Failing test schreiben**

Füge am Ende von `src-tauri/src/threemf/model_xml.rs` an (die Datei hat aktuell kein `#[cfg(test)]`-Modul):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_p_path_on_component_and_item_and_normalizes_leading_slash() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06">
  <resources>
    <object id="1" type="model">
      <components>
        <component p:path="/3D/Objects/object_2.model" objectid="2"/>
      </components>
    </object>
  </resources>
  <build>
    <item p:path="/3D/Objects/object_1.model" objectid="5"/>
    <item objectid="1"/>
  </build>
</model>"##;

        let model = parse_model_xml(xml).expect("parse should succeed");

        let component = &model.objects.get("1").expect("object 1 present").components[0];
        assert_eq!(component.object_id, "2");
        assert_eq!(
            component.path.as_deref(),
            Some("3D/Objects/object_2.model")
        );

        assert_eq!(
            model.build_items[0].path.as_deref(),
            Some("3D/Objects/object_1.model")
        );
        assert_eq!(model.build_items[1].path, None);
    }
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag verifizieren**

Run: `cd src-tauri && cargo test --no-default-features model_xml:: -- --nocapture`
Expected: FAIL (Kompilierfehler: `no field 'path' on type 'Component'` / `'BuildItem'`)

- [ ] **Step 3: `path`-Feld hinzufuegen und parsen**

In `src-tauri/src/threemf/model_xml.rs`, `Component` und `BuildItem` erweitern:

```rust
#[derive(Debug, Clone)]
pub struct Component {
    pub object_id: String,
    pub path: Option<String>,
    pub transform: Option<Matrix3x4>,
}
```

```rust
#[derive(Debug, Clone)]
pub struct BuildItem {
    pub object_id: String,
    pub path: Option<String>,
    pub transform: Option<Matrix3x4>,
}
```

In `handle_start`, die `"component"`- und `"item"`-Zweige anpassen:

```rust
        "component" => {
            if let Some((_, obj)) = ctx.current_object.as_mut() {
                let object_id = get_attr(e, "objectid").unwrap_or_default();
                let path = get_attr(e, "path").map(|p| p.trim_start_matches('/').to_string());
                let transform = get_attr(e, "transform")
                    .map(|t| Matrix3x4::parse(&t))
                    .transpose()?;
                obj.components.push(Component {
                    object_id,
                    path,
                    transform,
                });
            }
        }
        "item" => {
            let object_id = get_attr(e, "objectid").unwrap_or_default();
            let path = get_attr(e, "path").map(|p| p.trim_start_matches('/').to_string());
            let transform = get_attr(e, "transform")
                .map(|t| Matrix3x4::parse(&t))
                .transpose()?;
            ctx.model.build_items.push(BuildItem {
                object_id,
                path,
                transform,
            });
        }
```

- [ ] **Step 4: Test laufen lassen, Erfolg verifizieren**

Run: `cd src-tauri && cargo test --no-default-features -- --nocapture`
Expected: PASS für alle Tests in `model_xml`. Es gibt an dieser Stelle noch Kompilierfehler in `threemf/mod.rs`, weil dort `Component { object_id, transform }`/`BuildItem { object_id, transform }` ohne das neue Feld konstruiert bzw. gematcht wird - das ist erwartet und wird in Task 4 behoben. Führe daher gezielt nur `cargo test --no-default-features model_xml::` aus, nicht die volle Suite.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/threemf/model_xml.rs
git commit -m "$(cat <<'EOF'
Rust: p:path-Attribut an Component/BuildItem erfassen

3MF-Dateien im "Production Extension"-Format (u.a. Bambu Studio/
OrcaSlicer) lagern Objekt-Geometrie ueber p:path-Verweise in separate
ZIP-Eintraege aus. Das Attribut wurde bisher komplett ignoriert -
dieser Schritt erfasst und normalisiert es nur, die eigentliche
Mehrdatei-Aufloesung folgt in einem separaten Commit.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

---

### Task 3: Multi-File-Auflösung in `container.rs`

**Files:**
- Modify: `src-tauri/src/threemf/container.rs`

**Interfaces:**
- Consumes: `Component.path`/`BuildItem.path` aus Task 2; `model_xml::{parse_model_xml, ParsedModel, Object}`.
- Produces:
  - `pub struct PackageParts { pub root_model: ParsedModel, pub referenced_models: HashMap<String, ParsedModel>, pub thumbnail: Option<Vec<u8>> }` (ersetzt die bisherige Form mit `model_xml: String`)
  - `impl PackageParts { pub fn lookup_object(&self, file: Option<&str>, object_id: &str) -> Option<&Object> }`
  - `pub fn read_package<R: Read + Seek>(reader: R) -> Result<PackageParts, ThreeMfError>` (Signatur unveraendert, Rueckgabetyp geaendert)

  Task 4 baut direkt auf `PackageParts`/`lookup_object` auf.

**Hinweis:** Dieser Task macht `threemf/mod.rs` vorübergehend nicht kompilierbar (`parse_3mf_reader` nutzt noch die alte `PackageParts`-Form) - das wird in Task 4 behoben. Führe in diesem Task gezielt nur `cargo test --no-default-features container::` aus, nicht die volle Suite.

- [ ] **Step 1: Failing tests schreiben**

Füge am Ende von `src-tauri/src/threemf/container.rs` an:

```rust
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
```

- [ ] **Step 2: Tests laufen lassen, Fehlschlag verifizieren**

Run: `cd src-tauri && cargo test --no-default-features container:: -- --nocapture`
Expected: FAIL (Kompilierfehler: `no field 'referenced_models' on type 'PackageParts'` o.ä.)

- [ ] **Step 3: `PackageParts` und Fixpunkt-Auflösung implementieren**

Ersetze in `src-tauri/src/threemf/container.rs` die Imports am Dateianfang:

```rust
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek};

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use zip::ZipArchive;

use super::error::ThreeMfError;
use super::model_xml::{parse_model_xml, Object, ParsedModel};
```

Ersetze `pub struct PackageParts` und `pub fn read_package` komplett:

```rust
pub struct PackageParts {
    pub root_model: ParsedModel,
    /// 3MF-"Production-Extension"-Dateien lagern zusaetzliche Objekte in
    /// separaten ZIP-Eintraegen aus (referenziert via p:path). Schluessel
    /// ist der normalisierte (kein fuehrendes '/') Eintragspfad.
    pub referenced_models: HashMap<String, ParsedModel>,
    pub thumbnail: Option<Vec<u8>>,
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
```

Die restlichen Funktionen (`resolve_relationships`, `read_entry_to_string`, `read_entry_to_bytes`) bleiben unveraendert.

- [ ] **Step 4: Tests laufen lassen, Erfolg verifizieren**

Run: `cd src-tauri && cargo test --no-default-features container:: -- --nocapture`
Expected: PASS (5 Tests)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/threemf/container.rs
git commit -m "$(cat <<'EOF'
Rust: Multi-File-Aufloesung fuer 3MF-Production-Extension-Dateien

read_package loest p:path-Referenzen jetzt iterativ bis zum Fixpunkt
auf (Warteschlange + besuchte Pfade gegen Zyklen) und liefert alle
referenzierten Objekt-Dateien als PackageParts.referenced_models.
Fehlt eine referenzierte Datei oder ist sie nicht parsbar, wird sie
uebersprungen statt die ganze 3MF-Datei abzulehnen.

Macht threemf/mod.rs voruebergehend nicht kompilierbar (folgt im
naechsten Commit) - PackageParts.model_xml gibt es nicht mehr.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

---

### Task 4: Geometrie-Extraktion und Metadaten-Bugfix in `threemf/mod.rs`

**Files:**
- Modify: `src-tauri/src/threemf/mod.rs`

**Interfaces:**
- Consumes: `container::{read_package, PackageParts}`, `container::PackageParts::lookup_object` (Task 3); `RenderMesh` (Task 1); `model_xml::{Object, ParsedModel}` (mit `path`-Feldern aus Task 2).
- Produces:
  - `pub fn extract_render_meshes(package: &container::PackageParts) -> Vec<RenderMesh>`
  - `pub fn extract_render_meshes_from_path(path: &Path) -> Result<Vec<RenderMesh>, ThreeMfError>`

  Task 6 (Tauri-Command) ruft `extract_render_meshes_from_path` auf.

Dieser Task behebt zusätzlich einen bestehenden Bug als Nebeneffekt: `resolve_geometry`/`accumulate_object` (Größe/Volumen/Material im Detail-Panel) sahen bei Multi-Part-3MF-Dateien bisher nur die Root-Datei - nach diesem Task laufen sie automatisch gegen das vollständig aufgelöste Modell.

- [ ] **Step 1: Failing tests schreiben**

Füge im bestehenden `#[cfg(test)] mod tests`-Block in `src-tauri/src/threemf/mod.rs` (nach den bestehenden Tests, vor der schließenden `}`) folgendes hinzu - inklusive der neuen Helper-Konstanten/Funktion `build_multi_file_test_3mf`:

```rust
    const ROOT_MODEL_XML_MULTI: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06">
  <resources>
    <object id="1" type="model">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0"/>
          <vertex x="10" y="0" z="0"/>
          <vertex x="10" y="10" z="0"/>
          <vertex x="0" y="10" z="0"/>
          <vertex x="0" y="0" z="10"/>
          <vertex x="10" y="0" z="10"/>
          <vertex x="10" y="10" z="10"/>
          <vertex x="0" y="10" z="10"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="2" v3="1"/>
          <triangle v1="0" v2="3" v3="2"/>
          <triangle v1="4" v2="5" v3="6"/>
          <triangle v1="4" v2="6" v3="7"/>
          <triangle v1="0" v2="1" v3="5"/>
          <triangle v1="0" v2="5" v3="4"/>
          <triangle v1="3" v2="6" v3="2"/>
          <triangle v1="3" v2="7" v3="6"/>
          <triangle v1="0" v2="7" v3="3"/>
          <triangle v1="0" v2="4" v3="7"/>
          <triangle v1="1" v2="2" v3="6"/>
          <triangle v1="1" v2="6" v3="5"/>
        </triangles>
      </mesh>
    </object>
  </resources>
  <build>
    <item objectid="1"/>
    <item p:path="/3D/Objects/object_2.model" objectid="1" transform="1 0 0 0 1 0 0 0 1 20 0 0"/>
  </build>
</model>"##;

    const CHILD_MODEL_XML_MULTI: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <resources>
    <object id="1" type="model">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0"/>
          <vertex x="10" y="0" z="0"/>
          <vertex x="10" y="10" z="0"/>
          <vertex x="0" y="10" z="0"/>
          <vertex x="0" y="0" z="10"/>
          <vertex x="10" y="0" z="10"/>
          <vertex x="10" y="10" z="10"/>
          <vertex x="0" y="10" z="10"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="2" v3="1"/>
          <triangle v1="0" v2="3" v3="2"/>
          <triangle v1="4" v2="5" v3="6"/>
          <triangle v1="4" v2="6" v3="7"/>
          <triangle v1="0" v2="1" v3="5"/>
          <triangle v1="0" v2="5" v3="4"/>
          <triangle v1="3" v2="6" v3="2"/>
          <triangle v1="3" v2="7" v3="6"/>
          <triangle v1="0" v2="7" v3="3"/>
          <triangle v1="0" v2="4" v3="7"/>
          <triangle v1="1" v2="2" v3="6"/>
          <triangle v1="1" v2="6" v3="5"/>
        </triangles>
      </mesh>
    </object>
  </resources>
</model>"##;

    fn build_multi_file_test_3mf(include_referenced_file: bool) -> Vec<u8> {
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
            zip.write_all(ROOT_MODEL_XML_MULTI.as_bytes()).unwrap();

            if include_referenced_file {
                zip.start_file("3D/Objects/object_2.model", options)
                    .unwrap();
                zip.write_all(CHILD_MODEL_XML_MULTI.as_bytes()).unwrap();
            }

            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn multi_part_3mf_dimensions_and_volume_include_referenced_file_objects() {
        let bytes = build_multi_file_test_3mf(true);
        let doc = parse_3mf_bytes(&bytes).expect("parse should succeed");

        assert_eq!(doc.object_count, 2);
        let dims = doc.dimensions_mm.expect("dimensions present");
        assert!((dims[0] - 30.0).abs() < 1e-6, "unexpected x size: {}", dims[0]);
        assert!((dims[1] - 10.0).abs() < 1e-6, "unexpected y size: {}", dims[1]);
        assert!((dims[2] - 10.0).abs() < 1e-6, "unexpected z size: {}", dims[2]);

        let volume = doc.volume_cm3.expect("volume present");
        assert!((volume - 2.0).abs() < 1e-6, "unexpected volume: {volume}");
    }

    #[test]
    fn extract_render_meshes_returns_world_transformed_geometry_from_both_files() {
        let bytes = build_multi_file_test_3mf(true);
        let package =
            container::read_package(std::io::Cursor::new(bytes)).expect("read should succeed");

        let meshes = extract_render_meshes(&package);

        assert_eq!(meshes.len(), 2);
        for mesh in &meshes {
            assert_eq!(mesh.positions.len(), 8);
            assert_eq!(mesh.indices.len(), 12);
            assert!(mesh.normals.is_none());
        }

        assert!(meshes[0].positions.contains(&[0.0, 0.0, 0.0]));
        assert!(meshes[1].positions.contains(&[20.0, 0.0, 0.0]));
    }

    #[test]
    fn extract_render_meshes_skips_missing_referenced_object_without_failing() {
        let bytes = build_multi_file_test_3mf(false);
        let package =
            container::read_package(std::io::Cursor::new(bytes)).expect("read should succeed");

        let meshes = extract_render_meshes(&package);

        assert_eq!(meshes.len(), 1);
    }
```

Ergänze außerdem am Anfang des bestehenden `#[cfg(test)] mod tests`-Blocks (bei den bestehenden `use`-Anweisungen, die dort schon `use std::io::Write; use zip::write::SimpleFileOptions; use zip::ZipWriter;` enthalten - diese sind bereits vorhanden und werden von den neuen Tests mitgenutzt, keine Änderung nötig).

- [ ] **Step 2: Tests laufen lassen, Fehlschlag verifizieren**

Run: `cd src-tauri && cargo test --no-default-features threemf:: -- --nocapture`
Expected: FAIL (Kompilierfehler: `no function 'extract_render_meshes'`, außerdem bestehende Kompilierfehler aus Task 3 in `resolve_geometry`/`accumulate_object`/`parse_3mf_reader`, die jetzt behoben werden)

- [ ] **Step 3: `resolve_geometry`/`accumulate_object` anpassen, `extract_render_meshes` implementieren**

Ersetze in `src-tauri/src/threemf/mod.rs` die Imports:

```rust
pub mod container;
pub mod error;
pub mod geometry;
pub mod model_xml;

use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

pub use error::ThreeMfError;
use crate::geometry::{BoundingBox, RenderMesh};
use container::PackageParts;
use geometry::Matrix3x4;
```

Ersetze `parse_3mf_reader`, `resolve_geometry` und `accumulate_object` komplett, und ergänze die beiden neuen öffentlichen Funktionen direkt davor:

```rust
/// Liest und loest das 3MF-Paket unter `path` vollstaendig auf (inklusive
/// per p:path referenzierter Objekt-Dateien) und extrahiert daraus
/// render-fertige Mesh-Daten - Positionen bereits weltraum-transformiert,
/// Normalen bleiben `None` (entspricht dem bisherigen Verhalten von
/// three.js' ThreeMFLoader, der fuer 3MF nie Normalen gesetzt hat).
pub fn extract_render_meshes_from_path(path: &Path) -> Result<Vec<RenderMesh>, ThreeMfError> {
    let file = File::open(path)?;
    let package = container::read_package(file)?;
    Ok(extract_render_meshes(&package))
}

pub fn extract_render_meshes(package: &PackageParts) -> Vec<RenderMesh> {
    let mut meshes = Vec::new();
    for item in &package.root_model.build_items {
        let transform = item.transform.unwrap_or_else(Matrix3x4::identity);
        collect_render_meshes(
            package,
            item.path.as_deref(),
            &item.object_id,
            &transform,
            &mut meshes,
        );
    }
    meshes
}

fn collect_render_meshes(
    package: &PackageParts,
    file: Option<&str>,
    object_id: &str,
    transform: &Matrix3x4,
    out: &mut Vec<RenderMesh>,
) {
    let Some(object) = package.lookup_object(file, object_id) else {
        return;
    };

    let is_model_geometry = object.object_type.as_deref().is_none_or(|t| t == "model");

    if is_model_geometry {
        if let Some(mesh) = &object.mesh {
            if !mesh.triangles.is_empty() {
                let positions: Vec<[f32; 3]> = mesh
                    .vertices
                    .iter()
                    .map(|v| {
                        let [x, y, z] = transform.transform_point(*v);
                        [x as f32, y as f32, z as f32]
                    })
                    .collect();
                out.push(RenderMesh {
                    positions,
                    indices: mesh.triangles.clone(),
                    normals: None,
                });
            }
        }
    }

    for component in &object.components {
        let child_transform =
            transform.compose(&component.transform.unwrap_or_else(Matrix3x4::identity));
        let child_file = component.path.as_deref().or(file);
        collect_render_meshes(
            package,
            child_file,
            &component.object_id,
            &child_transform,
            out,
        );
    }
}

fn parse_3mf_reader<R: std::io::Read + std::io::Seek>(
    reader: R,
) -> Result<ThreeMfDocument, ThreeMfError> {
    let package = container::read_package(reader)?;

    let (bbox, volume_mm3) = resolve_geometry(&package);

    let dimensions_mm = bbox.is_valid().then(|| bbox.size());
    let volume_cm3 = bbox.is_valid().then_some(volume_mm3 / 1000.0);

    Ok(ThreeMfDocument {
        object_count: package.root_model.build_items.len(),
        dimensions_mm,
        volume_cm3,
        materials: package
            .root_model
            .materials
            .iter()
            .map(|m| ThreeMfMaterial {
                name: m.name.clone(),
                display_color: m.display_color.clone(),
            })
            .collect(),
        metadata: package.root_model.metadata.clone(),
        thumbnail_png: package.thumbnail.clone(),
    })
}

fn resolve_geometry(package: &PackageParts) -> (BoundingBox, f64) {
    let mut bbox = BoundingBox::empty();
    let mut volume_mm3 = 0.0;

    for item in &package.root_model.build_items {
        let transform = item.transform.unwrap_or_else(Matrix3x4::identity);
        accumulate_object(
            package,
            item.path.as_deref(),
            &item.object_id,
            &transform,
            &mut bbox,
            &mut volume_mm3,
        );
    }

    (bbox, volume_mm3)
}

fn accumulate_object(
    package: &PackageParts,
    file: Option<&str>,
    object_id: &str,
    transform: &Matrix3x4,
    bbox: &mut BoundingBox,
    volume_mm3: &mut f64,
) {
    let Some(object) = package.lookup_object(file, object_id) else {
        return;
    };

    // Only "model" objects (the default when unspecified) count toward the
    // catalog's reported size/volume — support/solidsupport/surface objects
    // aren't part of the printed part itself.
    let is_model_geometry = object.object_type.as_deref().is_none_or(|t| t == "model");

    if is_model_geometry {
        if let Some(mesh) = &object.mesh {
            let world_vertices: Vec<[f64; 3]> = mesh
                .vertices
                .iter()
                .map(|v| transform.transform_point(*v))
                .collect();
            for v in &world_vertices {
                bbox.extend(*v);
            }
            *volume_mm3 += crate::geometry::signed_volume(&world_vertices, &mesh.triangles).abs();
        }
    }

    for component in &object.components {
        let child_transform =
            transform.compose(&component.transform.unwrap_or_else(Matrix3x4::identity));
        let child_file = component.path.as_deref().or(file);
        accumulate_object(
            package,
            child_file,
            &component.object_id,
            &child_transform,
            bbox,
            volume_mm3,
        );
    }
}
```

- [ ] **Step 4: Volle Test-Suite laufen lassen, Erfolg verifizieren**

Run: `cd src-tauri && cargo test --no-default-features -- --nocapture`
Expected: PASS für **alle** Tests im Projekt (bestehende `threemf`-, `stl`-, `db`-, `cloud`-Tests unverändert grün, plus die 3 neuen aus diesem Task, plus die aus Task 1-3).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/threemf/mod.rs
git commit -m "$(cat <<'EOF'
Rust: Geometrie-Extraktion ueber Dateigrenzen hinweg, Metadaten-Bugfix

extract_render_meshes()/extract_render_meshes_from_path() liefern
weltraum-transformierte Render-Geometrie fuer 3MF, inklusive ueber
p:path referenzierter Objekt-Dateien - parallele Walk-Funktion zu
accumulate_object(), die jetzt ebenfalls dateiuebergreifend aufloest.

Nebeneffekt-Bugfix: Groesse/Volumen/Material im Detail-Panel wurden
bei Multi-Part-3MF-Dateien bisher nur aus der Root-Datei berechnet
und zeigten "-" an, weil dort keine Mesh-Daten liegen. Sie laufen
jetzt automatisch gegen das vollstaendig aufgeloeste Modell.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

---

### Task 5: STL-Geometrie-Extraktion in `stl/mod.rs`

**Files:**
- Modify: `src-tauri/src/stl/mod.rs`

**Interfaces:**
- Consumes: `crate::geometry::{RenderMesh, compute_flat_normals}` (Task 1); `parser::parse` (bereits vorhanden, unveraendert).
- Produces: `pub fn parse_stl_geometry(bytes: &[u8]) -> Result<RenderMesh, StlError>`

  Task 6 (Tauri-Command) ruft diese Funktion für `.stl`-Dateien auf.

- [ ] **Step 1: Failing test schreiben**

Füge im bestehenden `#[cfg(test)] mod tests`-Block in `src-tauri/src/stl/mod.rs` (nach `rejects_malformed_ascii`, vor der schließenden `}`) hinzu:

```rust
    #[test]
    fn computes_flat_outward_normals_for_ascii_cube() {
        let bytes = build_ascii_cube();
        let mesh = parse_stl_geometry(&bytes).expect("geometry parse should succeed");

        assert_eq!(mesh.positions.len(), 36);
        assert_eq!(mesh.indices.len(), 12);
        let normals = mesh.normals.expect("stl geometry always has normals");
        assert_eq!(normals.len(), 36);

        for n in &normals {
            let len = ((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]) as f64).sqrt();
            assert!((len - 1.0).abs() < 1e-4, "normal not unit length: {n:?}");
        }

        // Dreieck 2 (0-indiziert) ist CUBE_TRIANGLES[2] = [4,5,6], die
        // Deckflaeche bei z=10 - von Hand nachgerechnet
        // (cb=(C-B)=[0,10,0], ab=(A-B)=[-10,0,0], cb x ab = [0,0,100])
        // muss die Normale nach [0,0,1] zeigen.
        let top_face_normal = normals[6];
        assert!(top_face_normal[0].abs() < 1e-4, "unexpected: {top_face_normal:?}");
        assert!(top_face_normal[1].abs() < 1e-4, "unexpected: {top_face_normal:?}");
        assert!(
            (top_face_normal[2] - 1.0).abs() < 1e-4,
            "unexpected: {top_face_normal:?}"
        );
    }
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag verifizieren**

Run: `cd src-tauri && cargo test --no-default-features stl:: -- --nocapture`
Expected: FAIL mit `cannot find function 'parse_stl_geometry'`

- [ ] **Step 3: `parse_stl_geometry` implementieren**

Füge in `src-tauri/src/stl/mod.rs` nach `parse_stl_bytes` (vor dem `#[cfg(test)]`-Block) ein:

```rust
pub fn parse_stl_geometry(bytes: &[u8]) -> Result<crate::geometry::RenderMesh, StlError> {
    let (vertices, triangles) = parser::parse(bytes)?;
    let normals = crate::geometry::compute_flat_normals(&vertices, &triangles);
    let positions: Vec<[f32; 3]> = vertices
        .iter()
        .map(|v| [v[0] as f32, v[1] as f32, v[2] as f32])
        .collect();
    Ok(crate::geometry::RenderMesh {
        positions,
        indices: triangles,
        normals: Some(normals),
    })
}
```

- [ ] **Step 4: Tests laufen lassen, Erfolg verifizieren**

Run: `cd src-tauri && cargo test --no-default-features -- --nocapture`
Expected: PASS für alle Tests im Projekt.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/stl/mod.rs
git commit -m "$(cat <<'EOF'
Rust: STL-Geometrie-Extraktion mit Normalenberechnung

parse_stl_geometry() liefert Positionen, triviale fortlaufende
Indizes (STL-Format kennt kein Vertex-Sharing) und ueber
compute_flat_normals() berechnete Normalen - reproduziert das
bisherige three.js-computeVertexNormals()-Shading exakt.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

---

### Task 6: Wire-Format-Encoder und `get_model_geometry`-Command

**Files:**
- Modify: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: `threemf::extract_render_meshes_from_path` (Task 4), `stl::parse_stl_geometry` (Task 5), `crate::geometry::RenderMesh` (Task 1).
- Produces: `get_model_geometry`-Tauri-Command liefert `tauri::ipc::Response` mit dem unten beschriebenen Binärformat. Task 7 (Frontend-Decoder) implementiert das exakte Gegenstück.

**Wire-Format** (siehe `encode_render_meshes` unten): 4 Bytes Headerlänge (`u32`, little-endian), gefolgt von einem mit Leerzeichen auf ein Vielfaches von 4 Bytes aufgepolsterten JSON-Header (`Array<{ vertexCount: number, hasNormal: boolean, indexCount: number }>`), gefolgt von den rohen Daten pro Mesh in Header-Reihenfolge: `Float32`-Positionen, optional `Float32`-Normalen, dann **immer** `Uint32`-Indizes (bewusst keine `Uint16`-Variante - jeder Abschnitt besteht so ausschließlich aus 4-Byte-Elementen, wodurch der laufende Byte-Offset nach jedem Mesh automatisch ein Vielfaches von 4 bleibt und keine zusätzliche Ausrichtungs-Behandlung zwischen Meshes nötig ist).

- [ ] **Step 1: Failing tests schreiben**

Füge am Ende von `src-tauri/src/commands.rs` ein komplett neues Test-Modul an (die Datei hat aktuell kein `#[cfg(test)]`-Modul):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::RenderMesh;

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HeaderEntryForTest {
        vertex_count: usize,
        has_normal: bool,
        index_count: usize,
    }

    fn decode_for_test(bytes: &[u8]) -> Vec<(Vec<[f32; 3]>, Option<Vec<[f32; 3]>>, Vec<u32>)> {
        let header_len = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        let header_json = std::str::from_utf8(&bytes[4..4 + header_len]).unwrap();
        let headers: Vec<HeaderEntryForTest> = serde_json::from_str(header_json).unwrap();

        let mut offset = 4 + header_len;
        let mut result = Vec::new();
        for h in headers {
            let mut positions = Vec::with_capacity(h.vertex_count);
            for _ in 0..h.vertex_count {
                let x = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                let y = f32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
                let z = f32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap());
                positions.push([x, y, z]);
                offset += 12;
            }

            let normals = if h.has_normal {
                let mut ns = Vec::with_capacity(h.vertex_count);
                for _ in 0..h.vertex_count {
                    let x = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                    let y = f32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
                    let z = f32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap());
                    ns.push([x, y, z]);
                    offset += 12;
                }
                Some(ns)
            } else {
                None
            };

            let mut indices = Vec::with_capacity(h.index_count);
            for _ in 0..h.index_count {
                let idx = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                indices.push(idx);
                offset += 4;
            }

            result.push((positions, normals, indices));
        }
        assert_eq!(offset, bytes.len(), "encoder should not leave trailing bytes");
        result
    }

    #[test]
    fn encode_render_meshes_roundtrips_positions_normals_and_indices() {
        let meshes = vec![
            RenderMesh {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                indices: vec![[0, 1, 2]],
                normals: Some(vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]]),
            },
            RenderMesh {
                positions: vec![
                    [5.0, 5.0, 5.0],
                    [6.0, 5.0, 5.0],
                    [5.0, 6.0, 5.0],
                    [5.0, 5.0, 6.0],
                ],
                indices: vec![[0, 1, 2], [0, 1, 3]],
                normals: None,
            },
        ];

        let bytes = encode_render_meshes(&meshes);
        let decoded = decode_for_test(&bytes);

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].0, meshes[0].positions);
        assert_eq!(decoded[0].1, meshes[0].normals);
        assert_eq!(decoded[0].2, vec![0, 1, 2]);

        assert_eq!(decoded[1].0, meshes[1].positions);
        assert_eq!(decoded[1].1, None);
        assert_eq!(decoded[1].2, vec![0, 1, 2, 0, 1, 3]);
    }

    #[test]
    fn encode_render_meshes_handles_empty_mesh_list() {
        let bytes = encode_render_meshes(&[]);
        let decoded = decode_for_test(&bytes);
        assert!(decoded.is_empty());
    }
}
```

- [ ] **Step 2: Tests laufen lassen, Fehlschlag verifizieren**

Run: `cd src-tauri && cargo test --no-default-features commands:: -- --nocapture`
Expected: FAIL mit `cannot find function 'encode_render_meshes'`

- [ ] **Step 3: `encode_render_meshes` implementieren und `get_model_geometry` umbauen**

Füge in `src-tauri/src/commands.rs` ganz oben, bei den bestehenden `use`-Anweisungen, hinzu:

```rust
use crate::geometry::RenderMesh;
```

(die Zeile `use crate::{stl, threemf};` bleibt unverändert bestehen.)

Suche die aktuelle `get_model_geometry`-Funktion:

```rust
#[tauri::command]
pub fn get_model_geometry(
    state: State<AppState>,
    file_id: String,
) -> Result<tauri::ipc::Response, String> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;

    let bytes = std::fs::read(&file.path).map_err(|e| e.to_string())?;

    Ok(tauri::ipc::Response::new(bytes))
}
```

und ersetze sie komplett durch:

```rust
// Encodiert die extrahierte Geometrie als einzelnen Binaerstrom fuer
// tauri::ipc::Response: 4 Bytes Headerlaenge (u32 LE), dann ein mit
// Leerzeichen auf ein Vielfaches von 4 Bytes aufgepolsterter JSON-Header,
// gefolgt von den rohen Float32/Uint32-Puffern je Mesh in Header-
// Reihenfolge. Jeder Abschnitt (Position/Normale/Index) besteht
// ausschliesslich aus 4-Byte-Elementen, daher bleibt der laufende Offset
// nach jedem Mesh automatisch ein Vielfaches von 4 - keine zusaetzliche
// Ausrichtungs-Behandlung noetig (siehe auch die Wire-Format-Beschreibung
// in src/lib/parseModelGeometry.ts auf der Frontend-Seite).
fn encode_render_meshes(meshes: &[RenderMesh]) -> Vec<u8> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct MeshHeaderEntry {
        vertex_count: usize,
        has_normal: bool,
        index_count: usize,
    }

    let headers: Vec<MeshHeaderEntry> = meshes
        .iter()
        .map(|m| MeshHeaderEntry {
            vertex_count: m.positions.len(),
            has_normal: m.normals.is_some(),
            index_count: m.indices.len() * 3,
        })
        .collect();

    let mut header_json =
        serde_json::to_vec(&headers).expect("mesh header serialization cannot fail");
    while (4 + header_json.len()) % 4 != 0 {
        header_json.push(b' ');
    }

    let mut out = Vec::with_capacity(4 + header_json.len());
    out.extend_from_slice(&(header_json.len() as u32).to_le_bytes());
    out.extend_from_slice(&header_json);

    for mesh in meshes {
        for p in &mesh.positions {
            for &c in p {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        if let Some(normals) = &mesh.normals {
            for n in normals {
                for &c in n {
                    out.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
        for tri in &mesh.indices {
            for &idx in tri {
                out.extend_from_slice(&idx.to_le_bytes());
            }
        }
    }

    out
}

#[tauri::command]
pub async fn get_model_geometry(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<tauri::ipc::Response, String> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;

    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    drop(conn);

    let path = PathBuf::from(file.path);
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or_else(|| "file has no extension".to_string())?;

    let meshes = tauri::async_runtime::spawn_blocking(move || -> CmdResult<Vec<RenderMesh>> {
        match extension.as_str() {
            "stl" => {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                let mesh = stl::parse_stl_geometry(&bytes).map_err(|e| e.to_string())?;
                Ok(vec![mesh])
            }
            "3mf" => threemf::extract_render_meshes_from_path(&path).map_err(|e| e.to_string()),
            other => Err(format!("nicht unterstütztes Dateiformat: {other}")),
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(tauri::ipc::Response::new(encode_render_meshes(&meshes)))
}
```

- [ ] **Step 4: Volle Test-Suite laufen lassen, Erfolg verifizieren**

Run: `cd src-tauri && cargo test --no-default-features -- --nocapture`
Expected: PASS für **alle** Tests im Projekt.

Run zusätzlich: `cd src-tauri && cargo check --no-default-features`
Expected: keine Fehler (bestätigt, dass `get_model_geometry` als `async fn` mit `State<'_, AppState>` korrekt in `lib.rs`s `tauri::generate_handler![...]`-Liste registriert bleibt - dort ist keine Änderung nötig, das Makro behandelt sync/async-Commands identisch).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
Rust: get_model_geometry liefert extrahierte Geometrie statt Rohbytes

get_model_geometry ist jetzt async und delegiert das eigentliche
Entpacken/Parsen an tauri::async_runtime::spawn_blocking, damit eine
grosse Datei nicht den IPC-Dispatch-Thread blockiert. Statt der
rohen Dateibytes (die das Frontend bisher selbst per three.js-Loader
entpacken/parsen musste) liefert der Command jetzt fertige Vertex-/
Normalen-/Index-Puffer in einem kompakten Binaerformat.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

---

### Task 7: Frontend-Binärdecoder (`src/lib/parseModelGeometry.ts`)

**Files:**
- Modify: `src/lib/parseModelGeometry.ts` (kompletter Ersatz des Dateiinhalts)

**Interfaces:**
- Consumes: `ArrayBuffer` im in Task 6 definierten Wire-Format.
- Produces:
  - `export interface ParsedMesh { position: Float32Array; normal: Float32Array | null; index: Uint32Array; }`
  - `export function decodeModelGeometry(buffer: ArrayBuffer): ParsedMesh[]`

  Task 8 (`ModelViewer.tsx`) importiert `decodeModelGeometry`/`ParsedMesh` und ersetzt damit die bisherige `parseModelGeometry`/`ParsedMesh`-Verwendung (three.js-Loader-basiert).

**Hinweis:** Kein neuer JS-Testrunner (siehe Global Constraints) - Verifikation dieses Tasks erfolgt über `npx tsc --noEmit` und den End-to-End-Live-Test in Task 8.

- [ ] **Step 1: Datei komplett ersetzen**

Ersetze den kompletten Inhalt von `src/lib/parseModelGeometry.ts`:

```ts
// Decodiert das von get_model_geometry (src-tauri/src/commands.rs::
// encode_render_meshes) gelieferte Binaerformat direkt in typed-array-
// Views auf den ArrayBuffer - keine three.js-Loader (STLLoader/
// ThreeMFLoader) und damit keine DOMParser-Abhaengigkeit mehr noetig, da
// ZIP-Entpacken und Mesh-Parsing komplett in Rust passieren.
//
// Wire-Format:
// - 4 Bytes: Laenge des JSON-Headers, little-endian u32
// - JSON-Header (mit Leerzeichen auf ein Vielfaches von 4 Bytes
//   aufgepolstert): Array von { vertexCount, hasNormal, indexCount }
// - pro Mesh, in Header-Reihenfolge: Float32-Positionen, optional
//   Float32-Normalen, dann immer Uint32-Indizes. Jeder Abschnitt besteht
//   nur aus 4-Byte-Elementen, daher bleibt der Offset zwischen Meshes
//   automatisch ausgerichtet.

export interface ParsedMesh {
  position: Float32Array;
  normal: Float32Array | null;
  index: Uint32Array;
}

interface MeshHeaderEntry {
  vertexCount: number;
  hasNormal: boolean;
  indexCount: number;
}

export function decodeModelGeometry(buffer: ArrayBuffer): ParsedMesh[] {
  const view = new DataView(buffer);
  const headerLen = view.getUint32(0, true);
  const headerBytes = new Uint8Array(buffer, 4, headerLen);
  const headers: MeshHeaderEntry[] = JSON.parse(new TextDecoder().decode(headerBytes));

  let offset = 4 + headerLen;
  const meshes: ParsedMesh[] = [];

  for (const header of headers) {
    const position = new Float32Array(buffer, offset, header.vertexCount * 3);
    offset += position.byteLength;

    let normal: Float32Array | null = null;
    if (header.hasNormal) {
      normal = new Float32Array(buffer, offset, header.vertexCount * 3);
      offset += normal.byteLength;
    }

    const index = new Uint32Array(buffer, offset, header.indexCount);
    offset += index.byteLength;

    meshes.push({ position, normal, index });
  }

  return meshes;
}
```

- [ ] **Step 2: TypeScript-Kompilierung verifizieren**

Run: `npx tsc --noEmit`
Expected: Fehler in `src/components/ModelViewer.tsx` (importiert noch die alte `parseModelGeometry`-Funktion und das alte `ParsedMesh` mit `matrix`-Feld) - das ist erwartet und wird in Task 8 behoben. Bestätige, dass der Fehler ausschließlich `ModelViewer.tsx` betrifft, nicht `parseModelGeometry.ts` selbst.

- [ ] **Step 3: Commit**

```bash
git add src/lib/parseModelGeometry.ts
git commit -m "$(cat <<'EOF'
Frontend: parseModelGeometry durch reinen Binaerdecoder ersetzt

decodeModelGeometry() legt nur noch typed-array-Views auf den vom
Backend gelieferten ArrayBuffer - keine Kopie der Geometriedaten,
keine three.js-Loader (STLLoader/ThreeMFLoader) und damit keine
DOMParser-Abhaengigkeit mehr im Frontend.

Macht ModelViewer.tsx voruebergehend nicht kompilierbar (folgt im
naechsten Commit).

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

---

### Task 8: `ModelViewer`/`DetailPanel` anpassen und Live-Test

**Files:**
- Modify: `src/components/ModelViewer.tsx`
- Modify: `src/components/DetailPanel.tsx`

**Interfaces:**
- Consumes: `decodeModelGeometry`, `ParsedMesh` aus `src/lib/parseModelGeometry.ts` (Task 7).
- Produces: nichts (Blatt-Task, letzter Schritt des Plans).

- [ ] **Step 1: `ModelViewer.tsx` anpassen**

Ersetze den kompletten Inhalt von `src/components/ModelViewer.tsx`:

```tsx
import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { invoke } from '@tauri-apps/api/core';
import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { decodeModelGeometry } from '../lib/parseModelGeometry';
import type { ParsedMesh } from '../lib/parseModelGeometry';

interface Props {
  fileId: string;
}

function frameObject(object: THREE.Object3D, camera: THREE.PerspectiveCamera, controls: OrbitControls) {
  const box = new THREE.Box3().setFromObject(object);
  const size = box.getSize(new THREE.Vector3());
  const center = box.getCenter(new THREE.Vector3());

  object.position.sub(center);

  const radius = Math.max(size.x, size.y, size.z, 1) * 0.65;
  const distance = radius / Math.sin((camera.fov * Math.PI) / 360);

  camera.position.set(distance, distance * 0.8, distance);
  camera.near = distance / 100;
  camera.far = distance * 100;
  camera.updateProjectionMatrix();

  controls.target.set(0, 0, 0);
  controls.update();
}

function disposeObject(object: THREE.Object3D) {
  object.traverse((child) => {
    if (child instanceof THREE.Mesh) {
      child.geometry.dispose();
    }
  });
}

// Baut aus den von decodeModelGeometry gelieferten Rohdaten eine
// THREE-Objekthierarchie auf. Die Positionen sind bereits weltraum-
// transformiert (Rust liefert sie so) - anders als vor der Umstellung auf
// die native Geometrie-Extraktion ist daher keine Matrix-Handhabung pro
// Mesh mehr noetig, eine flache Gruppe aus Meshes mit Identitaets-
// Transformation genuegt.
function buildGroup(meshes: ParsedMesh[], material: THREE.MeshStandardMaterial): THREE.Group {
  const group = new THREE.Group();
  for (const mesh of meshes) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(mesh.position, 3));
    if (mesh.normal) {
      geometry.setAttribute('normal', new THREE.BufferAttribute(mesh.normal, 3));
    }
    geometry.setIndex(new THREE.BufferAttribute(mesh.index, 1));
    group.add(new THREE.Mesh(geometry, material));
  }
  return group;
}

interface ViewerContext {
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  renderer: THREE.WebGLRenderer;
  controls: OrbitControls;
  material: THREE.MeshStandardMaterial;
  currentObject: THREE.Object3D | null;
}

export function ModelViewer({ fileId }: Props) {
  const t = useT();
  const containerRef = useRef<HTMLDivElement>(null);
  const ctxRef = useRef<ViewerContext | null>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');

  // Renderer/Szene/Kamera/Controls/Licht werden nur einmal beim Mounten
  // aufgebaut und beim Unmounten freigegeben - ein WebGL-Kontext-Neuaufbau
  // ist teuer und bremste bei jedem Modellwechsel spuerbar die ganze App
  // aus. Modellwechsel (zweiter Effekt unten) tauschen nur das angezeigte
  // Objekt in dieser bestehenden Szene aus.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(40, 1, 0.1, 1000);
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    container.appendChild(renderer.domElement);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;

    scene.add(new THREE.AmbientLight(0xffffff, 0.65));
    const key = new THREE.DirectionalLight(0xffffff, 1.1);
    key.position.set(1, 1.4, 1);
    scene.add(key);
    const fill = new THREE.DirectionalLight(0xffffff, 0.4);
    fill.position.set(-1, -0.4, -1);
    scene.add(fill);

    const material = new THREE.MeshStandardMaterial({
      color: 0xd7c9a8,
      roughness: 0.55,
      metalness: 0.05,
    });

    const resize = () => {
      const { clientWidth, clientHeight } = container;
      if (!clientWidth || !clientHeight) return;
      camera.aspect = clientWidth / clientHeight;
      camera.updateProjectionMatrix();
      renderer.setSize(clientWidth, clientHeight);
    };

    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(container);
    resize();

    let frameHandle = 0;
    const animate = () => {
      frameHandle = requestAnimationFrame(animate);
      controls.update();
      renderer.render(scene, camera);
    };
    animate();

    ctxRef.current = { scene, camera, renderer, controls, material, currentObject: null };

    return () => {
      cancelAnimationFrame(frameHandle);
      resizeObserver.disconnect();
      controls.dispose();
      if (ctxRef.current?.currentObject) {
        disposeObject(ctxRef.current.currentObject);
      }
      material.dispose();
      renderer.dispose();
      container.removeChild(renderer.domElement);
      ctxRef.current = null;
    };
  }, []);

  // Modellwechsel: laedt die neue Geometrie und ersetzt nur das Objekt in
  // der bereits bestehenden Szene, statt den ganzen Viewer neu aufzubauen.
  useEffect(() => {
    const ctx = ctxRef.current;
    if (!ctx) return;

    let cancelled = false;
    setStatus('loading');

    invoke<ArrayBuffer>('get_model_geometry', { fileId })
      .then((buffer) => {
        if (cancelled) return;
        const meshes = decodeModelGeometry(buffer);
        const object = buildGroup(meshes, ctx.material);

        if (ctx.currentObject) {
          ctx.scene.remove(ctx.currentObject);
          disposeObject(ctx.currentObject);
        }
        ctx.currentObject = object;
        ctx.scene.add(object);

        const container = containerRef.current;
        if (container && container.clientWidth && container.clientHeight) {
          ctx.camera.aspect = container.clientWidth / container.clientHeight;
          ctx.camera.updateProjectionMatrix();
          ctx.renderer.setSize(container.clientWidth, container.clientHeight);
        }
        frameObject(object, ctx.camera, ctx.controls);
        setStatus('ready');
      })
      .catch((err) => {
        console.error('[ModelViewer] Laden fehlgeschlagen:', err);
        if (!cancelled) setStatus('error');
      });

    return () => {
      cancelled = true;
    };
  }, [fileId]);

  return (
    <div className="relative w-full h-full">
      <div ref={containerRef} className="absolute inset-0" />
      {status === 'loading' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none">
          {t('loadingPreview')}
        </div>
      )}
      {status === 'error' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none px-4 text-center">
          {t('previewUnavailable')}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: `DetailPanel.tsx` anpassen**

In `src/components/DetailPanel.tsx`, finde:

```tsx
          <ModelViewer
            fileId={model.id}
            extension={model.name.split('.').pop()?.toLowerCase() ?? ''}
          />
```

und ersetze durch:

```tsx
          <ModelViewer fileId={model.id} />
```

- [ ] **Step 3: TypeScript-Kompilierung verifizieren**

Run: `npx tsc --noEmit`
Expected: keine Fehler.

- [ ] **Step 4: Commit**

```bash
git add src/components/ModelViewer.tsx src/components/DetailPanel.tsx
git commit -m "$(cat <<'EOF'
Frontend: ModelViewer nutzt native Geometrie-Extraktion statt three.js-Loader

extension-Prop entfaellt komplett (Rust liefert fuer STL und 3MF
dasselbe Format, dispatcht selbst anhand des Dateipfads). buildGroup
baut Meshes jetzt ohne Matrix-Handhabung auf, da die Positionen
bereits weltraum-transformiert ankommen, und ruft geometry.setIndex()
immer statt nur optional auf.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

- [ ] **Step 5: Manueller Live-Test (Abschlusskriterium des gesamten Plans)**

Dieser Schritt lässt sich nicht automatisieren und ist das eigentliche Erfolgskriterium des Plans:

1. `npm run tauri dev` starten, warten bis die App läuft (nur die bekannten, bereits vor diesem Plan bestehenden `dead_code`-Warnungen erwartet, keine neuen Fehler).
2. Die ursprüngliche 194,9-MB-Datei `kakashi PROPORTIONED to Naruto(5).3mf` (35 Objekte) aus dem Bug-Report auswählen.
3. Prüfen: Vorschau lädt spürbar schneller als vorher (Sekunden statt der zuvor beobachteten Blockade), UI bleibt währenddessen bedienbar.
4. Prüfen: Größe/Volumen/Material im Detail-Panel zeigen jetzt Werte statt "–" (Metadaten-Bugfix aus Task 4).
5. Zusätzlich eine kleinere, bereits vorher funktionierende 3MF- und eine STL-Datei auswählen und prüfen, dass die Vorschau weiterhin korrekt aussieht (keine Regression bei Form/Shading).
