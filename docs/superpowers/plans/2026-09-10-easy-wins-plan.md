# "Leicht"-Features Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Vier unabhängige "Leicht"-Features aus der Konkurrenz-App-Analyse in 3mf-katalog-manager ergänzen: Druckstatus+Gewicht pro Modell, "Zuletzt angesehen"-Sortierung + NEU-Badge, Creators-Filterkategorie, exakte Duplikat-Erkennung beim Import.

**Architecture:** Alle vier Bausteine docken an die bestehende `files`-Tabelle an (vier neue, überwiegend nullable Spalten). Task 1 legt das gemeinsame Datenfundament (Schema, Migration, `NewFile`/`FileRecord`, `insert_file`/`get_file`/`list_files`) einmal zentral an, damit die folgenden vier Tasks nur noch je einen eigenen, in sich abgeschlossenen Befehl/DTO/UI-Pfad ergänzen, ohne dieselben SQL-Strings mehrfach anzufassen.

**Tech Stack:** Tauri v2 (Rust-Backend, `rusqlite` ohne Migrationsframework) + React/TypeScript-Frontend, eigenes i18n-Context-System (de/en/es/fr).

## Global Constraints

- SQLite ohne Migrationsframework: `CREATE TABLE IF NOT EXISTS` ändert eine bereits bestehende Tabelle nicht. Jede neue Spalte braucht zusätzlich eine fehlertolerante `ALTER TABLE ... ADD COLUMN`-Zeile in `repository.rs`s `init()` (Vorbild: die bereits vorhandene `image_png`-Zeile), da die echte, bereits befüllte Produktions-DB sonst die neuen Spalten nie bekäme.
- Kein Rust-seitiges Validieren von `CHECK`-constrained Strings (z. B. `print_status`) - gleiches Muster wie das bestehende `set_file_sync_status`: der DB-`CHECK` ist die einzige Durchsetzung.
- i18n: jeder neue sichtbare Text bekommt einen Key in `src/i18n/types.ts` (`Translations`-Interface) und in allen vier Sprachdateien (`de.ts`, `en.ts`, `es.ts`, `fr.ts`). "Creators" bleibt als Lehnwort in allen vier Sprachen identisch (gleiches Muster wie das bestehende `tagsHeading: 'Tags'` in allen vier Sprachen).
- Rust-Testabdeckung nur auf Repository-Ebene (`db/mod.rs`) - `commands.rs`s Import-Funktionen (`import_one`, `import_many`) sind in diesem Projekt bewusst nicht unit-getestet (brauchen echte Dateien auf der Platte), sondern nur live verifiziert. Reine Funktionen ohne Dateisystemzugriff (z. B. `estimate_weight_g`) bekommen dagegen einen Unit-Test in `commands.rs`s bestehendem `#[cfg(test)] mod tests`-Block.
- `cargo test` (Backend) und `npx tsc --noEmit` (Frontend) müssen nach jedem Task sauber durchlaufen.
- Planungs-Verfeinerung gegenüber der Spec (`docs/superpowers/specs/2026-09-10-easy-wins-design.md`): `NewFile.content_hash`/`FileRecord.content_hash` sind `Option<String>` statt des in der Spec genannten `String` - vermeidet einen künstlichen Platzhalter-String für Dateien, die vor Task 5 (echte Hash-Berechnung) importiert werden, und ist symmetrisch zur ohnehin nullable-en DB-Spalte.

---

## Task 1: Datenbank-Fundament + Creator-Extraktion beim Import

Legt alle vier neuen Spalten auf `files` an, erweitert die gemeinsamen Lese-/Schreibpfade (`insert_file`/`get_file`/`list_files`) und erledigt dabei bereits die komplette Creator-Extraktions-Logik (Baustein 3) sowie die Standardwerte für Druckstatus/Zuletzt-angesehen (Bausteine 1+2), da das ohne zusätzlichen Aufwand direkt an derselben Stelle passiert. Nur die echte Hash-Berechnung (Baustein 4) bleibt bewusst ein Platzhalter (`None`) für Task 5 - sie braucht eine neue Abhängigkeit (`sha2`), die hier noch nicht gerechtfertigt ist.

**Files:**
- Modify: `src-tauri/src/db/schema.sql`
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/cloud/commands.rs`
- Modify: `src-tauri/src/db/mod.rs`

**Interfaces:**
- Produces: `NewFile`/`FileRecord` (`src-tauri/src/db/models.rs`) mit vier neuen Feldern: `print_status: String`, `last_viewed_at: Option<String>`, `creator: Option<String>`, `content_hash: Option<String>`.
- Produces: `import_one(conn, path, origin, cloud_id, display_name, content_hash: Option<String>)` (`src-tauri/src/commands.rs`, `pub(crate)`) - neuer 6. Parameter, wird 1:1 nach `NewFile.content_hash` durchgereicht. Tasks 2-5 rufen diese Funktion nicht direkt auf (nur `import_many`/`import_from_cloud` tun das), betrifft sie also nicht.
- Consumes: nichts von späteren Tasks.

- [ ] **Step 1: Schema erweitern**

In `src-tauri/src/db/schema.sql` die `files`-Tabelle um vier Spalten und einen Index ergänzen. Die aktuelle Tabelle endet so:

```sql
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    file_type TEXT NOT NULL CHECK (file_type IN ('3mf', 'stl')),
    folder_id INTEGER REFERENCES folders (id) ON DELETE SET NULL,
    origin TEXT NOT NULL DEFAULT 'local'
        CHECK (origin IN ('local', 'gdrive', 'onedrive', 'dropbox', 'proton')),
    sync_status TEXT NOT NULL DEFAULT 'local-only'
        CHECK (sync_status IN ('synced', 'outdated', 'local-only', 'cloud-only')),
    cloud_id TEXT,
    file_size_bytes INTEGER NOT NULL,
    dimension_x_mm REAL,
    dimension_y_mm REAL,
    dimension_z_mm REAL,
    volume_cm3 REAL,
    object_count INTEGER,
    thumbnail_png BLOB,
    imported_at TEXT NOT NULL,
    file_modified_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_files_folder_id ON files (folder_id);
CREATE INDEX IF NOT EXISTS idx_files_file_type ON files (file_type);
```

Ersetzen durch:

```sql
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    file_type TEXT NOT NULL CHECK (file_type IN ('3mf', 'stl')),
    folder_id INTEGER REFERENCES folders (id) ON DELETE SET NULL,
    origin TEXT NOT NULL DEFAULT 'local'
        CHECK (origin IN ('local', 'gdrive', 'onedrive', 'dropbox', 'proton')),
    sync_status TEXT NOT NULL DEFAULT 'local-only'
        CHECK (sync_status IN ('synced', 'outdated', 'local-only', 'cloud-only')),
    cloud_id TEXT,
    file_size_bytes INTEGER NOT NULL,
    dimension_x_mm REAL,
    dimension_y_mm REAL,
    dimension_z_mm REAL,
    volume_cm3 REAL,
    object_count INTEGER,
    thumbnail_png BLOB,
    imported_at TEXT NOT NULL,
    file_modified_at TEXT,
    print_status TEXT NOT NULL DEFAULT 'not_printed'
        CHECK (print_status IN ('not_printed', 'printed')),
    last_viewed_at TEXT,
    creator TEXT,
    content_hash TEXT
);

CREATE INDEX IF NOT EXISTS idx_files_folder_id ON files (folder_id);
CREATE INDEX IF NOT EXISTS idx_files_file_type ON files (file_type);
CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files (content_hash);
```

- [ ] **Step 2: Migration für Bestands-DBs**

In `src-tauri/src/db/repository.rs`s `init()`-Funktion (aktuell Zeile 27-38) nach der bestehenden `image_png`-ALTER-TABLE-Zeile vier weitere ergänzen:

```rust
fn init(conn: &Connection) -> Result<(), DbError> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.execute_batch(SCHEMA_SQL)?;
    // filament_spools.image_png wurde nachtraeglich zur bereits bestehenden
    // Tabelle hinzugefuegt (kein Migrations-Framework in diesem Projekt) -
    // CREATE TABLE IF NOT EXISTS aendert eine schon vorhandene Tabelle nicht.
    // ALTER TABLE laeuft daher hier zusaetzlich und wird bewusst ignoriert,
    // falls die Spalte (auf einer frisch angelegten DB, wo CREATE TABLE sie
    // schon mitbringt) bereits existiert.
    let _ = conn.execute("ALTER TABLE filament_spools ADD COLUMN image_png BLOB", []);
    // Gleiches Muster fuer vier neue files-Spalten (Druckstatus, Zuletzt-
    // angesehen, Creator, Inhalts-Hash) auf einer bereits befuellten
    // Produktions-DB.
    let _ = conn.execute(
        "ALTER TABLE files ADD COLUMN print_status TEXT NOT NULL DEFAULT 'not_printed'
            CHECK (print_status IN ('not_printed', 'printed'))",
        [],
    );
    let _ = conn.execute("ALTER TABLE files ADD COLUMN last_viewed_at TEXT", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN creator TEXT", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN content_hash TEXT", []);
    Ok(())
}
```

- [ ] **Step 3: `NewFile`/`FileRecord` erweitern**

In `src-tauri/src/db/models.rs` beide Structs (aktuell Zeile 32-51 und 53-73) um vier Felder ergänzen, jeweils direkt vor der schließenden `}`, nach dem bestehenden `tags`-Feld:

```rust
#[derive(Debug, Clone)]
pub struct NewFile {
    pub name: String,
    pub path: String,
    pub file_type: FileType,
    pub folder_id: Option<i64>,
    pub origin: String,
    pub cloud_id: Option<String>,
    pub sync_status: String,
    pub file_size_bytes: i64,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub thumbnail_png: Option<Vec<u8>>,
    pub imported_at: String,
    pub file_modified_at: Option<String>,
    pub materials: Vec<MaterialRecord>,
    pub metadata: BTreeMap<String, String>,
    pub tags: Vec<String>,
    pub print_status: String,
    pub last_viewed_at: Option<String>,
    pub creator: Option<String>,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub file_type: FileType,
    pub folder_id: Option<i64>,
    pub origin: String,
    pub sync_status: String,
    pub cloud_id: Option<String>,
    pub file_size_bytes: i64,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub thumbnail_png: Option<Vec<u8>>,
    pub imported_at: String,
    pub file_modified_at: Option<String>,
    pub materials: Vec<MaterialRecord>,
    pub metadata: BTreeMap<String, String>,
    pub tags: Vec<String>,
    pub print_status: String,
    pub last_viewed_at: Option<String>,
    pub creator: Option<String>,
    pub content_hash: Option<String>,
}
```

- [ ] **Step 4: `insert_file`/`get_file`/`list_files`/`row_to_file` erweitern**

In `src-tauri/src/db/repository.rs`s `insert_file` (aktuell Zeile 158-217) die `INSERT`-Anweisung um die vier neuen Spalten am Ende ergänzen:

```rust
    tx.execute(
        "INSERT INTO files (
            name, path, file_type, folder_id, origin, cloud_id, sync_status,
            file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
            volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
            print_status, last_viewed_at, creator, content_hash
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        params![
            file.name,
            file.path,
            file.file_type.as_str(),
            file.folder_id,
            file.origin,
            file.cloud_id,
            file.sync_status,
            file.file_size_bytes,
            dim_x,
            dim_y,
            dim_z,
            file.volume_cm3,
            file.object_count,
            file.thumbnail_png,
            file.imported_at,
            file.file_modified_at,
            file.print_status,
            file.last_viewed_at,
            file.creator,
            file.content_hash,
        ],
    )?;
```

`get_file` (aktuell Zeile 245-264) und `list_files` (aktuell Zeile 266-283) beide SQL-Statements um die vier Spalten am Ende erweitern:

```rust
pub fn get_file(conn: &Connection, id: i64) -> Result<Option<FileRecord>, DbError> {
    let row = conn
        .query_row(
            "SELECT id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
                    file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
                    volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
                    print_status, last_viewed_at, creator, content_hash
             FROM files WHERE id = ?1",
            params![id],
            row_to_file,
        )
        .optional()?;

    let Some(mut file) = row else {
        return Ok(None);
    };
    file.materials = load_materials(conn, id)?;
    file.metadata = load_metadata(conn, id)?;
    file.tags = load_tags(conn, id)?;
    Ok(Some(file))
}

pub fn list_files(conn: &Connection) -> Result<Vec<FileRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
                file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
                volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
                print_status, last_viewed_at, creator, content_hash
         FROM files ORDER BY name",
    )?;
    let mut files = stmt
        .query_map([], row_to_file)?
        .collect::<Result<Vec<_>, _>>()?;

    for file in &mut files {
        file.materials = load_materials(conn, file.id)?;
        file.metadata = load_metadata(conn, file.id)?;
        file.tags = load_tags(conn, file.id)?;
    }
    Ok(files)
}
```

`row_to_file` (aktuell Zeile 285-315) um die vier neuen Spalten (Indizes 17-20) ergänzen:

```rust
fn row_to_file(row: &rusqlite::Row) -> rusqlite::Result<FileRecord> {
    let file_type_str: String = row.get(3)?;
    let dim_x: Option<f64> = row.get(9)?;
    let dim_y: Option<f64> = row.get(10)?;
    let dim_z: Option<f64> = row.get(11)?;
    let dimensions_mm = match (dim_x, dim_y, dim_z) {
        (Some(x), Some(y), Some(z)) => Some([x, y, z]),
        _ => None,
    };

    Ok(FileRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        file_type: FileType::parse(&file_type_str).unwrap_or(FileType::ThreeMf),
        folder_id: row.get(4)?,
        origin: row.get(5)?,
        sync_status: row.get(6)?,
        cloud_id: row.get(7)?,
        file_size_bytes: row.get(8)?,
        dimensions_mm,
        volume_cm3: row.get(12)?,
        object_count: row.get(13)?,
        thumbnail_png: row.get(14)?,
        imported_at: row.get(15)?,
        file_modified_at: row.get(16)?,
        materials: Vec::new(),
        metadata: BTreeMap::new(),
        tags: Vec::new(),
        print_status: row.get(17)?,
        last_viewed_at: row.get(18)?,
        creator: row.get(19)?,
        content_hash: row.get(20)?,
    })
}
```

- [ ] **Step 5: `import_one` erweitern und Creator-Extraktion einbauen**

In `src-tauri/src/commands.rs`s `import_one` (aktuell Zeile 296-388) die Signatur um den neuen Parameter `content_hash` ergänzen und die `NewFile`-Konstruktion (aktuell Zeile 363-381) um die vier neuen Felder erweitern. `creator` wird direkt aus der bereits vorhandenen `metadata`-Map gelesen (funktioniert für STL automatisch mit, da `metadata` dort schon eine leere `BTreeMap` ist):

```rust
pub(crate) fn import_one(
    conn: &mut Connection,
    path: &Path,
    origin: &str,
    cloud_id: Option<String>,
    display_name: Option<&str>,
    content_hash: Option<String>,
) -> CmdResult<ModelFileDto> {
    // Bei Cloud-Importen ist `path` aus Sicherheitsgruenden (kein Path
    // Traversal ueber den Drive-Dateinamen) ein von der Datei-ID abgeleiteter
    // Cache-Pfad, nicht der echte Dateiname - display_name liefert dann den
    // tatsaechlichen Namen fuer Katalog-Anzeige UND Auto-Tagging.
    let file_name = display_name.map(|n| n.to_string()).unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unbenannt")
            .to_string()
    });
    let file_size_bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len() as i64;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    let (file_type, dimensions_mm, volume_cm3, object_count, materials, metadata, thumbnail_png) =
        match extension.as_deref() {
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
                )
            }
            _ => return Err("nicht unterstütztes Dateiformat".to_string()),
        };

    let tags = tagging::suggest_tags(&TaggingContext {
        file_name: &file_name,
        dimensions_mm,
        object_count,
        materials: &materials,
    });

    let sync_status = if origin == "local" { "local-only" } else { "synced" };

    let new_file = NewFile {
        name: file_name,
        path: path.to_string_lossy().to_string(),
        file_type,
        folder_id: None,
        origin: origin.to_string(),
        cloud_id,
        sync_status: sync_status.to_string(),
        file_size_bytes,
        dimensions_mm,
        volume_cm3,
        object_count,
        thumbnail_png,
        imported_at: chrono::Utc::now().to_rfc3339(),
        file_modified_at: None,
        materials,
        metadata: metadata.clone(),
        tags,
        print_status: "not_printed".to_string(),
        last_viewed_at: None,
        creator: metadata.get("Designer").cloned(),
        content_hash,
    };

    let id = db::insert_file(conn, &new_file).map_err(|e| e.to_string())?;
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "imported file not found after insert".to_string())?;
    Ok(to_dto(file))
}
```

Wichtig: `metadata` wird jetzt zweimal gebraucht (`creator: metadata.get("Designer").cloned()` UND `metadata,` als eigenes Feld) - daher `metadata: metadata.clone()` statt der bisherigen Kurzform `metadata,`, damit der Borrow für `.get("Designer")` nicht mit dem Move kollidiert. Die Reihenfolge der beiden Zeilen im Struct-Literal spielt dabei keine Rolle, `metadata.clone()` und `metadata.get(...)` sind beides nur Borrows.

- [ ] **Step 6: Aufrufstellen von `import_one` anpassen**

In `src-tauri/src/commands.rs`s `import_many` (aktuell Zeile 394-425) den `import_one`-Aufruf um `None` erweitern (echter Hash kommt erst in Task 5):

```rust
        match import_one(&mut conn, &path, "local", None, None, None) {
            Ok(dto) => imported.push(dto),
            Err(e) => eprintln!("[import] Import fehlgeschlagen für {path_str}: {e}"),
        }
```

In `src-tauri/src/cloud/commands.rs`s `import_from_cloud` (aktuell Zeile 312-318) ebenso:

```rust
            let dto = match import_one(
                &mut conn,
                &cache_path,
                "gdrive",
                Some(file_id.clone()),
                Some(&metadata.name),
                None,
            ) {
```

- [ ] **Step 7: Test-Helper und Roundtrip-Test**

In `src-tauri/src/db/mod.rs`s `sample_file()`-Helper (aktuell Zeile 22-45) die vier neuen Felder am Ende des Struct-Literals ergänzen:

```rust
    fn sample_file() -> NewFile {
        let mut metadata = BTreeMap::new();
        metadata.insert("Designer".to_string(), "Jane".to_string());

        NewFile {
            name: "cube.3mf".to_string(),
            path: "/tmp/cube.3mf".to_string(),
            file_type: FileType::ThreeMf,
            folder_id: None,
            origin: "local".to_string(),
            cloud_id: None,
            sync_status: "local-only".to_string(),
            file_size_bytes: 1024,
            dimensions_mm: Some([10.0, 10.0, 10.0]),
            volume_cm3: Some(1.0),
            object_count: Some(1),
            thumbnail_png: None,
            imported_at: "2026-09-08T12:00:00Z".to_string(),
            file_modified_at: None,
            materials: vec![MaterialRecord {
                name: "PLA".to_string(),
                display_color: Some("#ff0000".to_string()),
            }],
            metadata,
            tags: vec!["cube".to_string(), "test".to_string()],
            print_status: "not_printed".to_string(),
            last_viewed_at: None,
            creator: None,
            content_hash: None,
        }
    }
```

Neuen Test direkt danach (vor `#[test] fn inserts_and_reads_back_a_file`) einfügen:

```rust
    #[test]
    fn stores_and_lists_the_new_file_columns() {
        let mut conn = connect_in_memory().expect("connect");
        let mut file = sample_file();
        file.print_status = "printed".to_string();
        file.creator = Some("Jane".to_string());
        file.content_hash = Some("abc123".to_string());
        let id = insert_file(&mut conn, &file).expect("insert");

        let stored = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(stored.print_status, "printed");
        assert_eq!(stored.last_viewed_at, None);
        assert_eq!(stored.creator, Some("Jane".to_string()));
        assert_eq!(stored.content_hash, Some("abc123".to_string()));
    }
```

- [ ] **Step 8: Testen**

Run: `cd src-tauri && cargo test`
Expected: alle bisherigen Tests weiterhin grün, plus der neue `stores_and_lists_the_new_file_columns`-Test.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/db/schema.sql src-tauri/src/db/repository.rs src-tauri/src/db/models.rs src-tauri/src/commands.rs src-tauri/src/cloud/commands.rs src-tauri/src/db/mod.rs
git commit -m "Rust: Datenfundament fuer Druckstatus, Zuletzt-angesehen, Creator, Inhalts-Hash

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V"
```

---

## Task 2: Druckstatus + geschätztes Gewicht (Baustein 1)

**Files:**
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types/index.ts`
- Modify: `src/i18n/types.ts`
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`
- Modify: `src/components/DetailPanel.tsx`
- Modify: `src/components/ModelGrid.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `FileRecord.print_status: String` (Task 1), `ModelFile` (`src/types/index.ts`, bisher ohne `printStatus`/`estimatedWeightG`).
- Produces: Tauri-Command `set_print_status(fileId: String, status: String) -> CmdResult<()>`. `ModelFileDto`/`ModelFile` mit `printStatus: 'not_printed' | 'printed'` und `estimatedWeightG: number | null`.

- [ ] **Step 1: Repository-Funktion**

In `src-tauri/src/db/repository.rs` nach `set_file_sync_status` (aktuell Zeile 400-403) einfügen:

```rust
pub fn set_print_status(conn: &Connection, file_id: i64, status: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET print_status = ?1 WHERE id = ?2",
        params![status, file_id],
    )?;
    Ok(())
}
```

- [ ] **Step 2: Export + Test**

In `src-tauri/src/db/mod.rs`s `pub use repository::{...}`-Liste (aktuell Zeile 5-10) `set_print_status` alphabetisch einsortieren (zwischen `set_file_sync_status` und `update_filament_spool`).

Neuen Test in `src-tauri/src/db/mod.rs`s `mod tests` nach `stores_and_lists_the_new_file_columns` einfügen:

```rust
    #[test]
    fn set_print_status_updates_the_status() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        set_print_status(&conn, id, "printed").expect("update");
        let file = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(file.print_status, "printed");
    }
```

Run: `cd src-tauri && cargo test set_print_status_updates_the_status`
Expected: PASS

- [ ] **Step 3: Gewichtsschätzung + DTO + Command**

In `src-tauri/src/commands.rs` nach `to_dto` (aktuell Zeile 63-89) die Dichte-Tabelle und Schätzfunktion einfügen:

```rust
const MATERIAL_DENSITY_G_CM3: &[(&str, f64)] = &[
    ("pla", 1.24),
    ("petg", 1.27),
    ("abs", 1.04),
    ("tpu", 1.21),
    ("asa", 1.05),
    ("pc", 1.20),
    ("nylon", 1.14),
];
const DEFAULT_DENSITY_G_CM3: f64 = 1.24;

pub(crate) fn estimate_weight_g(volume_cm3: Option<f64>, material_name: Option<&str>) -> Option<f64> {
    let volume = volume_cm3?;
    let density = material_name
        .and_then(|name| {
            let lower = name.to_lowercase();
            MATERIAL_DENSITY_G_CM3
                .iter()
                .find(|(key, _)| lower.contains(key))
                .map(|(_, d)| *d)
        })
        .unwrap_or(DEFAULT_DENSITY_G_CM3);
    Some(volume * density)
}
```

`ModelFileDto` (aktuell Zeile 22-38) um zwei Felder erweitern:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFileDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder_id: String,
    pub tags: Vec<String>,
    pub origin: String,
    pub sync: String,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub materials: Vec<MaterialDto>,
    pub file_size_bytes: i64,
    pub imported_at: String,
    pub print_status: String,
    pub estimated_weight_g: Option<f64>,
}
```

`to_dto` (aktuell Zeile 63-89) anpassen, damit die Gewichtsschätzung vor dem Verbrauch von `file.materials` berechnet wird:

```rust
pub(crate) fn to_dto(file: FileRecord) -> ModelFileDto {
    let estimated_weight_g =
        estimate_weight_g(file.volume_cm3, file.materials.first().map(|m| m.name.as_str()));
    ModelFileDto {
        id: file.id.to_string(),
        name: file.name,
        path: file.path,
        folder_id: file
            .folder_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        tags: file.tags,
        origin: file.origin,
        sync: file.sync_status,
        dimensions_mm: file.dimensions_mm,
        volume_cm3: file.volume_cm3,
        object_count: file.object_count,
        materials: file
            .materials
            .into_iter()
            .map(|m| MaterialDto {
                name: m.name,
                display_color: m.display_color,
            })
            .collect(),
        file_size_bytes: file.file_size_bytes,
        imported_at: file.imported_at,
        print_status: file.print_status,
        estimated_weight_g,
    }
}
```

Neuer Command nach `delete_file` (aktuell Zeile 256-271):

```rust
#[tauri::command]
pub fn set_print_status(state: State<AppState>, file_id: String, status: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_print_status(&conn, id, &status).map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Unit-Test für `estimate_weight_g`**

In `src-tauri/src/commands.rs`s bestehendem `#[cfg(test)] mod tests`-Block (aktuell ab Zeile 612) nach den vorhandenen Tests einfügen:

```rust
    #[test]
    fn estimate_weight_g_uses_known_material_density() {
        let grams = estimate_weight_g(Some(10.0), Some("Generic PLA")).expect("weight");
        assert!((grams - 12.4).abs() < 0.001);
    }

    #[test]
    fn estimate_weight_g_falls_back_to_default_density_for_unknown_material() {
        let grams = estimate_weight_g(Some(10.0), Some("Mystery-Filament")).expect("weight");
        assert!((grams - 12.4).abs() < 0.001);
    }

    #[test]
    fn estimate_weight_g_returns_none_without_volume() {
        assert_eq!(estimate_weight_g(None, Some("PLA")), None);
    }
```

Run: `cd src-tauri && cargo test estimate_weight_g`
Expected: PASS (3 Tests)

- [ ] **Step 5: Command registrieren**

In `src-tauri/src/lib.rs`s `generate_handler!`-Liste (aktuell Zeile 39-62) nach `commands::delete_file,` einfügen:

```rust
            commands::delete_file,
            commands::set_print_status,
```

- [ ] **Step 6: Frontend-Typ + i18n**

In `src/types/index.ts`s `ModelFile`-Interface (aktuell Zeile 5-18) nach `importedAt: string;` ergänzen:

```ts
  importedAt: string;
  printStatus: 'not_printed' | 'printed';
  estimatedWeightG: number | null;
```

In `src/i18n/types.ts`s `Translations`-Interface nach `filamentUploadImageLabel: string;` (aktuell letzte Zeile vor der schließenden `}`, Zeile 112) fünf neue Keys ergänzen:

```ts
  filamentUploadImageLabel: string;

  printedBadge: string;
  notPrintedLabel: string;
  markAsPrinted: string;
  markAsNotPrinted: string;
  metaWeight: string;
}
```

In allen vier Sprachdateien nach `filamentUploadImageLabel: '...'` (jeweils letzte Zeile vor der schließenden `};`) die Übersetzungen ergänzen.

`src/i18n/de.ts`:
```ts
  filamentUploadImageLabel: 'Bild hochladen',

  printedBadge: 'Gedruckt',
  notPrintedLabel: 'Nicht gedruckt',
  markAsPrinted: 'Als gedruckt markieren',
  markAsNotPrinted: 'Als nicht gedruckt markieren',
  metaWeight: 'Gewicht (geschätzt)',
};
```

`src/i18n/en.ts`:
```ts
  filamentUploadImageLabel: 'Upload image',

  printedBadge: 'Printed',
  notPrintedLabel: 'Not printed',
  markAsPrinted: 'Mark as printed',
  markAsNotPrinted: 'Mark as not printed',
  metaWeight: 'Weight (estimated)',
};
```

`src/i18n/es.ts`:
```ts
  filamentUploadImageLabel: 'Subir imagen',

  printedBadge: 'Impreso',
  notPrintedLabel: 'No impreso',
  markAsPrinted: 'Marcar como impreso',
  markAsNotPrinted: 'Marcar como no impreso',
  metaWeight: 'Peso (estimado)',
};
```

`src/i18n/fr.ts`:
```ts
  filamentUploadImageLabel: 'Envoyer une image',

  printedBadge: 'Imprimé',
  notPrintedLabel: 'Non imprimé',
  markAsPrinted: 'Marquer comme imprimé',
  markAsNotPrinted: 'Marquer comme non imprimé',
  metaWeight: 'Poids (estimé)',
};
```

- [ ] **Step 7: DetailPanel - Toggle + Gewichtsanzeige**

In `src/components/DetailPanel.tsx` den Import (aktuell Zeile 5) um `formatWeightG` erweitern:

```ts
import { formatBytes, formatDate, formatDimensions, formatRelativeTime, formatVolumeCm3, formatWeightG } from '../i18n/format';
```

`buildMetaRows` (aktuell Zeile 31-46) um die Gewichtszeile ergänzen (direkt nach der Volumen-Zeile):

```ts
function buildMetaRows(model: ModelFile, t: TFunction, language: Language): { label: string; value: string }[] {
  const materialsValue =
    model.materials.length === 0
      ? t('noValue')
      : model.materials.map((m) => m.name).join(', ');
  const objectCountValue = model.objectCount === null ? t('noValue') : String(model.objectCount);
  const weightValue =
    model.estimatedWeightG === null ? t('noValue') : `≈ ${formatWeightG(model.estimatedWeightG, language)}`;

  return [
    { label: t('metaDimensions'), value: formatDimensions(model.dimensionsMm, language) },
    { label: t('metaVolume'), value: formatVolumeCm3(model.volumeCm3, language) },
    { label: t('metaWeight'), value: weightValue },
    { label: t('metaObjectCount'), value: objectCountValue },
    { label: t('metaMaterial'), value: materialsValue },
    { label: t('metaFileSize'), value: formatBytes(model.fileSizeBytes, language) },
    { label: t('metaImported'), value: formatDate(model.importedAt, language) },
  ];
}
```

`Props`-Interface (aktuell Zeile 8-20) um eine neue Prop ergänzen:

```ts
interface Props {
  model: ModelFile | null;
  onAddTag: (tag: string) => void;
  onRemoveTag: (tag: string) => void;
  onDelete: () => void;
  onTogglePrintStatus: () => void;
  onOpenInSlicer: (slicerId?: string) => void;
  slicers: SlicerConfig[];
  slicerError: string | null;
  onUploadToCloud: () => void;
  cloudUploadAvailable: boolean;
  uploading: boolean;
  cloudUploadError: string | null;
}
```

Funktionssignatur (aktuell Zeile 48-60) entsprechend um `onTogglePrintStatus` erweitern (gleiche Position wie im Interface, nach `onDelete`).

Neue Zeile direkt nach dem bestehenden Sync-Status-Block (aktuell Zeile 125-135, endet mit dem schließenden `</div>` vor `<div className="px-4 pt-3.5 pb-1">`) einfügen:

```tsx
        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span className="flex-1 text-[12.5px] font-medium">
            {model.printStatus === 'printed' ? t('printedBadge') : t('notPrintedLabel')}
          </span>
          <button
            onClick={onTogglePrintStatus}
            className="h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {model.printStatus === 'printed' ? t('markAsNotPrinted') : t('markAsPrinted')}
          </button>
        </div>
```

- [ ] **Step 8: ModelGrid - Badge**

In `src/components/ModelGrid.tsx` nach dem `originAbbr`-Badge-Block (aktuell Zeile 51-55, endet mit `)}`) einfügen:

```tsx
            {m.printStatus === 'printed' && (
              <div className="absolute right-[7px] bottom-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]">
                ✓ {t('printedBadge')}
              </div>
            )}
```

- [ ] **Step 9: App.tsx verdrahten**

In `src/App.tsx` nach `deleteModel` (aktuell Zeile 239-246) eine neue Funktion einfügen:

```tsx
  const togglePrintStatus = (id: string) => {
    const current = models.find((m) => m.id === id);
    if (!current) return;
    const next = current.printStatus === 'printed' ? 'not_printed' : 'printed';
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, printStatus: next } : m)));
    invoke('set_print_status', { fileId: id, status: next }).catch((e) => {
      console.error('[print-status] Aktualisieren fehlgeschlagen:', e);
    });
  };
```

`<DetailPanel ...>`-Aufruf (aktuell Zeile 336-348) um die neue Prop ergänzen:

```tsx
          <DetailPanel
            model={selected}
            onAddTag={(t) => selected && addTag(selected.id, t)}
            onRemoveTag={(t) => selected && removeTag(selected.id, t)}
            onDelete={() => selected && deleteModel(selected.id)}
            onTogglePrintStatus={() => selected && togglePrintStatus(selected.id)}
            onOpenInSlicer={(slicerId) => selected && openInSlicer(selected.id, slicerId)}
            slicers={slicers}
            slicerError={slicerError}
            onUploadToCloud={() => selected && uploadWithFolderPicker(selected.id)}
            cloudUploadAvailable={clouds.some((c) => c.id === 'gdrive' && c.status === 'connected')}
            uploading={uploadingId !== null && uploadingId === selected?.id}
            cloudUploadError={cloudUploadError}
          />
```

- [ ] **Step 10: Testen**

Run: `cd src-tauri && cargo test` (Backend) und `npx tsc --noEmit` (Frontend, aus dem Projekt-Root)
Expected: beide sauber, keine Fehler.

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/types/index.ts src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts src/components/DetailPanel.tsx src/components/ModelGrid.tsx src/App.tsx
git commit -m "Druckstatus-Toggle + geschaetztes Gewicht pro Modell

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V"
```

---

## Task 3: "Zuletzt angesehen" + NEU-Badge (Baustein 2)

**Files:**
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types/index.ts`
- Modify: `src/i18n/types.ts`
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`
- Modify: `src/components/Header.tsx`
- Modify: `src/components/ModelGrid.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `FileRecord.last_viewed_at: Option<String>` (Task 1), `ModelFile.importedAt` (bereits vorhanden).
- Produces: Tauri-Command `mark_file_viewed(fileId: String) -> CmdResult<()>`. `ModelFileDto`/`ModelFile` mit `lastViewedAt: string | null`. `SortKey` erweitert um `'viewed'`.

- [ ] **Step 1: Repository-Funktion + Test**

In `src-tauri/src/db/repository.rs` nach der in Task 2 ergänzten `set_print_status` einfügen:

```rust
pub fn mark_file_viewed(conn: &Connection, file_id: i64) -> Result<(), DbError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE files SET last_viewed_at = ?1 WHERE id = ?2",
        params![now, file_id],
    )?;
    Ok(())
}
```

In `src-tauri/src/db/mod.rs`s `pub use repository::{...}`-Liste `mark_file_viewed` alphabetisch einsortieren.

Neuen Test nach `set_print_status_updates_the_status` einfügen:

```rust
    #[test]
    fn mark_file_viewed_sets_a_timestamp() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");
        let before = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(before.last_viewed_at, None);

        mark_file_viewed(&conn, id).expect("mark viewed");
        let after = get_file(&conn, id).expect("query").expect("present");
        assert!(after.last_viewed_at.is_some());
    }
```

Run: `cd src-tauri && cargo test mark_file_viewed_sets_a_timestamp`
Expected: PASS

- [ ] **Step 2: DTO + Command**

In `src-tauri/src/commands.rs`s `ModelFileDto` (nach Task 2 zuletzt endend mit `estimated_weight_g`) ein weiteres Feld ergänzen:

```rust
    pub print_status: String,
    pub estimated_weight_g: Option<f64>,
    pub last_viewed_at: Option<String>,
```

`to_dto` entsprechend ergänzen (nach `estimated_weight_g,`):

```rust
        print_status: file.print_status,
        estimated_weight_g,
        last_viewed_at: file.last_viewed_at,
```

Neuer Command nach `set_print_status` (aus Task 2):

```rust
#[tauri::command]
pub fn mark_file_viewed(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::mark_file_viewed(&conn, id).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Command registrieren**

In `src-tauri/src/lib.rs`s `generate_handler!`-Liste nach `commands::set_print_status,` (aus Task 2) einfügen:

```rust
            commands::set_print_status,
            commands::mark_file_viewed,
```

- [ ] **Step 4: Frontend-Typ + i18n**

In `src/types/index.ts`s `ModelFile`-Interface nach `estimatedWeightG: number | null;` (aus Task 2) ergänzen:

```ts
  estimatedWeightG: number | null;
  lastViewedAt: string | null;
```

`SortKey` (aktuell Zeile 43) erweitern:

```ts
export type SortKey = 'name' | 'date' | 'size' | 'vol' | 'viewed';
```

In `src/i18n/types.ts` nach `metaWeight: string;` (aus Task 2, letzte Zeile vor `}`) zwei Keys ergänzen:

```ts
  metaWeight: string;

  sortLastViewed: string;
  newBadge: string;
}
```

In allen vier Sprachdateien nach `metaWeight: '...'` (aus Task 2) ergänzen:

`src/i18n/de.ts`:
```ts
  metaWeight: 'Gewicht (geschätzt)',

  sortLastViewed: 'Zuletzt angesehen',
  newBadge: 'NEU',
};
```

`src/i18n/en.ts`:
```ts
  metaWeight: 'Weight (estimated)',

  sortLastViewed: 'Last viewed',
  newBadge: 'NEW',
};
```

`src/i18n/es.ts`:
```ts
  metaWeight: 'Peso (estimado)',

  sortLastViewed: 'Visto recientemente',
  newBadge: 'NUEVO',
};
```

`src/i18n/fr.ts`:
```ts
  metaWeight: 'Poids (estimé)',

  sortLastViewed: 'Vu récemment',
  newBadge: 'NOUVEAU',
};
```

- [ ] **Step 5: Header-Sortieroption**

In `src/components/Header.tsx`s Sortier-`<select>` (aktuell Zeile 156-165) nach `<option value="vol">{t('sortVolume')}</option>` ergänzen:

```tsx
          <option value="vol">{t('sortVolume')}</option>
          <option value="viewed">{t('sortLastViewed')}</option>
```

- [ ] **Step 6: ModelGrid NEU-Badge**

In `src/components/ModelGrid.tsx` vor dem `originAbbr`-Badge-Block (aktuell Zeile 51, innerhalb desselben `relative`-Containers) einfügen:

```tsx
            {Date.now() - new Date(m.importedAt).getTime() < 24 * 60 * 60 * 1000 && (
              <div className="absolute left-[7px] top-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]">
                {t('newBadge')}
              </div>
            )}
            {originAbbr[m.origin] && (
```

(Die letzte Zeile ist die bereits bestehende erste Zeile des `originAbbr`-Blocks - nur zur Verortung, nicht neu hinzuzufügen.)

- [ ] **Step 7: App.tsx - Sortierung + Zuletzt-angesehen-Tracking**

In `src/App.tsx`s `filtered`-`useMemo` (aktuell Zeile 166-177) einen neuen Sortier-Zweig ergänzen:

```tsx
  const filtered = useMemo(() => {
    return models
      .filter((m) => activeFolderId === 'all' || m.folderId === activeFolderId)
      .filter((m) => !activeTag || m.tags.includes(activeTag))
      .filter((m) => !query || m.name.toLowerCase().includes(query.toLowerCase()))
      .sort((a, b) => {
        if (sort === 'name') return a.name.localeCompare(b.name);
        if (sort === 'size') return a.fileSizeBytes - b.fileSizeBytes;
        if (sort === 'date') return b.importedAt.localeCompare(a.importedAt);
        if (sort === 'viewed') return (b.lastViewedAt ?? '').localeCompare(a.lastViewedAt ?? '');
        return 0;
      });
  }, [models, activeFolderId, activeTag, query, sort]);
```

Neue Funktion `selectModel` nach `mergeImported` (aktuell Zeile 125-131) einfügen - aktualisiert `lastViewedAt` sofort optimistisch im lokalen State, damit die "Zuletzt angesehen"-Sortierung noch in derselben Sitzung sofort korrekt ist, statt erst nach einem Neustart der App (das initiale `list_files` beim Mount ist die einzige Stelle, an der `models` sonst neu geladen wird):

```tsx
  const selectModel = (id: string) => {
    setSelectedId(id);
    const now = new Date().toISOString();
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, lastViewedAt: now } : m)));
    invoke('mark_file_viewed', { fileId: id }).catch((e) => {
      console.error('[last-viewed] Aktualisieren fehlgeschlagen:', e);
    });
  };
```

Beide `onSelect={setSelectedId}`-Stellen (aktuell Zeile 322 in `ModelGrid` und Zeile 329 in `ModelList`) auf `onSelect={selectModel}` ändern.

- [ ] **Step 8: Testen**

Run: `cd src-tauri && cargo test` (Backend) und `npx tsc --noEmit` (Frontend)
Expected: beide sauber.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/types/index.ts src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts src/components/Header.tsx src/components/ModelGrid.tsx src/App.tsx
git commit -m "Sortierung nach Zuletzt-angesehen + NEU-Badge fuer kuerzlich importierte Modelle

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V"
```

---

## Task 4: Creators-Filter (Baustein 3)

**Files:**
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types/index.ts`
- Modify: `src/i18n/types.ts`
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`
- Modify: `src/components/Sidebar.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `FileRecord.creator: Option<String>` (Task 1, wird dort bereits beim Import befüllt).
- Produces: Tauri-Command `list_creators() -> CmdResult<Vec<CreatorCountDto>>` mit `CreatorCountDto { label: String, count: i64 }`. Frontend-Typ `CreatorCount { label: string; count: number }`. `ModelFileDto`/`ModelFile` erweitert um `creator: Option<String>`/`creator: string | null` (nötig für den Pro-Modell-Filter in `App.tsx`, nicht nur die Sidebar-Aggregation).

- [ ] **Step 1: `CreatorCount`-Struct + Repository-Funktion**

In `src-tauri/src/db/models.rs` nach `TagCount` (aktuell Zeile 81-86) einfügen:

```rust
#[derive(Debug, Clone)]
pub struct CreatorCount {
    pub name: String,
    pub count: i64,
}
```

In `src-tauri/src/db/repository.rs` nach `list_tag_counts` (aktuell endet Zeile 156) einfügen:

```rust
pub fn list_creator_counts(conn: &Connection) -> Result<Vec<CreatorCount>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT creator, COUNT(*) FROM files WHERE creator IS NOT NULL GROUP BY creator ORDER BY creator",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CreatorCount {
                name: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
```

Import am Kopf von `repository.rs` (aktuell Zeile 7-10) um `CreatorCount` ergänzen:

```rust
use super::models::{
    CloudAccountRecord, FileRecord, FileType, FilamentSpoolRecord, FolderRecord, MaterialRecord,
    NewFile, NewFilamentSpool, TagCount, CreatorCount,
};
```

- [ ] **Step 2: Export + Test**

In `src-tauri/src/db/mod.rs`s `pub use repository::{...}`-Liste `list_creator_counts` alphabetisch einsortieren.

Neuen Test nach `mark_file_viewed_sets_a_timestamp` einfügen:

```rust
    #[test]
    fn list_creator_counts_groups_by_creator_and_excludes_missing() {
        let mut conn = connect_in_memory().expect("connect");

        let mut a = sample_file();
        a.creator = Some("Jane".to_string());
        insert_file(&mut conn, &a).expect("insert 1");

        let mut b = sample_file();
        b.name = "second.3mf".to_string();
        b.path = "/tmp/second.3mf".to_string();
        b.creator = Some("Jane".to_string());
        insert_file(&mut conn, &b).expect("insert 2");

        let mut c = sample_file();
        c.name = "third.stl".to_string();
        c.path = "/tmp/third.stl".to_string();
        c.creator = None;
        insert_file(&mut conn, &c).expect("insert 3");

        let counts = list_creator_counts(&conn).expect("list");
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].name, "Jane");
        assert_eq!(counts[0].count, 2);
    }
```

Run: `cd src-tauri && cargo test list_creator_counts_groups_by_creator_and_excludes_missing`
Expected: PASS

- [ ] **Step 3: `ModelFileDto` um `creator` erweitern**

`list_creators` liefert nur die aggregierte Zählung für die Sidebar - der eigentliche Filter in `App.tsx` (Step 7) vergleicht `m.creator === activeCreator` pro Modell und braucht das Feld deshalb auch auf `ModelFileDto`/`ModelFile` selbst, nicht nur in der Aggregation.

In `src-tauri/src/commands.rs`s `ModelFileDto` (nach Task 3 zuletzt endend mit `last_viewed_at`) ein weiteres Feld ergänzen:

```rust
    pub print_status: String,
    pub estimated_weight_g: Option<f64>,
    pub last_viewed_at: Option<String>,
    pub creator: Option<String>,
```

`to_dto` entsprechend ergänzen (nach `last_viewed_at: file.last_viewed_at,`):

```rust
        print_status: file.print_status,
        estimated_weight_g,
        last_viewed_at: file.last_viewed_at,
        creator: file.creator,
```

- [ ] **Step 4: `CreatorCountDto` + `list_creators`-Command**

In `src-tauri/src/commands.rs` nach `TagCountDto` (aktuell Zeile 55-61) einfügen:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorCountDto {
    pub label: String,
    pub count: i64,
}
```

Neuer Command nach `list_tag_counts` (aktuell Zeile 129-141):

```rust
#[tauri::command]
pub fn list_creators(state: State<AppState>) -> CmdResult<Vec<CreatorCountDto>> {
    let conn = lock_db(&state)?;
    let creators = db::list_creator_counts(&conn).map_err(|e| e.to_string())?;
    Ok(creators
        .into_iter()
        .map(|c| CreatorCountDto {
            label: c.name,
            count: c.count,
        })
        .collect())
}
```

- [ ] **Step 5: Command registrieren**

In `src-tauri/src/lib.rs`s `generate_handler!`-Liste nach `commands::list_tag_counts,` einfügen:

```rust
            commands::list_tag_counts,
            commands::list_creators,
```

- [ ] **Step 6: Frontend-Typ + i18n**

In `src/types/index.ts` nach `TagCount` (aktuell Zeile 27-31) einfügen:

```ts
export interface CreatorCount {
  label: string;
  count: number;
}
```

`ModelFile`-Interface nach `lastViewedAt: string | null;` (aus Task 3) ergänzen:

```ts
  lastViewedAt: string | null;
  creator: string | null;
```

In `src/i18n/types.ts` nach `newBadge: string;` (aus Task 3, letzte Zeile vor `}`) ergänzen:

```ts
  newBadge: string;

  creatorsHeading: string;
}
```

In allen vier Sprachdateien nach `newBadge: '...'` (aus Task 3) ergänzen - "Creators" bleibt bewusst identisch in allen vier Sprachen, gleiches Muster wie das bestehende `tagsHeading: 'Tags'`:

`src/i18n/de.ts`:
```ts
  newBadge: 'NEU',

  creatorsHeading: 'Creators',
};
```

`src/i18n/en.ts`:
```ts
  newBadge: 'NEW',

  creatorsHeading: 'Creators',
};
```

`src/i18n/es.ts`:
```ts
  newBadge: 'NUEVO',

  creatorsHeading: 'Creators',
};
```

`src/i18n/fr.ts`:
```ts
  newBadge: 'NOUVEAU',

  creatorsHeading: 'Creators',
};
```

- [ ] **Step 7: Sidebar - neue Sektion**

In `src/components/Sidebar.tsx`s `Props`-Interface (aktuell Zeile 4-18) drei neue Props ergänzen:

```ts
interface Props {
  query: string;
  onQueryChange: (q: string) => void;
  folders: Folder[];
  activeFolderId: string;
  onFolderSelect: (id: string) => void;
  tags: TagCount[];
  activeTag: string | null;
  onTagSelect: (label: string | null) => void;
  creators: CreatorCount[];
  activeCreator: string | null;
  onCreatorSelect: (label: string | null) => void;
  clouds: CloudAccount[];
  cloudError: string | null;
  onAddCloud: () => void;
  onConnectCloud: (id: string) => void;
  onDisconnectCloud: (id: string) => void;
}
```

Import (aktuell Zeile 1) um `CreatorCount` ergänzen:

```ts
import type { Folder, TagCount, CreatorCount, CloudAccount } from '../types';
```

Funktionssignatur (aktuell Zeile 37-51) entsprechend um die drei neuen Props erweitern (gleiche Position wie im Interface, nach `onTagSelect`).

Nach dem Tags-Block (aktuell Zeile 92-111, endet mit `))}` gefolgt von `</div>` in Zeile 112) eine neue Sektion einfügen, direkt vor dem schließenden `</div>` der scrollbaren Liste:

```tsx
        <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] px-1.5 pt-[18px] pb-2">
          {t('creatorsHeading')}
        </div>
        {creators.map((creator) => (
          <div
            key={creator.label}
            onClick={() => onCreatorSelect(activeCreator === creator.label ? null : creator.label)}
            className={`flex items-center gap-2 h-7 px-1.5 rounded-[3px] cursor-pointer ${
              activeCreator === creator.label
                ? 'bg-[var(--accent-soft)] text-[var(--accent)]'
                : 'text-[var(--ink-2)] hover:text-[var(--ink)]'
            }`}
          >
            <span className="flex-1 font-mono-ui text-xs overflow-hidden text-ellipsis whitespace-nowrap">
              {creator.label}
            </span>
            <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">{creator.count}</span>
          </div>
        ))}
```

- [ ] **Step 8: App.tsx verdrahten**

In `src/App.tsx`s Import (aktuell Zeile 13) `CreatorCount` ergänzen:

```tsx
import type { ModelFile, Folder, TagCount, CreatorCount, CloudAccount, Origin, ViewMode, SortKey } from './types';
```

Neuer State und Refresh-Funktion nach `refreshTags` (aktuell Zeile 61):

```tsx
  const refreshTags = () => invoke<TagCount[]>('list_tag_counts').then(setTags);
  const refreshCreators = () => invoke<CreatorCount[]>('list_creators').then(setCreators);
```

Neuer State direkt bei den übrigen `useState`-Deklarationen, nach `const [tags, setTags] = useState<TagCount[]>([]);` (aktuell Zeile 51):

```tsx
  const [tags, setTags] = useState<TagCount[]>([]);
  const [creators, setCreators] = useState<CreatorCount[]>([]);
  const [activeCreator, setActiveCreator] = useState<string | null>(null);
```

Im Mount-`useEffect` (aktuell Zeile 133-141) `refreshCreators()` neben `refreshTags()` aufrufen:

```tsx
  useEffect(() => {
    invoke<ModelFile[]>('list_files').then((files) => {
      setModels(files);
      setSelectedId((prev) => prev ?? files[0]?.id ?? null);
    });
    refreshFolders();
    refreshTags();
    refreshCreators();
    refreshClouds();
  }, []);
```

In `mergeImported` (aktuell Zeile 125-131) `refreshCreators()` neben `refreshTags()` aufrufen:

```tsx
  const mergeImported = (files: ModelFile[]) => {
    if (!files.length) return;
    setModels((prev) => [...prev, ...files]);
    setSelectedId(files[files.length - 1].id);
    refreshFolders();
    refreshTags();
    refreshCreators();
  };
```

`filtered`-`useMemo` (aktuell Zeile 166-177, nach Task 3 bereits um den `viewed`-Sortierzweig erweitert) um einen zusätzlichen Filter ergänzen:

```tsx
  const filtered = useMemo(() => {
    return models
      .filter((m) => activeFolderId === 'all' || m.folderId === activeFolderId)
      .filter((m) => !activeTag || m.tags.includes(activeTag))
      .filter((m) => !activeCreator || m.creator === activeCreator)
      .filter((m) => !query || m.name.toLowerCase().includes(query.toLowerCase()))
      .sort((a, b) => {
        if (sort === 'name') return a.name.localeCompare(b.name);
        if (sort === 'size') return a.fileSizeBytes - b.fileSizeBytes;
        if (sort === 'date') return b.importedAt.localeCompare(a.importedAt);
        if (sort === 'viewed') return (b.lastViewedAt ?? '').localeCompare(a.lastViewedAt ?? '');
        return 0;
      });
  }, [models, activeFolderId, activeTag, activeCreator, query, sort]);
```

`<Sidebar ...>`-Aufruf (aktuell Zeile 276-300) um die drei neuen Props ergänzen:

```tsx
          <Sidebar
            query={query}
            onQueryChange={setQuery}
            folders={folders}
            activeFolderId={activeFolderId}
            onFolderSelect={setActiveFolderId}
            tags={tags}
            activeTag={activeTag}
            onTagSelect={setActiveTag}
            creators={creators}
            activeCreator={activeCreator}
            onCreatorSelect={setActiveCreator}
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
```

Im Ordner-Leisten-Bereich (aktuell Zeile 303-315) nach dem `activeTag`-Chip einen analogen Chip für `activeCreator` ergänzen:

```tsx
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
              {activeCreator && (
                <span
                  onClick={() => setActiveCreator(null)}
                  className="flex items-center gap-1.5 h-[22px] px-2 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11px] cursor-pointer"
                >
                  {activeCreator} ✕
                </span>
              )}
            </div>
```

- [ ] **Step 9: Testen**

Run: `cd src-tauri && cargo test` (Backend) und `npx tsc --noEmit` (Frontend)
Expected: beide sauber.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/db/models.rs src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/types/index.ts src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts src/components/Sidebar.tsx src/App.tsx
git commit -m "Creators als eigene Sidebar-Filterkategorie

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V"
```

---

## Task 5: Exakte Duplikat-Erkennung beim Import (Baustein 4)

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/cloud/commands.rs`
- Modify: `src/types/index.ts` (nur falls nötig - hier keine Änderung, `ImportResultDto` ist reiner Command-Rückgabetyp)
- Modify: `src/i18n/types.ts`
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`
- Create: `src/components/ImportSummaryBanner.tsx`
- Modify: `src/App.tsx`
- Modify: `CHANGELOG.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: `import_one`s `content_hash: Option<String>`-Parameter (Task 1, bisher immer `None`).
- Produces: `ImportResultDto { imported: Vec<ModelFileDto>, duplicate_count: i64 }` als neuer Rückgabetyp von `import_files`/`import_folder`/`import_dropped`/`import_from_cloud` (ersetzt `Vec<ModelFileDto>`). Frontend-seitig `ImportResultDto { imported: ModelFile[]; duplicateCount: number }`.

- [ ] **Step 1: Dependency**

In `src-tauri/Cargo.toml`s `[dependencies]`-Block (aktuell endet Zeile 35 mit `base64 = "0.22"`) ergänzen:

```toml
base64 = "0.22"
sha2 = "0.10"
```

- [ ] **Step 2: Repository-Funktion + Test**

In `src-tauri/src/db/repository.rs` nach `file_exists_by_path` (aktuell Zeile 236-243) einfügen:

```rust
pub fn file_exists_by_hash(conn: &Connection, hash: &str) -> Result<bool, DbError> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM files WHERE content_hash = ?1",
            params![hash],
            |row| row.get(0),
        )
        .optional()?;
    Ok(exists.is_some())
}
```

In `src-tauri/src/db/mod.rs`s `pub use repository::{...}`-Liste `file_exists_by_hash` alphabetisch einsortieren.

Neuen Test nach `list_creator_counts_groups_by_creator_and_excludes_missing` einfügen:

```rust
    #[test]
    fn file_exists_by_hash_finds_only_matching_hash() {
        let mut conn = connect_in_memory().expect("connect");
        let mut file = sample_file();
        file.content_hash = Some("hash-a".to_string());
        insert_file(&mut conn, &file).expect("insert");

        assert!(file_exists_by_hash(&conn, "hash-a").expect("query"));
        assert!(!file_exists_by_hash(&conn, "hash-b").expect("query"));
    }
```

Run: `cd src-tauri && cargo test file_exists_by_hash_finds_only_matching_hash`
Expected: PASS

- [ ] **Step 3: Hash-Berechnung + `ImportResultDto`**

In `src-tauri/src/commands.rs` nach `is_supported_extension` (aktuell Zeile 273-278) einfügen:

```rust
pub(crate) fn compute_content_hash(path: &Path) -> CmdResult<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}
```

Nach `TagCountDto` (bzw. direkt vor `CreatorCountDto` aus Task 4) einfügen:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultDto {
    pub imported: Vec<ModelFileDto>,
    pub duplicate_count: i64,
}
```

- [ ] **Step 4: `import_many` umbauen**

`import_many` (aktuell Zeile 394-425) komplett ersetzen durch:

```rust
fn import_many(state: &State<AppState>, roots: Vec<PathBuf>) -> CmdResult<ImportResultDto> {
    let mut candidates = Vec::new();
    for root in roots {
        collect_supported_files(&root, &mut candidates);
    }

    let mut conn = lock_db(state)?;
    let mut seen = HashSet::new();
    let mut imported = Vec::new();
    let mut duplicate_count = 0i64;

    for path in candidates {
        let path_str = path.to_string_lossy().to_string();
        if !seen.insert(path_str.clone()) {
            continue;
        }
        match db::file_exists_by_path(&conn, &path_str) {
            Ok(true) => continue,
            Ok(false) => {}
            Err(e) => {
                eprintln!("[import] Duplikatprüfung fehlgeschlagen für {path_str}: {e}");
                continue;
            }
        }

        let content_hash = match compute_content_hash(&path) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[import] Hash fehlgeschlagen für {path_str}: {e}");
                continue;
            }
        };
        match db::file_exists_by_hash(&conn, &content_hash) {
            Ok(true) => {
                duplicate_count += 1;
                continue;
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!("[import] Duplikatprüfung (Hash) fehlgeschlagen für {path_str}: {e}");
                continue;
            }
        }

        match import_one(&mut conn, &path, "local", None, None, Some(content_hash)) {
            Ok(dto) => imported.push(dto),
            Err(e) => eprintln!("[import] Import fehlgeschlagen für {path_str}: {e}"),
        }
    }

    Ok(ImportResultDto { imported, duplicate_count })
}
```

`import_files`/`import_folder`/`import_dropped` (aktuell Zeile 427-465) brauchen nur den Rückgabetyp angepasst, der Funktionskörper ruft weiterhin unverändert `import_many` auf:

```rust
#[tauri::command]
pub async fn import_files(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ImportResultDto> {
    let picked = app
        .dialog()
        .file()
        .add_filter("3D-Modelle", &["3mf", "stl"])
        .blocking_pick_files();

    let Some(picked) = picked else {
        return Ok(ImportResultDto { imported: Vec::new(), duplicate_count: 0 });
    };
    let paths = picked
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .collect();
    import_many(&state, paths)
}

#[tauri::command]
pub async fn import_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ImportResultDto> {
    let picked = app.dialog().file().blocking_pick_folder();

    let Some(picked) = picked else {
        return Ok(ImportResultDto { imported: Vec::new(), duplicate_count: 0 });
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    import_many(&state, vec![path])
}

#[tauri::command]
pub fn import_dropped(state: State<AppState>, paths: Vec<String>) -> CmdResult<ImportResultDto> {
    import_many(&state, paths.into_iter().map(PathBuf::from).collect())
}
```

- [ ] **Step 5: Cloud-Import umbauen**

In `src-tauri/src/cloud/commands.rs`s Import-Liste (aktuell Zeile 10) `ImportResultDto` und `compute_content_hash` ergänzen:

```rust
use crate::commands::{compute_content_hash, import_one, lock_db, to_dto, AppState, ImportResultDto, ModelFileDto};
```

`import_from_cloud` (aktuell Zeile 214-342): Signatur-Rückgabetyp ändern, `duplicate_count` mitzählen, Hash-Prüfung nach dem Schreiben in den Cache und vor dem eigentlichen Import einfügen, finale Rückgabe anpassen:

```rust
#[tauri::command]
pub async fn import_from_cloud(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    file_ids: Vec<String>,
) -> CmdResult<ImportResultDto> {
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

    let mut imported = Vec::new();
    let mut duplicate_count = 0i64;
    for file_id in file_ids {
        let metadata = match with_gdrive_provider(&state, {
            let file_id = file_id.clone();
            move |provider| {
                let file_id = file_id.clone();
                async move { provider.get_metadata(&file_id).await }
            }
        })
        .await
        {
            Ok(metadata) => metadata,
            Err(e) => {
                eprintln!("[cloud-import] Metadaten fehlgeschlagen fuer {file_id}: {e}");
                continue;
            }
        };

        let extension = match std::path::Path::new(&metadata.name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .filter(|e| e == "3mf" || e == "stl")
        {
            Some(ext) => ext,
            None => {
                eprintln!(
                    "[cloud-import] nicht unterstuetztes Dateiformat fuer {file_id} ({}): uebersprungen",
                    metadata.name
                );
                continue;
            }
        };
        let cache_path = cache_dir.join(format!("{file_id}.{extension}"));
        let cache_path_str = cache_path.to_string_lossy().to_string();

        let already_imported = {
            let conn = lock_db(&state)?;
            match db::file_exists_by_path(&conn, &cache_path_str) {
                Ok(exists) => exists,
                Err(e) => {
                    eprintln!("[cloud-import] Duplikatpruefung fehlgeschlagen fuer {file_id}: {e}");
                    continue;
                }
            }
        };
        if already_imported {
            continue;
        }

        let data = match with_gdrive_provider(&state, {
            let file_id = file_id.clone();
            move |provider| {
                let file_id = file_id.clone();
                async move { provider.download(&file_id).await }
            }
        })
        .await
        {
            Ok(data) => data,
            Err(e) => {
                eprintln!("[cloud-import] Download fehlgeschlagen fuer {file_id}: {e}");
                continue;
            }
        };

        if let Err(e) = std::fs::write(&cache_path, &data) {
            eprintln!("[cloud-import] Schreiben in Cache fehlgeschlagen fuer {file_id}: {e}");
            continue;
        }

        // Hash erst NACH dem Schreiben moeglich (die Bytes liegen vorher nur
        // im RAM, nicht unter cache_path) - anders als beim lokalen Import in
        // import_many, wo die Quelldatei schon vor dem Import existiert.
        let content_hash = match compute_content_hash(&cache_path) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[cloud-import] Hash fehlgeschlagen fuer {file_id}: {e}");
                continue;
            }
        };
        let is_duplicate = {
            let conn = lock_db(&state)?;
            match db::file_exists_by_hash(&conn, &content_hash) {
                Ok(exists) => exists,
                Err(e) => {
                    eprintln!("[cloud-import] Duplikatpruefung (Hash) fehlgeschlagen fuer {file_id}: {e}");
                    continue;
                }
            }
        };
        if is_duplicate {
            duplicate_count += 1;
            continue;
        }

        // Ein durchgehender Lock-Scope genuegt hier: zwischen den beiden
        // DB-Aufrufen liegt kein .await, daher kein Konflikt mit der
        // "nie ueber .await halten"-Regel aus den Global Constraints.
        let dto = {
            let mut conn = lock_db(&state)?;
            let dto = match import_one(
                &mut conn,
                &cache_path,
                "gdrive",
                Some(file_id.clone()),
                Some(&metadata.name),
                Some(content_hash),
            ) {
                Ok(dto) => dto,
                Err(e) => {
                    eprintln!("[cloud-import] Import fehlgeschlagen fuer {file_id}: {e}");
                    continue;
                }
            };
            let id: i64 = match dto.id.parse() {
                Ok(id) => id,
                Err(_) => {
                    eprintln!("[cloud-import] ungueltige Datei-ID nach Import fuer {file_id}");
                    continue;
                }
            };
            if let Err(e) = db::set_file_modified_at(&conn, id, &metadata.modified_time) {
                eprintln!("[cloud-import] file_modified_at fehlgeschlagen fuer {file_id}: {e}");
            }
            dto
        };

        imported.push(dto);
    }

    Ok(ImportResultDto { imported, duplicate_count })
}
```

- [ ] **Step 5b: Testen (Backend)**

Run: `cd src-tauri && cargo test`
Expected: alle Tests grün (inkl. der 4 neuen aus Task 1/2/3/4/5).

- [ ] **Step 6: i18n**

In `src/i18n/types.ts` nach `creatorsHeading: string;` (aus Task 4, letzte Zeile vor `}`) ergänzen:

```ts
  creatorsHeading: string;

  importSummaryText: string;
}
```

In allen vier Sprachdateien nach `creatorsHeading: '...'` (aus Task 4) ergänzen:

`src/i18n/de.ts`:
```ts
  creatorsHeading: 'Creators',

  importSummaryText: '{imported} importiert, {duplicates} Duplikate übersprungen',
};
```

`src/i18n/en.ts`:
```ts
  creatorsHeading: 'Creators',

  importSummaryText: '{imported} imported, {duplicates} duplicates skipped',
};
```

`src/i18n/es.ts`:
```ts
  creatorsHeading: 'Creators',

  importSummaryText: '{imported} importados, {duplicates} duplicados omitidos',
};
```

`src/i18n/fr.ts`:
```ts
  creatorsHeading: 'Creators',

  importSummaryText: '{imported} importés, {duplicates} doublons ignorés',
};
```

- [ ] **Step 7: `ImportSummaryBanner`-Komponente**

Neue Datei `src/components/ImportSummaryBanner.tsx`:

```tsx
import { useEffect } from 'react';
import { useT } from '../i18n/LanguageContext';

interface Props {
  imported: number;
  duplicates: number;
  onClose: () => void;
}

export function ImportSummaryBanner({ imported, duplicates, onClose }: Props) {
  const t = useT();

  useEffect(() => {
    const timer = setTimeout(onClose, 5000);
    return () => clearTimeout(timer);
  }, [onClose]);

  return (
    <div className="fixed bottom-4 right-4 z-50 flex items-center gap-2.5 px-3.5 py-2.5 rounded-[4px] border border-[var(--line)] bg-[var(--panel)] shadow-[var(--shadow)] text-[12.5px] text-[var(--ink)]">
      <span>
        {t('importSummaryText')
          .replace('{imported}', String(imported))
          .replace('{duplicates}', String(duplicates))}
      </span>
      <span
        onClick={onClose}
        className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[10px] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
      >
        ✕
      </span>
    </div>
  );
}
```

- [ ] **Step 8: App.tsx verdrahten**

In `src/App.tsx` nach den bestehenden Interface-Deklarationen (aktuell Zeile 15-24, nach `PickerResultDto`) ein neues Interface ergänzen:

```tsx
interface ImportResultDto {
  imported: ModelFile[];
  duplicateCount: number;
}
```

Import um `ImportSummaryBanner` ergänzen (nach der `FilamentView`-Import-Zeile, aktuell Zeile 10):

```tsx
import { FilamentView } from './components/FilamentView';
import { ImportSummaryBanner } from './components/ImportSummaryBanner';
```

Neuer State bei den übrigen `useState`-Deklarationen, nach `const [mainView, setMainView] = useState<'catalog' | 'filament'>('catalog');` (aktuell Zeile 58):

```tsx
  const [mainView, setMainView] = useState<'catalog' | 'filament'>('catalog');
  const [importBanner, setImportBanner] = useState<{ imported: number; duplicates: number } | null>(null);
```

`mergeImported` (aktuell Zeile 125-131, nach Task 4 bereits um `refreshCreators()` erweitert) komplett ersetzen:

```tsx
  const mergeImported = (result: ImportResultDto) => {
    if (result.imported.length) {
      setModels((prev) => [...prev, ...result.imported]);
      setSelectedId(result.imported[result.imported.length - 1].id);
      refreshFolders();
      refreshTags();
      refreshCreators();
    }
    if (result.duplicateCount > 0) {
      setImportBanner({ imported: result.imported.length, duplicates: result.duplicateCount });
    }
  };
```

`handleCloudImport` (aktuell Zeile 83-92) anpassen:

```tsx
  const handleCloudImport = (fileIds: string[]) => {
    return invoke<ImportResultDto>('import_from_cloud', { fileIds })
      .then(mergeImported)
      .catch((e) => {
        console.error('[cloud] Import aus Google Drive fehlgeschlagen:', e);
        setCloudError(String(e));
      });
  };
```

Drag-and-Drop-`useEffect` (aktuell Zeile 143-152) den `invoke`-Aufruf anpassen:

```tsx
      invoke<ImportResultDto>('import_dropped', { paths: event.payload.paths }).then(mergeImported);
```

`importFiles`/`importFolder` (aktuell Zeile 199-200) anpassen:

```tsx
  const importFiles = () => invoke<ImportResultDto>('import_files').then(mergeImported);
  const importFolder = () => invoke<ImportResultDto>('import_folder').then(mergeImported);
```

Banner-Rendering am Ende der JSX (aktuell Zeile 354-362, nach dem `contextMenu`-Block, vor dem schließenden `</div>` der Wurzelkomponente) ergänzen:

```tsx
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          onOpenInSlicer={() => openInSlicer(contextMenu.modelId)}
          onDelete={() => deleteModel(contextMenu.modelId)}
        />
      )}

      {importBanner && (
        <ImportSummaryBanner
          imported={importBanner.imported}
          duplicates={importBanner.duplicates}
          onClose={() => setImportBanner(null)}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 9: Testen**

Run: `cd src-tauri && cargo test` (Backend) und `npx tsc --noEmit` (Frontend)
Expected: beide sauber.

- [ ] **Step 10: CHANGELOG.md und README.md aktualisieren**

In `CHANGELOG.md`s `### Added`-Abschnitt unter `[Unreleased]` (aktuell endet mit der Filament-Lager-Zeile, Zeile 27) eine neue Zeile ergänzen:

```markdown
- Filament-Lager: eigenständige Spulenverwaltung (Material, Hersteller, Farbe, Durchmesser, Ursprungs-/Restgewicht, Preis) über ein neues Header-Icon erreichbar, unabhängig vom Modell-Katalog - Verbrauchstracking pro Druck ist bewusst nicht Teil dieser Ausbaustufe
- Vier kleine Katalog-Erweiterungen: Druckstatus-Toggle + aus Volumen/Material geschätztes Gewicht pro Modell, Sortierung nach "Zuletzt angesehen" + NEU-Badge für kürzlich importierte Modelle, Creators als eigene Sidebar-Filterkategorie (aus dem beim 3MF-Import bereits geparsten Designer-Metadatum), automatische Erkennung exakter Datei-Duplikate beim Import (SHA-256-Inhalts-Hash) mit kurzer Zusammenfassungsmeldung
```

In `README.md`s Abschnitt "Funktionen" (aktuell Zeile 5-17) nach der Filament-Lager-Zeile (Zeile 17, letzte Zeile des Abschnitts) einen neuen Punkt ergänzen:

```markdown
- **Filament-Lager** - eigenständige Verwaltung deiner Filamentspulen (Material, Hersteller, Farbe, Durchmesser, Ursprungs-/Restgewicht, Preis), unabhängig vom Modell-Katalog
- **Katalog-Erweiterungen** — Druckstatus-Toggle + geschätztes Gewicht (aus Volumen × Materialdichte) pro Modell, Sortierung nach "Zuletzt angesehen", NEU-Badge für kürzlich importierte Modelle, Creators-Filter (aus 3MF-Designer-Metadatum), automatische Erkennung exakter Datei-Duplikate beim Import per Inhalts-Hash
```

- [ ] **Step 11: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs src-tauri/src/commands.rs src-tauri/src/cloud/commands.rs src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts src/components/ImportSummaryBanner.tsx src/App.tsx CHANGELOG.md README.md
git commit -m "Exakte Duplikat-Erkennung beim Import (SHA-256-Hash) + Zusammenfassungs-Banner

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V"
```

---

## Nach Abschluss aller Tasks

Nach der finalen Whole-Branch-Review (per subagent-driven-development) und vor dem Push: manueller Live-Test im laufenden `npm run tauri dev` (gemeinsam mit dem Nutzer, da dieses Environment keine zuverlässige synthetische Maussteuerung hat):

- Ein Modell auswählen, "Als gedruckt markieren" klicken, Badge auf der Karte prüfen, Gewicht im Detailpanel prüfen.
- Mehrere Modelle nacheinander auswählen, dann nach "Zuletzt angesehen" sortieren, Reihenfolge prüfen. Ein frisch importiertes Modell auf das NEU-Badge prüfen.
- Eine 3MF-Datei mit einem "Designer"-Metadatum importieren, in der Sidebar unter "Creators" filtern.
- Eine bereits importierte Datei über denselben oder einen kopierten Pfad erneut importieren, die Zusammenfassungs-Banner sehen.
