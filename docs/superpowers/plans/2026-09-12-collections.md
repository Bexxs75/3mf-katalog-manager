# Sammlungen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ein dritter Organisationsmechanismus „Sammlungen" — viele-zu-viele wie Tags, aber mit manuell festlegbarer Reihenfolge pro Sammlung. Erstellung über Mehrfachauswahl oder per Ordner-Import; eigener Navigations-Reiter mit Kartenübersicht aller Sammlungen.

**Architecture:** Zwei neue SQLite-Tabellen (`collections`, `collection_files` mit Positions-Spalte). Neues Rust-Repository-Modul + Tauri-Commands nach exakt demselben Muster wie die bestehende Warteschlange (`queue_position`/`reorder_queue`). Frontend: neue `CollectionsGallery`-Komponente, `activeCollection`-State analog zu `activeTag`, Drag-Umsortieren per Maus-Events (kein natives HTML5-DnD, siehe Global Constraints).

**Tech Stack:** Rust (`rusqlite`), Tauri 2 Commands, React/TypeScript.

## Global Constraints

- `collection_files` hat `ON DELETE CASCADE` auf beide Fremdschlüssel — Löschen einer Sammlung oder einer Datei löscht nur Zuordnungen, nie Modelle. `foreign_keys`-Pragma ist bereits projektweit aktiv (`db::repository::init`), Cascade funktioniert also ohne weiteres Zutun.
- Eine Sammlung enthält jede Datei höchstens einmal (`UNIQUE (collection_id, file_id)`).
- Drag-Umsortieren **muss** dem bestehenden Maus-Event-Muster aus `Sidebar.tsx` (Warteschlange) folgen — **kein** natives HTML5-Drag&Drop (`draggable`/`onDragStart`/etc.), das funktioniert unter Tauri/WebKitGTK nicht zuverlässig, da `dragDropEnabled` native Drag-Sessions auf Fensterebene abfängt.
- Kein Beschreibungs-/Notizfeld pro Sammlung, keine verschachtelten Sammlungen, kein Cover-Bild in der Galerie-Karte — bewusst out of scope laut Spec.
- `import_folder_as_collection` nutzt exakt dieselbe Import-Logik wie das bestehende `import_folder` (inkl. dessen Rekursionsverhalten), unverändert.
- Kein Test-Framework im Frontend — Verifikation über `npx tsc --noEmit` + `npm run build` + manuellen Smoke-Test. Rust-Tests laufen gegen `db::connect_in_memory()` wie das bestehende Testmuster.

---

### Task 1: Datenbank-Schema + Rust-Repository

**Files:**
- Modify: `src-tauri/src/db/schema.sql`
- Modify: `src-tauri/src/db/models.rs`
- Create: `src-tauri/src/db/collections.rs`
- Modify: `src-tauri/src/db/mod.rs`

**Interfaces:**
- Produces: `CollectionRecord { id: i64, name: String, model_count: i64 }`, `pub fn create_collection(conn, name: &str, created_at: &str) -> Result<i64, DbError>`, `pub fn list_collections(conn) -> Result<Vec<CollectionRecord>, DbError>`, `pub fn rename_collection(conn, id: i64, name: &str) -> Result<(), DbError>`, `pub fn delete_collection(conn, id: i64) -> Result<(), DbError>`, `pub fn add_file_to_collection(conn, collection_id: i64, file_id: i64, position: i64) -> Result<(), DbError>` (Kein-Op bei bereits vorhandener Zuordnung, siehe Step 4), `pub fn remove_file_from_collection(conn, collection_id: i64, file_id: i64) -> Result<(), DbError>`, `pub fn max_collection_position(conn, collection_id: i64) -> Result<Option<i64>, DbError>`, `pub fn set_collection_position(conn, collection_id: i64, file_id: i64, position: i64) -> Result<(), DbError>`, `pub fn list_collection_file_ids(conn, collection_id: i64) -> Result<Vec<i64>, DbError>` (sortiert nach `position`), `pub fn get_file_id_by_path(conn, path: &str) -> Result<Option<i64>, DbError>`. Werden von Task 2 (Tauri-Commands) importiert.

- [ ] **Step 1: Schema ergänzen**

In `src-tauri/src/db/schema.sql`, am Ende der Datei anfügen:

```sql
CREATE TABLE IF NOT EXISTS collections (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS collection_files (
    collection_id INTEGER NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    UNIQUE (collection_id, file_id)
);

CREATE INDEX IF NOT EXISTS idx_collection_files_collection_id
    ON collection_files (collection_id);
CREATE INDEX IF NOT EXISTS idx_collection_files_file_id
    ON collection_files (file_id);
```

- [ ] **Step 2: `CollectionRecord`-Modell hinzufügen**

In `src-tauri/src/db/models.rs`, am Ende der Datei anfügen:

```rust
#[derive(Debug, Clone)]
pub struct CollectionRecord {
    pub id: i64,
    pub name: String,
    pub model_count: i64,
}
```

- [ ] **Step 3: Repository-Grundfunktionen (CRUD) in neuer Datei**

Erstelle `src-tauri/src/db/collections.rs`:

```rust
// Repository-Funktionen fuer Sammlungen: viele-zu-viele wie Tags, aber mit
// einer position-Spalte pro Zuordnung fuer eine manuell festlegbare
// Reihenfolge - der eigentliche Mehrwert gegenueber einem Tag.

use rusqlite::{params, Connection, OptionalExtension};

use super::error::DbError;
use super::models::CollectionRecord;

pub fn create_collection(conn: &Connection, name: &str, created_at: &str) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO collections (name, created_at) VALUES (?1, ?2)",
        params![name, created_at],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_collections(conn: &Connection) -> Result<Vec<CollectionRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name,
                (SELECT COUNT(*) FROM collection_files cf WHERE cf.collection_id = c.id)
         FROM collections c
         ORDER BY c.created_at",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CollectionRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                model_count: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn rename_collection(conn: &Connection, id: i64, name: &str) -> Result<(), DbError> {
    conn.execute("UPDATE collections SET name = ?1 WHERE id = ?2", params![name, id])?;
    Ok(())
}

pub fn delete_collection(conn: &Connection, id: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM collections WHERE id = ?1", params![id])?;
    Ok(())
}
```

- [ ] **Step 4: Zuordnungs- und Reihenfolge-Funktionen**

Direkt an `src-tauri/src/db/collections.rs` anfügen:

```rust
pub fn max_collection_position(conn: &Connection, collection_id: i64) -> Result<Option<i64>, DbError> {
    Ok(conn.query_row(
        "SELECT MAX(position) FROM collection_files WHERE collection_id = ?1",
        params![collection_id],
        |row| row.get(0),
    )?)
}

/// Fuegt eine Datei ans Ende der Sammlung an. Ist die Datei bereits
/// zugeordnet, passiert nichts (kein Duplikat, keine Neu-Positionierung) -
/// "INSERT OR IGNORE" nutzt dafuer den UNIQUE-Constraint auf
/// (collection_id, file_id).
pub fn add_file_to_collection(
    conn: &Connection,
    collection_id: i64,
    file_id: i64,
    position: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR IGNORE INTO collection_files (collection_id, file_id, position) VALUES (?1, ?2, ?3)",
        params![collection_id, file_id, position],
    )?;
    Ok(())
}

pub fn remove_file_from_collection(conn: &Connection, collection_id: i64, file_id: i64) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM collection_files WHERE collection_id = ?1 AND file_id = ?2",
        params![collection_id, file_id],
    )?;
    Ok(())
}

pub fn set_collection_position(
    conn: &Connection,
    collection_id: i64,
    file_id: i64,
    position: i64,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE collection_files SET position = ?1 WHERE collection_id = ?2 AND file_id = ?3",
        params![position, collection_id, file_id],
    )?;
    Ok(())
}

pub fn list_collection_file_ids(conn: &Connection, collection_id: i64) -> Result<Vec<i64>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT file_id FROM collection_files WHERE collection_id = ?1 ORDER BY position",
    )?;
    let rows = stmt
        .query_map(params![collection_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_file_id_by_path(conn: &Connection, path: &str) -> Result<Option<i64>, DbError> {
    Ok(conn
        .query_row(
            "SELECT id FROM files WHERE path = ?1 AND deleted_at IS NULL",
            params![path],
            |row| row.get(0),
        )
        .optional()?)
}
```

- [ ] **Step 5: Modul registrieren und Funktionen exportieren**

In `src-tauri/src/db/mod.rs`, nach `mod repository;` einfügen:

```rust
mod collections;
```

Im bestehenden `pub use repository::{...};`-Block bleibt alles unverändert. Direkt darunter, einen neuen Export-Block hinzufügen:

```rust
pub use collections::{
    add_file_to_collection, create_collection, delete_collection, get_file_id_by_path,
    list_collection_file_ids, list_collections, max_collection_position,
    remove_file_from_collection, rename_collection, set_collection_position,
};
```

- [ ] **Step 6: Rust-Tests schreiben**

In `src-tauri/src/db/collections.rs`, am Ende der Datei anfügen:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repository::connect_in_memory;
    use crate::db::{insert_file, models::{FileType, NewFile}};
    use std::collections::BTreeMap;

    fn sample_file(path: &str) -> NewFile {
        NewFile {
            name: path.to_string(),
            path: path.to_string(),
            file_type: FileType::ThreeMf,
            folder_id: None,
            origin: "local".to_string(),
            cloud_id: None,
            sync_status: "local-only".to_string(),
            file_size_bytes: 100,
            dimensions_mm: None,
            volume_cm3: None,
            object_count: None,
            thumbnail_png: None,
            imported_at: "2026-09-12T10:00:00Z".to_string(),
            file_modified_at: None,
            materials: vec![],
            metadata: BTreeMap::new(),
            tags: vec![],
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
        }
    }

    #[test]
    fn add_file_to_collection_appends_at_given_position_and_ignores_duplicates() {
        let conn = connect_in_memory().unwrap();
        let file_id = insert_file(&conn, &sample_file("/tmp/a.3mf")).unwrap();
        let collection_id = create_collection(&conn, "Bauvorhaben X", "2026-09-12T10:00:00Z").unwrap();

        add_file_to_collection(&conn, collection_id, file_id, 0).unwrap();
        add_file_to_collection(&conn, collection_id, file_id, 5).unwrap(); // Duplikat, wird ignoriert

        let ids = list_collection_file_ids(&conn, collection_id).unwrap();
        assert_eq!(ids, vec![file_id]);
    }

    #[test]
    fn set_collection_position_reorders_correctly() {
        let conn = connect_in_memory().unwrap();
        let a = insert_file(&conn, &sample_file("/tmp/a.3mf")).unwrap();
        let b = insert_file(&conn, &sample_file("/tmp/b.3mf")).unwrap();
        let collection_id = create_collection(&conn, "Bauvorhaben X", "2026-09-12T10:00:00Z").unwrap();
        add_file_to_collection(&conn, collection_id, a, 0).unwrap();
        add_file_to_collection(&conn, collection_id, b, 1).unwrap();

        set_collection_position(&conn, collection_id, a, 5).unwrap();
        set_collection_position(&conn, collection_id, b, 0).unwrap();

        let ids = list_collection_file_ids(&conn, collection_id).unwrap();
        assert_eq!(ids, vec![b, a]);
    }

    #[test]
    fn delete_collection_removes_only_the_mapping_not_the_file() {
        let conn = connect_in_memory().unwrap();
        let file_id = insert_file(&conn, &sample_file("/tmp/a.3mf")).unwrap();
        let collection_id = create_collection(&conn, "Bauvorhaben X", "2026-09-12T10:00:00Z").unwrap();
        add_file_to_collection(&conn, collection_id, file_id, 0).unwrap();

        delete_collection(&conn, collection_id).unwrap();

        let ids = list_collection_file_ids(&conn, collection_id).unwrap();
        assert!(ids.is_empty());
        let file_still_exists: i64 = conn
            .query_row("SELECT COUNT(*) FROM files WHERE id = ?1", params![file_id], |row| row.get(0))
            .unwrap();
        assert_eq!(file_still_exists, 1);
    }

    #[test]
    fn list_collections_returns_correct_model_count() {
        let conn = connect_in_memory().unwrap();
        let a = insert_file(&conn, &sample_file("/tmp/a.3mf")).unwrap();
        let b = insert_file(&conn, &sample_file("/tmp/b.3mf")).unwrap();
        let collection_id = create_collection(&conn, "Bauvorhaben X", "2026-09-12T10:00:00Z").unwrap();
        add_file_to_collection(&conn, collection_id, a, 0).unwrap();
        add_file_to_collection(&conn, collection_id, b, 1).unwrap();

        let collections = list_collections(&conn).unwrap();
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].model_count, 2);
    }

    #[test]
    fn get_file_id_by_path_finds_existing_file_and_returns_none_for_unknown_path() {
        let conn = connect_in_memory().unwrap();
        let file_id = insert_file(&conn, &sample_file("/tmp/a.3mf")).unwrap();

        assert_eq!(get_file_id_by_path(&conn, "/tmp/a.3mf").unwrap(), Some(file_id));
        assert_eq!(get_file_id_by_path(&conn, "/tmp/unbekannt.3mf").unwrap(), None);
    }
}
```

- [ ] **Step 7: Tests und Build verifizieren**

Run: `cd src-tauri && cargo test --lib collections`
Expected: alle 5 Tests bestehen.

Run: `cd src-tauri && cargo build --release`
Expected: baut ohne Fehler (Warnungen zu bislang ungenutzten Funktionen wie `rename_collection`/`max_collection_position` sind an dieser Stelle normal - Task 2 verdrahtet sie).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/db/schema.sql src-tauri/src/db/models.rs src-tauri/src/db/collections.rs src-tauri/src/db/mod.rs
git commit -m "feat: Datenbank-Schema und Repository fuer Sammlungen"
```

---

### Task 2: Tauri-Commands für Sammlungen

**Files:**
- Modify: `src-tauri/src/commands.rs` (neue DTOs + Commands, direkt nach dem bestehenden `reorder_queue`-Block einfügen)
- Modify: `src-tauri/src/lib.rs` (Commands registrieren)

**Interfaces:**
- Consumes: alle Repository-Funktionen aus Task 1 (`db::create_collection`, `db::list_collections`, etc.), `db::CollectionRecord`, bestehende `import_many`/`collect_supported_files`/`lock_db`/`CmdResult`/`ImportResultDto` aus `commands.rs`.
- Produces: Tauri-Commands `list_collections`, `create_collection`, `rename_collection`, `delete_collection`, `add_files_to_collection`, `remove_file_from_collection`, `reorder_collection`, `list_collection_files`, `import_folder_as_collection`. `CollectionDto { id: String, name: String, modelCount: i64 }` (camelCase via serde). Werden von Task 3 (Frontend) über `invoke(...)` aufgerufen.

- [ ] **Step 1: `CollectionDto` und einfache CRUD-Commands**

In `src-tauri/src/commands.rs`, direkt nach dem bestehenden Block

```rust
#[tauri::command]
pub fn reorder_queue(state: State<AppState>, updates: Vec<QueuePositionUpdate>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    for update in updates {
        let id: i64 = update.file_id.parse().map_err(|_| "invalid file id".to_string())?;
        db::set_queue_position(&conn, id, Some(update.position)).map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

einfügen:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDto {
    pub id: String,
    pub name: String,
    pub model_count: i64,
}

fn to_collection_dto(record: db::models::CollectionRecord) -> CollectionDto {
    CollectionDto {
        id: record.id.to_string(),
        name: record.name,
        model_count: record.model_count,
    }
}

#[tauri::command]
pub fn list_collections(state: State<AppState>) -> CmdResult<Vec<CollectionDto>> {
    let conn = lock_db(&state)?;
    let collections = db::list_collections(&conn).map_err(|e| e.to_string())?;
    Ok(collections.into_iter().map(to_collection_dto).collect())
}

#[tauri::command]
pub fn create_collection(state: State<AppState>, name: String) -> CmdResult<CollectionDto> {
    let conn = lock_db(&state)?;
    let created_at = chrono::Utc::now().to_rfc3339();
    let id = db::create_collection(&conn, &name, &created_at).map_err(|e| e.to_string())?;
    Ok(CollectionDto { id: id.to_string(), name, model_count: 0 })
}

#[tauri::command]
pub fn rename_collection(state: State<AppState>, collection_id: String, name: String) -> CmdResult<()> {
    let id: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    db::rename_collection(&conn, id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_collection(state: State<AppState>, collection_id: String) -> CmdResult<()> {
    let id: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    db::delete_collection(&conn, id).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Zuordnungs- und Reihenfolge-Commands**

Direkt danach einfügen:

```rust
#[tauri::command]
pub fn add_files_to_collection(state: State<AppState>, collection_id: String, file_ids: Vec<String>) -> CmdResult<()> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    let mut next = db::max_collection_position(&conn, cid).map_err(|e| e.to_string())?.unwrap_or(-1) + 1;
    for file_id in file_ids {
        let fid: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
        db::add_file_to_collection(&conn, cid, fid, next).map_err(|e| e.to_string())?;
        next += 1;
    }
    Ok(())
}

#[tauri::command]
pub fn remove_file_from_collection(state: State<AppState>, collection_id: String, file_id: String) -> CmdResult<()> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let fid: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::remove_file_from_collection(&conn, cid, fid).map_err(|e| e.to_string())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionPositionUpdate {
    pub file_id: String,
    pub position: i64,
}

#[tauri::command]
pub fn reorder_collection(state: State<AppState>, collection_id: String, updates: Vec<CollectionPositionUpdate>) -> CmdResult<()> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    for update in updates {
        let fid: i64 = update.file_id.parse().map_err(|_| "invalid file id".to_string())?;
        db::set_collection_position(&conn, cid, fid, update.position).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn list_collection_files(state: State<AppState>, collection_id: String) -> CmdResult<Vec<ModelFileDto>> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    let ids = db::list_collection_file_ids(&conn, cid).map_err(|e| e.to_string())?;
    let mut dtos = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(file) = db::get_file(&conn, id).map_err(|e| e.to_string())? {
            dtos.push(to_dto(file));
        }
    }
    Ok(dtos)
}
```

Hinweis: `db::get_file(conn, id) -> Result<Option<FileRecord>, DbError>` ist bereits vorhanden und exportiert (siehe `db::mod.rs`) - keine neue Repository-Funktion nötig, `list_collection_files` nutzt sie nur, um pro ID den vollen Datensatz zu laden und mit dem bestehenden `to_dto` zu kodieren.

- [ ] **Step 3: `import_folder_as_collection`**

Direkt nach dem bestehenden `import_folder`-Command (endet mit `import_many(&state, vec![path])\n}`) einfügen:

```rust
#[tauri::command]
pub async fn import_folder_as_collection(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ImportResultDto> {
    let picked = app.dialog().file().blocking_pick_folder();

    let Some(picked) = picked else {
        return Ok(ImportResultDto { imported: Vec::new(), duplicate_count: 0 });
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let folder_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Sammlung".to_string());

    let mut candidates = Vec::new();
    collect_supported_files(&path, &mut candidates);

    let result = import_many(&state, vec![path])?;

    let conn = lock_db(&state)?;
    let created_at = chrono::Utc::now().to_rfc3339();
    let collection_id = db::create_collection(&conn, &folder_name, &created_at).map_err(|e| e.to_string())?;

    let mut position = 0i64;
    for candidate in &candidates {
        let path_str = candidate.to_string_lossy().to_string();
        if let Some(file_id) = db::get_file_id_by_path(&conn, &path_str).map_err(|e| e.to_string())? {
            db::add_file_to_collection(&conn, collection_id, file_id, position).map_err(|e| e.to_string())?;
            position += 1;
        }
    }

    Ok(result)
}
```

(`collect_supported_files` und `import_many` sind bereits in `commands.rs` vorhanden - `import_many` importiert wie gehabt, `collect_supported_files` liefert die vollständige Liste aller Datei-Pfade im Ordner, mit der anschließend JEDE Datei - egal ob gerade neu importiert oder schon vorher katalogisiert - der neuen Sammlung zugeordnet wird.)

- [ ] **Step 4: Commands in `lib.rs` registrieren**

In `src-tauri/src/lib.rs`, im `tauri::generate_handler![...]`-Makro, nach der Zeile `commands::empty_trash,` (letzter bestehender Eintrag) einfügen:

```rust
            commands::list_collections,
            commands::create_collection,
            commands::rename_collection,
            commands::delete_collection,
            commands::add_files_to_collection,
            commands::remove_file_from_collection,
            commands::reorder_collection,
            commands::list_collection_files,
            commands::import_folder_as_collection,
```

- [ ] **Step 5: Build und Tests verifizieren**

Run: `cd src-tauri && cargo build --release`
Expected: baut ohne Fehler.

Run: `cd src-tauri && cargo test --lib`
Expected: alle bestehenden Tests plus die 5 aus Task 1 bestehen weiterhin (keine Regression).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: Tauri-Commands fuer Sammlungen (CRUD, Zuordnung, Reihenfolge, Ordner-Import)"
```

---

### Task 3: Frontend-Datenmodell + i18n

**Files:**
- Modify: `src/types/index.ts` (neuer `Collection`-Typ, am Ende der Datei)
- Modify: `src/App.tsx` (neuer State + `refreshCollections`, in den bestehenden Mount-`useEffect` integriert)
- Modify: `src/i18n/types.ts` + alle 4 Sprachdateien

**Interfaces:**
- Consumes: Tauri-Commands aus Task 2 (`list_collections` liefert `{id: string; name: string; modelCount: number}[]`).
- Produces: `Collection { id: string; name: string; modelCount: number }` (Typ), `App.tsx`-State `collections: Collection[]`, `activeCollection: string | null`, `collectionsGalleryOpen: boolean`, Funktion `refreshCollections: () => void`. Werden von Task 4/5/6 konsumiert.

- [ ] **Step 1: `Collection`-Typ hinzufügen**

In `src/types/index.ts`, am Ende der Datei anfügen:

```ts
export interface Collection {
  id: string;
  name: string;
  modelCount: number;
}
```

- [ ] **Step 2: State und `refreshCollections` in `App.tsx`**

Ergänze den Import in `src/App.tsx` (in der bestehenden `import type { ... } from './types';`-Zeile), `Collection` zur Liste der importierten Typen hinzufügen:

```ts
import type { ModelFile, Folder, TagCount, CreatorCount, ViewMode, SortKey, SavedFilter, CatalogIssues, Collection } from './types';
```

Nach der bestehenden Zeile

```ts
  const refreshTrash = () => invoke<ModelFile[]>('list_trash').then(setTrashModels);
```

einfügen:

```ts
  const refreshCollections = () => invoke<Collection[]>('list_collections').then(setCollections);
```

Suche die State-Deklarationen (Bereich mit `const [activeFolderId, setActiveFolderId] = useState('all');` etc.) und ergänze dort. `collections` wird zwar in dieser Task bereits von `refreshCollections()` geschrieben, aber erst ab Task 4/5 tatsächlich gelesen - das Projekt hat `noUnusedLocals`/`noUnusedParameters` aktiv (`tsconfig.json`), daher hier bewusst mit `@ts-expect-error` markiert, wie an anderer Stelle im Projekt bereits etabliert (nur ein vorübergehendes Placeholder-Problem beim mehrstufigen Aufbau, kein Code-Smell):

```ts
  // @ts-expect-error - wird ab Task 4 (CollectionsGallery + Breadcrumb) gelesen
  const [collections, setCollections] = useState<Collection[]>([]);
  // @ts-expect-error - wird ab Task 4 (Breadcrumb) gelesen und gesetzt
  const [activeCollection, setActiveCollection] = useState<string | null>(null);
  // @ts-expect-error - wird ab Task 4 (Breadcrumb) gelesen und gesetzt
  const [collectionsGalleryOpen, setCollectionsGalleryOpen] = useState(false);
```

Im bestehenden Mount-`useEffect` (der bereits `refreshFolders(); refreshTags(); refreshCreators(); refreshSavedFilters(); refreshTrash();` aufruft, plus den in einer vorherigen Task ergänzten `scan_installed_slicers`-Aufruf), nach `refreshTrash();` einfügen:

```ts
    refreshCollections();
```

- [ ] **Step 3: i18n-Schlüssel hinzufügen**

In `src/i18n/types.ts`, am Ende des `Translations`-Interfaces (nach den zuletzt ergänzten Schlüsseln aus dem Dreh-Steuerelemente-Feature, z. B. nach `pauseRotationAria: string;`, falls dieses Feature vorher umgesetzt wurde - ansonsten ganz am Ende des Interfaces), einfügen:

```ts
  collectionsTab: string;
  modelCountLabel: string;
  addToCollectionLabel: string;
  newCollectionPlaceholder: string;
  removeFromCollectionLabel: string;
  deleteCollectionConfirmQuestion: string;
  renameCollectionAria: string;
  noCollectionsEmptyState: string;
  backToCollectionsLabel: string;
  createCollectionLabel: string;
  importFolderAsCollectionOption: string;
```

In `src/i18n/de.ts`, an derselben relativen Stelle (Ende der Datei bzw. nach den Dreh-Steuerelemente-Schlüsseln), einfügen:

```ts
  collectionsTab: 'Sammlungen',
  modelCountLabel: '{count} Modelle',
  addToCollectionLabel: 'Zu Sammlung hinzufügen',
  newCollectionPlaceholder: 'Name der Sammlung',
  removeFromCollectionLabel: 'Aus Sammlung entfernen',
  deleteCollectionConfirmQuestion: 'Sammlung wirklich löschen? Die Modelle selbst bleiben erhalten.',
  renameCollectionAria: 'Sammlung umbenennen',
  noCollectionsEmptyState: 'Noch keine Sammlungen angelegt.',
  backToCollectionsLabel: '← Sammlungen',
  createCollectionLabel: '+ Neue Sammlung',
  importFolderAsCollectionOption: 'Ordner als Sammlung importieren',
```

In `src/i18n/en.ts`, an derselben Stelle, einfügen:

```ts
  collectionsTab: 'Collections',
  modelCountLabel: '{count} models',
  addToCollectionLabel: 'Add to collection',
  newCollectionPlaceholder: 'Collection name',
  removeFromCollectionLabel: 'Remove from collection',
  deleteCollectionConfirmQuestion: 'Delete this collection? The models themselves stay in your catalog.',
  renameCollectionAria: 'Rename collection',
  noCollectionsEmptyState: 'No collections yet.',
  backToCollectionsLabel: '← Collections',
  createCollectionLabel: '+ New collection',
  importFolderAsCollectionOption: 'Import folder as collection',
```

In `src/i18n/es.ts`, an derselben Stelle, einfügen:

```ts
  collectionsTab: 'Colecciones',
  modelCountLabel: '{count} modelos',
  addToCollectionLabel: 'Añadir a colección',
  newCollectionPlaceholder: 'Nombre de la colección',
  removeFromCollectionLabel: 'Quitar de la colección',
  deleteCollectionConfirmQuestion: '¿Eliminar esta colección? Los modelos permanecen en tu catálogo.',
  renameCollectionAria: 'Renombrar colección',
  noCollectionsEmptyState: 'Aún no hay colecciones.',
  backToCollectionsLabel: '← Colecciones',
  createCollectionLabel: '+ Nueva colección',
  importFolderAsCollectionOption: 'Importar carpeta como colección',
```

In `src/i18n/fr.ts`, an derselben Stelle, einfügen:

```ts
  collectionsTab: 'Collections',
  modelCountLabel: '{count} modèles',
  addToCollectionLabel: 'Ajouter à une collection',
  newCollectionPlaceholder: 'Nom de la collection',
  removeFromCollectionLabel: 'Retirer de la collection',
  deleteCollectionConfirmQuestion: 'Supprimer cette collection ? Les modèles restent dans votre catalogue.',
  renameCollectionAria: 'Renommer la collection',
  noCollectionsEmptyState: 'Aucune collection pour le moment.',
  backToCollectionsLabel: '← Collections',
  createCollectionLabel: '+ Nouvelle collection',
  importFolderAsCollectionOption: 'Importer un dossier comme collection',
```

- [ ] **Step 4: TypeScript verifizieren**

Run: `npx tsc --noEmit`
Expected: 0 Fehler. Die drei `@ts-expect-error`-Kommentare aus Step 2 unterdrücken gezielt die `noUnusedLocals`-Fehler für `collections`, `activeCollection`/`setActiveCollection`, `collectionsGalleryOpen`/`setCollectionsGalleryOpen` (alle erst ab Task 4/5 tatsächlich gelesen/geschrieben). Wichtig: diese drei `@ts-expect-error`-Kommentare müssen in Task 4 bzw. Task 5 wieder entfernt werden, sobald der jeweilige Wert dort tatsächlich verwendet wird - ein verbleibendes, nicht mehr benötigtes `@ts-expect-error` erzeugt selbst einen TS-Fehler ("Unused '@ts-expect-error' directive").

- [ ] **Step 5: Commit**

```bash
git add src/types/index.ts src/App.tsx src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "feat: Collection-Typ, App.tsx State-Grundlage und i18n fuer Sammlungen"
```

---

### Task 4: CollectionsGallery-Komponente + Navigations-Reiter

**Files:**
- Create: `src/components/CollectionsGallery.tsx`
- Modify: `src/App.tsx` (Breadcrumb-Bereich, Rendering-Verzweigung)

**Interfaces:**
- Consumes: `Collection`-Typ (Task 3), `collections`/`activeCollection`/`collectionsGalleryOpen`-State und `refreshCollections` (Task 3).
- Produces: Komponente `CollectionsGallery` mit Props `{ collections: Collection[]; onSelect: (id: string) => void; onCreate: (name: string) => void; onRename: (id: string, name: string) => void; onDelete: (id: string) => void; }`. Wird auch von Task 5 (Klick-Navigation in die Detailansicht) genutzt.

- [ ] **Step 1: `CollectionsGallery.tsx` erstellen**

Erstelle `src/components/CollectionsGallery.tsx`:

```tsx
import { useState } from 'react';
import type { Collection } from '../types';
import { useT } from '../i18n/LanguageContext';

interface Props {
  collections: Collection[];
  onSelect: (id: string) => void;
  onCreate: (name: string) => void;
  onRename: (id: string, name: string) => void;
  onDelete: (id: string) => void;
}

export function CollectionsGallery({ collections, onSelect, onCreate, onRename, onDelete }: Props) {
  const t = useT();
  const [creating, setCreating] = useState(false);
  const [nameDraft, setNameDraft] = useState('');
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState('');

  const submitCreate = () => {
    const value = nameDraft.trim();
    if (value) onCreate(value);
    setNameDraft('');
    setCreating(false);
  };

  const submitRename = (id: string) => {
    const value = renameDraft.trim();
    if (value) onRename(id, value);
    setRenamingId(null);
  };

  return (
    <div className="flex-1 overflow-y-auto p-4">
      {collections.length === 0 && !creating ? (
        <p className="font-mono-ui text-[12.5px] text-[var(--ink-3)]">{t('noCollectionsEmptyState')}</p>
      ) : null}
      <div className="grid gap-3.5" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))' }}>
        {collections.map((c) => (
          <div
            key={c.id}
            className="rounded-[var(--radius-card)] overflow-hidden cursor-pointer bg-[var(--panel)] shadow-[var(--shadow)] border-2 border-transparent hover:border-[var(--line-strong)] p-4 flex flex-col gap-2"
            onClick={() => onSelect(c.id)}
          >
            {renamingId === c.id ? (
              <input
                autoFocus
                value={renameDraft}
                onClick={(e) => e.stopPropagation()}
                onChange={(e) => setRenameDraft(e.target.value)}
                onBlur={() => submitRename(c.id)}
                onKeyDown={(e) => e.key === 'Enter' && submitRename(c.id)}
                className="bg-[var(--panel-2)] border border-[var(--line-strong)] rounded px-2 py-1 text-[13px]"
              />
            ) : (
              <div className="flex items-center justify-between gap-2">
                <span className="text-[14px] font-semibold text-[var(--ink)] truncate">{c.name}</span>
                <div className="flex items-center gap-1 flex-none">
                  <span
                    onClick={(e) => {
                      e.stopPropagation();
                      setRenamingId(c.id);
                      setRenameDraft(c.name);
                    }}
                    aria-label={t('renameCollectionAria')}
                    className="w-5 h-5 grid place-items-center rounded-full cursor-pointer text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                  >
                    ✎
                  </span>
                  <span
                    onClick={(e) => {
                      e.stopPropagation();
                      if (window.confirm(t('deleteCollectionConfirmQuestion'))) onDelete(c.id);
                    }}
                    aria-label={t('deleteCollectionConfirmQuestion')}
                    className="w-5 h-5 grid place-items-center rounded-full cursor-pointer text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                  >
                    ✕
                  </span>
                </div>
              </div>
            )}
            <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">
              {t('modelCountLabel').replace('{count}', String(c.modelCount))}
            </span>
          </div>
        ))}

        {creating ? (
          <div className="rounded-[var(--radius-card)] border-2 border-dashed border-[var(--line-strong)] p-4 flex flex-col gap-2">
            <input
              autoFocus
              value={nameDraft}
              onChange={(e) => setNameDraft(e.target.value)}
              onBlur={submitCreate}
              onKeyDown={(e) => {
                if (e.key === 'Enter') submitCreate();
                if (e.key === 'Escape') {
                  setCreating(false);
                  setNameDraft('');
                }
              }}
              placeholder={t('newCollectionPlaceholder')}
              className="bg-[var(--panel-2)] border border-[var(--line-strong)] rounded px-2 py-1 text-[13px]"
            />
          </div>
        ) : (
          <button
            onClick={() => setCreating(true)}
            className="rounded-[var(--radius-card)] border-2 border-dashed border-[var(--line-strong)] p-4 flex items-center justify-center text-[13px] font-semibold text-[var(--ink-2)] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] min-h-[76px]"
          >
            {t('createCollectionLabel')}
          </button>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Reiter und Rendering-Verzweigung in `App.tsx`**

Ergänze den Import in `src/App.tsx` (nach `import { CatalogCleanupDialog } from './components/CatalogCleanupDialog';`):

```ts
import { CollectionsGallery } from './components/CollectionsGallery';
```

Im Breadcrumb-Bereich (aktuell):

```tsx
            <div className="flex-none h-[38px] flex items-center gap-2.5 px-4 border-b border-[var(--line)] bg-[var(--bg)]">
              <span className="font-mono-ui text-[11px] text-[var(--ink-2)]">
                {folders.find((f) => f.id === activeFolderId)?.name}
              </span>
```

ersetzen durch:

```tsx
            <div className="flex-none h-[38px] flex items-center gap-2.5 px-4 border-b border-[var(--line)] bg-[var(--bg)]">
              <span
                onClick={() => {
                  setCollectionsGalleryOpen(false);
                  setActiveCollection(null);
                }}
                className={`font-mono-ui text-[11px] cursor-pointer ${
                  !collectionsGalleryOpen && !activeCollection ? 'text-[var(--ink)]' : 'text-[var(--ink-2)] hover:text-[var(--ink)]'
                }`}
              >
                {folders.find((f) => f.id === activeFolderId)?.name}
              </span>
              <span
                onClick={() => {
                  setCollectionsGalleryOpen(true);
                  setActiveCollection(null);
                }}
                className={`font-mono-ui text-[11px] cursor-pointer ${
                  collectionsGalleryOpen || activeCollection ? 'text-[var(--accent)] font-semibold' : 'text-[var(--ink-2)] hover:text-[var(--ink)]'
                }`}
              >
                {t('collectionsTab')}
              </span>
```

Direkt darunter bleibt der bestehende `activeTag`/`activeCreator`-Block unverändert.

Nach der bestehenden Zeile `{!detailModel && selectedForBulk.size > 0 && (...)}`-Block (Bulk-Aktionsleiste) und vor `{detailModel ? (` einfügen (die bestehende `{detailModel ? (...) : (...)}`-Verzweigung bleibt als äußerste Fallback-Ebene erhalten, die neue Bedingung wird davor geprüft):

Ersetze

```tsx
            {detailModel ? (
```

durch

```tsx
            {collectionsGalleryOpen ? (
              <CollectionsGallery
                collections={collections}
                onSelect={(id) => {
                  setActiveCollection(id);
                  setCollectionsGalleryOpen(false);
                }}
                onCreate={(name) => invoke<Collection>('create_collection', { name }).then(() => refreshCollections())}
                onRename={(id, name) => invoke('rename_collection', { collectionId: id, name }).then(() => refreshCollections())}
                onDelete={(id) => {
                  invoke('delete_collection', { collectionId: id }).then(() => {
                    refreshCollections();
                    if (activeCollection === id) setActiveCollection(null);
                  });
                }}
              />
            ) : detailModel ? (
```

(Die schließende Klammer der ursprünglichen `{detailModel ? (...) : (...)}`-Struktur bleibt unverändert - es wird nur eine zusätzliche Bedingung `collectionsGalleryOpen ? (...) : detailModel ? (...) : (...)` vor die bestehende gesetzt.)

- [ ] **Step 3: Obsolete `@ts-expect-error`-Marker aus Task 3 entfernen**

Durch Step 2 werden `collections`, `activeCollection`/`setActiveCollection` und `collectionsGalleryOpen`/`setCollectionsGalleryOpen` jetzt tatsächlich gelesen und gesetzt (Breadcrumb-Reiter, `CollectionsGallery`-Props). Entferne alle drei `// @ts-expect-error`-Kommentarzeilen, die in Task 3 Step 2 direkt über diesen drei `useState`-Deklarationen eingefügt wurden - sie sind jetzt nicht mehr nötig und würden selbst einen TS-Fehler erzeugen ("Unused '@ts-expect-error' directive").

- [ ] **Step 4: TypeScript und Build verifizieren**

Run: `npx tsc --noEmit`
Expected: 0 Fehler.

Run: `npm run build`
Expected: baut ohne Fehler.

- [ ] **Step 5: Commit**

```bash
git add src/components/CollectionsGallery.tsx src/App.tsx
git commit -m "feat: CollectionsGallery-Komponente und Sammlungen-Reiter in der Kopfleiste"
```

---

### Task 5: Sammlungs-Detailansicht + Mehrfachauswahl-Integration

**Files:**
- Modify: `src/App.tsx` (Collection-Modelle laden, Bulk-Action-Button, `ModelGrid`-Aufruf für Sammlungsansicht, Sortieren-Dropdown ausblenden)
- Modify: `src/components/Header.tsx` (Sortieren-Dropdown optional ausblenden)

**Interfaces:**
- Consumes: `activeCollection`-State (Task 3), `CollectionsGallery` (Task 4), bestehende `ModelGrid`, bestehende Bulk-Auswahl-Infrastruktur (`selectedForBulk`, `toggleBulkSelect` etc.).
- Produces: `App.tsx`-State `collectionModels: ModelFile[]`, Funktion `refreshCollectionModels()`, Bulk-Handler `bulkAddToCollection(collectionId: string)`. Werden von Task 6 (Drag-Reorder, "Aus Sammlung entfernen") weiterverwendet.

- [ ] **Step 1: Sammlungs-Modelle laden**

In `src/App.tsx`, nach der Deklaration von `const [collectionsGalleryOpen, setCollectionsGalleryOpen] = useState(false);` (aus Task 3) einfügen:

```ts
  const [collectionModels, setCollectionModels] = useState<ModelFile[]>([]);

  const refreshCollectionModels = (collectionId: string) =>
    invoke<ModelFile[]>('list_collection_files', { collectionId }).then(setCollectionModels);
```

Ergänze einen `useEffect`, der bei jedem Wechsel von `activeCollection` neu lädt - füge ihn direkt nach dem bestehenden `useEffect` ein, der `pendingSnapshotIds`/`displayPreference` betrifft (oder, falls jenes Feature noch nicht umgesetzt ist, direkt nach dem Haupt-Mount-`useEffect`):

```ts
  useEffect(() => {
    if (activeCollection) {
      refreshCollectionModels(activeCollection);
    } else {
      setCollectionModels([]);
    }
  }, [activeCollection]);
```

- [ ] **Step 2: Bulk-Handler „Zu Sammlung hinzufügen"**

Direkt nach der bestehenden Funktion `bulkSetPrintStatus` einfügen:

```ts
  const bulkAddToCollection = (collectionId: string) => {
    invoke('add_files_to_collection', { collectionId, fileIds: Array.from(selectedForBulk) }).then(() => {
      refreshCollections();
      if (activeCollection === collectionId) refreshCollectionModels(collectionId);
      clearBulkSelection();
    });
  };
```

- [ ] **Step 3: Bulk-Aktionsleiste um „Zu Sammlung hinzufügen"-Auswahl erweitern**

In der bestehenden Bulk-Aktionsleiste (`src/App.tsx`), direkt nach dem Button

```tsx
                    <button onClick={bulkAddToQueue} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
                      {t('addToQueue')}
                    </button>
```

einfügen (neuer State `addToCollectionMenuOpen` wird lokal in dieser Task ergänzt - direkt bei den anderen Bulk-bezogenen `useState`-Deklarationen, z. B. neben `confirmBulkDelete`):

```ts
  const [addToCollectionMenuOpen, setAddToCollectionMenuOpen] = useState(false);
```

Und die JSX-Ergänzung in der Aktionsleiste:

```tsx
                    <div className="relative">
                      <button
                        onClick={() => setAddToCollectionMenuOpen((prev) => !prev)}
                        className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]"
                      >
                        {t('addToCollectionLabel')}
                      </button>
                      {addToCollectionMenuOpen && (
                        <div className="absolute top-9 left-0 w-[220px] py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
                          {collections.map((c) => (
                            <button
                              key={c.id}
                              onClick={() => {
                                bulkAddToCollection(c.id);
                                setAddToCollectionMenuOpen(false);
                              }}
                              className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
                            >
                              {c.name}
                            </button>
                          ))}
                          {collections.length === 0 && (
                            <div className="px-3 py-1.5 font-mono-ui text-[11px] text-[var(--ink-3)]">
                              {t('noCollectionsEmptyState')}
                            </div>
                          )}
                        </div>
                      )}
                    </div>
```

(Neue Sammlungen anlegen geschieht über die `CollectionsGallery` — dieses Dropdown zeigt nur bestehende Sammlungen zur Auswahl, das deckt den in der Spec beschriebenen Hauptweg ab und hält die Aktionsleiste einfach.)

- [ ] **Step 4: Sammlungsansicht rendern, Sortieren-Dropdown ausblenden**

In `src/App.tsx`, im bestehenden Block, der `ModelGrid`/`ModelList` für die normale Katalogansicht rendert (aktuell innerhalb von `{detailModel ? (...) : (...)}`, im `else`-Zweig), wird bei aktiver Sammlung `collectionModels` statt `filtered` als `models`-Prop übergeben, und der Reorder-Modus aktiviert. Suche die Stelle

```tsx
                {view === 'grid' ? (
                  <ModelGrid
                    models={filtered}
                    selectedId={selectedId}
                    onSelect={selectModel}
                    onOpenDetail={setDetailModelId}
                    onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                    onToggleFavorite={toggleFavorite}
                    selectedForBulk={selectedForBulk}
                    onToggleBulkSelect={toggleBulkSelect}
                    displayPreference={displayPreference}
                  />
                ) : (
```

und ersetze `models={filtered}` durch:

```tsx
                    models={activeCollection ? collectionModels : filtered}
```

Im Header-Aufruf (`<Header ... />`), ergänze eine neue Prop `hideSortControl={activeCollection !== null}` nach der bestehenden Zeile `onSortChange={setSort}`:

```tsx
        onSortChange={setSort}
        hideSortControl={activeCollection !== null}
```

In `src/components/Header.tsx`, im `Props`-Interface, nach `onSortChange: (s: SortKey) => void;` einfügen:

```ts
  hideSortControl?: boolean;
```

In der Funktionssignatur, `hideSortControl` zur Destrukturierung hinzufügen. Das bestehende Sortieren-JSX beginnt mit:

```tsx
      <div className="flex items-center gap-1.5">
        <span className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.1em] uppercase text-[var(--ink-3)]">
          {t('sortLabel')}
        </span>
        <div className="relative">
```

Ersetze die öffnende Zeile

```tsx
      <div className="flex items-center gap-1.5">
```

(genau diese eine Stelle, direkt vor `{t('sortLabel')}`) durch:

```tsx
      <div className="flex items-center gap-1.5" style={hideSortControl ? { display: 'none' } : undefined}>
```

(Das gesamte Sortier-UI inkl. `sortMenuOpen`-State bleibt strukturell unverändert - nur `display: none`, wenn `hideSortControl` aktiv ist. Ein bedingtes Nicht-Rendern des ganzen Blocks würde `sortMenuOpen` beim Wiedereinblenden zurücksetzen, was hier keinen Unterschied macht, `style`-basiertes Ausblenden ist aber die kleinere, risikoärmere Änderung.)

- [ ] **Step 5: TypeScript und Build verifizieren**

Run: `npx tsc --noEmit`
Expected: 0 Fehler.

Run: `npm run build`
Expected: baut ohne Fehler.

- [ ] **Step 6: Commit**

```bash
git add src/App.tsx src/components/Header.tsx
git commit -m "feat: Sammlungs-Detailansicht, Zu-Sammlung-hinzufuegen-Aktion, Sortieren-Dropdown ausgeblendet"
```

---

### Task 6: Drag-Umsortieren, Aus-Sammlung-entfernen, Ordner-Import-Option

**Files:**
- Modify: `src/components/ModelGrid.tsx` (neue optionale Props `reorderable`/`onReorder`, Maus-Event-Handler an den Karten)
- Modify: `src/App.tsx` (Reorder-Handler, „Aus Sammlung entfernen"-Bulk-Aktion, `ModelGrid`-Aufruf um neue Props ergänzt)
- Modify: `src/components/Header.tsx` (Import-Menü um „Ordner als Sammlung importieren" ergänzt)

**Interfaces:**
- Consumes: `reorder_collection`/`remove_file_from_collection`-Commands (Task 2), `activeCollection`/`collectionModels` (Task 5).
- Produces: keine neuen Exporte für weitere Tasks (letzte Task des Plans).

- [ ] **Step 1: Maus-basiertes Drag-Umsortieren in `ModelGrid.tsx`**

`ModelGrid.tsx` importiert aktuell keine React-Hooks (keine `useState`/`useEffect`-Nutzung bisher). Füge ganz am Dateianfang, vor der bestehenden ersten Zeile `import type { ModelFile } from '../types';`, eine neue Zeile ein:

```ts
import { useEffect, useState } from 'react';
```

Im `Props`-Interface, nach `displayPreference: DisplayPreference;` einfügen:

```ts
  reorderable?: boolean;
  onReorder?: (orderedIds: string[]) => void;
```

In der Funktionssignatur von `ModelGrid`, `reorderable` und `onReorder` zur Destrukturierung hinzufügen.

Direkt nach der bestehenden Zeile `const { language } = useLanguage();` (innerhalb der Komponente, vor `renderCompactCard`), einfügen - das exakt gleiche Maus-Event-Muster wie in `Sidebar.tsx` für die Warteschlange, hier auf die per `density` gerenderte `models`-Liste angewendet:

```ts
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);

  // Reihenfolge-Aenderung per Maus-Events statt nativem HTML5-Drag&Drop -
  // identisches Muster wie die Warteschlange in Sidebar.tsx. Natives
  // draggable/onDragStart/onDragOver/onDrop funktioniert unter Tauri/
  // WebKitGTK nicht zuverlaessig, da dragDropEnabled native Drag-Sessions
  // auf Fensterebene abfaengt.
  useEffect(() => {
    if (!reorderable || dragIndex === null) return;
    const handleMouseUp = () => {
      const from = dragIndex;
      const to = overIndex;
      setDragIndex(null);
      setOverIndex(null);
      if (from === null || to === null || to === from) return;
      const ids = models.map((m) => m.id);
      const [moved] = ids.splice(from, 1);
      ids.splice(to, 0, moved);
      onReorder?.(ids);
    };
    document.addEventListener('mouseup', handleMouseUp);
    return () => document.removeEventListener('mouseup', handleMouseUp);
  }, [reorderable, dragIndex, overIndex, models, onReorder]);
```

In `renderCompactCard(m: ModelFile)`, füge dem äußeren `<div>` (das bereits `onClick`/`onDoubleClick`/`onContextMenu`/`className` trägt) zwei weitere Event-Handler hinzu - suche die Zeile

```tsx
        onContextMenu={(e) => {
          e.preventDefault();
          onSelect(m.id);
          onContextMenu(m.id, e.clientX, e.clientY);
        }}
```

und ergänze direkt danach:

```tsx
        onMouseDown={() => reorderable && setDragIndex(models.findIndex((x) => x.id === m.id))}
        onMouseEnter={() => reorderable && dragIndex !== null && setOverIndex(models.findIndex((x) => x.id === m.id))}
```

Für die Comfort-Karten-Variante (inline im `.map()`-Ternary, gleiches `m`), suche dort ebenfalls den `onContextMenu`-Block und ergänze dieselben zwei Zeilen an derselben relativen Position.

- [ ] **Step 2: Reorder-Handler und „Aus Sammlung entfernen" in `App.tsx`**

Nach der bestehenden Funktion `bulkAddToCollection` (aus Task 5) einfügen:

```ts
  const reorderCollection = (orderedIds: string[]) => {
    if (!activeCollection) return;
    const updates = orderedIds.map((fileId, position) => ({ fileId, position }));
    setCollectionModels((prev) => {
      const byId = new Map(prev.map((m) => [m.id, m]));
      return orderedIds.map((id) => byId.get(id)).filter((m): m is ModelFile => m !== undefined);
    });
    invoke('reorder_collection', { collectionId: activeCollection, updates }).catch((e) => {
      console.error('[collections] Umsortieren fehlgeschlagen:', e);
    });
  };

  const bulkRemoveFromCollection = () => {
    if (!activeCollection) return;
    const ids = Array.from(selectedForBulk);
    Promise.all(
      ids.map((fileId) => invoke('remove_file_from_collection', { collectionId: activeCollection, fileId })),
    ).then(() => {
      refreshCollections();
      refreshCollectionModels(activeCollection);
      clearBulkSelection();
    });
  };
```

- [ ] **Step 3: `ModelGrid`-Aufruf um Reorder-Props ergänzen, „Aus Sammlung entfernen"-Button in Aktionsleiste**

Im `<ModelGrid>`-Aufruf aus Task 5 (dort wo `models={activeCollection ? collectionModels : filtered}` steht), ergänze nach `displayPreference={displayPreference}`:

```tsx
                    reorderable={activeCollection !== null}
                    onReorder={reorderCollection}
```

In der Bulk-Aktionsleiste, nach dem in Task 5 hinzugefügten „Zu Sammlung hinzufügen"-`<div>`-Block, einfügen (nur sichtbar, wenn eine Sammlung aktiv ist):

```tsx
                    {activeCollection && (
                      <button
                        onClick={bulkRemoveFromCollection}
                        className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]"
                      >
                        {t('removeFromCollectionLabel')}
                      </button>
                    )}
```

- [ ] **Step 4: Import-Menü um „Ordner als Sammlung importieren" ergänzen**

In `src/App.tsx`, nach der bestehenden Funktion `const importFolder = () => invoke<ImportResultDto>('import_folder').then(mergeImported);` einfügen:

```ts
  const importFolderAsCollection = () =>
    invoke<ImportResultDto>('import_folder_as_collection').then(mergeImported).then(() => refreshCollections());
```

Im `<Header ... />`-Aufruf, nach `onImportFolder={importFolder}` einfügen:

```tsx
        onImportFolderAsCollection={importFolderAsCollection}
```

In `src/components/Header.tsx`, im `Props`-Interface, nach `onImportFolder: () => void;` einfügen:

```ts
  onImportFolderAsCollection: () => void;
```

In der Funktionssignatur, `onImportFolderAsCollection` zur Destrukturierung hinzufügen. Im bestehenden Import-Dropdown-JSX, nach dem Button

```tsx
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFolder();
              }}
              className="w-full text-left px-3 py-1.5 text-[length:var(--font-size-body)] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFolderOption')}
            </button>
```

einfügen:

```tsx
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFolderAsCollection();
              }}
              className="w-full text-left px-3 py-1.5 text-[length:var(--font-size-body)] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFolderAsCollectionOption')}
            </button>
```

- [ ] **Step 5: TypeScript und Build verifizieren**

Run: `npx tsc --noEmit`
Expected: 0 Fehler.

Run: `npm run build`
Expected: baut ohne Fehler.

- [ ] **Step 6: Manueller Smoke-Test**

Run: `cd src-tauri && cargo build --release && cd .. && npm run tauri dev`
Erwartet: Reiter „Sammlungen" öffnet die Kartenübersicht, „+ Neue Sammlung" legt eine leere Sammlung an. Mehrfachauswahl in der Katalogansicht → „Zu Sammlung hinzufügen" → Sammlung wählen → Modelle erscheinen darin. Klick auf eine Sammlungs-Karte zeigt ihre Modelle, Sortieren-Dropdown ist ausgeblendet, Karten lassen sich per Maus-Ziehen neu anordnen (kein natives Browser-Drag-Icon). In der Sammlungsansicht markierte Modelle lassen sich über „Aus Sammlung entfernen" wieder entfernen (Datei bleibt im Katalog). Import-Menü zeigt „Ordner als Sammlung importieren", wählt man einen Ordner mit mehreren Modell-Dateien, landet eine neue, nach dem Ordner benannte Sammlung mit allen Dateien in der Übersicht.

- [ ] **Step 7: Commit**

```bash
git add src/components/ModelGrid.tsx src/App.tsx src/components/Header.tsx
git commit -m "feat: Drag-Umsortieren, Aus-Sammlung-entfernen und Ordner-Import als Sammlung"
```
