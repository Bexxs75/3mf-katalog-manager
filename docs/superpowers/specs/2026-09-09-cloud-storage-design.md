# Cloud-Storage-Integration (Google Drive) — Design

**Datum:** 2026-09-09
**Status:** Freigegeben

## Ziel

Anbindung von Google Drive als zusätzliche Dateiquelle neben dem lokalen Dateisystem: Dateien aus Drive per Dialog durchsuchen und importieren (parsen, Metadaten/Thumbnail extrahieren, katalogisieren), lokale Katalog-Dateien zu Drive hochladen (Spiegelung), Sync-Status pro Datei sichtbar machen, Konto verbinden/trennen über die Seitenleiste.

Dies ist die **erste** von vier im Original-Projektauftrag genannten Cloud-Anbindungen (Google Drive, OneDrive, Dropbox, Proton Drive). Die anderen drei sind bewusst **nicht** Teil dieser Spec — sie folgen später als eigene, kleinere Runden, die nur noch eine weitere `StorageProvider`-Implementierung gegen die hier gebaute Abstraktion hinzufügen.

## Nicht-Ziele

- OneDrive, Dropbox, Proton Drive — spätere, separate Specs.
- Mehrere gleichzeitig verbundene Google-Konten — v1 unterstützt genau ein verbundenes Konto pro Anbieter (passt zum bestehenden `CloudAccount.id: Origin`-Typ im Frontend).
- Hintergrund-Sync/Polling — Änderungserkennung ist On-Demand (beim Öffnen der Detailansicht), kein periodischer Dienst.
- Fester "Sync-Ordner" wie bei klassischen Cloud-Clients — Import läuft über einen expliziten Datei-Browser-Dialog, analog zum bestehenden lokalen Import.
- Automatisches Löschen von Katalog-Einträgen beim Trennen eines Kontos — Einträge bleiben erhalten (siehe Abschnitt 6).
- Live-Test gegen die echte Google-API — kann nicht automatisiert durchgeführt werden (siehe Abschnitt 8), erfordert einen manuellen Durchlauf mit echtem Google-Login.

## Bekannte externe Abhängigkeit

Für echtes OAuth2 wird eine OAuth-Client-ID (Typ "Desktop-App") aus der Google Cloud Console benötigt. Dies erfordert ein Google-Konto und die manuelle Einrichtung eines Cloud-Projekts durch den Nutzer — kann nicht durch Claude Code automatisiert werden. Die Client-ID wird als Konfigurationswert (z. B. `src-tauri/cloud.config.json`, git-ignored, mit Beispieldatei `cloud.config.example.json`) erwartet; ohne sie kompiliert und läuft die App weiterhin, aber "Google Drive verbinden" schlägt mit einer klaren Fehlermeldung fehl.

## Abschnitt 1 — Architektur & Komponenten

Neues Rust-Modul `src-tauri/src/cloud/`:

```
src-tauri/src/cloud/
  mod.rs        // öffentliche API des Moduls
  provider.rs   // StorageProvider-Trait + CloudEntry-Typ (anbieterunabhängig)
  gdrive.rs     // GoogleDriveProvider: StorageProvider-Impl gegen Drive REST API v3
  oauth.rs      // PKCE-Loopback-Redirect-Helfer (anbieterunabhängig, spätere Provider nutzen ihn mit)
  tokens.rs     // Token-Ablage/-Abruf über das keyring-Crate
```

```rust
#[async_trait]
pub trait StorageProvider: Send + Sync {
    async fn list_folder(&self, folder_id: Option<&str>) -> Result<Vec<CloudEntry>>; // None = Wurzel
    async fn download(&self, file_id: &str) -> Result<Vec<u8>>;
    async fn upload(&self, folder_id: Option<&str>, file_name: &str, data: &[u8]) -> Result<CloudEntry>;
    async fn get_metadata(&self, file_id: &str) -> Result<CloudEntry>; // fuer Aenderungserkennung
}

pub struct CloudEntry {
    pub id: String,
    pub name: String,
    pub is_folder: bool,
    pub modified_time: String, // RFC3339
    pub size_bytes: Option<i64>,
}
```

Neue Tauri-Commands (in `src-tauri/src/commands.rs` oder neuem `cloud_commands.rs`):
`connect_google_drive`, `disconnect_cloud_account`, `list_cloud_accounts`, `browse_cloud_folder`, `import_from_cloud`, `upload_to_cloud`, `check_cloud_sync_status`.

**Neue Rust-Abhängigkeiten** (keine davon bisher in `Cargo.toml` vorhanden): `oauth2` (PKCE-Flow), `tiny_http` (kurzlebiger Loopback-Server), `keyring` (Token-Ablage), `reqwest` (Drive-API-Calls), `async-trait` (für den `StorageProvider`-Trait mit `async fn`).

## Abschnitt 2 — OAuth2-Flow

Google blockiert eingebettete WebViews für OAuth (`disallowed_useragent`) und akzeptiert für den "Desktop-App"-Client-Typ keine beliebigen Custom-URI-Schemes als Redirect — daher **Loopback-Redirect** (Googles offiziell dokumentierter und einzig unterstützter Weg für Desktop-Apps):

1. `connect_google_drive` startet einen kurzlebigen `tiny_http`-Server auf `127.0.0.1:<zufälliger freier Port>`.
2. App öffnet den System-Standardbrowser mit der Google-Consent-URL (PKCE, `redirect_uri=http://127.0.0.1:<Port>/callback`, Scope: `drive.readonly` + `drive.file` — kein voller `drive`-Scope, um nicht mehr Rechte anzufordern als nötig).
3. Nutzer meldet sich im echten Browser an (2FA/Passkeys/gespeicherte Logins funktionieren normal).
4. Google leitet zum lokalen Server um, der den Code abfängt, gegen Access-/Refresh-Token tauscht und sich sofort beendet.
5. Tokens werden über `tokens.rs` im OS-Schlüsselbund abgelegt (Service `3mf-katalog-manager`, Account `gdrive:<Google-Konto-E-Mail>`), **nicht** in SQLite.
6. `cloud_accounts`-Tabelle (Abschnitt 3) bekommt einen Eintrag mit Status `connected` und der E-Mail als Anzeigename.

Automatischer Token-Refresh bei `401`-Antworten der Drive-API, transparent für alle Commands. Schlägt der Refresh fehl (Zugriff widerrufen), wechselt der Konto-Status auf `error`.

## Abschnitt 3 — Datenmodell

Neue Tabelle:

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

`UNIQUE (provider)` erzwingt die v1-Einschränkung "ein Konto pro Anbieter". Kein Token-Feld — Tokens leben ausschließlich im Schlüsselbund. Bestehende Felder `files.origin`, `files.cloud_id`, `files.sync_status` (bereits im Schema vorhanden) werden wie ursprünglich vorgesehen genutzt, keine Schemaänderung an `files` nötig.

## Abschnitt 4 — Import-Flow

Neue Komponente `src/components/CloudBrowserDialog.tsx`: erreichbar über das bestehende Import-Dropdown im Header, neuer Menüpunkt "Aus Google Drive importieren" (nur sichtbar/aktiv, wenn ein Drive-Konto verbunden ist). Zeigt die Ordnerstruktur (lazy geladen über `browse_cloud_folder` bei Navigation), Mehrfachauswahl von Dateien/Ordnern über Checkboxen, "Importieren"-Button.

`import_from_cloud` läuft je gewählter Datei wie der bestehende lokale Import: herunterladen (temporärer Cache, siehe Abschnitt 5), mit den vorhandenen 3MF-/STL-Parsern parsen, Metadaten/Thumbnail extrahieren, in SQLite einfügen mit `origin='gdrive'`, `cloud_id=<Drive-Datei-ID>`, `sync_status='synced'`.

## Abschnitt 5 — Lokaler Cache & Änderungserkennung

Heruntergeladene Cloud-Dateien werden in Tauris App-Cache-Verzeichnis zwischengespeichert (plattformkonform über Tauris `path`-API), nicht im Katalog-Datenverzeichnis — sie sind rekonstruierbar und dürfen bei Bedarf gelöscht werden.

**Änderungserkennung ist On-Demand:** Beim Öffnen der Detailansicht einer Cloud-Datei ruft das Frontend `check_cloud_sync_status(file_id)` auf; das Backend vergleicht `get_metadata(cloud_id).modified_time` mit dem beim letzten Sync gespeicherten Wert und setzt `sync_status` auf `outdated`, falls abweichend. Kein Hintergrund-Polling in v1.

## Abschnitt 6 — Trennen eines Kontos

`disconnect_cloud_account`: löscht den Token aus dem Schlüsselbund, setzt `cloud_accounts.status = 'disconnected'` (Zeile bleibt erhalten, damit der Anbieter weiterhin in der Sidebar-Liste als trennbar/wieder-verbindbar sichtbar ist). **Katalog-Einträge bleiben unverändert** — Metadaten und Thumbnail sind bereits lokal in SQLite gespeichert und bleiben sichtbar; künftige Änderungserkennungs-Aufrufe für diese Dateien schlagen mangels Token fehl und werden übersprungen, ohne den zuletzt bekannten `sync_status` zu verändern.

## Abschnitt 7 — Upload-Flow

Neue Aktion in `DetailPanel.tsx`/`ContextMenu.tsx` für Dateien mit `origin='local'`: "Zu Google Drive hochladen" (nur aktiv, wenn ein Drive-Konto verbunden ist). Zielordner wird über denselben `CloudBrowserDialog` im "Zielordner wählen"-Modus bestimmt. `upload_to_cloud` lädt die lokale Datei hoch; bei Erfolg bleibt `origin='local'` (die lokale Kopie bleibt maßgeblich), aber `cloud_id` wird gesetzt und `sync_status` wechselt von `local-only` zu `synced` — die Datei ist danach gespiegelt und nimmt an der Änderungserkennung teil.

## Abschnitt 8 — Fehlerbehandlung & Testbarkeit

**Fehlerbehandlung:** Netzwerkfehler bei Browse/Import/Upload werden als Command-Fehler durchgereicht und im Frontend angezeigt (bestehendes Fehler-Anzeige-Muster, keine neue Infrastruktur). Fehlender/ungültiger Token führt zu Konto-Status `error` statt einem harten Absturz.

**Testbarkeit — wichtige Einschränkung:** `StorageProvider`-Trait und der PKCE-Helfer sind isoliert mit einem Mock-Provider testbar (keine echten Netzwerkaufrufe). Der eigentliche Google-Drive-Livetest (echter OAuth-Login, echte Dateiliste, echter Download/Upload) **kann nicht automatisiert durchgeführt werden** — er erfordert einen echten Google-Account-Login im System-Browser und damit einen manuellen Durchlauf durch den Nutzer nach Bereitstellung der OAuth-Client-ID (siehe "Bekannte externe Abhängigkeit").

## Global Constraints (für die Implementierungsplanung)

- Neue Rust-Abhängigkeiten (`oauth2`, `tiny_http`, `keyring`, `reqwest`, `async-trait`) sind für dieses Feature ausdrücklich erlaubt (anders als beim i18n-Feature, das explizit ohne neue Bibliotheken auskommen sollte).
- Keine Token/Secrets in SQLite oder Git — Tokens ausschließlich über `keyring`, OAuth-Client-ID ausschließlich über eine git-ignored Konfigurationsdatei mit Beispieldatei.
- `files`-Tabellenschema bleibt unverändert; nur die neue `cloud_accounts`-Tabelle kommt hinzu.
- Ein verbundenes Konto pro Anbieter (v1-Einschränkung, technisch über `UNIQUE(provider)` erzwungen).
- Der Live-OAuth-Test gegen echtes Google-Konto ist manuelle Nutzerarbeit, kein automatisierter Verifikationsschritt.
