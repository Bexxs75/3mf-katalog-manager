# Katalog-Backup (Export/Import) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Den kompletten Katalogzustand (SQLite-DB + fünf `localStorage`-Einstellungen) als ZIP-Datei exportieren und auf einer anderen Maschine/nach einer Neuinstallation wieder importieren können.

**Architecture:** Export nutzt SQLite's Online-Backup-API für eine konsistente Kopie der laufenden DB, packt sie zusammen mit den Frontend-Settings in eine ZIP-Datei. Import validiert das Archiv, ersetzt `catalog.db` sicher (alte Datei wird zu `.bak-<Zeitstempel>` statt gelöscht, laufende Connection wird vor dem Umbenennen durch eine In-Memory-Platzhalter-Connection ersetzt, damit das OS-Datei-Handle wirklich freigegeben wird), und verlangt einen App-Neustart.

**Tech Stack:** Rust/Tauri Backend (rusqlite Online-Backup-API, `zip`-Crate), React/TypeScript Frontend.

## Global Constraints

- Es werden **keine** 3MF/STL-Modelldateien mitgesichert, nur Katalog-Metadaten (DB + Settings) — siehe Spec.
- Import darf den bestehenden Katalogzustand niemals ersatzlos zerstören: die alte `catalog.db` wird umbenannt (`.bak-<Zeitstempel>`), nie gelöscht.
- Ein ungültiges/korruptes Import-Archiv muss **vor** jeder Änderung am bestehenden Zustand erkannt und abgelehnt werden.
- Kein Hot-Swap der laufenden DB-Verbindung — Import verlangt explizit einen App-Neustart, kein Live-Reconnect-Code.
- Alte `.bak-*`-Dateien werden nicht automatisch aufgeräumt (bewusst minimal, siehe Spec).

---

### Task 1: Backend — `export_catalog` Command

**Files:**
- Modify: `src-tauri/Cargo.toml` (rusqlite `backup`-Feature aktivieren)
- Modify: `src-tauri/src/commands.rs` (neuer Command, `AppState` bekommt `db_path`)
- Modify: `src-tauri/src/lib.rs` (`db_path` in `AppState` befüllen, Command registrieren)

**Interfaces:**
- Produces: `AppState.db_path: std::path::PathBuf`, `#[tauri::command] pub async fn export_catalog(app: tauri::AppHandle, state: State<'_, AppState>, settings_json: String) -> CmdResult<()>`.

- [ ] **Step 1: Enable the rusqlite `backup` feature**

In `src-tauri/Cargo.toml`, ändere:

```toml
rusqlite = { version = "0.40.2", features = ["bundled"] }
```

zu:

```toml
rusqlite = { version = "0.40.2", features = ["bundled", "backup"] }
```

- [ ] **Step 2: Add `db_path` to `AppState`**

In `src-tauri/src/commands.rs`, ändere:

```rust
pub struct AppState {
    pub db: Mutex<Connection>,
    pub trash_dir: std::path::PathBuf,
}
```

zu:

```rust
pub struct AppState {
    pub db: Mutex<Connection>,
    pub trash_dir: std::path::PathBuf,
    pub db_path: std::path::PathBuf,
}
```

In `src-tauri/src/lib.rs`, in der `.setup(|app| { ... })`-Closure, ändere die `app.manage(...)`-Konstruktion:

```rust
            app.manage(commands::AppState {
                db: Mutex::new(conn),
                trash_dir,
            });
```

zu:

```rust
            app.manage(commands::AppState {
                db: Mutex::new(conn),
                trash_dir,
                db_path,
            });
```

(`db_path` ist im selben Scope bereits als `let db_path = app_data_dir.join("catalog.db");` vorhanden — keine weitere Änderung nötig.)

- [ ] **Step 3: Write `export_catalog`**

In `src-tauri/src/commands.rs`, füge irgendwo nach der bestehenden `delete_saved_filter`- oder einer anderen Top-Level-Command-Funktion hinzu:

```rust
/// Exportiert den kompletten Katalogzustand (DB + Frontend-Settings) als
/// ZIP-Datei. Nutzt SQLite's Online-Backup-API statt eines rohen
/// Datei-Kopierens fuer die DB-Kopie: die laufende Connection kann im
/// WAL-Modus sein, ein fs::copy koennte eine inkonsistente Zwischenstufe
/// der Datei erwischen.
#[tauri::command]
pub async fn export_catalog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings_json: String,
) -> CmdResult<()> {
    use std::io::Write;

    let picked = app
        .dialog()
        .file()
        .add_filter("ZIP-Archiv", &["zip"])
        .set_file_name(format!(
            "3mf-katalog-backup_{}.zip",
            chrono::Utc::now().format("%Y-%m-%d")
        ))
        .blocking_save_file();

    let Some(picked) = picked else {
        return Ok(());
    };
    let dest_path = picked.into_path().map_err(|e| e.to_string())?;

    let backup_db_path =
        std::env::temp_dir().join(format!("3mf-katalog-export-{}.db", std::process::id()));
    {
        let conn = lock_db(&state)?;
        let mut dst = Connection::open(&backup_db_path).map_err(|e| e.to_string())?;
        let backup = rusqlite::backup::Backup::new(&conn, &mut dst).map_err(|e| e.to_string())?;
        backup
            .run_to_completion(5, std::time::Duration::from_millis(250), None)
            .map_err(|e| e.to_string())?;
    }

    let tmp_zip_path = dest_path.with_extension("zip.tmp");
    {
        let zip_file = std::fs::File::create(&tmp_zip_path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(zip_file);
        let options = zip::write::SimpleFileOptions::default();

        zip.start_file("catalog.db", options).map_err(|e| e.to_string())?;
        let db_bytes = std::fs::read(&backup_db_path).map_err(|e| e.to_string())?;
        zip.write_all(&db_bytes).map_err(|e| e.to_string())?;

        zip.start_file("settings.json", options).map_err(|e| e.to_string())?;
        zip.write_all(settings_json.as_bytes()).map_err(|e| e.to_string())?;

        zip.finish().map_err(|e| e.to_string())?;
    }
    let _ = std::fs::remove_file(&backup_db_path);

    // Zip erst nach vollstaendigem, erfolgreichem Schreiben an den
    // eigentlichen Zielpfad verschieben - kein unvollstaendiges Archiv am
    // sichtbaren Zielort, falls das Packen mittendrin fehlschlaegt.
    std::fs::rename(&tmp_zip_path, &dest_path).map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 4: Register the command**

In `src-tauri/src/lib.rs`, im `tauri::generate_handler![...]`-Block, nach `commands::rescan_file_metadata,`:

```rust
            commands::rescan_file_metadata,
            commands::export_catalog,
```

- [ ] **Step 5: Run tests and build**

Run: `cd src-tauri && cargo test && cargo build`
Expected: alle Tests PASS (keine neuen Tests in diesem Task — Export braucht einen echten Datei-Dialog und wird in Task 3 end-to-end über das Frontend getestet; ein reiner Unit-Test für `export_catalog` würde nur die Dialog-Absage-Kurzschluss-Pruefung `if picked.is_none()` abdecken koennen, was keinen echten Mehrwert hat), Build erfolgreich (bestaetigt insbesondere, dass die neu aktivierte `backup`-Cargo-Feature kompiliert).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "Katalog-Export: export_catalog Command (DB-Backup + Settings als ZIP)"
```

---

### Task 2: Backend — `import_catalog` Command

**Files:**
- Modify: `src-tauri/src/commands.rs` (neuer Command + DTO)
- Modify: `src-tauri/src/lib.rs` (Command registrieren)

**Interfaces:**
- Consumes: `AppState.db_path` (Task 1).
- Produces: `pub struct ImportCatalogResultDto { pub imported: bool, pub settings_json: Option<String> }`, `#[tauri::command] pub async fn import_catalog(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult<ImportCatalogResultDto>`.

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/commands.rs`, in `mod tests`, füge hinzu (baut ein echtes Test-ZIP mit einer minimalen gültigen SQLite-DB und prüft die Kernlogik der Validierung isoliert von den Datei-Dialogen — die volle Funktion `import_catalog` selbst braucht `AppHandle`/Dialog und wird nicht direkt unit-getestet, siehe Step 3 unten für die Begründung):

```rust
#[test]
fn validate_catalog_db_bytes_accepts_a_real_sqlite_database_with_files_table() {
    let tmp_path = std::env::temp_dir().join(format!(
        "validate_catalog_db_test_{}.db",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    {
        let conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
        drop(conn);
    }
    let bytes = std::fs::read(&tmp_path).expect("read temp db");

    let result = validate_catalog_db_bytes(&bytes);

    let _ = std::fs::remove_file(&tmp_path);
    assert!(result.is_ok(), "expected valid catalog db to pass validation: {result:?}");
}

#[test]
fn validate_catalog_db_bytes_rejects_garbage_bytes() {
    let result = validate_catalog_db_bytes(b"this is not a sqlite database");
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test validate_catalog_db_bytes`
Expected: FAIL with "cannot find function `validate_catalog_db_bytes`".

- [ ] **Step 3: Extract the validation helper and write `import_catalog`**

Warum `validate_catalog_db_bytes` als eigene Funktion: der Rest von
`import_catalog` braucht `tauri::AppHandle` (für den Datei-Dialog) und
`State<AppState>` (für die laufende Connection) und lässt sich daher nicht
sinnvoll in einem reinen `#[test]` aufrufen — die eigentliche
Validierungslogik (liest rohe DB-Bytes, prüft ob es eine brauchbare
Katalog-DB ist) wird deshalb als separate, reine Funktion herausgezogen und
isoliert getestet (Step 1/2), der Rest des Commands bleibt dünne
Verdrahtung ohne eigenen Test (gleiches Muster wie bei anderen
Dialog-abhängigen Commands in dieser Datei, z. B. `pick_and_read_image`).

In `src-tauri/src/commands.rs`, füge hinzu:

```rust
/// Prueft, ob `bytes` eine brauchbare Katalog-Datenbank sind (oeffnbar und
/// mit einer `files`-Tabelle) - Schutz davor, ein falsches/kaputtes ZIP zu
/// importieren, BEVOR die bestehende catalog.db angefasst wird.
fn validate_catalog_db_bytes(bytes: &[u8]) -> Result<(), String> {
    let tmp_path = std::env::temp_dir().join(format!(
        "3mf-katalog-import-check-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    std::fs::write(&tmp_path, bytes).map_err(|e| e.to_string())?;

    let result = Connection::open(&tmp_path)
        .map_err(|e| e.to_string())
        .and_then(|conn| {
            conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get::<_, i64>(0))
                .map(|_| ())
                .map_err(|e| e.to_string())
        });

    let _ = std::fs::remove_file(&tmp_path);
    result.map_err(|e| format!("Archiv enthält keine gültige Katalog-Datenbank: {e}"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCatalogResultDto {
    pub imported: bool,
    pub settings_json: Option<String>,
}

/// Importiert einen per `export_catalog` erzeugten Katalog-Export. Ersetzt
/// die laufende `catalog.db` NUR nach erfolgreicher Validierung (siehe
/// `validate_catalog_db_bytes`); die alte Datei wird zu `.bak-<Zeitstempel>`
/// umbenannt statt geloescht. Die laufende `Connection` in `AppState` wird
/// vor dem Umbenennen durch eine In-Memory-Platzhalter-Connection ersetzt -
/// ein blosses Freigeben des Mutex-Locks wuerde das zugrundeliegende
/// Datei-Handle NICHT schliessen, was auf Windows das nachfolgende
/// `fs::rename` mit einer Sharing-Violation zum Scheitern braechte. Ein
/// Neustart der App ist danach erforderlich, um die neue DB zu laden (kein
/// Live-Reconnect vorgesehen, siehe Spec).
#[tauri::command]
pub async fn import_catalog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ImportCatalogResultDto> {
    use std::io::Read;

    let picked = app.dialog().file().add_filter("ZIP-Archiv", &["zip"]).blocking_pick_file();
    let Some(picked) = picked else {
        return Ok(ImportCatalogResultDto { imported: false, settings_json: None });
    };
    let archive_path = picked.into_path().map_err(|e| e.to_string())?;

    let file = std::fs::File::open(&archive_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut db_bytes = Vec::new();
    archive
        .by_name("catalog.db")
        .map_err(|_| "Archiv enthält keine catalog.db".to_string())?
        .read_to_end(&mut db_bytes)
        .map_err(|e| e.to_string())?;

    let mut settings_bytes = Vec::new();
    archive
        .by_name("settings.json")
        .map_err(|_| "Archiv enthält keine settings.json".to_string())?
        .read_to_end(&mut settings_bytes)
        .map_err(|e| e.to_string())?;
    let settings_json = String::from_utf8(settings_bytes).map_err(|e| e.to_string())?;

    validate_catalog_db_bytes(&db_bytes)?;

    let tmp_db_path =
        std::env::temp_dir().join(format!("3mf-katalog-import-{}.db", std::process::id()));
    std::fs::write(&tmp_db_path, &db_bytes).map_err(|e| e.to_string())?;

    {
        let mut guard = lock_db(&state)?;
        let placeholder = Connection::open_in_memory().map_err(|e| e.to_string())?;
        *guard = placeholder; // alte Connection droppt hier -> OS-Handle auf catalog.db wird geschlossen
    }

    let backup_path = state.db_path.with_file_name(format!(
        "catalog.db.bak-{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    ));
    std::fs::rename(&state.db_path, &backup_path).map_err(|e| e.to_string())?;

    if let Err(e) = std::fs::copy(&tmp_db_path, &state.db_path) {
        let _ = std::fs::rename(&backup_path, &state.db_path);
        let _ = std::fs::remove_file(&tmp_db_path);
        return Err(format!(
            "Kopieren der neuen Datenbank fehlgeschlagen, alter Katalog wiederhergestellt: {e}"
        ));
    }
    let _ = std::fs::remove_file(&tmp_db_path);

    Ok(ImportCatalogResultDto { imported: true, settings_json: Some(settings_json) })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test validate_catalog_db_bytes`
Expected: both tests PASS.

- [ ] **Step 5: Register the command**

In `src-tauri/src/lib.rs`, im `tauri::generate_handler![...]`-Block, nach `commands::export_catalog,`:

```rust
            commands::export_catalog,
            commands::import_catalog,
```

- [ ] **Step 6: Run the full backend test suite and build**

Run: `cd src-tauri && cargo test && cargo build`
Expected: alle Tests PASS, Build erfolgreich.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "Katalog-Import: import_catalog Command (validiert, sichert alte DB, ersetzt)"
```

---

### Task 3: Frontend — Buttons, Settings-Sammlung, Restart-Hinweis

**Files:**
- Modify: `src/components/Header.tsx` (neue Sektion im Einstellungen-Panel + Bestätigungsdialog vor Import)
- Create: `src/components/CatalogImportRestartBanner.tsx`
- Modify: `src/App.tsx` (Handler, State, Banner-Wiring)
- Modify: `src/i18n/types.ts`, `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`

**Interfaces:**
- Consumes: Tauri-Commands `export_catalog(settingsJson: string): Promise<void>`, `import_catalog(): Promise<{ imported: boolean; settingsJson: string | null }>` (Task 1/2).
- Produces: `Header` Props `onExportCatalog: () => void`, `onImportCatalog: () => void`, `catalogBackupError: string | null`.

- [ ] **Step 1: Add i18n keys**

In `src/i18n/types.ts`, nach `rescanMetadataSuccess: string;`:

```ts
  rescanMetadataSuccess: string;
  catalogBackupTitle: string;
  exportCatalogButton: string;
  importCatalogButton: string;
  importCatalogConfirmQuestion: string;
  importCatalogConfirmYes: string;
  importCatalogRestartHint: string;
```

In `src/i18n/de.ts`, nach `rescanMetadataSuccess: 'Metadaten aktualisiert.',`:

```ts
  rescanMetadataSuccess: 'Metadaten aktualisiert.',
  catalogBackupTitle: 'Katalog-Backup',
  exportCatalogButton: 'Katalog exportieren',
  importCatalogButton: 'Katalog importieren',
  importCatalogConfirmQuestion: 'Ersetzt den kompletten aktuellen Katalog. Fortfahren?',
  importCatalogConfirmYes: 'Ja, ersetzen',
  importCatalogRestartHint: 'Katalog importiert. Bitte die App jetzt neu starten, damit die Änderungen wirksam werden.',
```

In `src/i18n/en.ts`, nach `rescanMetadataSuccess: 'Metadata updated.',`:

```ts
  rescanMetadataSuccess: 'Metadata updated.',
  catalogBackupTitle: 'Catalog Backup',
  exportCatalogButton: 'Export catalog',
  importCatalogButton: 'Import catalog',
  importCatalogConfirmQuestion: 'This replaces your entire current catalog. Continue?',
  importCatalogConfirmYes: 'Yes, replace',
  importCatalogRestartHint: 'Catalog imported. Please restart the app now for the changes to take effect.',
```

In `src/i18n/es.ts`, nach `rescanMetadataSuccess: 'Metadatos actualizados.',`:

```ts
  rescanMetadataSuccess: 'Metadatos actualizados.',
  catalogBackupTitle: 'Copia de seguridad del catálogo',
  exportCatalogButton: 'Exportar catálogo',
  importCatalogButton: 'Importar catálogo',
  importCatalogConfirmQuestion: 'Esto reemplaza todo tu catálogo actual. ¿Continuar?',
  importCatalogConfirmYes: 'Sí, reemplazar',
  importCatalogRestartHint: 'Catálogo importado. Reinicia la aplicación ahora para que los cambios surtan efecto.',
```

In `src/i18n/fr.ts`, nach `rescanMetadataSuccess: 'Métadonnées mises à jour.',`:

```ts
  rescanMetadataSuccess: 'Métadonnées mises à jour.',
  catalogBackupTitle: 'Sauvegarde du catalogue',
  exportCatalogButton: 'Exporter le catalogue',
  importCatalogButton: 'Importer le catalogue',
  importCatalogConfirmQuestion: 'Cela remplace tout votre catalogue actuel. Continuer ?',
  importCatalogConfirmYes: 'Oui, remplacer',
  importCatalogRestartHint: 'Catalogue importé. Veuillez redémarrer l\'application maintenant pour que les modifications prennent effet.',
```

- [ ] **Step 2: Create the restart banner component**

Create `src/components/CatalogImportRestartBanner.tsx`:

```tsx
import { useT } from '../i18n/LanguageContext';

interface Props {
  onClose: () => void;
}

export function CatalogImportRestartBanner({ onClose }: Props) {
  const t = useT();

  return (
    <div className="fixed bottom-4 right-4 z-50 flex items-center gap-2.5 px-3.5 py-2.5 rounded-[4px] border border-[var(--accent)] bg-[var(--panel)] shadow-[var(--shadow)] text-[length:var(--font-size-title)] text-[var(--ink)] max-w-[360px]">
      <span>{t('importCatalogRestartHint')}</span>
      <span
        onClick={onClose}
        className="w-4 h-4 flex-none grid place-items-center rounded-full cursor-pointer text-[length:var(--font-size-meta)] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
      >
        ✕
      </span>
    </div>
  );
}
```

(Bewusst ohne Auto-Dismiss-Timer, anders als `ImportSummaryBanner` — der Hinweis ist handlungsrelevant, der Nutzer soll ihn nicht verpassen.)

- [ ] **Step 3: Add the settings-collection + handlers in `App.tsx`**

In `src/App.tsx`, ergänze nach dem Import von `ImportSummaryBanner`:

```ts
import { CatalogImportRestartBanner } from './components/CatalogImportRestartBanner';
```

Ergänze nach der bestehenden `rescanFeedback`-State-Deklaration:

```ts
  const [catalogBackupError, setCatalogBackupError] = useState<string | null>(null);
  const [showImportRestartBanner, setShowImportRestartBanner] = useState(false);

  const CATALOG_SETTINGS_KEYS = [
    '3mf-katalog-theme',
    '3mf-katalog-display-preference',
    '3mf-katalog-language',
    '3mf-katalog-slicers',
    '3mf-katalog-density',
  ] as const;

  const exportCatalog = () => {
    setCatalogBackupError(null);
    const settings: Record<string, string | null> = {};
    for (const key of CATALOG_SETTINGS_KEYS) {
      settings[key] = localStorage.getItem(key);
    }
    invoke('export_catalog', { settingsJson: JSON.stringify(settings) }).catch((e) => {
      console.error('[catalog-backup] Export fehlgeschlagen:', e);
      setCatalogBackupError(String(e));
    });
  };

  const importCatalog = () => {
    setCatalogBackupError(null);
    invoke<{ imported: boolean; settingsJson: string | null }>('import_catalog')
      .then((result) => {
        if (!result.imported || !result.settingsJson) return;
        const settings = JSON.parse(result.settingsJson) as Record<string, string | null>;
        for (const key of CATALOG_SETTINGS_KEYS) {
          const value = settings[key];
          if (value === null || value === undefined) {
            localStorage.removeItem(key);
          } else {
            localStorage.setItem(key, value);
          }
        }
        setShowImportRestartBanner(true);
      })
      .catch((e) => {
        console.error('[catalog-backup] Import fehlgeschlagen:', e);
        setCatalogBackupError(String(e));
      });
  };
```

Ergänze im `<Header ... />`-Aufruf (nach `cleanupError={cleanupError}`):

```tsx
        cleanupError={cleanupError}
        onExportCatalog={exportCatalog}
        onImportCatalog={importCatalog}
        catalogBackupError={catalogBackupError}
```

Ergänze im JSX nach dem bestehenden `{importBanner && (<ImportSummaryBanner .../>)}`-Block:

```tsx
      {showImportRestartBanner && (
        <CatalogImportRestartBanner onClose={() => setShowImportRestartBanner(false)} />
      )}
```

- [ ] **Step 4: Add the UI section in `Header.tsx`**

Erweitere das `Props`-Interface in `src/components/Header.tsx` (nach `cleanupError: string | null;`):

```ts
  cleanupError: string | null;
  onExportCatalog: () => void;
  onImportCatalog: () => void;
  catalogBackupError: string | null;
```

Destrukturiere entsprechend in der Funktionssignatur (nach `cleanupError,`):

```ts
  cleanupError,
  onExportCatalog,
  onImportCatalog,
  catalogBackupError,
```

Füge lokalen State für die Import-Bestätigung hinzu (bei den anderen `useState`-Deklarationen der Komponente):

```ts
  const [confirmImportCatalog, setConfirmImportCatalog] = useState(false);
```

Füge im Einstellungen-Panel, direkt vor dem schließenden `</div>` (nach dem bestehenden `{cleanupError && (...)}`-Block, also als letzter Inhalt vor dem Panel-Ende), die neue Sektion ein:

```tsx
            <div className="text-[length:var(--font-size-body)] font-semibold mt-4 mb-2">{t('catalogBackupTitle')}</div>
            <button
              onClick={onExportCatalog}
              className="h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('exportCatalogButton')}
            </button>
            {confirmImportCatalog ? (
              <div className="mt-1.5 flex flex-col gap-1.5">
                <div className="font-mono-ui text-[10.5px] text-[var(--ink-2)]">
                  {t('importCatalogConfirmQuestion')}
                </div>
                <div className="flex gap-1.5">
                  <button
                    onClick={() => {
                      setConfirmImportCatalog(false);
                      onImportCatalog();
                    }}
                    className="flex-1 h-7 rounded-[3px] border border-red-400 bg-transparent text-red-400 text-[12px] cursor-pointer hover:bg-red-400/10"
                  >
                    {t('importCatalogConfirmYes')}
                  </button>
                  <button
                    onClick={() => setConfirmImportCatalog(false)}
                    className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)]"
                  >
                    {t('cancel')}
                  </button>
                </div>
              </div>
            ) : (
              <button
                onClick={() => setConfirmImportCatalog(true)}
                className="mt-1.5 h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('importCatalogButton')}
              </button>
            )}
            {catalogBackupError && (
              <div className="mt-1.5 font-mono-ui text-[length:var(--font-size-meta)] text-[var(--accent)] break-words">
                {catalogBackupError}
              </div>
            )}
```

(`t('cancel')` existiert bereits als i18n-Key, verwendet u. a. beim Slicer-Hinzufügen-Formular in derselben Datei.)

- [ ] **Step 5: Typecheck**

Run: `npx tsc --noEmit` (vom Projekt-Root)
Expected: keine Fehler.

- [ ] **Step 6: Manual verification**

Run: `npm run tauri dev`. "Katalog exportieren" klicken, Speicherort wählen, prüfen dass eine `.zip` mit `catalog.db` + `settings.json` entsteht (z. B. `unzip -l`). "Katalog importieren" klicken, dieselbe Datei wählen, Bestätigung bestätigen, prüfen dass der Neustart-Hinweis erscheint und im App-Datenverzeichnis eine `catalog.db.bak-*`-Datei neben der (kopierten) neuen `catalog.db` liegt. App neu starten, prüfen dass der Katalog wie zuvor geladen wird (da hier dieselbe Datei re-importiert wurde, sollte sich nichts sichtbar ändern — das bestätigt lediglich, dass der Roundtrip nicht kaputt geht).

- [ ] **Step 7: Commit**

```bash
git add src/components/Header.tsx src/components/CatalogImportRestartBanner.tsx src/App.tsx src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "Frontend: Katalog-Export/-Import im Einstellungen-Panel"
```
