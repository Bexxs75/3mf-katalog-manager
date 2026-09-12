# Modell-Detailseite + Druckplatten-Erkennung Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine vollflächige Modell-Detailseite (Doppelklick auf eine Karte öffnet sie, additiv zum bestehenden Seitenpanel) plus Best-Effort-Erkennung der Druckplattenanzahl für Bambu-Studio/OrcaSlicer-`.3mf`-Dateien.

**Architecture:** Neues Backend-Modul liest `Metadata/model_settings.config` aus dem 3MF-Zip zusätzlich zum bestehenden Model-XML-Parsing. Die neue `plate_count`-Spalte fließt durch dieselbe `NewFile`→DB→`ModelFileDto`-Pipeline wie jedes andere Metadatenfeld. Frontend bekommt eine neue `ModelDetailPage`-Komponente und einen neuen `detailModelId`-State in `App.tsx`, ohne Router (bestehendes Zustand-Schalt-Muster).

**Tech Stack:** Rust (Tauri 2, rusqlite, quick-xml, zip), React 19 + TypeScript, Tailwind (Utility-Klassen, keine neuen CSS-Dateien).

## Global Constraints

- Backend-Tests: `cd src-tauri && cargo test` — muss nach jedem Backend-Task grün bleiben (Stand vor diesem Plan: 79/79).
- Frontend: kein Test-Framework — Verifikation über `npx tsc --noEmit` (im Projekt-Root) nach jedem Frontend-Task.
- `Translations`-Interface (`src/i18n/types.ts`) erzwingt zur Compile-Zeit Vollständigkeit — jeder neue i18n-Key MUSS in allen 4 Sprachdateien (`de.ts`, `en.ts`, `es.ts`, `fr.ts`) UND in `types.ts` ergänzt werden, sonst schlägt `tsc` fehl.
- Migrations-Muster für neue DB-Spalten: `ALTER TABLE files ADD COLUMN ...` als fehlertolerante Einzelzeile (`let _ = conn.execute(...)`) in `repository.rs`s `init()`, zusätzlich Teil von `CREATE TABLE IF NOT EXISTS files` in `schema.sql` für Neuinstallationen.
- Commit-Attribution: `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` unter jedem Commit dieses Plans, plus `Claude-Session: https://claude.ai/code/session_01WAQ5i49v1oEqoX5RzCBUz6`.

---

### Task 1: Backend — Druckplatten-Zählung aus `Metadata/model_settings.config`

**Files:**
- Create: `src-tauri/src/threemf/plates.rs`
- Modify: `src-tauri/src/threemf/mod.rs` (neues `pub mod plates;`)
- Test: eingebettet in `plates.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `pub fn count_plates<R: std::io::Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Option<u32>` — für Task 2 (Aufruf aus `container::read_package`).

- [ ] **Step 1: Write the failing test**

```rust
// src-tauri/src/threemf/plates.rs
use std::io::{Read, Seek};
use zip::ZipArchive;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
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
```

Need `use std::io::Write;` for the test module — add it inside the `tests` module's `use super::*;` block scope (it's only needed there).

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib threemf::plates`
Expected: FAIL with "cannot find function `count_plates`" (function body doesn't exist yet).

- [ ] **Step 3: Write minimal implementation**

Above the `#[cfg(test)]` block in the same file:

```rust
/// Liest `Metadata/model_settings.config` (Bambu Studio/OrcaSlicer-
/// spezifisch, kein Teil des offiziellen 3MF-Standards) aus dem bereits
/// geoeffneten Zip-Archiv und zaehlt die enthaltenen `<plate>`-Elemente
/// (Namespace-Praefix wird ignoriert, gleiche Toleranz wie der
/// bestehende Model-Parser). Existiert die Datei nicht oder laesst sie
/// sich nicht als XML lesen, wird `None` zurueckgegeben - kein
/// Fehlerfall, gleiches Verhalten wie das bestehende Thumbnail-Fallback.
pub fn count_plates<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Option<u32> {
    let mut file = archive.by_name("Metadata/model_settings.config").ok()?;
    let mut xml = String::new();
    file.read_to_string(&mut xml).ok()?;
    drop(file);

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
```

Register the module in `src-tauri/src/threemf/mod.rs` — add near the other `pub mod` lines at the top:

```rust
pub mod plates;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib threemf::plates`
Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
cd /home/andreasm/Projekte/3mf-katalog-manager
git add src-tauri/src/threemf/plates.rs src-tauri/src/threemf/mod.rs
git commit -m "$(cat <<'EOF'
Druckplatten-Zählung für Bambu Studio/OrcaSlicer-3MF-Dateien

Neues threemf::plates-Modul liest Metadata/model_settings.config
(herstellerspezifisch, kein 3MF-Standard) und zählt <plate>-Elemente,
namespace-tolerant. None bei fehlender/kaputter Datei statt Fehler.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01WAQ5i49v1oEqoX5RzCBUz6
EOF
)"
```

---

### Task 2: Backend — `plate_count` durch die Import-Pipeline führen

**Files:**
- Modify: `src-tauri/src/threemf/container.rs` (`PackageParts` + `read_package`)
- Modify: `src-tauri/src/threemf/mod.rs` (`ThreeMfDocument` + `parse_3mf_reader`)
- Modify: `src-tauri/src/db/schema.sql`
- Modify: `src-tauri/src/db/repository.rs` (`init`, `insert_file`, `row_to_file`, `list_files`, `get_file` SQL)
- Modify: `src-tauri/src/db/models.rs` (`NewFile`, `FileRecord`)
- Modify: `src-tauri/src/commands.rs` (`import_one`, `ModelFileDto`, `to_dto`)
- Test: `src-tauri/src/db/mod.rs` (Roundtrip-Test)

**Interfaces:**
- Consumes: `plates::count_plates` aus Task 1.
- Produces: `ModelFileDto.plate_count: Option<i64>` — für Task 3 (Frontend-Typ `ModelFile.plateCount`).

- [ ] **Step 1: `PackageParts` bekommt das Feld, `read_package` füllt es**

In `src-tauri/src/threemf/container.rs`, `PackageParts`-Struct erweitern:

```rust
pub struct PackageParts {
    pub root_model: ParsedModel,
    pub referenced_models: HashMap<String, ParsedModel>,
    pub thumbnail: Option<Vec<u8>>,
    pub plate_count: Option<u32>,
}
```

In `read_package` (gleiche Datei), direkt nach `let mut archive = ZipArchive::new(reader)?;`:

```rust
    let plate_count = super::plates::count_plates(&mut archive);
```

Und im finalen `Ok(PackageParts { ... })`-Konstruktor (Rückgabe von `read_package`) das neue Feld ergänzen: `plate_count,`.

- [ ] **Step 2: `ThreeMfDocument` bekommt das Feld**

In `src-tauri/src/threemf/mod.rs`:

```rust
pub struct ThreeMfDocument {
    pub object_count: usize,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub materials: Vec<ThreeMfMaterial>,
    pub metadata: BTreeMap<String, String>,
    pub thumbnail_png: Option<Vec<u8>>,
    pub plate_count: Option<u32>,
}
```

In `parse_3mf_reader`, im `Ok(ThreeMfDocument { ... })`-Konstruktor:

```rust
        thumbnail_png: package.thumbnail.clone(),
        plate_count: package.plate_count,
    })
```

- [ ] **Step 3: Kompilieren und bestehende Threemf-Tests laufen lassen**

Run: `cd src-tauri && cargo build && cargo test --lib threemf`
Expected: Kompiliert; bestehende Tests in `container.rs`/`mod.rs`, die `PackageParts { ... }` per Struct-Literal bauen, schlagen jetzt ggf. mit "missing field `plate_count`" fehl — dort `plate_count: None,` ergänzen (per Grep `PackageParts {` in Testcode finden).

- [ ] **Step 4: DB-Schema — neue Spalte**

In `src-tauri/src/db/schema.sql`, im `CREATE TABLE IF NOT EXISTS files (...)`-Block eine Zeile ergänzen (neben `queue_position INTEGER,` o. ä.):

```sql
    plate_count INTEGER,
```

In `src-tauri/src/db/repository.rs`s `init()`, neben der bestehenden `queue_position`-Migrationszeile:

```rust
    let _ = conn.execute("ALTER TABLE files ADD COLUMN plate_count INTEGER", []);
```

- [ ] **Step 5: `NewFile`/`FileRecord` bekommen das Feld**

In `src-tauri/src/db/models.rs`, beide Structs um `pub plate_count: Option<i64>,` ergänzen (gleiche Stelle wie `object_count`).

- [ ] **Step 6: `insert_file`, `row_to_file`, `list_files`, `get_file` anpassen**

In `src-tauri/src/db/repository.rs`s `insert_file`: SQL-Spaltenliste und VALUES-Platzhalter um `plate_count` erweitern (letzte Spalte, `?26`), Parameter-Liste um `file.plate_count,` ergänzen (Platzhalterzahlen der nachfolgenden Parameter entsprechend hochzählen — `favorite` wird `?26`... **wichtig:** da `plate_count` als neue, letzte Spalte angehängt wird, einfach ans Ende der bestehenden Spalten-/Platzhalter-/Parameter-Listen anhängen, keine bestehenden Indizes verschieben).

In `list_files` und `get_file`s SQL-`SELECT`-Statements: `plate_count` ans Ende der Spaltenliste anhängen (Index 26).

In `row_to_file`: `plate_count: row.get(26)?,` im `Ok(FileRecord { ... })` ergänzen.

- [ ] **Step 7: `import_one` und `to_dto` verdrahten**

In `src-tauri/src/commands.rs`s `import_one`, im `NewFile { ... }`-Konstruktor: `plate_count: doc.plate_count.map(|c| c as i64),` für den `.3mf`-Zweig (aus `doc: ThreeMfDocument`). Für den `.stl`-Zweig bleibt es implizit `None` — `import_one` unterscheidet bereits per `match extension.as_deref()` zwischen beiden Zweigen; da beide Zweige aktuell dieselbe `NewFile`-Konstruktion NACH dem `match` teilen (siehe `let (file_type, dimensions_mm, ...) = match ... { ... };` gefolgt von einem gemeinsamen `NewFile { ... }`), am saubersten: `plate_count` als zusätzliche Variable in beiden Zweigen des `match` binden (3mf-Zweig: `doc.plate_count`, stl-Zweig: `None`), dann `plate_count,` im `NewFile`-Konstruktor verwenden — analog zum bestehenden `object_count`, das genauso pro Zweig unterschiedlich gesetzt wird.

`ModelFileDto` (`commands.rs`) bekommt `pub plate_count: Option<i64>,`. `to_dto` bekommt `plate_count: file.plate_count,` im `ModelFileDto { ... }`-Konstruktor.

- [ ] **Step 8: Roundtrip-Test schreiben**

In `src-tauri/src/db/mod.rs` (dort liegen die bestehenden Roundtrip-Tests für andere Felder, z. B. `content_hash`):

```rust
    #[test]
    fn insert_and_get_file_roundtrips_plate_count() {
        let mut conn = test_connection();
        let mut file = sample_new_file();
        file.plate_count = Some(2);
        let id = repository::insert_file(&mut conn, &file).unwrap();
        let fetched = repository::get_file(&conn, id).unwrap().unwrap();
        assert_eq!(fetched.plate_count, Some(2));
    }
```

(`sample_new_file()`/`test_connection()` sind bestehende Test-Helfer in dieser Datei — per Grep `fn sample_new_file` bzw. `fn test_connection` in `db/mod.rs` die exakte Signatur prüfen, falls sie abweicht, den Helfer entsprechend nutzen statt neu zu erfinden.)

- [ ] **Step 9: Vollständigen Testlauf verifizieren**

Run: `cd src-tauri && cargo build && cargo test`
Expected: alle Tests grün (79 vorherige + 4 neue aus Task 1 + 1 neuer Roundtrip-Test = 84).

- [ ] **Step 10: Commit**

```bash
cd /home/andreasm/Projekte/3mf-katalog-manager
git add src-tauri/src/threemf/container.rs src-tauri/src/threemf/mod.rs \
  src-tauri/src/db/schema.sql src-tauri/src/db/repository.rs \
  src-tauri/src/db/models.rs src-tauri/src/db/mod.rs src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
plate_count durch Import-Pipeline bis zum Frontend-DTO verdrahtet

Neue nullable files.plate_count-Spalte (fehlertolerante ALTER-TABLE-
Migration für Bestands-DBs). import_one übernimmt threemf::count_plates
nur für .3mf-Dateien, ModelFileDto stellt das Feld ans Frontend aus.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01WAQ5i49v1oEqoX5RzCBUz6
EOF
)"
```

---

### Task 3: Frontend — `plateCount`-Typ + gemeinsame Metadaten-Helper-Datei

**Files:**
- Modify: `src/types/index.ts` (`ModelFile.plateCount`)
- Create: `src/lib/modelMetadata.ts`
- Modify: `src/components/DetailPanel.tsx` (Import statt lokaler Definition)
- Modify: `src/i18n/types.ts`, `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts` (neuer Key `metaPlateCount`)

**Interfaces:**
- Produces: `export function buildMetaRows(model: ModelFile, t: TFunction, language: Language): { label: string; value: string }[]` — für Task 4 (`ModelDetailPage.tsx`).

- [ ] **Step 1: Typ ergänzen**

In `src/types/index.ts`, `ModelFile`-Interface: `plateCount: number | null;` ergänzen (neben `objectCount`).

- [ ] **Step 2: i18n-Key in allen 4 Sprachdateien + Interface**

`src/i18n/types.ts`, im `Translations`-Interface neben `metaObjectCount: string;`:

```typescript
  metaPlateCount: string;
```

`src/i18n/de.ts` neben `metaObjectCount: 'Objekte',`:

```typescript
  metaPlateCount: 'Druckplatten',
```

`src/i18n/en.ts`:

```typescript
  metaPlateCount: 'Build plates',
```

`src/i18n/es.ts`:

```typescript
  metaPlateCount: 'Placas de impresión',
```

`src/i18n/fr.ts`:

```typescript
  metaPlateCount: 'Plateaux d\'impression',
```

- [ ] **Step 3: `tsc` prüfen, dass die 4 Sprachdateien vollständig sind**

Run: `npx tsc --noEmit`
Expected: kompiliert (noch keine Verwendung des neuen Keys, aber die 4 Dateien müssen bereits jetzt konsistent sein, da `Translations` Vollständigkeit erzwingt).

- [ ] **Step 4: `buildMetaRows` nach `src/lib/modelMetadata.ts` verschieben, um `plateCount` erweitert**

Aus `src/components/DetailPanel.tsx` den kompletten `buildMetaRows`-Funktionsblock (inkl. der `TFunction`-Typalias-Zeile darüber) ausschneiden und in eine neue Datei verschieben:

```typescript
// src/lib/modelMetadata.ts
import type { ModelFile } from '../types';
import type { Language, Translations } from '../i18n/types';
import { formatBytes, formatDate, formatDimensions, formatVolumeCm3, formatWeightG } from '../i18n/format';

export type TFunction = <K extends keyof Translations>(key: K) => Translations[K];

export function buildMetaRows(
  model: ModelFile,
  t: TFunction,
  language: Language,
): { label: string; value: string }[] {
  const materialsValue =
    model.materials.length === 0
      ? t('noValue')
      : model.materials.map((m) => m.name).join(', ');
  const objectCountValue = model.objectCount === null ? t('noValue') : String(model.objectCount);
  const weightValue =
    model.estimatedWeightG === null ? t('noValue') : `≈ ${formatWeightG(model.estimatedWeightG, language)}`;

  const rows = [
    { label: t('metaDimensions'), value: formatDimensions(model.dimensionsMm, language) },
    { label: t('metaVolume'), value: formatVolumeCm3(model.volumeCm3, language) },
    { label: t('metaWeight'), value: weightValue },
    { label: t('metaObjectCount'), value: objectCountValue },
  ];

  if (model.plateCount !== null) {
    rows.push({ label: t('metaPlateCount'), value: String(model.plateCount) });
  }

  rows.push(
    { label: t('metaMaterial'), value: materialsValue },
    { label: t('metaFileSize'), value: formatBytes(model.fileSizeBytes, language) },
    { label: t('metaImported'), value: formatDate(model.importedAt, language) },
  );

  return rows;
}
```

In `src/components/DetailPanel.tsx`: den ausgeschnittenen Block entfernen, stattdessen importieren:

```typescript
import { buildMetaRows } from '../lib/modelMetadata';
```

(Der bisherige lokale `TFunction`-Typalias in `DetailPanel.tsx` wird ebenfalls entfernt, falls er anderswo in der Datei noch gebraucht wird, von dort `import type { TFunction } from '../lib/modelMetadata';` ergänzen — per Grep `TFunction` in `DetailPanel.tsx` prüfen, ob er außerhalb von `buildMetaRows` noch referenziert wird.)

- [ ] **Step 5: `tsc` verifizieren**

Run: `npx tsc --noEmit`
Expected: kompiliert fehlerfrei, `DetailPanel.tsx` zeigt weiterhin identische Metadaten-Zeilen wie vorher (plus die neue Druckplatten-Zeile, wenn `plateCount` gesetzt ist).

- [ ] **Step 6: Commit**

```bash
cd /home/andreasm/Projekte/3mf-katalog-manager
git add src/types/index.ts src/lib/modelMetadata.ts src/components/DetailPanel.tsx \
  src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "$(cat <<'EOF'
Metadaten-Zeilen-Logik in geteilte Datei extrahiert, Druckplatten-Zeile

buildMetaRows() zieht von DetailPanel.tsx nach src/lib/modelMetadata.ts
um, damit die kommende ModelDetailPage dieselbe Logik nutzen kann statt
sie zu duplizieren. Neue Zeile "Druckplatten" (nur sichtbar, wenn
plateCount gesetzt ist), i18n-Key in allen 4 Sprachen.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01WAQ5i49v1oEqoX5RzCBUz6
EOF
)"
```

---

### Task 4: Frontend — `ModelDetailPage.tsx`-Komponente

**Files:**
- Create: `src/components/ModelDetailPage.tsx`

**Interfaces:**
- Consumes: `buildMetaRows`/`TFunction` aus Task 3 (`src/lib/modelMetadata.ts`); `ModelViewer` (Props: `fileId: string`, `needsSnapshot: boolean`, `onSnapshotCaptured: (base64: string) => void`) aus `src/components/ModelViewer.tsx`; `useT`/`useLanguage` aus `src/i18n/LanguageContext`.
- Produces: `export function ModelDetailPage(props: Props): JSX.Element` mit folgenden Props (exakt gleiche Callback-Signaturen wie `DetailPanel`, siehe Task 5):

```typescript
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
  slicers: SlicerConfig[];
  slicerError: string | null;
}
```

- [ ] **Step 1: Komponente schreiben**

```typescript
// src/components/ModelDetailPage.tsx
import { useState } from 'react';
import type { ModelFile, SlicerConfig } from '../types';
import { useT, useLanguage } from '../i18n/LanguageContext';
import { buildMetaRows } from '../lib/modelMetadata';
import { ModelViewer } from './ModelViewer';

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
  slicers: SlicerConfig[];
  slicerError: string | null;
}

export function ModelDetailPage({
  model,
  onClose,
  onAddTag,
  onRemoveTag,
  onDelete,
  onTogglePrintStatus,
  onToggleFavorite,
  onToggleQueue,
  onUploadImage,
  onSnapshotCaptured,
  onSetSourceUrl,
  onOpenInSlicer,
  slicers,
  slicerError,
}: Props) {
  const t = useT();
  const { language } = useLanguage();
  const [tagDraft, setTagDraft] = useState('');
  const [sourceDraft, setSourceDraft] = useState(model.sourceUrl ?? '');
  const [editingSource, setEditingSource] = useState(false);
  const [showCustomImage, setShowCustomImage] = useState(model.displayImage !== null);

  const rows = buildMetaRows(model, t, language);

  const submitTag = () => {
    const value = tagDraft.trim();
    if (value) onAddTag(value);
    setTagDraft('');
  };

  const submitSource = () => {
    const value = sourceDraft.trim();
    onSetSourceUrl(model.id, value || null);
    setEditingSource(false);
  };

  return (
    <div className="flex-1 min-w-0 overflow-y-auto p-6 flex flex-col gap-6">
      <header className="flex items-start gap-4">
        <button
          onClick={onClose}
          title={t('backToCatalog')}
          className="flex-none w-9 h-9 rounded-md border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] grid place-items-center hover:border-[var(--line-strong)] hover:text-[var(--ink)]"
        >
          ←
        </button>
        <h1 className="flex-1 min-w-0 text-[1.5rem] font-semibold leading-tight break-words">
          {model.name}
        </h1>
      </header>

      <div className="flex gap-7 flex-wrap items-start">
        <div className="flex-[3_1_480px] min-w-[320px]">
          <div className="relative aspect-[4/3] rounded-[10px] border border-[var(--line)] bg-[var(--plate)] overflow-hidden">
            {showCustomImage && model.displayImage ? (
              <img src={model.displayImage} alt={model.name} className="absolute inset-0 w-full h-full object-contain" />
            ) : (
              <ModelViewer
                fileId={model.id}
                needsSnapshot={model.displayImage === null}
                onSnapshotCaptured={onSnapshotCaptured}
              />
            )}
            {model.displayImage && (
              <div className="absolute top-3 right-3 flex bg-[var(--panel-2)] border border-[var(--line)] rounded-full overflow-hidden font-mono-ui text-[11.5px]">
                <button
                  onClick={() => setShowCustomImage(false)}
                  className={`px-3.5 py-1.5 ${!showCustomImage ? 'bg-[var(--accent)] text-[var(--accent-ink)] font-semibold' : 'text-[var(--ink-3)]'}`}
                >
                  {t('detailViewer3d')}
                </button>
                <button
                  onClick={() => setShowCustomImage(true)}
                  className={`px-3.5 py-1.5 ${showCustomImage ? 'bg-[var(--accent)] text-[var(--accent-ink)] font-semibold' : 'text-[var(--ink-3)]'}`}
                >
                  {t('detailViewerImage')}
                </button>
              </div>
            )}
          </div>
          <button
            onClick={onUploadImage}
            className="mt-2 h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12px] font-semibold hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('uploadModelImageLabel')}
          </button>
        </div>

        <div className="flex-[2_1_340px] min-w-[300px] flex flex-col gap-4">
          <div className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4">
            {rows.map((row) => (
              <div
                key={row.label}
                className="flex justify-between items-baseline gap-4 py-2 border-b border-[var(--line)] last:border-b-0 text-[13.5px]"
              >
                <span className="text-[var(--ink-3)]">{row.label}</span>
                <span className="font-mono-ui tabular-nums text-right">{row.value}</span>
              </div>
            ))}
            <div className="flex justify-between items-baseline gap-4 py-2 border-b border-[var(--line)] text-[13.5px]">
              <span className="text-[var(--ink-3)]">{t('metaCreator')}</span>
              <span className="font-mono-ui text-right">{model.creator ?? t('noValue')}</span>
            </div>
            <div className="flex justify-between items-baseline gap-4 py-2 text-[13.5px]">
              <span className="text-[var(--ink-3)]">{t('metaSourceUrl')}</span>
              {editingSource ? (
                <input
                  autoFocus
                  value={sourceDraft}
                  onChange={(e) => setSourceDraft(e.target.value)}
                  onBlur={submitSource}
                  onKeyDown={(e) => e.key === 'Enter' && submitSource()}
                  placeholder={t('sourceUrlPlaceholder')}
                  className="flex-1 min-w-0 bg-[var(--panel-2)] border border-[var(--line-strong)] rounded px-2 py-1 text-[13px]"
                />
              ) : model.sourceUrl ? (
                <span className="text-right">
                  <a href={model.sourceUrl} target="_blank" rel="noreferrer" className="underline decoration-[var(--line-strong)] underline-offset-2">
                    {model.sourceUrl}
                  </a>{' '}
                  <button onClick={() => setEditingSource(true)} className="text-[var(--ink-3)]">✎</button>
                </span>
              ) : (
                <button onClick={() => setEditingSource(true)} className="text-[var(--ink-3)] underline decoration-dotted">
                  {t('sourceUrlPlaceholder')} ✎
                </button>
              )}
            </div>

            <div className="mt-4">
              <p className="font-mono-ui text-[10.5px] tracking-[0.06em] uppercase text-[var(--ink-3)] mb-2">
                {t('hashtagsHeading')}
              </p>
              <div className="flex flex-wrap gap-1.5">
                {model.tags.map((tag) => (
                  <span key={tag} className="font-mono-ui text-[12px] px-2.5 py-1 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)]">
                    #{tag}{' '}
                    <button onClick={() => onRemoveTag(tag)} className="text-[var(--ink-3)]">✕</button>
                  </span>
                ))}
                <input
                  value={tagDraft}
                  onChange={(e) => setTagDraft(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && submitTag()}
                  placeholder={t('addTagPlaceholder')}
                  className="font-mono-ui text-[12px] px-2.5 py-1 rounded-full bg-transparent border border-dashed border-[var(--line)] text-[var(--ink-3)] w-32"
                />
              </div>
            </div>
          </div>

          <div className="flex flex-wrap gap-2 items-center">
            <button
              onClick={onTogglePrintStatus}
              className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-[12.5px] font-semibold ${
                model.printStatus === 'printed'
                  ? 'bg-[var(--good-soft,var(--accent-soft))] text-[var(--good,var(--accent))]'
                  : 'bg-[var(--panel-2)] text-[var(--ink-2)] border border-[var(--line)]'
              }`}
            >
              {model.printStatus === 'printed' ? t('printedBadge') : t('notPrintedLabel')}
            </button>
            <button
              onClick={onToggleFavorite}
              aria-label={model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
              title={model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
              className={`w-9 h-9 rounded-md border grid place-items-center ${
                model.favorite ? 'border-[var(--accent)] text-[var(--accent)] bg-[var(--accent-soft)]' : 'border-[var(--line)] text-[var(--ink-2)]'
              }`}
            >
              ♥
            </button>
            <button onClick={onToggleQueue} className="text-[13px] font-semibold text-[var(--ink-2)] hover:text-[var(--ink)]">
              {model.queuePosition !== null ? t('removeFromQueue') : `+ ${t('addToQueue')}`}
            </button>
          </div>
        </div>
      </div>

      <footer className="flex items-center justify-between gap-4 flex-wrap rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-4 py-3.5 mt-auto">
        <span className="font-mono-ui text-[12px] text-[var(--ink-3)] break-all">{model.path}</span>
        <div className="flex gap-2.5 items-center flex-none">
          <button onClick={onDelete} className="px-4 py-2 rounded-md border border-[var(--line)] text-[13px] font-semibold text-red-400 hover:border-red-400">
            {t('delete')}
          </button>
          <button
            onClick={() => onOpenInSlicer()}
            disabled={slicers.length === 0}
            className="px-4 py-2 rounded-md bg-[var(--accent)] text-[var(--accent-ink)] text-[13px] font-semibold disabled:opacity-50"
          >
            {t('openInSlicer')} ↗
          </button>
        </div>
      </footer>
      {slicerError && <p className="text-[12.5px] text-red-400">{slicerError}</p>}
    </div>
  );
}
```

`t('detailViewer3d')`, `t('detailViewerImage')`, `t('backToCatalog')`, `t('metaCreator')` sind neue i18n-Keys (siehe Step 2 — `metaCreator` ist NEU, DetailPanel.tsx zeigt aktuell gar keine Ersteller-Zeile, nur als Sidebar-Filter genutzt). Alle anderen verwendeten Keys existieren bereits unter folgenden **verifizierten** Namen (per Grep in `src/i18n/types.ts`/`DetailPanel.tsx` bestätigt, NICHT die zunächst angenommenen): `metaSourceUrl` (Label), `sourceUrlPlaceholder` (Platzhaltertext, dient hier doppelt als Editier-Placeholder UND als Leer-Zustand-Text), `hashtagsHeading`, `addTagPlaceholder`, `printedBadge`/`notPrintedLabel` (statt "printedLabel"/"markAsPrinted"), `favoriteAdd`/`favoriteRemove` (statt "favoriteLabel"), `addToQueue`/`removeFromQueue` (statt "addToQueueLabel"/"removeFromQueueLabel"), `delete` (statt "deleteLabel"), `openInSlicer` (statt "openInSlicerLabel"), `uploadModelImageLabel` (dieser eine hatte den angenommenen Namen tatsächlich korrekt).

- [ ] **Step 2: Fehlende neue i18n-Keys ergänzen**

`src/i18n/types.ts`:

```typescript
  backToCatalog: string;
  detailViewer3d: string;
  detailViewerImage: string;
  metaCreator: string;
```

`src/i18n/de.ts`:

```typescript
  backToCatalog: 'Zurück zum Katalog',
  detailViewer3d: '3D-Ansicht',
  detailViewerImage: 'Eigenes Bild',
  metaCreator: 'Ersteller',
```

`src/i18n/en.ts`:

```typescript
  backToCatalog: 'Back to catalog',
  detailViewer3d: '3D view',
  detailViewerImage: 'Custom image',
  metaCreator: 'Creator',
```

`src/i18n/es.ts`:

```typescript
  backToCatalog: 'Volver al catálogo',
  detailViewer3d: 'Vista 3D',
  detailViewerImage: 'Imagen propia',
  metaCreator: 'Creador',
```

`src/i18n/fr.ts`:

```typescript
  backToCatalog: 'Retour au catalogue',
  detailViewer3d: 'Vue 3D',
  detailViewerImage: 'Image personnalisée',
  metaCreator: 'Créateur',
```

- [ ] **Step 3: `tsc` verifizieren**

Run: `npx tsc --noEmit`
Expected: kompiliert fehlerfrei. Bei Abweichungen bei den in Step 1 angenommenen bestehenden Key-Namen (`metaCreator` etc.) hier die Fehlermeldungen nutzen, um die Komponente auf die tatsächlichen Namen zu korrigieren.

- [ ] **Step 4: Commit**

```bash
cd /home/andreasm/Projekte/3mf-katalog-manager
git add src/components/ModelDetailPage.tsx src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "$(cat <<'EOF'
Neue ModelDetailPage-Komponente (vollflächige Modell-Detailansicht)

Zweispaltig: große 3D-Vorschau/eigenes Bild links, Metadaten (inkl.
Druckplatten, wenn erkannt), Ersteller, Quelle, Tags rechts. Aktionen
(Druckstatus, Favorit, Warteschlange) darunter, Slicer/Löschen im
Footer. Noch nicht in App.tsx verdrahtet (folgt in Task 5).

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01WAQ5i49v1oEqoX5RzCBUz6
EOF
)"
```

---

### Task 5: Frontend — In `App.tsx` verdrahten, Doppelklick auf Karten

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/components/ModelGrid.tsx`
- Modify: `src/components/ModelList.tsx`

**Interfaces:**
- Consumes: `ModelDetailPage` aus Task 4.

- [ ] **Step 1: `onOpenDetail`-Prop auf `ModelGrid`/`ModelList`**

In `src/components/ModelGrid.tsx`, `Props`-Interface um `onOpenDetail: (id: string) => void;` erweitern. In BEIDEN Karten-Renderern (`renderCompactCard` und dem zweiten, komfort-artigen Renderer weiter unten in derselben Datei — per Grep `onClick={() => onSelect(m.id)}` beide Fundstellen bearbeiten) die `onDoubleClick`-Prop ergänzen:

```typescript
        onDoubleClick={() => onOpenDetail(m.id)}
```

(direkt neben der bestehenden `onClick={() => onSelect(m.id)}`-Zeile, jeweils im selben `<div>`).

In `src/components/ModelList.tsx` analog: `Props` um `onOpenDetail: (id: string) => void;` erweitern, in der Zeilen-`<div>` neben `onClick={() => onSelect(m.id)}`:

```typescript
          onDoubleClick={() => onOpenDetail(m.id)}
```

- [ ] **Step 2: `detailModelId`-State + Escape-Handler in `App.tsx`**

Neben dem bestehenden `const [selectedId, setSelectedId] = useState<string | null>(null);`:

```typescript
  const [detailModelId, setDetailModelId] = useState<string | null>(null);
```

Neuer `useEffect` (neben bestehenden `useEffect`-Aufrufen in der Datei, z. B. in der Nähe des Kontextmenü-Escape-Handlers, falls vorhanden — sonst direkt nach den State-Deklarationen):

```typescript
  useEffect(() => {
    if (!detailModelId) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setDetailModelId(null);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [detailModelId]);
```

- [ ] **Step 3: Render-Umschaltung im Hauptbereich**

Im JSX, wo aktuell `<ModelGrid .../>` bzw. `<ModelList .../>` innerhalb von `<div className="flex-1 overflow-y-auto p-4">` gerendert werden: den `detailModel`-Lookup und die bedingte Anzeige ergänzen. Vor dem `return (...)`-Block der Komponente (oder direkt vor der Stelle, an der `selected` bereits berechnet wird, `const selected = models.find(...)`):

```typescript
  const detailModel = detailModelId ? models.find((m) => m.id === detailModelId) ?? null : null;
```

Die bestehende `<div className="flex-1 overflow-y-auto p-4">{view === 'grid' ? (...) : (...)}</div>`-Struktur wird um eine äußere Bedingung erweitert:

```typescript
            {detailModel ? (
              <ModelDetailPage
                model={detailModel}
                onClose={() => setDetailModelId(null)}
                onAddTag={(t) => addTag(detailModel.id, t)}
                onRemoveTag={(t) => removeTag(detailModel.id, t)}
                onDelete={() => {
                  deleteModel(detailModel.id);
                  setDetailModelId(null);
                }}
                onTogglePrintStatus={() => togglePrintStatus(detailModel.id)}
                onToggleFavorite={() => toggleFavorite(detailModel.id)}
                onToggleQueue={() =>
                  detailModel.queuePosition !== null ? removeFromQueue(detailModel.id) : addToQueue(detailModel.id)
                }
                onUploadImage={() => uploadCustomImage(detailModel.id)}
                onSnapshotCaptured={(base64) => captureRenderSnapshot(detailModel.id, base64)}
                onSetSourceUrl={(fileId, url) => setModelSourceUrl(fileId, url)}
                onOpenInSlicer={(slicerId) => openInSlicer(detailModel.id, slicerId)}
                slicers={slicers}
                slicerError={slicerError}
              />
            ) : (
              <div className="flex-1 overflow-y-auto p-4">
                {view === 'grid' ? (
                  <ModelGrid
                    models={filtered}
                    selectedId={selectedId}
                    onSelect={selectModel}
                    onOpenDetail={setDetailModelId}
                    onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                    onToggleFavorite={toggleFavorite}
                  />
                ) : (
                  <ModelList
                    models={filtered}
                    selectedId={selectedId}
                    onSelect={selectModel}
                    onOpenDetail={setDetailModelId}
                    onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                  />
                )}
              </div>
            )}
```

`ModelDetailPage` importieren:

```typescript
import { ModelDetailPage } from './components/ModelDetailPage';
```

Das bestehende `<DetailPanel .../>` (rechtes Seitenpanel) bleibt unverändert an seiner Stelle außerhalb dieses `<main>`-Blocks bestehen — es wird nur ausgeblendet, wenn `detailModel` aktiv ist, sinnvollerweise durch eine analoge Bedingung um das `<DetailPanel>`-Element (`{!detailModel && <DetailPanel ... />}`), damit nicht zwei Detail-Ansichten gleichzeitig sichtbar sind.

- [ ] **Step 4: `tsc` verifizieren**

Run: `npx tsc --noEmit`
Expected: kompiliert fehlerfrei.

- [ ] **Step 5: Live-Verifikation**

Run: `npm run tauri dev` (im Hintergrund starten, kurz warten, dann per Screenshot/manuell prüfen — siehe Testing-Abschnitt der Spec). Doppelklick auf eine Karte öffnet die neue Seite, Escape/Zurück-Pfeil schließt sie wieder, das rechte Seitenpanel ist währenddessen nicht sichtbar. Anschließend den Dev-Server wieder beenden.

- [ ] **Step 6: Commit**

```bash
cd /home/andreasm/Projekte/3mf-katalog-manager
git add src/App.tsx src/components/ModelGrid.tsx src/components/ModelList.tsx
git commit -m "$(cat <<'EOF'
ModelDetailPage in App.tsx verdrahtet, Doppelklick öffnet sie

Neuer detailModelId-State (kein Router, folgt dem bestehenden
Zustand-Schalt-Muster wie mainView). Grid/Liste bekommen onOpenDetail,
ausgelöst per Doppelklick zusätzlich zum bestehenden Einzelklick
(Panel-Auswahl bleibt unverändert). Escape schließt die Seite.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01WAQ5i49v1oEqoX5RzCBUz6
EOF
)"
```

---

### Task 6: Backend + Frontend — Voller Verifikationslauf

**Files:** keine Änderungen, nur Verifikation über den gesamten Plan hinweg.

- [ ] **Step 1: Backend voll durchtesten**

Run: `cd src-tauri && cargo build && cargo test`
Expected: alle Tests grün (84, siehe Task 2).

- [ ] **Step 2: Frontend voll bauen**

Run: `cd /home/andreasm/Projekte/3mf-katalog-manager && npm run build`
Expected: `tsc && vite build` fehlerfrei.

- [ ] **Step 3: Live-Smoke-Test mit einer echten Mehrplatten-Datei**

Run: `npm run tauri dev`, eine Bambu-Studio-`.3mf`-Datei mit mehreren Platten aus dem eigenen Katalog importieren (falls noch nicht im Katalog: z. B. eine Datei mit "plates" im Namen aus `/mnt/Daten2/3D Druck Sammelordner/`), Doppelklick öffnen, prüfen dass die Druckplatten-Zeile mit der korrekten Anzahl erscheint. Dev-Server danach beenden.

- [ ] **Step 4: Kein Commit in diesem Task** (reiner Verifikationsschritt; bei gefundenen Problemen zurück zum jeweiligen Task, dort fixen und dort erneut committen).
