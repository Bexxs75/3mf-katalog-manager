# Cloud-Anbindung Google Drive — Datei-Browsing, Import & Änderungserkennung Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Google-Drive-Dateien über einen Browser-Dialog durchsuchen und in den Katalog importieren (parsen, Metadaten/Thumbnail extrahieren, wie beim lokalen Import), plus On-Demand-Änderungserkennung beim Öffnen einer Cloud-Datei in der Detailansicht. **Noch nicht Teil dieses Plans:** Hochladen lokaler Dateien zu Google Drive — das ist eine spätere, dritte Runde auf demselben Fundament.

**Architecture:** `GoogleDriveProvider` implementiert das in Plan 1 definierte `StorageProvider`-Trait gegen die echte Google Drive REST API v3 (List/Get-Metadata/Download; Upload bleibt vorerst ein Stub). Eine neue Session-Schicht (`cloud/session.rs`) kapselt Token-Laden + automatischen Refresh-bei-401 + Retry, sodass Tauri-Commands nie direkt mit Tokens hantieren. Heruntergeladene Dateien landen im Tauri-App-Cache-Verzeichnis und durchlaufen danach dieselbe Parse-Pipeline wie lokale Importe (`commands::import_one`, um Dopplung zu vermeiden). Frontend bekommt einen neuen Dialog (`CloudBrowserDialog.tsx`) für die Ordner-Navigation.

**Tech Stack:** Tauri v2 (Rust/`rusqlite`), React 19 + TypeScript 6. Keine neuen Rust-Abhängigkeiten — der Google-Drive-„Multipart"-Upload-Stub braucht (später) nur manuell gebaute `multipart/related`-Bodies, kein `reqwest`-Feature-Flag.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-09-09-cloud-storage-design.md`, Abschnitte 4 (Import-Flow), 5 (Cache & Änderungserkennung), 8 (Fehlerbehandlung/Testbarkeit). Baut auf dem in `docs/superpowers/plans/2026-09-09-cloud-storage-gdrive-plan.md` gebauten Fundament auf (StorageProvider-Trait, Token-Speicherung, `cloud_accounts`-Tabelle, `connect_google_drive`/`disconnect_cloud_account`/`list_cloud_accounts`).
- **Kritische Lektion aus Plan 1 (Finding 4 der Abschlussreview):** Ein `MutexGuard` der DB-Connection darf NIEMALS über einen `.await`-Punkt hinweg gehalten werden. Jede DB-Lese-/Schreiboperation läuft in einem eigenen kurzen `{ let conn = lock_db(...)?; ... }`-Block, der endet, BEVOR ein Netzwerk-Aufruf (`.await`) beginnt.
- Google-Drive-API-Aufrufe laufen über die eigenständige `reqwest`-Abhängigkeit (nicht `oauth2::reqwest`, das bleibt dem Token-Austausch/-Refresh in `cloud/oauth.rs` vorbehalten).
- Automatischer Token-Refresh bei 401 ist zentral in `cloud/session.rs` gekapselt (Spec Abschnitt 2/8) — kein Tauri-Command lädt/erneuert Tokens selbst.
- **Testmethode:** Neue reine HTTP-Parsing-Funktionen (`list_folder`, `get_metadata`, `download`) werden wie in Plan 1 gegen einen lokalen Mock-HTTP-Server getestet (kein echter Google-Netzwerkzugriff, kein neues Test-Crate — nur `std::net::TcpListener` + ein Hintergrund-Thread, exaktes Muster aus `cloud/gdrive.rs`s bestehendem `fetch_google_account_email`-Test). Token-Refresh (`refresh_access_token`) und die Session-Orchestrierung (`with_gdrive_provider`) sind wie der ursprüngliche OAuth-Flow **nicht automatisiert testbar** (echter Google-Token nötig) — kein Test in diesen Tasks, dafür sorgfältige Transkription und `cargo build`-Verifikation.
- Datei-Browser-Dialog ist eine neue, eigenständige Komponente (`CloudBrowserDialog.tsx`), kein natives OS-Dateiauswahlfenster (der bestehende `tauri-plugin-dialog` kann keine Cloud-Ordnerstruktur anzeigen).
- Neue i18n-Keys (Dialog-Texte, neuer Import-Menüpunkt) müssen in `src/i18n/types.ts` UND allen vier Wörterbüchern (`de.ts`/`en.ts`/`es.ts`/`fr.ts`) ergänzt werden — fehlende Keys sind ein Compile-Fehler in diesem Projekt. Anbietername „Google Drive" bleibt an allen Stellen unübersetzt (Markenname), analog zum bereits etablierten `providerName`-Muster in `Sidebar.tsx`.
- Git-Konventionen: explizites `git add <file1> <file2>` pro Datei (nie `-A`/`.`), deutsche Commit-Messages via HEREDOC, nie `--amend`.
- Nach jedem Task: kurze Zusammenfassung und Rücksprache mit dem User, bevor der nächste Task begonnen wird — sofern nicht per `/goal` explizit durchgehende Ausführung angeordnet wurde.

---

## File Structure

**Create:**
- `src-tauri/src/cloud/session.rs`
- `src/components/CloudBrowserDialog.tsx`

**Modify:**
- `src-tauri/src/db/models.rs` (`NewFile` um `origin`/`cloud_id`/`sync_status` erweitert)
- `src-tauri/src/db/repository.rs` (`insert_file`-SQL erweitert, neue Funktion `set_file_sync_status`)
- `src-tauri/src/db/mod.rs` (Re-Export + Test-Fixture-Anpassung)
- `src-tauri/src/commands.rs` (`import_one` um `origin`/`cloud_id`-Parameter erweitert, `pub(crate)` statt privat)
- `src-tauri/src/cloud/oauth.rs` (neue Funktion `refresh_access_token`)
- `src-tauri/src/cloud/gdrive.rs` (`GoogleDriveProvider`-Struct implementiert `StorageProvider`)
- `src-tauri/src/cloud/mod.rs` (neues `pub mod session;`)
- `src-tauri/src/cloud/commands.rs` (drei neue Commands: `browse_cloud_folder`, `import_from_cloud`, `check_cloud_sync_status`)
- `src-tauri/src/lib.rs` (Command-Registrierung)
- `src/i18n/types.ts`, `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`
- `src/components/Header.tsx`
- `src/App.tsx`

---

## Task 1: DB-Schicht — `NewFile` um `origin`/`cloud_id`/`sync_status` erweitern

**Files:**
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: nichts Neues
- Produces:
  - `NewFile` bekommt drei neue Felder: `origin: String`, `cloud_id: Option<String>`, `sync_status: String`
  - `pub fn set_file_sync_status(conn: &Connection, file_id: i64, status: &str) -> Result<(), DbError>`
  - `pub(crate) fn import_one(conn: &mut Connection, path: &Path, origin: &str, cloud_id: Option<String>) -> CmdResult<ModelFileDto>` (Signatur erweitert, Sichtbarkeit geändert)

Aktuell schreibt `insert_file` `origin`/`cloud_id`/`sync_status` nie explizit — die Spalten fallen auf ihre SQL-`DEFAULT`-Werte (`'local'`/`NULL`/`'local-only'`) zurück. Für Cloud-Importe müssen diese Werte explizit gesetzt werden können.

- [ ] **Step 1: `NewFile` in `src-tauri/src/db/models.rs` erweitern**

Aktueller Inhalt (Zeilen 34-49):
```rust
#[derive(Debug, Clone)]
pub struct NewFile {
    pub name: String,
    pub path: String,
    pub file_type: FileType,
    pub folder_id: Option<i64>,
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
}
```

Neuer Inhalt:
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
}
```

Den Doc-Kommentar direkt über `NewFile` ("Data needed to catalog a newly imported file. `origin` defaults to ...") entfernen, da er nicht mehr zutrifft (die Felder sind jetzt explizit, kein impliziter DB-Default mehr).

- [ ] **Step 2: `insert_file` in `src-tauri/src/db/repository.rs` erweitern**

Aktueller Inhalt (Zeilen 113-140):
```rust
pub fn insert_file(conn: &mut Connection, file: &NewFile) -> Result<i64, DbError> {
    let tx = conn.transaction()?;

    let [dim_x, dim_y, dim_z] = match file.dimensions_mm {
        Some(d) => [Some(d[0]), Some(d[1]), Some(d[2])],
        None => [None, None, None],
    };

    tx.execute(
        "INSERT INTO files (
            name, path, file_type, folder_id, file_size_bytes,
            dimension_x_mm, dimension_y_mm, dimension_z_mm,
            volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            file.name,
            file.path,
            file.file_type.as_str(),
            file.folder_id,
            file.file_size_bytes,
            dim_x,
            dim_y,
            dim_z,
            file.volume_cm3,
            file.object_count,
            file.thumbnail_png,
            file.imported_at,
            file.file_modified_at,
        ],
    )?;
    let file_id = tx.last_insert_rowid();
```

Neuer Inhalt:
```rust
pub fn insert_file(conn: &mut Connection, file: &NewFile) -> Result<i64, DbError> {
    let tx = conn.transaction()?;

    let [dim_x, dim_y, dim_z] = match file.dimensions_mm {
        Some(d) => [Some(d[0]), Some(d[1]), Some(d[2])],
        None => [None, None, None],
    };

    tx.execute(
        "INSERT INTO files (
            name, path, file_type, folder_id, origin, cloud_id, sync_status,
            file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
            volume_cm3, object_count, thumbnail_png, imported_at, file_modified_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
        ],
    )?;
    let file_id = tx.last_insert_rowid();
```

Am Ende der Datei (nach `set_cloud_account_status`) anfügen:
```rust

pub fn set_file_sync_status(conn: &Connection, file_id: i64, status: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET sync_status = ?1 WHERE id = ?2",
        params![status, file_id],
    )?;
    Ok(())
}
```

- [ ] **Step 3: Re-Export in `src-tauri/src/db/mod.rs` ergänzen**

Aktueller Inhalt:
```rust
pub use repository::{
    add_tag_to_file, connect, delete_file, file_exists_by_path, get_file, insert_file,
    insert_folder, list_cloud_accounts, list_files, list_folders, list_tag_counts,
    remove_tag_from_file, set_cloud_account_status, upsert_cloud_account,
};
```

Neuer Inhalt:
```rust
pub use repository::{
    add_tag_to_file, connect, delete_file, file_exists_by_path, get_file, insert_file,
    insert_folder, list_cloud_accounts, list_files, list_folders, list_tag_counts,
    remove_tag_from_file, set_cloud_account_status, set_file_sync_status, upsert_cloud_account,
};
```

- [ ] **Step 4: Test-Fixture `sample_file()` in `src-tauri/src/db/mod.rs` anpassen**

Aktueller Inhalt:
```rust
        NewFile {
            name: "cube.3mf".to_string(),
            path: "/tmp/cube.3mf".to_string(),
            file_type: FileType::ThreeMf,
            folder_id: None,
            file_size_bytes: 1024,
```

Neuer Inhalt:
```rust
        NewFile {
            name: "cube.3mf".to_string(),
            path: "/tmp/cube.3mf".to_string(),
            file_type: FileType::ThreeMf,
            folder_id: None,
            origin: "local".to_string(),
            cloud_id: None,
            sync_status: "local-only".to_string(),
            file_size_bytes: 1024,
```

- [ ] **Step 5: `import_one` in `src-tauri/src/commands.rs` erweitern**

Aktueller Inhalt (Funktionssignatur und `NewFile`-Konstruktion, Zeilen 204-273):
```rust
fn import_one(conn: &mut Connection, path: &Path) -> CmdResult<ModelFileDto> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unbenannt")
        .to_string();
    let file_size_bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len() as i64;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
```

Neuer Inhalt:
```rust
pub(crate) fn import_one(
    conn: &mut Connection,
    path: &Path,
    origin: &str,
    cloud_id: Option<String>,
) -> CmdResult<ModelFileDto> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unbenannt")
        .to_string();
    let file_size_bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len() as i64;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
```

Aktueller Inhalt (die `NewFile`-Konstruktion weiter unten in derselben Funktion):
```rust
    let new_file = NewFile {
        name: file_name,
        path: path.to_string_lossy().to_string(),
        file_type,
        folder_id: None,
        file_size_bytes,
        dimensions_mm,
        volume_cm3,
        object_count,
        thumbnail_png,
        imported_at: chrono::Utc::now().to_rfc3339(),
        file_modified_at: None,
        materials,
        metadata,
        tags,
    };
```

Neuer Inhalt:
```rust
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
        metadata,
        tags,
    };
```

Den einzigen bestehenden Aufruf von `import_one` (in `import_many`) anpassen:

Aktueller Inhalt:
```rust
        match import_one(&mut conn, &path) {
```

Neuer Inhalt:
```rust
        match import_one(&mut conn, &path, "local", None) {
```

- [ ] **Step 6: Bauen und bestehende Tests prüfen**

Run: `cd src-tauri && cargo build && cargo test`
Expected: Build PASS, alle bisherigen Tests weiterhin grün (37 aus Plan 1, keine neuen Tests in diesem Task — `set_file_sync_status` wird in Task 8 mitverifiziert).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/models.rs src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
refactor: NewFile/import_one um origin, cloud_id, sync_status erweitert

Bisher fielen diese Spalten immer auf ihre SQL-Defaults ('local'/NULL/
'local-only') zurueck. Fuer Cloud-Importe (naechste Tasks) muessen sie
explizit gesetzt werden koennen. import_one wird pub(crate), damit das
cloud-Modul es fuer den Cloud-Import wiederverwenden kann, statt die
Parse-Pipeline zu duplizieren.
EOF
)"
```

---

## Task 2: Token-Refresh — `refresh_access_token`

**Files:**
- Modify: `src-tauri/src/cloud/oauth.rs`

**Interfaces:**
- Consumes: `CloudError`, `CloudResult` aus `provider.rs`
- Produces: `pub async fn refresh_access_token(client_id: &str, client_secret: &str, refresh_token: &str) -> CloudResult<OAuthTokens>`

Nutzt Googles Token-Endpunkt, um mit einem gespeicherten Refresh-Token einen neuen Access-Token zu holen (RFC 6749 Abschnitt 6). Wie der ursprüngliche OAuth-Flow **nicht automatisiert testbar** — kein Test in diesem Task.

- [ ] **Step 1: `refresh_access_token` in `src-tauri/src/cloud/oauth.rs` ergänzen**

Am Ende der Datei anfügen:
```rust

/// Tauscht einen gespeicherten Refresh-Token gegen einen neuen Access-Token
/// (RFC 6749 Abschnitt 6). Google gibt bei einem Refresh in der Regel
/// keinen neuen Refresh-Token zurueck (dieser bleibt gueltig) - der
/// `refresh_token` im Ergebnis ist dann `None`; der Aufrufer behaelt in
/// diesem Fall den bisherigen Refresh-Token.
pub async fn refresh_access_token(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> CloudResult<OAuthTokens> {
    let client = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_client_secret(ClientSecret::new(client_secret.to_string()))
        .set_token_uri(
            TokenUrl::new("https://www.googleapis.com/oauth2/v3/token".to_string())
                .map_err(|e| CloudError::Auth(e.to_string()))?,
        );

    let http_client = oauth2::reqwest::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| CloudError::Network(e.to_string()))?;

    let token_result = client
        .exchange_refresh_token(&oauth2::RefreshToken::new(refresh_token.to_string()))
        .request_async(&http_client)
        .await
        .map_err(|e| CloudError::Auth(format!("Token-Refresh fehlgeschlagen: {e}")))?;

    Ok(OAuthTokens {
        access_token: token_result.access_token().secret().clone(),
        refresh_token: token_result.refresh_token().map(|t| t.secret().clone()),
    })
}
```

- [ ] **Step 2: Bauen prüfen**

Run: `cd src-tauri && cargo build`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/cloud/oauth.rs
git commit -m "$(cat <<'EOF'
feat: Access-Token per Refresh-Token erneuern

RFC-6749-Refresh-Flow ueber das oauth2-Crate. Nicht automatisiert
testbar (echter Google-Refresh-Token noetig), wie der urspruengliche
OAuth-Austausch.
EOF
)"
```

---

## Task 3: Session-Schicht — `with_gdrive_provider` (Token-Laden, Refresh-bei-401, Retry)

**Files:**
- Create: `src-tauri/src/cloud/session.rs`
- Modify: `src-tauri/src/cloud/mod.rs`

**Interfaces:**
- Consumes: `db::list_cloud_accounts`/`db::set_cloud_account_status` (Plan 1); `cloud::config::{load_cloud_config, default_config_path}`; `cloud::oauth::refresh_access_token`; `cloud::tokens::{KeyringTokenStore, StoredTokens, TokenStore}`; `cloud::gdrive::GoogleDriveProvider` (wird in Task 4 erst erstellt — dieser Task referenziert ihn bereits, kompiliert aber erst nach Task 4 vollständig; siehe Hinweis unten)
- Produces: `pub async fn with_gdrive_provider<F, Fut, T>(state: &State<'_, AppState>, operation: F) -> CmdResult<T> where F: Fn(GoogleDriveProvider) -> Fut, Fut: Future<Output = CloudResult<T>>`

**Wichtiger Hinweis zur Reihenfolge:** Dieser Task referenziert `cloud::gdrive::GoogleDriveProvider`, das erst in Task 4 entsteht. `cargo build` schlägt nach diesem Task fehl (unbekannter Typ) — das ist erwartet und wird in Task 4 behoben. Committe diesen Task trotzdem einzeln (bessere Nachvollziehbarkeit), aber überspringe Step 2 (Bauen prüfen) hier und hole die Verifikation in Task 4 nach.

- [ ] **Step 1: `src-tauri/src/cloud/session.rs` schreiben**

```rust
use tauri::State;

use crate::cloud::config::{default_config_path, load_cloud_config};
use crate::cloud::gdrive::GoogleDriveProvider;
use crate::cloud::oauth::refresh_access_token;
use crate::cloud::provider::CloudError;
use crate::cloud::tokens::{KeyringTokenStore, StoredTokens, TokenStore};
use crate::commands::{lock_db, AppState};
use crate::db;

type CmdResult<T> = Result<T, String>;

/// Fuehrt `operation` mit einem gueltigen Google-Drive-Access-Token aus.
/// Schlaegt der erste Versuch mit einem Auth-Fehler fehl (Token abgelaufen),
/// wird per Refresh-Token ein neuer Access-Token geholt, im Schluesselbund
/// gespeichert und der Versuch genau einmal wiederholt. Schlaegt auch der
/// Refresh fehl, wechselt der Konto-Status auf 'error'.
///
/// Haelt an keiner Stelle einen DB-MutexGuard ueber einen .await-Punkt
/// hinweg: jede DB-Operation laeuft in ihrem eigenen kurzen Block.
pub async fn with_gdrive_provider<F, Fut, T>(state: &State<'_, AppState>, operation: F) -> CmdResult<T>
where
    F: Fn(GoogleDriveProvider) -> Fut,
    Fut: std::future::Future<Output = Result<T, CloudError>>,
{
    let account_label = {
        let conn = lock_db(state)?;
        let accounts = db::list_cloud_accounts(&conn).map_err(|e| e.to_string())?;
        accounts
            .into_iter()
            .find(|a| a.provider == "gdrive" && a.status != "disconnected")
            .map(|a| a.account_label)
            .ok_or_else(|| "Kein verbundenes Google-Drive-Konto".to_string())?
    };

    let account_key = format!("gdrive:{account_label}");
    let token_store = KeyringTokenStore;
    let stored = token_store
        .load(&account_key)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Kein Token im Schluesselbund gefunden - bitte erneut verbinden".to_string())?;

    let provider = GoogleDriveProvider::new(stored.access_token);
    match operation(provider).await {
        Ok(value) => Ok(value),
        Err(CloudError::Auth(_)) => {
            match refresh_and_store(&account_key, stored.refresh_token).await {
                Ok(new_access_token) => operation(GoogleDriveProvider::new(new_access_token))
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => {
                    let conn = lock_db(state)?;
                    let _ = db::set_cloud_account_status(&conn, "gdrive", "error");
                    Err(e.to_string())
                }
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

async fn refresh_and_store(
    account_key: &str,
    refresh_token: Option<String>,
) -> Result<String, CloudError> {
    let refresh_token = refresh_token.ok_or_else(|| {
        CloudError::Auth("Kein Refresh-Token vorhanden - bitte erneut verbinden".to_string())
    })?;
    let config = load_cloud_config(&default_config_path()).map_err(|e| CloudError::Auth(e.to_string()))?;

    let new_tokens = refresh_access_token(
        &config.google_client_id,
        &config.google_client_secret,
        &refresh_token,
    )
    .await?;

    let token_store = KeyringTokenStore;
    token_store
        .save(
            account_key,
            &StoredTokens {
                access_token: new_tokens.access_token.clone(),
                refresh_token: new_tokens.refresh_token.or(Some(refresh_token)),
            },
        )
        .map_err(CloudError::Auth)?;

    Ok(new_tokens.access_token)
}
```

- [ ] **Step 2: `src-tauri/src/cloud/mod.rs` erweitern**

Aktueller Inhalt:
```rust
pub mod commands;
pub mod config;
pub mod gdrive;
pub mod oauth;
pub mod provider;
pub mod tokens;
```

Neuer Inhalt:
```rust
pub mod commands;
pub mod config;
pub mod gdrive;
pub mod oauth;
pub mod provider;
pub mod session;
pub mod tokens;
```

- [ ] **Step 3: Commit (ohne Bauen-Prüfung, siehe Hinweis oben)**

```bash
git add src-tauri/src/cloud/session.rs src-tauri/src/cloud/mod.rs
git commit -m "$(cat <<'EOF'
feat: Session-Schicht fuer Google Drive (Token-Refresh-bei-401, Retry)

with_gdrive_provider kapselt Token-Laden, automatischen Refresh bei
Auth-Fehlern und genau einen Retry - kein Tauri-Command haelt einen
DB-MutexGuard ueber einen Netzwerk-Aufruf hinweg (Lektion aus Plan 1s
Abschlussreview, Finding 4). Baut erst nach Task 4 (GoogleDriveProvider)
vollstaendig.
EOF
)"
```

---

## Task 4: `GoogleDriveProvider` — `list_folder` und `get_metadata`

**Files:**
- Modify: `src-tauri/src/cloud/gdrive.rs`

**Interfaces:**
- Consumes: `StorageProvider`, `CloudEntry`, `CloudError`, `CloudResult` aus `provider.rs`
- Produces: `pub struct GoogleDriveProvider { ... }` mit `pub fn new(access_token: String) -> Self`, implementiert `StorageProvider::list_folder` und `StorageProvider::get_metadata` (die zwei übrigen Trait-Methoden `download`/`upload` folgen in Task 5)

Ab diesem Task kompiliert das gesamte Projekt wieder (Task 3s Referenz auf `GoogleDriveProvider` löst sich auf) — `download`/`upload` müssen aber als Platzhalter existieren, da `StorageProvider` alle vier Methoden verlangt; `download` wird in Task 5 fertiggestellt, `upload` bleibt bis zu einer späteren Runde (Hochladen ist nicht Teil dieses Plans) ein klar markierter Stub.

- [ ] **Step 1: Imports und `GoogleDriveProvider`-Grundgerüst in `src-tauri/src/cloud/gdrive.rs` ergänzen**

Aktueller Inhalt (Zeile 1):
```rust
use crate::cloud::provider::{CloudError, CloudResult};
```

Neuer Inhalt:
```rust
use async_trait::async_trait;

use crate::cloud::provider::{CloudEntry, CloudError, CloudResult, StorageProvider};

const DRIVE_API_BASE: &str = "https://www.googleapis.com/drive/v3";

pub struct GoogleDriveProvider {
    access_token: String,
    client: reqwest::Client,
}

impl GoogleDriveProvider {
    pub fn new(access_token: String) -> Self {
        Self {
            access_token,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl StorageProvider for GoogleDriveProvider {
    async fn list_folder(&self, folder_id: Option<&str>) -> CloudResult<Vec<CloudEntry>> {
        list_folder_from(DRIVE_API_BASE, &self.client, &self.access_token, folder_id).await
    }

    async fn download(&self, _file_id: &str) -> CloudResult<Vec<u8>> {
        Err(CloudError::Network(
            "download() wird in einem spaeteren Task implementiert".to_string(),
        ))
    }

    async fn upload(&self, _folder_id: Option<&str>, _file_name: &str, _data: &[u8]) -> CloudResult<CloudEntry> {
        Err(CloudError::Network(
            "Hochladen zu Google Drive ist noch nicht implementiert".to_string(),
        ))
    }

    async fn get_metadata(&self, file_id: &str) -> CloudResult<CloudEntry> {
        get_metadata_from(DRIVE_API_BASE, &self.client, &self.access_token, file_id).await
    }
}

#[derive(serde::Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(rename = "modifiedTime")]
    modified_time: String,
    #[serde(default)]
    size: Option<String>,
}

impl From<DriveFile> for CloudEntry {
    fn from(f: DriveFile) -> Self {
        CloudEntry {
            id: f.id,
            name: f.name,
            is_folder: f.mime_type == "application/vnd.google-apps.folder",
            modified_time: f.modified_time,
            size_bytes: f.size.and_then(|s| s.parse::<i64>().ok()),
        }
    }
}

async fn list_folder_from(
    base_url: &str,
    client: &reqwest::Client,
    access_token: &str,
    folder_id: Option<&str>,
) -> CloudResult<Vec<CloudEntry>> {
    #[derive(serde::Deserialize)]
    struct FileListResponse {
        files: Vec<DriveFile>,
    }

    let parent = folder_id.unwrap_or("root");
    let query = format!(
        "'{parent}' in parents and trashed = false and \
         (mimeType = 'application/vnd.google-apps.folder' or name contains '.3mf' or name contains '.stl')"
    );

    let response = client
        .get(format!("{base_url}/files"))
        .bearer_auth(access_token)
        .query(&[
            ("q", query.as_str()),
            ("fields", "files(id,name,mimeType,modifiedTime,size)"),
            ("pageSize", "1000"),
        ])
        .send()
        .await
        .map_err(|e| CloudError::Network(e.to_string()))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(CloudError::Auth("Zugriffstoken abgelaufen".to_string()));
    }
    if !response.status().is_success() {
        return Err(CloudError::Network(format!(
            "Google-Drive-Anfrage fehlgeschlagen: HTTP {}",
            response.status()
        )));
    }

    let parsed: FileListResponse = response
        .json()
        .await
        .map_err(|e| CloudError::Network(format!("Antwort konnte nicht gelesen werden: {e}")))?;

    Ok(parsed.files.into_iter().map(CloudEntry::from).collect())
}

async fn get_metadata_from(
    base_url: &str,
    client: &reqwest::Client,
    access_token: &str,
    file_id: &str,
) -> CloudResult<CloudEntry> {
    let response = client
        .get(format!("{base_url}/files/{file_id}"))
        .bearer_auth(access_token)
        .query(&[("fields", "id,name,mimeType,modifiedTime,size")])
        .send()
        .await
        .map_err(|e| CloudError::Network(e.to_string()))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CloudError::NotFound(file_id.to_string()));
    }
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(CloudError::Auth("Zugriffstoken abgelaufen".to_string()));
    }
    if !response.status().is_success() {
        return Err(CloudError::Network(format!(
            "Google-Drive-Anfrage fehlgeschlagen: HTTP {}",
            response.status()
        )));
    }

    let parsed: DriveFile = response
        .json()
        .await
        .map_err(|e| CloudError::Network(format!("Antwort konnte nicht gelesen werden: {e}")))?;

    Ok(CloudEntry::from(parsed))
}
```

- [ ] **Step 2: Tests ergänzen**

Im bestehenden `#[cfg(test)] mod tests`-Block von `src-tauri/src/cloud/gdrive.rs` (nutzt bereits `spawn_mock_userinfo_server` — für diese Tests wird eine allgemeinere Variante gebraucht, die beliebigen JSON-Body zurückgibt; die bestehende Helper-Funktion `spawn_mock_userinfo_server(body, status_line)` ist dafür bereits generisch genug, trotz ihres Namens):

Am Ende des `mod tests`-Blocks anfügen:
```rust

    #[tokio::test]
    async fn list_folder_parses_files_and_folders() {
        let url = spawn_mock_userinfo_server(
            r#"{"files": [
                {"id": "folder-1", "name": "Vasen", "mimeType": "application/vnd.google-apps.folder", "modifiedTime": "2026-09-01T10:00:00Z"},
                {"id": "file-1", "name": "cube.3mf", "mimeType": "application/octet-stream", "modifiedTime": "2026-09-02T11:00:00Z", "size": "2048"}
            ]}"#,
            "HTTP/1.1 200 OK",
        );
        let client = reqwest::Client::new();
        let entries = list_folder_from(&url, &client, "fake-token", None).await.expect("list");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "folder-1");
        assert!(entries[0].is_folder);
        assert_eq!(entries[0].size_bytes, None);
        assert_eq!(entries[1].id, "file-1");
        assert!(!entries[1].is_folder);
        assert_eq!(entries[1].size_bytes, Some(2048));
    }

    #[tokio::test]
    async fn list_folder_returns_auth_error_for_401() {
        let url = spawn_mock_userinfo_server("", "HTTP/1.1 401 Unauthorized");
        let client = reqwest::Client::new();
        let result = list_folder_from(&url, &client, "expired-token", None).await;
        assert!(matches!(result, Err(CloudError::Auth(_))));
    }

    #[tokio::test]
    async fn get_metadata_parses_a_single_file() {
        let url = spawn_mock_userinfo_server(
            r#"{"id": "file-1", "name": "cube.3mf", "mimeType": "application/octet-stream", "modifiedTime": "2026-09-02T11:00:00Z", "size": "2048"}"#,
            "HTTP/1.1 200 OK",
        );
        let client = reqwest::Client::new();
        let entry = get_metadata_from(&url, &client, "fake-token", "file-1").await.expect("metadata");
        assert_eq!(entry.name, "cube.3mf");
        assert_eq!(entry.modified_time, "2026-09-02T11:00:00Z");
        assert_eq!(entry.size_bytes, Some(2048));
    }

    #[tokio::test]
    async fn get_metadata_returns_not_found_for_404() {
        let url = spawn_mock_userinfo_server("", "HTTP/1.1 404 Not Found");
        let client = reqwest::Client::new();
        let result = get_metadata_from(&url, &client, "fake-token", "missing-file").await;
        assert!(matches!(result, Err(CloudError::NotFound(_))));
    }
```

- [ ] **Step 3: Bauen und testen (holt Task 3s übersprungene Prüfung nach)**

Run: `cd src-tauri && cargo build && cargo test cloud::`
Expected: Build PASS (Task 3s vorher fehlender Typ `GoogleDriveProvider` existiert jetzt), 4 neue Tests plus alle bisherigen `cloud::`-Tests grün.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/cloud/gdrive.rs
git commit -m "$(cat <<'EOF'
feat: GoogleDriveProvider implementiert list_folder und get_metadata

Echte Google Drive REST-API-v3-Aufrufe (Files: list/get), getestet
gegen einen lokalen Mock-HTTP-Server. download()/upload() bleiben
Platzhalter (naechster Task bzw. spaetere Runde fuer Uploads).
EOF
)"
```

---

## Task 5: `GoogleDriveProvider` — `download`

**Files:**
- Modify: `src-tauri/src/cloud/gdrive.rs`

**Interfaces:**
- Consumes: nichts Neues
- Produces: `StorageProvider::download` vollständig implementiert (ersetzt den Platzhalter aus Task 4)

- [ ] **Step 1: `download`-Methode und Hilfsfunktion in `src-tauri/src/cloud/gdrive.rs` ersetzen**

Aktueller Inhalt:
```rust
    async fn download(&self, _file_id: &str) -> CloudResult<Vec<u8>> {
        Err(CloudError::Network(
            "download() wird in einem spaeteren Task implementiert".to_string(),
        ))
    }
```

Neuer Inhalt:
```rust
    async fn download(&self, file_id: &str) -> CloudResult<Vec<u8>> {
        download_from(DRIVE_API_BASE, &self.client, &self.access_token, file_id).await
    }
```

Nach der Funktion `get_metadata_from` (am Ende der Datei, vor dem `#[cfg(test)]`-Block) anfügen:
```rust

async fn download_from(
    base_url: &str,
    client: &reqwest::Client,
    access_token: &str,
    file_id: &str,
) -> CloudResult<Vec<u8>> {
    let response = client
        .get(format!("{base_url}/files/{file_id}"))
        .bearer_auth(access_token)
        .query(&[("alt", "media")])
        .send()
        .await
        .map_err(|e| CloudError::Network(e.to_string()))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CloudError::NotFound(file_id.to_string()));
    }
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(CloudError::Auth("Zugriffstoken abgelaufen".to_string()));
    }
    if !response.status().is_success() {
        return Err(CloudError::Network(format!(
            "Google-Drive-Download fehlgeschlagen: HTTP {}",
            response.status()
        )));
    }

    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| CloudError::Network(e.to_string()))
}
```

- [ ] **Step 2: Test ergänzen**

Am Ende des `mod tests`-Blocks anfügen:
```rust

    #[tokio::test]
    async fn download_returns_raw_bytes() {
        let url = spawn_mock_userinfo_server("raw-file-content", "HTTP/1.1 200 OK");
        let client = reqwest::Client::new();
        let data = download_from(&url, &client, "fake-token", "file-1").await.expect("download");
        assert_eq!(data, b"raw-file-content");
    }
```

- [ ] **Step 3: Bauen und testen**

Run: `cd src-tauri && cargo build && cargo test cloud::gdrive::`
Expected: PASS, 5 Tests in diesem Modul (4 aus Task 4 plus dieser).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/cloud/gdrive.rs
git commit -m "$(cat <<'EOF'
feat: GoogleDriveProvider implementiert download

Rohe Bytes per alt=media, getestet gegen lokalen Mock-Server. Damit
implementiert GoogleDriveProvider drei der vier StorageProvider-
Methoden vollstaendig; upload() bleibt bewusst Platzhalter (spaetere
Runde).
EOF
)"
```

---

## Task 6: Tauri-Command `browse_cloud_folder`

**Files:**
- Modify: `src-tauri/src/cloud/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `session::with_gdrive_provider` (Task 3); `StorageProvider::list_folder`
- Produces: `pub struct CloudEntryDto { id: String, name: String, is_folder: bool, modified_time: String, size_bytes: Option<i64> }` (camelCase); `#[tauri::command] pub async fn browse_cloud_folder(state, folder_id: Option<String>) -> Result<Vec<CloudEntryDto>, String>`

Kein Test in diesem Task — reine Verdrahtung ohne eigene Logik über das bereits getestete `list_folder`/`with_gdrive_provider` hinaus.

- [ ] **Step 1: `browse_cloud_folder` in `src-tauri/src/cloud/commands.rs` ergänzen**

Import-Zeile am Dateianfang:

Aktueller Inhalt:
```rust
use crate::cloud::config::{default_config_path, load_cloud_config};
use crate::cloud::gdrive::fetch_google_account_email;
use crate::cloud::oauth::run_google_oauth_flow;
use crate::cloud::tokens::{KeyringTokenStore, StoredTokens, TokenStore};
use crate::commands::{lock_db, AppState};
use crate::db;
```

Neuer Inhalt:
```rust
use crate::cloud::config::{default_config_path, load_cloud_config};
use crate::cloud::gdrive::fetch_google_account_email;
use crate::cloud::oauth::run_google_oauth_flow;
use crate::cloud::provider::{CloudEntry, StorageProvider};
use crate::cloud::session::with_gdrive_provider;
use crate::cloud::tokens::{KeyringTokenStore, StoredTokens, TokenStore};
use crate::commands::{lock_db, AppState};
use crate::db;
```

Am Ende der Datei anfügen:
```rust

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudEntryDto {
    pub id: String,
    pub name: String,
    pub is_folder: bool,
    pub modified_time: String,
    pub size_bytes: Option<i64>,
}

impl From<CloudEntry> for CloudEntryDto {
    fn from(e: CloudEntry) -> Self {
        CloudEntryDto {
            id: e.id,
            name: e.name,
            is_folder: e.is_folder,
            modified_time: e.modified_time,
            size_bytes: e.size_bytes,
        }
    }
}

#[tauri::command]
pub async fn browse_cloud_folder(
    state: State<'_, AppState>,
    folder_id: Option<String>,
) -> CmdResult<Vec<CloudEntryDto>> {
    let entries = with_gdrive_provider(&state, |provider| {
        let folder_id = folder_id.clone();
        async move { provider.list_folder(folder_id.as_deref()).await }
    })
    .await?;

    Ok(entries.into_iter().map(CloudEntryDto::from).collect())
}
```

- [ ] **Step 2: Command in `src-tauri/src/lib.rs` registrieren**

Aktueller Inhalt:
```rust
            cloud::commands::connect_google_drive,
            cloud::commands::disconnect_cloud_account,
            cloud::commands::list_cloud_accounts,
        ])
```

Neuer Inhalt:
```rust
            cloud::commands::connect_google_drive,
            cloud::commands::disconnect_cloud_account,
            cloud::commands::list_cloud_accounts,
            cloud::commands::browse_cloud_folder,
        ])
```

- [ ] **Step 3: Bauen und bestehende Tests prüfen**

Run: `cd src-tauri && cargo build && cargo test`
Expected: Build PASS, alle bisherigen Tests weiterhin grün (keine neuen Tests in diesem Task).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/cloud/commands.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: Tauri-Command browse_cloud_folder

Reine Verdrahtung ueber with_gdrive_provider/list_folder, beide
bereits einzeln getestet. Kein neuer Test in diesem Task.
EOF
)"
```

---

## Task 7: Tauri-Command `import_from_cloud`

**Files:**
- Modify: `src-tauri/src/cloud/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `session::with_gdrive_provider`; `StorageProvider::{get_metadata, download}`; `commands::import_one` (Task 1, jetzt `pub(crate)`)
- Produces: `#[tauri::command] pub async fn import_from_cloud(app, state, file_ids: Vec<String>) -> Result<Vec<ModelFileDto>, String>`

Lädt jede gewählte Datei herunter, speichert sie im Tauri-App-Cache-Verzeichnis zwischen und übergibt sie danach an dieselbe Parse-Pipeline wie der lokale Import (`commands::import_one`). Speichert zusätzlich `file_modified_at = metadata.modified_time` — Grundlage für die Änderungserkennung in Task 8. Kein Test in diesem Task (reine Orchestrierung über bereits getestete Bausteine plus Dateisystem-Seiteneffekte, die eine echte Tauri-`AppHandle` bräuchten).

- [ ] **Step 1: `import_from_cloud` in `src-tauri/src/cloud/commands.rs` ergänzen**

Import-Zeile ergänzen:

Aktueller Inhalt:
```rust
use crate::cloud::provider::{CloudEntry, StorageProvider};
use crate::cloud::session::with_gdrive_provider;
use crate::cloud::tokens::{KeyringTokenStore, StoredTokens, TokenStore};
use crate::commands::{lock_db, AppState};
use crate::db;
```

Neuer Inhalt:
```rust
use tauri::Manager;

use crate::cloud::provider::{CloudEntry, StorageProvider};
use crate::cloud::session::with_gdrive_provider;
use crate::cloud::tokens::{KeyringTokenStore, StoredTokens, TokenStore};
use crate::commands::{import_one, lock_db, AppState, ModelFileDto};
use crate::db;
```

Am Ende der Datei anfügen:
```rust

#[tauri::command]
pub async fn import_from_cloud(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    file_ids: Vec<String>,
) -> CmdResult<Vec<ModelFileDto>> {
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

    let mut imported = Vec::new();
    for file_id in file_ids {
        let metadata = with_gdrive_provider(&state, {
            let file_id = file_id.clone();
            move |provider| {
                let file_id = file_id.clone();
                async move { provider.get_metadata(&file_id).await }
            }
        })
        .await?;

        let data = with_gdrive_provider(&state, {
            let file_id = file_id.clone();
            move |provider| {
                let file_id = file_id.clone();
                async move { provider.download(&file_id).await }
            }
        })
        .await?;

        let cache_path = cache_dir.join(&metadata.name);
        std::fs::write(&cache_path, &data).map_err(|e| e.to_string())?;

        // Ein durchgehender Lock-Scope genuegt hier: zwischen den beiden
        // DB-Aufrufen liegt kein .await, daher kein Konflikt mit der
        // "nie ueber .await halten"-Regel aus den Global Constraints.
        let dto = {
            let mut conn = lock_db(&state)?;
            let dto = import_one(&mut conn, &cache_path, "gdrive", Some(file_id))?;
            let id: i64 = dto.id.parse().map_err(|_| "invalid file id".to_string())?;
            db::set_file_modified_at(&conn, id, &metadata.modified_time).map_err(|e| e.to_string())?;
            dto
        };

        imported.push(dto);
    }

    Ok(imported)
}
```

- [ ] **Step 2: `set_file_modified_at` in der DB-Schicht ergänzen**

Diese Funktion fehlt noch (Task 1 fügte nur `set_file_sync_status` hinzu). In `src-tauri/src/db/repository.rs` am Ende der Datei anfügen:
```rust

pub fn set_file_modified_at(conn: &Connection, file_id: i64, modified_at: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET file_modified_at = ?1 WHERE id = ?2",
        params![modified_at, file_id],
    )?;
    Ok(())
}
```

In `src-tauri/src/db/mod.rs`s Re-Export-Liste ergänzen:

Aktueller Inhalt:
```rust
pub use repository::{
    add_tag_to_file, connect, delete_file, file_exists_by_path, get_file, insert_file,
    insert_folder, list_cloud_accounts, list_files, list_folders, list_tag_counts,
    remove_tag_from_file, set_cloud_account_status, set_file_sync_status, upsert_cloud_account,
};
```

Neuer Inhalt:
```rust
pub use repository::{
    add_tag_to_file, connect, delete_file, file_exists_by_path, get_file, insert_file,
    insert_folder, list_cloud_accounts, list_files, list_folders, list_tag_counts,
    remove_tag_from_file, set_cloud_account_status, set_file_modified_at, set_file_sync_status,
    upsert_cloud_account,
};
```

- [ ] **Step 3: Command in `src-tauri/src/lib.rs` registrieren**

Aktueller Inhalt:
```rust
            cloud::commands::browse_cloud_folder,
        ])
```

Neuer Inhalt:
```rust
            cloud::commands::browse_cloud_folder,
            cloud::commands::import_from_cloud,
        ])
```

- [ ] **Step 4: Bauen und bestehende Tests prüfen**

Run: `cd src-tauri && cargo build && cargo test`
Expected: Build PASS, alle bisherigen Tests weiterhin grün.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/cloud/commands.rs src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: Tauri-Command import_from_cloud

Laedt Drive-Dateien in den App-Cache herunter und uebergibt sie an
dieselbe Parse-Pipeline wie den lokalen Import (import_one, jetzt
pub(crate)). Speichert modifiedTime als file_modified_at - Basis fuer
die Aenderungserkennung im naechsten Task.
EOF
)"
```

---

## Task 8: Tauri-Command `check_cloud_sync_status`

**Files:**
- Modify: `src-tauri/src/cloud/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `session::with_gdrive_provider`; `StorageProvider::get_metadata`; `db::set_file_sync_status`
- Produces: `#[tauri::command] pub async fn check_cloud_sync_status(state, file_id: String) -> Result<String, String>`

Vergleicht das beim Import gespeicherte `file_modified_at` mit dem aktuellen `modifiedTime` von Drive; weicht es ab, wird der Sync-Status auf `outdated` gesetzt. Kein Test in diesem Task (Orchestrierung über bereits getestete Bausteine).

- [ ] **Step 1: `check_cloud_sync_status` in `src-tauri/src/cloud/commands.rs` ergänzen**

Am Ende der Datei anfügen:
```rust

#[tauri::command]
pub async fn check_cloud_sync_status(state: State<'_, AppState>, file_id: String) -> CmdResult<String> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;

    let (cloud_id, last_known_modified, current_status) = {
        let conn = lock_db(&state)?;
        let file = db::get_file(&conn, id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "file not found".to_string())?;
        (file.cloud_id, file.file_modified_at, file.sync_status)
    };

    let Some(cloud_id) = cloud_id else {
        return Ok(current_status);
    };

    let metadata = with_gdrive_provider(&state, {
        let cloud_id = cloud_id.clone();
        move |provider| {
            let cloud_id = cloud_id.clone();
            async move { provider.get_metadata(&cloud_id).await }
        }
    })
    .await?;

    let new_status = if last_known_modified.as_deref() == Some(metadata.modified_time.as_str()) {
        "synced"
    } else {
        "outdated"
    };

    {
        let conn = lock_db(&state)?;
        db::set_file_sync_status(&conn, id, new_status).map_err(|e| e.to_string())?;
    }

    Ok(new_status.to_string())
}
```

- [ ] **Step 2: Command in `src-tauri/src/lib.rs` registrieren**

Aktueller Inhalt:
```rust
            cloud::commands::import_from_cloud,
        ])
```

Neuer Inhalt:
```rust
            cloud::commands::import_from_cloud,
            cloud::commands::check_cloud_sync_status,
        ])
```

- [ ] **Step 3: Bauen und bestehende Tests prüfen**

Run: `cd src-tauri && cargo build && cargo test`
Expected: Build PASS, alle bisherigen Tests weiterhin grün.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/cloud/commands.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: Tauri-Command check_cloud_sync_status

On-Demand-Aenderungserkennung: vergleicht das beim Import gespeicherte
file_modified_at gegen Drives aktuelles modifiedTime, setzt sync_status
entsprechend auf 'synced'/'outdated'. Kein Hintergrund-Polling.
EOF
)"
```

---

## Task 9: Frontend — `CloudBrowserDialog.tsx`

**Files:**
- Create: `src/components/CloudBrowserDialog.tsx`
- Modify: `src/i18n/types.ts`
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`

**Interfaces:**
- Consumes: `browse_cloud_folder`-Command (Task 6)
- Produces: `export function CloudBrowserDialog({ onClose, onImport }: { onClose: () => void; onImport: (fileIds: string[]) => void }): JSX.Element`

Neue, eigenständige Dialog-Komponente (zentriertes Modal mit Backdrop — bisher gibt es dafür kein Vorbild in der Codebasis, `ContextMenu.tsx` ist koordinatenpositioniert, kein zentriertes Modal). Ordner-Navigation per Breadcrumb, Mehrfachauswahl von Dateien per Checkbox.

- [ ] **Step 1: Vier neue Keys in `src/i18n/types.ts`s `Translations`-Interface ergänzen**

Innerhalb des `Translations`-Interfaces (Position: nach `previewUnavailable`, dem bisher letzten Key) anfügen:
```typescript
  cloudBrowserTitle: string;
  cloudBrowserLoading: string;
  cloudBrowserEmpty: string;
  cloudBrowserImportButton: string;
```

- [ ] **Step 2: Keys in allen vier Wörterbüchern ergänzen**

In `src/i18n/de.ts`, an der gleichen Position (nach `previewUnavailable`) im Objektliteral:
```typescript
  cloudBrowserTitle: 'Aus Google Drive importieren',
  cloudBrowserLoading: 'Lädt …',
  cloudBrowserEmpty: 'Keine Dateien in diesem Ordner',
  cloudBrowserImportButton: 'Importieren ({count})',
```

In `src/i18n/en.ts`:
```typescript
  cloudBrowserTitle: 'Import from Google Drive',
  cloudBrowserLoading: 'Loading …',
  cloudBrowserEmpty: 'No files in this folder',
  cloudBrowserImportButton: 'Import ({count})',
```

In `src/i18n/es.ts`:
```typescript
  cloudBrowserTitle: 'Importar desde Google Drive',
  cloudBrowserLoading: 'Cargando …',
  cloudBrowserEmpty: 'No hay archivos en esta carpeta',
  cloudBrowserImportButton: 'Importar ({count})',
```

In `src/i18n/fr.ts`:
```typescript
  cloudBrowserTitle: 'Importer depuis Google Drive',
  cloudBrowserLoading: 'Chargement …',
  cloudBrowserEmpty: 'Aucun fichier dans ce dossier',
  cloudBrowserImportButton: 'Importer ({count})',
```

- [ ] **Step 3: `src/components/CloudBrowserDialog.tsx` schreiben**

```tsx
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n/LanguageContext';

interface CloudEntryDto {
  id: string;
  name: string;
  isFolder: boolean;
  modifiedTime: string;
  sizeBytes: number | null;
}

interface Crumb {
  id: string | null;
  name: string;
}

interface Props {
  onClose: () => void;
  onImport: (fileIds: string[]) => void;
}

export function CloudBrowserDialog({ onClose, onImport }: Props) {
  const t = useT();
  const [crumbs, setCrumbs] = useState<Crumb[]>([{ id: null, name: 'Google Drive' }]);
  const [entries, setEntries] = useState<CloudEntryDto[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);

  const currentFolderId = crumbs[crumbs.length - 1].id;

  useEffect(() => {
    setLoading(true);
    invoke<CloudEntryDto[]>('browse_cloud_folder', { folderId: currentFolderId })
      .then(setEntries)
      .catch((e) => console.error('[cloud] Ordner konnte nicht geladen werden:', e))
      .finally(() => setLoading(false));
  }, [currentFolderId]);

  const openFolder = (entry: CloudEntryDto) => {
    setCrumbs((prev) => [...prev, { id: entry.id, name: entry.name }]);
  };

  const goToCrumb = (index: number) => {
    setCrumbs((prev) => prev.slice(0, index + 1));
  };

  const toggleSelected = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/50">
      <div className="w-[480px] max-h-[560px] flex flex-col bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)]">
        <div className="flex-none px-4 py-3 border-b border-[var(--line)] text-[13px] font-semibold">
          {t('cloudBrowserTitle')}
        </div>

        <div className="flex-none flex items-center gap-1 px-4 py-2 border-b border-[var(--line)] font-mono-ui text-[11px] text-[var(--ink-2)] overflow-x-auto whitespace-nowrap">
          {crumbs.map((crumb, index) => (
            <span key={crumb.id ?? 'root'} className="flex items-center gap-1">
              {index > 0 && <span className="text-[var(--ink-3)]">/</span>}
              <span
                onClick={() => goToCrumb(index)}
                className={`cursor-pointer ${
                  index === crumbs.length - 1 ? 'text-[var(--ink)]' : 'hover:text-[var(--accent)]'
                }`}
              >
                {crumb.name}
              </span>
            </span>
          ))}
        </div>

        <div className="flex-1 overflow-y-auto px-2 py-2">
          {loading ? (
            <div className="px-2 py-4 text-[12.5px] text-[var(--ink-3)]">{t('cloudBrowserLoading')}</div>
          ) : entries.length === 0 ? (
            <div className="px-2 py-4 text-[12.5px] text-[var(--ink-3)]">{t('cloudBrowserEmpty')}</div>
          ) : (
            entries.map((entry) => (
              <div
                key={entry.id}
                onClick={() => (entry.isFolder ? openFolder(entry) : toggleSelected(entry.id))}
                className="flex items-center gap-2 h-8 px-2 rounded-[3px] text-[13px] cursor-pointer hover:bg-[var(--panel-2)]"
              >
                {!entry.isFolder && (
                  <input
                    type="checkbox"
                    checked={selected.has(entry.id)}
                    onChange={() => toggleSelected(entry.id)}
                    onClick={(e) => e.stopPropagation()}
                    className="cursor-pointer"
                  />
                )}
                <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{entry.name}</span>
                {entry.isFolder && (
                  <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">▸</span>
                )}
              </div>
            ))
          )}
        </div>

        <div className="flex-none flex gap-2 px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
          <button
            onClick={onClose}
            className="flex-1 h-8 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('cancel')}
          </button>
          <button
            onClick={() => onImport(Array.from(selected))}
            disabled={selected.size === 0}
            className="flex-1 h-8 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {t('cloudBrowserImportButton').replace('{count}', String(selected.size))}
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Kompilieren prüfen**

Run: `npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/components/CloudBrowserDialog.tsx src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "$(cat <<'EOF'
feat: CloudBrowserDialog-Komponente fuer Google-Drive-Ordner-Navigation

Zentriertes Modal (neues Muster, bisher gab es nur koordinaten-
positionierte Popups). Breadcrumb-Navigation, Mehrfachauswahl per
Checkbox. Noch nicht in App.tsx/Header.tsx eingebunden (naechster
Task).
EOF
)"
```

---

## Task 10: Frontend — Header/App-Verdrahtung und On-Demand-Änderungserkennung

**Files:**
- Modify: `src/components/Header.tsx`
- Modify: `src/App.tsx`
- Modify: `src/i18n/types.ts`
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`

**Interfaces:**
- Consumes: `CloudBrowserDialog` (Task 9); `import_from_cloud`/`check_cloud_sync_status`-Commands (Task 7/8)
- Produces: keine neuen Exporte — reine Verdrahtung

- [ ] **Step 1: Fünften i18n-Key `importFromCloudOption` in `src/i18n/types.ts` ergänzen**

Innerhalb des `Translations`-Interfaces, direkt nach `cloudBrowserImportButton`:
```typescript
  importFromCloudOption: string;
```

- [ ] **Step 2: Key in allen vier Wörterbüchern ergänzen**

In `src/i18n/de.ts`, nach `cloudBrowserImportButton`:
```typescript
  importFromCloudOption: 'Aus Google Drive importieren…',
```
In `src/i18n/en.ts`:
```typescript
  importFromCloudOption: 'Import from Google Drive…',
```
In `src/i18n/es.ts`:
```typescript
  importFromCloudOption: 'Importar desde Google Drive…',
```
In `src/i18n/fr.ts`:
```typescript
  importFromCloudOption: 'Importer depuis Google Drive…',
```

- [ ] **Step 3: `src/components/Header.tsx` — neuer Menüpunkt**

Props-Interface, aktueller Inhalt:
```tsx
interface Props {
  view: ViewMode;
  onViewChange: (v: ViewMode) => void;
  sort: SortKey;
  onSortChange: (s: SortKey) => void;
  count: number;
  themeSetting: ThemeSetting;
  onThemeChange: (t: ThemeSetting) => void;
  onImportFiles: () => void;
  onImportFolder: () => void;
}
```

Neuer Inhalt:
```tsx
interface Props {
  view: ViewMode;
  onViewChange: (v: ViewMode) => void;
  sort: SortKey;
  onSortChange: (s: SortKey) => void;
  count: number;
  themeSetting: ThemeSetting;
  onThemeChange: (t: ThemeSetting) => void;
  onImportFiles: () => void;
  onImportFolder: () => void;
  cloudDriveConnected: boolean;
  onImportFromCloud: () => void;
}
```

Funktionssignatur, aktueller Inhalt:
```tsx
export function Header({
  view,
  onViewChange,
  sort,
  onSortChange,
  count,
  themeSetting,
  onThemeChange,
  onImportFiles,
  onImportFolder,
}: Props) {
```

Neuer Inhalt:
```tsx
export function Header({
  view,
  onViewChange,
  sort,
  onSortChange,
  count,
  themeSetting,
  onThemeChange,
  onImportFiles,
  onImportFolder,
  cloudDriveConnected,
  onImportFromCloud,
}: Props) {
```

Import-Dropdown, aktueller Inhalt:
```tsx
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFolder();
              }}
              className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFolderOption')}
            </button>
          </div>
        )}
      </div>
```

Neuer Inhalt:
```tsx
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFolder();
              }}
              className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFolderOption')}
            </button>
            {cloudDriveConnected && (
              <button
                onClick={() => {
                  setImportMenuOpen(false);
                  onImportFromCloud();
                }}
                className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
              >
                {t('importFromCloudOption')}
              </button>
            )}
          </div>
        )}
      </div>
```

- [ ] **Step 4: `src/App.tsx` — Dialog-State, Import-Handler, Änderungserkennung**

Import-Zeile ergänzen:

Aktueller Inhalt:
```tsx
import { ContextMenu } from './components/ContextMenu';
```

Neuer Inhalt:
```tsx
import { ContextMenu } from './components/ContextMenu';
import { CloudBrowserDialog } from './components/CloudBrowserDialog';
```

Neuer State (nach der bestehenden `contextMenu`-State-Deklaration):

Aktueller Inhalt:
```tsx
  const [contextMenu, setContextMenu] = useState<{ modelId: string; x: number; y: number } | null>(null);
```

Neuer Inhalt:
```tsx
  const [contextMenu, setContextMenu] = useState<{ modelId: string; x: number; y: number } | null>(null);
  const [cloudBrowserOpen, setCloudBrowserOpen] = useState(false);
```

Nach der bestehenden `refreshClouds`/`connectCloud`-Deklaration (vor `mergeImported`) einfügen:
```tsx

  const handleCloudImport = (fileIds: string[]) => {
    setCloudBrowserOpen(false);
    invoke<ModelFile[]>('import_from_cloud', { fileIds })
      .then(mergeImported)
      .catch((e) => {
        console.error('[cloud] Import aus Google Drive fehlgeschlagen:', e);
        setCloudError(String(e));
      });
  };
```

Neuer `useEffect` für die On-Demand-Änderungserkennung (nach dem bestehenden Drag-and-Drop-`useEffect`, vor `const filtered = useMemo(...)`):
```tsx

  useEffect(() => {
    const model = models.find((m) => m.id === selectedId);
    if (!model || model.origin === 'local') return;
    invoke<string>('check_cloud_sync_status', { fileId: model.id })
      .then((status) => {
        setModels((prev) =>
          prev.map((m) => (m.id === model.id ? { ...m, sync: status as ModelFile['sync'] } : m)),
        );
      })
      .catch((e) => console.error('[cloud] Sync-Check fehlgeschlagen:', e));
  }, [selectedId]);
```

`Header`-Aufruf im JSX, aktueller Inhalt:
```tsx
      <Header
        view={view}
        onViewChange={setView}
        sort={sort}
        onSortChange={setSort}
        count={filtered.length}
        themeSetting={setting}
        onThemeChange={setTheme}
        onImportFiles={importFiles}
        onImportFolder={importFolder}
      />
```

Neuer Inhalt:
```tsx
      <Header
        view={view}
        onViewChange={setView}
        sort={sort}
        onSortChange={setSort}
        count={filtered.length}
        themeSetting={setting}
        onThemeChange={setTheme}
        onImportFiles={importFiles}
        onImportFolder={importFolder}
        cloudDriveConnected={clouds.some((c) => c.id === 'gdrive' && c.status === 'connected')}
        onImportFromCloud={() => setCloudBrowserOpen(true)}
      />
```

Dialog-Rendering: am Ende der Komponente einfügen, direkt nach dem bestehenden `{contextMenu && (...)}`-Block, vor dem schließenden `</div>` der Wurzelkomponente:

Aktueller Inhalt:
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
    </div>
  );
}
```

Neuer Inhalt:
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

      {cloudBrowserOpen && (
        <CloudBrowserDialog onClose={() => setCloudBrowserOpen(false)} onImport={handleCloudImport} />
      )}
    </div>
  );
}
```

- [ ] **Step 5: Kompilieren prüfen**

Run: `npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 6: Manuell im Browser prüfen (Dev-Server)**

Run: `npm run tauri dev` kurz starten. Erwartet: Solange kein Google-Drive-Konto verbunden ist, taucht „Aus Google Drive importieren…" im Import-Menü nicht auf (kein Absturz). Falls in dieser Sandbox kein interaktiver GUI-Test möglich ist: mindestens sauberer Start ohne Panics als Minimalnachweis, mit klarer Kennzeichnung im Bericht. Dev-Server danach beenden.

- [ ] **Step 7: Commit**

```bash
git add src/components/Header.tsx src/App.tsx src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "$(cat <<'EOF'
feat: Google-Drive-Import und On-Demand-Aenderungserkennung verdrahtet

Neuer Menuepunkt im Import-Dropdown (nur sichtbar bei verbundenem
Konto), oeffnet CloudBrowserDialog. Aenderungserkennung laeuft
automatisch beim Auswaehlen einer Cloud-Datei in der Detailansicht,
kein manueller Trigger.
EOF
)"
```

---

## Task 11: Abschlussverifikation

**Files:** keine (nur Build/Test-Ausführung und Hinweis zum manuellen Live-Test)

**Interfaces:** keine

- [ ] **Step 1: Backend vollständig bauen und testen**

Run: `cd src-tauri && cargo build && cargo test`
Expected: Build PASS, alle Tests PASS (bestehende 37 aus Plan 1 plus die in diesem Plan neu hinzugekommenen aus Task 4/5).

- [ ] **Step 2: Frontend vollständig bauen**

Run: `npm run build`
Expected: PASS (`tsc && vite build`).

- [ ] **Step 3: Git-Log prüfen**

Run: `git log --oneline -14` und `git status --short`
Expected: Alle 10 Feature-Commits dieses Plans sichtbar (Task 1-10, in der richtigen Reihenfolge), Arbeitsverzeichnis sauber.

- [ ] **Step 4: Hinweis zum manuellen Live-Test dokumentieren**

Dieser Task kann NICHT automatisiert verifizieren, dass Browse/Import/Änderungserkennung gegen ein echtes Google-Drive-Konto funktionieren. Sobald der Nutzer möchte, sollte er einmal manuell:
1. `npm run tauri dev` starten (Google-Drive-Konto muss bereits verbunden sein, siehe Plan 1)
2. Im Import-Menü „Aus Google Drive importieren…" wählen
3. Durch Ordner navigieren (Breadcrumb-Klick zum Zurückgehen), eine 3MF- oder STL-Datei auswählen, „Importieren" klicken
4. Prüfen, dass die Datei im Katalog erscheint (Thumbnail/Metadaten wie bei lokalem Import)
5. Die importierte Datei in der Detailansicht öffnen, prüfen, dass der Sync-Status "aktuell" zeigt
6. In Google Drive selbst die Datei ersetzen/ändern, dann in der App erneut auswählen (Detailansicht neu öffnen) — Sync-Status sollte auf "veraltet" wechseln

- [ ] **Step 5: Abschluss-Zusammenfassung an den User**

Kurze Zusammenfassung: Google-Drive-Datei-Browsing, -Import und On-Demand-Änderungserkennung sind vollständig implementiert, Build und Tests grün, echter Live-Test steht noch aus (Nutzeraufgabe). Hochladen zu Google Drive ist bewusst nicht Teil dieses Plans — das wäre die dritte, separate Runde auf demselben Fundament (`GoogleDriveProvider::upload` ist aktuell noch ein Platzhalter).

---
