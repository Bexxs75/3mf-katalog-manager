# Filamentverbrauch aus gesliceten 3mf-Dateien Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Beim Import oder manuellen "Metadaten neu einlesen" einer 3mf-Datei den von OrcaSlicer/Bambu Studio berechneten Filamentverbrauch (`Metadata/slice_info.config`) auslesen, speichern und auf der Modell-Detailseite statt der bisherigen Grob-Schätzung anzeigen.

**Architecture:** Neues Parser-Modul `threemf::slice_info` (Muster: bestehendes `threemf::plates`) liefert `Option<SliceInfo>` aus dem 3mf-Zip. Wird als JSON in einer neuen `files.slice_info_json`-Spalte gespeichert (Muster: `plate_count`-Migration). `to_dto` liest die Spalte, ersetzt bei Vorhandensein die Gewichtsschätzung durch den realen Wert und liefert die Aufschlüsselung an das Frontend. Ein neuer Command `rescan_file_metadata` liest eine bereits katalogisierte Datei erneut vom Pfad ein und aktualisiert alle davon abhängigen Spalten.

**Tech Stack:** Rust/Tauri (rusqlite, quick-xml, zip, serde_json) Backend; React/TypeScript Frontend, kein Test-Runner im Frontend (Verifikation über `npx tsc --noEmit`).

## Global Constraints

- Kein Drucker-/Cloud-Zugriff, kein Headless-Slicing, kein automatischer Lagerabzug (bewusst außerhalb des Scopes, siehe Spec).
- `slice_info.config` fehlt/kaputt → `None`, niemals ein Fehler (gleiches Verhalten wie bestehendes `count_plates`/Thumbnail-Fallback).
- Neue DB-Spalte per idempotenter `ALTER TABLE ... ADD COLUMN` (kein Migrations-Framework in diesem Projekt) UND in `schema.sql` für frische Installationen.
- Deutsch ist die Quellsprache für UI-Texte; jede neue `i18n`-Textkonstante braucht Einträge in allen vier Sprachdateien (`de.ts`, `en.ts`, `es.ts`, `fr.ts`).
- Rescan überschreibt alle abgeleiteten Felder (Maße, Volumen, Materialien, Metadaten, Thumbnail, Plattenzahl, Slice-Info) bedingungslos mit dem aktuellen Datei-Inhalt — "neu einlesen" heißt vollständiger Refresh, kein Merge.

---

### Task 1: Slice-Info-Parser (`threemf::slice_info`)

**Files:**
- Create: `src-tauri/src/threemf/slice_info.rs`
- Modify: `src-tauri/src/threemf/mod.rs:1-5` (Modul registrieren)

**Interfaces:**
- Produces: `pub struct SliceInfo { pub total_weight_g: f64, pub plates: Vec<PlateFilamentUsage> }`, `pub struct PlateFilamentUsage { pub plate_index: u32, pub weight_g: f64, pub filaments: Vec<FilamentUsage> }`, `pub struct FilamentUsage { pub filament_type: String, pub color: Option<String>, pub used_g: f64, pub used_m: f64 }` (alle `#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]`), `pub fn parse_slice_info<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Option<SliceInfo>`.

- [ ] **Step 1: Write the failing tests**

```rust
// src-tauri/src/threemf/slice_info.rs
use std::io::{Read, Seek};

use quick_xml::events::{BytesStart, Event};
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
            a.unescape_value().ok().map(|v| v.into_owned())
        } else {
            None
        }
    })
}

/// Liest `Metadata/slice_info.config` (Bambu Studio/OrcaSlicer-spezifisch,
/// kein Teil des offiziellen 3MF-Standards) aus dem bereits geoeffneten
/// Zip-Archiv. Existiert die Datei nicht, laesst sie sich nicht als XML
/// lesen, oder enthaelt sie keine `<plate>`-Elemente, wird `None`
/// zurueckgegeben - kein Fehlerfall, gleiches Verhalten wie `count_plates`.
pub fn parse_slice_info<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Option<SliceInfo> {
    let mut file = archive.by_name("Metadata/slice_info.config").ok()?;
    let mut xml = String::new();
    file.read_to_string(&mut xml).ok()?;
    drop(file);

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
            Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }

    if plates.is_empty() {
        return None;
    }

    let total_weight_g = plates.iter().map(|p| p.weight_g).sum();
    Some(SliceInfo { total_weight_g, plates })
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

    const SINGLE_PLATE_SINGLE_FILAMENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="weight" value="15.83"/>
    <object identify_id="1" name="Shape.stl" skipped="false"/>
    <filament id="1" tray_info_idx="GFA00" type="PLA" color="#FFFFFFFF" used_m="14.5" used_g="43.24"/>
  </plate>
</config>"#;

    #[test]
    fn parses_single_plate_single_filament() {
        let mut archive = build_zip(&[("Metadata/slice_info.config", SINGLE_PLATE_SINGLE_FILAMENT)]);
        let info = parse_slice_info(&mut archive).expect("slice info present");

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

    const MULTICOLOR_PLATE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="weight" value="15.20"/>
    <filament id="1" type="PLA" color="#FFFFFFFF" used_m="10.5" used_g="10.43"/>
    <filament id="2" type="PLA" color="#000000FF" used_m="4.8" used_g="4.77"/>
  </plate>
</config>"#;

    #[test]
    fn parses_multicolor_plate_with_multiple_filaments() {
        let mut archive = build_zip(&[("Metadata/slice_info.config", MULTICOLOR_PLATE)]);
        let info = parse_slice_info(&mut archive).expect("slice info present");

        assert_eq!(info.plates[0].filaments.len(), 2);
        assert_eq!(info.plates[0].filaments[1].color.as_deref(), Some("#000000FF"));
    }

    const TWO_PLATES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
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
</config>"#;

    #[test]
    fn parses_multiple_plates_and_sums_total_weight() {
        let mut archive = build_zip(&[("Metadata/slice_info.config", TWO_PLATES)]);
        let info = parse_slice_info(&mut archive).expect("slice info present");

        assert_eq!(info.plates.len(), 2);
        assert!((info.total_weight_g - 15.00).abs() < 1e-6);
        assert_eq!(info.plates[1].plate_index, 2);
        assert_eq!(info.plates[1].filaments[0].filament_type, "PETG");
    }

    #[test]
    fn falls_back_to_summed_filament_weight_when_weight_metadata_missing() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <filament id="1" type="PLA" color="#FFFFFFFF" used_m="4.0" used_g="12.5"/>
  </plate>
</config>"#;
        let mut archive = build_zip(&[("Metadata/slice_info.config", xml)]);
        let info = parse_slice_info(&mut archive).expect("slice info present");

        assert!((info.plates[0].weight_g - 12.5).abs() < 1e-6);
    }

    #[test]
    fn returns_none_when_config_missing() {
        let mut archive = build_zip(&[("3D/3dmodel.model", "<model></model>")]);
        assert!(parse_slice_info(&mut archive).is_none());
    }

    #[test]
    fn returns_none_when_config_is_not_valid_xml() {
        let mut archive = build_zip(&[("Metadata/slice_info.config", "not xml at all <<<")]);
        assert!(parse_slice_info(&mut archive).is_none());
    }

    #[test]
    fn returns_none_when_no_plate_elements_present() {
        let mut archive = build_zip(&[("Metadata/slice_info.config", "<config></config>")]);
        assert!(parse_slice_info(&mut archive).is_none());
    }

    #[test]
    fn ignores_namespace_prefix_on_plate_elements() {
        let xml = r#"<config><p:plate xmlns:p="urn:x">
          <p:metadata key="weight" value="1.5"/>
          <p:filament type="PLA" used_g="1.5" used_m="0.5"/>
        </p:plate></config>"#;
        let mut archive = build_zip(&[("Metadata/slice_info.config", xml)]);
        let info = parse_slice_info(&mut archive).expect("slice info present");
        assert_eq!(info.plates.len(), 1);
    }
}
```

- [ ] **Step 2: Register the module**

In `src-tauri/src/threemf/mod.rs`, add to the module list at the top:

```rust
pub mod slice_info;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cd src-tauri && cargo test slice_info`
Expected: all 8 tests in `threemf::slice_info::tests` PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/threemf/slice_info.rs src-tauri/src/threemf/mod.rs
git commit -m "Parser fuer slice_info.config (Filamentverbrauch aus OrcaSlicer/Bambu Studio)"
```

---

### Task 2: Slice-Info in `PackageParts`/`ThreeMfDocument` verdrahten

**Files:**
- Modify: `src-tauri/src/threemf/container.rs:21-103`
- Modify: `src-tauri/src/threemf/mod.rs:16-145`

**Interfaces:**
- Consumes: `threemf::slice_info::{SliceInfo, parse_slice_info}` (Task 1).
- Produces: `PackageParts.slice_info: Option<SliceInfo>`, `ThreeMfDocument.slice_info: Option<SliceInfo>`.

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/threemf/mod.rs`, inside `#[cfg(test)] mod tests`, add:

```rust
#[test]
fn parses_slice_info_when_present() {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let slice_info_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="weight" value="12.40"/>
    <filament id="1" type="PLA" color="#FF8800FF" used_m="5.0" used_g="12.40"/>
  </plate>
</config>"#;

    let mut buf = build_test_3mf();
    // build_test_3mf() liefert bereits fertige Zip-Bytes - fuer diesen Test
    // wird stattdessen ein eigenes Archiv mit zusaetzlichem Slice-Info-Eintrag
    // gebaut, da ZipWriter nicht nachtraeglich in fertige Bytes einfuegen kann.
    buf.clear();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
        zip.start_file("_rels/.rels", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
        zip.start_file("3D/3dmodel.model", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
        zip.start_file("Metadata/slice_info.config", options).unwrap();
        zip.write_all(slice_info_xml.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    let doc = parse_3mf_bytes(&buf).expect("parse should succeed");
    let slice_info = doc.slice_info.expect("slice info present");
    assert!((slice_info.total_weight_g - 12.40).abs() < 1e-6);
}

#[test]
fn slice_info_is_none_when_absent() {
    let bytes = build_test_3mf();
    let doc = parse_3mf_bytes(&bytes).expect("parse should succeed");
    assert!(doc.slice_info.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test slice_info_is_none_when_absent parses_slice_info_when_present`
Expected: FAIL with "no field `slice_info` on type `ThreeMfDocument`".

- [ ] **Step 3: Wire `slice_info` into `PackageParts`**

In `src-tauri/src/threemf/container.rs`, add the field:

```rust
pub struct PackageParts {
    pub root_model: ParsedModel,
    pub referenced_models: HashMap<String, ParsedModel>,
    pub thumbnail: Option<Vec<u8>>,
    pub plate_count: Option<u32>,
    pub slice_info: Option<super::slice_info::SliceInfo>,
}
```

In `read_package`, right after the existing `let plate_count = ...` line:

```rust
let plate_count = super::plates::count_plates(&mut archive);
let slice_info = super::slice_info::parse_slice_info(&mut archive);
```

And add `slice_info,` to the `Ok(PackageParts { ... })` construction at the end of `read_package`.

- [ ] **Step 4: Wire `slice_info` into `ThreeMfDocument`**

In `src-tauri/src/threemf/mod.rs`, add the field and re-export the type:

```rust
pub use slice_info::SliceInfo;

#[derive(Debug, Clone)]
pub struct ThreeMfDocument {
    pub object_count: usize,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub materials: Vec<ThreeMfMaterial>,
    pub metadata: BTreeMap<String, String>,
    pub thumbnail_png: Option<Vec<u8>>,
    pub plate_count: Option<u32>,
    pub slice_info: Option<SliceInfo>,
}
```

In `parse_3mf_reader`, add `slice_info: package.slice_info.clone(),` to the `Ok(ThreeMfDocument { ... })` construction.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib threemf`
Expected: all `threemf::*` tests PASS, including the two new ones.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/threemf/container.rs src-tauri/src/threemf/mod.rs
git commit -m "3mf-Parser liefert Slice-Info (Filamentverbrauch) mit"
```

---

### Task 3: DB-Spalte `slice_info_json`

**Files:**
- Modify: `src-tauri/src/db/schema.sql:47` (frische Installationen)
- Modify: `src-tauri/src/db/repository.rs:79` (Migration Bestands-DB), `:224-296` (`insert_file`), `:346-351` (`TRASH_SELECT_COLUMNS`), `:402-463` (`get_file`, `list_files_by_ids`), `:465-485` (`list_files`), `:487-529` (`row_to_file`)
- Modify: `src-tauri/src/db/models.rs:33-61` (`NewFile`, `FileRecord`)
- Modify: `src-tauri/src/db/mod.rs:76-98` (Test-Fixture + Roundtrip-Test)

**Interfaces:**
- Produces: `NewFile.slice_info_json: Option<String>`, `FileRecord.slice_info_json: Option<String>`, Spalte `files.slice_info_json TEXT`.

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/db/mod.rs`, add to `sample_file()` the new field `slice_info_json: None,` (siehe Step 4 unten fuer den vollstaendigen Struct-Zusatz), dann:

```rust
#[test]
fn insert_and_get_file_roundtrips_slice_info_json() {
    let mut conn = connect_in_memory().expect("connect");
    let mut file = sample_file();
    file.slice_info_json = Some(r#"{"total_weight_g":12.4,"plates":[]}"#.to_string());
    let id = insert_file(&mut conn, &file).expect("insert");
    let fetched = get_file(&conn, id).expect("query").expect("present");
    assert_eq!(fetched.slice_info_json, Some(r#"{"total_weight_g":12.4,"plates":[]}"#.to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test insert_and_get_file_roundtrips_slice_info_json`
Expected: FAIL with "no field `slice_info_json` on type `NewFile`" (compile error).

- [ ] **Step 3: Add the column to `schema.sql`**

In `src-tauri/src/db/schema.sql`, change:

```sql
    plate_count INTEGER,
    deleted_at TEXT,
    trash_path TEXT
```

to:

```sql
    plate_count INTEGER,
    slice_info_json TEXT,
    deleted_at TEXT,
    trash_path TEXT
```

- [ ] **Step 4: Add the migration in `repository.rs`**

In `src-tauri/src/db/repository.rs`, in `init()`, right after the existing `plate_count`-ALTER line:

```rust
let _ = conn.execute("ALTER TABLE files ADD COLUMN plate_count INTEGER", []);
let _ = conn.execute("ALTER TABLE files ADD COLUMN slice_info_json TEXT", []);
```

- [ ] **Step 5: Extend `NewFile` and `FileRecord`**

In `src-tauri/src/db/models.rs`, add `pub slice_info_json: Option<String>,` as the last field of both `NewFile` (after `pub plate_count: Option<i64>,`) and `FileRecord` (after `pub plate_count: Option<i64>,`).

- [ ] **Step 6: Extend `insert_file`, the SELECT column lists, and `row_to_file`**

In `src-tauri/src/db/repository.rs`:

`insert_file` — extend the column list and placeholders to 27 total, add `file.slice_info_json,` as the last bound value:

```rust
tx.execute(
    "INSERT INTO files (
        name, path, file_type, folder_id, origin, cloud_id, sync_status,
        file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
        volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
        print_status, last_viewed_at, creator, content_hash,
        render_snapshot_png, custom_image_png, source_url, queue_position, favorite,
        plate_count, slice_info_json
    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)",
    params![
        file.name, file.path, file.file_type.as_str(), file.folder_id, file.origin, file.cloud_id,
        file.sync_status, file.file_size_bytes, dim_x, dim_y, dim_z, file.volume_cm3, file.object_count,
        file.thumbnail_png, file.imported_at, file.file_modified_at, file.print_status, file.last_viewed_at,
        file.creator, file.content_hash, file.render_snapshot_png, file.custom_image_png, file.source_url,
        file.queue_position, file.favorite, file.plate_count, file.slice_info_json,
    ],
)?;
```

`TRASH_SELECT_COLUMNS`, das `get_file`-SELECT und das `list_files_by_ids`/`list_files`-SELECT: jeweils `slice_info_json` nach `plate_count` einfuegen, z.B. in `TRASH_SELECT_COLUMNS`:

```rust
const TRASH_SELECT_COLUMNS: &str = "id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
     file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
     volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
     print_status, last_viewed_at, creator, content_hash,
     render_snapshot_png, custom_image_png, source_url, queue_position, favorite,
     plate_count, slice_info_json, deleted_at, trash_path";
```

Gleiche Ergänzung (`plate_count, slice_info_json, deleted_at, trash_path`) in den SELECT-Statements von `get_file` und `list_files_by_ids`, sowie (`plate_count, slice_info_json` vor `deleted_at, trash_path`) in `list_files`.

`row_to_file`: die Spaltenindizes verschieben sich ab `deleted_at`/`trash_path` um eins nach hinten, `slice_info_json` liegt bei Index 26 (direkt nach `plate_count` bei 26 → `plate_count` bleibt 26, `slice_info_json` wird 27, `deleted_at` wird 28, `trash_path` wird 29):

```rust
        plate_count: row.get(26)?,
        slice_info_json: row.get(27)?,
        deleted_at: row.get(28)?,
        trash_path: row.get(29)?,
```

- [ ] **Step 7: Update the test fixture**

In `src-tauri/src/db/mod.rs`, in `sample_file()`, add `slice_info_json: None,` as the last field of the returned `NewFile`.

- [ ] **Step 8: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib db`
Expected: all `db::*` tests PASS, including `insert_and_get_file_roundtrips_slice_info_json`.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/db/schema.sql src-tauri/src/db/repository.rs src-tauri/src/db/models.rs src-tauri/src/db/mod.rs
git commit -m "DB: neue Spalte files.slice_info_json fuer Filamentverbrauch"
```

---

### Task 4: `import_one` befuellt `slice_info_json`

**Files:**
- Modify: `src-tauri/src/commands.rs:719-816` (`import_one`)

**Interfaces:**
- Consumes: `NewFile.slice_info_json` (Task 3), `ThreeMfDocument.slice_info` (Task 2).

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/commands.rs`, in `mod tests`, add (this test writes a real temp 3mf file to disk, since `import_one` reads from a path):

```rust
#[test]
fn import_one_stores_slice_info_json_when_present() {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let slice_info_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="weight" value="9.90"/>
    <filament id="1" type="PLA" color="#112233FF" used_m="3.0" used_g="9.90"/>
  </plate>
</config>"#;

    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
        zip.start_file("_rels/.rels", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
        zip.start_file("3D/3dmodel.model", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
        zip.start_file("Metadata/slice_info.config", options).unwrap();
        zip.write_all(slice_info_xml.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("import_one_slice_info_test_{nanos}.3mf"));
    std::fs::write(&path, &buf).expect("write temp file");

    let mut conn = crate::db::connect_in_memory().expect("connect");
    let dto = import_one(&mut conn, &path, None, None).expect("import should succeed");

    let stored = crate::db::get_file(&conn, dto.id.parse().unwrap()).expect("query").expect("present");
    assert!(stored.slice_info_json.is_some());
    assert!(stored.slice_info_json.unwrap().contains("9.9"));

    let _ = std::fs::remove_file(&path);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test import_one_stores_slice_info_json_when_present`
Expected: FAIL — `stored.slice_info_json` is `None` (field exists from Task 3, but `import_one` never sets it yet).

- [ ] **Step 3: Populate `slice_info_json` in `import_one`**

In `src-tauri/src/commands.rs`, extend the `match extension.as_deref()` block in `import_one` to also destructure `slice_info`:

```rust
    let (
        file_type,
        dimensions_mm,
        volume_cm3,
        object_count,
        materials,
        metadata,
        thumbnail_png,
        plate_count,
        slice_info_json,
    ) = match extension.as_deref() {
        Some("3mf") => {
            let doc = threemf::parse_3mf_file(path).map_err(|e| e.to_string())?;
            (
                FileType::ThreeMf,
                doc.dimensions_mm,
                doc.volume_cm3,
                Some(doc.object_count as i64),
                doc.materials
                    .into_iter()
                    .map(|m| MaterialRecord {
                        name: m.name,
                        display_color: m.display_color,
                    })
                    .collect::<Vec<_>>(),
                doc.metadata,
                doc.thumbnail_png,
                doc.plate_count.map(|c| c as i64),
                doc.slice_info.and_then(|s| serde_json::to_string(&s).ok()),
            )
        }
        Some("stl") => {
            let doc = stl::parse_stl_file(path).map_err(|e| e.to_string())?;
            (
                FileType::Stl,
                doc.dimensions_mm,
                doc.volume_cm3,
                None,
                Vec::new(),
                BTreeMap::new(),
                None,
                None,
                None,
            )
        }
        _ => return Err("nicht unterstütztes Dateiformat".to_string()),
    };
```

And add `slice_info_json,` as the last field of the `NewFile { ... }` construction further down in the same function.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test import_one_stores_slice_info_json_when_present`
Expected: PASS.

- [ ] **Step 5: Run the full backend test suite**

Run: `cd src-tauri && cargo test`
Expected: all tests PASS (no regressions in existing `import_one` callers).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "Import speichert Slice-Info (Filamentverbrauch) aus 3mf-Dateien"
```

---

### Task 5: `to_dto` liefert Slice-Info + `weightSource`

**Files:**
- Modify: `src-tauri/src/commands.rs:26-59` (DTOs), `:162-207` (`to_dto`)

**Interfaces:**
- Consumes: `FileRecord.slice_info_json` (Task 3), `threemf::slice_info::SliceInfo` (Task 1, fuer Deserialisierung).
- Produces: `ModelFileDto.weight_source: String`, `ModelFileDto.slice_info: Option<SliceInfoDto>`.

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/commands.rs`, in `mod tests`, add:

```rust
#[test]
fn to_dto_uses_slicer_weight_and_marks_source_when_slice_info_present() {
    let mut file = sample_file_record(1, None, "2026-09-13T00:00:00Z");
    file.volume_cm3 = Some(100.0); // wuerde ohne slice_info eine Schaetzung liefern
    file.slice_info_json = Some(
        r#"{"total_weight_g":42.5,"plates":[{"plate_index":1,"weight_g":42.5,"filaments":[{"filament_type":"PLA","color":"#FFFFFFFF","used_g":42.5,"used_m":15.0}]}]}"#
            .to_string(),
    );

    let dto = to_dto(file);

    assert_eq!(dto.weight_source, "slicer");
    assert!((dto.estimated_weight_g.expect("weight") - 42.5).abs() < 1e-6);
    let slice_info = dto.slice_info.expect("slice info dto present");
    assert_eq!(slice_info.plates.len(), 1);
    assert_eq!(slice_info.plates[0].filaments[0].filament_type, "PLA");
}

#[test]
fn to_dto_falls_back_to_estimate_when_slice_info_absent() {
    let mut file = sample_file_record(2, None, "2026-09-13T00:00:00Z");
    file.volume_cm3 = Some(10.0);
    file.slice_info_json = None;

    let dto = to_dto(file);

    assert_eq!(dto.weight_source, "estimated");
    assert!(dto.slice_info.is_none());
    assert!(dto.estimated_weight_g.is_some());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test to_dto_uses_slicer_weight to_dto_falls_back_to_estimate`
Expected: FAIL with "no field `weight_source`/`slice_info` on type `ModelFileDto`".

- [ ] **Step 3: Add the DTOs**

In `src-tauri/src/commands.rs`, right after `pub struct MaterialDto { ... }`:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilamentUsageDto {
    #[serde(rename = "type")]
    pub filament_type: String,
    pub color: Option<String>,
    pub used_g: f64,
    pub used_m: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlateFilamentUsageDto {
    pub plate_index: u32,
    pub weight_g: f64,
    pub filaments: Vec<FilamentUsageDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceInfoDto {
    pub total_weight_g: f64,
    pub plates: Vec<PlateFilamentUsageDto>,
}

impl From<threemf::SliceInfo> for SliceInfoDto {
    fn from(info: threemf::SliceInfo) -> Self {
        SliceInfoDto {
            total_weight_g: info.total_weight_g,
            plates: info
                .plates
                .into_iter()
                .map(|p| PlateFilamentUsageDto {
                    plate_index: p.plate_index,
                    weight_g: p.weight_g,
                    filaments: p
                        .filaments
                        .into_iter()
                        .map(|f| FilamentUsageDto {
                            filament_type: f.filament_type,
                            color: f.color,
                            used_g: f.used_g,
                            used_m: f.used_m,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}
```

And extend `ModelFileDto` (after `pub plate_count: Option<i64>,`):

```rust
    pub plate_count: Option<i64>,
    pub weight_source: String,
    pub slice_info: Option<SliceInfoDto>,
    pub deleted_at: Option<String>,
```

- [ ] **Step 4: Update `to_dto`**

In `src-tauri/src/commands.rs`, replace the start of `to_dto`:

```rust
pub(crate) fn to_dto(file: FileRecord) -> ModelFileDto {
    let slice_info: Option<threemf::SliceInfo> = file
        .slice_info_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());
    let (estimated_weight_g, weight_source) = match &slice_info {
        Some(info) => (Some(info.total_weight_g), "slicer".to_string()),
        None => (
            estimate_weight_g(file.volume_cm3, file.materials.first().map(|m| m.name.as_str())),
            "estimated".to_string(),
        ),
    };
    let custom_image = encode_image(file.custom_image_png);
    let thumbnail_image = encode_image(file.thumbnail_png);
    let render_snapshot_image = encode_image(file.render_snapshot_png);
    ModelFileDto {
        // ... (unveraendert bis zum Ende) ...
```

Entferne die bisherige `let estimated_weight_g = estimate_weight_g(...)`-Zeile (wird jetzt oben im `match` berechnet) und ergaenze am Ende der `ModelFileDto { ... }`-Konstruktion (nach `plate_count: file.plate_count,`):

```rust
        plate_count: file.plate_count,
        weight_source,
        slice_info: slice_info.map(SliceInfoDto::from),
        deleted_at: file.deleted_at,
```

(Die bisherige Position von `deleted_at: file.deleted_at,` entsprechend entfernen, falls sie vorher an anderer Stelle stand — sie bleibt das letzte Feld.)

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib commands`
Expected: all `commands::*` tests PASS, including the two new ones.

- [ ] **Step 6: Run the full backend test suite**

Run: `cd src-tauri && cargo test`
Expected: PASS. Falls `sample_file_record` in den bestehenden Tests kein `slice_info_json`-Feld setzt, ergänze es dort mit `slice_info_json: None,` (Compile-Fehler zeigt die Stelle).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "ModelFileDto liefert Slice-Info und weight_source ans Frontend"
```

---

### Task 6: Command `rescan_file_metadata`

**Files:**
- Modify: `src-tauri/src/commands.rs` (neue Funktion + Command, nahe `import_one`)
- Modify: `src-tauri/src/lib.rs:81` (Command registrieren)

**Interfaces:**
- Consumes: `import_one`s Parser-Aufrufe (Task 4 Pattern), `to_dto` (Task 5).
- Produces: `pub(crate) fn rescan_file(conn: &mut Connection, id: i64) -> CmdResult<ModelFileDto>`, `#[tauri::command] pub fn rescan_file_metadata(state: State<AppState>, file_id: String) -> CmdResult<ModelFileDto>`.

- [ ] **Step 1: Write the failing tests**

In `src-tauri/src/commands.rs`, in `mod tests`, add:

```rust
#[test]
fn rescan_file_updates_plate_count_and_slice_info_from_current_disk_contents() {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn write_3mf(path: &std::path::Path, slice_info_xml: Option<&str>) {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();
            zip.start_file("[Content_Types].xml", options).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
            zip.start_file("_rels/.rels", options).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
            zip.start_file("3D/3dmodel.model", options).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
            if let Some(xml) = slice_info_xml {
                zip.start_file("Metadata/slice_info.config", options).unwrap();
                zip.write_all(xml.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        std::fs::write(path, &buf).expect("write temp file");
    }

    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("rescan_test_{nanos}.3mf"));
    write_3mf(&path, None);

    let mut conn = crate::db::connect_in_memory().expect("connect");
    let imported = import_one(&mut conn, &path, None, None).expect("initial import");
    let id: i64 = imported.id.parse().unwrap();
    assert_eq!(imported.weight_source, "estimated");

    let slice_info_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="weight" value="7.70"/>
    <filament id="1" type="PLA" color="#00FF00FF" used_m="2.5" used_g="7.70"/>
  </plate>
</config>"#;
    write_3mf(&path, Some(slice_info_xml));

    let rescanned = rescan_file(&mut conn, id).expect("rescan should succeed");
    assert_eq!(rescanned.weight_source, "slicer");
    assert!((rescanned.estimated_weight_g.expect("weight") - 7.70).abs() < 1e-6);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn rescan_file_returns_error_when_file_missing_on_disk() {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("rescan_missing_test_{nanos}.3mf"));
    // Nie geschrieben - Datei existiert nicht auf der Platte.

    let mut conn = crate::db::connect_in_memory().expect("connect");
    let mut new_file = sample_new_file_for_rescan_test(&path);
    new_file.file_type = FileType::ThreeMf;
    let id = crate::db::insert_file(&mut conn, &new_file).expect("insert");

    let result = rescan_file(&mut conn, id);
    assert!(result.is_err());
}

/// Minimaler `NewFile` fuer den Fehlerfall-Test oben - nur Pfad/Typ sind
/// relevant, alle anderen Felder sind fuer `rescan_file` irrelevant, da die
/// Funktion bei fehlender Datei abbricht, bevor sie sie liest.
fn sample_new_file_for_rescan_test(path: &std::path::Path) -> NewFile {
    NewFile {
        name: "missing.3mf".to_string(),
        path: path.to_string_lossy().to_string(),
        file_type: FileType::ThreeMf,
        folder_id: None,
        origin: "local".to_string(),
        cloud_id: None,
        sync_status: "local-only".to_string(),
        file_size_bytes: 0,
        dimensions_mm: None,
        volume_cm3: None,
        object_count: None,
        thumbnail_png: None,
        imported_at: "2026-09-13T00:00:00Z".to_string(),
        file_modified_at: None,
        materials: Vec::new(),
        metadata: BTreeMap::new(),
        tags: Vec::new(),
        print_status: "not_printed".to_string(),
        last_viewed_at: None,
        creator: None,
        content_hash: None,
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
        queue_position: None,
        favorite: false,
        plate_count: None,
        slice_info_json: None,
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test rescan_file`
Expected: FAIL with "cannot find function `rescan_file`".

- [ ] **Step 3: Implement `rescan_file` and the `NewFile`-analoge Update-Struktur**

In `src-tauri/src/db/models.rs`, füge nach `NewFile` hinzu:

```rust
/// Felder, die `rescan_file` (commands.rs) nach dem erneuten Einlesen einer
/// bereits katalogisierten Datei unbedingt ueberschreibt - "neu einlesen"
/// ist ein voller Refresh, kein Merge mit dem alten Zustand.
pub struct ScannedMetadataUpdate {
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub thumbnail_png: Option<Vec<u8>>,
    pub plate_count: Option<i64>,
    pub slice_info_json: Option<String>,
    pub materials: Vec<MaterialRecord>,
    pub metadata: BTreeMap<String, String>,
}
```

In `src-tauri/src/db/repository.rs`, füge hinzu:

```rust
pub fn update_scanned_metadata(
    conn: &mut Connection,
    file_id: i64,
    update: &super::models::ScannedMetadataUpdate,
) -> Result<(), DbError> {
    let tx = conn.transaction()?;

    let [dim_x, dim_y, dim_z] = match update.dimensions_mm {
        Some(d) => [Some(d[0]), Some(d[1]), Some(d[2])],
        None => [None, None, None],
    };

    tx.execute(
        "UPDATE files SET
            dimension_x_mm = ?1, dimension_y_mm = ?2, dimension_z_mm = ?3,
            volume_cm3 = ?4, object_count = ?5, thumbnail_png = ?6,
            plate_count = ?7, slice_info_json = ?8
         WHERE id = ?9",
        params![
            dim_x, dim_y, dim_z, update.volume_cm3, update.object_count,
            update.thumbnail_png, update.plate_count, update.slice_info_json, file_id,
        ],
    )?;

    tx.execute("DELETE FROM file_materials WHERE file_id = ?1", params![file_id])?;
    for material in &update.materials {
        tx.execute(
            "INSERT INTO file_materials (file_id, name, display_color) VALUES (?1, ?2, ?3)",
            params![file_id, material.name, material.display_color],
        )?;
    }

    tx.execute("DELETE FROM file_metadata WHERE file_id = ?1", params![file_id])?;
    for (label, value) in &update.metadata {
        tx.execute(
            "INSERT INTO file_metadata (file_id, label, value) VALUES (?1, ?2, ?3)",
            params![file_id, label, value],
        )?;
    }

    tx.commit()?;
    Ok(())
}
```

In `src-tauri/src/db/mod.rs`, `update_scanned_metadata` zur `pub use repository::{...};`-Liste hinzufuegen.

In `src-tauri/src/commands.rs`, füge nach `import_one` hinzu (und importiere `db::models::ScannedMetadataUpdate` oben im `use`-Block):

```rust
/// Liest die Datei einer bereits katalogisierten `FileRecord` erneut vom
/// gespeicherten Pfad ein und ueberschreibt alle davon abgeleiteten Spalten
/// (Maße, Volumen, Materialien, Metadaten, Thumbnail, Plattenzahl,
/// Slice-Info) - fuer den Fall, dass der Nutzer die Datei inzwischen in
/// OrcaSlicer/Bambu Studio gesliced und am selben Pfad ueberschrieben hat.
/// Existiert die Datei am Pfad nicht mehr, bricht die Funktion mit einem
/// Fehler ab, BEVOR irgendetwas in der DB veraendert wird.
pub(crate) fn rescan_file(conn: &mut Connection, id: i64) -> CmdResult<ModelFileDto> {
    let existing = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Datei nicht im Katalog gefunden".to_string())?;
    let path = Path::new(&existing.path);
    if !path.exists() {
        return Err(format!("Datei nicht gefunden: {}", existing.path));
    }
    let extension = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase());

    let update = match extension.as_deref() {
        Some("3mf") => {
            let doc = threemf::parse_3mf_file(path).map_err(|e| e.to_string())?;
            ScannedMetadataUpdate {
                dimensions_mm: doc.dimensions_mm,
                volume_cm3: doc.volume_cm3,
                object_count: Some(doc.object_count as i64),
                thumbnail_png: doc.thumbnail_png,
                plate_count: doc.plate_count.map(|c| c as i64),
                slice_info_json: doc.slice_info.and_then(|s| serde_json::to_string(&s).ok()),
                materials: doc
                    .materials
                    .into_iter()
                    .map(|m| MaterialRecord { name: m.name, display_color: m.display_color })
                    .collect(),
                metadata: doc.metadata,
            }
        }
        Some("stl") => {
            let doc = stl::parse_stl_file(path).map_err(|e| e.to_string())?;
            ScannedMetadataUpdate {
                dimensions_mm: doc.dimensions_mm,
                volume_cm3: doc.volume_cm3,
                object_count: None,
                thumbnail_png: None,
                plate_count: None,
                slice_info_json: None,
                materials: Vec::new(),
                metadata: BTreeMap::new(),
            }
        }
        _ => return Err("nicht unterstütztes Dateiformat".to_string()),
    };

    db::update_scanned_metadata(conn, id, &update).map_err(|e| e.to_string())?;
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Datei nach Aktualisierung nicht mehr gefunden".to_string())?;
    Ok(to_dto(file))
}

#[tauri::command]
pub fn rescan_file_metadata(state: State<AppState>, file_id: String) -> CmdResult<ModelFileDto> {
    let id: i64 = file_id.parse().map_err(|_| "ungueltige Datei-ID".to_string())?;
    let mut conn = lock_db(&state)?;
    rescan_file(&mut conn, id)
}
```

- [ ] **Step 4: Register the command in `lib.rs`**

In `src-tauri/src/lib.rs`, im `tauri::generate_handler![...]`-Block, nach `commands::set_source_url,`:

```rust
            commands::set_source_url,
            commands::rescan_file_metadata,
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test rescan_file`
Expected: both new tests PASS.

- [ ] **Step 6: Run the full backend test suite and build**

Run: `cd src-tauri && cargo test && cargo build`
Expected: all tests PASS, release-relevant build succeeds without new warnings besides pre-existing ones.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/db/models.rs src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs src-tauri/src/lib.rs
git commit -m "Neuer Command rescan_file_metadata: Datei erneut vom Pfad einlesen"
```

---

### Task 7: Frontend-Typen erweitern

**Files:**
- Modify: `src/types/index.ts:5-31`

**Interfaces:**
- Consumes: JSON-Form von `ModelFileDto` (Task 5) — camelCase, `filaments[].type` (nicht `filamentType`).
- Produces: `ModelFile.weightSource`, `ModelFile.sliceInfo`.

- [ ] **Step 1: Add the types**

In `src/types/index.ts`, ergänze `ModelFile` (nach `estimatedWeightG: number | null;`):

```ts
export interface FilamentUsage {
  type: string;
  color: string | null;
  usedG: number;
  usedM: number;
}

export interface PlateFilamentUsage {
  plateIndex: number;
  weightG: number;
  filaments: FilamentUsage[];
}

export interface SliceInfo {
  totalWeightG: number;
  plates: PlateFilamentUsage[];
}

export interface ModelFile {
  id: string;
  name: string;
  path: string;
  folderId: string;
  tags: string[];
  origin: Origin;
  sync: SyncStatus;
  dimensionsMm: [number, number, number] | null;
  volumeCm3: number | null;
  objectCount: number | null;
  plateCount: number | null;
  materials: { name: string; displayColor: string | null }[];
  fileSizeBytes: number;
  importedAt: string;
  printStatus: 'not_printed' | 'printed';
  estimatedWeightG: number | null;
  weightSource: 'slicer' | 'estimated';
  sliceInfo: SliceInfo | null;
  lastViewedAt: string | null;
  creator: string | null;
  customImage: string | null;
  thumbnailImage: string | null;
  renderSnapshotImage: string | null;
  sourceUrl: string | null;
  queuePosition: number | null;
  favorite: boolean;
  deletedAt: string | null;
}
```

(Die bereits vorhandene `ModelFile`-Definition wird durch die obige ersetzt — bestehende Felder bleiben identisch, nur `weightSource` und `sliceInfo` sind neu.)

- [ ] **Step 2: Typecheck**

Run: `npx tsc --noEmit`
Expected: Fehler an jeder Stelle, die `ModelFile` literal konstruiert, ohne die zwei neuen Felder zu setzen (z.B. Test-Fixtures, falls vorhanden). Da dieses Projekt keine Frontend-Tests hat, betrifft das nur `App.tsx`, falls dort irgendwo ein `ModelFile`-Literal gebaut wird — voraussichtlich nicht der Fall, da Modelle ausschließlich per `invoke<ModelFile[]>('list_files')` vom Backend kommen. Falls doch ein Fehler auftritt, an der jeweiligen Stelle die zwei Felder ergänzen.

- [ ] **Step 3: Commit**

```bash
git add src/types/index.ts
git commit -m "Frontend-Typen: weightSource und sliceInfo an ModelFile"
```

---

### Task 8: i18n-Schlüssel + Gewichtsanzeige in `modelMetadata.ts`

**Files:**
- Modify: `src/i18n/types.ts:138` (nach `metaWeight`)
- Modify: `src/i18n/de.ts:131`, `src/i18n/en.ts:131`, `src/i18n/es.ts:131`, `src/i18n/fr.ts:131`
- Modify: `src/lib/modelMetadata.ts:17-18`

**Interfaces:**
- Consumes: `ModelFile.weightSource`, `ModelFile.sliceInfo` (Task 7).
- Produces: aktualisierte `buildMetaRows()`-Gewichtszeile; neue i18n-Keys `metaWeightFromSlicer`, `sliceFilamentHeading`, `sliceFilamentPlateLabel`, `rescanMetadataButton`.

- [ ] **Step 1: Add the keys to `types.ts`**

In `src/i18n/types.ts`, nach `metaWeight: string;`:

```ts
  metaWeightFromSlicer: string;
  sliceFilamentHeading: string;
  sliceFilamentPlateLabel: string;
  rescanMetadataButton: string;
```

- [ ] **Step 2: Add translations to all four locale files**

In `src/i18n/de.ts`, nach `metaWeight: 'Gewicht (geschätzt)',`:

```ts
  metaWeightFromSlicer: 'Gewicht (aus Slicer)',
  sliceFilamentHeading: 'Filamentverbrauch (aus Slicer)',
  sliceFilamentPlateLabel: 'Platte {index}',
  rescanMetadataButton: 'Metadaten neu einlesen',
```

In `src/i18n/en.ts`, nach `metaWeight: 'Weight (estimated)',`:

```ts
  metaWeightFromSlicer: 'Weight (from slicer)',
  sliceFilamentHeading: 'Filament usage (from slicer)',
  sliceFilamentPlateLabel: 'Plate {index}',
  rescanMetadataButton: 'Rescan metadata',
```

In `src/i18n/es.ts`, nach `metaWeight: 'Peso (estimado)',`:

```ts
  metaWeightFromSlicer: 'Peso (del laminador)',
  sliceFilamentHeading: 'Consumo de filamento (del laminador)',
  sliceFilamentPlateLabel: 'Placa {index}',
  rescanMetadataButton: 'Volver a leer metadatos',
```

In `src/i18n/fr.ts`, nach `metaWeight: 'Poids (estimé)',`:

```ts
  metaWeightFromSlicer: 'Poids (du logiciel de découpe)',
  sliceFilamentHeading: 'Consommation de filament (du logiciel de découpe)',
  sliceFilamentPlateLabel: 'Plateau {index}',
  rescanMetadataButton: 'Relire les métadonnées',
```

- [ ] **Step 3: Update the weight row in `modelMetadata.ts`**

In `src/lib/modelMetadata.ts`, ersetze:

```ts
  const weightValue =
    model.estimatedWeightG === null ? t('noValue') : `≈ ${formatWeightG(model.estimatedWeightG, language)}`;

  const rows = [
    { label: t('metaDimensions'), value: formatDimensions(model.dimensionsMm, language) },
    { label: t('metaVolume'), value: formatVolumeCm3(model.volumeCm3, language) },
    { label: t('metaWeight'), value: weightValue },
```

durch:

```ts
  const weightValue =
    model.estimatedWeightG === null
      ? t('noValue')
      : model.weightSource === 'slicer'
        ? formatWeightG(model.estimatedWeightG, language)
        : `≈ ${formatWeightG(model.estimatedWeightG, language)}`;
  const weightLabel = model.weightSource === 'slicer' ? t('metaWeightFromSlicer') : t('metaWeight');

  const rows = [
    { label: t('metaDimensions'), value: formatDimensions(model.dimensionsMm, language) },
    { label: t('metaVolume'), value: formatVolumeCm3(model.volumeCm3, language) },
    { label: weightLabel, value: weightValue },
```

- [ ] **Step 4: Typecheck**

Run: `npx tsc --noEmit`
Expected: keine Fehler.

- [ ] **Step 5: Commit**

```bash
git add src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts src/lib/modelMetadata.ts
git commit -m "i18n: Slicer-Gewicht/Filamentverbrauch-Texte in allen vier Sprachen"
```

---

### Task 9: Detailseite — Filament-Aufschlüsselung + "Metadaten neu einlesen"

**Files:**
- Modify: `src/components/ModelDetailPage.tsx`
- Modify: `src/App.tsx` (neue Funktion `rescanMetadata`, neuer State `rescanError`, Wiring an `ModelDetailPage`)

**Interfaces:**
- Consumes: `ModelFile.sliceInfo` (Task 7), Command `rescan_file_metadata` (Task 6).
- Produces: `App.tsx`-Funktion `rescanMetadata(id: string): void`, State `rescanError: string | null`.

- [ ] **Step 1: Add `rescanMetadata` and `rescanError` state in `App.tsx`**

Direkt nach der bestehenden `openInSlicer`-Funktion in `src/App.tsx` (nahe Zeile 253) einfügen:

```ts
  const [rescanError, setRescanError] = useState<string | null>(null);

  const rescanMetadata = (id: string) => {
    setRescanError(null);
    invoke<ModelFile>('rescan_file_metadata', { fileId: id })
      .then((updated) => {
        setModels((prev) => prev.map((m) => (m.id === id ? updated : m)));
      })
      .catch((e) => {
        console.error('[rescan] Neu-Einlesen fehlgeschlagen:', e);
        setRescanError(String(e));
      });
  };
```

- [ ] **Step 2: Pass the new props to `ModelDetailPage`**

Am `ModelDetailPage`-Aufruf in `src/App.tsx` (nahe Zeile 824) ergänzen:

```tsx
                onRescanMetadata={() => rescanMetadata(detailModel.id)}
                rescanError={rescanError}
```

- [ ] **Step 3: Extend `ModelDetailPage.tsx` props and render the slice-info section**

In `src/components/ModelDetailPage.tsx`, Props-Interface erweitern:

```ts
interface Props {
  model: ModelFile;
  onClose: () => void;
  onAddTag: (tag: string) => void;
  onRemoveTag: (tag: string) => void;
  onDelete: () => void;
  onTogglePrintStatus: () => void;
  onToggleFavorite: () => void;
  onToggleQueue: () => void;
  onUploadImage: () => void;
  onSnapshotCaptured: (base64: string) => void;
  onSetSourceUrl: (fileId: string, url: string | null) => void;
  onOpenInSlicer: (slicerId?: string) => void;
  onRescanMetadata: () => void;
  slicers: SlicerConfig[];
  slicerError: string | null;
  rescanError: string | null;
  displayPreference: DisplayPreference;
}
```

Destrukturierung entsprechend um `onRescanMetadata, rescanError` erweitern.

Direkt vor dem schließenden `<footer>`-Block (nach dem `</div>` der `flex gap-7`-Zeile, vor `<footer`) einen neuen Abschnitt einfügen, sichtbar nur wenn `model.sliceInfo` vorhanden ist:

```tsx
      {model.sliceInfo && (
        <div className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4">
          <p className="font-mono-ui text-[10.5px] tracking-[0.06em] uppercase text-[var(--ink-3)] mb-3">
            {t('sliceFilamentHeading')}
          </p>
          <div className="flex flex-col gap-3">
            {model.sliceInfo.plates.map((plate) => (
              <div key={plate.plateIndex}>
                <p className="text-[12.5px] font-semibold text-[var(--ink-2)] mb-1.5">
                  {t('sliceFilamentPlateLabel').replace('{index}', String(plate.plateIndex))}
                </p>
                <div className="flex flex-col gap-1">
                  {plate.filaments.map((filament, i) => (
                    <div key={i} className="flex items-center gap-2 text-[13px]">
                      {filament.color && (
                        <span
                          className="w-3 h-3 rounded-full border border-[var(--line)] flex-none"
                          style={{ backgroundColor: filament.color.slice(0, 7) }}
                        />
                      )}
                      <span className="text-[var(--ink-2)]">{filament.type}</span>
                      <span className="font-mono-ui tabular-nums text-[var(--ink-3)]">
                        {formatWeightG(filament.usedG, language)}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
```

`formatWeightG` importieren (`import { formatWeightG } from '../i18n/format';` am Dateikopf ergänzen).

Im `<footer>`-Block, den bestehenden `onOpenInSlicer`-Button-Block um einen weiteren Button ergänzen (vor dem "In Slicer öffnen"-Button):

```tsx
          <button
            onClick={onRescanMetadata}
            className="px-4 py-2 rounded-md border border-[var(--line)] text-[13px] font-semibold text-[var(--ink-2)] hover:border-[var(--line-strong)]"
          >
            {t('rescanMetadataButton')}
          </button>
```

Am Ende der Komponente, neben der bestehenden `{slicerError && ...}`-Zeile:

```tsx
      {rescanError && <p className="text-[12.5px] text-red-400">{rescanError}</p>}
```

- [ ] **Step 4: Typecheck**

Run: `npx tsc --noEmit`
Expected: keine Fehler.

- [ ] **Step 5: Manual verification**

Run: `npm run tauri dev` (oder das im Projekt übliche Dev-Kommando). Eine bereits mit OrcaSlicer/Bambu Studio geslicete 3mf-Projektdatei importieren (oder eine bestehende katalogisierte 3mf extern in Orca slicen und überschreiben), dann auf der Detailseite "Metadaten neu einlesen" klicken:
- Gewichtszeile zeigt "aus Slicer" statt "≈" mit dem realen Wert.
- Neuer Abschnitt "Filamentverbrauch (aus Slicer)" zeigt Platte(n) mit Typ/Farb-Punkt/Gramm.
- Bei einer nicht mehr existierenden Datei (Pfad umbenannt/gelöscht) erscheint eine Fehlermeldung unter dem Footer, der Katalog-Eintrag bleibt unverändert.

- [ ] **Step 6: Commit**

```bash
git add src/App.tsx src/components/ModelDetailPage.tsx
git commit -m "Detailseite: Filamentverbrauch-Aufschluesselung und 'Metadaten neu einlesen'-Button"
```
