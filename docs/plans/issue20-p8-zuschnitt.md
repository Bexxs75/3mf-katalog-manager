# Issue #20, Paket P8: Abgleich (Desktop `disk_sync`) im Kern (Zuschnitt)

Ziel: Scan-Planung und Anwenden des Abgleichs (`src-tauri/src/commands/disk_sync.rs`, rund 1.700 Zeilen ohne Tests) wandern
nach `crates/katalog-core/src/disk_sync.rs`, mit Desktop-Semantik (absolute Pfade, Hash-Umbenennungserkennung, bekannte Kopien,
geleerte Ordner, Groß/Klein über den Parameter `case_insensitive`). Keine Vereinheitlichung mit dem Web-Bericht
(`disk_report`/`disk_apply`), nur dieselben Bausteine aus dem Kern (`db`, `import::files`, `placement`), wo sie heute schon
benutzt werden. Tauri-Befehle, Signaturen, Fehlercodes (`DiskSyncError` `{code, paths}`), Rückgaben und Logzeilen bleiben gleich.
Nicht angefasst: `files.rs`, `trash.rs`, `folders.rs`, `archives.rs`, Web (`disk_report`, `disk_apply`), Frontend.

## Teilschritte

| Schritt | Inhalt | Commit |
|---|---|---|
| Naht | `_fs`-Fassungen im alten Code (`scan_with_fs`, `apply_with_fs`, `KeyResolver::with_fs`, `no_symlinks_fs`, `read_directory_fs`, `hash_for_match_fs` ...), Plattenzugriffe über `&dyn Fs`; alte Namen rufen `RealFs`. `Fs` bekommt `read_dir_entries` (Einträge mit Typ). Keine Verhaltensänderung. | eigener |
| Goldens | `disk_sync/golden_sync_tests.rs` mit dem alten Code aufgenommen | eigener |
| 8a Scan | Typen + Scan-Planung in den Kern, Desktop-Anwenden arbeitet auf den Kern-Typen | eigener |
| 8b Anwenden | `apply`, `apply_rename`, `move_emptied_folders`, `remember_declined_copies` in den Kern | eigener |

## Was wandert (Kern `disk_sync`) / was bleibt (Desktop)

| Kern | Desktop (`commands/disk_sync.rs`) |
|---|---|
| Typen `DiskEntry`, `DiskScan`, `DiskRename`, `DiskDuplicate`, `DiskFolderEntry`, `DiskSelection`, `RenameChoice`, `DiskApplied`, `DiskFailedPath`, `DiskRoot` (serde unverändert, Felder `pub`) | Tauri-Befehle `scan_catalog_disk`, `apply_catalog_disk_sync`, Start-Abgleich (`start_sync.rs`), `known_copies.rs`, `missing_folder_ids` (Ordnerbaum), `connection` |
| `path_key`, `KeyResolver`, `no_symlinks`, `read_directory`, `walk`, `scan`, `skip_known_copies`, `classify_by_content`, `find_missing_folders`, `suppress_case_only_new_files`, `hash_for_match` | eigener `DiskSyncError` (gleiches JSON) mit `From<String>`, `From<CmdError>` (Orphan-Regel) und `From<core::DiskSyncError>` |
| `apply`, `checked_path`, `ensure_sync_folder`, `verify_rename`, `confirm_rename_target`, `apply_rename`, `move_emptied_folders`, `emptied_folder_target`, `remains_at_old_location`, `remove_missing_folders`, `remember_declined_copies` | dünne Hüllen mit den alten Namen (`scan`, `scan_with_reader`, `scan_with_path_mode`, `scan_with_hasher`, `apply`, `apply_with_reader`, `apply_with_path_mode`, `read_directory`, `hash_for_match`, ...), Adapter `DiskScanFsAdapter(&dyn Fs)` und `DesktopDiskSyncEffects` |
| Trait `DiskScanFs` (`canonicalize`, `metadata`, `symlink_metadata`, `read_dir_entries`) + `StdDiskScanFs` | `read_model_with_hash` (Leser bleibt Closure, wie heute) |
| Trait `DiskSyncEffects` (`content_hash`, `check_managed_location`, `may_relocate_folder`, `delete_missing_file`) + `StdDiskSyncEffects` | Effekte rufen die heutigen Desktop-Funktionen (`files::compute_content_hash`, `archives::check_managed_location`, `folders::reject_catalog_root_relocation`, `trash::delete_missing_file_with_conn`) |

Warum ein Effekt-Trait: Diese vier Desktop-Funktionen erzeugen beim Fehlschlag einen Desktop-`CmdError`, und der loggt sich beim
Erzeugen selbst (`error.rs`). Würde der Kern die Kern-Fassungen rufen, fielen diese Logzeilen weg (Kern-`ImportError` loggt nicht).
Über den Effekt-Trait bleiben Text, Stufe und Ursprungsort der Logzeilen gleich; die Goldens schreiben die Logzeilen ohne
`(datei:zeile)` mit. Der Leser für den Import bleibt eine Closure (`FnMut(&Path) -> Result<NewFile, String>` im Kern; der Desktop
gibt `CmdError::message` weiter, wie heute `error.message.chars().take(160)`).

Fehler: Der Abgleich verwirft schon heute jeden Fehlertext (`From<String>` → `diskSyncFailed`, keine Logzeile); es gibt nichts
zusammenzusetzen. `delete_missing_file` behält die Sonderregel `importActive` (Text gleich → Code `importActive`). Der Kern-Fehler ist
klein (`&'static str` + `Vec<String>`), `result_large_err` greift nicht.

Groß/Klein: Alle Kern-Funktionen nehmen `case_insensitive` als Parameter. `CASE_INSENSITIVE_PATHS` (`cfg!`) setzt nur die
Desktop-Hülle ein. Ausnahme wie heute: `move_emptied_folders` ruft `ensure_sync_folder` mit `CASE_INSENSITIVE_PATHS` statt mit
dem Parameter (Auffälligkeit A1, unverändert übernommen, gemeldet).

Plattenzugriffe, die nicht über `DiskScanFs` laufen (wie heute std): Inhalt lesen (Hash, Import-Leser), die Schutzprüfungen
(`resolve_path_for_sensitivity_check` in den Effekten), `missing_folder_ids`, Start-Abgleich `repeated_new_files_within`.
Kein `fsops`/`upload_fs`: Der Kern baut auf Windows/macOS.

## Szenarien der Goldens (alter Code zuerst, `disk_sync/golden_sync_tests.rs`)

Jeder Schritt schreibt Scan- bzw. Anwenden-JSON (oder Fehler-JSON), die Logzeilen (Stufe, Ziel, Text ohne Ort) und am Ende alle
`GOLDEN_TABLES` (inkl. `disk_sync_known_copies`) und den Dateibaum. `FaultFs`-Aufrufe werden sortiert und gezählt mitgeschrieben
(die Reihenfolge von `read_dir` hängt vom Dateisystem ab). Der Import-Leser ist der echte, ohne Vorschaubilder (PNG-Größe hängt
von der Plattform ab). `case_insensitive` wird immer ausdrücklich gesetzt.

1. `disk_sync_plan`: neu, fehlend, umbenannt, Kopie, Papierkorb-Sperre, Begleitordner, ungelesen, Ordner neu/fehlend, Anwenden
   alles, Scan danach leer.
2. `disk_sync_known_copies`: Kopie ablehnen, Scan danach (zählt), Größe ändert sich (vergessen), Original fehlt (wieder angeboten).
3. `disk_sync_emptied_folders`: Ordner umbenannt/verschoben (Zeile folgt), Teilumzug (bleibt), Zielordner im Katalog, Basisordner.
4. `disk_sync_refusals`: veraltete Auswahl, fremder Pfad, Wurzel leer/fehlt, Ziel geändert (Hash), Ziel schon im Katalog,
   Schutzpfad, DB-Fehler beim Einfügen (alles zurück), fehlende entfernen + Ordner entfernen, Basisordner bleibt.
5. `disk_sync_faults`: `FaultFs`: `read_dir` eines Unterordners/der Wurzel scheitert, `metadata` (fehlend?) scheitert,
   `canonicalize` der Wurzel scheitert, Fehler im n-ten Schritt beim Anwenden.
6. `disk_sync_case_insensitive`: `FaultFs::case_insensitive` + `case_insensitive=true`: nur Groß/Klein umbenannt (weder neu noch
   fehlend), Kopie mit anderer Schreibweise merken, bekannte Kopie.
7. `disk_sync_symlinks` (nur Unix): Wurzel über Symlink, Symlink-Datei/-Ordner werden nicht verfolgt; Symlinks vor dem Baum
   entfernt (ihre Größe hängt vom Temp-Pfad ab).

## Risiken

- Windows/macOS nur per CI belegt (Groß/Klein über `FaultFs`, `\\?\`-Pfade aus `canonicalize`).
- `read_dir`-Reihenfolge: Ergebnisse sind sortiert; nur die Aufrufliste wird sortiert geschrieben.
- Die Prüfung-dann-Handeln-Lücken (Ziel ändert sich nach `validate`) bleiben, wie heute.
- Laufzeit großer Kataloge: zusätzlich nur eine dynamische Indirektion je Plattenaufruf.
