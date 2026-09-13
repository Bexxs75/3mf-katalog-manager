# Echte Ordnerstruktur Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** "Ordner" wird von totem UI zu einem echten Feature: Import einer Verzeichnisstruktur legt eine passende Ordner-Hierarchie in der DB an, die Sidebar zeigt einen auf-/zuklappbaren Baum, und Dateien/Ordner lassen sich per Drag&Drop physisch verschieben (Datei auf der Platte UND DB bleiben konsistent).

**Architektur:** Rust-Backend bekommt eine `parent_id`/`path`-Erweiterung der `folders`-Tabelle, neue Helper (`ensure_folder_path`, `update_paths_under_folder`) und vier neue Tauri-Commands (`create_folder`, `rename_folder`, `move_folder`, `move_file_to_folder`). Frontend bekommt eine neue `FolderTree.tsx`-Komponente (flache `FolderDto[]`-Liste → Baum) mit maus-basiertem Drag&Drop (kein natives HTML5-DnD, siehe Global Constraints).

**Tech Stack:** Rust (rusqlite), React 19, TypeScript, Tauri v2 Commands.

## Abweichung vom Spec (`docs/superpowers/specs/2026-09-13-real-folder-tree-design.md`)

Der Spec erwähnt ein `app_settings`-basiertes "Katalog-Basisverzeichnis"
für `create_folder` ohne `parent_id`. Beim Grounding dieses Plans wurde
keine wiederverwendbare `app_settings`-Infrastruktur für einen solchen
Pfad gefunden. Dieser Plan verzichtet bewusst darauf: `create_folder` ohne
`parent_id` öffnet stattdessen denselben nativen Verzeichnis-Dialog wie
`import_folder` (`app.dialog().file().blocking_pick_folder()`), um den
Ort für den neuen Ordner zu wählen — ein Konzept weniger, keine neue
Einstellung nötig. Ebenso baut `ensure_folder_path` die Hierarchie ab dem
JEWEILS importierten Wurzelordner auf (dieser wird selbst zum obersten
Baum-Knoten, `parent_id: NULL`) statt einem globalen Basisverzeichnis zu
folgen — mehrere unabhängig importierte Ordner erscheinen dadurch als
mehrere Wurzel-Knoten nebeneinander, was der Sidebar-Baum ohnehin
unterstützen muss (flache Liste, kein Zwang zu genau einer Wurzel).

## Global Constraints

- Migration folgt dem bestehenden Projektmuster: additive `ALTER TABLE` in
  `src-tauri/src/db/repository.rs::init()`, Fehler bei bereits
  vorhandener Spalte via `let _ = conn.execute(...)` verschluckt (siehe
  Zeilen 27-49 als Referenz). KEIN Migrations-Framework einführen.
- Physisches Verschieben von Dateien nutzt die bestehende Helper-Funktion
  `move_file()` (`commands.rs:628`, hat bereits `CrossesDevices`-Fallback)
  — nicht neu erfinden.
- **Frontend-Drag&Drop MUSS dem bestehenden Maus-Event-Muster folgen**
  (`onMouseDown`/`onMouseEnter`/globaler `mouseup`-Listener), siehe
  `Sidebar.tsx:58-84` (Warteschlangen-Reorder) und `ModelGrid.tsx:33-73`
  (Karten-Reorder) als Referenzimplementierung. Natives HTML5
  `draggable`/`dragstart`/`dragover`/`drop` funktioniert unter
  Tauri/WebKitGTK NICHT zuverlässig (wird von `dragDropEnabled` auf
  Fensterebene abgefangen, das für OS-Datei-Drop-Import gebraucht wird).
  Der HTML-Mockup (`docs/superpowers/mockups/2026-09-13-gui-redesign.html`)
  nutzt natives DnD, weil er im Browser läuft — das ist NICHT 1:1
  übertragbar, nur das visuelle Ergebnis (Baum-Optik, Tooltip, Toast) ist
  Vorlage.
- `cargo test` (aus `src-tauri/`) und `npx tsc --noEmit` müssen nach jedem
  Task sauber durchlaufen.
- Kein AppImage-Rebuild/Deploy als Teil dieses Plans.
- Dieser Plan setzt NICHT voraus, dass `2026-09-13-gui-shell-redesign.md`
  bereits gemerged ist — beide Pläne ändern überschneidungsfreie Dateien
  (jener nur `Header.tsx`/`App.tsx`-Layout/neue `Rail.tsx`; dieser hier
  `Sidebar.tsx`-Ordner-Abschnitt/Backend). Reihenfolge des Merges ist
  daher frei, sollte aber nacheinander erfolgen (nicht parallel mergen
  ohne Rebase), da beide `App.tsx` anfassen (Rail-Props vs.
  Folder-Handler-Props).

---

### Task 1: DB-Schema erweitern, `list_folders` liefert flache Hierarchie

**Files:**
- Modify: `src-tauri/src/db/schema.sql` (Kommentar bei `folders`-Tabelle,
  keine Struktur-Änderung dort selbst — neue Spalten kommen additiv über
  `ALTER TABLE`, wie bei `filament_spools`/`files` üblich)
- Modify: `src-tauri/src/db/repository.rs` (`init()`, `list_folders()`)
- Modify: `src-tauri/src/db/models.rs` (`FolderRecord`)
- Modify: `src-tauri/src/commands.rs` (`FolderDto`, `list_folders`-Command)
- Test: `src-tauri/src/db/repository.rs` (inline `#[cfg(test)]`-Modul, wie
  im restlichen File üblich)

**Interfaces:**
- Produces: `FolderRecord { id: i64, name: String, parent_id: Option<i64>, path: String }`,
  `FolderDto { id: String, name: String, path: String, parent_id: Option<String>, count: i64 }`
  (camelCase im JSON über `#[serde(rename_all = "camelCase")]`, bereits
  Projektkonvention). Spätere Tasks (2-7) nutzen beide Typen unverändert.

- [ ] **Step 1: Migration in `db/repository.rs::init()` ergänzen**

Direkt nach dem bestehenden Block `let _ = conn.execute("ALTER TABLE files ADD COLUMN creator TEXT", []);` (siehe Zeile ~44) einfügen:

```rust
    let _ = conn.execute(
        "ALTER TABLE folders ADD COLUMN parent_id INTEGER REFERENCES folders(id) ON DELETE CASCADE",
        [],
    );
    let _ = conn.execute("ALTER TABLE folders ADD COLUMN path TEXT", []);
    let _ = conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_folders_path ON folders (path)",
        [],
    );
```

- [ ] **Step 2: `FolderRecord` erweitern**

In `db/models.rs`:
```rust
#[derive(Debug, Clone)]
pub struct FolderRecord {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub path: String,
}
```

- [ ] **Step 3: `list_folders()` in `db/repository.rs` anpassen**

```rust
pub fn list_folders(conn: &Connection) -> Result<Vec<FolderRecord>, DbError> {
    let mut stmt = conn.prepare("SELECT id, name, parent_id, path FROM folders ORDER BY name")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(FolderRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                path: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
```

- [ ] **Step 4: `FolderDto` und `list_folders`-Command in `commands.rs` anpassen**

`FolderDto` (aktuell Zeile 180-184):
```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub parent_id: Option<String>,
    pub count: i64,
}
```

`list_folders`-Command (aktuell Zeile 351-374) — die bisherige
synthetische `"all"`-Zeile mit hartkodiertem deutschem Namen
`"Alle Modelle"` entfällt (Bug: war nicht i18n-fähig); das Frontend
rendert "Alle Modelle" künftig selbst über `t()` (siehe Task 6). `count`
ist rekursiv (Ordner + alle Nachfahren):

```rust
#[tauri::command]
pub fn list_folders(state: State<AppState>) -> CmdResult<Vec<FolderDto>> {
    let conn = lock_db(&state)?;
    let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
    let files = db::list_files(&conn).map_err(|e| e.to_string())?;

    fn is_descendant_or_self(folders: &[db::FolderRecord], candidate_id: i64, ancestor_id: i64) -> bool {
        if candidate_id == ancestor_id {
            return true;
        }
        let mut current = candidate_id;
        while let Some(f) = folders.iter().find(|f| f.id == current) {
            match f.parent_id {
                Some(pid) if pid == ancestor_id => return true,
                Some(pid) => current = pid,
                None => return false,
            }
        }
        false
    }

    let dtos = folders
        .iter()
        .map(|folder| {
            let count = files
                .iter()
                .filter(|f| f.folder_id.is_some_and(|fid| is_descendant_or_self(&folders, fid, folder.id)))
                .count() as i64;
            FolderDto {
                id: folder.id.to_string(),
                name: folder.name.clone(),
                path: folder.path.clone(),
                parent_id: folder.parent_id.map(|id| id.to_string()),
                count,
            }
        })
        .collect();

    Ok(dtos)
}
```

- [ ] **Step 5: Rust-Unit-Test für die neue `list_folders`-Form**

In `db/repository.rs`, im bestehenden `#[cfg(test)] mod tests`-Block
(nach vorhandenem Muster: `connect_in_memory()`, `insert_folder` o.ä.
verwenden — falls `insert_folder` noch die alte 1-Parameter-Signatur hat,
wird sie erst in Task 2 erweitert; für diesen Test reicht ein direktes
`conn.execute("INSERT INTO folders (name, path) VALUES ('Test', '/tmp/Test')", [])`):

```rust
#[test]
fn list_folders_returns_path_and_parent_id() {
    let conn = connect_in_memory().unwrap();
    conn.execute("INSERT INTO folders (name, path) VALUES ('Root', '/tmp/Root')", []).unwrap();
    let root_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO folders (name, path, parent_id) VALUES ('Child', '/tmp/Root/Child', ?1)",
        params![root_id],
    ).unwrap();

    let folders = list_folders(&conn).unwrap();
    assert_eq!(folders.len(), 2);
    let child = folders.iter().find(|f| f.name == "Child").unwrap();
    assert_eq!(child.parent_id, Some(root_id));
    assert_eq!(child.path, "/tmp/Root/Child");
}
```

- [ ] **Step 6: Tests laufen lassen**

```bash
cd src-tauri && cargo test list_folders
```
Erwartet: neuer Test grün, keine bestehenden Tests gebrochen (falls doch,
prüfen ob ein anderer Test `FolderDto`/`FolderRecord` mit der alten
2-Feld-Form konstruiert — dann dort ebenfalls `path`/`parent_id`
ergänzen).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/repository.rs src-tauri/src/db/models.rs src-tauri/src/commands.rs
git commit -m "Backend: folders-Tabelle um parent_id/path erweitert, list_folders liefert flache Hierarchie"
```

---

### Task 2: `ensure_folder_path` — Ordner-Hierarchie beim Import anlegen

**Files:**
- Modify: `src-tauri/src/db/repository.rs` (neue Funktion `ensure_folder_path`, neue Funktion `insert_folder_with_parent`)
- Test: `src-tauri/src/db/repository.rs`

**Interfaces:**
- Consumes: `FolderRecord` aus Task 1.
- Produces: `pub fn ensure_folder_path(conn: &Connection, import_root: &Path, dir: &Path) -> Result<Option<i64>, DbError>`
  — wird in Task 3 von `import_many`/`import_one` konsumiert. Gibt `Ok(None)`
  zurück, wenn `dir == import_root` (Datei liegt direkt im importierten
  Wurzelordner, kein Unterordner-Eintrag nötig außer der Wurzel selbst,
  die trotzdem angelegt wird) — Achtung: das bedeutet, die Funktion
  legt IMMER mindestens den `import_root`-Knoten an und gibt dessen id
  zurück, außer `dir` läge SELBST oberhalb/außerhalb von `import_root`
  (Fehlerfall, siehe Step 1).

- [ ] **Step 1: `ensure_folder_path` implementieren**

```rust
use std::path::{Component, Path};

/// Legt fuer jede Verzeichnisebene zwischen `import_root` (inklusive) und
/// `dir` (inklusive) einen folders-Eintrag an, sofern er noch nicht
/// existiert (Lookup per `path`-Spalte, idempotent bei wiederholtem
/// Import desselben Baums). Gibt die id der tiefsten Ebene (= `dir`)
/// zurueck.
pub fn ensure_folder_path(conn: &Connection, import_root: &Path, dir: &Path) -> Result<i64, DbError> {
    let relative = dir.strip_prefix(import_root).map_err(|_| {
        DbError::Other(format!(
            "{} liegt nicht unter {}",
            dir.display(),
            import_root.display()
        ))
    })?;

    let mut current_path = import_root.to_path_buf();
    let mut parent_id: Option<i64> = None;
    parent_id = Some(find_or_insert_folder(conn, &current_path, parent_id, folder_name(&current_path))?);

    for component in relative.components() {
        if let Component::Normal(part) = component {
            current_path.push(part);
            parent_id = Some(find_or_insert_folder(
                conn,
                &current_path,
                parent_id,
                part.to_string_lossy().to_string(),
            )?);
        }
    }

    Ok(parent_id.expect("mindestens import_root wurde oben eingefuegt"))
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn find_or_insert_folder(
    conn: &Connection,
    path: &Path,
    parent_id: Option<i64>,
    name: String,
) -> Result<i64, DbError> {
    let path_str = path.to_string_lossy().to_string();
    if let Some(id) = conn
        .query_row("SELECT id FROM folders WHERE path = ?1", params![path_str], |r| r.get(0))
        .optional()?
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO folders (name, parent_id, path) VALUES (?1, ?2, ?3)",
        params![name, parent_id, path_str],
    )?;
    Ok(conn.last_insert_rowid())
}
```

`DbError::Other(String)` — falls diese Variante im bestehenden
`DbError`-Enum (`src-tauri/src/db/error.rs`) noch nicht existiert, dort
ergänzen (`#[error("{0}")] Other(String)` nach `thiserror`-Konvention,
Datei vor dem Ergänzen lesen und dem bestehenden Stil folgen).

- [ ] **Step 2: Tests**

```rust
#[test]
fn ensure_folder_path_creates_missing_levels() {
    let conn = connect_in_memory().unwrap();
    let root = Path::new("/tmp/Tabletop");
    let dir = Path::new("/tmp/Tabletop/Reaper");

    let leaf_id = ensure_folder_path(&conn, root, dir).unwrap();
    let folders = list_folders(&conn).unwrap();
    assert_eq!(folders.len(), 2);
    let leaf = folders.iter().find(|f| f.id == leaf_id).unwrap();
    assert_eq!(leaf.name, "Reaper");
    assert_eq!(leaf.path, "/tmp/Tabletop/Reaper");
    let parent = folders.iter().find(|f| f.id == leaf.parent_id.unwrap()).unwrap();
    assert_eq!(parent.name, "Tabletop");
    assert_eq!(parent.parent_id, None);
}

#[test]
fn ensure_folder_path_is_idempotent() {
    let conn = connect_in_memory().unwrap();
    let root = Path::new("/tmp/Tabletop");
    let dir = Path::new("/tmp/Tabletop/Reaper");

    let first = ensure_folder_path(&conn, root, dir).unwrap();
    let second = ensure_folder_path(&conn, root, dir).unwrap();
    assert_eq!(first, second);
    assert_eq!(list_folders(&conn).unwrap().len(), 2);
}

#[test]
fn ensure_folder_path_file_directly_in_root() {
    let conn = connect_in_memory().unwrap();
    let root = Path::new("/tmp/Tabletop");

    let id = ensure_folder_path(&conn, root, root).unwrap();
    let folders = list_folders(&conn).unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].id, id);
    assert_eq!(folders[0].name, "Tabletop");
}
```

- [ ] **Step 3: Tests laufen lassen**

```bash
cd src-tauri && cargo test ensure_folder_path
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/db/repository.rs src-tauri/src/db/error.rs
git commit -m "Backend: ensure_folder_path legt Ordner-Hierarchie beim Import rekursiv an"
```

---

### Task 3: Import setzt echte `folder_id`

**Files:**
- Modify: `src-tauri/src/commands.rs` (`import_one`, `import_many`, alle Aufrufer)

**Interfaces:**
- Consumes: `db::ensure_folder_path` aus Task 2.
- Produces: `import_one(conn, path, display_name, content_hash, folder_id: Option<i64>) -> CmdResult<ModelFileDto>`
  — Signatur-Änderung betrifft ALLE bestehenden Aufrufer (siehe Step 2).

- [ ] **Step 1: `import_one` um `folder_id`-Parameter erweitern**

In `import_one()` (`commands.rs:921`) die Signatur um `folder_id:
Option<i64>` ergänzen und in der `NewFile`-Konstruktion (aktuell
`folder_id: None` fest verdrahtet, siehe die drei Fundstellen aus der
Recherche: `commands.rs:998`, und die beiden Test-Fixtures
`commands.rs:2583`/`commands.rs:2856`/`commands.rs:2897`) durch den
übergebenen Parameter ersetzen — NUR bei der produktiven Konstruktion in
`import_one` selbst (Zeile ~998), NICHT in den Test-Fixtures (die bleiben
bei `folder_id: None`, sofern der jeweilige Test keinen Ordner-Kontext
prüft).

- [ ] **Step 2: `import_many` berechnet `folder_id` pro Wurzel**

Ersetze die Flatten-Logik in `import_many()` (`commands.rs:1110-1163`):
statt alle `roots` vorab in eine gemeinsame `candidates`-Liste zu
flatten, wird pro `root` einzeln gewalkt, damit bekannt bleibt, ob dieser
Kandidat aus einem Ordner-Import stammt:

```rust
fn import_many(state: &State<AppState>, roots: Vec<PathBuf>) -> CmdResult<ImportResultDto> {
    let mut conn = lock_db(state)?;
    let mut seen = HashSet::new();
    let mut imported = Vec::new();
    let mut duplicate_count = 0i64;

    for root in roots {
        let is_folder_root = root.is_dir();
        let mut candidates = Vec::new();
        collect_supported_files(&root, &mut candidates);

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

            let folder_id = if is_folder_root {
                path.parent().and_then(|dir| db::ensure_folder_path(&conn, &root, dir).ok())
            } else {
                None
            };

            match import_one(&mut conn, &path, None, Some(content_hash), folder_id) {
                Ok(dto) => imported.push(dto),
                Err(e) if e.contains("UNIQUE constraint failed") => {
                    duplicate_count += 1;
                }
                Err(e) => eprintln!("[import] Import fehlgeschlagen für {path_str}: {e}"),
            }
        }
    }

    Ok(ImportResultDto { imported, duplicate_count })
}
```

Hinweis: `is_folder_root` ist `false` für die einzelnen Datei-Pfade, die
`import_files` (`commands.rs:1173` ff., Datei-Auswahldialog) übergibt —
jeder `root` dort IST bereits eine einzelne Datei, kein Verzeichnis, also
automatisch `folder_id: None` ohne Sonderfall-Code. `import_folder` und
`import_dropped` übergeben tatsächliche Verzeichnisse (bzw. bei
`import_dropped` potenziell gemischt Dateien+Ordner, die dann korrekt
gemischt behandelt werden).

- [ ] **Step 3: `import_folder_as_collection` unverändert lassen**

Ruft ebenfalls `import_many` auf (`commands.rs:1225`) — durch obige
Änderung bekommen die dort importierten Dateien jetzt ZUSÄTZLICH einen
echten `folder_id` (vorher immer `None`). Das ist gewünscht (Datei
gehört sowohl zu einer Sammlung als auch zu ihrem echten Ordner) und
braucht keine Code-Änderung an dieser Funktion selbst.

- [ ] **Step 4: Bestehende Rust-Tests anpassen**

```bash
cd src-tauri && cargo build --tests 2>&1 | grep "error\[" 
```
Jeden Kompilierfehler durch die geänderte `import_one`-Signatur beheben
(fehlenden `folder_id`-Parameter an den jeweiligen Testaufrufen mit
`None` ergänzen, außer der Test prüft explizit Ordner-Zuordnung — dann
mit sinnvollem `Some(id)` aus einem vorher angelegten Test-Ordner).

- [ ] **Step 5: Neuer Test: Ordner-Import setzt folder_id**

```rust
#[test]
fn import_many_assigns_folder_id_for_folder_roots() {
    let tmp = tempfile::tempdir().unwrap();
    let sub = tmp.path().join("Tabletop");
    std::fs::create_dir(&sub).unwrap();
    let file_path = sub.join("model.3mf");
    write_minimal_3mf(&file_path); // vorhandener Test-Helper in commands.rs, siehe bestehende Tests in diesem Modul

    let state = test_app_state(); // vorhandener Test-Helper, siehe bestehende Tests in diesem Modul
    import_many(&state_ref(&state), vec![tmp.path().to_path_buf()]).unwrap();

    let conn = state.db.lock().unwrap();
    let files = db::list_files(&conn).unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].folder_id.is_some());
}
```

Falls `write_minimal_3mf`/`test_app_state`/`state_ref` unter anderen
Namen existieren, die tatsächlichen Helfer aus den bestehenden Tests
direkt oberhalb/unterhalb von `import_one_stores_slice_info_json_when_present`
(`commands.rs:2675`) übernehmen statt neue zu erfinden.

- [ ] **Step 6: Alle Tests grün**

```bash
cd src-tauri && cargo test
```

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "Backend: Ordner-Import setzt echte folder_id ueber ensure_folder_path"
```

---

### Task 4: `create_folder` und `move_file_to_folder`

**Files:**
- Modify: `src-tauri/src/commands.rs` (zwei neue `#[tauri::command]`-Funktionen)
- Modify: `src-tauri/src/db/repository.rs` (`pub fn update_file_folder`)
- Modify: `src-tauri/src-tauri.conf.json` NICHT nötig (keine neue Capability, Dialog-API bereits genutzt)

**Interfaces:**
- Consumes: `move_file()` (`commands.rs:628`), `db::ensure_folder_path`/`find_or_insert_folder` (Task 2).
- Produces: `#[tauri::command] pub async fn create_folder(app, state, parent_id: Option<String>, name: String) -> CmdResult<FolderDto>`,
  `#[tauri::command] pub fn move_file_to_folder(state, file_id: String, folder_id: Option<String>) -> CmdResult<()>`
  — von Task 7 (Frontend-DnD) konsumiert.

- [ ] **Step 1: `db::update_file_folder` ergänzen**

In `db/repository.rs`:
```rust
pub fn update_file_folder(conn: &Connection, file_id: i64, folder_id: Option<i64>, path: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET folder_id = ?1, path = ?2 WHERE id = ?3",
        params![folder_id, path, file_id],
    )?;
    Ok(())
}
```

- [ ] **Step 2: `move_file_to_folder`-Command**

```rust
#[tauri::command]
pub fn move_file_to_folder(state: State<AppState>, file_id: String, folder_id: Option<String>) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let target_id: Option<i64> = folder_id
        .map(|s| s.parse::<i64>().map_err(|_| "invalid folder id".to_string()))
        .transpose()?;

    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id).map_err(|e| e.to_string())?.ok_or_else(|| "file not found".to_string())?;

    let target_dir: std::path::PathBuf = match target_id {
        Some(fid) => {
            let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
            let folder = folders.iter().find(|f| f.id == fid).ok_or_else(|| "folder not found".to_string())?;
            std::path::PathBuf::from(&folder.path)
        }
        None => std::path::Path::new(&file.path)
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| "invalid current path".to_string())?,
    };

    let new_path = target_dir.join(&file.name);
    move_file(std::path::Path::new(&file.path), &new_path).map_err(|e| e.to_string())?;

    db::update_file_folder(&conn, id, target_id, &new_path.to_string_lossy())
        .map_err(|e| e.to_string())
}
```

Hinweis `folder_id: None`-Fall (Datei "zurück an die Wurzel"): da es
keinen globalen Basisordner gibt (siehe "Abweichung vom Spec" oben), wird
dieser Fall im ersten Wurf NICHT über die Sidebar angeboten (Task 7
bietet nur echte Ordner-Zeilen als Drop-Ziel, keine "Alle
Modelle"-Zeile) — der `folder_id: None`-Zweig hier bleibt trotzdem
implementiert (verschiebt die Datei einfach nicht vom Fleck, `target_dir`
= aktuelles Verzeichnis), für zukünftige Erweiterung ohne API-Bruch.

- [ ] **Step 3: `create_folder`-Command**

```rust
#[tauri::command]
pub async fn create_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    parent_id: Option<String>,
    name: String,
) -> CmdResult<FolderDto> {
    let parent: Option<i64> = parent_id
        .map(|s| s.parse::<i64>().map_err(|_| "invalid folder id".to_string()))
        .transpose()?;

    let new_dir = {
        let conn = lock_db(&state)?;
        match parent {
            Some(pid) => {
                let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
                let parent_folder = folders.iter().find(|f| f.id == pid).ok_or_else(|| "folder not found".to_string())?;
                std::path::PathBuf::from(&parent_folder.path).join(&name)
            }
            None => {
                let picked = app.dialog().file().blocking_pick_folder();
                let Some(picked) = picked else {
                    return Err("cancelled".to_string());
                };
                picked.into_path().map_err(|e| e.to_string())?.join(&name)
            }
        }
    };

    std::fs::create_dir(&new_dir).map_err(|e| e.to_string())?;

    let conn = lock_db(&state)?;
    let new_id = db::insert_folder_with_parent(&conn, &name, parent, &new_dir.to_string_lossy())
        .map_err(|e| e.to_string())?;
    Ok(FolderDto {
        id: new_id.to_string(),
        name,
        path: new_dir.to_string_lossy().to_string(),
        parent_id: parent.map(|p| p.to_string()),
        count: 0,
    })
}
```

`db::insert_folder_with_parent(conn, name, parent_id, path) -> Result<i64, DbError>`
in `db/repository.rs` ergänzen — dünner Wrapper um dasselbe
`INSERT INTO folders (name, parent_id, path) VALUES (...)` wie in
`find_or_insert_folder` (Task 2), aber OHNE den vorherigen
Existenz-Check (ein `create_folder`-Aufruf mit bereits existierendem
Zielpfad soll fehlschlagen, nicht still den bestehenden Ordner
zurückgeben — `std::fs::create_dir` schlägt in diesem Fall ohnehin schon
mit `AlreadyExists` fehl, bevor die DB überhaupt erreicht wird).

- [ ] **Step 4: Beide Commands in `lib.rs`/`main.rs` registrieren**

In der `tauri::generate_handler![...]`-Liste (Datei, in der die
bestehenden Commands wie `move_file_to_folder`/`create_folder`
Geschwister — z. B. `set_favorite`, `list_folders` — bereits eingetragen
sind) `create_folder` und `move_file_to_folder` ergänzen.

- [ ] **Step 5: Rust-Test für `move_file_to_folder`**

```rust
#[test]
fn move_file_to_folder_updates_path_and_db() {
    let tmp = tempfile::tempdir().unwrap();
    let src_dir = tmp.path().join("A");
    let dst_dir = tmp.path().join("B");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dst_dir).unwrap();
    let file_path = src_dir.join("model.3mf");
    write_minimal_3mf(&file_path);

    let state = test_app_state();
    // Datei importieren, Zielordner anlegen (ueber ensure_folder_path direkt, kein Command-Roundtrip noetig)
    // ... Details analog zu bestehenden Tests in diesem Modul; pruefen:
    // nach move_file_to_folder liegt model.3mf physisch unter dst_dir UND
    // db::get_file(...).path == dst_dir/model.3mf
}
```

Implementierer: exakten Testkörper an die bestehenden Test-Helper in
`commands.rs` (siehe `#[cfg(test)] mod tests` unten im File) anpassen —
Kernaussage (physischer Pfad UND DB-Pfad stimmen nach dem Move überein)
ist bindend, der genaue Helper-Aufruf-Stil folgt dem, was im File bereits
für Import-Tests verwendet wird.

- [ ] **Step 6: Tests + Commit**

```bash
cd src-tauri && cargo test
git add src-tauri/src/commands.rs src-tauri/src/db/repository.rs src-tauri/src/lib.rs
git commit -m "Backend: create_folder und move_file_to_folder Commands (physisches Verschieben)"
```

---

### Task 5: `rename_folder` und `move_folder`

**Files:**
- Modify: `src-tauri/src/commands.rs` (zwei neue Commands)
- Modify: `src-tauri/src/db/repository.rs` (`update_paths_under_folder`)

**Interfaces:**
- Consumes: Task 4's `db::insert_folder_with_parent`, Task 1's `FolderRecord`.
- Produces: `#[tauri::command] pub fn rename_folder(state, folder_id: String, name: String) -> CmdResult<()>`,
  `#[tauri::command] pub fn move_folder(state, folder_id: String, new_parent_id: Option<String>) -> CmdResult<()>`
  — von Task 7 konsumiert.

- [ ] **Step 1: `update_paths_under_folder` in `db/repository.rs`**

Rekursives Präfix-Update für `folders.path` UND `files.path` unterhalb
eines Ordners, nachdem dessen eigener Pfad sich geändert hat:

```rust
pub fn update_paths_under_folder(conn: &Connection, folder_id: i64, old_path: &str, new_path: &str) -> Result<(), DbError> {
    conn.execute("UPDATE folders SET path = ?1 WHERE id = ?2", params![new_path, folder_id])?;
    conn.execute(
        "UPDATE files SET path = ?1 || substr(path, ?2) WHERE folder_id = ?3",
        params![new_path, (old_path.len() + 1) as i64, folder_id],
    )?;

    let mut stmt = conn.prepare("SELECT id, path FROM folders WHERE parent_id = ?1")?;
    let children: Vec<(i64, String)> = stmt
        .query_map(params![folder_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    for (child_id, child_old_path) in children {
        let suffix = &child_old_path[old_path.len()..];
        let child_new_path = format!("{new_path}{suffix}");
        update_paths_under_folder(conn, child_id, &child_old_path, &child_new_path)?;
    }
    Ok(())
}
```

- [ ] **Step 2: `rename_folder`-Command**

```rust
#[tauri::command]
pub fn rename_folder(state: State<AppState>, folder_id: String, name: String) -> CmdResult<()> {
    let id: i64 = folder_id.parse().map_err(|_| "invalid folder id".to_string())?;
    let conn = lock_db(&state)?;
    let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
    let folder = folders.iter().find(|f| f.id == id).ok_or_else(|| "folder not found".to_string())?;

    let old_path = std::path::PathBuf::from(&folder.path);
    let new_path = old_path.with_file_name(&name);

    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;
    conn.execute("UPDATE folders SET name = ?1 WHERE id = ?2", params![name, id]).map_err(|e| e.to_string())?;
    db::update_paths_under_folder(&conn, id, &folder.path, &new_path.to_string_lossy())
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 3: `move_folder`-Command mit Zyklus-Schutz**

```rust
#[tauri::command]
pub fn move_folder(state: State<AppState>, folder_id: String, new_parent_id: Option<String>) -> CmdResult<()> {
    let id: i64 = folder_id.parse().map_err(|_| "invalid folder id".to_string())?;
    let target: Option<i64> = new_parent_id
        .map(|s| s.parse::<i64>().map_err(|_| "invalid folder id".to_string()))
        .transpose()?;

    let conn = lock_db(&state)?;
    let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
    let folder = folders.iter().find(|f| f.id == id).ok_or_else(|| "folder not found".to_string())?.clone();

    if let Some(target_id) = target {
        if target_id == id || is_descendant(&folders, target_id, id) {
            return Err("Ein Ordner kann nicht in einen eigenen Unterordner verschoben werden".to_string());
        }
    }

    let new_parent_path = match target {
        Some(target_id) => folders.iter().find(|f| f.id == target_id).map(|f| f.path.clone()),
        None => std::path::Path::new(&folder.path).parent().map(|p| p.to_string_lossy().to_string()),
    }
    .ok_or_else(|| "target folder not found".to_string())?;

    let old_path = std::path::PathBuf::from(&folder.path);
    let new_path = std::path::PathBuf::from(&new_parent_path).join(&folder.name);

    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;
    conn.execute("UPDATE folders SET parent_id = ?1 WHERE id = ?2", params![target, id]).map_err(|e| e.to_string())?;
    db::update_paths_under_folder(&conn, id, &folder.path, &new_path.to_string_lossy())
        .map_err(|e| e.to_string())
}

fn is_descendant(folders: &[db::FolderRecord], candidate_id: i64, ancestor_id: i64) -> bool {
    let mut current = candidate_id;
    while let Some(f) = folders.iter().find(|f| f.id == current) {
        match f.parent_id {
            Some(pid) if pid == ancestor_id => return true,
            Some(pid) => current = pid,
            None => return false,
        }
    }
    false
}
```

`FolderRecord` braucht `#[derive(Debug, Clone)]` (bereits in Task 1
vorgesehen) für das `.clone()` hier.

- [ ] **Step 4: Beide Commands registrieren** (wie Task 4 Step 4).

- [ ] **Step 5: Tests**

```rust
#[test]
fn move_folder_rejects_moving_into_own_descendant() { /* Baum A -> B anlegen, move_folder(A, Some(B)) erwartet Err */ }

#[test]
fn move_folder_updates_all_descendant_paths() { /* A/B/C anlegen, move_folder(B, None) [an Wurzel], pruefen dass C.path und alle files.path darunter das neue Praefix haben */ }

#[test]
fn rename_folder_updates_own_and_descendant_paths() { /* analog */ }
```

Implementierer: konkrete Testkörper nach demselben Muster wie Task 2/4
(reales Temp-Verzeichnis, `db::insert_folder_with_parent`/
`ensure_folder_path` zum Aufbau der Ausgangslage, dann Command-Funktion
direkt aufrufen — nicht über den Tauri-IPC-Layer, wie es die
bestehenden Tests in `commands.rs` bereits handhaben).

- [ ] **Step 6: Tests + Commit**

```bash
cd src-tauri && cargo test
git add src-tauri/src/commands.rs src-tauri/src/db/repository.rs
git commit -m "Backend: rename_folder und move_folder mit rekursivem Pfad-Update und Zyklus-Schutz"
```

---

### Task 6: Frontend `FolderTree.tsx` — Baum anzeigen, Navigation

**Files:**
- Create: `src/components/FolderTree.tsx`
- Modify: `src/components/Sidebar.tsx` (Zeilen 100-119, alte flache Ordnerliste ersetzt durch `<FolderTree>`)
- Modify: `src/App.tsx` (`filtered`-Berechnung Zeile 185-190: rekursive Ordner-Filterung; `folders`-State bleibt `FolderDto[]`, Typ um `path`/`parentId` erweitern in `src/types.ts`)
- Modify: `src/types.ts` (`Folder`-Typ um `path: string`, `parentId: string | null` erweitern; `ModelFile` hat bereits `folderId`)
- Modify: i18n-Dateien (neuer Key `allModelsLabel`, falls `filesCount`/vorhandene Keys nicht schon passend — bestehenden Key `t('filesCount')`-Nachbarschaft prüfen und wiederverwenden wo möglich, sonst neuen Key nach demselben Muster wie Task 1 von `2026-09-13-gui-shell-redesign.md` ergänzen)

**Interfaces:**
- Consumes: `FolderDto[]` (Task 1, jetzt mit `path`/`parentId`).
- Produces: `<FolderTree folders={FolderDto[]} activeFolderId={string} onSelect={(id: string) => void} />`
  (reine Navigation in diesem Task — Drag&Drop kommt in Task 7 als
  Erweiterung derselben Komponente, nicht als Neubau).

- [ ] **Step 1: `types.ts` erweitern**

`Folder`-Typ (Feld `path: string`, `parentId: string | null` ergänzen,
bestehende Felder `id`/`name`/`count` bleiben).

- [ ] **Step 2: `FolderTree.tsx` — Baum aus flacher Liste bauen, auf-/zuklappen**

```tsx
import { useMemo, useState } from 'react';
import type { Folder } from '../types';
import { useT } from '../i18n/LanguageContext';

interface TreeNode extends Folder {
  children: TreeNode[];
}

interface Props {
  folders: Folder[];
  activeFolderId: string;
  onSelect: (id: string) => void;
}

function buildTree(folders: Folder[]): TreeNode[] {
  const byId = new Map<string, TreeNode>(folders.map((f) => [f.id, { ...f, children: [] }]));
  const roots: TreeNode[] = [];
  for (const node of byId.values()) {
    if (node.parentId && byId.has(node.parentId)) {
      byId.get(node.parentId)!.children.push(node);
    } else {
      roots.push(node);
    }
  }
  return roots;
}

export function FolderTree({ folders, activeFolderId, onSelect }: Props) {
  const t = useT();
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const tree = useMemo(() => buildTree(folders), [folders]);
  const totalCount = useMemo(() => folders.filter((f) => !f.parentId).reduce((sum, f) => sum + f.count, 0), [folders]);

  const toggle = (id: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });

  function renderNode(node: TreeNode, depth: number) {
    const isOpen = expanded.has(node.id);
    return (
      <div key={node.id}>
        <div
          onClick={() => onSelect(node.id)}
          style={{ paddingLeft: 6 + depth * 16 }}
          className={`flex items-center gap-1.5 h-7 pr-2 rounded-[7px] cursor-pointer text-[12.5px] ${
            node.id === activeFolderId
              ? 'bg-[var(--accent-soft)] text-[var(--accent)] font-semibold'
              : 'text-[var(--ink-2)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)]'
          }`}
        >
          <span
            onClick={(e) => {
              if (node.children.length === 0) return;
              e.stopPropagation();
              toggle(node.id);
            }}
            className={`w-3.5 text-[9px] text-[var(--ink-3)] ${node.children.length === 0 ? 'invisible' : ''}`}
          >
            {isOpen ? '▾' : '▸'}
          </span>
          <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{node.name}</span>
          <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">{node.count}</span>
        </div>
        {isOpen && node.children.map((c) => renderNode(c, depth + 1))}
      </div>
    );
  }

  return (
    <div>
      <div
        onClick={() => onSelect('all')}
        className={`flex items-center gap-2 h-7 px-1.5 rounded-[7px] cursor-pointer text-[12.5px] ${
          activeFolderId === 'all'
            ? 'bg-[var(--accent-soft)] text-[var(--accent)] font-semibold'
            : 'text-[var(--ink-2)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)]'
        }`}
      >
        <span className="flex-1">{t('allModelsLabel')}</span>
        <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">{totalCount}</span>
      </div>
      {tree.map((n) => renderNode(n, 0))}
    </div>
  );
}
```

`t('allModelsLabel')` — Key in jeder i18n-Datei ergänzen (deutscher Wert
`'Alle Modelle'`, analog zu Task 1 von `2026-09-13-gui-shell-redesign.md`
Step 2 im Vorgehen). Falls bereits ein passender Key mit exakt diesem
Textinhalt existiert (`grep -rn "Alle Modelle" src/i18n/`), den
vorhandenen wiederverwenden statt zu duplizieren.

- [ ] **Step 3: `Sidebar.tsx` verdrahten**

Den bestehenden Block (Zeilen 100-119: `{t('foldersHeading')}` +
`folders.map(...)`) ersetzen durch:
```tsx
        <div className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)] px-1.5 pb-2">
          {t('foldersHeading')}
        </div>
        <FolderTree folders={folders} activeFolderId={activeFolderId} onSelect={onFolderSelect} />
```
`Sidebar`s `Props`-Interface/Import bleiben bei `folders: Folder[]`,
`activeFolderId`/`onFolderSelect` unverändert (Typen jetzt erweitert
durch Step 1) — kein Signaturbruch nach außen.

- [ ] **Step 4: `App.tsx` — rekursive Ordner-Filterung**

Aktuelle Zeile 187:
```tsx
      .filter((m) => activeFolderId === 'all' || m.folderId === activeFolderId)
```
ersetzen durch (rekursiv: Ordner UND alle Nachfahren zählen mit, analog
zur Backend-`count`-Semantik aus Task 1):
```tsx
      .filter((m) => activeFolderId === 'all' || isFileInFolderOrDescendant(m.folderId, activeFolderId, folders))
```
Neue Hilfsfunktion (Modulebene in `App.tsx` oder `src/lib/`, falls dort
bereits ähnliche reine Helfer liegen — bestehende Konvention in
`src/lib/` vor dem Entscheiden prüfen):
```ts
function isFileInFolderOrDescendant(fileFolderId: string | null, targetId: string, folders: Folder[]): boolean {
  if (fileFolderId === null) return false;
  let current: string | null = fileFolderId;
  while (current !== null) {
    if (current === targetId) return true;
    current = folders.find((f) => f.id === current)?.parentId ?? null;
  }
  return false;
}
```

- [ ] **Step 5: Typecheck + manueller Test**

```bash
npx tsc --noEmit
npm run tauri dev
```
Prüfen: importierten Ordner mit Unterordner öffnen (echter Testordner
mit `.3mf`-Dateien in zwei Ebenen anlegen), Baum zeigt beide Ebenen
korrekt verschachtelt, Klick auf Elternordner zeigt auch die Dateien aus
dem Unterordner (rekursiv), Klick auf "Alle Modelle" zeigt wieder alles.

- [ ] **Step 6: Commit**

```bash
git add src/components/FolderTree.tsx src/components/Sidebar.tsx src/App.tsx src/types.ts src/i18n/
git commit -m "Frontend: Ordner-Baum in der Sidebar (Navigation, rekursive Filterung)"
```

---

### Task 7: Drag&Drop — physisches Verschieben aus der App heraus

**Files:**
- Modify: `src/components/FolderTree.tsx` (Drop-Ziele)
- Modify: `src/components/ModelGrid.tsx` (Karten als Drag-Quelle)
- Modify: `src/App.tsx` (Handler `onMoveFileToFolder`, `onMoveFolder`, `onCreateFolder`, Toast-State)
- Create: `src/components/MoveToast.tsx` (kleine Bestätigungs-Meldung, analog zum Toast im HTML-Mockup)

**Interfaces:**
- Consumes: `move_file_to_folder`, `move_folder`, `create_folder` (Tauri-Commands aus Task 4/5).
- Produces: Keine neuen exportierten Typen — reine Interaktions-Verdrahtung.

- [ ] **Step 1: Maus-basiertes Drag-Tracking in `App.tsx`**

Folgt exakt dem Muster aus `Sidebar.tsx:65-84`
(Warteschlange) bzw. `ModelGrid.tsx:34-73` (Karten-Reorder): globaler
`mousemove`/`mouseup`-Listener über `useEffect`, State für
`draggedFileId: string | null` und `dragOverFolderId: string | null`.
Neuer State in `App.tsx`:
```ts
const [draggedFileId, setDraggedFileId] = useState<string | null>(null);
const [dragOverFolderId, setDragOverFolderId] = useState<string | null>(null);
const [moveToast, setMoveToast] = useState<{ from: string; to: string } | null>(null);
```

- [ ] **Step 2: `ModelGrid.tsx` — Karten als Drag-Quelle**

`onMouseDown` auf einer Karte ruft zusätzlich zum bestehenden
Reorder-Start (`reorderable`-Zweig, unverändert) — bei NICHT
`reorderable` (normale Katalog-Ansicht, kein Sammlungs-Reorder aktiv) —
einen neuen, von `App.tsx` durchgereichten Handler `onDragFileStart(id)`
auf, der `draggedFileId` setzt. Analog zum bestehenden
`DRAG_THRESHOLD_PX`-Muster (Zeile 40) erst nach Mindestbewegung als
"echtes Ziehen" werten, um normale Klicks (Öffnen der Detailseite) nicht
zu stören.

- [ ] **Step 3: `FolderTree.tsx` — Drop-Ziele**

Jede Baum-Zeile bekommt `onMouseEnter`, das bei aktivem `draggedFileId`
(aus `App.tsx` durchgereicht) `dragOverFolderId` auf die eigene `id`
setzt (visuelles Highlight: `border-[var(--accent)] bg-[var(--accent-soft)]`
zusätzlich zur aktiven Klasse, wenn `dragOverFolderId === node.id`).

- [ ] **Step 4: Globaler `mouseup`-Handler löst den Move aus**

In `App.tsx`, im selben `useEffect`-Muster wie `Sidebar.tsx:65-84`:
```ts
useEffect(() => {
  if (!draggedFileId) return;
  const handleMouseUp = () => {
    const fileId = draggedFileId;
    const folderId = dragOverFolderId;
    setDraggedFileId(null);
    setDragOverFolderId(null);
    if (!folderId) return;
    const file = models.find((m) => m.id === fileId);
    const targetFolder = folders.find((f) => f.id === folderId);
    if (!file || !targetFolder) return;
    invoke('move_file_to_folder', { fileId, folderId })
      .then(() => {
        setMoveToast({ from: file.name, to: targetFolder.path });
        refreshFolders();
        refreshFiles(); // vorhandene Reload-Funktion, exakten Namen aus bestehendem App.tsx-Code uebernehmen
      })
      .catch((e) => setCatalogBackupError(String(e))); // oder vorhandenes generisches Fehler-Anzeige-Muster verwenden
  };
  document.addEventListener('mouseup', handleMouseUp);
  return () => document.removeEventListener('mouseup', handleMouseUp);
}, [draggedFileId, dragOverFolderId, models, folders]);
```
Implementierer: `refreshFiles`/Fehler-State exakt an die tatsächlich in
`App.tsx` vorhandenen Namen anpassen (Datei vor diesem Schritt lesen —
es existieren bereits `refresh*`-Funktionen für andere Entitäten, deren
Namensmuster hier fortgesetzt wird, nicht neu erfunden).

- [ ] **Step 5: `MoveToast.tsx`**

Kleine, unten mittig positionierte Komponente (`fixed bottom-4
left-1/2 -translate-x-1/2`), zeigt `moveToast.from`/`moveToast.to`,
verschwindet nach 3 Sekunden (`useEffect` mit `setTimeout`), analog zur
Toast-Optik im HTML-Mockup (`.toast`-Klasse dort als visuelle Referenz,
nicht 1:1 CSS-Kopie nötig — Projekt-Tokens `var(--ink)`/`var(--bg)`
verwenden).

- [ ] **Step 6: "+ Neuer Ordner"-Button**

In `Sidebar.tsx`, direkt unter dem `<FolderTree>`-Aufruf, ein Button
`+ Neuer Ordner`, ruft `onCreateFolder(activeFolderId === 'all' ? null :
activeFolderId)` auf (App.tsx-Handler ruft `invoke('create_folder', {
parentId, name })` — Name-Eingabe über ein einfaches
`window.prompt`-Äquivalent ist in diesem Projekt NICHT üblich
(`FilamentSpoolForm.tsx`/`CollectionsGallery.tsx` verwenden Inline-Input
mit `autoFocus`+`onBlur`+`Enter`-Submit, siehe
`CollectionsGallery.tsx` `creating`-State als Vorlage) — denselben
Inline-Input-Ansatz für den neuen Ordnernamen übernehmen statt
`window.prompt`.

- [ ] **Step 7: Ordner-Verschieben per Drag (Ordner-auf-Ordner)**

Analog zu Step 2-4, aber Drag-Quelle ist eine `FolderTree`-Zeile selbst
(nicht nur Modell-Karten): `onMouseDown` auf einer Baum-Zeile setzt
`draggedFolderId` statt `draggedFileId` (zweiter, paralleler State,
gleiches Muster). Beim Drop auf eine andere Baum-Zeile
`invoke('move_folder', { folderId: draggedFolderId, newParentId:
targetId })`. Clientseitige Zyklus-Vorabprüfung (Baum-Traversierung wie
in `is_descendant` auf Rust-Seite, hier in TS dupliziert für sofortiges
visuelles Feedback — kein Drop-Highlight auf dem eigenen Unterbaum) —
die serverseitige Prüfung aus Task 5 bleibt die verbindliche.

- [ ] **Step 8: Typecheck + manueller Test in der GEBAUTEN App**

```bash
npx tsc --noEmit
NO_STRIP=1 npm run tauri build -- --bundles appimage
```
Smoke-Test AUSSCHLIESSLICH über die gebaute AppImage (nicht nur
`npm run tauri dev`) — genau hier zeigt sich, ob das Maus-Event-Muster
unter WebKitGTK tatsächlich funktioniert (der Browser-Mockup mit
nativem HTML5-DnD hätte diesen Unterschied nicht aufgedeckt, siehe
Global Constraints). Datei per Maus auf einen Ordner ziehen → Toast
erscheint, Datei liegt danach nachweislich am neuen Pfad (`ls` auf dem
Zielverzeichnis prüfen).

- [ ] **Step 9: Commit**

```bash
git add src/components/FolderTree.tsx src/components/ModelGrid.tsx src/components/MoveToast.tsx src/components/Sidebar.tsx src/App.tsx
git commit -m "Frontend: Drag&Drop verschiebt Dateien/Ordner physisch (maus-basiert, WebKitGTK-kompatibel)"
```
