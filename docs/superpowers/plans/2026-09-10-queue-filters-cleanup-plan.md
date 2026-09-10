# Warteschlange, Gespeicherte Filter, Aufräum-Vorschläge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Drei unabhängige Features aus der Konkurrenz-App-Analyse ergänzen: eine geordnete Druck-Warteschlange mit Drag&Drop-Sortierung, benannte gespeicherte Filterkombinationen, und ein manuell ausgelöster Katalog-Scan, der verwaiste Dateipfade und Bestands-Duplikate zur Bereinigung vorschlägt.

**Architecture:** Task 1 legt das gemeinsame Datenfundament an (neue `files.queue_position`-Spalte + neue `saved_filters`-Tabelle, inkl. Migration, Modelle, Repository-Funktionen und Tests). Die drei folgenden Tasks sind vollständig unabhängige vertikale Slices (Backend-Commands + Frontend-UI je Feature) und bauen nur auf Task 1 auf, nicht aufeinander. Aufräum-Vorschläge braucht keine Schema-Änderung (nutzt die bereits vorhandenen Spalten `path` und `content_hash`) und ist daher komplett in seinem eigenen Task enthalten.

**Tech Stack:** Tauri v2 (Rust-Backend, `rusqlite` ohne Migrationsframework) + React/TypeScript-Frontend, eigenes i18n-Context-System (de/en/es/fr).

## Global Constraints

- SQLite ohne Migrationsframework: `CREATE TABLE IF NOT EXISTS` ändert eine bereits bestehende Tabelle nicht. Jede neue Spalte auf einer bestehenden Tabelle (hier: `files.queue_position`) braucht zusätzlich eine fehlertolerante `ALTER TABLE ... ADD COLUMN`-Zeile in `repository.rs`s `init()`, nach dem Vorbild der bereits vorhandenen Zeilen für `print_status`, `creator` etc. Eine komplett neue Tabelle (hier: `saved_filters`) braucht das nicht - `CREATE TABLE IF NOT EXISTS` legt sie auf einer bestehenden DB genauso an wie auf einer frischen.
- Kein Rust-seitiges Validieren von `CHECK`-constrained Strings - gleiches Muster wie das bestehende `set_file_sync_status`: der DB-`CHECK` ist die einzige Durchsetzung.
- i18n: jeder neue sichtbare Text bekommt einen Key in `src/i18n/types.ts` (`Translations`-Interface) und in allen vier Sprachdateien (`de.ts`, `en.ts`, `es.ts`, `fr.ts`). Neue Keys werden jeweils am Ende des Interfaces bzw. am Ende des jeweiligen Sprachobjekts ergänzt (unmittelbar vor der schließenden `}` bzw. `};`), damit die Einfügestelle unabhängig davon eindeutig bleibt, welche vorherigen Tasks bereits eigene Keys angehängt haben.
- Rust-Testabdeckung nur auf Repository-Ebene (`db/mod.rs`) - Befehle mit Dateisystemzugriff (`scan_catalog_issues`, `delete_files`, Import-Funktionen) sind in diesem Projekt bewusst nicht unit-getestet, sondern nur live verifiziert. Reine Repository-Funktionen ohne Dateisystemzugriff bekommen dagegen einen Test in `db/mod.rs`s bestehendem `#[cfg(test)] mod tests`-Block.
- `cargo test` (Backend, aus `src-tauri/`) und `npx tsc --noEmit` (Frontend, aus dem Projekt-Root) müssen nach jedem Task sauber durchlaufen.
- DTO-Namenskonvention: Rust-Structs mit `#[serde(rename_all = "camelCase")]`, damit Backend `snake_case` und Frontend `camelCase` jeweils idiomatisch bleiben (gleiches Muster wie `ModelFileDto`).
- Planungs-Verfeinerung gegenüber der Spec (`docs/superpowers/specs/2026-09-10-queue-filters-cleanup-design.md`): `NewFile` bekommt zusätzlich zu `FileRecord` ein `queue_position`-Feld (immer `None` bei neu importierten Dateien), obwohl die Spec das nicht explizit fordert - erhält die im Projekt etablierte Symmetrie zwischen `NewFile` und `FileRecord` (identische Feldmengen bis auf `id`).

---

## Task 1: Datenbank-Fundament (queue_position-Spalte + saved_filters-Tabelle)

Legt beide Datenmodell-Änderungen aus der Spec an: die neue `files.queue_position`-Spalte (inkl. Migration und der Auto-Clear-Logik beim Markieren als gedruckt) und die komplett neue `saved_filters`-Tabelle mit ihren Repository-Funktionen. Beide werden hier zusammen erledigt, weil sie klein sind und beide reine Datenschicht-Arbeit ohne UI sind - die drei folgenden Tasks bauen jeweils nur ihren eigenen Command- und UI-Layer darauf auf.

**Files:**
- Modify: `src-tauri/src/db/schema.sql`
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/db/mod.rs`

**Interfaces:**
- Produces: `FileRecord`/`NewFile` (`src-tauri/src/db/models.rs`) mit neuem Feld `queue_position: Option<i64>`.
- Produces: `SavedFilterRecord`/`NewSavedFilter` (`src-tauri/src/db/models.rs`): `SavedFilterRecord { id: i64, name: String, folder_id: Option<i64>, tag: Option<String>, creator: Option<String>, query: Option<String>, sort: String, created_at: String }`, `NewSavedFilter` identisch ohne `id`/`created_at`.
- Produces: `db::set_queue_position(conn, file_id: i64, position: Option<i64>) -> Result<(), DbError>`, `db::max_queue_position(conn) -> Result<Option<i64>, DbError>`, `db::insert_saved_filter(conn, filter: &NewSavedFilter) -> Result<i64, DbError>`, `db::list_saved_filters(conn) -> Result<Vec<SavedFilterRecord>, DbError>`, `db::delete_saved_filter(conn, id: i64) -> Result<(), DbError>` (alle `pub` in `repository.rs`, re-exportiert über `db/mod.rs`).
- Produces: `db::set_print_status` (bestehende Funktion) setzt ab jetzt zusätzlich `queue_position` auf `NULL`, wenn der neue Status `"printed"` ist - Task 2 verlässt sich auf dieses Verhalten für "Auto-Entfernen aus der Warteschlange beim Markieren als gedruckt".
- Consumes: nichts von späteren Tasks.

- [ ] **Step 1: Schema erweitern**

In `src-tauri/src/db/schema.sql` die `files`-Tabelle um eine Spalte ergänzen. Aktuell endet sie so:

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
    content_hash TEXT,
    render_snapshot_png BLOB,
    custom_image_png BLOB,
    source_url TEXT
);
```

Ersetzen durch (nur `queue_position` neu):

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
    content_hash TEXT,
    render_snapshot_png BLOB,
    custom_image_png BLOB,
    source_url TEXT,
    queue_position INTEGER
);
```

Am Ende der Datei (nach der bestehenden `filament_spools`-Tabelle) die neue Tabelle ergänzen:

```sql
CREATE TABLE IF NOT EXISTS saved_filters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    folder_id INTEGER,
    tag TEXT,
    creator TEXT,
    query TEXT,
    sort TEXT NOT NULL,
    created_at TEXT NOT NULL
);
```

- [ ] **Step 2: Migration + Auto-Clear-Logik in repository.rs**

In `src-tauri/src/db/repository.rs`s `init()`-Funktion nach der letzten bestehenden `ALTER TABLE`-Zeile (`let _ = conn.execute("ALTER TABLE files ADD COLUMN source_url TEXT", []);`) eine weitere ergänzen:

```rust
    let _ = conn.execute("ALTER TABLE files ADD COLUMN source_url TEXT", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN queue_position INTEGER", []);
    Ok(())
}
```

(`saved_filters` braucht keine `ALTER TABLE`-Zeile - es ist eine komplett neue Tabelle, die `CREATE TABLE IF NOT EXISTS` im bereits laufenden `execute_batch(SCHEMA_SQL)` sowohl auf frischen als auch auf bestehenden DBs korrekt anlegt.)

Den Import-Block am Dateianfang erweitern:

```rust
use super::models::{
    CloudAccountRecord, FileRecord, FileType, FilamentSpoolRecord, FolderRecord, MaterialRecord,
    NewFile, NewFilamentSpool, NewSavedFilter, SavedFilterRecord, TagCount, CreatorCount,
};
```

Die bestehende `set_print_status`-Funktion so ändern, dass sie `queue_position` beim Markieren als gedruckt automatisch löscht:

```rust
pub fn set_print_status(conn: &Connection, file_id: i64, status: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET print_status = ?1,
                queue_position = CASE WHEN ?1 = 'printed' THEN NULL ELSE queue_position END
         WHERE id = ?2",
        params![status, file_id],
    )?;
    Ok(())
}
```

Direkt danach zwei neue Funktionen ergänzen:

```rust
pub fn set_queue_position(conn: &Connection, file_id: i64, position: Option<i64>) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET queue_position = ?1 WHERE id = ?2",
        params![position, file_id],
    )?;
    Ok(())
}

pub fn max_queue_position(conn: &Connection) -> Result<Option<i64>, DbError> {
    Ok(conn.query_row("SELECT MAX(queue_position) FROM files", [], |row| row.get(0))?)
}
```

Die `insert_file`-Funktion um die neue Spalte erweitern. Aktuell:

```rust
    tx.execute(
        "INSERT INTO files (
            name, path, file_type, folder_id, origin, cloud_id, sync_status,
            file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
            volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
            print_status, last_viewed_at, creator, content_hash,
            render_snapshot_png, custom_image_png, source_url
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
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
            file.render_snapshot_png,
            file.custom_image_png,
            file.source_url,
        ],
    )?;
```

Ersetzen durch:

```rust
    tx.execute(
        "INSERT INTO files (
            name, path, file_type, folder_id, origin, cloud_id, sync_status,
            file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
            volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
            print_status, last_viewed_at, creator, content_hash,
            render_snapshot_png, custom_image_png, source_url, queue_position
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
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
            file.render_snapshot_png,
            file.custom_image_png,
            file.source_url,
            file.queue_position,
        ],
    )?;
```

`get_file` und `list_files` um die neue Spalte in der SELECT-Liste erweitern. Aktuell (beide Funktionen haben dieselbe Spaltenliste):

```sql
SELECT id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
        file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
        volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
        print_status, last_viewed_at, creator, content_hash,
        render_snapshot_png, custom_image_png, source_url
```

In beiden Vorkommen (in `get_file`s `query_row`-Aufruf und in `list_files`s `stmt`-Vorbereitung) ersetzen durch:

```sql
SELECT id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
        file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
        volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
        print_status, last_viewed_at, creator, content_hash,
        render_snapshot_png, custom_image_png, source_url, queue_position
```

`row_to_file` entsprechend erweitern. Aktuell endet die `Ok(FileRecord { ... })`-Konstruktion mit:

```rust
        render_snapshot_png: row.get(21)?,
        custom_image_png: row.get(22)?,
        source_url: row.get(23)?,
    })
}
```

Ersetzen durch:

```rust
        render_snapshot_png: row.get(21)?,
        custom_image_png: row.get(22)?,
        source_url: row.get(23)?,
        queue_position: row.get(24)?,
    })
}
```

Am Ende der Datei (nach `delete_filament_spool`) die drei neuen `saved_filters`-Funktionen ergänzen:

```rust
pub fn insert_saved_filter(conn: &Connection, filter: &NewSavedFilter) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO saved_filters (name, folder_id, tag, creator, query, sort, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            filter.name,
            filter.folder_id,
            filter.tag,
            filter.creator,
            filter.query,
            filter.sort,
            chrono::Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_saved_filters(conn: &Connection) -> Result<Vec<SavedFilterRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, folder_id, tag, creator, query, sort, created_at
         FROM saved_filters ORDER BY created_at",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SavedFilterRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                folder_id: row.get(2)?,
                tag: row.get(3)?,
                creator: row.get(4)?,
                query: row.get(5)?,
                sort: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn delete_saved_filter(conn: &Connection, id: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM saved_filters WHERE id = ?1", params![id])?;
    Ok(())
}
```

- [ ] **Step 3: models.rs erweitern**

In `src-tauri/src/db/models.rs` sowohl `NewFile` als auch `FileRecord` um ein Feld erweitern. Beide Structs enden aktuell mit:

```rust
    pub source_url: Option<String>,
}
```

In beiden Vorkommen ersetzen durch:

```rust
    pub source_url: Option<String>,
    pub queue_position: Option<i64>,
}
```

Am Ende der Datei (nach `NewFilamentSpool`) zwei neue Structs ergänzen:

```rust
#[derive(Debug, Clone)]
pub struct SavedFilterRecord {
    pub id: i64,
    pub name: String,
    pub folder_id: Option<i64>,
    pub tag: Option<String>,
    pub creator: Option<String>,
    pub query: Option<String>,
    pub sort: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NewSavedFilter {
    pub name: String,
    pub folder_id: Option<i64>,
    pub tag: Option<String>,
    pub creator: Option<String>,
    pub query: Option<String>,
    pub sort: String,
}
```

- [ ] **Step 4: import_one in commands.rs anpassen**

In `src-tauri/src/commands.rs`s `import_one`-Funktion baut die `NewFile`-Konstruktion aktuell mit `source_url: None,` als letztem Feld ab. Neu importierte Dateien sind nie vorab in der Warteschlange:

```rust
        content_hash,
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
    };
```

Ersetzen durch:

```rust
        content_hash,
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
        queue_position: None,
    };
```

- [ ] **Step 5: db/mod.rs - Re-Exports, Testhelfer und Tests**

In `src-tauri/src/db/mod.rs` die `pub use repository::{...}`-Liste erweitern. Aktuell:

```rust
pub use repository::{
    add_tag_to_file, connect, delete_file, delete_filament_spool, delete_unused_tags,
    file_exists_by_hash, file_exists_by_path, get_file, insert_file, insert_filament_spool, insert_folder,
    list_cloud_accounts, list_creator_counts, list_filament_spools, list_files,
    list_files_missing_content_hash, list_folders, list_tag_counts, mark_file_viewed,
    remove_tag_from_file, set_cloud_account_status, set_content_hash, set_custom_image_png, set_file_cloud_link,
    set_file_modified_at, set_file_sync_status, set_print_status, set_render_snapshot_png, set_source_url, update_filament_spool,
    upsert_cloud_account,
};
```

Ersetzen durch:

```rust
pub use repository::{
    add_tag_to_file, connect, delete_file, delete_filament_spool, delete_saved_filter, delete_unused_tags,
    file_exists_by_hash, file_exists_by_path, get_file, insert_file, insert_filament_spool, insert_folder,
    insert_saved_filter, list_cloud_accounts, list_creator_counts, list_filament_spools, list_files,
    list_files_missing_content_hash, list_folders, list_saved_filters, list_tag_counts, mark_file_viewed,
    max_queue_position, remove_tag_from_file, set_cloud_account_status, set_content_hash, set_custom_image_png,
    set_file_cloud_link, set_file_modified_at, set_file_sync_status, set_print_status, set_queue_position,
    set_render_snapshot_png, set_source_url, update_filament_spool, upsert_cloud_account,
};
```

Im Testmodul den `use models::{...}`-Import erweitern:

```rust
    use models::{FileType, MaterialRecord, NewFile, NewFilamentSpool};
```

Ersetzen durch:

```rust
    use models::{FileType, MaterialRecord, NewFile, NewFilamentSpool, NewSavedFilter};
```

`sample_file()` um das neue Feld ergänzen. Aktuell endet die `NewFile { ... }`-Konstruktion mit:

```rust
            content_hash: None,
            render_snapshot_png: None,
            custom_image_png: None,
            source_url: None,
        }
    }
```

Ersetzen durch:

```rust
            content_hash: None,
            render_snapshot_png: None,
            custom_image_png: None,
            source_url: None,
            queue_position: None,
        }
    }
```

Die bestehende Migrationstest-Funktion `init_migrates_a_pre_existing_database_missing_the_new_columns` prüft aktuell nach `repository::init(&conn)` vier Felder. Direkt nach der letzten Assertion dort:

```rust
        assert_eq!(file.content_hash, None);
    }
```

Ersetzen durch:

```rust
        assert_eq!(file.content_hash, None);
        assert_eq!(file.queue_position, None);
    }
```

Am Ende der Datei (nach dem letzten bestehenden Test `set_source_url_updates_and_clears_the_url`) folgende neue Tests ergänzen:

```rust
    #[test]
    fn set_queue_position_stores_and_clears_the_position() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        set_queue_position(&conn, id, Some(3)).expect("set position");
        let file = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(file.queue_position, Some(3));

        set_queue_position(&conn, id, None).expect("clear position");
        let file = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(file.queue_position, None);
    }

    #[test]
    fn max_queue_position_returns_the_highest_value_or_none() {
        let mut conn = connect_in_memory().expect("connect");
        assert_eq!(max_queue_position(&conn).expect("query"), None);

        let id_a = insert_file(&mut conn, &sample_file()).expect("insert a");
        let mut b = sample_file();
        b.name = "second.3mf".to_string();
        b.path = "/tmp/second.3mf".to_string();
        let id_b = insert_file(&mut conn, &b).expect("insert b");

        set_queue_position(&conn, id_a, Some(1)).expect("set a");
        set_queue_position(&conn, id_b, Some(5)).expect("set b");

        assert_eq!(max_queue_position(&conn).expect("query"), Some(5));
    }

    #[test]
    fn set_print_status_to_printed_clears_the_queue_position() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");
        set_queue_position(&conn, id, Some(2)).expect("set position");

        set_print_status(&conn, id, "printed").expect("mark printed");
        let file = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(file.print_status, "printed");
        assert_eq!(file.queue_position, None);
    }

    #[test]
    fn set_print_status_to_not_printed_keeps_the_queue_position() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");
        set_queue_position(&conn, id, Some(2)).expect("set position");

        set_print_status(&conn, id, "not_printed").expect("mark not printed");
        let file = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(file.queue_position, Some(2));
    }

    fn sample_saved_filter() -> NewSavedFilter {
        NewSavedFilter {
            name: "Meine Vasen".to_string(),
            folder_id: None,
            tag: Some("vase".to_string()),
            creator: None,
            query: None,
            sort: "name".to_string(),
        }
    }

    #[test]
    fn saved_filters_round_trip() {
        let conn = connect_in_memory().expect("connect");
        let id = insert_saved_filter(&conn, &sample_saved_filter()).expect("insert");
        assert!(id > 0);

        let filters = list_saved_filters(&conn).expect("list");
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].name, "Meine Vasen");
        assert_eq!(filters[0].tag, Some("vase".to_string()));
        assert_eq!(filters[0].sort, "name");
    }

    #[test]
    fn delete_saved_filter_removes_it() {
        let conn = connect_in_memory().expect("connect");
        let id = insert_saved_filter(&conn, &sample_saved_filter()).expect("insert");

        delete_saved_filter(&conn, id).expect("delete");

        let filters = list_saved_filters(&conn).expect("list");
        assert!(filters.is_empty());
    }
```

- [ ] **Step 6: Tests laufen lassen**

Run: `cd src-tauri && cargo test`
Expected: alle Tests (bestehende + neue) grün.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/schema.sql src-tauri/src/db/repository.rs src-tauri/src/db/models.rs src-tauri/src/db/mod.rs src-tauri/src/commands.rs
git commit -m "feat: Datenfundament für Warteschlange und gespeicherte Filter"
```

---

## Task 2: Warteschlange ("als Nächstes drucken")

Vollständiger vertikaler Slice für die Druck-Warteschlange: Backend-Commands zum Hinzufügen/Entfernen/Neusortieren, eine neue einklappbare Sidebar-Sektion mit nativer HTML5-Drag&Drop-Neusortierung, ein Umschalt-Button im DetailPanel und ein Eintrag im Kartei-Kontextmenü. Baut ausschließlich auf Task 1 auf.

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types/index.ts`
- Modify: `src/App.tsx`
- Modify: `src/components/Sidebar.tsx`
- Modify: `src/components/DetailPanel.tsx`
- Modify: `src/components/ContextMenu.tsx`
- Modify: `src/i18n/types.ts`
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`

**Interfaces:**
- Consumes: `db::set_queue_position`, `db::max_queue_position`, `db::set_print_status` (Task 1).
- Produces: Tauri-Commands `add_to_queue(fileId: string) -> number`, `remove_from_queue(fileId: string) -> void`, `reorder_queue(updates: {fileId: string, position: number}[]) -> void`.
- Produces: `ModelFile.queuePosition: number | null` (Frontend-Typ) - spätere Tasks berühren dieses Feld nicht, aber jede Stelle, die `ModelFile` aus dem Backend liest, bekommt es automatisch mitgeliefert.

- [ ] **Step 1: ModelFileDto um queue_position erweitern**

In `src-tauri/src/commands.rs`s `ModelFileDto`-Struct endet die Feldliste aktuell mit:

```rust
    pub display_image: Option<String>,
    pub source_url: Option<String>,
}
```

Ersetzen durch:

```rust
    pub display_image: Option<String>,
    pub source_url: Option<String>,
    pub queue_position: Option<i64>,
}
```

In `to_dto()` endet die `ModelFileDto { ... }`-Konstruktion aktuell mit:

```rust
        display_image,
        source_url: file.source_url,
    }
}
```

Ersetzen durch:

```rust
        display_image,
        source_url: file.source_url,
        queue_position: file.queue_position,
    }
}
```

- [ ] **Step 2: Drei neue Commands**

In `src-tauri/src/commands.rs` nach der bestehenden `set_print_status`-Command-Funktion (vor `mark_file_viewed`) drei neue Commands ergänzen:

```rust
#[tauri::command]
pub fn add_to_queue(state: State<AppState>, file_id: String) -> CmdResult<i64> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let next = db::max_queue_position(&conn).map_err(|e| e.to_string())?.unwrap_or(0) + 1;
    db::set_queue_position(&conn, id, Some(next)).map_err(|e| e.to_string())?;
    Ok(next)
}

#[tauri::command]
pub fn remove_from_queue(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_queue_position(&conn, id, None).map_err(|e| e.to_string())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuePositionUpdate {
    pub file_id: String,
    pub position: i64,
}

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

- [ ] **Step 3: Commands registrieren**

In `src-tauri/src/lib.rs`s `generate_handler!`-Liste nach `commands::mark_file_viewed,` drei neue Zeilen ergänzen:

```rust
            commands::mark_file_viewed,
            commands::add_to_queue,
            commands::remove_from_queue,
            commands::reorder_queue,
            commands::upload_custom_image,
```

- [ ] **Step 4: Backend-Tests laufen lassen**

Run: `cd src-tauri && cargo test`
Expected: kompiliert und alle Tests grün (die drei neuen Commands haben keine eigenen Unit-Tests - Dateisystem-/DB-Interaktion über `AppState` wird wie die übrigen Commands nur live verifiziert; die zugrundeliegende Repository-Logik ist bereits in Task 1 getestet).

- [ ] **Step 5: Frontend-Typ erweitern**

In `src/types/index.ts` endet `ModelFile` aktuell mit:

```ts
  displayImage: string | null;
  sourceUrl: string | null;
}
```

Ersetzen durch:

```ts
  displayImage: string | null;
  sourceUrl: string | null;
  queuePosition: number | null;
}
```

- [ ] **Step 6: i18n-Keys ergänzen**

In `src/i18n/types.ts` im `Translations`-Interface vor der schließenden `}` (nach `sourceUrlPlaceholder: string;`) ergänzen:

```ts
  sourceUrlPlaceholder: string;

  queueHeading: string;
  queueEmptyState: string;
  inQueueLabel: string;
  notInQueueLabel: string;
  addToQueue: string;
  removeFromQueue: string;
}
```

In `src/i18n/de.ts` vor der schließenden `};` (nach `sourceUrlPlaceholder: 'https://…',`) ergänzen:

```ts
  sourceUrlPlaceholder: 'https://…',

  queueHeading: 'Warteschlange',
  queueEmptyState: 'Keine Modelle in der Warteschlange',
  inQueueLabel: 'In Warteschlange',
  notInQueueLabel: 'Nicht in Warteschlange',
  addToQueue: 'Zur Warteschlange hinzufügen',
  removeFromQueue: 'Aus Warteschlange entfernen',
};
```

In `src/i18n/en.ts` analog:

```ts
  sourceUrlPlaceholder: 'https://…',

  queueHeading: 'Print Queue',
  queueEmptyState: 'No models in the queue',
  inQueueLabel: 'In queue',
  notInQueueLabel: 'Not in queue',
  addToQueue: 'Add to queue',
  removeFromQueue: 'Remove from queue',
};
```

In `src/i18n/es.ts` analog:

```ts
  sourceUrlPlaceholder: 'https://…',

  queueHeading: 'Cola de impresión',
  queueEmptyState: 'No hay modelos en la cola',
  inQueueLabel: 'En cola',
  notInQueueLabel: 'No está en cola',
  addToQueue: 'Añadir a la cola',
  removeFromQueue: 'Quitar de la cola',
};
```

In `src/i18n/fr.ts` analog:

```ts
  sourceUrlPlaceholder: 'https://…',

  queueHeading: "File d'impression",
  queueEmptyState: 'Aucun modèle dans la file',
  inQueueLabel: 'Dans la file',
  notInQueueLabel: "Pas dans la file",
  addToQueue: "Ajouter à la file",
  removeFromQueue: 'Retirer de la file',
};
```

- [ ] **Step 7: Sidebar - neue Warteschlange-Sektion mit Drag&Drop**

In `src/components/Sidebar.tsx` das `Props`-Interface um vier Felder erweitern. Aktuell beginnt es mit:

```ts
interface Props {
  query: string;
  onQueryChange: (q: string) => void;
  folders: Folder[];
```

Ersetzen durch:

```ts
interface Props {
  query: string;
  onQueryChange: (q: string) => void;
  queue: ModelFile[];
  onQueueReorder: (orderedIds: string[]) => void;
  onQueueRemove: (id: string) => void;
  onQueueSelect: (id: string) => void;
  folders: Folder[];
```

Den Import am Dateianfang erweitern - aktuell:

```ts
import type { Folder, TagCount, CreatorCount, CloudAccount } from '../types';
```

Ersetzen durch:

```ts
import type { Folder, TagCount, CreatorCount, CloudAccount, ModelFile } from '../types';
```

Die Funktionssignatur um die neuen Props erweitern - aktuell:

```ts
export function Sidebar({
  query,
  onQueryChange,
  folders,
```

Ersetzen durch:

```ts
export function Sidebar({
  query,
  onQueryChange,
  queue,
  onQueueReorder,
  onQueueRemove,
  onQueueSelect,
  folders,
```

Nach der bestehenden Zustandszeile `const [tagsCollapsed, setTagsCollapsed] = useState(false);` zwei neue Zustände ergänzen:

```ts
  const [tagsCollapsed, setTagsCollapsed] = useState(false);
  const [creatorsCollapsed, setCreatorsCollapsed] = useState(false);
  const [queueCollapsed, setQueueCollapsed] = useState(false);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
```

Direkt nach dem schließenden `))}` der Ordnerliste (vor dem Tags-Überschriften-`<div>`, das mit `onClick={() => setTagsCollapsed((c) => !c)}` beginnt) die neue Sektion einfügen:

```tsx
        <div
          onClick={() => setQueueCollapsed((c) => !c)}
          className="flex items-center justify-between px-1.5 pt-[18px] pb-2 cursor-pointer"
        >
          <span className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('queueHeading')}
          </span>
          <span className="font-mono-ui text-[9px] leading-none text-[var(--ink-3)]">
            {queueCollapsed ? '▾' : '▴'}
          </span>
        </div>
        {!queueCollapsed && queue.length === 0 && (
          <div className="px-1.5 pb-2 font-mono-ui text-[10.5px] text-[var(--ink-3)]">
            {t('queueEmptyState')}
          </div>
        )}
        {!queueCollapsed && queue.map((model, index) => (
          <div
            key={model.id}
            draggable
            onDragStart={() => setDragIndex(index)}
            onDragOver={(e) => e.preventDefault()}
            onDrop={() => {
              if (dragIndex === null || dragIndex === index) return;
              const ids = queue.map((m) => m.id);
              const [moved] = ids.splice(dragIndex, 1);
              ids.splice(index, 0, moved);
              onQueueReorder(ids);
              setDragIndex(null);
            }}
            onDragEnd={() => setDragIndex(null)}
            onClick={() => onQueueSelect(model.id)}
            className="flex items-center gap-2 h-7 px-1.5 rounded-[3px] cursor-grab text-[var(--ink-2)] hover:text-[var(--ink)]"
          >
            <span className="font-mono-ui text-[10px] text-[var(--ink-3)] w-3.5">{index + 1}</span>
            <span className="flex-1 text-xs overflow-hidden text-ellipsis whitespace-nowrap">
              {model.name}
            </span>
            <span
              onClick={(e) => {
                e.stopPropagation();
                onQueueRemove(model.id);
              }}
              className="font-mono-ui text-[10px] text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]"
            >
              ✕
            </span>
          </div>
        ))}

        <div
          onClick={() => setTagsCollapsed((c) => !c)}
          className="flex items-center justify-between px-1.5 pt-[18px] pb-2 cursor-pointer"
        >
```

(Die letzte Zeile oben ist die bereits bestehende Tags-Überschrift und dient hier nur als Ankerpunkt - sie bleibt unverändert stehen, es wird nur davor eingefügt.)

- [ ] **Step 8: App.tsx - Zustand und Handler**

In `src/App.tsx` den Typ-Import erweitern - aktuell:

```ts
import type { ModelFile, Folder, TagCount, CreatorCount, CloudAccount, Origin, ViewMode, SortKey } from './types';
```

(bleibt unverändert - `ModelFile` ist bereits importiert und trägt das neue Feld automatisch mit.)

Nach der bestehenden `filtered`-`useMemo` (endet mit `}, [models, activeFolderId, activeTag, activeCreator, query, sort]);`) eine neue `useMemo` für die Warteschlange ergänzen:

```ts
  const queue = useMemo(
    () =>
      models
        .filter((m) => m.queuePosition !== null)
        .sort((a, b) => (a.queuePosition ?? 0) - (b.queuePosition ?? 0)),
    [models],
  );
```

Die bestehende `togglePrintStatus`-Funktion so ändern, dass sie beim Markieren als gedruckt auch `queuePosition` lokal löscht (spiegelt die Backend-Logik aus Task 1). Aktuell:

```ts
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

Ersetzen durch:

```ts
  const togglePrintStatus = (id: string) => {
    const current = models.find((m) => m.id === id);
    if (!current) return;
    const next = current.printStatus === 'printed' ? 'not_printed' : 'printed';
    setModels((prev) =>
      prev.map((m) =>
        m.id === id
          ? { ...m, printStatus: next, queuePosition: next === 'printed' ? null : m.queuePosition }
          : m,
      ),
    );
    invoke('set_print_status', { fileId: id, status: next }).catch((e) => {
      console.error('[print-status] Aktualisieren fehlgeschlagen:', e);
    });
  };
```

Direkt danach (vor `const uploadCustomImage`) drei neue Handler ergänzen:

```ts
  const addToQueue = (id: string) => {
    invoke<number>('add_to_queue', { fileId: id })
      .then((position) => {
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, queuePosition: position } : m)));
      })
      .catch((e) => console.error('[queue] Hinzufügen fehlgeschlagen:', e));
  };

  const removeFromQueue = (id: string) => {
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, queuePosition: null } : m)));
    invoke('remove_from_queue', { fileId: id }).catch((e) => {
      console.error('[queue] Entfernen fehlgeschlagen:', e);
    });
  };

  const reorderQueue = (orderedIds: string[]) => {
    const updates: { fileId: string; position: number }[] = [];
    orderedIds.forEach((id, index) => {
      const position = index + 1;
      const current = models.find((m) => m.id === id);
      if (current && current.queuePosition !== position) {
        updates.push({ fileId: id, position });
      }
    });
    if (updates.length === 0) return;
    setModels((prev) =>
      prev.map((m) => {
        const index = orderedIds.indexOf(m.id);
        return index === -1 ? m : { ...m, queuePosition: index + 1 };
      }),
    );
    invoke('reorder_queue', { updates }).catch((e) => {
      console.error('[queue] Neusortierung fehlgeschlagen:', e);
    });
  };
```

Vor dem `return (` der Komponente (direkt nach `const selected = ...`) eine Variable für das aktuell im Kontextmenü referenzierte Modell ergänzen. Aktuell:

```ts
  const selected = models.find((m) => m.id === selectedId) ?? null;
```

Ersetzen durch:

```ts
  const selected = models.find((m) => m.id === selectedId) ?? null;
  const contextModel = contextMenu ? models.find((m) => m.id === contextMenu.modelId) ?? null : null;
```

- [ ] **Step 9: App.tsx - Sidebar/DetailPanel/ContextMenu verdrahten**

Die `<Sidebar ... />`-Instanz um die neuen Props erweitern. Aktuell beginnt sie:

```tsx
          <Sidebar
            query={query}
            onQueryChange={setQuery}
            folders={folders}
```

Ersetzen durch:

```tsx
          <Sidebar
            query={query}
            onQueryChange={setQuery}
            queue={queue}
            onQueueReorder={reorderQueue}
            onQueueRemove={removeFromQueue}
            onQueueSelect={selectModel}
            folders={folders}
```

Die `<DetailPanel ... />`-Instanz um `onToggleQueue` erweitern. Aktuell:

```tsx
            onTogglePrintStatus={() => selected && togglePrintStatus(selected.id)}
            onUploadImage={() => selected && uploadCustomImage(selected.id)}
```

Ersetzen durch:

```tsx
            onTogglePrintStatus={() => selected && togglePrintStatus(selected.id)}
            onToggleQueue={() =>
              selected && (selected.queuePosition !== null ? removeFromQueue(selected.id) : addToQueue(selected.id))
            }
            onUploadImage={() => selected && uploadCustomImage(selected.id)}
```

Den `<ContextMenu ... />`-Block um `inQueue`/`onToggleQueue` erweitern und auf `contextModel` statt direkter `contextMenu`-Prüfung umstellen. Aktuell:

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
```

Ersetzen durch:

```tsx
      {contextMenu && contextModel && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          onOpenInSlicer={() => openInSlicer(contextMenu.modelId)}
          onDelete={() => deleteModel(contextMenu.modelId)}
          inQueue={contextModel.queuePosition !== null}
          onToggleQueue={() =>
            contextModel.queuePosition !== null ? removeFromQueue(contextModel.id) : addToQueue(contextModel.id)
          }
        />
      )}
```

- [ ] **Step 10: DetailPanel - Warteschlange-Umschalter**

In `src/components/DetailPanel.tsx` das `Props`-Interface erweitern. Aktuell:

```ts
  onTogglePrintStatus: () => void;
  onUploadImage: () => void;
```

Ersetzen durch:

```ts
  onTogglePrintStatus: () => void;
  onToggleQueue: () => void;
  onUploadImage: () => void;
```

Die Funktionssignatur entsprechend erweitern. Aktuell:

```ts
  onTogglePrintStatus,
  onUploadImage,
```

Ersetzen durch:

```ts
  onTogglePrintStatus,
  onToggleQueue,
  onUploadImage,
```

Direkt nach dem bestehenden Druckstatus-Block (endet mit `</div>` nach dem `markAsPrinted`/`markAsNotPrinted`-Button, vor dem `<div className="px-4 pt-3.5 pb-1">`-Block für die Metadaten) einen neuen Block einfügen:

```tsx
        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span className="flex-1 text-[12.5px] font-medium">
            {model.queuePosition !== null ? t('inQueueLabel') : t('notInQueueLabel')}
          </span>
          <button
            onClick={onToggleQueue}
            className="h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {model.queuePosition !== null ? t('removeFromQueue') : t('addToQueue')}
          </button>
        </div>

        <div className="px-4 pt-3.5 pb-1">
```

(Die letzte Zeile ist der bereits bestehende Metadaten-Block-Anfang und dient nur als Ankerpunkt.)

- [ ] **Step 11: ContextMenu - Warteschlange-Eintrag**

In `src/components/ContextMenu.tsx` das `Props`-Interface erweitern. Aktuell:

```ts
interface Props {
  x: number;
  y: number;
  onClose: () => void;
  onOpenInSlicer: () => void;
  onDelete: () => void;
}
```

Ersetzen durch:

```ts
interface Props {
  x: number;
  y: number;
  onClose: () => void;
  onOpenInSlicer: () => void;
  onDelete: () => void;
  inQueue: boolean;
  onToggleQueue: () => void;
}
```

Die Funktionssignatur entsprechend erweitern:

```ts
export function ContextMenu({ x, y, onClose, onOpenInSlicer, onDelete, inQueue, onToggleQueue }: Props) {
```

Im nicht-Lösch-Bestätigungs-Zweig (der `<>...</>`-Block mit den beiden Buttons `openInSlicer`/`delete`) zwischen den beiden bestehenden Buttons einen neuen einfügen. Aktuell:

```tsx
          <button
            onClick={() => {
              onOpenInSlicer();
              onClose();
            }}
            className="w-full text-left px-3 py-2 text-[12.5px] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {t('openInSlicer')}
          </button>
          <button
            onClick={() => setConfirmDelete(true)}
            className="w-full text-left px-3 py-2 text-[12.5px] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {t('delete')}
          </button>
```

Ersetzen durch:

```tsx
          <button
            onClick={() => {
              onOpenInSlicer();
              onClose();
            }}
            className="w-full text-left px-3 py-2 text-[12.5px] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {t('openInSlicer')}
          </button>
          <button
            onClick={() => {
              onToggleQueue();
              onClose();
            }}
            className="w-full text-left px-3 py-2 text-[12.5px] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {inQueue ? t('removeFromQueue') : t('addToQueue')}
          </button>
          <button
            onClick={() => setConfirmDelete(true)}
            className="w-full text-left px-3 py-2 text-[12.5px] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {t('delete')}
          </button>
```

- [ ] **Step 12: Typecheck**

Run: `npx tsc --noEmit`
Expected: keine Fehler.

- [ ] **Step 13: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/types/index.ts src/App.tsx src/components/Sidebar.tsx src/components/DetailPanel.tsx src/components/ContextMenu.tsx src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "feat: Druck-Warteschlange mit Drag&Drop-Sortierung"
```

---

## Task 3: Gespeicherte Filter

Vollständiger vertikaler Slice für benannte Filterkombinationen: Backend-Commands zum Speichern/Auflisten/Löschen und eine neue einklappbare Sidebar-Sektion mit "+"-Button zum Speichern der aktuell aktiven Filterkombination. Baut ausschließlich auf Task 1 auf, unabhängig von Task 2.

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types/index.ts`
- Modify: `src/App.tsx`
- Modify: `src/components/Sidebar.tsx`
- Modify: `src/i18n/types.ts`
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`

**Interfaces:**
- Consumes: `db::insert_saved_filter`, `db::list_saved_filters`, `db::delete_saved_filter` (Task 1).
- Produces: Tauri-Commands `save_filter(filter: SavedFilterInputDto) -> SavedFilterDto`, `list_saved_filters() -> SavedFilterDto[]`, `delete_saved_filter(filterId: string) -> void`.
- Produces: `SavedFilter`-Frontend-Typ (`src/types/index.ts`).

- [ ] **Step 1: DTOs und Commands in commands.rs**

In `src-tauri/src/commands.rs` nach der bestehenden `CreatorCountDto`-Struct-Definition (vor `const MATERIAL_DENSITY_G_CM3`) die neuen DTOs ergänzen:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFilterDto {
    pub id: String,
    pub name: String,
    pub folder_id: Option<String>,
    pub tag: Option<String>,
    pub creator: Option<String>,
    pub query: Option<String>,
    pub sort: String,
}

fn saved_filter_to_dto(record: db::models::SavedFilterRecord) -> SavedFilterDto {
    SavedFilterDto {
        id: record.id.to_string(),
        name: record.name,
        folder_id: record.folder_id.map(|id| id.to_string()),
        tag: record.tag,
        creator: record.creator,
        query: record.query,
        sort: record.sort,
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFilterInputDto {
    pub name: String,
    pub folder_id: Option<String>,
    pub tag: Option<String>,
    pub creator: Option<String>,
    pub query: Option<String>,
    pub sort: String,
}
```

Am Ende der Datei, direkt vor dem `#[cfg(test)]`-Block, die drei neuen Commands ergänzen:

```rust
#[tauri::command]
pub fn save_filter(state: State<AppState>, filter: SavedFilterInputDto) -> CmdResult<SavedFilterDto> {
    let folder_id = filter
        .folder_id
        .map(|s| s.parse::<i64>().map_err(|_| "invalid folder id".to_string()))
        .transpose()?;
    let conn = lock_db(&state)?;
    let new_filter = db::models::NewSavedFilter {
        name: filter.name,
        folder_id,
        tag: filter.tag,
        creator: filter.creator,
        query: filter.query,
        sort: filter.sort,
    };
    let id = db::insert_saved_filter(&conn, &new_filter).map_err(|e| e.to_string())?;
    Ok(saved_filter_to_dto(db::models::SavedFilterRecord {
        id,
        name: new_filter.name,
        folder_id: new_filter.folder_id,
        tag: new_filter.tag,
        creator: new_filter.creator,
        query: new_filter.query,
        sort: new_filter.sort,
        created_at: String::new(),
    }))
}

#[tauri::command]
pub fn list_saved_filters(state: State<AppState>) -> CmdResult<Vec<SavedFilterDto>> {
    let conn = lock_db(&state)?;
    let filters = db::list_saved_filters(&conn).map_err(|e| e.to_string())?;
    Ok(filters.into_iter().map(saved_filter_to_dto).collect())
}

#[tauri::command]
pub fn delete_saved_filter(state: State<AppState>, filter_id: String) -> CmdResult<()> {
    let id: i64 = filter_id.parse().map_err(|_| "invalid filter id".to_string())?;
    let conn = lock_db(&state)?;
    db::delete_saved_filter(&conn, id).map_err(|e| e.to_string())
}
```

(`created_at: String::new()` ist bewusst ein Platzhalter, der nie nach außen dringt - `SavedFilterDto` transportiert `created_at` gar nicht erst, `saved_filter_to_dto` verwirft das Feld einfach.)

- [ ] **Step 2: Commands registrieren**

In `src-tauri/src/lib.rs`s `generate_handler!`-Liste nach `commands::set_source_url,` drei neue Zeilen ergänzen:

```rust
            commands::set_source_url,
            commands::save_filter,
            commands::list_saved_filters,
            commands::delete_saved_filter,
            commands::import_files,
```

- [ ] **Step 3: Backend-Tests laufen lassen**

Run: `cd src-tauri && cargo test`
Expected: kompiliert und alle Tests grün.

- [ ] **Step 4: Frontend-Typ ergänzen**

In `src/types/index.ts` nach der bestehenden `CreatorCount`-Interface (vor `CloudAccount`) ein neues Interface ergänzen:

```ts
export interface SavedFilter {
  id: string;
  name: string;
  folderId: string | null;
  tag: string | null;
  creator: string | null;
  query: string | null;
  sort: SortKey;
}
```

(Steht syntaktisch vor der `SortKey`-Definition weiter unten in derselben Datei - das ist in TypeScript unproblematisch, da Typdeklarationen in einer Datei nicht der Lesereihenfolge folgen müssen.)

- [ ] **Step 5: i18n-Keys ergänzen**

In `src/i18n/types.ts` im `Translations`-Interface vor der schließenden `}` ergänzen:

```ts
  savedFiltersHeading: string;
  savedFilterNamePlaceholder: string;
}
```

In `src/i18n/de.ts` vor der schließenden `};` ergänzen:

```ts
  savedFiltersHeading: 'Gespeicherte Filter',
  savedFilterNamePlaceholder: 'Name…',
};
```

In `src/i18n/en.ts` analog:

```ts
  savedFiltersHeading: 'Saved Filters',
  savedFilterNamePlaceholder: 'Name…',
};
```

In `src/i18n/es.ts` analog:

```ts
  savedFiltersHeading: 'Filtros guardados',
  savedFilterNamePlaceholder: 'Nombre…',
};
```

In `src/i18n/fr.ts` analog:

```ts
  savedFiltersHeading: 'Filtres enregistrés',
  savedFilterNamePlaceholder: 'Nom…',
};
```

(Falls Task 2 bereits eigene Keys am Dateiende ergänzt hat, landen diese Keys danach - die Einfügestelle ist immer "unmittelbar vor der schließenden `}`/`};`", unabhängig von zuvor eingefügten Keys.)

- [ ] **Step 6: Sidebar - neue Gespeicherte-Filter-Sektion**

In `src/components/Sidebar.tsx` den Typ-Import erweitern. Aktuell (nach Task 2 bereits um `ModelFile` erweitert):

```ts
import type { Folder, TagCount, CreatorCount, CloudAccount, ModelFile } from '../types';
```

Ersetzen durch:

```ts
import type { Folder, TagCount, CreatorCount, CloudAccount, ModelFile, SavedFilter } from '../types';
```

(Falls Task 2 noch nicht implementiert wurde, ist der Ausgangspunkt stattdessen `import type { Folder, TagCount, CreatorCount, CloudAccount } from '../types';` - in diesem Fall `ModelFile, SavedFilter` beide neu ergänzen.)

Das `Props`-Interface um vier Felder erweitern - nach `onCreatorSelect: (label: string | null) => void;` ergänzen:

```ts
  onCreatorSelect: (label: string | null) => void;
  savedFilters: SavedFilter[];
  onSaveFilter: (name: string) => void;
  onApplyFilter: (filter: SavedFilter) => void;
  onDeleteFilter: (id: string) => void;
```

Die Funktionssignatur entsprechend erweitern - nach `onCreatorSelect,` ergänzen:

```ts
  onCreatorSelect,
  savedFilters,
  onSaveFilter,
  onApplyFilter,
  onDeleteFilter,
```

Nach der bestehenden Zustandszeile `const [creatorsCollapsed, setCreatorsCollapsed] = useState(false);` zwei neue Zustände ergänzen:

```ts
  const [filtersCollapsed, setFiltersCollapsed] = useState(false);
  const [savingFilter, setSavingFilter] = useState(false);
  const [filterNameDraft, setFilterNameDraft] = useState('');
```

Direkt nach dem schließenden `))}` der Creators-Liste (vor dem schließenden `</div>` des scrollbaren mittleren Bereichs, also vor der Zeile `</div>` gefolgt von `<div className="flex-none border-t border-[var(--line)] px-3.5 pt-3 pb-3.5">`) die neue Sektion einfügen:

```tsx
        <div className="flex items-center justify-between px-1.5 pt-[18px] pb-2">
          <span className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('savedFiltersHeading')}
          </span>
          <div className="flex items-center gap-2">
            <span
              onClick={() => {
                setSavingFilter(true);
                setFilterNameDraft('');
              }}
              className="font-mono-ui text-sm leading-none text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]"
            >
              +
            </span>
            <span
              onClick={() => setFiltersCollapsed((c) => !c)}
              className="font-mono-ui text-[9px] leading-none text-[var(--ink-3)] cursor-pointer"
            >
              {filtersCollapsed ? '▾' : '▴'}
            </span>
          </div>
        </div>
        {!filtersCollapsed && savingFilter && (
          <div className="flex items-center gap-1.5 px-1.5 pb-2">
            <input
              value={filterNameDraft}
              onChange={(e) => setFilterNameDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  const name = filterNameDraft.trim();
                  if (name) onSaveFilter(name);
                  setSavingFilter(false);
                }
                if (e.key === 'Escape') setSavingFilter(false);
              }}
              autoFocus
              placeholder={t('savedFilterNamePlaceholder')}
              className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-[11.5px]"
            />
            <span
              onClick={() => {
                const name = filterNameDraft.trim();
                if (name) onSaveFilter(name);
                setSavingFilter(false);
              }}
              className="font-mono-ui text-[11px] text-[var(--accent)] cursor-pointer"
            >
              ✓
            </span>
          </div>
        )}
        {!filtersCollapsed && savedFilters.map((filter) => (
          <div
            key={filter.id}
            onClick={() => onApplyFilter(filter)}
            className="flex items-center gap-2 h-7 px-1.5 rounded-[3px] cursor-pointer text-[var(--ink-2)] hover:text-[var(--ink)]"
          >
            <span className="flex-1 text-xs overflow-hidden text-ellipsis whitespace-nowrap">
              {filter.name}
            </span>
            <span
              onClick={(e) => {
                e.stopPropagation();
                onDeleteFilter(filter.id);
              }}
              className="font-mono-ui text-[10px] text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]"
            >
              ✕
            </span>
          </div>
        ))}
```

- [ ] **Step 7: App.tsx - Zustand, Handler und Verdrahtung**

In `src/App.tsx` den Typ-Import erweitern. Aktuell:

```ts
import type { ModelFile, Folder, TagCount, CreatorCount, CloudAccount, Origin, ViewMode, SortKey } from './types';
```

Ersetzen durch:

```ts
import type { ModelFile, Folder, TagCount, CreatorCount, CloudAccount, Origin, ViewMode, SortKey, SavedFilter } from './types';
```

Nach der bestehenden Zustandszeile `const [importBanner, setImportBanner] = useState<{ imported: number; duplicates: number } | null>(null);` einen neuen Zustand ergänzen:

```ts
  const [importBanner, setImportBanner] = useState<{ imported: number; duplicates: number } | null>(null);
  const [savedFilters, setSavedFilters] = useState<SavedFilter[]>([]);
```

Die `refreshCreators`-Zeile um eine analoge Zeile für gespeicherte Filter ergänzen. Aktuell:

```ts
  const refreshCreators = () => invoke<CreatorCount[]>('list_creators').then(setCreators);
```

Ersetzen durch:

```ts
  const refreshCreators = () => invoke<CreatorCount[]>('list_creators').then(setCreators);
  const refreshSavedFilters = () => invoke<SavedFilter[]>('list_saved_filters').then(setSavedFilters);
```

Im ersten `useEffect` (Initial-Load) nach `refreshCreators();` einen Aufruf ergänzen:

```ts
    refreshFolders();
    refreshTags();
    refreshCreators();
    refreshSavedFilters();
    refreshClouds();
```

Vor dem `return (` (nach den in Task 2 ggf. ergänzten Warteschlangen-Handlern, bzw. direkt nach `const contextModel = ...` falls Task 2 noch nicht implementiert ist) drei neue Handler ergänzen:

```ts
  const saveCurrentFilter = (name: string) => {
    invoke<SavedFilter>('save_filter', {
      filter: {
        name,
        folderId: activeFolderId === 'all' ? null : activeFolderId,
        tag: activeTag,
        creator: activeCreator,
        query: query || null,
        sort,
      },
    })
      .then((saved) => setSavedFilters((prev) => [...prev, saved]))
      .catch((e) => console.error('[saved-filter] Speichern fehlgeschlagen:', e));
  };

  const applySavedFilter = (filter: SavedFilter) => {
    setActiveFolderId(filter.folderId ?? 'all');
    setActiveTag(filter.tag);
    setActiveCreator(filter.creator);
    setQuery(filter.query ?? '');
    setSort(filter.sort);
  };

  const deleteSavedFilter = (id: string) => {
    setSavedFilters((prev) => prev.filter((f) => f.id !== id));
    invoke('delete_saved_filter', { filterId: id }).catch((e) => {
      console.error('[saved-filter] Löschen fehlgeschlagen:', e);
    });
  };
```

Die `<Sidebar ... />`-Instanz um die neuen Props erweitern - nach `onCreatorSelect={setActiveCreator}` ergänzen:

```tsx
            onCreatorSelect={setActiveCreator}
            savedFilters={savedFilters}
            onSaveFilter={saveCurrentFilter}
            onApplyFilter={applySavedFilter}
            onDeleteFilter={deleteSavedFilter}
```

- [ ] **Step 8: Typecheck**

Run: `npx tsc --noEmit`
Expected: keine Fehler.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/types/index.ts src/App.tsx src/components/Sidebar.tsx src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "feat: gespeicherte Filterkombinationen"
```

---

## Task 4: Aufräum-Vorschläge (verwaiste Pfade + Bestands-Duplikate)

Vollständiger vertikaler Slice für den manuellen Katalog-Scan: ein neuer Command, der verwaiste Dateipfade (Datei existiert nicht mehr auf der Platte) und Duplikat-Gruppen im gesamten Bestand (gleicher `content_hash`, mehr als ein Eintrag) findet, ein Scan-Button im Einstellungen-Panel, und ein neuer Bereinigungs-Dialog mit Einzelauswahl. Braucht keine Schema-Änderung - nutzt ausschließlich die bereits vorhandenen Spalten `path` und `content_hash`. Unabhängig von Task 2 und Task 3.

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types/index.ts`
- Modify: `src/App.tsx`
- Modify: `src/components/Header.tsx`
- Create: `src/components/CatalogCleanupDialog.tsx`
- Modify: `src/i18n/types.ts`
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`

**Interfaces:**
- Consumes: `db::list_files`, `db::get_file`, `db::delete_file` (bereits vorhanden), `ModelFileDto`/`to_dto` (bereits vorhanden).
- Produces: Tauri-Commands `scan_catalog_issues() -> { orphaned: ModelFileDto[], duplicateGroups: ModelFileDto[][] }`, `delete_files(fileIds: string[]) -> void`.
- Produces: `CatalogCleanupDialog`-Komponente (`src/components/CatalogCleanupDialog.tsx`), Props `{ issues: CatalogIssues, onClose: () => void, onDelete: (fileIds: string[]) => void }`.

- [ ] **Step 1: scan_catalog_issues und delete_files in commands.rs**

In `src-tauri/src/commands.rs` am Ende der Datei, direkt vor dem `#[cfg(test)]`-Block, die neue DTO und die beiden neuen Commands ergänzen:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogIssuesDto {
    pub orphaned: Vec<ModelFileDto>,
    pub duplicate_groups: Vec<Vec<ModelFileDto>>,
}

#[tauri::command]
pub fn scan_catalog_issues(state: State<AppState>) -> CmdResult<CatalogIssuesDto> {
    let conn = lock_db(&state)?;
    let files = db::list_files(&conn).map_err(|e| e.to_string())?;

    let orphaned: Vec<ModelFileDto> = files
        .iter()
        .filter(|f| std::fs::metadata(&f.path).is_err())
        .cloned()
        .map(to_dto)
        .collect();

    let mut by_hash: BTreeMap<String, Vec<FileRecord>> = BTreeMap::new();
    for file in files {
        if let Some(hash) = file.content_hash.clone() {
            by_hash.entry(hash).or_default().push(file);
        }
    }

    let mut duplicate_groups: Vec<Vec<ModelFileDto>> = by_hash
        .into_values()
        .filter(|group| group.len() >= 2)
        .map(|mut group| {
            group.sort_by(|a, b| a.imported_at.cmp(&b.imported_at));
            group.into_iter().map(to_dto).collect()
        })
        .collect();
    duplicate_groups.sort_by(|a, b| a[0].imported_at.cmp(&b[0].imported_at));

    Ok(CatalogIssuesDto { orphaned, duplicate_groups })
}

#[tauri::command]
pub fn delete_files(state: State<AppState>, file_ids: Vec<String>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    for file_id in file_ids {
        let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
        // Bereinigungs-Batch: eine zwischenzeitlich bereits geloeschte Datei
        // (z.B. doppelt in der Auswahl) wird uebersprungen statt den ganzen
        // Batch abzubrechen.
        let Some(file) = db::get_file(&conn, id).map_err(|e| e.to_string())? else {
            continue;
        };
        if let Err(e) = std::fs::remove_file(&file.path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.to_string());
            }
        }
        db::delete_file(&conn, id).map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

`FileRecord` ist bereits über die bestehende Zeile `use crate::db::{self, models::FileRecord};` am Dateianfang importiert - keine Änderung am Import-Block nötig.

- [ ] **Step 2: Commands registrieren**

In `src-tauri/src/lib.rs`s `generate_handler!`-Liste nach `commands::open_in_slicer,` zwei neue Zeilen ergänzen:

```rust
            commands::open_in_slicer,
            commands::scan_catalog_issues,
            commands::delete_files,
            cloud::commands::connect_google_drive,
```

- [ ] **Step 3: Backend-Tests laufen lassen**

Run: `cd src-tauri && cargo test`
Expected: kompiliert und alle Tests grün (`scan_catalog_issues`/`delete_files` brauchen echten Dateisystemzugriff und werden - wie `import_one`/`import_many` - bewusst nur live verifiziert, nicht unit-getestet).

- [ ] **Step 4: Frontend-Typ ergänzen**

In `src/types/index.ts` nach der bestehenden `FilamentSpool`-Interface (am Dateiende) ein neues Interface ergänzen:

```ts
export interface CatalogIssues {
  orphaned: ModelFile[];
  duplicateGroups: ModelFile[][];
}
```

- [ ] **Step 5: i18n-Keys ergänzen**

In `src/i18n/types.ts` im `Translations`-Interface vor der schließenden `}` ergänzen:

```ts
  catalogCleanupTitle: string;
  catalogCleanupScanButton: string;
  catalogCleanupScanning: string;
  catalogCleanupError: string;
  cleanupDialogTitle: string;
  cleanupNoIssues: string;
  cleanupOrphanedHeading: string;
  cleanupDuplicateGroupHeading: string;
  cleanupKeepOldest: string;
  cleanupDeleteSelected: string;
}
```

In `src/i18n/de.ts` vor der schließenden `};` ergänzen:

```ts
  catalogCleanupTitle: 'Katalog prüfen',
  catalogCleanupScanButton: 'Katalog auf Probleme prüfen',
  catalogCleanupScanning: 'Prüfe…',
  catalogCleanupError: 'Prüfung fehlgeschlagen:',
  cleanupDialogTitle: 'Aufräum-Vorschläge',
  cleanupNoIssues: 'Keine Probleme gefunden.',
  cleanupOrphanedHeading: 'Verwaiste Einträge',
  cleanupDuplicateGroupHeading: 'Duplikat-Gruppe',
  cleanupKeepOldest: 'wird behalten',
  cleanupDeleteSelected: 'Auswahl löschen',
};
```

In `src/i18n/en.ts` analog:

```ts
  catalogCleanupTitle: 'Check catalog',
  catalogCleanupScanButton: 'Scan catalog for issues',
  catalogCleanupScanning: 'Scanning…',
  catalogCleanupError: 'Scan failed:',
  cleanupDialogTitle: 'Cleanup suggestions',
  cleanupNoIssues: 'No issues found.',
  cleanupOrphanedHeading: 'Orphaned entries',
  cleanupDuplicateGroupHeading: 'Duplicate group',
  cleanupKeepOldest: 'kept',
  cleanupDeleteSelected: 'Delete selection',
};
```

In `src/i18n/es.ts` analog:

```ts
  catalogCleanupTitle: 'Revisar catálogo',
  catalogCleanupScanButton: 'Buscar problemas en el catálogo',
  catalogCleanupScanning: 'Buscando…',
  catalogCleanupError: 'Error al revisar:',
  cleanupDialogTitle: 'Sugerencias de limpieza',
  cleanupNoIssues: 'No se encontraron problemas.',
  cleanupOrphanedHeading: 'Entradas huérfanas',
  cleanupDuplicateGroupHeading: 'Grupo de duplicados',
  cleanupKeepOldest: 'se conserva',
  cleanupDeleteSelected: 'Eliminar selección',
};
```

In `src/i18n/fr.ts` analog:

```ts
  catalogCleanupTitle: 'Vérifier le catalogue',
  catalogCleanupScanButton: 'Rechercher des problèmes',
  catalogCleanupScanning: 'Analyse…',
  catalogCleanupError: "Échec de l'analyse :",
  cleanupDialogTitle: 'Suggestions de nettoyage',
  cleanupNoIssues: 'Aucun problème trouvé.',
  cleanupOrphanedHeading: 'Entrées orphelines',
  cleanupDuplicateGroupHeading: 'Groupe de doublons',
  cleanupKeepOldest: 'conservé',
  cleanupDeleteSelected: 'Supprimer la sélection',
};
```

- [ ] **Step 6: CatalogCleanupDialog-Komponente erstellen**

Neue Datei `src/components/CatalogCleanupDialog.tsx` anlegen:

```tsx
import { useState } from 'react';
import type { CatalogIssues } from '../types';
import { useT } from '../i18n/LanguageContext';

interface Props {
  issues: CatalogIssues;
  onClose: () => void;
  onDelete: (fileIds: string[]) => void;
}

function initialSelection(issues: CatalogIssues): Set<string> {
  const checked = new Set<string>();
  issues.orphaned.forEach((m) => checked.add(m.id));
  issues.duplicateGroups.forEach((group) => {
    group.slice(1).forEach((m) => checked.add(m.id));
  });
  return checked;
}

export function CatalogCleanupDialog({ issues, onClose, onDelete }: Props) {
  const t = useT();
  const [checked, setChecked] = useState<Set<string>>(() => initialSelection(issues));

  const toggle = (id: string) => {
    setChecked((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const hasIssues = issues.orphaned.length > 0 || issues.duplicateGroups.length > 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-[480px] max-h-[80vh] flex flex-col bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)]">
        <div className="flex-none px-4 py-3 border-b border-[var(--line)] flex items-center justify-between">
          <span className="text-[14px] font-semibold">{t('cleanupDialogTitle')}</span>
          <span onClick={onClose} className="cursor-pointer text-[var(--ink-3)] hover:text-[var(--accent)]">
            ✕
          </span>
        </div>

        <div className="flex-1 overflow-y-auto px-4 py-3">
          {!hasIssues && <div className="text-[12.5px] text-[var(--ink-3)]">{t('cleanupNoIssues')}</div>}

          {issues.orphaned.length > 0 && (
            <div className="pb-4">
              <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2">
                {t('cleanupOrphanedHeading')}
              </div>
              {issues.orphaned.map((model) => (
                <label key={model.id} className="flex items-center gap-2 py-1 text-[12.5px] cursor-pointer">
                  <input type="checkbox" checked={checked.has(model.id)} onChange={() => toggle(model.id)} />
                  <span className="flex-1 truncate">{model.name}</span>
                  <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)] truncate max-w-[160px]">
                    {model.path}
                  </span>
                </label>
              ))}
            </div>
          )}

          {issues.duplicateGroups.map((group, groupIndex) => (
            <div key={groupIndex} className="pb-4">
              <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2">
                {t('cleanupDuplicateGroupHeading')} {groupIndex + 1}
              </div>
              {group.map((model, index) => (
                <label
                  key={model.id}
                  className={`flex items-center gap-2 py-1 text-[12.5px] ${index === 0 ? 'opacity-60' : 'cursor-pointer'}`}
                >
                  <input
                    type="checkbox"
                    checked={checked.has(model.id)}
                    disabled={index === 0}
                    onChange={() => toggle(model.id)}
                  />
                  <span className="flex-1 truncate">{model.name}</span>
                  {index === 0 && (
                    <span className="font-mono-ui text-[10px] text-[var(--ink-3)]">{t('cleanupKeepOldest')}</span>
                  )}
                </label>
              ))}
            </div>
          ))}
        </div>

        <div className="flex-none px-4 py-3 border-t border-[var(--line)] flex justify-end gap-2">
          <button
            onClick={onClose}
            className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('cancel')}
          </button>
          <button
            onClick={() => onDelete(Array.from(checked))}
            disabled={checked.size === 0}
            className={`h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold ${
              checked.size === 0 ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer'
            }`}
          >
            {t('cleanupDeleteSelected')}
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 7: Header.tsx - "Katalog prüfen"-Sektion im Einstellungen-Panel**

In `src/components/Header.tsx` das `Props`-Interface um zwei Felder erweitern - nach `onRemoveSlicer: (id: string) => void;` ergänzen:

```ts
  onRemoveSlicer: (id: string) => void;
  onScanCatalogIssues: () => void;
  cleanupScanning: boolean;
  cleanupError: string | null;
```

Die Funktionssignatur entsprechend erweitern - nach `onRemoveSlicer,` ergänzen:

```ts
  onRemoveSlicer,
  onScanCatalogIssues,
  cleanupScanning,
  cleanupError,
```

Nach dem bestehenden Slicer-Abschnitt (der mit dem `pendingSlicerPath ? (...) : (...)`-Block endet, direkt vor dem schließenden `</div>` des Einstellungen-Panels) eine neue Sektion einfügen. Der Slicer-Abschnitt endet aktuell mit:

```tsx
            ) : (
              <button
                onClick={handlePickSlicer}
                className="mt-2 h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                + {t('addSlicer')}
              </button>
            )}
          </div>
        )}
      </div>
    </header>
```

Ersetzen durch:

```tsx
            ) : (
              <button
                onClick={handlePickSlicer}
                className="mt-2 h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                + {t('addSlicer')}
              </button>
            )}

            <div className="text-[13px] font-semibold mt-4 mb-2">{t('catalogCleanupTitle')}</div>
            <button
              onClick={onScanCatalogIssues}
              disabled={cleanupScanning}
              className={`h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] ${
                cleanupScanning
                  ? 'opacity-40 cursor-not-allowed'
                  : 'cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]'
              }`}
            >
              {cleanupScanning ? t('catalogCleanupScanning') : t('catalogCleanupScanButton')}
            </button>
            {cleanupError && (
              <div className="mt-1.5 font-mono-ui text-[10px] text-[var(--accent)] break-words">
                {t('catalogCleanupError')} {cleanupError}
              </div>
            )}
          </div>
        )}
      </div>
    </header>
```

- [ ] **Step 8: App.tsx - Zustand, Handler und Verdrahtung**

In `src/App.tsx` den Typ-Import erweitern (fügt `CatalogIssues` hinzu, zusätzlich zu allen bereits in Task 2/3 ergänzten Typen). Aktuell (Ausgangszustand vor jeder der drei Feature-Tasks):

```ts
import type { ModelFile, Folder, TagCount, CreatorCount, CloudAccount, Origin, ViewMode, SortKey } from './types';
```

Ersetzen durch (unter Beibehaltung bereits von Task 2/3 ergänzter Typen wie `SavedFilter`):

```ts
import type { ModelFile, Folder, TagCount, CreatorCount, CloudAccount, Origin, ViewMode, SortKey, CatalogIssues } from './types';
```

Den Komponenten-Import erweitern - nach `import { ImportSummaryBanner } from './components/ImportSummaryBanner';` ergänzen:

```ts
import { ImportSummaryBanner } from './components/ImportSummaryBanner';
import { CatalogCleanupDialog } from './components/CatalogCleanupDialog';
```

Nach der bestehenden Zustandszeile `const [importBanner, setImportBanner] = useState<{ imported: number; duplicates: number } | null>(null);` (bzw. direkt danach, unabhängig davon ob Task 3 dort bereits `savedFilters` ergänzt hat) vier neue Zustände ergänzen:

```ts
  const [cleanupDialogOpen, setCleanupDialogOpen] = useState(false);
  const [cleanupIssues, setCleanupIssues] = useState<CatalogIssues | null>(null);
  const [cleanupScanning, setCleanupScanning] = useState(false);
  const [cleanupError, setCleanupError] = useState<string | null>(null);
```

Vor dem `return (` der Komponente zwei neue Handler ergänzen:

```ts
  const scanCatalogIssues = () => {
    setCleanupScanning(true);
    setCleanupError(null);
    invoke<CatalogIssues>('scan_catalog_issues')
      .then((issues) => {
        setCleanupIssues(issues);
        setCleanupDialogOpen(true);
      })
      .catch((e) => {
        console.error('[cleanup] Scan fehlgeschlagen:', e);
        setCleanupError(String(e));
      })
      .finally(() => setCleanupScanning(false));
  };

  const deleteSelectedCleanupFiles = (fileIds: string[]) => {
    invoke('delete_files', { fileIds })
      .then(() => {
        setModels((prev) => prev.filter((m) => !fileIds.includes(m.id)));
        setSelectedId((prev) => (prev && fileIds.includes(prev) ? null : prev));
        setCleanupDialogOpen(false);
        setCleanupIssues(null);
        refreshFolders();
        refreshTags();
        refreshCreators();
      })
      .catch((e) => {
        console.error('[cleanup] Löschen fehlgeschlagen:', e);
        setCleanupError(String(e));
      });
  };
```

Die `<Header ... />`-Instanz um die neuen Props erweitern - nach `onRemoveSlicer={removeSlicer}` ergänzen:

```tsx
        onRemoveSlicer={removeSlicer}
        onScanCatalogIssues={scanCatalogIssues}
        cleanupScanning={cleanupScanning}
        cleanupError={cleanupError}
```

Nach dem bestehenden `{importBanner && (...)}`-Block (am Ende der Komponente, vor dem schließenden `</div>` des äußersten Containers) den neuen Dialog ergänzen:

```tsx
      {importBanner && (
        <ImportSummaryBanner
          imported={importBanner.imported}
          duplicates={importBanner.duplicates}
          onClose={() => setImportBanner(null)}
        />
      )}

      {cleanupDialogOpen && cleanupIssues && (
        <CatalogCleanupDialog
          issues={cleanupIssues}
          onClose={() => setCleanupDialogOpen(false)}
          onDelete={deleteSelectedCleanupFiles}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 9: Typecheck**

Run: `npx tsc --noEmit`
Expected: keine Fehler.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/types/index.ts src/App.tsx src/components/Header.tsx src/components/CatalogCleanupDialog.tsx src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "feat: Aufräum-Vorschläge für verwaiste Pfade und Bestands-Duplikate"
```
