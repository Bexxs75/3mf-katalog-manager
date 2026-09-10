# Filament-Lager: Hauptansicht + Bild-Upload Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Das bestehende Filament-Lager-Popup (`FilamentDialog.tsx`) wird zu einer vollwertigen Hauptansicht (`FilamentView.tsx`, per Header-Umschalter erreichbar), und jede Spule bekommt ein eigenes hochladbares Foto statt des Material-Name-Platzhalters.

**Architecture:** Neue Spalte `image_png BLOB` an der bestehenden `filament_spools`-Tabelle (analog zu `files.thumbnail_png`), ein neuer Tauri-Command `pick_and_read_image` (nativer Datei-Dialog + Base64-Rückgabe, Muster wie `pick_slicer_executable`). `App.tsx` bekommt einen neuen `mainView: 'catalog' | 'filament'`-State, der Sidebar/DetailPanel/modell-spezifische Header-Controls ein-/ausblendet und den Hauptbereich zwischen dem bestehenden Modell-Katalog und der neuen `FilamentView` umschaltet.

**Tech Stack:** Tauri v2 (`tauri_plugin_dialog`, neue direkte Abhängigkeit `base64`), Rust-Backend, React 19/TypeScript-Frontend.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-09-10-filament-catalog-main-view-design.md`
- Deutsche Kommentare nur wo das WARUM nicht aus dem Code ersichtlich ist.
- Bewusst keine Bild-Kompression/-Skalierung/-Größenlimit, kein "Bild entfernen" (nur Ersetzen durch neuen Upload) - siehe Spec "Out of Scope".
- `pick_and_read_image` bekommt (wie `pick_slicer_executable`) keinen automatisierten Test - echter blockierender OS-Dialog.
- Kein neues JS-Test-Framework - Frontend-Tasks werden über `npx tsc --noEmit` und den abschließenden manuellen Test verifiziert.
- Git-Commits auf Deutsch, mit gezieltem `git add <Datei>` (nie `-A`/`.`), jeder Commit endet mit:
  ```
  Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
  ```
- Nach jedem Rust-Task: `cd src-tauri && cargo test` muss vollständig grün sein.
- Kein `git commit --amend`, keine `--no-verify`.

---

### Task 1: `image_png`-Spalte, Datenmodell, Repository

**Files:**
- Modify: `src-tauri/src/db/schema.sql`
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/mod.rs`

**Interfaces:**
- Consumes: bestehende `FilamentSpoolRecord`/`NewFilamentSpool`-Structs (`db/models.rs:98-118`), bestehende `insert_filament_spool`/`list_filament_spools`/`update_filament_spool`-Funktionen (`db/repository.rs:428-487`).
- Produces: `FilamentSpoolRecord`/`NewFilamentSpool` bekommen ein neues Feld `pub image_png: Option<Vec<u8>>`. `insert_filament_spool`/`list_filament_spools`/`update_filament_spool` verarbeiten das neue Feld.

  Task 2 nutzt `image_png` beim Konvertieren zwischen DB-Record und `FilamentSpoolDto`.

- [ ] **Step 1: Spalte zu `schema.sql` hinzufügen**

In `src-tauri/src/db/schema.sql`, ersetze den bestehenden `filament_spools`-Block:

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
    image_png BLOB,
    created_at TEXT NOT NULL
);
```

(Einzige Änderung: `image_png BLOB,` vor `created_at`.)

- [ ] **Step 2: Structs in `db/models.rs` erweitern**

In `src-tauri/src/db/models.rs`, ersetze den bestehenden Block (Zeilen 97-118):

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
    pub image_png: Option<Vec<u8>>,
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
    pub image_png: Option<Vec<u8>>,
}
```

- [ ] **Step 3: Failing Tests in `db/mod.rs` schreiben**

In `src-tauri/src/db/mod.rs`, ersetze `sample_filament_spool()` (Zeilen 282-292):

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
            image_png: None,
        }
    }
```

Füge direkt nach dem bestehenden Test `deletes_a_filament_spool` (endet auf Zeile 335 mit `}`) diesen neuen Test ein:

```rust

    #[test]
    fn stores_and_lists_a_filament_spool_image() {
        let conn = connect_in_memory().expect("connect");
        let mut with_image = sample_filament_spool();
        with_image.image_png = Some(vec![137, 80, 78, 71]); // PNG-Magic-Bytes als Platzhalter-Daten
        insert_filament_spool(&conn, &with_image).expect("insert");

        let spools = list_filament_spools(&conn).expect("list");
        assert_eq!(spools.len(), 1);
        assert_eq!(spools[0].image_png, Some(vec![137, 80, 78, 71]));
    }
```

- [ ] **Step 4: Tests laufen lassen, Fehlschlag verifizieren**

Run: `cd src-tauri && cargo test filament -- --nocapture`
Expected: FAIL (Kompilierfehler - `image_png` existiert noch nicht auf `NewFilamentSpool`/in den SQL-Statements).

- [ ] **Step 5: Repository-Funktionen erweitern**

In `src-tauri/src/db/repository.rs`, ersetze die drei Funktionen (Zeilen 428-491):

```rust
pub fn insert_filament_spool(conn: &Connection, spool: &NewFilamentSpool) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO filament_spools
            (material, manufacturer, color, diameter_mm, original_weight_g, remaining_weight_g, price, image_png, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            spool.material,
            spool.manufacturer,
            spool.color,
            spool.diameter_mm,
            spool.original_weight_g,
            spool.remaining_weight_g,
            spool.price,
            spool.image_png,
            chrono::Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_filament_spools(conn: &Connection) -> Result<Vec<FilamentSpoolRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, material, manufacturer, color, diameter_mm, original_weight_g, remaining_weight_g, price, image_png
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
                image_png: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn update_filament_spool(conn: &Connection, id: i64, spool: &NewFilamentSpool) -> Result<(), DbError> {
    conn.execute(
        "UPDATE filament_spools
         SET material = ?1, manufacturer = ?2, color = ?3, diameter_mm = ?4,
             original_weight_g = ?5, remaining_weight_g = ?6, price = ?7, image_png = ?8
         WHERE id = ?9",
        params![
            spool.material,
            spool.manufacturer,
            spool.color,
            spool.diameter_mm,
            spool.original_weight_g,
            spool.remaining_weight_g,
            spool.price,
            spool.image_png,
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

(Nur `insert_filament_spool` und `update_filament_spool`/`list_filament_spools` ändern sich inhaltlich; `delete_filament_spool` bleibt identisch, hier nur als Anker mit abgedruckt.)

- [ ] **Step 6: Tests laufen lassen, Erfolg verifizieren**

Run: `cd src-tauri && cargo test`
Expected: PASS für alle Tests (76 bisherige + 1 neuer = 77).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/schema.sql src-tauri/src/db/models.rs src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs
git commit -m "$(cat <<'EOF'
Rust: image_png-Spalte fuer Filament-Spulen

Neue optionale BLOB-Spalte an filament_spools, analog zu
files.thumbnail_png. FilamentSpoolRecord/NewFilamentSpool sowie
insert/list/update-Repository-Funktionen entsprechend erweitert. Noch
nicht an Tauri-Commands oder Frontend angebunden.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

### Task 2: `pick_and_read_image`-Command + `image_png` in `FilamentSpoolDto`

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `db::models::{FilamentSpoolRecord, NewFilamentSpool}` mit `image_png`-Feld (Task 1).
- Produces:
  - `FilamentSpoolDto` bekommt ein neues Feld `pub image_png: Option<String>` (Base64).
  - Neuer Command `pick_and_read_image() -> CmdResult<Option<String>>` (Base64 der gewählten Bilddatei, `None` bei Abbruch).

  Task 5 (`FilamentView.tsx`) ruft `invoke<string | null>('pick_and_read_image')` auf und verwendet `imagePng` in der `FilamentSpool`-Payload.

- [ ] **Step 1: `base64`-Abhängigkeit ergänzen**

In `src-tauri/Cargo.toml`, füge im `[dependencies]`-Block nach `tokio = { version = "1", features = [...] }` eine neue Zeile an:

```toml
base64 = "0.22"
```

- [ ] **Step 2: `FilamentSpoolDto` und Konvertierung erweitern**

In `src-tauri/src/commands.rs`, ersetze den Block von `FilamentSpoolDto` bis `filament_dto_to_record` (Zeilen 143-166):

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
    pub image_png: Option<String>,
}

fn filament_dto_to_record(spool: &FilamentSpoolDto) -> db::models::NewFilamentSpool {
    use base64::Engine;
    db::models::NewFilamentSpool {
        material: spool.material.clone(),
        manufacturer: spool.manufacturer.clone(),
        color: spool.color.clone(),
        diameter_mm: spool.diameter_mm,
        original_weight_g: spool.original_weight_g,
        remaining_weight_g: spool.remaining_weight_g,
        price: spool.price,
        image_png: spool
            .image_png
            .as_ref()
            .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok()),
    }
}
```

**Hinweis:** `.and_then(...).ok()` statt `.map_err(...)?` - ein ungültiger
Base64-String vom Frontend (sollte nie vorkommen, da das Frontend nur
selbst per `pick_and_read_image` erzeugte Strings zurückschickt) führt
hier bewusst zu "kein Bild speichern" statt zu einem harten Fehler beim
Speichern der übrigen Felder.

- [ ] **Step 3: `list_filament_spools`-Command erweitern**

In `src-tauri/src/commands.rs`, ersetze den `.map(|s| FilamentSpoolDto { ... })`-Block innerhalb von `list_filament_spools` (Zeilen 174-183):

```rust
        .map(|s| {
            use base64::Engine;
            FilamentSpoolDto {
                id: s.id.to_string(),
                material: s.material,
                manufacturer: s.manufacturer,
                color: s.color,
                diameter_mm: s.diameter_mm,
                original_weight_g: s.original_weight_g,
                remaining_weight_g: s.remaining_weight_g,
                price: s.price,
                image_png: s
                    .image_png
                    .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)),
            }
        })
```

- [ ] **Step 4: `pick_and_read_image`-Command hinzufügen**

Füge in `src-tauri/src/commands.rs` nach `delete_filament_spool` (endet auf Zeile 209 mit `}`) ein:

```rust

#[tauri::command]
pub async fn pick_and_read_image(app: tauri::AppHandle) -> CmdResult<Option<String>> {
    use base64::Engine;
    let picked = app
        .dialog()
        .file()
        .add_filter("Bilder", &["png", "jpg", "jpeg", "webp"])
        .blocking_pick_file();

    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(Some(base64::engine::general_purpose::STANDARD.encode(bytes)))
}
```

- [ ] **Step 5: Kompilierung prüfen**

Run: `cd src-tauri && cargo check`
Expected: nur die bekannten, bereits bestehenden `dead_code`-Warnungen, keine neuen Fehler.

- [ ] **Step 6: Command in `lib.rs` registrieren**

In `src-tauri/src/lib.rs`, im `tauri::generate_handler![...]`-Aufruf, nach `commands::delete_filament_spool,` (Zeile 45) ergänzen:

```rust
            commands::delete_filament_spool,
            commands::pick_and_read_image,
```

- [ ] **Step 7: Tests laufen lassen**

Run: `cd src-tauri && cargo test`
Expected: PASS für alle Tests (unverändert 77 - dieser Task fügt keine neuen Rust-Tests hinzu, siehe Global Constraints zu `pick_and_read_image`).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
Rust: pick_and_read_image-Command, image_png in FilamentSpoolDto

Neuer Command oeffnet einen nativen Datei-Dialog (Bilder-Filter) und
gibt die gewaehlte Datei Base64-kodiert zurueck, gleiches Muster wie
pick_slicer_executable. FilamentSpoolDto transportiert das Bild jetzt
als Base64-String zwischen Frontend und den bestehenden
Repository-Funktionen (Task 1). Noch nicht ans Frontend angebunden.

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
  - `FilamentSpool` (in `src/types/index.ts`) bekommt ein neues Feld `imagePng: string | null;`.
  - 3 neue Felder im `Translations`-Interface: `filamentNavButton`, `filamentBackToCatalogButton`, `filamentUploadImageLabel`.

  Tasks 4 und 5 verwenden `FilamentSpool.imagePng` und die neuen i18n-Schlüssel.

- [ ] **Step 1: `FilamentSpool`-Typ erweitern**

In `src/types/index.ts`, ersetze den bestehenden `FilamentSpool`-Block (Zeilen 51-60):

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
  imagePng: string | null;
}
```

- [ ] **Step 2: Schlüssel zum `Translations`-Interface hinzufügen**

In `src/i18n/types.ts`, nach `filamentError: string;` (Zeile 109, letztes Feld vor der schließenden `}`) ergänzen:

```ts
  filamentError: string;
  filamentNavButton: string;
  filamentBackToCatalogButton: string;
  filamentUploadImageLabel: string;
}
```

- [ ] **Step 3: Deutsche Übersetzungen**

In `src/i18n/de.ts`, nach `filamentError: 'Fehler:',` ergänzen:

```ts
  filamentError: 'Fehler:',
  filamentNavButton: 'Filament',
  filamentBackToCatalogButton: 'Katalog',
  filamentUploadImageLabel: 'Bild hochladen',
```

- [ ] **Step 4: Englische Übersetzungen**

In `src/i18n/en.ts`, nach `filamentError: 'Error:',` ergänzen:

```ts
  filamentError: 'Error:',
  filamentNavButton: 'Filament',
  filamentBackToCatalogButton: 'Catalog',
  filamentUploadImageLabel: 'Upload image',
```

- [ ] **Step 5: Spanische Übersetzungen**

In `src/i18n/es.ts`, nach `filamentError: 'Error:',` ergänzen:

```ts
  filamentError: 'Error:',
  filamentNavButton: 'Filamento',
  filamentBackToCatalogButton: 'Catálogo',
  filamentUploadImageLabel: 'Subir imagen',
```

- [ ] **Step 6: Französische Übersetzungen**

In `src/i18n/fr.ts`, nach `filamentError: 'Erreur :',` ergänzen:

```ts
  filamentError: 'Erreur :',
  filamentNavButton: 'Filament',
  filamentBackToCatalogButton: 'Catalogue',
  filamentUploadImageLabel: 'Envoyer une image',
```

- [ ] **Step 7: TypeScript-Kompilierung verifizieren**

Run: `npx tsc --noEmit`
Expected: keine Fehler (alle 4 Sprachdateien implementieren wieder vollständig das `Translations`-Interface).

- [ ] **Step 8: Commit**

```bash
git add src/types/index.ts src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "$(cat <<'EOF'
Frontend: imagePng-Feld und i18n-Texte fuer Filament-Hauptansicht (DE/EN/ES/FR)

FilamentSpool bekommt imagePng (Base64), drei neue
Uebersetzungsschluessel fuer den Header-Umschalter und den
Bild-Upload-Button. Noch nicht verwendet, das folgt in den naechsten
Commits.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

### Task 4: `Header.tsx` - Umschalter statt Icon, modell-spezifische Controls ausblenden

**Files:**
- Modify: `src/components/Header.tsx`

**Interfaces:**
- Consumes: i18n-Schlüssel `filamentNavButton`/`filamentBackToCatalogButton` (Task 3).
- Produces: `Header` erwartet ab jetzt `mainView: 'catalog' | 'filament'` und
  `onMainViewChange: (view: 'catalog' | 'filament') => void` **statt** der
  bisherigen Prop `onOpenFilamentCatalog: () => void`.

**Hinweis:** Dieser Task macht `App.tsx`s Aufruf von `<Header ... />`
vorübergehend nicht typkonform (alte Prop entfernt, neue fehlt dort noch) -
das wird in Task 6 behoben. `npx tsc --noEmit` zeigt bis dahin Fehler in
`App.tsx`, nicht in `Header.tsx` selbst - das ist erwartet.

- [ ] **Step 1: Props-Interface anpassen**

In `src/components/Header.tsx`, ersetze im `Props`-Interface die Zeile
`onOpenFilamentCatalog: () => void;` (Zeile 26):

```tsx
  onOpenFilamentCatalog: () => void;
```

durch:

```tsx
  mainView: 'catalog' | 'filament';
  onMainViewChange: (view: 'catalog' | 'filament') => void;
```

- [ ] **Step 2: Funktionssignatur anpassen**

Ersetze in der destrukturierten Funktionssignatur die Zeile
`onOpenFilamentCatalog,` (Zeile 58):

```tsx
  onOpenFilamentCatalog,
```

durch:

```tsx
  mainView,
  onMainViewChange,
```

- [ ] **Step 3: Modell-spezifische Header-Blöcke in eine Bedingung einpacken**

Der komplette Block von `<div className="relative flex">` (Import-Button,
Zeile 94) bis zum schließenden `</span>` der Dateianzahl (Zeile 183) wird
mit `{mainView === 'catalog' && (<>...</>)}` umschlossen. Konkret: ersetze

```tsx
      <div className="relative flex">
```

durch

```tsx
      {mainView === 'catalog' && (
      <>
      <div className="relative flex">
```

und ersetze

```tsx
      <span className="shrink-0 font-mono-ui text-[11px] text-[var(--ink-3)]">
        {formatCount(t('filesCount'), count)}
      </span>
```

durch

```tsx
      <span className="shrink-0 font-mono-ui text-[11px] text-[var(--ink-3)]">
        {formatCount(t('filesCount'), count)}
      </span>
      </>
      )}
```

(Alles dazwischen - Sortieren-Dropdown, Raster/Liste-Umschalter,
`flex-1`-Spacer - bleibt unverändert an seiner Stelle, liegt jetzt nur
innerhalb der neuen Bedingung.)

- [ ] **Step 4: ⊙-Icon-Button durch ausgeschriebenen Umschalter ersetzen**

Ersetze den bestehenden Button-Block (Zeilen 185-192):

```tsx
      <button
        onClick={onOpenFilamentCatalog}
        aria-label={t('filamentCatalogAria')}
        title={t('filamentCatalogAria')}
        className="shrink-0 w-8 h-8 grid place-items-center rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)] text-[15px] cursor-pointer hover:text-[var(--ink)] hover:border-[var(--line-strong)]"
      >
        ⊙
      </button>
```

durch:

```tsx
      <button
        onClick={() => onMainViewChange(mainView === 'catalog' ? 'filament' : 'catalog')}
        className="shrink-0 h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)] text-[13px] font-semibold cursor-pointer hover:text-[var(--ink)] hover:border-[var(--line-strong)]"
      >
        {mainView === 'catalog' ? t('filamentNavButton') : t('filamentBackToCatalogButton')}
      </button>
```

**Hinweis:** falls im Frontend noch der jetzt ungenutzte Schlüssel
`filamentCatalogAria` referenziert wird, ist das nach diesem Step nicht
mehr der Fall - der Schlüssel selbst bleibt im `Translations`-Interface
bestehen (kein separater Aufräum-Task nötig, ungenutzte i18n-Schlüssel
sind in diesem Projekt kein Kompilierfehler).

- [ ] **Step 5: Kompilierung prüfen**

Run: `npx tsc --noEmit`
Expected: Fehler ausschließlich in `App.tsx` (alte Prop `onOpenFilamentCatalog`
entfernt, neue Props `mainView`/`onMainViewChange` fehlen dort noch), keine
Fehler in `Header.tsx` selbst.

- [ ] **Step 6: Commit**

```bash
git add src/components/Header.tsx
git commit -m "$(cat <<'EOF'
Frontend: Filament-Umschalter im Header statt Icon-Button

Ausgeschriebener Text-Button (Filament/Katalog je nach aktuellem
mainView) ersetzt das bisherige Icon. Importieren/Sortieren/
Raster-Liste/Dateianzahl werden ausgeblendet, solange die
Filament-Ansicht aktiv ist - macht in dieser Ansicht keinen Sinn.

Macht App.tsx voruebergehend nicht kompilierbar (folgt in einem
spaeteren Commit).

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

### Task 5: `FilamentView.tsx` (ersetzt `FilamentDialog.tsx`)

**Files:**
- Create: `src/components/FilamentView.tsx`
- Delete: `src/components/FilamentDialog.tsx`

**Interfaces:**
- Consumes: `FilamentSpool` aus `../types` (Task 3, mit `imagePng`); i18n-Schlüssel aus Task 3; Backend-Commands `list_filament_spools`/`add_filament_spool`/`update_filament_spool`/`delete_filament_spool` (bestehend) sowie `pick_and_read_image` (Task 2).
- Produces: `export function FilamentView()` - **keine Props** (kein `onClose`
  mehr, das Verlassen der Ansicht läuft über `Header`s Umschalter in
  `App.tsx`).

  Task 6 rendert `<FilamentView />` in `App.tsx` (ohne Props).

- [ ] **Step 1: Neue Datei erstellen**

Erstelle `src/components/FilamentView.tsx`:

```tsx
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatWeightG, formatDiameterMm, formatPrice } from '../i18n/format';
import type { FilamentSpool } from '../types';

interface FormState {
  material: string;
  manufacturer: string;
  color: string;
  diameterMm: string;
  originalWeightG: string;
  remainingWeightG: string;
  price: string;
  imagePng: string | null;
}

const EMPTY_FORM: FormState = {
  material: '',
  manufacturer: '',
  color: '',
  diameterMm: '1.75',
  originalWeightG: '1000',
  remainingWeightG: '1000',
  price: '',
  imagePng: null,
};

const fieldClass =
  'h-8 px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-[12.5px]';

export function FilamentView() {
  const t = useT();
  const { language } = useLanguage();
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
      imagePng: spool.imagePng,
    });
  };

  const cancelForm = () => {
    setEditingId(null);
    setForm(EMPTY_FORM);
  };

  const handlePickImage = () => {
    invoke<string | null>('pick_and_read_image')
      .then((base64) => {
        if (base64 === null) return;
        setForm((prev) => ({ ...prev, imagePng: base64 }));
      })
      .catch((e) => setError(String(e)));
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
      imagePng: form.imagePng,
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
        if (editingId === id) cancelForm();
        refresh();
      })
      .catch((e) => setError(String(e)));
  };

  return (
    <div className="flex-1 min-w-0 flex flex-col min-h-0">
      <div className="flex-none px-4 py-3 border-b border-[var(--line)] text-[13px] font-semibold">
        {t('filamentDialogTitle')}
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {error && (
          <div className="pb-2 text-[12.5px] text-[var(--accent)] break-words">
            {t('filamentError')} {error}
          </div>
        )}
        {spools.length === 0 ? (
          <div className="text-[12.5px] text-[var(--ink-3)]">{t('filamentEmptyState')}</div>
        ) : (
          <div className="grid gap-3.5" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(178px, 1fr))' }}>
            {spools.map((spool) => (
              <div key={spool.id} className="rounded-[4px] overflow-hidden border border-[var(--line)]">
                <div className="relative aspect-square bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
                  {spool.imagePng ? (
                    <img
                      src={`data:image/png;base64,${spool.imagePng}`}
                      className="absolute inset-0 w-full h-full object-cover"
                    />
                  ) : (
                    <>
                      <div
                        className="absolute inset-0 opacity-90"
                        style={{
                          backgroundImage:
                            'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 9px)',
                        }}
                      />
                      <div className="absolute inset-0 grid place-items-center px-2">
                        <span className="text-[13px] font-semibold text-center truncate">{spool.material}</span>
                      </div>
                    </>
                  )}
                </div>
                <div className="flex flex-col gap-1 px-2.5 py-2 bg-[var(--panel)]">
                  <div className="text-[11.5px] text-[var(--ink-2)] truncate">
                    {[spool.manufacturer, spool.color].filter(Boolean).join(' · ') || t('noValue')}
                  </div>
                  <div className="font-mono-ui text-[10px] text-[var(--ink-3)]">
                    {formatWeightG(spool.remainingWeightG, language)} / {formatWeightG(spool.originalWeightG, language)}
                  </div>
                  <div className="font-mono-ui text-[10px] text-[var(--ink-3)]">
                    {formatDiameterMm(spool.diameterMm, language)}
                    {spool.price !== null ? ` · ${formatPrice(spool.price, language)}` : ''}
                  </div>

                  {confirmDeleteId === spool.id ? (
                    <div className="flex items-center gap-1.5 pt-1">
                      <span className="flex-1 text-[10.5px] text-[var(--ink)]">{t('deleteConfirmQuestion')}</span>
                      <button
                        onClick={() => setConfirmDeleteId(null)}
                        className="h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[10px] cursor-pointer"
                      >
                        {t('cancel')}
                      </button>
                      <button
                        onClick={() => deleteSpool(spool.id)}
                        className="h-6 px-1.5 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[10px] cursor-pointer"
                      >
                        {t('delete')}
                      </button>
                    </div>
                  ) : (
                    <div className="flex items-center justify-end gap-1.5 pt-1">
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
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="flex-none px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
        <div className="flex items-center gap-2 mb-2">
          <button
            type="button"
            onClick={handlePickImage}
            className="h-8 px-3 rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('filamentUploadImageLabel')}
          </button>
          {form.imagePng && (
            <img
              src={`data:image/png;base64,${form.imagePng}`}
              className="w-8 h-8 rounded-[3px] object-cover border border-[var(--line)]"
            />
          )}
        </div>
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
  );
}
```

**Hinweis zum Bild:** `data:image/png;base64,` wird immer als Präfix
verwendet, unabhängig vom tatsächlichen Dateiformat (auch bei
hochgeladenen JPEG/WebP-Dateien) - Browser/WebKitGTK erkennen das
tatsächliche Bildformat anhand der Datei-Magic-Bytes beim Rendern von
`<img>`, nicht anhand des deklarierten MIME-Typs im Daten-URI-Präfix.
Dadurch muss der Dateiformat-Typ nicht durch Backend/DB/Frontend
durchgereicht werden - vereinfacht die Umsetzung erheblich.

- [ ] **Step 2: Alte Datei löschen**

```bash
rm src/components/FilamentDialog.tsx
```

- [ ] **Step 3: Kompilierung prüfen**

Run: `npx tsc --noEmit`
Expected: Fehler weiterhin nur in `App.tsx` (aus Task 4, plus jetzt auch
der Import von `FilamentDialog` dort, der nicht mehr existiert) - keine
Fehler in `FilamentView.tsx` selbst.

- [ ] **Step 4: Commit**

```bash
git add src/components/FilamentView.tsx
git rm src/components/FilamentDialog.tsx
git commit -m "$(cat <<'EOF'
Frontend: FilamentView ersetzt FilamentDialog (kein Popup mehr)

Modal-Wrapper entfernt, keine Props mehr (kein onClose - das
Verlassen der Ansicht laeuft jetzt ueber den Header-Umschalter aus
Task 4). Karten-Raster nutzt wieder die responsive Spaltenregel des
Modell-Grids statt fester 2 Spalten, da jetzt die volle Fensterbreite
zur Verfuegung steht. Kartenvorschau zeigt das hochgeladene Bild
(neuer "Bild hochladen"-Button im Formular, invoke('pick_and_read_image')),
faellt ohne Bild weiterhin auf den Material-Name-Platzhalter zurueck.
Noch nirgends eingebunden (App.tsx folgt im naechsten Commit).

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
- Consumes: `Header`s neue Props (Task 4), `FilamentView` (Task 5, keine Props).
- Produces: nichts (Blatt-Task, letzter Schritt des Plans).

- [ ] **Step 1: Import ersetzen**

Ersetze in `src/App.tsx` die Zeile:

```tsx
import { FilamentDialog } from './components/FilamentDialog';
```

durch:

```tsx
import { FilamentView } from './components/FilamentView';
```

- [ ] **Step 2: State umbenennen**

Ersetze die Zeile:

```tsx
  const [filamentDialogOpen, setFilamentDialogOpen] = useState(false);
```

durch:

```tsx
  const [mainView, setMainView] = useState<'catalog' | 'filament'>('catalog');
```

- [ ] **Step 3: Prop-Übergabe an `Header` ändern**

Ersetze die Zeile im `<Header ... />`-Aufruf:

```tsx
        onOpenFilamentCatalog={() => setFilamentDialogOpen(true)}
```

durch:

```tsx
        mainView={mainView}
        onMainViewChange={setMainView}
```

- [ ] **Step 4: Hauptbereich bedingt rendern**

Ersetze den kompletten Block von `<div className="flex-1 flex min-h-0">`
(Zeile 272) bis zu dessen schließendem `</div>` (Zeile 346, direkt vor
`{contextMenu && (`):

```tsx
      {mainView === 'catalog' ? (
        <div className="flex-1 flex min-h-0">
          <Sidebar
            query={query}
            onQueryChange={setQuery}
            folders={folders}
            activeFolderId={activeFolderId}
            onFolderSelect={setActiveFolderId}
            tags={tags}
            activeTag={activeTag}
            onTagSelect={setActiveTag}
            clouds={clouds}
            cloudError={cloudError}
            onAddCloud={() => connectCloud('gdrive')}
            onConnectCloud={connectCloud}
            onDisconnectCloud={(id) => {
              invoke('disconnect_cloud_account', { provider: id })
                .then(() => {
                  setCloudError(null);
                  refreshClouds();
                })
                .catch((e) => {
                  console.error('[cloud] Trennen fehlgeschlagen:', e);
                  setCloudError(String(e));
                });
            }}
          />

          <main className="flex-1 min-w-0 flex flex-col min-h-0">
            <div className="flex-none h-[38px] flex items-center gap-2.5 px-4 border-b border-[var(--line)] bg-[var(--bg)]">
              <span className="font-mono-ui text-[11px] text-[var(--ink-2)]">
                {folders.find((f) => f.id === activeFolderId)?.name}
              </span>
              {activeTag && (
                <span
                  onClick={() => setActiveTag(null)}
                  className="flex items-center gap-1.5 h-[22px] px-2 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11px] cursor-pointer"
                >
                  #{activeTag} ✕
                </span>
              )}
            </div>

            <div className="flex-1 overflow-y-auto p-4">
              {view === 'grid' ? (
                <ModelGrid
                  models={filtered}
                  selectedId={selectedId}
                  onSelect={setSelectedId}
                  onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                />
              ) : (
                <ModelList
                  models={filtered}
                  selectedId={selectedId}
                  onSelect={setSelectedId}
                  onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                />
              )}
            </div>
          </main>

          <DetailPanel
            model={selected}
            onAddTag={(t) => selected && addTag(selected.id, t)}
            onRemoveTag={(t) => selected && removeTag(selected.id, t)}
            onDelete={() => selected && deleteModel(selected.id)}
            onOpenInSlicer={(slicerId) => selected && openInSlicer(selected.id, slicerId)}
            slicers={slicers}
            slicerError={slicerError}
            onUploadToCloud={() => selected && uploadWithFolderPicker(selected.id)}
            cloudUploadAvailable={clouds.some((c) => c.id === 'gdrive' && c.status === 'connected')}
            uploading={uploadingId !== null && uploadingId === selected?.id}
            cloudUploadError={cloudUploadError}
          />
        </div>
      ) : (
        <FilamentView />
      )}
```

(Inhaltlich identisch zum bisherigen Block - `Sidebar`/`main`/`DetailPanel`
unverändert - nur jetzt in `{mainView === 'catalog' ? (...) : (<FilamentView />)}`
eingebettet statt bedingungslos gerendert.)

- [ ] **Step 5: Letzten Dialog-Aufruf entfernen**

Entferne die Zeile am Ende der Datei (vor der schließenden `</div>` der
Wurzelkomponente):

```tsx
      {filamentDialogOpen && <FilamentDialog onClose={() => setFilamentDialogOpen(false)} />}
```

(Ersatzlos - `FilamentView` wird jetzt in Step 4 als Teil des
Hauptbereichs gerendert, kein zusätzliches bedingtes Overlay mehr nötig.)

- [ ] **Step 6: TypeScript-Kompilierung verifizieren**

Run: `npx tsc --noEmit`
Expected: keine Fehler.

- [ ] **Step 7: Commit**

```bash
git add src/App.tsx
git commit -m "$(cat <<'EOF'
Frontend: Filament-Hauptansicht vollstaendig verdrahtet

mainView-State ersetzt filamentDialogOpen. Im Filament-Modus werden
Sidebar und DetailPanel nicht gerendert, FilamentView nimmt die volle
Breite des bisherigen Hauptbereichs ein. Damit ist das Feature aus
Spec 2026-09-10-filament-catalog-main-view-design.md vollstaendig
umgesetzt.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

- [ ] **Step 8: Manueller Live-Test (Abschlusskriterium des gesamten Plans)**

Dieser Schritt lässt sich nicht automatisieren:

1. `npm run tauri dev` starten (oder den bereits laufenden Dev-Server
   nutzen - Rust-Änderungen lösen automatisch einen Neubau aus).
2. Header-Button "Filament" klicken - Hauptansicht wechselt komplett
   (Sidebar und rechter Detailbereich verschwinden, Importieren/
   Sortieren/Raster|Liste/Dateianzahl verschwinden aus dem Header).
3. Eine neue Spule über das Formular anlegen, dabei "Bild hochladen"
   nutzen und eine echte Bilddatei wählen - die Vorschau im Formular
   erscheint sofort, nach dem Speichern zeigt die Karte im Raster das
   hochgeladene Bild statt des Material-Name-Platzhalters.
4. Diese Spule über ✎ bearbeiten - die vorhandene Bild-Vorschau
   erscheint im Formular.
5. Eine zweite Spule ohne Bild anlegen - Karte zeigt weiterhin den
   bisherigen Platzhalter (Material-Name + Schraffur).
6. Header-Button "Katalog" klicken - Hauptansicht wechselt zurück zum
   gewohnten Modell-Katalog, alle vorher ausgeblendeten Header-Elemente
   und Sidebar/Detailbereich erscheinen wieder normal.
