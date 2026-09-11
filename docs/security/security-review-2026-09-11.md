# Security Review — 3mf Katalog Manager

**Datum:** 2026-09-11
**Geprüfter Stand:** Commit `34eacb5` (master, 2026-09-10 18:15)
**Methode:** Manuelle Code-Prüfung (Rust-Backend `src-tauri/src/`, TS/React-Frontend `src/`) durch
Lesen/Grep der IPC-Commands, DB-Layer, Cloud/OAuth-Flow, Tauri-Konfiguration, plus `cargo audit`
(RustSec-Advisory-DB) und `npm audit`. Kein automatisierter Multi-Agent-Scan (Workflow-Tool in
dieser Session nicht verfügbar) — dafür jede Aussage unten am tatsächlichen Code verifiziert,
keine geratenen Befunde.

Kontrollzuordnung sinngemäß nach **ISO/IEC 27001:2022 Anhang A** bzw. **ISO/IEC 27002:2022** —
als Orientierung, nicht als zertifizierte Konformitätsaussage (dafür wäre ein vollständiges ISMS
mit Geltungsbereich, Risikobewertung etc. nötig, nicht nur ein Code-Review).

---

## Zusammenfassung

| # | Befund | Schweregrad | Kontrolle (sinngemäß) |
|---|--------|-------------|------------------------|
| 1 | `quick-xml` 0.36.2 — zwei DoS-Schwachstellen (RUSTSEC-2026-0194/0195) | **Hoch** — ✅ behoben 2026-09-11 | A.8.8 Technisches Schwachstellenmanagement |
| 2 | Content-Security-Policy deaktiviert (`"csp": null`) | Mittel | A.8.26 Anwendungssicherheitsanforderungen |
| 3 | Ungeprüftes URL-Schema bei "Quell-URL" (`source_url`) | Niedrig | A.8.28 Sichere Programmierung |
| 4 | Lokale Build-Pfade/Benutzername im Release-Binary sichtbar | Niedrig/Info | A.8.9 Konfigurationsmanagement |
| 5 | OAuth-Client-Secret architektonisch nicht vertraulich (Desktop-Client) | Info (bereits bekannt) | A.8.24 Kryptografie / A.5.23 Cloud-Dienste |
| 6 | Unmaintained-Transitiv-Dependencies + eine "unsound"-Meldung (glib) | Info | A.8.8 Technisches Schwachstellenmanagement |

Kein Fund zu: SQL-Injection, Zip-Slip/Path-Traversal, Command-Injection, TLS-Zertifikatsprüfung,
hartcodierte Secrets im Quellcode, npm-Abhängigkeiten. Details dazu am Ende unter "Geprüft, unauffällig".

---

## 1. `quick-xml` 0.36.2 — Denial-of-Service beim Parsen von 3MF-Dateien (Hoch)

**Betroffen:** `src-tauri/Cargo.toml` (Dependency `quick-xml = "0.36"`), genutzt in
`src-tauri/src/threemf/model_xml.rs:198` und `src-tauri/src/threemf/container.rs:131` beim
Parsen von `3D/3dmodel.model` und `_rels/.rels` aus importierten `.3mf`-Dateien.

**Befund (via `cargo audit`):**
- **RUSTSEC-2026-0194** (CVSS 7.5, Hoch): Quadratische Laufzeit beim Prüfen von Start-Tags auf
  doppelte Attributnamen.
- **RUSTSEC-2026-0195** (CVSS 7.5, Hoch): Unbegrenzte Namespace-Deklarations-Allokation in
  `NsReader` ermöglicht Speicher-Erschöpfung.

**Angriffsszenario:** `.3mf`-Dateien sind das zentrale Import-Format dieser App (`import_files`,
`import_folder`, `import_dropped` in `commands.rs`) — genau die Art Datei, die Nutzer aus dem
Netz herunterladen und in den Katalog importieren. Eine präparierte `.3mf`-Datei mit pathologisch
vielen doppelten Attributen bzw. Namespace-Deklarationen im `3dmodel.model`-XML kann beim Import
den Prozess für Sekunden bis Minuten blockieren oder den verfügbaren Speicher erschöpfen (lokaler
DoS). Kein Remote-Server betroffen, aber die App läuft mit vollen Nutzerrechten — ein blockierter/
abgestürzter Prozess während eines Katalog-Scans ist der praktische Impact.

**Empfehlung:** `quick-xml` auf `>=0.41.0` anheben (`cargo update -p quick-xml --precise 0.41.x`
bzw. in `Cargo.toml` die Versionsangabe entsprechend erhöhen), danach die 3MF-Testsuite laufen
lassen — das ist ein Minor-Versionssprung mit APIs, die in diesem Projekt nur schmal genutzt
werden (`Reader::from_str`), Breaking Changes sind hier unwahrscheinlich, aber zu verifizieren.

**Status: Behoben (2026-09-11).** `Cargo.toml` auf `quick-xml = "0.41"` angehoben, Lock-Datei
aktualisiert (0.36.2 → 0.41.0). Eine Breaking Change traf tatsächlich zu:
`BytesText::unescape()` wurde zu `BytesText::decode()` (anderer Fehlertyp, `EncodingError` statt
`quick_xml::Error`) — Aufrufstelle in `src-tauri/src/threemf/model_xml.rs:215` angepasst
(`.decode().map_err(quick_xml::Error::from)?`, nutzt die bereits vorhandene
`From<quick_xml::Error> for ThreeMfError`-Implementierung). `cargo build`, alle 106
Backend-Tests sowie erneuter `cargo audit`-Lauf bestätigen: beide Advisories verschwunden, keine
Regression.

---

## 2. Content-Security-Policy deaktiviert (Mittel)

**Betroffen:** `src-tauri/tauri.conf.json:23` — `"security": { "csp": null }`.

**Befund:** Tauri setzt bei `csp: null` keinerlei Content-Security-Policy im WebView. Das ist die
letzte Verteidigungslinie gegen die Ausführung von injiziertem Script im Frontend (z. B. falls in
Zukunft doch mal unsanitierter externer Content gerendert wird — aktuell habe ich keine konkrete
Injection-Stelle gefunden, siehe Abschnitt "Geprüft, unauffällig", aber CSP ist genau die
Kontrolle, die auch unbekannte/zukünftige Lücken abfedert statt nur bekannte).

**Empfehlung:** Eine möglichst restriktive CSP setzen, z. B. als Startpunkt:
```json
"csp": "default-src 'self'; img-src 'self' data: asset: https://asset.localhost; connect-src 'self' https://www.googleapis.com https://accounts.google.com"
```
und dann gegen die tatsächlich genutzten Quellen (Google-Drive-API, eingebettete Base64-Bilder,
Tauri-Asset-Protokoll für Thumbnails) schrittweise verfeinern. Tauri dokumentiert das unter
"Content Security Policy" in den App-Security-Guides.

---

## 3. Ungeprüftes URL-Schema bei "Quell-URL" (Niedrig)

**Betroffen:** `src-tauri/src/commands.rs:493-498` (`set_source_url`, keine Validierung),
`src/components/DetailPanel.tsx:248-255` (Rendering als `<a href={model.sourceUrl} target="_blank" rel="noreferrer">`).

**Befund:** Das Feld wird ausschließlich manuell vom Nutzer selbst über die UI gesetzt (keine
automatische Befüllung aus importierten/Cloud-Daten — das habe ich explizit geprüft), das senkt
den Schweregrad deutlich. Es gibt aber weder Backend- noch Frontend-seitig eine Prüfung, dass der
Wert tatsächlich ein `http(s)`-Link ist. In Kombination mit Befund 2 (kein CSP) ist unklar, ob ein
Klick auf einen `file://`- oder anderen Nicht-http(s)-Link im System-Browser (unkritisch) oder in
einem eingebetteten WebView-Fenster der App landet (dort potenziell Zugriff auf lokale Dateien).

**Empfehlung:** Schema-Whitelist (`http://`/`https://`) beim Speichern in `set_source_url`
serverseitig erzwingen, unabhängig vom aktuell geringen Risiko — wird relevant, sobald es mal
einen Katalog-Import/-Export oder Sharing-Feature gibt, das dieses Feld aus fremden Quellen
befüllt.

---

## 4. Lokale Build-Pfade/Benutzername im Release-Binary (Niedrig/Info)

**Befund:** `strings` auf `target/release/mf-katalog-manager` zeigt eingebettete absolute Pfade
wie `/home/thebexxs/.cargo/registry/...` und `/home/thebexxs/...` (Standard-Rust-Verhalten:
Quelldatei-Pfade werden für Panic-Meldungen eingebettet, kein App-spezifischer Fehler). Das wurde
konkret relevant, weil heute in dieser Session ein AppImage-Build zum externen Testen kopiert
wurde (`~/Schreibtisch/3mf-Katalog-Manager_v0.1.0_test.AppImage`) — der Empfänger könnte daraus
den lokalen Linux-Benutzernamen und die Verzeichnisstruktur des Entwicklungsrechners ablesen.

**Empfehlung:** Für Release-Builds, die das Haus verlassen: `RUSTFLAGS="--remap-path-prefix=$HOME=~"`
setzen oder in `Cargo.toml` unter `[profile.release]` `trim-paths = "all"` (stabil seit Rust 1.75)
ergänzen.

---

## 5. OAuth-Client-Secret architektonisch nicht vertraulich (Info, bereits bekannt)

**Betroffen:** `src-tauri/src/cloud/oauth.rs`, `src-tauri/src/cloud/config.rs`.

**Befund:** Der Google-OAuth-Flow ist vorbildlich implementiert — PKCE (`PkceCodeChallenge::new_random_sha256`),
CSRF-State-Prüfung (`returned_state.secret() != csrf_state.secret()`), Loopback-Redirect nach
RFC 8252, `redirect::Policy::none()` gegen Redirect-basierte Token-Leaks. Das ist genau der von
Google für Desktop-Apps vorgeschriebene Weg.

Der `google_client_secret` wird dennoch vom `oauth2`-Crate verlangt (`BasicClient::set_client_secret`).
Bei einem als "Desktop-App" registrierten Google-OAuth-Client ist dieser Wert per Definition nicht
geheim haltbar (er landet zwangsläufig in jeder Distribution der App) — das ist eine von Google
selbst so akzeptierte Einschränkung, kein Implementierungsfehler hier. Sicherheitsrelevant bleibt
nur, dass niemand sich darauf verlässt, das Secret sei vertraulich (z. B. für Autorisierungsentscheidungen)
— das ist hier nicht der Fall, die eigentliche Absicherung läuft korrekt über PKCE + redirect-URI.

Aktuell ist das Feature ohnehin nur auf dem Entwicklungsrechner lauffähig (`default_config_path()`
in `config.rs:49` löst zur Kompilierzeit über `CARGO_MANIFEST_DIR` einen absoluten Pfad auf der
Build-Maschine auf) — bereits als bekannte Lücke vor Release erfasst (siehe frühere Projektnotizen
zu diesem Thema). Kein neuer Handlungsbedarf hier, nur zur Vollständigkeit dokumentiert.

**Positiv geprüft:** `cloud.config.json` (die echte Datei mit den Secrets) ist über
`src-tauri/.gitignore:10` korrekt ausgeschlossen und war nie im Git-Verlauf — nur die
`cloud.config.example.json` mit Platzhaltern ist versioniert.

---

## 6. Unmaintained/unsound Transitiv-Dependencies (Info)

**Befund (`cargo audit`):** `proc-macro-error` 1.0.4, `unic-char-property`/`unic-char-range`/
`unic-common`/`unic-ucd-ident`/`unic-ucd-version` 0.9.0 sind als unmaintained markiert; `glib`
0.18.5 hat eine "unsound"-Meldung (RUSTSEC-2024-0429, `VariantStrIter`). Alles transitive
Abhängigkeiten (vermutlich über GTK-Bindings/Tauri-Plugins bzw. eine Unicode-Normalisierungs-Kette),
kein direkter Nutzungspfad in diesem App-Code gefunden, der die konkrete unsound-Stelle triggert.

**Empfehlung:** Kein akuter Fix nötig, aber bei nächster größerer Dependency-Aktualisierung im
Auge behalten, ob die betroffenen Crates durch aktuellere Alternativen ersetzt werden können
(liegt meist an Tauri/GTK-Plugin-Versionen, nicht direkt beeinflussbar).

---

## Geprüft, unauffällig

Damit dieser Bericht nicht nur Probleme auflistet, hier die konkret geprüften Punkte ohne Befund:

- **SQL-Injection:** Alle Queries in `src-tauri/src/db/repository.rs` nutzen parametrisierte
  Statements (`params![...]`, `?1`/`?2`-Platzhalter). Keine String-Konkatenation/`format!()` in
  SQL gefunden. Filter/Suche laufen clientseitig über die vollständige Liste, nicht über
  dynamisch gebaute SQL-WHERE-Klauseln — entsprechend keine Injection-Fläche dafür.
- **Zip-Slip / Path Traversal beim 3MF-Import:** `threemf/container.rs` liest Einträge nur per
  `ZipArchive::by_name()` in den Speicher (`read_to_string`/`read_to_end`), es wird nichts aus dem
  Archiv auf die Festplatte extrahiert — der klassische Zip-Slip-Angriffsweg (Schreiben über
  `../../`-präparierte Zip-Eintragsnamen) existiert hier strukturell nicht.
- **IPC-Command-Eingaben:** Alle `file_id`/`spool_id`/`filter_id`-Parameter werden vor DB-Zugriff
  auf `i64` geparst (`.parse().map_err(...)`) und Löschoperationen (`delete_file`, `delete_files`)
  lösen die tatsächliche Dateipfad-Löschung immer über den in der DB hinterlegten Pfad auf, nie
  über einen roh vom Client übergebenen Pfad — kein Path-Traversal über die IPC-Grenze.
- **`open_in_slicer`:** Nutzt `std::process::Command::new(&slicer_path).arg(&file_path)` (kein
  Shell-Interpreter, keine String-Konkatenation eines Shell-Kommandos) — klassische
  Command-Injection via Metazeichen ist damit nicht möglich.
- **Token-/Credential-Speicherung:** OAuth-Access-/Refresh-Tokens landen über das `keyring`-Crate
  im OS-Schlüsselbund (`src-tauri/src/cloud/tokens.rs`), nicht in der SQLite-DB oder Klartext-Dateien.
- **TLS:** `reqwest` 0.13.5 ist mit `rustls` + `rustls-platform-verifier` verlinkt (kein OpenSSL/
  native-tls), keine Stelle mit `danger_accept_invalid_certs` oder deaktivierter
  Zertifikatsprüfung gefunden.
- **Hartcodierte Secrets:** Keine API-Keys/Passwörter/Tokens im Quellcode gefunden (grep über
  Rust/TS/JSON nach typischen Mustern).
- **Frontend-XSS:** Kein `dangerouslySetInnerHTML`, `eval()` oder `new Function()` im gesamten
  `src/`-Frontend gefunden. React escaped standardmäßig.
- **Abhängigkeits-Scan Frontend:** `npm audit` → 0 gemeldete Schwachstellen.

---

## Priorisierte Empfehlung

1. ~~**Sofort (vor nächstem AppImage-Test-Build an Dritte):** `quick-xml` auf ≥0.41.0 anheben
   (Befund 1) — einzige Hoch-Einstufung, direkt über den Standard-Import-Weg erreichbar.~~
   **Erledigt 2026-09-11**, siehe Status-Vermerk bei Befund 1.
2. **Kurzfristig:** CSP setzen (Befund 2), URL-Schema-Validierung für `source_url` (Befund 3).
3. **Vor externer Verteilung von Builds:** `trim-paths`/`remap-path-prefix` in den Release-Build
   aufnehmen (Befund 4).
4. **Kein akuter Handlungsbedarf:** Befunde 5 und 6 sind dokumentiert, nicht dringend.
