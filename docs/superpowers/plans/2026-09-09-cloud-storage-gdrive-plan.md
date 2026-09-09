# Cloud-Anbindung Google Drive — Fundament (OAuth, Token-Speicherung, Konto verbinden/trennen) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Google Drive als echtes, verbindbares/trennbares Cloud-Konto in der Seitenleiste — vollständiger PKCE-Loopback-OAuth2-Flow, sichere Token-Ablage im OS-Schlüsselbund, `cloud_accounts`-Tabelle, `StorageProvider`-Trait als Fundament für Dateizugriff. **Noch nicht Teil dieses Plans:** Drive-Dateien durchsuchen/importieren/hochladen — das ist Plan 2, der auf dem hier gebauten `StorageProvider`-Trait und den Tokens aufsetzt.

**Architecture:** Neues Rust-Modul `src-tauri/src/cloud/` (Trait, OAuth-Helfer, Token-Ablage, Config-Laden, Google-Account-Abfrage, Tauri-Commands). Neue `cloud_accounts`-SQLite-Tabelle. Frontend ersetzt die hartcodierte `CLOUDS`-Beispieldaten-Konstante in `App.tsx` durch echte Backend-Aufrufe.

**Tech Stack:** Tauri v2 (Rust/`rusqlite`), React 19 + TypeScript 6. Neu: `oauth2` 5.0.0 (PKCE-Flow), `keyring` 4.2.0 (Token-Ablage), `reqwest` 0.13.5 (Google-API-Calls), `async-trait` 0.1.92 (für `Box<dyn StorageProvider>` — natives `async fn` in Traits ist nicht objektsicher).

## Global Constraints

- Spec: `docs/superpowers/specs/2026-09-09-cloud-storage-design.md` — alle Abschnitte dieses Plans referenzieren die dortige Architektur.
- Ein verbundenes Google-Konto gleichzeitig (technisch erzwungen über `UNIQUE(provider)` in `cloud_accounts`).
- **Keine Tokens/Secrets in SQLite oder Git.** Tokens ausschließlich über `keyring`; OAuth-Client-ID/-Secret ausschließlich über `src-tauri/cloud.config.json` (git-ignored), mit `cloud.config.example.json` als Vorlage.
- **Testmethode:** Rust-Module in diesem Plan sind neu und in sich abgeschlossen — Implementierung und Tests werden je Datei gemeinsam geschrieben (kein isolierter Rot/Grün-Zyklus pro einzelner Assertion), danach `cargo test <modulname>::` zur Verifikation. `KeyringTokenStore` (echter OS-Schlüsselbund) wird **nicht** automatisiert getestet — nur `InMemoryTokenStore` (Test-Fake). Der komplette OAuth-Live-Flow gegen echtes Google-Konto ist manuelle Nutzerarbeit (siehe Task 10), kein automatisierter Verifikationsschritt.
- Der OAuth-Token-Austausch (`cloud/oauth.rs`) nutzt zwingend `oauth2::reqwest` (das im `oauth2`-Crate gebündelte, separat versionierte reqwest) — NICHT die eigenständig hinzugefügte `reqwest`-Abhängigkeit. Die eigenständige `reqwest`-Abhängigkeit ist ausschließlich für eigene Google-API-Calls (`cloud/gdrive.rs`) gedacht. Beide Versionen koexistieren unabhängig in `Cargo.lock`, das ist beabsichtigt, kein Fehler.
- Browser-Öffnen für den OAuth-Consent-Schritt läuft über das bereits vorhandene `tauri-plugin-opener` (`use tauri_plugin_opener::OpenerExt; app.opener().open_url(url, None::<&str>)`) — keine neue Browser-Öffnen-Bibliothek.
- Git-Konventionen: explizites `git add <file1> <file2>` pro Datei (nie `-A`/`.`), deutsche Commit-Messages via HEREDOC, nie `--amend`.
- Nach jedem Task: kurze Zusammenfassung und Rücksprache mit dem User, bevor der nächste Task begonnen wird — **außer** der User hat per `/goal` explizit durchgehende Ausführung angeordnet (wie für diesen Plan geschehen).

---

## File Structure

**Create:**
- `src-tauri/src/cloud/mod.rs`
- `src-tauri/src/cloud/provider.rs`
- `src-tauri/src/cloud/tokens.rs`
- `src-tauri/src/cloud/config.rs`
- `src-tauri/src/cloud/oauth.rs`
- `src-tauri/src/cloud/gdrive.rs`
- `src-tauri/src/cloud/commands.rs`
- `src-tauri/cloud.config.example.json`

**Modify:**
- `src-tauri/Cargo.toml`
- `src-tauri/.gitignore`
- `src-tauri/src/lib.rs`
- `src-tauri/src/db/schema.sql`
- `src-tauri/src/db/models.rs`
- `src-tauri/src/db/repository.rs`
- `src-tauri/src/db/mod.rs`
- `src-tauri/src/commands.rs` (nur `lock_db` sichtbar für `cloud::commands` machen)
- `src/App.tsx`
- `src/components/Sidebar.tsx`

---

## Task 1: Rust-Abhängigkeiten hinzufügen

**Files:**
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: nichts (Fundament-Task)
- Produces: `oauth2`, `keyring`, `reqwest`, `async-trait` als `[dependencies]`; `tokio` als `[dev-dependencies]` (nötig für `#[tokio::test]` in späteren Tasks — Tauris eigene Async-Runtime deckt Produktionscode ab, aber isolierte `cargo test`-Läufe brauchen eine eigene Runtime für `async fn`-Tests)

- [ ] **Step 1: Abhängigkeiten in `src-tauri/Cargo.toml` ergänzen**

Aktueller `[dependencies]`-Block endet mit:
```toml
base64 = "0.22"
```

Danach einfügen (vor der Leerzeile zu `[profile.release]`):
```toml
oauth2 = "5.0.0"
keyring = "4.2.0"
reqwest = { version = "0.13.5", features = ["json"] }
async-trait = "0.1.92"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

- [ ] **Step 2: Bauen prüfen**

Run: `cd src-tauri && cargo build`
Expected: PASS, nur bereits bestehende Dead-Code-Warnungen, keine neuen Fehler. `Cargo.lock` bekommt neue Einträge, u. a. zwei verschiedene `reqwest`-Versionen (0.12.x transitiv über `oauth2`, 0.13.5 direkt) — das ist erwartet, siehe Global Constraints.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "$(cat <<'EOF'
build: Rust-Abhaengigkeiten fuer Google-Drive-OAuth ergaenzt

oauth2 (PKCE-Flow), keyring (Token-Ablage), reqwest (Google-API-Calls),
async-trait (fuer Box<dyn StorageProvider>). tokio als Dev-Dependency
fuer async Unit-Tests spaeterer Tasks.
EOF
)"
```

---

## Task 2: `cloud_accounts`-Tabelle, Model und Repository-Funktionen

**Files:**
- Modify: `src-tauri/src/db/schema.sql`
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/db/repository.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Test: `src-tauri/src/db/mod.rs` (bestehendes `#[cfg(test)] mod tests`-Modul erweitern)

**Interfaces:**
- Consumes: nichts Neues (nutzt bestehende `Connection`/`DbError`/`connect_in_memory`-Infrastruktur)
- Produces:
  - `pub struct CloudAccountRecord { id: i64, provider: String, account_label: String, status: String, connected_at: String }`
  - `pub fn upsert_cloud_account(conn: &Connection, provider: &str, account_label: &str, connected_at: &str) -> Result<i64, DbError>`
  - `pub fn list_cloud_accounts(conn: &Connection) -> Result<Vec<CloudAccountRecord>, DbError>`
  - `pub fn set_cloud_account_status(conn: &Connection, provider: &str, status: &str) -> Result<(), DbError>`

- [ ] **Step 1: Tabelle in `src-tauri/src/db/schema.sql` ergänzen**

Am Ende der Datei anfügen:
```sql

CREATE TABLE IF NOT EXISTS cloud_accounts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL CHECK (provider IN ('gdrive', 'onedrive', 'dropbox', 'proton')),
    account_label TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'connected' CHECK (status IN ('connected', 'error', 'disconnected')),
    connected_at TEXT NOT NULL,
    UNIQUE (provider)
);
```

- [ ] **Step 2: `CloudAccountRecord` in `src-tauri/src/db/models.rs` ergänzen**

Am Ende der Datei anfügen:
```rust

#[derive(Debug, Clone)]
pub struct CloudAccountRecord {
    pub id: i64,
    pub provider: String,
    pub account_label: String,
    pub status: String,
    pub connected_at: String,
}
```

- [ ] **Step 3: Repository-Funktionen in `src-tauri/src/db/repository.rs` ergänzen**

Import-Zeile am Dateianfang:
```rust
use super::models::{CloudAccountRecord, FileRecord, FileType, FolderRecord, MaterialRecord, NewFile, TagCount};
```
ersetzt die bestehende:
```rust
use super::models::{FileRecord, FileType, FolderRecord, MaterialRecord, NewFile, TagCount};
```

Am Ende der Datei anfügen:
```rust

pub fn upsert_cloud_account(
    conn: &Connection,
    provider: &str,
    account_label: &str,
    connected_at: &str,
) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO cloud_accounts (provider, account_label, status, connected_at)
         VALUES (?1, ?2, 'connected', ?3)
         ON CONFLICT(provider) DO UPDATE SET
             account_label = excluded.account_label,
             status = 'connected',
             connected_at = excluded.connected_at",
        params![provider, account_label, connected_at],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM cloud_accounts WHERE provider = ?1",
        params![provider],
        |row| row.get(0),
    )?)
}

pub fn list_cloud_accounts(conn: &Connection) -> Result<Vec<CloudAccountRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, account_label, status, connected_at FROM cloud_accounts ORDER BY provider",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CloudAccountRecord {
            id: row.get(0)?,
            provider: row.get(1)?,
            account_label: row.get(2)?,
            status: row.get(3)?,
            connected_at: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn set_cloud_account_status(conn: &Connection, provider: &str, status: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE cloud_accounts SET status = ?1 WHERE provider = ?2",
        params![status, provider],
    )?;
    Ok(())
}
```

- [ ] **Step 4: Re-Exports in `src-tauri/src/db/mod.rs` ergänzen**

Aktueller Inhalt:
```rust
pub use repository::{
    add_tag_to_file, connect, delete_file, file_exists_by_path, get_file, insert_file,
    insert_folder, list_files, list_folders, list_tag_counts, remove_tag_from_file,
};
```

Neuer Inhalt:
```rust
pub use repository::{
    add_tag_to_file, connect, delete_file, file_exists_by_path, get_file, insert_file,
    insert_folder, list_cloud_accounts, list_files, list_folders, list_tag_counts,
    remove_tag_from_file, set_cloud_account_status, upsert_cloud_account,
};
```

- [ ] **Step 5: Tests in `src-tauri/src/db/mod.rs`s bestehendem `#[cfg(test)] mod tests`-Block ergänzen**

Am Ende des `mod tests`-Blocks (vor der letzten schließenden `}`) anfügen:
```rust

    #[test]
    fn upserts_and_lists_cloud_accounts() {
        let conn = connect_in_memory().expect("connect");
        let id = upsert_cloud_account(&conn, "gdrive", "user@example.com", "2026-09-09T12:00:00Z")
            .expect("upsert");
        assert!(id > 0);

        let accounts = list_cloud_accounts(&conn).expect("list");
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].provider, "gdrive");
        assert_eq!(accounts[0].account_label, "user@example.com");
        assert_eq!(accounts[0].status, "connected");
    }

    #[test]
    fn upsert_cloud_account_updates_existing_row_for_same_provider() {
        let conn = connect_in_memory().expect("connect");
        upsert_cloud_account(&conn, "gdrive", "first@example.com", "2026-09-09T12:00:00Z")
            .expect("first upsert");
        upsert_cloud_account(&conn, "gdrive", "second@example.com", "2026-09-09T13:00:00Z")
            .expect("second upsert");

        let accounts = list_cloud_accounts(&conn).expect("list");
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].account_label, "second@example.com");
    }

    #[test]
    fn sets_cloud_account_status() {
        let conn = connect_in_memory().expect("connect");
        upsert_cloud_account(&conn, "gdrive", "user@example.com", "2026-09-09T12:00:00Z")
            .expect("upsert");
        set_cloud_account_status(&conn, "gdrive", "disconnected").expect("set status");

        let accounts = list_cloud_accounts(&conn).expect("list");
        assert_eq!(accounts[0].status, "disconnected");
    }
```

- [ ] **Step 6: Testen**

Run: `cd src-tauri && cargo test db::`
Expected: PASS, bestehende DB-Tests weiterhin grün plus die 3 neuen (insgesamt mehr als die vorherigen 8 db-Tests).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/schema.sql src-tauri/src/db/models.rs src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs
git commit -m "$(cat <<'EOF'
feat: cloud_accounts-Tabelle und Repository-Funktionen

Neue Tabelle mit UNIQUE(provider) fuer die v1-Einschraenkung "ein
Konto pro Anbieter". Kein Token-Feld - Tokens leben ausschliesslich
im OS-Schluesselbund (naechster Task).
EOF
)"
```

---

## Task 3: `StorageProvider`-Trait und `CloudEntry`/`CloudError`-Typen

**Files:**
- Create: `src-tauri/src/cloud/mod.rs`
- Create: `src-tauri/src/cloud/provider.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: nichts
- Produces:
  - `pub struct CloudEntry { id: String, name: String, is_folder: bool, modified_time: String, size_bytes: Option<i64> }`
  - `pub enum CloudError { Network(String), Auth(String), NotFound(String) }` (implementiert `Display`/`Error`)
  - `pub type CloudResult<T> = Result<T, CloudError>`
  - `#[async_trait] pub trait StorageProvider: Send + Sync { async fn list_folder(...); async fn download(...); async fn upload(...); async fn get_metadata(...); }`

Dieser Trait hat in diesem Plan noch keine konkrete Google-Drive-Implementierung — das browsen/herunterladen/hochladen echter Drive-Dateien ist Plan 2. Hier wird nur das gemeinsame Interface festgelegt und über einen Mock-Provider bewiesen, dass es objektsicher ist (`Box<dyn StorageProvider>`), da spätere Provider (OneDrive, Dropbox, Proton) polymorph darüber laufen sollen.

- [ ] **Step 1: `src-tauri/src/cloud/provider.rs` schreiben**

```rust
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct CloudEntry {
    pub id: String,
    pub name: String,
    pub is_folder: bool,
    pub modified_time: String, // RFC3339
    pub size_bytes: Option<i64>,
}

#[derive(Debug)]
pub enum CloudError {
    Network(String),
    Auth(String),
    NotFound(String),
}

impl std::fmt::Display for CloudError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloudError::Network(msg) => write!(f, "network error: {msg}"),
            CloudError::Auth(msg) => write!(f, "authentication error: {msg}"),
            CloudError::NotFound(msg) => write!(f, "not found: {msg}"),
        }
    }
}

impl std::error::Error for CloudError {}

pub type CloudResult<T> = Result<T, CloudError>;

#[async_trait]
pub trait StorageProvider: Send + Sync {
    async fn list_folder(&self, folder_id: Option<&str>) -> CloudResult<Vec<CloudEntry>>;
    async fn download(&self, file_id: &str) -> CloudResult<Vec<u8>>;
    async fn upload(&self, folder_id: Option<&str>, file_name: &str, data: &[u8]) -> CloudResult<CloudEntry>;
    async fn get_metadata(&self, file_id: &str) -> CloudResult<CloudEntry>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockProvider {
        entries: Mutex<HashMap<String, Vec<u8>>>,
    }

    #[async_trait]
    impl StorageProvider for MockProvider {
        async fn list_folder(&self, _folder_id: Option<&str>) -> CloudResult<Vec<CloudEntry>> {
            Ok(vec![CloudEntry {
                id: "file-1".to_string(),
                name: "cube.3mf".to_string(),
                is_folder: false,
                modified_time: "2026-09-09T12:00:00Z".to_string(),
                size_bytes: Some(1024),
            }])
        }

        async fn download(&self, file_id: &str) -> CloudResult<Vec<u8>> {
            self.entries
                .lock()
                .unwrap()
                .get(file_id)
                .cloned()
                .ok_or_else(|| CloudError::NotFound(file_id.to_string()))
        }

        async fn upload(&self, _folder_id: Option<&str>, file_name: &str, data: &[u8]) -> CloudResult<CloudEntry> {
            self.entries.lock().unwrap().insert(file_name.to_string(), data.to_vec());
            Ok(CloudEntry {
                id: file_name.to_string(),
                name: file_name.to_string(),
                is_folder: false,
                modified_time: "2026-09-09T12:00:00Z".to_string(),
                size_bytes: Some(data.len() as i64),
            })
        }

        async fn get_metadata(&self, file_id: &str) -> CloudResult<CloudEntry> {
            self.entries
                .lock()
                .unwrap()
                .get(file_id)
                .map(|data| CloudEntry {
                    id: file_id.to_string(),
                    name: file_id.to_string(),
                    is_folder: false,
                    modified_time: "2026-09-09T12:00:00Z".to_string(),
                    size_bytes: Some(data.len() as i64),
                })
                .ok_or_else(|| CloudError::NotFound(file_id.to_string()))
        }
    }

    #[tokio::test]
    async fn mock_provider_satisfies_storage_provider_as_trait_object() {
        let provider: Box<dyn StorageProvider> = Box::new(MockProvider {
            entries: Mutex::new(HashMap::new()),
        });

        let entries = provider.list_folder(None).await.expect("list");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "cube.3mf");

        let uploaded = provider.upload(None, "test.stl", b"data").await.expect("upload");
        assert_eq!(uploaded.id, "test.stl");

        let downloaded = provider.download("test.stl").await.expect("download");
        assert_eq!(downloaded, b"data");

        let meta = provider.get_metadata("test.stl").await.expect("metadata");
        assert_eq!(meta.size_bytes, Some(4));
    }
}
```

- [ ] **Step 2: `src-tauri/src/cloud/mod.rs` schreiben**

```rust
pub mod provider;
```

- [ ] **Step 3: Modul in `src-tauri/src/lib.rs` einbinden**

Aktueller Inhalt (Zeilen 1-6):
```rust
mod commands;
mod db;
mod geometry;
mod stl;
mod tagging;
mod threemf;
```

Neuer Inhalt:
```rust
mod cloud;
mod commands;
mod db;
mod geometry;
mod stl;
mod tagging;
mod threemf;
```

- [ ] **Step 4: Testen**

Run: `cd src-tauri && cargo build && cargo test cloud::`
Expected: `cargo build` PASS (Modul kompiliert, auch wenn noch nirgends konsumiert — erzeugt ggf. eine `dead_code`-Warnung für den noch ungenutzten Trait, das ist an dieser Stelle im Plan erwartet und wird ab Task 6 aufgelöst). `cargo test cloud::` PASS mit dem einen neuen Test.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/cloud/mod.rs src-tauri/src/cloud/provider.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: StorageProvider-Trait als Cloud-Provider-Abstraktion

CloudEntry/CloudError/CloudResult plus #[async_trait]-Trait, objekt-
sicher (Box<dyn StorageProvider>) fuer spaetere polymorphe Provider.
Google-Drive-Implementierung folgt in Plan 2, hier nur das Interface
plus Mock-Beweis der Objektsicherheit.
EOF
)"
```

---

## Task 4: Token-Speicherung — `TokenStore`-Trait, `KeyringTokenStore`, `InMemoryTokenStore`

**Files:**
- Create: `src-tauri/src/cloud/tokens.rs`
- Modify: `src-tauri/src/cloud/mod.rs`

**Interfaces:**
- Consumes: nichts
- Produces:
  - `pub struct StoredTokens { access_token: String, refresh_token: Option<String> }`
  - `pub trait TokenStore: Send + Sync { fn save(&self, account_key: &str, tokens: &StoredTokens) -> Result<(), String>; fn load(&self, account_key: &str) -> Result<Option<StoredTokens>, String>; fn delete(&self, account_key: &str) -> Result<(), String>; }`
  - `pub struct KeyringTokenStore;` (implementiert `TokenStore` gegen den echten OS-Schlüsselbund)
  - `#[cfg(test)] pub struct InMemoryTokenStore { ... }` (Test-Fake, implementiert `TokenStore`)

`account_key` ist die Schlüsselbund-Kennung, Konvention `"<provider>:<account_label>"` (z. B. `"gdrive:user@example.com"`).

- [ ] **Step 1: `src-tauri/src/cloud/tokens.rs` schreiben**

```rust
use keyring::Entry;

const SERVICE_NAME: &str = "3mf-katalog-manager";

#[derive(Debug, Clone)]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedTokens {
    access_token: String,
    refresh_token: Option<String>,
}

pub trait TokenStore: Send + Sync {
    fn save(&self, account_key: &str, tokens: &StoredTokens) -> Result<(), String>;
    fn load(&self, account_key: &str) -> Result<Option<StoredTokens>, String>;
    fn delete(&self, account_key: &str) -> Result<(), String>;
}

/// Speichert Tokens im OS-Schluesselbund (libsecret/Keychain/Credential
/// Manager je Plattform, ueber das `keyring`-Crate). Wird bewusst NICHT
/// automatisiert getestet - ein Test wuerde den echten System-Schluesselbund
/// beruehren, der in CI/Sandbox-Umgebungen nicht garantiert verfuegbar ist.
/// Fuer Tests siehe `InMemoryTokenStore` unten.
pub struct KeyringTokenStore;

impl TokenStore for KeyringTokenStore {
    fn save(&self, account_key: &str, tokens: &StoredTokens) -> Result<(), String> {
        let serialized = serde_json::to_string(&SerializedTokens {
            access_token: tokens.access_token.clone(),
            refresh_token: tokens.refresh_token.clone(),
        })
        .map_err(|e| e.to_string())?;
        let entry = Entry::new(SERVICE_NAME, account_key).map_err(|e| e.to_string())?;
        entry.set_password(&serialized).map_err(|e| e.to_string())
    }

    fn load(&self, account_key: &str) -> Result<Option<StoredTokens>, String> {
        let entry = Entry::new(SERVICE_NAME, account_key).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(serialized) => {
                let parsed: SerializedTokens =
                    serde_json::from_str(&serialized).map_err(|e| e.to_string())?;
                Ok(Some(StoredTokens {
                    access_token: parsed.access_token,
                    refresh_token: parsed.refresh_token,
                }))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn delete(&self, account_key: &str) -> Result<(), String> {
        let entry = Entry::new(SERVICE_NAME, account_key).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
pub struct InMemoryTokenStore {
    entries: std::sync::Mutex<std::collections::HashMap<String, StoredTokens>>,
}

#[cfg(test)]
impl InMemoryTokenStore {
    pub fn new() -> Self {
        Self {
            entries: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[cfg(test)]
impl TokenStore for InMemoryTokenStore {
    fn save(&self, account_key: &str, tokens: &StoredTokens) -> Result<(), String> {
        self.entries
            .lock()
            .unwrap()
            .insert(account_key.to_string(), tokens.clone());
        Ok(())
    }

    fn load(&self, account_key: &str) -> Result<Option<StoredTokens>, String> {
        Ok(self.entries.lock().unwrap().get(account_key).cloned())
    }

    fn delete(&self, account_key: &str) -> Result<(), String> {
        self.entries.lock().unwrap().remove(account_key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_round_trips_tokens() {
        let store = InMemoryTokenStore::new();
        let tokens = StoredTokens {
            access_token: "access-123".to_string(),
            refresh_token: Some("refresh-456".to_string()),
        };
        store.save("gdrive:user@example.com", &tokens).expect("save");

        let loaded = store
            .load("gdrive:user@example.com")
            .expect("load")
            .expect("present");
        assert_eq!(loaded.access_token, "access-123");
        assert_eq!(loaded.refresh_token, Some("refresh-456".to_string()));
    }

    #[test]
    fn in_memory_store_returns_none_for_missing_account() {
        let store = InMemoryTokenStore::new();
        assert!(store.load("gdrive:nobody@example.com").expect("load").is_none());
    }

    #[test]
    fn in_memory_store_deletes_tokens() {
        let store = InMemoryTokenStore::new();
        let tokens = StoredTokens {
            access_token: "a".to_string(),
            refresh_token: None,
        };
        store.save("gdrive:user@example.com", &tokens).expect("save");
        store.delete("gdrive:user@example.com").expect("delete");
        assert!(store.load("gdrive:user@example.com").expect("load").is_none());
    }
}
```

- [ ] **Step 2: `src-tauri/src/cloud/mod.rs` erweitern**

Aktueller Inhalt:
```rust
pub mod provider;
```

Neuer Inhalt:
```rust
pub mod provider;
pub mod tokens;
```

- [ ] **Step 3: Testen**

Run: `cd src-tauri && cargo test cloud::tokens::`
Expected: PASS, 3 neue Tests (nur gegen `InMemoryTokenStore`, keine Interaktion mit dem echten Schlüsselbund).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/cloud/tokens.rs src-tauri/src/cloud/mod.rs
git commit -m "$(cat <<'EOF'
feat: Token-Speicherung ueber OS-Schluesselbund

KeyringTokenStore fuer den echten Schluesselbund (nicht automatisiert
getestet - beruehrt echtes OS), InMemoryTokenStore als Test-Fake ueber
dieselbe TokenStore-Schnittstelle.
EOF
)"
```

---

## Task 5: Cloud-Konfiguration laden (Client-ID/-Secret)

**Files:**
- Create: `src-tauri/src/cloud/config.rs`
- Create: `src-tauri/cloud.config.example.json`
- Modify: `src-tauri/src/cloud/mod.rs`
- Modify: `src-tauri/.gitignore`

**Interfaces:**
- Consumes: nichts
- Produces:
  - `pub struct CloudConfig { google_client_id: String, google_client_secret: String }` (deserialisierbar)
  - `pub enum ConfigError { Missing(PathBuf), Invalid(String) }` (implementiert `Display`/`Error`)
  - `pub fn load_cloud_config(config_path: &Path) -> Result<CloudConfig, ConfigError>`

- [ ] **Step 1: `src-tauri/src/cloud/config.rs` schreiben**

```rust
use std::path::Path;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CloudConfig {
    pub google_client_id: String,
    pub google_client_secret: String,
}

#[derive(Debug)]
pub enum ConfigError {
    Missing(std::path::PathBuf),
    Invalid(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Missing(path) => write!(
                f,
                "Cloud-Konfiguration fehlt: {} nicht gefunden. Siehe cloud.config.example.json.",
                path.display()
            ),
            ConfigError::Invalid(msg) => write!(f, "Cloud-Konfiguration ungueltig: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

pub fn load_cloud_config(config_path: &Path) -> Result<CloudConfig, ConfigError> {
    if !config_path.exists() {
        return Err(ConfigError::Missing(config_path.to_path_buf()));
    }
    let contents =
        std::fs::read_to_string(config_path).map_err(|e| ConfigError::Invalid(e.to_string()))?;
    serde_json::from_str(&contents).map_err(|e| ConfigError::Invalid(e.to_string()))
}

/// Pfad zu `cloud.config.json` relativ zum `src-tauri`-Verzeichnis, zur
/// Kompilierzeit ueber CARGO_MANIFEST_DIR aufgeloest. Funktioniert damit
/// unabhaengig vom Arbeitsverzeichnis in `cargo build`/`npm run tauri dev`.
/// Fuer signierte Release-Pakete (noch nicht Teil dieses Projekts, siehe
/// Schritt 9 des urspruenglichen Projektauftrags) muesste dies durch einen
/// nutzerbeschreibbaren App-Datenverzeichnis-Pfad ersetzt werden.
pub fn default_config_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cloud.config.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_missing_error_for_nonexistent_file() {
        let result = load_cloud_config(Path::new("/tmp/does-not-exist-3mf-katalog-cloud.json"));
        assert!(matches!(result, Err(ConfigError::Missing(_))));
    }

    #[test]
    fn loads_valid_config() {
        let dir = std::env::temp_dir().join(format!("3mf-cloud-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("cloud.config.json");
        std::fs::write(
            &path,
            r#"{"google_client_id": "test-id", "google_client_secret": "test-secret"}"#,
        )
        .expect("write config");

        let config = load_cloud_config(&path).expect("load");
        assert_eq!(config.google_client_id, "test-id");
        assert_eq!(config.google_client_secret, "test-secret");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn returns_invalid_error_for_malformed_json() {
        let dir =
            std::env::temp_dir().join(format!("3mf-cloud-config-test-invalid-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("cloud.config.json");
        std::fs::write(&path, "not json").expect("write config");

        let result = load_cloud_config(&path);
        assert!(matches!(result, Err(ConfigError::Invalid(_))));

        std::fs::remove_dir_all(&dir).ok();
    }
}
```

- [ ] **Step 2: Beispieldatei `src-tauri/cloud.config.example.json` schreiben**

```json
{
  "google_client_id": "DEINE-CLIENT-ID.apps.googleusercontent.com",
  "google_client_secret": "DEIN-CLIENT-SECRET"
}
```

- [ ] **Step 3: `src-tauri/.gitignore` ergänzen**

Am Ende der Datei anfügen:
```

# Cloud-OAuth-Zugangsdaten, niemals committen
/cloud.config.json
```

- [ ] **Step 4: `src-tauri/src/cloud/mod.rs` erweitern**

Aktueller Inhalt:
```rust
pub mod provider;
pub mod tokens;
```

Neuer Inhalt:
```rust
pub mod config;
pub mod provider;
pub mod tokens;
```

- [ ] **Step 5: Testen**

Run: `cd src-tauri && cargo test cloud::config::`
Expected: PASS, 3 neue Tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/cloud/config.rs src-tauri/cloud.config.example.json src-tauri/.gitignore src-tauri/src/cloud/mod.rs
git commit -m "$(cat <<'EOF'
feat: Cloud-Konfiguration (Google-Client-ID/-Secret) laden

cloud.config.json (git-ignored) mit Beispieldatei. Fehlende Datei
fuehrt zu klarer Fehlermeldung statt Absturz - "Google Drive
verbinden" schlaegt dann kontrolliert fehl (Task 8).
EOF
)"
```

---

## Task 6: PKCE-Loopback-OAuth2-Flow für Google

**Files:**
- Create: `src-tauri/src/cloud/oauth.rs`
- Modify: `src-tauri/src/cloud/mod.rs`

**Interfaces:**
- Consumes: `CloudError`, `CloudResult` aus `provider.rs` (Task 3)
- Produces:
  - `pub struct OAuthTokens { access_token: String, refresh_token: Option<String> }`
  - `pub async fn run_google_oauth_flow(app: &tauri::AppHandle, client_id: &str, client_secret: &str, scopes: &[&str]) -> CloudResult<OAuthTokens>`

Dieser Flow ist **nicht automatisiert testbar** (siehe Global Constraints) — er öffnet echt den System-Browser und wartet auf eine echte Google-Anmeldung. Kein Test in diesem Task; die Verifikation ist der manuelle Durchlauf in Task 10.

- [ ] **Step 1: `src-tauri/src/cloud/oauth.rs` schreiben**

```rust
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use tauri_plugin_opener::OpenerExt;

use crate::cloud::provider::{CloudError, CloudResult};

pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

/// Fuehrt den kompletten Loopback-Redirect-PKCE-Flow fuer Google aus: startet
/// einen kurzlebigen lokalen Server auf einem freien Port, oeffnet die
/// Consent-URL im System-Browser (ueber tauri-plugin-opener), wartet auf
/// genau einen Redirect, tauscht den Code gegen Tokens und beendet den
/// Server wieder. Dies ist Googles offiziell dokumentierter und einzig
/// unterstuetzter Weg fuer Desktop-Apps (RFC 8252) - eingebettete WebViews
/// blockiert Google aktiv, Custom-URI-Schemes akzeptiert der "Desktop-App"-
/// Client-Typ nicht.
pub async fn run_google_oauth_flow(
    app: &tauri::AppHandle,
    client_id: &str,
    client_secret: &str,
    scopes: &[&str],
) -> CloudResult<OAuthTokens> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| CloudError::Network(format!("Loopback-Server konnte nicht gestartet werden: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| CloudError::Network(e.to_string()))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}");

    let client = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_client_secret(ClientSecret::new(client_secret.to_string()))
        .set_auth_uri(
            AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())
                .map_err(|e| CloudError::Auth(e.to_string()))?,
        )
        .set_token_uri(
            TokenUrl::new("https://www.googleapis.com/oauth2/v3/token".to_string())
                .map_err(|e| CloudError::Auth(e.to_string()))?,
        )
        .set_redirect_uri(RedirectUrl::new(redirect_uri).map_err(|e| CloudError::Auth(e.to_string()))?);

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let mut auth_request = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge);
    for scope in scopes {
        auth_request = auth_request.add_scope(Scope::new(scope.to_string()));
    }
    let (authorize_url, csrf_state) = auth_request.url();

    app.opener()
        .open_url(authorize_url.to_string(), None::<&str>)
        .map_err(|e| CloudError::Network(format!("Browser konnte nicht geoeffnet werden: {e}")))?;

    let (code, returned_state) = wait_for_redirect(&listener)?;
    if returned_state.secret() != csrf_state.secret() {
        return Err(CloudError::Auth("CSRF-Status stimmt nicht ueberein".to_string()));
    }

    let http_client = oauth2::reqwest::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| CloudError::Network(e.to_string()))?;

    let token_result = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(&http_client)
        .await
        .map_err(|e| CloudError::Auth(e.to_string()))?;

    Ok(OAuthTokens {
        access_token: token_result.access_token().secret().clone(),
        refresh_token: token_result.refresh_token().map(|t| t.secret().clone()),
    })
}

/// Blockiert, bis der Loopback-Server genau eine Anfrage empfaengt, und
/// extrahiert `code`/`state` aus der Redirect-URL. Sendet eine einfache
/// HTML-Antwort zurueck, damit der Browser-Tab dem Nutzer signalisiert,
/// dass er das Fenster schliessen kann.
fn wait_for_redirect(listener: &TcpListener) -> CloudResult<(String, CsrfToken)> {
    let (mut stream, _) = listener
        .accept()
        .map_err(|e| CloudError::Network(format!("Redirect nicht empfangen: {e}")))?;

    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| CloudError::Network(e.to_string()))?;

    let redirect_path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| CloudError::Auth("ungueltige Redirect-Anfrage".to_string()))?;
    let full_url = format!("http://127.0.0.1{redirect_path}");
    let url = oauth2::url::Url::parse(&full_url).map_err(|e| CloudError::Auth(e.to_string()))?;

    let code = url
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| CloudError::Auth("kein 'code'-Parameter im Redirect".to_string()))?;
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| CsrfToken::new(value.into_owned()))
        .ok_or_else(|| CloudError::Auth("kein 'state'-Parameter im Redirect".to_string()))?;

    let response_body =
        "<html><body>Verbindung hergestellt. Dieses Fenster kann geschlossen werden.</body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    stream.write_all(response.as_bytes()).ok();

    Ok((code, state))
}
```

- [ ] **Step 2: `src-tauri/src/cloud/mod.rs` erweitern**

Aktueller Inhalt:
```rust
pub mod config;
pub mod provider;
pub mod tokens;
```

Neuer Inhalt:
```rust
pub mod config;
pub mod oauth;
pub mod provider;
pub mod tokens;
```

- [ ] **Step 3: Bauen prüfen**

Run: `cd src-tauri && cargo build`
Expected: PASS. Kein Testlauf hier — dieses Modul hat keine automatisierten Tests (siehe Begründung oben).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/cloud/oauth.rs src-tauri/src/cloud/mod.rs
git commit -m "$(cat <<'EOF'
feat: PKCE-Loopback-OAuth2-Flow fuer Google

std::net::TcpListener auf freiem Port, System-Browser via tauri-
plugin-opener, PKCE ueber das oauth2-Crate. Nicht automatisiert
testbar (echter Google-Login noetig) - Verifikation manuell in
Task 10.
EOF
)"
```

---

## Task 7: Google-Account-E-Mail abrufen

**Files:**
- Create: `src-tauri/src/cloud/gdrive.rs`
- Modify: `src-tauri/src/cloud/mod.rs`

**Interfaces:**
- Consumes: `CloudError`, `CloudResult` aus `provider.rs` (Task 3)
- Produces: `pub async fn fetch_google_account_email(access_token: &str) -> CloudResult<String>`

Ruft Googles OAuth2-Userinfo-Endpunkt auf, um die E-Mail-Adresse des verbundenen Kontos zu ermitteln (wird als `account_label` in `cloud_accounts` gespeichert, Task 8). Die eigentliche `StorageProvider`-Implementierung gegen die Drive-Datei-API folgt in Plan 2 — hier nur dieser eine, für den Connect-Flow nötige Aufruf.

- [ ] **Step 1: `src-tauri/src/cloud/gdrive.rs` schreiben**

```rust
use crate::cloud::provider::{CloudError, CloudResult};

const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";

pub async fn fetch_google_account_email(access_token: &str) -> CloudResult<String> {
    fetch_google_account_email_from(USERINFO_URL, access_token).await
}

async fn fetch_google_account_email_from(base_url: &str, access_token: &str) -> CloudResult<String> {
    let client = reqwest::Client::new();
    let response = client
        .get(base_url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| CloudError::Network(e.to_string()))?;

    if !response.status().is_success() {
        return Err(CloudError::Auth(format!(
            "Google-Userinfo-Anfrage fehlgeschlagen: HTTP {}",
            response.status()
        )));
    }

    #[derive(serde::Deserialize)]
    struct UserInfo {
        email: String,
    }

    let info: UserInfo = response
        .json()
        .await
        .map_err(|e| CloudError::Auth(format!("Antwort konnte nicht gelesen werden: {e}")))?;
    Ok(info.email)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Minimaler synchroner Mock-HTTP-Server, der genau eine Anfrage
    /// beantwortet - kein neues Test-Crate noetig, nur std::net.
    fn spawn_mock_userinfo_server(body: &'static str, status_line: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let port = listener.local_addr().expect("local_addr").port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "{status_line}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    #[tokio::test]
    async fn parses_email_from_successful_response() {
        let url = spawn_mock_userinfo_server(r#"{"email": "user@example.com"}"#, "HTTP/1.1 200 OK");
        let email = fetch_google_account_email_from(&url, "fake-token").await.expect("fetch");
        assert_eq!(email, "user@example.com");
    }

    #[tokio::test]
    async fn returns_auth_error_for_non_success_status() {
        let url = spawn_mock_userinfo_server("", "HTTP/1.1 401 Unauthorized");
        let result = fetch_google_account_email_from(&url, "expired-token").await;
        assert!(matches!(result, Err(CloudError::Auth(_))));
    }
}
```

- [ ] **Step 2: `src-tauri/src/cloud/mod.rs` erweitern**

Aktueller Inhalt:
```rust
pub mod config;
pub mod oauth;
pub mod provider;
pub mod tokens;
```

Neuer Inhalt:
```rust
pub mod config;
pub mod gdrive;
pub mod oauth;
pub mod provider;
pub mod tokens;
```

- [ ] **Step 3: Testen**

Run: `cd src-tauri && cargo test cloud::gdrive::`
Expected: PASS, 2 neue Tests, kein echter Netzwerkzugriff (lokaler Mock-Server).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/cloud/gdrive.rs src-tauri/src/cloud/mod.rs
git commit -m "$(cat <<'EOF'
feat: Google-Account-E-Mail per Userinfo-Endpunkt abrufen

Liefert das account_label fuer cloud_accounts. Getestet gegen einen
lokalen Mock-HTTP-Server (std::net, kein Mocking-Crate noetig), kein
echter Google-Netzwerkzugriff in Tests.
EOF
)"
```

---

## Task 8: Tauri-Commands (`connect_google_drive`, `disconnect_cloud_account`, `list_cloud_accounts`)

**Files:**
- Create: `src-tauri/src/cloud/commands.rs`
- Modify: `src-tauri/src/cloud/mod.rs`
- Modify: `src-tauri/src/commands.rs` (nur Sichtbarkeit von `lock_db` ändern)
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `AppState` aus `commands.rs`; `db::{upsert_cloud_account, list_cloud_accounts, set_cloud_account_status}` (Task 2); `cloud::config::{load_cloud_config, default_config_path}` (Task 5); `cloud::oauth::run_google_oauth_flow` (Task 6); `cloud::gdrive::fetch_google_account_email` (Task 7); `cloud::tokens::{KeyringTokenStore, StoredTokens, TokenStore}` (Task 4)
- Produces:
  - `pub struct CloudAccountDto { id: String, name: String, status: String }` (camelCase serialisiert)
  - `#[tauri::command] pub async fn connect_google_drive(app, state) -> Result<CloudAccountDto, String>`
  - `#[tauri::command] pub fn disconnect_cloud_account(state, provider: String) -> Result<(), String>`
  - `#[tauri::command] pub fn list_cloud_accounts(state) -> Result<Vec<CloudAccountDto>, String>`

Kein automatisierter Test für `connect_google_drive` (ruft den nicht testbaren OAuth-Flow auf, Task 6) — `disconnect_cloud_account`/`list_cloud_accounts` sind reine DB-Operationen und werden bereits über die Repository-Tests in Task 2 abgedeckt; ein weiterer Test auf Command-Ebene würde nur dieselbe Logik duplizieren.

- [ ] **Step 1: `lock_db` in `src-tauri/src/commands.rs` sichtbar für das `cloud`-Modul machen**

Aktueller Inhalt:
```rust
fn lock_db<'a>(state: &'a State<AppState>) -> CmdResult<std::sync::MutexGuard<'a, Connection>> {
    state.db.lock().map_err(|_| "database lock poisoned".to_string())
}
```

Neuer Inhalt:
```rust
pub(crate) fn lock_db<'a>(state: &'a State<AppState>) -> CmdResult<std::sync::MutexGuard<'a, Connection>> {
    state.db.lock().map_err(|_| "database lock poisoned".to_string())
}
```

- [ ] **Step 2: `src-tauri/src/cloud/commands.rs` schreiben**

```rust
use tauri::State;

use crate::cloud::config::{default_config_path, load_cloud_config};
use crate::cloud::gdrive::fetch_google_account_email;
use crate::cloud::oauth::run_google_oauth_flow;
use crate::cloud::tokens::{KeyringTokenStore, StoredTokens, TokenStore};
use crate::commands::{lock_db, AppState};
use crate::db;

type CmdResult<T> = Result<T, String>;

const GOOGLE_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/drive.readonly",
    "https://www.googleapis.com/auth/drive.file",
    "https://www.googleapis.com/auth/userinfo.email",
];

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudAccountDto {
    pub id: String,
    pub name: String,
    pub status: String,
}

#[tauri::command]
pub async fn connect_google_drive(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<CloudAccountDto> {
    let config = load_cloud_config(&default_config_path()).map_err(|e| e.to_string())?;

    let tokens = run_google_oauth_flow(
        &app,
        &config.google_client_id,
        &config.google_client_secret,
        GOOGLE_SCOPES,
    )
    .await
    .map_err(|e| e.to_string())?;

    let email = fetch_google_account_email(&tokens.access_token)
        .await
        .map_err(|e| e.to_string())?;

    let token_store = KeyringTokenStore;
    token_store
        .save(
            &format!("gdrive:{email}"),
            &StoredTokens {
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
            },
        )
        .map_err(|e| e.to_string())?;

    let connected_at = chrono::Utc::now().to_rfc3339();
    {
        let conn = lock_db(&state)?;
        db::upsert_cloud_account(&conn, "gdrive", &email, &connected_at).map_err(|e| e.to_string())?;
    }

    Ok(CloudAccountDto {
        id: "gdrive".to_string(),
        name: email,
        status: "connected".to_string(),
    })
}

#[tauri::command]
pub fn disconnect_cloud_account(state: State<AppState>, provider: String) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    let accounts = db::list_cloud_accounts(&conn).map_err(|e| e.to_string())?;
    let account = accounts
        .into_iter()
        .find(|a| a.provider == provider)
        .ok_or_else(|| "Konto nicht gefunden".to_string())?;

    let token_store = KeyringTokenStore;
    token_store
        .delete(&format!("{provider}:{}", account.account_label))
        .map_err(|e| e.to_string())?;

    db::set_cloud_account_status(&conn, &provider, "disconnected").map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_cloud_accounts(state: State<AppState>) -> CmdResult<Vec<CloudAccountDto>> {
    let conn = lock_db(&state)?;
    let accounts = db::list_cloud_accounts(&conn).map_err(|e| e.to_string())?;
    Ok(accounts
        .into_iter()
        .map(|a| CloudAccountDto {
            id: a.provider,
            name: a.account_label,
            status: a.status,
        })
        .collect())
}
```

- [ ] **Step 3: `src-tauri/src/cloud/mod.rs` erweitern**

Aktueller Inhalt:
```rust
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
pub mod tokens;
```

- [ ] **Step 4: Commands in `src-tauri/src/lib.rs` registrieren**

Aktueller Inhalt:
```rust
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::list_files,
            commands::list_folders,
            commands::list_tag_counts,
            commands::add_tag,
            commands::remove_tag,
            commands::delete_file,
            commands::import_files,
            commands::import_folder,
            commands::import_dropped,
            commands::get_model_geometry,
        ])
```

Neuer Inhalt:
```rust
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::list_files,
            commands::list_folders,
            commands::list_tag_counts,
            commands::add_tag,
            commands::remove_tag,
            commands::delete_file,
            commands::import_files,
            commands::import_folder,
            commands::import_dropped,
            commands::get_model_geometry,
            cloud::commands::connect_google_drive,
            cloud::commands::disconnect_cloud_account,
            cloud::commands::list_cloud_accounts,
        ])
```

- [ ] **Step 5: Bauen und bestehende Tests prüfen**

Run: `cd src-tauri && cargo build && cargo test`
Expected: Build PASS, alle bisherigen Tests weiterhin grün (keine neuen Tests in diesem Task, siehe Begründung oben).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/cloud/commands.rs src-tauri/src/cloud/mod.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: Tauri-Commands fuer Google-Drive-Konto verbinden/trennen/auflisten

connect_google_drive orchestriert OAuth-Flow, E-Mail-Abruf, Token-
Ablage und cloud_accounts-Eintrag. lock_db aus commands.rs fuer das
cloud-Modul freigegeben statt Logik zu duplizieren.
EOF
)"
```

---

## Task 9: Frontend — echte Cloud-Konten statt Beispieldaten

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/components/Sidebar.tsx`

**Interfaces:**
- Consumes: Tauri-Commands `list_cloud_accounts`, `connect_google_drive`, `disconnect_cloud_account` (Task 8)
- Produces: keine neuen Exporte — reine Verdrahtung. Keine neuen i18n-Keys nötig (wiederverwendet die bestehenden `cloudConnected`/`cloudError`/`cloudDisconnected`-Keys).

- [ ] **Step 1: `src/components/Sidebar.tsx` — Trennen-Aktion ergänzen**

Aktueller Inhalt (Props-Interface, Zeilen 1-15):
```tsx
import type { Folder, TagCount, CloudAccount } from '../types';
import { useT } from '../i18n/LanguageContext';

interface Props {
  query: string;
  onQueryChange: (q: string) => void;
  folders: Folder[];
  activeFolderId: string;
  onFolderSelect: (id: string) => void;
  tags: TagCount[];
  activeTag: string | null;
  onTagSelect: (label: string | null) => void;
  clouds: CloudAccount[];
  onAddCloud: () => void;
}
```

Neuer Inhalt:
```tsx
import type { Folder, TagCount, CloudAccount } from '../types';
import { useT } from '../i18n/LanguageContext';

interface Props {
  query: string;
  onQueryChange: (q: string) => void;
  folders: Folder[];
  activeFolderId: string;
  onFolderSelect: (id: string) => void;
  tags: TagCount[];
  activeTag: string | null;
  onTagSelect: (label: string | null) => void;
  clouds: CloudAccount[];
  onAddCloud: () => void;
  onDisconnectCloud: (id: string) => void;
}
```

Funktionssignatur (Zeilen 24-35):
```tsx
export function Sidebar({
  query,
  onQueryChange,
  folders,
  activeFolderId,
  onFolderSelect,
  tags,
  activeTag,
  onTagSelect,
  clouds,
  onAddCloud,
}: Props) {
```

Neuer Inhalt:
```tsx
export function Sidebar({
  query,
  onQueryChange,
  folders,
  activeFolderId,
  onFolderSelect,
  tags,
  activeTag,
  onTagSelect,
  clouds,
  onAddCloud,
  onDisconnectCloud,
}: Props) {
```

Status-Badge-Span innerhalb der `clouds.map`-Schleife:
```tsx
              <span
                className={`font-mono-ui text-[10px] ${
                  c.status === 'connected' ? 'text-[var(--ink-3)]' : 'text-[var(--accent)]'
                }`}
              >
                {c.status === 'connected'
                  ? t('cloudConnected')
                  : c.status === 'error'
                  ? t('cloudError')
                  : t('cloudDisconnected')}
              </span>
```

Neuer Inhalt:
```tsx
              <span
                onClick={() => c.status !== 'disconnected' && onDisconnectCloud(c.id)}
                className={`font-mono-ui text-[10px] ${
                  c.status === 'connected'
                    ? 'text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]'
                    : c.status === 'error'
                    ? 'text-[var(--accent)] cursor-pointer hover:opacity-70'
                    : 'text-[var(--accent)]'
                }`}
              >
                {c.status === 'connected'
                  ? t('cloudConnected')
                  : c.status === 'error'
                  ? t('cloudError')
                  : t('cloudDisconnected')}
              </span>
```

- [ ] **Step 2: `src/App.tsx` — hartcodierte `CLOUDS` durch echte Backend-Daten ersetzen**

Aktueller Inhalt (Imports + `CLOUDS`-Konstante, Zeilen 1-19):
```tsx
import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { Header } from './components/Header';
import { Sidebar } from './components/Sidebar';
import { ModelGrid } from './components/ModelGrid';
import { ModelList } from './components/ModelList';
import { DetailPanel } from './components/DetailPanel';
import { ContextMenu } from './components/ContextMenu';
import { useTheme } from './hooks/useTheme';
import type { ModelFile, Folder, TagCount, CloudAccount, ViewMode, SortKey } from './types';

// Cloud-Anbindung ist noch nicht implementiert (spätere Phase) – Beispieldaten bleiben bis dahin.
const CLOUDS: CloudAccount[] = [
  { id: 'gdrive', abbr: 'GD', name: 'Google Drive', status: 'connected', usedPercent: 38, quotaLabel: '5,7/15 GB' },
  { id: 'onedrive', abbr: 'OD', name: 'OneDrive', status: 'connected', usedPercent: 62, quotaLabel: '3,1/5 GB' },
  { id: 'dropbox', abbr: 'DB', name: 'Dropbox', status: 'disconnected', usedPercent: 0, quotaLabel: '—' },
  { id: 'proton', abbr: 'PD', name: 'Proton Drive', status: 'connected', usedPercent: 15, quotaLabel: '0,8/5 GB' },
];
```

Neuer Inhalt:
```tsx
import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { Header } from './components/Header';
import { Sidebar } from './components/Sidebar';
import { ModelGrid } from './components/ModelGrid';
import { ModelList } from './components/ModelList';
import { DetailPanel } from './components/DetailPanel';
import { ContextMenu } from './components/ContextMenu';
import { useTheme } from './hooks/useTheme';
import type { ModelFile, Folder, TagCount, CloudAccount, Origin, ViewMode, SortKey } from './types';

interface CloudAccountDto {
  id: string;
  name: string;
  status: 'connected' | 'error' | 'disconnected';
}

// usedPercent/quotaLabel sind noch nicht Teil dieses Backends (echte
// Speicherplatz-Abfrage folgt bei Bedarf spaeter) - "–" statt erfundener
// Zahlen.
const toCloudAccount = (dto: CloudAccountDto): CloudAccount => ({
  id: dto.id as Origin,
  abbr: '',
  name: dto.name,
  status: dto.status,
  usedPercent: 0,
  quotaLabel: '—',
});
```

Im Komponentenkörper: State-Deklarationen (nach `const [tags, setTags] = useState<TagCount[]>([]);`):

Aktueller Inhalt:
```tsx
  const [tags, setTags] = useState<TagCount[]>([]);
  const [contextMenu, setContextMenu] = useState<{ modelId: string; x: number; y: number } | null>(null);

  const refreshFolders = () => invoke<Folder[]>('list_folders').then(setFolders);
  const refreshTags = () => invoke<TagCount[]>('list_tag_counts').then(setTags);
```

Neuer Inhalt:
```tsx
  const [tags, setTags] = useState<TagCount[]>([]);
  const [clouds, setClouds] = useState<CloudAccount[]>([]);
  const [connectingCloud, setConnectingCloud] = useState(false);
  const [contextMenu, setContextMenu] = useState<{ modelId: string; x: number; y: number } | null>(null);

  const refreshFolders = () => invoke<Folder[]>('list_folders').then(setFolders);
  const refreshTags = () => invoke<TagCount[]>('list_tag_counts').then(setTags);
  const refreshClouds = () =>
    invoke<CloudAccountDto[]>('list_cloud_accounts').then((accounts) => setClouds(accounts.map(toCloudAccount)));
```

Initialer `useEffect`:

Aktueller Inhalt:
```tsx
  useEffect(() => {
    invoke<ModelFile[]>('list_files').then((files) => {
      setModels(files);
      setSelectedId((prev) => prev ?? files[0]?.id ?? null);
    });
    refreshFolders();
    refreshTags();
  }, []);
```

Neuer Inhalt:
```tsx
  useEffect(() => {
    invoke<ModelFile[]>('list_files').then((files) => {
      setModels(files);
      setSelectedId((prev) => prev ?? files[0]?.id ?? null);
    });
    refreshFolders();
    refreshTags();
    refreshClouds();
  }, []);
```

`Sidebar`-Aufruf im JSX:

Aktueller Inhalt:
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
          clouds={CLOUDS}
          onAddCloud={() => {
            // OAuth2 Flow für weiteren Cloud-Anbieter starten
          }}
        />
```

Neuer Inhalt:
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
          clouds={clouds}
          onAddCloud={() => {
            if (connectingCloud) return;
            setConnectingCloud(true);
            invoke('connect_google_drive')
              .then(refreshClouds)
              .catch((e) => console.error('[cloud] Google Drive verbinden fehlgeschlagen:', e))
              .finally(() => setConnectingCloud(false));
          }}
          onDisconnectCloud={(id) => {
            invoke('disconnect_cloud_account', { provider: id })
              .then(refreshClouds)
              .catch((e) => console.error('[cloud] Trennen fehlgeschlagen:', e));
          }}
        />
```

- [ ] **Step 3: Kompilieren prüfen**

Run: `npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 4: Manuell im Browser prüfen (Dev-Server)**

Run: `npm run tauri dev` kurz starten. Erwartet: Sidebar zeigt "Cloud-Konten" leer (keine `cloud.config.json` vorhanden → `list_cloud_accounts` liefert `[]`, kein Absturz), Klick auf "+" löst `connect_google_drive` aus, das ohne `cloud.config.json` einen kontrollierten Fehler wirft, der in der Konsole geloggt wird (kein Crash). Falls dieses Sandbox-Environment keinen funktionierenden GUI-Pfad hat: alternativ per Headless-Chrome/CDP gegen `npm run dev` prüfen (siehe Vorgehen aus der finalen i18n-Review), oder zumindest bestätigen, dass `npm run tauri dev` sauber ohne Panics startet. Dev-Server danach beenden.

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx src/components/Sidebar.tsx
git commit -m "$(cat <<'EOF'
feat: Cloud-Konten in der Sidebar an echtes Backend angebunden

CLOUDS-Beispieldaten entfernt, list_cloud_accounts/connect_google_drive/
disconnect_cloud_account verdrahtet. usedPercent/quotaLabel vorerst "–"
statt erfundener Zahlen (echte Quota-Abfrage folgt bei Bedarf spaeter).
EOF
)"
```

---

## Task 10: Abschlussverifikation

**Files:** keine (nur Build/Test-Ausführung und Hinweis zum manuellen Live-Test)

**Interfaces:** keine

- [ ] **Step 1: Backend vollständig bauen und testen**

Run: `cd src-tauri && cargo build && cargo test`
Expected: Build PASS, alle Tests PASS (bestehende 25 plus die in diesem Plan neu hinzugekommenen aus Task 2/3/4/5/7).

- [ ] **Step 2: Frontend vollständig bauen**

Run: `npm run build`
Expected: PASS (`tsc && vite build`).

- [ ] **Step 3: Git-Log prüfen**

Run: `git log --oneline -12` und `git status --short`
Expected: Alle 9 Feature-Commits dieses Plans sichtbar (Task 1-9, in der richtigen Reihenfolge), Arbeitsverzeichnis sauber.

- [ ] **Step 4: Hinweis zum manuellen Live-Test dokumentieren**

Dieser Task kann NICHT automatisiert verifizieren, dass der komplette OAuth-Flow gegen ein echtes Google-Konto funktioniert (siehe Spec Abschnitt 8 / Global Constraints). Sobald der Nutzer eine echte `cloud.config.json` (Client-ID + Secret aus der Google Cloud Console) bereitgestellt hat, sollte er einmal manuell:
1. `npm run tauri dev` starten
2. In den Einstellungen/Sidebar auf "Google Drive verbinden" (das "+" bei Cloud-Konten) klicken
3. Im sich öffnenden Browser-Tab mit einem echten Google-Konto anmelden und Zugriff erlauben
4. Prüfen, dass die Sidebar danach die verbundene E-Mail-Adresse mit Status "verbunden" zeigt
5. Auf den Status-Text klicken, um zu trennen, und prüfen, dass der Status auf "getrennt" wechselt

- [ ] **Step 5: Abschluss-Zusammenfassung an den User**

Kurze Zusammenfassung: Google-Drive-Verbinden/Trennen ist vollständig implementiert (echter PKCE-OAuth2-Loopback-Flow, Token-Ablage im OS-Schlüsselbund, `cloud_accounts`-Tabelle, `StorageProvider`-Trait als Fundament), Build und Tests grün, echter Live-Test mit Google-Konto steht noch aus (Nutzeraufgabe). Datei-Browsing/Import/Upload aus Google Drive ist bewusst nicht Teil dieses Plans — das ist die nächste, separate Spec/Plan-Runde auf dem hier gebauten Fundament.

---
