# Filament-Katalog (Spulenverwaltung) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine eigenständige Spulenverwaltung (Material, Hersteller, Farbe, Durchmesser, Ursprungs-/Restgewicht, Preis) ergänzen, erreichbar über ein neues Icon im Header, ohne Bezug zum bestehenden Modell-Katalog.

**Architecture:** Neue SQLite-Tabelle `filament_spools` mit Standard-CRUD-Repository-Funktionen (wie bei `folders`/`tags`), vier neue Tauri-Commands, ein neuer modaler React-Dialog (`FilamentDialog.tsx`) mit Liste + wiederverwendetem Hinzufügen-/Bearbeiten-Formular, erreichbar über ein neues Header-Icon.

**Tech Stack:** Rust/`rusqlite` (Backend), React 19/TypeScript (Frontend), kein neues npm-Paket.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-09-10-filament-catalog-design.md` (vom Nutzer approved).
- Deutsche Kommentare nur wo das WARUM nicht aus dem Code ersichtlich ist.
- Kein Bezug/Fremdschlüssel zu `files` in dieser Phase (Verbrauchstracking ist bewusst zurückgestellt, siehe Spec "Out of Scope").
- Kein neues JS-Test-Framework - Frontend-Tasks werden über `npx tsc --noEmit` und den abschließenden manuellen Test verifiziert.
- Git-Commits auf Deutsch, mit gezieltem `git add <Datei>` (nie `-A`/`.`), jeder Commit endet mit:
  ```
  Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
  ```
- Nach jedem Rust-Task: `cd src-tauri && cargo test` muss vollständig grün sein.
- Kein `git commit --amend`, keine `--no-verify`.

---

### Task 1: Datenmodell und Repository (Rust)

**Files:**
- Modify: `src-tauri/src/db/schema.sql`
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/mod.rs`

**Interfaces:**
- Consumes: nichts.
- Produces:
  - `pub struct FilamentSpoolRecord { id: i64, material: String, manufacturer: Option<String>, color: Option<String>, diameter_mm: f64, original_weight_g: i64, remaining_weight_g: i64, price: Option<f64> }` (`db/models.rs`)
  - `pub struct NewFilamentSpool { material: String, manufacturer: Option<String>, color: Option<String>, diameter_mm: f64, original_weight_g: i64, remaining_weight_g: i64, price: Option<f64> }` (`db/models.rs`)
  - `pub fn insert_filament_spool(conn: &Connection, spool: &NewFilamentSpool) -> Result<i64, DbError>`
  - `pub fn list_filament_spools(conn: &Connection) -> Result<Vec<FilamentSpoolRecord>, DbError>`
  - `pub fn update_filament_spool(conn: &Connection, id: i64, spool: &NewFilamentSpool) -> Result<(), DbError>`
  - `pub fn delete_filament_spool(conn: &Connection, id: i64) -> Result<(), DbError>`

  Task 2 importiert alle vier Funktionen plus beide Structs aus `crate::db`.

- [ ] **Step 1: Tabelle zu `schema.sql` hinzufügen**

Füge am Ende von `src-tauri/src/db/schema.sql` an:

```sql

CREATE TABLE IF NOT EXISTS filament_spools (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    material TEXT NOT NULL,
    manufacturer TEXT,
    color TEXT,
    diameter_mm REAL NOT NULL,
    original_weight_g INTEGER NOT NULL,
    remaining_weight_g INTEGER NOT NULL,
    price REAL,
    created_at TEXT NOT NULL
);
```

- [ ] **Step 2: Structs zu `db/models.rs` hinzufügen**

Füge am Ende von `src-tauri/src/db/models.rs` an:

```rust

#[derive(Debug, Clone)]
pub struct FilamentSpoolRecord {
    pub id: i64,
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: i64,
    pub remaining_weight_g: i64,
    pub price: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct NewFilamentSpool {
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: i64,
    pub remaining_weight_g: i64,
    pub price: Option<f64>,
}
```

- [ ] **Step 3: Failing Tests in `db/mod.rs` schreiben**

In `src-tauri/src/db/mod.rs`, der `use models::{FileType, MaterialRecord, NewFile};`-Import im
`#[cfg(test)] mod tests`-Block wird erweitert:

```rust
    use models::{FileType, MaterialRecord, NewFile, NewFilamentSpool};
```

Füge direkt vor der letzten schließenden `}` der Datei (nach dem Test
`sets_cloud_account_status`) diese drei Tests ein:

```rust

    fn sample_filament_spool() -> NewFilamentSpool {
        NewFilamentSpool {
            material: "PLA".to_string(),
            manufacturer: Some("Bambu Lab".to_string()),
            color: Some("Schwarz".to_string()),
            diameter_mm: 1.75,
            original_weight_g: 1000,
            remaining_weight_g: 620,
            price: Some(19.99),
        }
    }

    #[test]
    fn inserts_and_lists_a_filament_spool() {
        let conn = connect_in_memory().expect("connect");
        insert_filament_spool(&conn, &sample_filament_spool()).expect("insert");

        let spools = list_filament_spools(&conn).expect("list");
        assert_eq!(spools.len(), 1);
        assert_eq!(spools[0].material, "PLA");
        assert_eq!(spools[0].manufacturer, Some("Bambu Lab".to_string()));
        assert_eq!(spools[0].color, Some("Schwarz".to_string()));
        assert_eq!(spools[0].diameter_mm, 1.75);
        assert_eq!(spools[0].original_weight_g, 1000);
        assert_eq!(spools[0].remaining_weight_g, 620);
        assert_eq!(spools[0].price, Some(19.99));
    }

    #[test]
    fn updates_a_filament_spool() {
        let conn = connect_in_memory().expect("connect");
        let id = insert_filament_spool(&conn, &sample_filament_spool()).expect("insert");

        let mut updated = sample_filament_spool();
        updated.remaining_weight_g = 450;
        updated.color = None;
        update_filament_spool(&conn, id, &updated).expect("update");

        let spools = list_filament_spools(&conn).expect("list");
        assert_eq!(spools.len(), 1);
        assert_eq!(spools[0].remaining_weight_g, 450);
        assert_eq!(spools[0].color, None);
    }

    #[test]
    fn deletes_a_filament_spool() {
        let conn = connect_in_memory().expect("connect");
        let id = insert_filament_spool(&conn, &sample_filament_spool()).expect("insert");

        delete_filament_spool(&conn, id).expect("delete");

        let spools = list_filament_spools(&conn).expect("list");
        assert!(spools.is_empty());
    }
```

- [ ] **Step 4: Tests laufen lassen, Fehlschlag verifizieren**

Run: `cd src-tauri && cargo test filament -- --nocapture`
Expected: FAIL mit `cannot find function 'insert_filament_spool'` (o. ä. - die Funktionen existieren noch nicht).

- [ ] **Step 5: Repository-Funktionen implementieren**

In `src-tauri/src/db/repository.rs`, den Import-Block am Kopf der Datei erweitern:

```rust
use super::models::{
    CloudAccountRecord, FileRecord, FileType, FilamentSpoolRecord, FolderRecord, MaterialRecord,
    NewFile, NewFilamentSpool, TagCount,
};
```

Füge am Ende der Datei (nach `set_file_cloud_link`) an:

```rust

pub fn insert_filament_spool(conn: &Connection, spool: &NewFilamentSpool) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO filament_spools
            (material, manufacturer, color, diameter_mm, original_weight_g, remaining_weight_g, price, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            spool.material,
            spool.manufacturer,
            spool.color,
            spool.diameter_mm,
            spool.original_weight_g,
            spool.remaining_weight_g,
            spool.price,
            chrono::Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_filament_spools(conn: &Connection) -> Result<Vec<FilamentSpoolRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, material, manufacturer, color, diameter_mm, original_weight_g, remaining_weight_g, price
         FROM filament_spools ORDER BY material, manufacturer",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(FilamentSpoolRecord {
                id: row.get(0)?,
                material: row.get(1)?,
                manufacturer: row.get(2)?,
                color: row.get(3)?,
                diameter_mm: row.get(4)?,
                original_weight_g: row.get(5)?,
                remaining_weight_g: row.get(6)?,
                price: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn update_filament_spool(conn: &Connection, id: i64, spool: &NewFilamentSpool) -> Result<(), DbError> {
    conn.execute(
        "UPDATE filament_spools
         SET material = ?1, manufacturer = ?2, color = ?3, diameter_mm = ?4,
             original_weight_g = ?5, remaining_weight_g = ?6, price = ?7
         WHERE id = ?8",
        params![
            spool.material,
            spool.manufacturer,
            spool.color,
            spool.diameter_mm,
            spool.original_weight_g,
            spool.remaining_weight_g,
            spool.price,
            id,
        ],
    )?;
    Ok(())
}

pub fn delete_filament_spool(conn: &Connection, id: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM filament_spools WHERE id = ?1", params![id])?;
    Ok(())
}
```

- [ ] **Step 6: Re-Exports in `db/mod.rs` ergänzen**

Ersetze den `pub use repository::{...}`-Block in `src-tauri/src/db/mod.rs`:

```rust
pub use repository::{
    add_tag_to_file, connect, delete_file, delete_filament_spool, delete_unused_tags,
    file_exists_by_path, get_file, insert_file, insert_filament_spool, insert_folder,
    list_cloud_accounts, list_filament_spools, list_files, list_folders, list_tag_counts,
    remove_tag_from_file, set_cloud_account_status, set_file_cloud_link, set_file_modified_at,
    set_file_sync_status, update_filament_spool, upsert_cloud_account,
};
```

- [ ] **Step 7: Tests laufen lassen, Erfolg verifizieren**

Run: `cd src-tauri && cargo test`
Expected: PASS für alle Tests im Projekt (73 bisherige + 3 neue = 76).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/db/schema.sql src-tauri/src/db/models.rs src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs
git commit -m "$(cat <<'EOF'
Rust: Datenmodell und Repository fuer Filament-Spulen

Neue Tabelle filament_spools (Material, Hersteller, Farbe,
Durchmesser, Ursprungs-/Restgewicht, Preis) - bewusst ohne
Fremdschluessel zu files, siehe Spec. Standard-CRUD-Repository-
Funktionen nach dem Vorbild von insert_folder/list_folders. Noch
nicht ans Frontend angebunden.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

### Task 2: Tauri-Commands

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `db::{FilamentSpoolRecord, NewFilamentSpool, insert_filament_spool, list_filament_spools, update_filament_spool, delete_filament_spool}` (Task 1).
- Produces:
  - `pub struct FilamentSpoolDto { id: String, material: String, manufacturer: Option<String>, color: Option<String>, diameter_mm: f64, original_weight_g: i64, remaining_weight_g: i64, price: Option<f64> }` (Serialize + Deserialize, camelCase)
  - Tauri-Commands `list_filament_spools() -> CmdResult<Vec<FilamentSpoolDto>>`, `add_filament_spool(spool: FilamentSpoolDto) -> CmdResult<FilamentSpoolDto>`, `update_filament_spool(spool: FilamentSpoolDto) -> CmdResult<FilamentSpoolDto>`, `delete_filament_spool(spool_id: String) -> CmdResult<()>`, registriert in `lib.rs`.

  Task 5 (`FilamentDialog.tsx`) ruft alle vier per `invoke(...)` auf.

- [ ] **Step 1: `FilamentSpoolDto` und Commands implementieren**

Füge in `src-tauri/src/commands.rs` nach `list_tag_counts` (vor `add_tag`) ein:

```rust

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilamentSpoolDto {
    pub id: String,
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: i64,
    pub remaining_weight_g: i64,
    pub price: Option<f64>,
}

fn filament_dto_to_record(spool: &FilamentSpoolDto) -> db::models::NewFilamentSpool {
    db::models::NewFilamentSpool {
        material: spool.material.clone(),
        manufacturer: spool.manufacturer.clone(),
        color: spool.color.clone(),
        diameter_mm: spool.diameter_mm,
        original_weight_g: spool.original_weight_g,
        remaining_weight_g: spool.remaining_weight_g,
        price: spool.price,
    }
}

#[tauri::command]
pub fn list_filament_spools(state: State<AppState>) -> CmdResult<Vec<FilamentSpoolDto>> {
    let conn = lock_db(&state)?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(spools
        .into_iter()
        .map(|s| FilamentSpoolDto {
            id: s.id.to_string(),
            material: s.material,
            manufacturer: s.manufacturer,
            color: s.color,
            diameter_mm: s.diameter_mm,
            original_weight_g: s.original_weight_g,
            remaining_weight_g: s.remaining_weight_g,
            price: s.price,
        })
        .collect())
}

#[tauri::command]
pub fn add_filament_spool(state: State<AppState>, spool: FilamentSpoolDto) -> CmdResult<FilamentSpoolDto> {
    let conn = lock_db(&state)?;
    let new_spool = filament_dto_to_record(&spool);
    let id = db::insert_filament_spool(&conn, &new_spool).map_err(|e| e.to_string())?;
    Ok(FilamentSpoolDto { id: id.to_string(), ..spool })
}

#[tauri::command]
pub fn update_filament_spool(state: State<AppState>, spool: FilamentSpoolDto) -> CmdResult<FilamentSpoolDto> {
    let id: i64 = spool.id.parse().map_err(|_| "invalid spool id".to_string())?;
    let conn = lock_db(&state)?;
    let new_spool = filament_dto_to_record(&spool);
    db::update_filament_spool(&conn, id, &new_spool).map_err(|e| e.to_string())?;
    Ok(spool)
}

#[tauri::command]
pub fn delete_filament_spool(state: State<AppState>, spool_id: String) -> CmdResult<()> {
    let id: i64 = spool_id.parse().map_err(|_| "invalid spool id".to_string())?;
    let conn = lock_db(&state)?;
    db::delete_filament_spool(&conn, id).map_err(|e| e.to_string())
}
```

**Hinweis:** `Ok(FilamentSpoolDto { id: id.to_string(), ..spool })` nutzt
Rusts Struct-Update-Syntax - übernimmt alle Felder von `spool` außer `id`,
das durch die von der DB vergebene echte ID ersetzt wird. `spool.id` beim
Hinzufügen ist ein vom Frontend gesendeter Platzhalter (leerer String) und
wird hier verworfen.

- [ ] **Step 2: Kompilierung prüfen**

Run: `cd src-tauri && cargo check`
Expected: nur die bekannten, bereits bestehenden `dead_code`-Warnungen, keine neuen Fehler. (Es gibt in dieser Datei bereits zwei Funktionen namens `list_filament_spools`/... - Namenskollision mit `db::list_filament_spools` ist durch den `db::`-Präfix beim Aufruf ausgeschlossen, das ist dasselbe Muster wie bei `commands::list_files` vs. `db::list_files`.)

- [ ] **Step 3: Commands in `lib.rs` registrieren**

In `src-tauri/src/lib.rs`, im `tauri::generate_handler![...]`-Aufruf, nach
`commands::list_tag_counts,` ergänzen:

```rust
            commands::list_tag_counts,
            commands::list_filament_spools,
            commands::add_filament_spool,
            commands::update_filament_spool,
            commands::delete_filament_spool,
```

- [ ] **Step 4: Tests laufen lassen**

Run: `cd src-tauri && cargo test`
Expected: PASS für alle Tests (unverändert 76 - dieser Task fügt keine neuen Rust-Tests hinzu, die Commands sind duenne Wrapper um die in Task 1 getesteten Repository-Funktionen).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
Rust: Tauri-Commands fuer Filament-Spulen-CRUD

FilamentSpoolDto (camelCase) plus vier duenne Commands, die auf die
in einem frueheren Commit hinzugefuegten Repository-Funktionen
abbilden. Noch nicht ans Frontend angebunden.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

### Task 3: TypeScript-Typ und i18n-Schlüssel

**Files:**
- Modify: `src/types/index.ts`
- Modify: `src/i18n/types.ts`
- Modify: `src/i18n/de.ts`
- Modify: `src/i18n/en.ts`
- Modify: `src/i18n/es.ts`
- Modify: `src/i18n/fr.ts`

**Interfaces:**
- Consumes: nichts.
- Produces:
  - `export interface FilamentSpool { id: string; material: string; manufacturer: string | null; color: string | null; diameterMm: number; originalWeightG: number; remainingWeightG: number; price: number | null }` (`src/types/index.ts`)
  - 14 neue Felder im `Translations`-Interface (siehe unten), in allen 4 Sprachen befüllt.

  Tasks 4 und 5 verwenden `FilamentSpool` und die neuen i18n-Schlüssel.

Neue Schlüssel: `filamentCatalogAria`, `filamentDialogTitle`,
`filamentEmptyState`, `filamentMaterialLabel`, `filamentManufacturerLabel`,
`filamentColorLabel`, `filamentDiameterLabel`, `filamentOriginalWeightLabel`,
`filamentRemainingWeightLabel`, `filamentPriceLabel`, `filamentAddButton`,
`filamentSaveButton`, `filamentEditAria`, `filamentError`.

- [ ] **Step 1: `FilamentSpool`-Typ zu `src/types/index.ts` hinzufügen**

Füge am Ende von `src/types/index.ts` an:

```ts

export interface FilamentSpool {
  id: string;
  material: string;
  manufacturer: string | null;
  color: string | null;
  diameterMm: number;
  originalWeightG: number;
  remainingWeightG: number;
  price: number | null;
}
```

- [ ] **Step 2: Schlüssel zum `Translations`-Interface hinzufügen**

In `src/i18n/types.ts`, nach `importFromCloudOption: string;` (letztes
Feld vor der schließenden `}`) ergänzen:

```ts
  importFromCloudOption: string;

  filamentCatalogAria: string;
  filamentDialogTitle: string;
  filamentEmptyState: string;
  filamentMaterialLabel: string;
  filamentManufacturerLabel: string;
  filamentColorLabel: string;
  filamentDiameterLabel: string;
  filamentOriginalWeightLabel: string;
  filamentRemainingWeightLabel: string;
  filamentPriceLabel: string;
  filamentAddButton: string;
  filamentSaveButton: string;
  filamentEditAria: string;
  filamentError: string;
}
```

(Die bisherige letzte Zeile `importFromCloudOption: string;\n}` wird durch
obigen Block ersetzt - `importFromCloudOption` bleibt inhaltlich
unverändert, nur die schließende `}` rutscht ans Ende des neuen Blocks.)

- [ ] **Step 3: Deutsche Übersetzungen**

In `src/i18n/de.ts`, nach `importFromCloudOption: 'Aus Google Drive importieren…',` ergänzen:

```ts
  importFromCloudOption: 'Aus Google Drive importieren…',

  filamentCatalogAria: 'Filament-Lager',
  filamentDialogTitle: 'Filament-Lager',
  filamentEmptyState: 'Noch keine Spulen erfasst.',
  filamentMaterialLabel: 'Material',
  filamentManufacturerLabel: 'Hersteller',
  filamentColorLabel: 'Farbe',
  filamentDiameterLabel: 'Durchmesser (mm)',
  filamentOriginalWeightLabel: 'Ursprungsgewicht (g)',
  filamentRemainingWeightLabel: 'Restgewicht (g)',
  filamentPriceLabel: 'Preis',
  filamentAddButton: 'Hinzufügen',
  filamentSaveButton: 'Speichern',
  filamentEditAria: 'Spule bearbeiten',
  filamentError: 'Fehler:',
};
```

- [ ] **Step 4: Englische Übersetzungen**

In `src/i18n/en.ts`, nach `importFromCloudOption: 'Import from Google Drive…',` ergänzen:

```ts
  importFromCloudOption: 'Import from Google Drive…',

  filamentCatalogAria: 'Filament inventory',
  filamentDialogTitle: 'Filament Inventory',
  filamentEmptyState: 'No spools recorded yet.',
  filamentMaterialLabel: 'Material',
  filamentManufacturerLabel: 'Manufacturer',
  filamentColorLabel: 'Color',
  filamentDiameterLabel: 'Diameter (mm)',
  filamentOriginalWeightLabel: 'Original weight (g)',
  filamentRemainingWeightLabel: 'Remaining weight (g)',
  filamentPriceLabel: 'Price',
  filamentAddButton: 'Add',
  filamentSaveButton: 'Save',
  filamentEditAria: 'Edit spool',
  filamentError: 'Error:',
};
```

- [ ] **Step 5: Spanische Übersetzungen**

In `src/i18n/es.ts`, nach `importFromCloudOption: 'Importar desde Google Drive…',` ergänzen:

```ts
  importFromCloudOption: 'Importar desde Google Drive…',

  filamentCatalogAria: 'Inventario de filamento',
  filamentDialogTitle: 'Inventario de Filamento',
  filamentEmptyState: 'Aún no hay bobinas registradas.',
  filamentMaterialLabel: 'Material',
  filamentManufacturerLabel: 'Fabricante',
  filamentColorLabel: 'Color',
  filamentDiameterLabel: 'Diámetro (mm)',
  filamentOriginalWeightLabel: 'Peso original (g)',
  filamentRemainingWeightLabel: 'Peso restante (g)',
  filamentPriceLabel: 'Precio',
  filamentAddButton: 'Añadir',
  filamentSaveButton: 'Guardar',
  filamentEditAria: 'Editar bobina',
  filamentError: 'Error:',
};
```

- [ ] **Step 6: Französische Übersetzungen**

In `src/i18n/fr.ts`, nach `importFromCloudOption: 'Importer depuis Google Drive…',` ergänzen:

```ts
  importFromCloudOption: 'Importer depuis Google Drive…',

  filamentCatalogAria: 'Stock de filament',
  filamentDialogTitle: 'Stock de Filament',
  filamentEmptyState: 'Aucune bobine enregistrée pour le moment.',
  filamentMaterialLabel: 'Matériau',
  filamentManufacturerLabel: 'Fabricant',
  filamentColorLabel: 'Couleur',
  filamentDiameterLabel: 'Diamètre (mm)',
  filamentOriginalWeightLabel: 'Poids initial (g)',
  filamentRemainingWeightLabel: 'Poids restant (g)',
  filamentPriceLabel: 'Prix',
  filamentAddButton: 'Ajouter',
  filamentSaveButton: 'Enregistrer',
  filamentEditAria: 'Modifier la bobine',
  filamentError: 'Erreur :',
};
```

- [ ] **Step 7: TypeScript-Kompilierung verifizieren**

Run: `npx tsc --noEmit`
Expected: keine Fehler (alle 4 Sprachdateien implementieren wieder vollständig das `Translations`-Interface).

- [ ] **Step 8: Commit**

```bash
git add src/types/index.ts src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "$(cat <<'EOF'
Frontend: FilamentSpool-Typ und i18n-Texte (DE/EN/ES/FR)

14 neue Uebersetzungsschluessel fuer den Filament-Lager-Dialog
(Titel, Feldlabels, Buttons, Leerzustand, Fehlerpraefix). Noch nicht
verwendet, das folgt in den naechsten Commits.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

### Task 4: Header-Icon

**Files:**
- Modify: `src/components/Header.tsx`

**Interfaces:**
- Consumes: i18n-Schlüssel `filamentCatalogAria` (Task 3).
- Produces: `Header` erwartet ab jetzt eine zusätzliche Prop `onOpenFilamentCatalog: () => void`.

**Hinweis:** Dieser Task macht `App.tsx`s Aufruf von `<Header ... />`
vorübergehend nicht typkonform (die neue Pflicht-Prop fehlt dort noch) -
das wird in Task 6 behoben. `npx tsc --noEmit` zeigt bis dahin einen
Fehler in `App.tsx`, nicht in `Header.tsx` selbst - das ist erwartet.

- [ ] **Step 1: Prop zum `Props`-Interface hinzufügen**

In `src/components/Header.tsx`, im `Props`-Interface, nach
`onRemoveSlicer: (id: string) => void;` ergänzen:

```tsx
  onRemoveSlicer: (id: string) => void;
  onOpenFilamentCatalog: () => void;
}
```

- [ ] **Step 2: Prop in der Funktionssignatur destrukturieren**

In der `export function Header({ ... }: Props)`-Signatur, nach
`onRemoveSlicer,` ergänzen:

```tsx
  onRemoveSlicer,
  onOpenFilamentCatalog,
}: Props) {
```

- [ ] **Step 3: Neuen Icon-Button einfügen**

Füge im `return`-Block, direkt vor `<div className="relative shrink-0">`
(dem Wrapper des ⚙-Settings-Buttons), den neuen Button ein:

```tsx
      <button
        onClick={onOpenFilamentCatalog}
        aria-label={t('filamentCatalogAria')}
        title={t('filamentCatalogAria')}
        className="shrink-0 w-8 h-8 grid place-items-center rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)] text-[15px] cursor-pointer hover:text-[var(--ink)] hover:border-[var(--line-strong)]"
      >
        ⊙
      </button>

      <div className="relative shrink-0">
```

- [ ] **Step 4: Kompilierung prüfen**

Run: `npx tsc --noEmit`
Expected: Fehler ausschließlich in `App.tsx` (fehlende neue Prop beim
`<Header ... />`-Aufruf dort), keine Fehler in `Header.tsx` selbst.

- [ ] **Step 5: Commit**

```bash
git add src/components/Header.tsx
git commit -m "$(cat <<'EOF'
Frontend: Header-Icon fuer das Filament-Lager

Neuer Button (⊙) neben dem Einstellungen-Zahnrad, gleiches visuelles
Muster. Oeffnet noch keinen Dialog (folgt in einem spaeteren Commit) -
App.tsx uebergibt die neue Pflicht-Prop noch nicht, macht die Datei
voruebergehend nicht kompilierbar.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

### Task 5: `FilamentDialog.tsx`

**Files:**
- Create: `src/components/FilamentDialog.tsx`

**Interfaces:**
- Consumes: `FilamentSpool` aus `../types` (Task 3); i18n-Schlüssel aus Task 3; Backend-Commands `list_filament_spools`/`add_filament_spool`/`update_filament_spool`/`delete_filament_spool` (Task 2).
- Produces: `export function FilamentDialog({ onClose }: { onClose: () => void })`.

  Task 6 rendert `<FilamentDialog onClose={...} />` in `App.tsx`.

- [ ] **Step 1: Komponente schreiben**

Erstelle `src/components/FilamentDialog.tsx`:

```tsx
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n/LanguageContext';
import type { FilamentSpool } from '../types';

interface Props {
  onClose: () => void;
}

interface FormState {
  material: string;
  manufacturer: string;
  color: string;
  diameterMm: string;
  originalWeightG: string;
  remainingWeightG: string;
  price: string;
}

const EMPTY_FORM: FormState = {
  material: '',
  manufacturer: '',
  color: '',
  diameterMm: '1.75',
  originalWeightG: '1000',
  remainingWeightG: '1000',
  price: '',
};

const fieldClass =
  'h-8 px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-[12.5px]';

export function FilamentDialog({ onClose }: Props) {
  const t = useT();
  const [spools, setSpools] = useState<FilamentSpool[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

  const refresh = () => {
    invoke<FilamentSpool[]>('list_filament_spools')
      .then((result) => {
        setSpools(result);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  };

  useEffect(refresh, []);

  const startEdit = (spool: FilamentSpool) => {
    setEditingId(spool.id);
    setConfirmDeleteId(null);
    setForm({
      material: spool.material,
      manufacturer: spool.manufacturer ?? '',
      color: spool.color ?? '',
      diameterMm: String(spool.diameterMm),
      originalWeightG: String(spool.originalWeightG),
      remainingWeightG: String(spool.remainingWeightG),
      price: spool.price === null ? '' : String(spool.price),
    });
  };

  const cancelForm = () => {
    setEditingId(null);
    setForm(EMPTY_FORM);
  };

  const submitForm = () => {
    if (!form.material.trim()) return;
    const payload: FilamentSpool = {
      id: editingId ?? '',
      material: form.material.trim(),
      manufacturer: form.manufacturer.trim() || null,
      color: form.color.trim() || null,
      diameterMm: parseFloat(form.diameterMm) || 0,
      originalWeightG: parseInt(form.originalWeightG, 10) || 0,
      remainingWeightG: parseInt(form.remainingWeightG, 10) || 0,
      price: form.price.trim() === '' ? null : parseFloat(form.price),
    };
    const command = editingId ? 'update_filament_spool' : 'add_filament_spool';
    invoke(command, { spool: payload })
      .then(() => {
        cancelForm();
        refresh();
      })
      .catch((e) => setError(String(e)));
  };

  const deleteSpool = (id: string) => {
    invoke('delete_filament_spool', { spoolId: id })
      .then(() => {
        setConfirmDeleteId(null);
        refresh();
      })
      .catch((e) => setError(String(e)));
  };

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/50">
      <div className="w-[480px] max-h-[640px] flex flex-col bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)]">
        <div className="flex-none px-4 py-3 border-b border-[var(--line)] flex items-center justify-between">
          <span className="text-[13px] font-semibold">{t('filamentDialogTitle')}</span>
          <span
            onClick={onClose}
            aria-label={t('cancel')}
            className="w-6 h-6 grid place-items-center rounded-full cursor-pointer text-[12px] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
          >
            ✕
          </span>
        </div>

        <div className="flex-1 overflow-y-auto px-4 py-3">
          {error && (
            <div className="pb-2 text-[12.5px] text-[var(--accent)] break-words">
              {t('filamentError')} {error}
            </div>
          )}
          {spools.length === 0 ? (
            <div className="text-[12.5px] text-[var(--ink-3)]">{t('filamentEmptyState')}</div>
          ) : (
            <div className="flex flex-col gap-2">
              {spools.map((spool) => (
                <div
                  key={spool.id}
                  className="flex items-center gap-2 px-2.5 py-2 rounded-[3px] border border-[var(--line)]"
                >
                  <div className="flex-1 min-w-0">
                    <div className="text-[13px] text-[var(--ink)] truncate">
                      {spool.material}
                      {spool.color ? ` · ${spool.color}` : ''}
                      {spool.manufacturer ? ` · ${spool.manufacturer}` : ''}
                    </div>
                    <div className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">
                      {spool.remainingWeightG} g / {spool.originalWeightG} g · {spool.diameterMm} mm
                      {spool.price !== null ? ` · ${spool.price}` : ''}
                    </div>
                  </div>
                  {confirmDeleteId === spool.id ? (
                    <div className="flex items-center gap-1.5 flex-none">
                      <span className="text-[11.5px] text-[var(--ink)]">{t('deleteConfirmQuestion')}</span>
                      <button
                        onClick={() => setConfirmDeleteId(null)}
                        className="h-6 px-2 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[11px] cursor-pointer"
                      >
                        {t('cancel')}
                      </button>
                      <button
                        onClick={() => deleteSpool(spool.id)}
                        className="h-6 px-2 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[11px] cursor-pointer"
                      >
                        {t('delete')}
                      </button>
                    </div>
                  ) : (
                    <div className="flex items-center gap-1.5 flex-none">
                      <span
                        onClick={() => startEdit(spool)}
                        aria-label={t('filamentEditAria')}
                        className="w-6 h-6 grid place-items-center rounded-full cursor-pointer text-[11px] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
                      >
                        ✎
                      </span>
                      <span
                        onClick={() => setConfirmDeleteId(spool.id)}
                        aria-label={t('deleteAriaLabel')}
                        className="w-6 h-6 grid place-items-center rounded-full cursor-pointer text-[11px] text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                      >
                        ✕
                      </span>
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="flex-none px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
          <div className="grid grid-cols-2 gap-2 mb-2">
            <input
              value={form.material}
              onChange={(e) => setForm({ ...form, material: e.target.value })}
              placeholder={t('filamentMaterialLabel')}
              className={fieldClass}
            />
            <input
              value={form.manufacturer}
              onChange={(e) => setForm({ ...form, manufacturer: e.target.value })}
              placeholder={t('filamentManufacturerLabel')}
              className={fieldClass}
            />
            <input
              value={form.color}
              onChange={(e) => setForm({ ...form, color: e.target.value })}
              placeholder={t('filamentColorLabel')}
              className={fieldClass}
            />
            <input
              type="number"
              step="0.01"
              value={form.diameterMm}
              onChange={(e) => setForm({ ...form, diameterMm: e.target.value })}
              placeholder={t('filamentDiameterLabel')}
              className={fieldClass}
            />
            <input
              type="number"
              value={form.originalWeightG}
              onChange={(e) => setForm({ ...form, originalWeightG: e.target.value })}
              placeholder={t('filamentOriginalWeightLabel')}
              className={fieldClass}
            />
            <input
              type="number"
              value={form.remainingWeightG}
              onChange={(e) => setForm({ ...form, remainingWeightG: e.target.value })}
              placeholder={t('filamentRemainingWeightLabel')}
              className={fieldClass}
            />
            <input
              type="number"
              step="0.01"
              value={form.price}
              onChange={(e) => setForm({ ...form, price: e.target.value })}
              placeholder={t('filamentPriceLabel')}
              className={fieldClass}
            />
          </div>
          <div className="flex gap-2">
            {editingId && (
              <button
                onClick={cancelForm}
                className="flex-1 h-8 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('cancel')}
              </button>
            )}
            <button
              onClick={submitForm}
              disabled={!form.material.trim()}
              className="flex-1 h-8 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
            >
              {editingId ? t('filamentSaveButton') : t('filamentAddButton')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
```

**Hinweis zum Formular:** dasselbe `form`/`editingId`-Paar bedient sowohl
"Hinzufügen" (`editingId === null`, `EMPTY_FORM`) als auch "Bearbeiten"
(`editingId` gesetzt, Formular durch `startEdit` vorbefüllt) - kein
zweiter UI-Pfad, wie in der Spec festgelegt. Zahlenfelder nutzen
`parseFloat`/`parseInt` mit `|| 0`-Fallback statt expliziter Validierung
(bewusst einfach gehalten, siehe Spec "Fehlerbehandlung").

- [ ] **Step 2: Kompilierung prüfen**

Run: `npx tsc --noEmit`
Expected: Fehler weiterhin nur in `App.tsx` (aus Task 4, unverändert) -
`FilamentDialog.tsx` selbst kompiliert sauber, wird aber noch nirgends
importiert/gerendert.

- [ ] **Step 3: Commit**

```bash
git add src/components/FilamentDialog.tsx
git commit -m "$(cat <<'EOF'
Frontend: FilamentDialog-Komponente

Modaler Dialog: Liste vorhandener Spulen (Material/Hersteller/Farbe/
Rest- und Ursprungsgewicht/Durchmesser/Preis) mit Bearbeiten- und
Loeschen-Icon je Zeile (Loeschen mit Inline-Bestaetigung), darunter
ein Formular, das sowohl fuers Hinzufuegen als auch - vorbefuellt -
fuers Bearbeiten dient. Noch nirgends eingebunden.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

### Task 6: `App.tsx` verdrahten, Live-Test

**Files:**
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `Header`s neue Prop (Task 4), `FilamentDialog` (Task 5).
- Produces: nichts (Blatt-Task, letzter Schritt des Plans).

- [ ] **Step 1: Import und State ergänzen**

In `src/App.tsx`, den Import-Block erweitern:

```tsx
import { ContextMenu } from './components/ContextMenu';
import { FilamentDialog } from './components/FilamentDialog';
```

Nach der Zeile `const [cloudUploadError, setCloudUploadError] = useState<string | null>(null);` ergänzen:

```tsx
  const [cloudUploadError, setCloudUploadError] = useState<string | null>(null);
  const [filamentDialogOpen, setFilamentDialogOpen] = useState(false);
```

- [ ] **Step 2: Prop an `Header` übergeben**

Im `<Header ... />`-Aufruf, nach `onRemoveSlicer={removeSlicer}`, ergänzen:

```tsx
        onRemoveSlicer={removeSlicer}
        onOpenFilamentCatalog={() => setFilamentDialogOpen(true)}
      />
```

- [ ] **Step 3: Dialog rendern**

Nach dem schließenden `)}` des `{contextMenu && (...)}`-Blocks (vor der
schließenden `</div>` der Wurzelkomponente), ergänzen:

```tsx
      )}

      {filamentDialogOpen && <FilamentDialog onClose={() => setFilamentDialogOpen(false)} />}
    </div>
  );
}
```

- [ ] **Step 4: TypeScript-Kompilierung verifizieren**

Run: `npx tsc --noEmit`
Expected: keine Fehler.

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx
git commit -m "$(cat <<'EOF'
Frontend: Filament-Lager vollstaendig verdrahtet

Neuer filamentDialogOpen-State, Header-Icon oeffnet den
FilamentDialog. Damit ist das Feature aus Spec
2026-09-10-filament-catalog-design.md vollstaendig umgesetzt.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

- [ ] **Step 6: Manueller Live-Test (Abschlusskriterium des gesamten Plans)**

Dieser Schritt lässt sich nicht automatisieren:

1. `npm run tauri dev` starten (oder den bereits laufenden Dev-Server nutzen - Rust-Aenderungen loesen automatisch einen Neubau aus).
2. Neues Icon (⊙) im Header klicken - Dialog öffnet sich, zeigt den Leerzustand-Text.
3. Eine Spule über das Formular hinzufügen (mind. Material ausfüllen) - erscheint sofort in der Liste.
4. Die Spule über das Bearbeiten-Icon (✎) editieren, einen Wert ändern, speichern - Änderung erscheint in der Liste.
5. Über das Löschen-Icon (✕) löschen, Bestätigung bestätigen - Eintrag verschwindet, Leerzustand-Text erscheint wieder, falls es die letzte Spule war.
6. Dialog über das ✕ oben rechts schließen, erneut öffnen - Liste ist weiterhin korrekt (Persistenz über SQLite bestätigt).
