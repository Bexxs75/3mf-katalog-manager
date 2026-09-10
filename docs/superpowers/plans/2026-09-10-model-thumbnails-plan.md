# Modell-Thumbnails, Bild-Upload und Quelle-Link Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Modell-Karten im Grid zeigen ein echtes Bild statt eines Platzhalters (eigenes Upload > eingebettetes 3MF-Thumbnail > automatischer 3D-Snapshot), der Nutzer kann ein eigenes Bild hochladen und eine Quelle-URL pro Modell hinterlegen.

**Architecture:** Drei neue, überwiegend nullable Spalten auf `files` (Task 1 legt sie zentral an, inkl. der serverseitigen Prioritäts-Auflösung zu einem einzigen `displayImage`-Feld). Die vier sichtbaren Bausteine (Grid-Anzeige, Bild-Upload, automatischer Snapshot, Quelle-URL) sind danach unabhängige, aufeinanderfolgende Vertical-Slice-Tasks.

**Tech Stack:** Tauri v2 (Rust-Backend, `rusqlite`) + React/TypeScript-Frontend (three.js für die 3D-Live-Ansicht), eigenes i18n-Context-System (de/en/es/fr).

## Global Constraints

- SQLite ohne Migrationsframework: jede neue Spalte braucht sowohl Aufnahme in `schema.sql`s `CREATE TABLE IF NOT EXISTS` (Neuinstallationen) als auch eine fehlertolerante `ALTER TABLE ... ADD COLUMN` in `repository.rs`s `init()` (reale, bereits befüllte Produktions-DB).
- Bild-Priorität für die Grid-Anzeige: **eigenes Upload > eingebettetes 3MF-Thumbnail > automatischer 3D-Snapshot > kein Bild (Platzhalter bleibt)**.
- Base64-Bilddaten über die IPC-Grenze werden unconditional mit `data:image/png;base64,`-Präfix ausgeliefert, unabhängig vom tatsächlichen hochgeladenen Bildformat (jpg/jpeg/webp eingeschlossen) - gleiche, bereits etablierte und akzeptierte Vereinfachung wie beim Filament-Bild-Upload (verlässt sich auf Browser-Magic-Byte-Sniffing beim `<img>`-Rendering). Das ist **keine** zu behebende Ungenauigkeit.
- `cargo test` und `npx tsc --noEmit` müssen nach jedem Task sauber durchlaufen.
- i18n: jeder neue sichtbare Text bekommt einen Key in `src/i18n/types.ts` und echte Übersetzungen in allen vier Sprachdateien (de/en/es/fr).

---

## Task 1: Datenbank-Fundament + Bild-Prioritäts-Auflösung

**Files:**
- Modify: `src-tauri/src/db/schema.sql`
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/db/mod.rs`

**Interfaces:**
- Produces: `NewFile`/`FileRecord` (`src-tauri/src/db/models.rs`) mit drei neuen Feldern: `render_snapshot_png: Option<Vec<u8>>`, `custom_image_png: Option<Vec<u8>>`, `source_url: Option<String>`.
- Produces: `ModelFileDto` (`src-tauri/src/commands.rs`) mit `display_image: Option<String>` (bereits Base64-kodiert mit `data:image/png;base64,`-Präfix, Priorität custom > eingebettet > Snapshot) und `source_url: Option<String>`. **Wichtig:** `source_url` wird ab diesem Task vom Backend mitgeschickt, aber erst in Task 5 im Frontend-Typ (`ModelFile`) deklariert und genutzt - bis dahin kommt das Feld ungenutzt über die IPC-Grenze an, das ist unschädlich (TypeScript prüft keine überzähligen JSON-Felder). `display_image` wird dagegen bereits in Task 2 im Frontend-Typ ergänzt.
- Produces: `pub(crate) fn resolve_display_image(custom_image_png: Option<Vec<u8>>, thumbnail_png: Option<Vec<u8>>, render_snapshot_png: Option<Vec<u8>>) -> Option<String>` (`src-tauri/src/commands.rs`) - spätere Tasks rufen diese Funktion nicht direkt auf, sie ist intern von `to_dto` genutzt.

- [ ] **Step 1: Schema erweitern**

In `src-tauri/src/db/schema.sql` die `files`-Tabelle (aktuell endet bei `content_hash TEXT`) um drei Spalten ergänzen:

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

- [ ] **Step 2: Migration für Bestands-DBs**

In `src-tauri/src/db/repository.rs`s `init()` (aktuell endet mit der `idx_files_content_hash`-Zeile) drei weitere fehlertolerante Migrationszeilen ergänzen:

```rust
    let _ = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files (content_hash)",
        [],
    );
    let _ = conn.execute("ALTER TABLE files ADD COLUMN render_snapshot_png BLOB", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN custom_image_png BLOB", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN source_url TEXT", []);
    Ok(())
}
```

- [ ] **Step 3: `NewFile`/`FileRecord` erweitern**

In `src-tauri/src/db/models.rs` beide Structs um drei Felder ergänzen, jeweils direkt vor der schließenden `}`, nach dem bestehenden `content_hash`-Feld:

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
    pub render_snapshot_png: Option<Vec<u8>>,
    pub custom_image_png: Option<Vec<u8>>,
    pub source_url: Option<String>,
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
    pub render_snapshot_png: Option<Vec<u8>>,
    pub custom_image_png: Option<Vec<u8>>,
    pub source_url: Option<String>,
}
```

- [ ] **Step 4: `insert_file`/`get_file`/`list_files`/`row_to_file` erweitern**

In `src-tauri/src/db/repository.rs`s `insert_file` die `INSERT`-Anweisung um die drei neuen Spalten am Ende ergänzen:

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

`get_file` und `list_files` beide SQL-Statements um die drei Spalten am Ende erweitern:

```rust
pub fn get_file(conn: &Connection, id: i64) -> Result<Option<FileRecord>, DbError> {
    let row = conn
        .query_row(
            "SELECT id, name, path, file_type, folder_id, origin, sync_status, cloud_id,
                    file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
                    volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at,
                    print_status, last_viewed_at, creator, content_hash,
                    render_snapshot_png, custom_image_png, source_url
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
                print_status, last_viewed_at, creator, content_hash,
                render_snapshot_png, custom_image_png, source_url
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

`row_to_file` um die drei neuen Spalten (Indizes 21-23) ergänzen:

```rust
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
        render_snapshot_png: row.get(21)?,
        custom_image_png: row.get(22)?,
        source_url: row.get(23)?,
    })
```

- [ ] **Step 5: `import_one` erweitern**

In `src-tauri/src/commands.rs`s `import_one` die `NewFile`-Konstruktion um drei Felder ergänzen (alle drei sind bei einem frischen Import ihre echten, korrekten Startwerte - keine Platzhalter, da keiner der drei automatisch beim Import entsteht):

```rust
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
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
    };
```

- [ ] **Step 6: `display_image`-Priorität + DTO**

In `src-tauri/src/commands.rs` nach `estimate_weight_g` die Auflösungsfunktion ergänzen:

```rust
pub(crate) fn resolve_display_image(
    custom_image_png: Option<Vec<u8>>,
    thumbnail_png: Option<Vec<u8>>,
    render_snapshot_png: Option<Vec<u8>>,
) -> Option<String> {
    use base64::Engine;
    custom_image_png
        .or(thumbnail_png)
        .or(render_snapshot_png)
        .map(|bytes| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        })
}
```

`ModelFileDto` um zwei Felder erweitern (nach `creator`):

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
    pub last_viewed_at: Option<String>,
    pub creator: Option<String>,
    pub display_image: Option<String>,
    pub source_url: Option<String>,
}
```

`to_dto` anpassen - `display_image` muss berechnet werden, bevor `file.materials` weiter unten per `.into_iter()` verbraucht wird (die drei BLOB-Felder sind davon unabhängige Felder desselben Structs, ein partieller Move ist unproblematisch):

```rust
pub(crate) fn to_dto(file: FileRecord) -> ModelFileDto {
    let estimated_weight_g =
        estimate_weight_g(file.volume_cm3, file.materials.first().map(|m| m.name.as_str()));
    let display_image = resolve_display_image(
        file.custom_image_png,
        file.thumbnail_png,
        file.render_snapshot_png,
    );
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
        last_viewed_at: file.last_viewed_at,
        creator: file.creator,
        display_image,
        source_url: file.source_url,
    }
}
```

- [ ] **Step 7: Test-Helper, Roundtrip-Test, Prioritäts-Tests**

In `src-tauri/src/db/mod.rs`s `sample_file()`-Helper die drei neuen Felder am Ende des Struct-Literals ergänzen:

```rust
            content_hash: None,
            render_snapshot_png: None,
            custom_image_png: None,
            source_url: None,
        }
    }
```

Neuen Test nach dem letzten bestehenden Test in `db/mod.rs`s `mod tests` einfügen:

```rust
    #[test]
    fn stores_and_lists_the_image_and_source_url_columns() {
        let mut conn = connect_in_memory().expect("connect");
        let mut file = sample_file();
        file.render_snapshot_png = Some(vec![1, 2, 3]);
        file.custom_image_png = Some(vec![4, 5, 6]);
        file.source_url = Some("https://example.com/model".to_string());
        let id = insert_file(&mut conn, &file).expect("insert");

        let stored = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(stored.render_snapshot_png, Some(vec![1, 2, 3]));
        assert_eq!(stored.custom_image_png, Some(vec![4, 5, 6]));
        assert_eq!(stored.source_url, Some("https://example.com/model".to_string()));
    }
```

In `src-tauri/src/commands.rs`s bestehendem `#[cfg(test)] mod tests`-Block nach den vorhandenen Tests einfügen:

```rust
    #[test]
    fn resolve_display_image_prefers_custom_over_embedded_over_snapshot() {
        use base64::Engine;
        let result = resolve_display_image(Some(vec![1]), Some(vec![2]), Some(vec![3])).expect("some image");
        let b64 = result.strip_prefix("data:image/png;base64,").expect("data url prefix");
        let decoded = base64::engine::general_purpose::STANDARD.decode(b64).expect("valid base64");
        assert_eq!(decoded, vec![1]);
    }

    #[test]
    fn resolve_display_image_falls_back_to_embedded_thumbnail() {
        use base64::Engine;
        let result = resolve_display_image(None, Some(vec![2]), Some(vec![3])).expect("some image");
        let b64 = result.strip_prefix("data:image/png;base64,").expect("data url prefix");
        let decoded = base64::engine::general_purpose::STANDARD.decode(b64).expect("valid base64");
        assert_eq!(decoded, vec![2]);
    }

    #[test]
    fn resolve_display_image_falls_back_to_render_snapshot() {
        use base64::Engine;
        let result = resolve_display_image(None, None, Some(vec![3])).expect("some image");
        let b64 = result.strip_prefix("data:image/png;base64,").expect("data url prefix");
        let decoded = base64::engine::general_purpose::STANDARD.decode(b64).expect("valid base64");
        assert_eq!(decoded, vec![3]);
    }

    #[test]
    fn resolve_display_image_returns_none_without_any_source() {
        assert_eq!(resolve_display_image(None, None, None), None);
    }
```

- [ ] **Step 8: Testen**

Run: `cd src-tauri && cargo test`
Expected: alle bisherigen Tests weiterhin grün, plus die 5 neuen Tests.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/db/schema.sql src-tauri/src/db/repository.rs src-tauri/src/db/models.rs src-tauri/src/commands.rs src-tauri/src/db/mod.rs
git commit -m "$(cat <<'EOF'
Rust: Datenfundament fuer Modell-Bilder (Upload/Snapshot) und Quelle-URL

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

## Task 2: Grid zeigt das aufgelöste Bild

**Files:**
- Modify: `src/types/index.ts`
- Modify: `src/components/ModelGrid.tsx`

**Interfaces:**
- Consumes: `ModelFileDto.display_image` (Task 1, bereits Base64-Data-URL oder `null`).
- Produces: `ModelFile.displayImage: string | null` - von Task 3 und Task 4 für optimistische lokale Updates genutzt.

Kein Rust-Test nötig - reiner Frontend-Task, Task 1 hat die Backend-Logik bereits vollständig abgedeckt.

- [ ] **Step 1: Frontend-Typ**

In `src/types/index.ts`s `ModelFile`-Interface nach `creator: string | null;` ergänzen:

```ts
  creator: string | null;
  displayImage: string | null;
```

- [ ] **Step 2: ModelGrid zeigt das Bild**

In `src/components/ModelGrid.tsx` den Vorschau-Container (aktuell Zeile 37-66, von `<div className="relative aspect-square ...">` bis zur schließenden `</div>`) ersetzen: der bisherige Platzhalter (Schraffur, gestricheltes Quadrat, `previewLabel3d`-Text) wird nur noch gezeigt, wenn `m.displayImage` fehlt; die drei Badges (NEU, Herkunft, Gedruckt) bleiben davon unabhängig immer sichtbar:

```tsx
          <div className="relative aspect-square bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
            {m.displayImage ? (
              <img
                src={m.displayImage}
                alt=""
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
                <div className="absolute inset-0 grid place-items-center">
                  <div className="w-[52px] h-[52px] border border-dashed border-[var(--line-strong)] rotate-45" />
                </div>
                <div className="absolute left-2 bottom-[7px] font-mono-ui text-[9px] tracking-[0.08em] uppercase text-[var(--ink-3)]">
                  {t('previewLabel3d')}
                </div>
              </>
            )}
            {Date.now() - new Date(m.importedAt).getTime() < 24 * 60 * 60 * 1000 && (
              <div className="absolute left-[7px] top-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]">
                {t('newBadge')}
              </div>
            )}
            {originAbbr[m.origin] && (
              <div className="absolute right-[7px] top-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]">
                {originAbbr[m.origin]}
              </div>
            )}
            {m.printStatus === 'printed' && (
              <div className="absolute right-[7px] bottom-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]">
                ✓ {t('printedBadge')}
              </div>
            )}
          </div>
```

- [ ] **Step 3: Testen**

Run: `npx tsc --noEmit` (aus dem Projekt-Root)
Expected: sauber, keine Fehler.

- [ ] **Step 4: Commit**

```bash
git add src/types/index.ts src/components/ModelGrid.tsx
git commit -m "$(cat <<'EOF'
Grid zeigt aufgeloestes Modell-Bild statt Platzhalter

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

## Task 3: Eigenes Bild hochladen

**Files:**
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/components/DetailPanel.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `ModelFile.displayImage` (Task 2).
- Produces: Tauri-Command `upload_custom_image(file_id: String) -> CmdResult<Option<String>>` (öffnet Dateidialog, liefert die neue `displayImage`-Data-URL oder `None` bei Abbruch). `DetailPanel`-Prop `onUploadImage: () => void`.

- [ ] **Step 1: Repository-Funktion + Test**

In `src-tauri/src/db/repository.rs` nach `set_content_hash` einfügen:

```rust
pub fn set_custom_image_png(conn: &Connection, file_id: i64, png: &[u8]) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET custom_image_png = ?1 WHERE id = ?2",
        params![png, file_id],
    )?;
    Ok(())
}
```

In `src-tauri/src/db/mod.rs`s `pub use repository::{...}`-Liste `set_custom_image_png` alphabetisch einsortieren.

Neuen Test nach dem letzten bestehenden Test in `db/mod.rs`s `mod tests` einfügen:

```rust
    #[test]
    fn set_custom_image_png_updates_the_image() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        set_custom_image_png(&conn, id, &[9, 9, 9]).expect("update");
        let file = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(file.custom_image_png, Some(vec![9, 9, 9]));
    }
```

Run: `cd src-tauri && cargo test set_custom_image_png_updates_the_image`
Expected: PASS

- [ ] **Step 2: Command**

In `src-tauri/src/commands.rs` nach `mark_file_viewed` einfügen:

```rust
#[tauri::command]
pub async fn upload_custom_image(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    file_id: String,
) -> CmdResult<Option<String>> {
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

    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_custom_image_png(&conn, id, &bytes).map_err(|e| e.to_string())?;

    Ok(Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    )))
}
```

- [ ] **Step 3: Command registrieren**

In `src-tauri/src/lib.rs`s `generate_handler!`-Liste nach `commands::mark_file_viewed,` einfügen:

```rust
            commands::mark_file_viewed,
            commands::upload_custom_image,
```

- [ ] **Step 4: i18n**

In `src/i18n/types.ts` nach `importSummaryText: string;` (letzte Zeile vor der schließenden `}`) ergänzen:

```ts
  importSummaryText: string;

  uploadModelImageLabel: string;
}
```

In allen vier Sprachdateien nach `importSummaryText: '...'` (letzte Zeile vor der schließenden `};`) ergänzen:

`src/i18n/de.ts`:
```ts
  importSummaryText: '{imported} importiert, {duplicates} Duplikate übersprungen',

  uploadModelImageLabel: 'Bild hochladen',
};
```

`src/i18n/en.ts`:
```ts
  importSummaryText: '{imported} imported, {duplicates} duplicates skipped',

  uploadModelImageLabel: 'Upload image',
};
```

`src/i18n/es.ts`:
```ts
  importSummaryText: '{imported} importados, {duplicates} duplicados omitidos',

  uploadModelImageLabel: 'Subir imagen',
};
```

`src/i18n/fr.ts`:
```ts
  importSummaryText: '{imported} importés, {duplicates} doublons ignorés',

  uploadModelImageLabel: 'Envoyer une image',
};
```

- [ ] **Step 5: DetailPanel - Upload-Button**

In `src/components/DetailPanel.tsx`s `Props`-Interface (aktuell Zeile 8-21) nach `onTogglePrintStatus: () => void;` ergänzen:

```ts
interface Props {
  model: ModelFile | null;
  onAddTag: (tag: string) => void;
  onRemoveTag: (tag: string) => void;
  onDelete: () => void;
  onTogglePrintStatus: () => void;
  onUploadImage: () => void;
  onOpenInSlicer: (slicerId?: string) => void;
  slicers: SlicerConfig[];
  slicerError: string | null;
  onUploadToCloud: () => void;
  cloudUploadAvailable: boolean;
  uploading: boolean;
  cloudUploadError: string | null;
}
```

Funktionssignatur (aktuell Zeile 52-65) entsprechend um `onUploadImage` erweitern (gleiche Position, nach `onTogglePrintStatus`).

Der 3D-Ansicht-Container (aktuell Zeile 116-128) bekommt den neuen Button:

```tsx
        <div className="relative aspect-[4/3] bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
          <div
            className="absolute inset-0"
            style={{
              backgroundImage:
                'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 11px)',
            }}
          />
          <ModelViewer fileId={model.id} />
          <div className="absolute left-2.5 bottom-2 font-mono-ui text-[9.5px] tracking-[0.08em] uppercase text-[var(--ink-3)] pointer-events-none">
            {t('dragToRotate')}
          </div>
          <button
            onClick={onUploadImage}
            className="absolute right-2.5 top-2.5 h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('uploadModelImageLabel')}
          </button>
        </div>
```

(`<ModelViewer fileId={model.id} />` bekommt in Task 4 zwei weitere Props - hier unverändert lassen.)

- [ ] **Step 6: App.tsx verdrahten**

In `src/App.tsx` nach `togglePrintStatus` (aktuell Zeile 274-282) eine neue Funktion einfügen:

```tsx
  const uploadCustomImage = (id: string) => {
    invoke<string | null>('upload_custom_image', { fileId: id })
      .then((displayImage) => {
        if (displayImage === null) return;
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, displayImage } : m)));
      })
      .catch((e) => {
        console.error('[custom-image] Hochladen fehlgeschlagen:', e);
      });
  };
```

`<DetailPanel ...>`-Aufruf (aktuell Zeile 383-396) um die neue Prop ergänzen:

```tsx
          <DetailPanel
            model={selected}
            onAddTag={(t) => selected && addTag(selected.id, t)}
            onRemoveTag={(t) => selected && removeTag(selected.id, t)}
            onDelete={() => selected && deleteModel(selected.id)}
            onTogglePrintStatus={() => selected && togglePrintStatus(selected.id)}
            onUploadImage={() => selected && uploadCustomImage(selected.id)}
            onOpenInSlicer={(slicerId) => selected && openInSlicer(selected.id, slicerId)}
            slicers={slicers}
            slicerError={slicerError}
            onUploadToCloud={() => selected && uploadWithFolderPicker(selected.id)}
            cloudUploadAvailable={clouds.some((c) => c.id === 'gdrive' && c.status === 'connected')}
            uploading={uploadingId !== null && uploadingId === selected?.id}
            cloudUploadError={cloudUploadError}
          />
```

- [ ] **Step 7: Testen**

Run: `cd src-tauri && cargo test` (Backend) und `npx tsc --noEmit` (Frontend)
Expected: beide sauber.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts src/components/DetailPanel.tsx src/App.tsx
git commit -m "$(cat <<'EOF'
Eigenes Bild pro Modell hochladen

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

## Task 4: Automatischer 3D-Snapshot

**Files:**
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/components/ModelViewer.tsx`
- Modify: `src/components/DetailPanel.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `ModelFile.displayImage` (Task 2) zur Bestimmung, ob ein Snapshot überhaupt noch gebraucht wird.
- Produces: Tauri-Command `set_render_snapshot(file_id: String, image_base64: String) -> CmdResult<()>`. `ModelViewer`-Props `needsSnapshot: boolean` und `onSnapshotCaptured: (base64: string) => void`.

- [ ] **Step 1: Repository-Funktion + Test**

In `src-tauri/src/db/repository.rs` nach `set_custom_image_png` einfügen:

```rust
pub fn set_render_snapshot_png(conn: &Connection, file_id: i64, png: &[u8]) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET render_snapshot_png = ?1 WHERE id = ?2",
        params![png, file_id],
    )?;
    Ok(())
}
```

In `src-tauri/src/db/mod.rs`s `pub use repository::{...}`-Liste `set_render_snapshot_png` alphabetisch einsortieren.

Neuen Test nach `set_custom_image_png_updates_the_image` einfügen:

```rust
    #[test]
    fn set_render_snapshot_png_updates_the_image() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        set_render_snapshot_png(&conn, id, &[7, 7, 7]).expect("update");
        let file = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(file.render_snapshot_png, Some(vec![7, 7, 7]));
    }
```

Run: `cd src-tauri && cargo test set_render_snapshot_png_updates_the_image`
Expected: PASS

- [ ] **Step 2: Command**

In `src-tauri/src/commands.rs` nach `upload_custom_image` einfügen:

```rust
#[tauri::command]
pub fn set_render_snapshot(state: State<AppState>, file_id: String, image_base64: String) -> CmdResult<()> {
    use base64::Engine;
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&image_base64)
        .map_err(|e| e.to_string())?;
    let conn = lock_db(&state)?;
    db::set_render_snapshot_png(&conn, id, &bytes).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Command registrieren**

In `src-tauri/src/lib.rs`s `generate_handler!`-Liste nach `commands::upload_custom_image,` einfügen:

```rust
            commands::upload_custom_image,
            commands::set_render_snapshot,
```

- [ ] **Step 4: ModelViewer erzeugt den Snapshot**

In `src/components/ModelViewer.tsx`s `Props`-Interface (aktuell Zeile 9-11) zwei Felder ergänzen:

```ts
interface Props {
  fileId: string;
  needsSnapshot: boolean;
  onSnapshotCaptured: (base64: string) => void;
}
```

Funktionssignatur (aktuell Zeile 69) entsprechend erweitern:

```tsx
export function ModelViewer({ fileId, needsSnapshot, onSnapshotCaptured }: Props) {
```

`WebGLRenderer`-Konstruktion (aktuell Zeile 86) bekommt `preserveDrawingBuffer: true` - ohne dieses Flag kann der Zeichenpuffer vor dem Auslesen per `toDataURL()` bereits gelöscht sein:

```tsx
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
```

Im Modellwechsel-Effekt (aktuell Zeile 146-183) wird nach `setStatus('ready');` (aktuell Zeile 173) die Snapshot-Erzeugung ergänzt. Zwei verschachtelte `requestAnimationFrame`-Aufrufe stellen sicher, dass der Renderer das neu geladene Objekt tatsächlich schon gezeichnet hat, bevor der Canvas ausgelesen wird (der laufende `animate()`-Loop rendert erst beim nächsten Frame, nicht synchron in diesem `.then()`):

```tsx
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

        if (needsSnapshot) {
          requestAnimationFrame(() => {
            requestAnimationFrame(() => {
              if (cancelled) return;
              try {
                const dataUrl = ctx.renderer.domElement.toDataURL('image/png');
                const base64 = dataUrl.split(',')[1];
                if (base64) onSnapshotCaptured(base64);
              } catch (err) {
                console.error('[ModelViewer] Snapshot fehlgeschlagen:', err);
              }
            });
          });
        }
      })
      .catch((err) => {
        console.error('[ModelViewer] Laden fehlgeschlagen:', err);
        if (!cancelled) setStatus('error');
      });
```

- [ ] **Step 5: DetailPanel reicht die neuen Props durch**

In `src/components/DetailPanel.tsx`s `Props`-Interface nach `onUploadImage: () => void;` (aus Task 3) ergänzen:

```ts
  onUploadImage: () => void;
  onSnapshotCaptured: (base64: string) => void;
```

Funktionssignatur entsprechend erweitern (nach `onUploadImage`, aus Task 3).

`<ModelViewer fileId={model.id} />` (aus Task 3, im 3D-Ansicht-Container) wird zu:

```tsx
          <ModelViewer
            fileId={model.id}
            needsSnapshot={model.displayImage === null}
            onSnapshotCaptured={onSnapshotCaptured}
          />
```

- [ ] **Step 6: App.tsx verdrahten**

In `src/App.tsx` nach `uploadCustomImage` (aus Task 3) eine neue Funktion einfügen:

```tsx
  const captureRenderSnapshot = (id: string, base64: string) => {
    const displayImage = `data:image/png;base64,${base64}`;
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, displayImage } : m)));
    invoke('set_render_snapshot', { fileId: id, imageBase64: base64 }).catch((e) => {
      console.error('[render-snapshot] Speichern fehlgeschlagen:', e);
    });
  };
```

`<DetailPanel ...>`-Aufruf um die neue Prop ergänzen (nach `onUploadImage`, aus Task 3):

```tsx
            onUploadImage={() => selected && uploadCustomImage(selected.id)}
            onSnapshotCaptured={(base64) => selected && captureRenderSnapshot(selected.id, base64)}
```

- [ ] **Step 7: Testen**

Run: `cd src-tauri && cargo test` (Backend) und `npx tsc --noEmit` (Frontend)
Expected: beide sauber.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/components/ModelViewer.tsx src/components/DetailPanel.tsx src/App.tsx
git commit -m "$(cat <<'EOF'
Automatischer 3D-Snapshot als Fallback-Thumbnail

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

## Task 5: Quelle-URL pro Modell

**Files:**
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types/index.ts`
- Modify: `src/i18n/types.ts`
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`
- Modify: `src/components/DetailPanel.tsx`
- Modify: `src/App.tsx`
- Modify: `CHANGELOG.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: `ModelFileDto.source_url` (Task 1, bereits ans Frontend ausgestellt, aber bisher ungenutzt).
- Produces: Tauri-Command `set_source_url(file_id: String, url: Option<String>) -> CmdResult<()>`. `ModelFile.sourceUrl: string | null`.

- [ ] **Step 1: Repository-Funktion + Test**

In `src-tauri/src/db/repository.rs` nach `set_render_snapshot_png` einfügen:

```rust
pub fn set_source_url(conn: &Connection, file_id: i64, url: Option<&str>) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET source_url = ?1 WHERE id = ?2",
        params![url, file_id],
    )?;
    Ok(())
}
```

In `src-tauri/src/db/mod.rs`s `pub use repository::{...}`-Liste `set_source_url` alphabetisch einsortieren.

Neuen Test nach `set_render_snapshot_png_updates_the_image` einfügen:

```rust
    #[test]
    fn set_source_url_updates_and_clears_the_url() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        set_source_url(&conn, id, Some("https://example.com/model")).expect("set");
        let file = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(file.source_url, Some("https://example.com/model".to_string()));

        set_source_url(&conn, id, None).expect("clear");
        let file = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(file.source_url, None);
    }
```

Run: `cd src-tauri && cargo test set_source_url_updates_and_clears_the_url`
Expected: PASS

- [ ] **Step 2: Command**

In `src-tauri/src/commands.rs` nach `set_render_snapshot` einfügen:

```rust
#[tauri::command]
pub fn set_source_url(state: State<AppState>, file_id: String, url: Option<String>) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_source_url(&conn, id, url.as_deref()).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Command registrieren**

In `src-tauri/src/lib.rs`s `generate_handler!`-Liste nach `commands::set_render_snapshot,` einfügen:

```rust
            commands::set_render_snapshot,
            commands::set_source_url,
```

- [ ] **Step 4: Frontend-Typ + i18n**

In `src/types/index.ts`s `ModelFile`-Interface nach `displayImage: string | null;` (aus Task 2) ergänzen:

```ts
  displayImage: string | null;
  sourceUrl: string | null;
```

In `src/i18n/types.ts` nach `uploadModelImageLabel: string;` (aus Task 3, letzte Zeile vor `}`) ergänzen:

```ts
  uploadModelImageLabel: string;

  metaSourceUrl: string;
  sourceUrlPlaceholder: string;
}
```

In allen vier Sprachdateien nach `uploadModelImageLabel: '...'` (aus Task 3) ergänzen:

`src/i18n/de.ts`:
```ts
  uploadModelImageLabel: 'Bild hochladen',

  metaSourceUrl: 'Quelle',
  sourceUrlPlaceholder: 'https://…',
};
```

`src/i18n/en.ts`:
```ts
  uploadModelImageLabel: 'Upload image',

  metaSourceUrl: 'Source',
  sourceUrlPlaceholder: 'https://…',
};
```

`src/i18n/es.ts`:
```ts
  uploadModelImageLabel: 'Subir imagen',

  metaSourceUrl: 'Fuente',
  sourceUrlPlaceholder: 'https://…',
};
```

`src/i18n/fr.ts`:
```ts
  uploadModelImageLabel: 'Envoyer une image',

  metaSourceUrl: 'Source',
  sourceUrlPlaceholder: 'https://…',
};
```

- [ ] **Step 5: DetailPanel - Quelle-Zeile**

In `src/components/DetailPanel.tsx`s `Props`-Interface nach `onSnapshotCaptured: (base64: string) => void;` (aus Task 4) ergänzen:

```ts
  onSnapshotCaptured: (base64: string) => void;
  onSetSourceUrl: (url: string | null) => void;
```

Funktionssignatur entsprechend erweitern (nach `onSnapshotCaptured`, aus Task 4).

Neuer lokaler State nach `const [slicerMenuOpen, setSlicerMenuOpen] = useState(false);` (aktuell Zeile 70):

```tsx
  const [slicerMenuOpen, setSlicerMenuOpen] = useState(false);
  const [editingSourceUrl, setEditingSourceUrl] = useState(false);
  const [sourceUrlDraft, setSourceUrlDraft] = useState('');
```

Der bestehende Reset-Effekt (aktuell Zeile 72-75) bekommt eine dritte Zeile:

```tsx
  useEffect(() => {
    setConfirmDelete(false);
    setSlicerMenuOpen(false);
    setEditingSourceUrl(false);
  }, [model?.id]);
```

Nach `submitDraft` (aktuell endet Zeile 89) zwei neue Funktionen einfügen (model ist an dieser Stelle bereits als nicht-null garantiert, da der `if (!model)`-Early-Return in Zeile 77-83 davor liegt):

```tsx
  const startEditingSourceUrl = () => {
    setSourceUrlDraft(model.sourceUrl ?? '');
    setEditingSourceUrl(true);
  };

  const submitSourceUrl = () => {
    const value = sourceUrlDraft.trim();
    onSetSourceUrl(value || null);
    setEditingSourceUrl(false);
  };
```

Direkt nach dem `{buildMetaRows(model, t, language).map((row) => (...))}`-Block (aktuell Zeile 158-168, innerhalb desselben `<div className="px-4 pt-3.5 pb-1">`-Containers) eine neue, interaktive Zeile ergänzen:

```tsx
          {buildMetaRows(model, t, language).map((row) => (
            <div
              key={row.label}
              className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]"
            >
              <span className="flex-none w-[108px] text-[12.5px] text-[var(--ink-2)]">
                {row.label}
              </span>
              <span className="flex-1 font-mono-ui text-xs text-right">{row.value}</span>
            </div>
          ))}
          <div className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]">
            <span className="flex-none w-[108px] text-[12.5px] text-[var(--ink-2)]">
              {t('metaSourceUrl')}
            </span>
            <span className="flex-1 flex items-center justify-end gap-1.5 min-w-0 font-mono-ui text-xs">
              {editingSourceUrl ? (
                <input
                  value={sourceUrlDraft}
                  onChange={(e) => setSourceUrlDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') submitSourceUrl();
                    if (e.key === 'Escape') setEditingSourceUrl(false);
                  }}
                  onBlur={submitSourceUrl}
                  autoFocus
                  placeholder={t('sourceUrlPlaceholder')}
                  className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 font-mono-ui text-xs"
                />
              ) : model.sourceUrl ? (
                <a
                  href={model.sourceUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="flex-1 min-w-0 truncate text-right text-[var(--accent)] hover:underline"
                >
                  {model.sourceUrl}
                </a>
              ) : (
                <span className="flex-1 text-right text-[var(--ink-3)]">{t('noValue')}</span>
              )}
              <span
                onClick={startEditingSourceUrl}
                className="flex-none w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[10px] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
              >
                ✎
              </span>
            </span>
          </div>
```

- [ ] **Step 6: App.tsx verdrahten**

In `src/App.tsx` nach `captureRenderSnapshot` (aus Task 4) eine neue Funktion einfügen:

```tsx
  const setModelSourceUrl = (id: string, url: string | null) => {
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, sourceUrl: url } : m)));
    invoke('set_source_url', { fileId: id, url }).catch((e) => {
      console.error('[source-url] Speichern fehlgeschlagen:', e);
    });
  };
```

`<DetailPanel ...>`-Aufruf um die neue Prop ergänzen (nach `onSnapshotCaptured`, aus Task 4):

```tsx
            onSnapshotCaptured={(base64) => selected && captureRenderSnapshot(selected.id, base64)}
            onSetSourceUrl={(url) => selected && setModelSourceUrl(selected.id, url)}
```

- [ ] **Step 7: Testen**

Run: `cd src-tauri && cargo test` (Backend) und `npx tsc --noEmit` (Frontend)
Expected: beide sauber.

- [ ] **Step 8: CHANGELOG.md und README.md aktualisieren**

In `CHANGELOG.md`s `### Added`-Abschnitt unter `[Unreleased]` (letzte bestehende Zeile im Abschnitt) ergänzen:

```markdown
- Modell-Thumbnails im Raster: Bild-Priorität eigenes Upload > eingebettetes 3MF-Thumbnail > automatisch erzeugter 3D-Snapshot (einmalig beim ersten Ansehen im Detailbereich, client-seitig aus der bestehenden Live-Vorschau erzeugt) > Platzhalter; zusätzlich pro Modell eine Quelle als Link hinterlegbar
```

In `README.md`s Abschnitt "Funktionen" nach der zuletzt hinzugefügten Zeile ("Katalog-Erweiterungen ...") ergänzen:

```markdown
- **Modell-Thumbnails** — Raster-Ansicht zeigt ein echtes Bild pro Modell (eigenes Upload, eingebettetes 3MF-Thumbnail oder automatisch aus der 3D-Live-Vorschau erzeugter Snapshot), zusätzlich pro Modell eine Quelle als Link hinterlegbar
```

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/types/index.ts src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts src/components/DetailPanel.tsx src/App.tsx CHANGELOG.md README.md
git commit -m "$(cat <<'EOF'
Quelle-URL pro Modell hinterlegbar

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01F4WhGwB4Z3rSnNVUDjo25V
EOF
)"
```

---

## Nach Abschluss aller Tasks

Nach der finalen Whole-Branch-Review (per subagent-driven-development) und vor dem Push: manueller Live-Test im laufenden `npm run tauri dev` (gemeinsam mit dem Nutzer, da dieses Environment keine zuverlässige synthetische Maussteuerung hat):

- Ein Modell mit eingebettetem 3MF-Thumbnail prüfen: erscheint es sofort im Grid, ohne die 3D-Ansicht je geöffnet zu haben?
- Ein Modell ohne eingebettetes Thumbnail einmal im Detailbereich ansehen, dann zurück zum Grid wechseln: erscheint danach ein automatischer Snapshot?
- Für ein Modell ein eigenes Bild hochladen: überschreibt es sowohl den Platzhalter als auch ein vorhandenes eingebettetes/Snapshot-Bild im Grid?
- Eine Quelle-URL setzen, Seite verlassen und zurückkommen (Modell erneut auswählen): bleibt die URL erhalten und ist der Link klickbar?

Nach erfolgreichem Live-Test: `git push origin master` (vom Nutzer bereits autorisiert).
