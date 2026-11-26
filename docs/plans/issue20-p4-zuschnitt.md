# Issue #20, Paket P4: Datei umbenennen im Kern (Zuschnitt)

Ziel: Umbenennen einer Datei (`src-tauri/src/commands/files.rs`, Block `rename_file_with_conn*`) samt Helfern aus
`src-tauri/src/commands/rename.rs` (Kollisionsprüfung, Zwischennamen-Weg für reine Groß/Klein-Umbenennung, Rollback der
Begleitdateien) wandert in `crates/katalog-core/src/file_rename.rs`. Tauri-Befehl `rename_file`, `*_with_conn`-Signaturen,
Fehlercodes/-texte, Logzeilen (Target `datei`), Rückroll-Zweige bleiben gleich. Begleitdateien laufen über die P3-Kern-API
`companion_actions::carry`/`Carried`; `case_insensitive` bleibt Parameter.

## Was wandert (Kern) / was bleibt (Desktop)

| Kern (`file_rename`) | Desktop |
|---|---|
| `rename_file` (Namensprüfung, Zeile lesen, Papierkorb, Schutzordner, Kollision, Ablauf, DB-Zeile) | Tauri-Befehl, Sperre, `lock_db`, dünne Hülle `rename_file_with_conn(_fs)` |
| `rename_reversible` (Datei umbenennen, Begleitdateien mitnehmen, Commit, beide Rückrollzweige, Logzeile) | `DesktopDateRenameEffect` (Datumsumbenennung) ruft denselben Ablauf (R20-01) |
| `normalized_lowercase`, `is_case_only`, `target_collides`, `intermediate_name`, `rename_file_on_disk`, `rename_on_disk`, `rename_with`, `rename_via_intermediate` | `rename.rs` behält die alten Funktionsnamen als dünne Hüllen (Ordner-Umbenennen/P6 und die Tests nutzen sie) |

## Trait und Parameter

`FileRenameFs` (Kern, kein `fsops`/`upload_fs`, baut auf Windows/macOS): `symlink_metadata`, `read_dir_names`,
`canonicalize`, `rename`, `move_file` (Hartlink + Kopie, Fallback wie `placement::move_file`). Der Desktop-Adapter
`FileRenameFsAdapter(&dyn Fs)` leitet 1:1 an `Fs` weiter (`move_file` ueber `move_file_with`), damit die Aufruffolge des
`FaultFs` unveraendert bleibt. Die Begleitdateien bekommen zusaetzlich `&dyn CompanionFs` (P3). `case_insensitive: bool` ist
Parameter; der Desktop uebergibt `cfg!(any(windows, target_os = "macos"))`.

## Risiken

- Fehler: der Kern bildet `ImportError`, der Desktop wandelt an der Grenze in `CmdError` (Log-Parameter bleiben, P3-Fix 677fb423).
  Die Datumsumbenennung bekommt eine Fehlerart je Schritt (`Disk`/`Companion`/`Combined`/`Commit`), damit ihre Texte und
  Logzeilen (nur gekennzeichnete Fehler loggen) unveraendert bleiben.
- Zwischenname traegt PID und Zaehler (`.3mf-rename-tmp-<pid>-<n>`): Goldens ersetzen beides durch Platzhalter.
- Windows/macOS: Gross/Klein (APFS/NTFS), Sperren im 2. Schritt des Zwischennamen-Wegs (Rollback, "Daten liegen unter"),
  `target_collides` ohne `MetadataExt` (nicht-Unix-Zweig). Per Fake belegt, echte Laeufe erst in der CI.
- `move_file_with` und `placement::move_file` muessen synchron bleiben (P0a-Vereinbarung).

## Szenarien der Goldens (alter Code zuerst, `golden_rename_tests.rs`)

1. `file_rename_rules`: Erfolg, gleicher Name, jede Namensablehnung (leer, Zeichen, zu lang, `.`/`..`, Punkt/Leerzeichen am Ende,
   Windows-reserviert), unbekannte Id, Datei im Papierkorb, Schutzordner, Kollision mit Datei/Ordner, Quelle fehlt.
2. `file_rename_companions`: Begleitdateien folgen (`Foo_extras` -> `Bar_extras`), Name im Ziel belegt (Rueckroll der Datei),
   Rueckroll der Datei scheitert (kombinierte Meldung), DB-Fehler nach dem Verschieben (Trigger; Rueckroll, auch mit gescheitertem Rueckroll).
3. `file_rename_faults`: `FaultFs`: Sperre (Hartlink+Kopie), Hartlink nicht moeglich (Kopierpfad), Quelle nicht entfernbar,
   Ziel erscheint nach der Pruefung (`AlreadyExists`), Metadatenfehler bei der Pruefung, mit Aufruffolge.
4. `file_rename_case_only`: `FaultFs::case_insensitive` + `case_insensitive=true`: Zwischennamen-Weg (Erfolg mit Begleitordner),
   Fehler im 1. und im 2. Schritt (Sperre; Rueckroll heilt), 2. Schritt und Rueckroll scheitern (Zwischendatei bleibt, Meldung), Begleitdateien
   scheitern/DB scheitert nach Groß/Klein-Umbenennung (zurueck ueber den Zwischennamen).
5. `file_rename_symlinks` (nur Unix, eigene Datei): Symlink umbenennen, Ziel ist Symlink, defekter Symlink, Hartlink-Zwilling.
6. `date_rename_apply`: Datumsumbenennung mit Begleitdateien, Namenskollision in den Begleitdateien, Commit-Fehler, Ziel erscheint nach der Pruefung.

Alle schreiben Rueckgabe/Fehler-JSON, alle `GOLDEN_TABLES` und den Dateibaum. Danach: Umbau, Goldens byte-gleich,
`rename.rs`-Tests unveraendert, neue Kern-Tests mit Fake-`FileRenameFs`.
