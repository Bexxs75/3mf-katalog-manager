# Security Review — 3mf Katalog Manager

**Datum:** 2026-09-18
**Geprüfter Stand:** Commit `c227976` (master)
**Methode:** Manuelle Code-Prüfung (Rust-Backend `src-tauri/src/`) der IPC-Commands, DB-Layer,
Katalog-Backup-Export/-Import, Ordner-/Datei-Operationen, Slicer-Auto-Erkennung sowie der
GitHub-Actions-CI-Workflows, plus `cargo audit` und `npm audit`. Fokus auf alle Features, die seit
dem letzten Vollaudit ([2026-09-12](../../CHANGELOG.md), GitHub Issue #1) neu dazugekommen sind:
Sammlungen, Druckprotokoll, Materialkosten-Schätzung, Katalog-Backup, echter Ordnerbaum,
Slicer-Auto-Erkennung, macOS/Windows-CI.

Kontrollzuordnung sinngemäß nach **ISO/IEC 27001:2022 Anhang A** bzw. **ISO/IEC 27002:2022** — als
Orientierung, nicht als zertifizierte Konformitätsaussage.

---

## Zusammenfassung

| # | Befund | Schweregrad | Kontrolle (sinngemäß) | Status |
|---|--------|-------------|------------------------|--------|
| 1 | Importierte Katalog-Backups: Ordner-/Dateipfade ungeprüft an `fs::rename`/`fs::create_dir` weitergereicht | **Mittel** | A.8.28 Sichere Programmierung | ✅ behoben 2026-09-18 |
| 2 | Unsichere temporäre Dateien bei Katalog-Export/-Import (`/tmp`, vorhersagbarer Name, Standard-Rechte) | Niedrig | A.8.28 Sichere Programmierung | ✅ behoben 2026-09-18 |

Kein Fund zu: SQL-Injection (weiterhin durchgehend parametrisiert, auch die neue Batch-`IN(...)`-Query),
Zip-Slip beim Backup-Import (nur zwei fest benannte Einträge per `by_name()`, keine pfadbasierte
Extraktion), Command-Injection (Slicer-Start nutzt argv statt Shell-String, bereits seit
2026-09-12 gehärtet), Script-Injection in den CI-Workflows (`workflow_dispatch`-only, kein
Trigger über PR-Titel/Branch-Namen). `cargo audit`/`npm audit` zeigen keine aktiven CVEs (nur
bereits bekannte, niedrig priorisierte Unmaintained-Dependency-Warnungen, siehe
[Review vom 2026-09-11](security-review-2026-09-11.md)).

---

## 1. Importierte Katalog-Backups werden als vertrauenswürdig für Dateisystem-Operationen behandelt (Mittel)

**Betroffen:** `src-tauri/src/commands.rs` — `import_catalog`, `rename_folder_with_conn`,
`move_folder_with_conn`, `move_file_to_folder_with_conn`, `create_folder`.
**CWE:** CWE-829 (Inclusion of Functionality from Untrusted Control Sphere)

`import_catalog` ersetzt die komplette laufende Datenbank aus einem vom Nutzer gewählten ZIP,
nach nur minimaler Validierung (`validate_catalog_db_bytes`: DB muss öffenbar sein und eine
`files`-Tabelle haben). `folders.path`/`files.path` aus dieser potenziell fremden Datenbank wurden
anschließend ungeprüft an `std::fs::rename`/`std::fs::create_dir` weitergereicht — es gab keine
Stelle, die nach dem Import verifizierte, dass diese Pfade außerhalb bekannter sensibler
Systemverzeichnisse liegen.

**Exploit-Szenario:** Ein präpariertes "Katalog-Backup"-ZIP enthält einen Ordner-Eintrag mit
`path` = z.B. `~/.config/autostart`. Verschiebt der Nutzer nach dem Import eine (im Ordnerbaum
unauffällig benannte, da nur Namen statt Pfade angezeigt werden) Datei regulär per UI in diesen
"Ordner", landet sie real dort — mit Persistenz-Wirkung beim nächsten Login.

**Fix:** Neue Prüfung `reject_if_sensitive_path` gegen eine beim Start einmalig berechnete Liste
sensibler Verzeichnisse (Config-/Daten-Verzeichnis, `.ssh`, `.gnupg`, `.password-store`, je nach
Betriebssystem `Library`/Systemwurzeln), angewendet an zwei Stellen:

1. **An der Vertrauensgrenze** (`validate_catalog_db_bytes`): der Import wird komplett abgelehnt,
   wenn irgendein `folders.path` in der importierten DB sensibel ist.
2. **Defense-in-depth** direkt vor jedem `fs::rename`/`fs::create_dir` in `create_folder`,
   `rename_folder_with_conn`, `move_folder_with_conn`, `move_file_to_folder_with_conn` — falls
   sensible Pfade auf einem anderen Weg (z.B. direkte SQLite-Manipulation außerhalb der App) in
   die Datenbank gelangen.

Fünf neue Tests decken beide Ebenen ab (`commands.rs`, Modul `tests`).

---

## 2. Unsichere temporäre Dateien bei Katalog-Export/-Import (Niedrig)

**Betroffen:** `src-tauri/src/commands.rs` — `export_catalog`, `import_catalog`,
`validate_catalog_db_bytes`.
**CWE:** CWE-377 (Insecure Temporary File)

Alle drei Stellen schrieben eine unverschlüsselte Kopie der Katalog-Datenbank nach
`std::env::temp_dir()` mit vorhersagbarem, PID-basiertem Dateinamen, ohne die Rechte einzuschränken
— anders als die eigentliche `catalog.db`, die seit dem Audit vom 2026-09-12 (Finding N8) auf
`0600` gehärtet ist. Auf Mehrbenutzer-Systemen hätte ein anderer lokaler Account die Datei während
des kurzen Zeitfensters mitlesen oder den Pfad vorab als Symlink anlegen können (`fs::write`/
`File::create` folgen einem bestehenden Symlink, statt dessen Existenz abzulehnen).

**Fix:** Neue Helper-Funktion `write_temp_file_exclusive` (nutzt `OpenOptions::create_new` statt
`fs::write`, schlägt fehl statt einem vorhandenen Symlink zu folgen) plus `crate::harden_permissions`
(bereits vorhandene 0600-Härtung aus dem N8-Fix, jetzt `pub(crate)`), angewendet auf alle drei
Temp-Dateien.

---

## Geprüft, unauffällig

- **ZIP-Import:** kein Zip-Slip — nur zwei fest benannte Einträge (`catalog.db`, `settings.json`)
  per `archive.by_name()` gelesen, nie pfadbasiert extrahiert.
- **Druckprotokoll-Fotos:** als Base64 → BLOB in der DB gespeichert, keine dateinamenbasierte
  Schreiboperation.
- **Slicer-Auto-Erkennung:** reine PATH-/Standardpfad-Suche, kein Prozessstart.
- **`open_in_slicer`/`open_in_file_manager`:** bereits seit 2026-09-12 gehärtet (validierte
  ausführbare Datei/echtes Verzeichnis, argv statt Shell-String, AppImage-Env-Stripping).
- **GitHub-Actions-Workflows:** `workflow_dispatch`-only, kein Trigger über PR/Push. Actions sind
  auf Major-Version-Tags statt volle SHAs gepinnt (Supply-Chain-Best-Practice-Lücke), aber ohne
  konkreten Angriffspfad — nur als optionale Härtung vermerkt, kein Finding.
- **CSP, `source_url`-Validierung, Slicer-Pfad-Validierung, Path-Traversal in
  `create_folder`/`rename_folder`:** weiterhin wirksam, keine Regression.

## Bezug zu früheren Reviews

- [Review 2026-09-11](security-review-2026-09-11.md): Finding 4 (Build-Pfad-/Username-Disclosure)
  wurde am 2026-09-18 im Rahmen des Public-Release-Cleanups per vollständigem `git filter-repo`-
  History-Rewrite gelöst (betraf `docs/superpowers/`, nicht die Release-Binaries). Unmaintained
  Dependencies bleiben offen (niedrige Priorität, keine aktiven CVEs).
- [Audit 2026-09-12](../../CHANGELOG.md) (GitHub Issue #1): alle damaligen Findings weiterhin
  wirksam behoben, keine Regression festgestellt.
