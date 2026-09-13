# Materialkosten-Schätzung Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auf der Modell-Detailseite eine geschätzte Materialkosten-Summe anzeigen, berechnet aus dem realen Filamentverbrauch (`slice_info`) und den Preisen der Spulen im Filament-Lager.

**Architecture:** Neue reine Funktion `estimate_material_cost` in `commands.rs` matcht jedes Filament aus `slice_info` per Materialtyp-Teilstring gegen Spulen im Lager, mittelt deren Preis/Gramm und multipliziert mit dem Verbrauch. Wird in `to_dto` aufgerufen (dessen Signatur dafür eine Spulenliste als Parameter bekommt) und als neues `cost_estimate`-Feld an `ModelFileDto` angehängt.

**Tech Stack:** Rust/Tauri Backend (rusqlite, serde), React/TypeScript Frontend.

## Global Constraints

- Kosten werden **nur geschätzt**, keine Bestandsänderung an Spulen, keine manuelle Zuordnung "diese Spule für diesen Druck" (bewusst außerhalb des Scopes, siehe Spec).
- Farbe wird beim Matching **nicht** berücksichtigt, nur der Materialtyp (Teilstring, case-insensitive).
- Kein Treffer/kein Preis → `null`, niemals `0` (0 würde "kostenlos" suggerieren).
- Kostenzeile erscheint nur, wenn `slice_info` vorhanden ist (kein `cost_estimate` ohne echten Filamentverbrauch).
- Keine Währungseinheit (folgt dem bestehenden `filament_spools.price`-Feld, das ebenfalls keine führt).

---

### Task 1: `estimate_material_cost` + `CostEstimateDto` + Verdrahtung in `to_dto`

**Files:**
- Modify: `src-tauri/src/commands.rs` (neue Funktion + DTO, `to_dto`-Signatur, alle Aufrufer)

**Interfaces:**
- Produces: `pub(crate) fn estimate_material_cost(slice_info: &threemf::SliceInfo, spools: &[db::models::FilamentSpoolRecord]) -> CostEstimateDto`, `pub struct CostEstimateDto { pub total_cost: Option<f64>, pub has_unpriced_filaments: bool }`, `pub(crate) fn to_dto(file: FileRecord, spools: &[db::models::FilamentSpoolRecord]) -> ModelFileDto` (Signatur geändert — zusätzlicher Parameter), `ModelFileDto.cost_estimate: Option<CostEstimateDto>`.

- [ ] **Step 1: Write the failing tests**

In `src-tauri/src/commands.rs`, in `mod tests`, füge hinzu (Fixtures für `FilamentSpoolRecord` werden hier lokal gebaut, kein bestehender Helper dafür vorhanden):

```rust
fn sample_spool(material: &str, original_weight_g: i64, price: Option<f64>) -> db::models::FilamentSpoolRecord {
    db::models::FilamentSpoolRecord {
        id: 1,
        material: material.to_string(),
        manufacturer: None,
        color: None,
        location: None,
        diameter_mm: 1.75,
        original_weight_g,
        remaining_weight_g: original_weight_g,
        price,
        image_png: None,
    }
}

fn sample_slice_info_single_filament(filament_type: &str, used_g: f64) -> threemf::SliceInfo {
    threemf::SliceInfo {
        total_weight_g: used_g,
        plates: vec![threemf::slice_info::PlateFilamentUsage {
            plate_index: 1,
            weight_g: used_g,
            filaments: vec![threemf::slice_info::FilamentUsage {
                filament_type: filament_type.to_string(),
                color: None,
                used_g,
                used_m: 0.0,
            }],
        }],
    }
}

#[test]
fn estimate_material_cost_uses_single_matching_spool() {
    let slice_info = sample_slice_info_single_filament("PLA", 20.0);
    let spools = vec![sample_spool("PLA", 1000, Some(20.0))]; // 0.02 pro Gramm
    let cost = estimate_material_cost(&slice_info, &spools);
    assert!((cost.total_cost.expect("cost") - 0.4).abs() < 1e-6);
    assert!(!cost.has_unpriced_filaments);
}

#[test]
fn estimate_material_cost_averages_multiple_matching_spools() {
    let slice_info = sample_slice_info_single_filament("PLA", 10.0);
    let spools = vec![
        sample_spool("PLA", 1000, Some(20.0)), // 0.02/g
        sample_spool("Generic PLA", 1000, Some(30.0)), // 0.03/g
    ];
    let cost = estimate_material_cost(&slice_info, &spools);
    // Durchschnitt 0.025/g * 10g = 0.25
    assert!((cost.total_cost.expect("cost") - 0.25).abs() < 1e-6);
}

#[test]
fn estimate_material_cost_returns_none_when_no_matching_material() {
    let slice_info = sample_slice_info_single_filament("PETG", 10.0);
    let spools = vec![sample_spool("PLA", 1000, Some(20.0))];
    let cost = estimate_material_cost(&slice_info, &spools);
    assert_eq!(cost.total_cost, None);
    assert!(cost.has_unpriced_filaments);
}

#[test]
fn estimate_material_cost_ignores_spools_without_price() {
    let slice_info = sample_slice_info_single_filament("PLA", 10.0);
    let spools = vec![sample_spool("PLA", 1000, None)];
    let cost = estimate_material_cost(&slice_info, &spools);
    assert_eq!(cost.total_cost, None);
    assert!(cost.has_unpriced_filaments);
}

#[test]
fn estimate_material_cost_sums_only_priced_filaments_when_mixed() {
    let slice_info = threemf::SliceInfo {
        total_weight_g: 30.0,
        plates: vec![threemf::slice_info::PlateFilamentUsage {
            plate_index: 1,
            weight_g: 30.0,
            filaments: vec![
                threemf::slice_info::FilamentUsage {
                    filament_type: "PLA".to_string(),
                    color: None,
                    used_g: 20.0,
                    used_m: 0.0,
                },
                threemf::slice_info::FilamentUsage {
                    filament_type: "Nylon".to_string(),
                    color: None,
                    used_g: 10.0,
                    used_m: 0.0,
                },
            ],
        }],
    };
    let spools = vec![sample_spool("PLA", 1000, Some(20.0))]; // 0.02/g, kein Nylon im Lager
    let cost = estimate_material_cost(&slice_info, &spools);
    assert!((cost.total_cost.expect("cost") - 0.4).abs() < 1e-6); // nur PLA-Anteil
    assert!(cost.has_unpriced_filaments); // Nylon fehlt
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test estimate_material_cost`
Expected: FAIL with "cannot find function `estimate_material_cost`".

- [ ] **Step 3: Add `CostEstimateDto` and `estimate_material_cost`**

In `src-tauri/src/commands.rs`, direkt nach der bestehenden `SliceInfoDto`-Definition (aus dem Filamentverbrauch-Feature):

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimateDto {
    pub total_cost: Option<f64>,
    pub has_unpriced_filaments: bool,
}

/// Schaetzt die Materialkosten eines Modells: pro Filament im `slice_info`
/// wird nach Lager-Spulen gesucht, deren `material`-Feld den Filament-Typ
/// als Teilstring enthaelt (case-insensitive, Farbe wird NICHT verglichen -
/// Slicer- und Lager-Farbwerte stimmen selten exakt ueberein). Aus allen
/// Treffern mit gesetztem Preis wird ein Durchschnittspreis pro Gramm
/// gebildet. Filamente ohne Treffer/Preis fliessen nicht in die Summe ein
/// (0 wuerde faelschlich "kostenlos" bedeuten) - stattdessen markiert
/// `has_unpriced_filaments`, dass die Summe unvollstaendig ist.
pub(crate) fn estimate_material_cost(
    slice_info: &threemf::SliceInfo,
    spools: &[db::models::FilamentSpoolRecord],
) -> CostEstimateDto {
    let mut total_cost = 0.0;
    let mut priced_any = false;
    let mut has_unpriced = false;

    for plate in &slice_info.plates {
        for filament in &plate.filaments {
            let type_lower = filament.filament_type.to_lowercase();
            let matching: Vec<&db::models::FilamentSpoolRecord> = spools
                .iter()
                .filter(|s| {
                    s.material.to_lowercase().contains(&type_lower)
                        && s.price.is_some()
                        && s.original_weight_g > 0
                })
                .collect();

            if matching.is_empty() {
                has_unpriced = true;
                continue;
            }

            let avg_price_per_gram: f64 = matching
                .iter()
                .map(|s| s.price.unwrap() / s.original_weight_g as f64)
                .sum::<f64>()
                / matching.len() as f64;

            total_cost += filament.used_g * avg_price_per_gram;
            priced_any = true;
        }
    }

    CostEstimateDto {
        total_cost: if priced_any { Some(total_cost) } else { None },
        has_unpriced_filaments: has_unpriced,
    }
}
```

- [ ] **Step 4: Change `to_dto`'s signature and wire in the cost estimate**

In `src-tauri/src/commands.rs`, ändere die Signatur von:

```rust
pub(crate) fn to_dto(file: FileRecord) -> ModelFileDto {
```

zu:

```rust
pub(crate) fn to_dto(file: FileRecord, spools: &[db::models::FilamentSpoolRecord]) -> ModelFileDto {
```

Direkt nach dem bestehenden Block, der `estimated_weight_g`/`weight_source` aus `slice_info` berechnet (vor der `ModelFileDto { ... }`-Konstruktion), ergänze:

```rust
    let cost_estimate = slice_info
        .as_ref()
        .map(|info| estimate_material_cost(info, spools));
```

In der `ModelFileDto { ... }`-Konstruktion, direkt nach `slice_info: slice_info.map(SliceInfoDto::from),`:

```rust
        slice_info: slice_info.map(SliceInfoDto::from),
        cost_estimate,
```

In `ModelFileDto`'s Struct-Definition, direkt nach `pub slice_info: Option<SliceInfoDto>,`:

```rust
    pub slice_info: Option<SliceInfoDto>,
    pub cost_estimate: Option<CostEstimateDto>,
```

- [ ] **Step 5: Update every call site of `to_dto`**

Sieben Aufrufer in `src-tauri/src/commands.rs` müssen die Spulenliste beschaffen und mitgeben:

**`list_files`** (Funktion `pub fn list_files`):

```rust
pub fn list_files(state: State<AppState>) -> CmdResult<Vec<ModelFileDto>> {
    let conn = lock_db(&state)?;
    let files = db::list_files(&conn).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
}
```

**`list_collection_files`** (gleiches Muster):

```rust
pub fn list_collection_files(state: State<AppState>, collection_id: String) -> CmdResult<Vec<ModelFileDto>> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    let ids = db::list_collection_file_ids(&conn, cid).map_err(|e| e.to_string())?;
    let files = db::list_files_by_ids(&conn, &ids).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
}
```

**`list_trash`** (gleiches Muster):

```rust
pub fn list_trash(state: State<AppState>) -> CmdResult<Vec<ModelFileDto>> {
    let conn = lock_db(&state)?;
    let files = db::list_trash(&conn).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
}
```

**`import_one`** (die letzten drei Zeilen der Funktion, `conn` ist dort bereits `&mut Connection`):

```rust
    let id = db::insert_file(conn, &new_file).map_err(|e| e.to_string())?;
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "imported file not found after insert".to_string())?;
    let spools = db::list_filament_spools(conn).map_err(|e| e.to_string())?;
    Ok(to_dto(file, &spools))
```

**`rescan_file`** (gleiches Muster am Ende der Funktion):

```rust
    db::update_scanned_metadata(conn, id, &update).map_err(|e| e.to_string())?;
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Datei nach Aktualisierung nicht mehr gefunden".to_string())?;
    let spools = db::list_filament_spools(conn).map_err(|e| e.to_string())?;
    Ok(to_dto(file, &spools))
```

**`scan_catalog_issues`** (zwei `to_dto`-Aufrufe in einer Funktion, `conn` bleibt fuer die ganze Funktion im Scope):

```rust
pub fn scan_catalog_issues(state: State<AppState>) -> CmdResult<CatalogIssuesDto> {
    let conn = lock_db(&state)?;
    let files = db::list_files(&conn).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;

    let mut orphaned_ids: HashSet<i64> = HashSet::new();
    let mut orphaned: Vec<ModelFileDto> = Vec::new();
    for file in &files {
        if let Err(e) = std::fs::metadata(&file.path) {
            if e.kind() == std::io::ErrorKind::NotFound {
                orphaned_ids.insert(file.id);
                orphaned.push(to_dto(file.clone(), &spools));
            }
        }
    }

    let duplicate_groups: Vec<Vec<ModelFileDto>> = group_duplicates(files, &orphaned_ids)
        .into_iter()
        .map(|group| group.into_iter().map(|f| to_dto(f, &spools)).collect())
        .collect();

    Ok(CatalogIssuesDto { orphaned, duplicate_groups })
}
```

(Die dazwischenliegenden Kommentare im bestehenden Code bleiben unverändert stehen, nur die drei gezeigten Zeilen/Blöcke ändern sich.)

**Drei bestehende Tests** in `mod tests` rufen `to_dto(file)` direkt auf und müssen auf die neue Signatur umgestellt werden — ersetze in allen drei Vorkommen `to_dto(file)` durch `to_dto(file, &[])` (leere Spulenliste, diese Tests prüfen keine Kosten):
- `to_dto_uses_slicer_weight_and_marks_source_when_slice_info_present`
- `to_dto_serializes_slice_info_with_camel_case_and_type_rename`
- `to_dto_falls_back_to_estimate_when_slice_info_absent`

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd src-tauri && cargo test estimate_material_cost`
Expected: all 5 new tests PASS.

- [ ] **Step 7: Run the full backend test suite and build**

Run: `cd src-tauri && cargo test && cargo build`
Expected: all tests PASS (inkl. der drei angepassten `to_dto`-Tests), Build erfolgreich.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "Materialkosten-Schaetzung: estimate_material_cost + Verdrahtung in to_dto"
```

---

### Task 2: Frontend — Typen + Anzeige auf der Detailseite

**Files:**
- Modify: `src/types/index.ts`
- Modify: `src/components/ModelDetailPage.tsx`
- Modify: `src/i18n/types.ts`, `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`

**Interfaces:**
- Consumes: JSON-Form von `ModelFileDto.cost_estimate` (Task 1) — camelCase: `{ totalCost: number | null; hasUnpricedFilaments: boolean } | null`.

- [ ] **Step 1: Add the type**

In `src/types/index.ts`, ergänze nach `sliceInfo: SliceInfo | null;` innerhalb von `ModelFile`:

```ts
export interface CostEstimate {
  totalCost: number | null;
  hasUnpricedFilaments: boolean;
}
```

(als eigenständiges Interface vor `ModelFile`, analog zu `SliceInfo`) und im `ModelFile`-Interface direkt nach `sliceInfo: SliceInfo | null;`:

```ts
  sliceInfo: SliceInfo | null;
  costEstimate: CostEstimate | null;
```

- [ ] **Step 2: Add i18n keys**

In `src/i18n/types.ts`, nach `sliceFilamentHeading: string;`:

```ts
  sliceFilamentHeading: string;
  metaCostEstimate: string;
  costEstimateUnpricedHint: string;
```

In `src/i18n/de.ts`, nach `sliceFilamentHeading: 'Filamentverbrauch (aus Slicer)',`:

```ts
  sliceFilamentHeading: 'Filamentverbrauch (aus Slicer)',
  metaCostEstimate: 'Geschätzte Materialkosten',
  costEstimateUnpricedHint: 'Nicht für alle Filamente ein Lagerpreis gefunden',
```

In `src/i18n/en.ts`, nach `sliceFilamentHeading: 'Filament usage (from slicer)',`:

```ts
  sliceFilamentHeading: 'Filament usage (from slicer)',
  metaCostEstimate: 'Estimated material cost',
  costEstimateUnpricedHint: 'Not all filaments matched to an inventory price',
```

In `src/i18n/es.ts`, nach `sliceFilamentHeading: 'Consumo de filamento (del laminador)',`:

```ts
  sliceFilamentHeading: 'Consumo de filamento (del laminador)',
  metaCostEstimate: 'Coste de material estimado',
  costEstimateUnpricedHint: 'No se encontró un precio de inventario para todos los filamentos',
```

In `src/i18n/fr.ts`, nach `sliceFilamentHeading: 'Consommation de filament (du logiciel de découpe)',`:

```ts
  sliceFilamentHeading: 'Consommation de filament (du logiciel de découpe)',
  metaCostEstimate: 'Coût matière estimé',
  costEstimateUnpricedHint: "Prix de stock introuvable pour tous les filaments",
```

- [ ] **Step 3: Render the cost row**

In `src/components/ModelDetailPage.tsx`, im bereits vorhandenen Filamentverbrauch-Abschnitt (`{model.sliceInfo && (...)}`-Block), direkt nach dem schließenden `</div>` der `plates.map(...)`-Liste und vor dem schließenden `</div>` des Abschnitts, ergänze:

```tsx
          {model.costEstimate && (
            <div className="flex items-center justify-between gap-2 mt-3 pt-3 border-t border-[var(--line)] text-[13px]">
              <span className="text-[var(--ink-3)]">
                {t('metaCostEstimate')}
                {model.costEstimate.hasUnpricedFilaments && (
                  <span title={t('costEstimateUnpricedHint')} className="ml-1 text-[var(--ink-3)]">
                    *
                  </span>
                )}
              </span>
              <span className="font-mono-ui tabular-nums">
                {model.costEstimate.totalCost === null
                  ? t('noValue')
                  : formatPrice(model.costEstimate.totalCost, language)}
              </span>
            </div>
          )}
```

`formatPrice` ist bereits in `../i18n/format` definiert; ergänze den Import in `ModelDetailPage.tsx`:

```ts
import { formatWeightG, formatLengthM, formatPrice } from '../i18n/format';
```

- [ ] **Step 4: Typecheck**

Run: `npx tsc --noEmit` (vom Projekt-Root, nicht `src-tauri`)
Expected: keine Fehler.

- [ ] **Step 5: Commit**

```bash
git add src/types/index.ts src/components/ModelDetailPage.tsx src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "Frontend: Materialkosten-Schaetzung auf der Detailseite anzeigen"
```
