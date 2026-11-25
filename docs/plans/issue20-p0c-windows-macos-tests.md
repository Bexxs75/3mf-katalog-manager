# Issue #20, Paket P0c: Kern- und Befehlstests auf Windows und macOS

Stand: 25.11.2026. Basis `dev/0.19.0` (54082126), Branch `feat/0.19.0-k20-p0c`.

## Ausgangslage

Die Testschritte `cargo test -p katalog-core --no-default-features` und
`cargo test -p mf-katalog-manager --no-default-features commands::` liefen im Job
`desktop-compile` zum ersten Mal auf Windows und macOS (Lauf 45602887314), mit
`continue-on-error`:

| System | Schritt | Ergebnis |
|---|---|---|
| Windows | Kern | 1432 ok, 48 fehlgeschlagen, 9 ausgelassen |
| Windows | Befehle | **kein Test gelaufen**: Testprogramm endet mit `STATUS_ENTRYPOINT_NOT_FOUND` |
| macOS | Kern | 1526 ok, 6 fehlgeschlagen, 9 ausgelassen |
| macOS | Befehle | 851 ok, 3 fehlgeschlagen, 8 ausgelassen |

Klassen: **(a)** der Test nimmt Linux an, **(b)** das Produkt verhält sich falsch.

## Triage Kern (`katalog-core`)

| # | Tests | System | Meldung | Klasse | Ursache (belegt im Code) | Behebung |
|---|---|---|---|---|---|---|
| K1 | `catalog_restore::tests` (18), `catalog_restore::inspection::tests` (7), `companions::tests` (2) | Windows | `backupInvalidDatabase`, `Err("location")` | a | Die Tests speichern Pfade wie `/tmp/x.3mf`. Unter Windows ist ein Pfad ohne Laufwerk nicht absolut; `imported_paths::validate_path` lehnt ihn mit Absicht ab (`cfg!(windows) \|\| …`), `companions::validate_stored_fields` verlangt `is_absolute()`. | Helfer `native_test_path` (`/a/b` wird unter Windows `C:\a\b`); die Tests schreiben ihre Pfade damit. |
| K2 | `catalog_restore::tests::trash_policy_…`, `catalog_issues::tests::roots_probe_only_decides_below_a_root`, `import::sources::tests::unfit_folders_are_refused` | Windows | `is_err()` erwartet | a | `canonicalize` liefert unter Windows `\\?\C:\…`. `Path::join` löst auf so einem Pfad `..` beim Anhängen auf; der Test prüfte also gar keinen Pfad mit `..`. | Helfer `join_unresolved` baut den Pfad als Text. |
| K3 | `disk_report::tests::rows_with_dot_dot_…` | Windows | `(2, 1)` statt `(1, 1)` | a | Wie K2, dazu `/` als Text an einen `\\?\`-Pfad gehängt (dort kein Trenner). Zusätzlich gilt in `\\?\`-Pfaden `.` als eigene Komponente. | Test nimmt die Wurzel in der eingetippten Schreibweise und baut die Zeilen mit `join_unresolved`. Siehe Beobachtung B1. |
| K4 | `backup::tests` (2), `catalog_export::tests` (5), `db::repository::tests::migration_report_…` | Windows | os error 32 (Datei wird verwendet) | a | Der Test löscht seinen Ordner, während er selbst noch eine SQLite-Verbindung oder Datei darin offen hält. Die Produktfunktionen (`backup::create_named`, `catalog_export::snapshot`, `write_zip`) schließen ihre Handles selbst. | Handles vor dem Aufräumen schließen. |
| K5 | `backup::tests::directories_named_like_backups_are_not_touched` | Windows | os error 5 | a | `File::open` auf einen Ordner geht unter Windows nur mit `FILE_FLAG_BACKUP_SEMANTICS`. | Helfer `set_dir_modified` im Test. |
| K6 | `db::instance_lock::tests` (2) | Windows | os error 33 | a | Der Test liest die Sperrdatei, während die Sperre gehalten wird. Unix-Sperren sind beratend, eine Windows-Sperre verwehrt anderen Handles das Lesen. Die Sperre nutzt nur der Web-Server (Linux). | Unter Windows werden Größe und Änderungszeit verglichen, der Text nach der Freigabe gelesen. Siehe Beobachtung B2. |
| K7 | `db::repository::tests::ensure_folder_path_…`, `db::companions::tests::a_folder_rename_…` | Windows | `/tmp/Tabletop\Reaper`, Pfad nicht umgeschrieben | a | Die App speichert Pfade in der Schreibweise des Systems (`update_paths_under_folder` nimmt `MAIN_SEPARATOR`); die Tests erwarteten `/`. | Erwartung und Eingaben in nativer Schreibweise. |
| K8 | `gcode::tests` (3) | Windows | `unwrap()` auf `None` | a | Git checkt die Textvorlagen unter Windows mit CRLF aus; die Tests suchen `"G28\n"`. | `.gitattributes`: Testvorlagen und Golden-Dateien `-text`. |
| K9 | `thingiverse::tests::a_refused_connection_is_offline` | Windows | `Timeout` statt `Offline` | a | Windows meldet eine abgelehnte lokale Verbindung erst nach eigenen Wiederholungen (rund 2 s); der Test gab genau 2 s. | Zeitgrenze im Test 15 s (unter Linux weiter sofort). |
| K10 | `import::import_job_tests::dropped_folder_with_a_protected_subfolder_…` | Windows, macOS | `moveFailed` statt `protected` | **b** | `placement::expand_sensitive_dirs` ergänzte die aufgelöste Schreibweise nur für **vorhandene** Schutzordner. Der geprüfte Pfad wird aber über seinen nächsten vorhandenen Vorfahren aufgelöst (`/private/var/…` auf macOS, `\\?\C:\…` auf Windows, verlinktes Home auf Linux). Ein noch nicht vorhandener Schutzordner passte dann nie, und der Ordner wurde im geschützten Bereich angelegt. | Behoben: auch nicht vorhandene Schutzordner werden über den nächsten vorhandenen Vorfahren aufgelöst. Neuer Test mit Link (läuft auf Linux, schlägt ohne die Änderung fehl). |
| K11 | `printer_link::moonraker::client_tests` (2) | macOS | `Unreachable` statt `Timeout` | **b** | Drei Zeitgrenzen laufen fast gleichzeitig ab. Gewinnt die Gesamtgrenze der Anfrage in reqwest, kommt ein Fehler ohne I/O-Art zurück; `class_in_chain` fand nichts und meldete „nicht erreichbar“. Unter Linux gewinnt fast immer der wartende Aufrufer, deshalb fiel es nicht auf. | Behoben: ein reqwest-Fehler mit `is_timeout()` zählt als Zeitüberschreitung. Neuer Test (schlägt ohne die Änderung auch unter Linux fehl). |
| K12 | `import::import_job_tests::contract_08_heartbeat_…` | macOS | 17 Meldungen, 18 verlangt | a | `sleep(250 ms)` wacht auf dem geteilten macOS-Runner spät auf. Dass der Takt nie aussetzt, prüft die Lückenprüfung (höchstens 500 ms) weiter auf jedem System. | Untergrenze auf macOS 14, sonst unverändert 18. |
| K13 | `import::import_job_tests::non_utf8_import_names_…` | macOS | os error 92 | a | macOS legt keine Datei an, deren Name kein gültiges UTF-8 ist. | Test auf macOS ausgenommen (`cfg`), mit Grund. |
| K14 | `catalog_issues::tests::roots_probe_races::a_root_replaced_by_another_folder_is_unclear` | macOS | `is_err()` erwartet | a | Nur Linux merkt sich Gerät und Inode der Wurzel (`RootsMissingProbe`, dokumentiert schwächerer Ersatzweg auf anderen Systemen). Die Probe nutzt nur der Web-Server. | Test nur auf Linux (`cfg`), mit Grund. |

## Triage Befehle (`mf-katalog-manager`, `commands::`)

| # | Tests | System | Meldung | Klasse | Ursache | Behebung |
|---|---|---|---|---|---|---|
| D0 | alle | Windows | `STATUS_ENTRYPOINT_NOT_FOUND` | a (Aufbau) | `tauri-build` bettet das Anwendungsmanifest (Common Controls 6, vom Dialog-Plugin gebraucht) nur in das App-Programm ein, nicht in das Testprogramm der Bibliothek. | CI-Schritt legt das Manifest als Datei neben das Testprogramm. Der Bau der App bleibt unverändert. |
| D1 | `source_folders::tests` (2) | macOS | `sourceFileNotNew` | a | `JobFixture` nahm den Temp-Ordner in der Schreibweise `/var/…`; der Scan listet unter dem aufgelösten Ordner (`/private/var/…`), und eine Auswahl muss die Pfade genau so nennen. | Vorrichtung löst ihren Ordner auf. |
| D2 | `disk_sync::roadmap_1g_tests::h2_unicode_spellings_…` | macOS | „ext4 keeps the spellings apart“ | a | Der Test stellt die macOS-Lage ausdrücklich mit zwei Dateien nach; APFS behandelt NFC und NFD als eine Datei. | Auf macOS `ignore` mit Grund. Siehe Beobachtung B3. |
| D3 | `backup::tests`, `backup::companion_tests` (20) | Windows | `backupInvalidDatabase` | a | Wie K1 (dieselben Tests noch einmal in `src-tauri`). | `native_test_path`. |
| D4 | `companions::tests` (3), `source_folders::tests::unfit_folders_are_refused` | Windows | „with ..“, `InvalidFilename` | a | Wie K1/K2; dazu eine Quelldatei namens `a:b*c?.PNG`, die es unter Windows nicht geben kann. | `native_test_path`, `join_unresolved`; der Namensteil läuft nur außerhalb von Windows. |
| D5 | `disk_sync` (4), `files::tests` (3) | Windows | `\` statt `/`, „moved“ statt „skipped“ | a | Die Tests speicherten Pfade mit gemischten Trennern (`dir.join("dst/m.3mf")`) und verglichen mit `/`. Gespeicherte Pfade werden als Text verglichen. | Pfade komponentenweise bauen, Erwartung in nativer Schreibweise. |
| D6 | `file_modified_backfill::tests` (2), `files::tests::hash_backfill_…` | Windows | Leser nicht aufgerufen | a | Wie K1: `/tmp/…` gilt unter Windows nicht als lokaler Pfad, der Leser wird übersprungen. | `native_test_path`. |
| D7 | `slicers::tests::registering_two_different_files_…` | Windows | `slicerPathNotExe` | a | Windows nimmt einen Slicer nur mit Endung `.exe`. | Testdatei heißt unter Windows `other-slicer.exe`. |
| D8 | `open_requests::tests::args_yield_…` | Windows | dritter Pfad fehlt | a | Der Testhelfer baute aus `C:\…` keine gültige Datei-URI. | Helfer schreibt `file:///C:/…`. |

| D9 | `backup::companion_tests::catalog_lock_survives_replace_catalog_db` | Windows | os error 33 | a | Wie K6: liest die Sperrdatei, während die Sperre gehalten wird. | Unter Windows Vergleich von Größe und Änderungszeit. |

D3 bis D9 (37 Tests) stammen aus dem ersten Lauf, in dem das Testprogramm unter Windows
überhaupt startete (Lauf 45609856443); siehe Abschnitt „Läufe“.

## Beobachtungen ohne Änderung

- **B1** `disk_report::Root::relative`: Eine Zeile, deren Pfad in `\\?\`-Schreibweise ein
  `.` enthält, zählt unter Windows als „außerhalb“, unter Linux als innerhalb. Die App
  schreibt solche Pfade nicht; nur als Unterschied vermerkt.
- **B2** `db::instance_lock`: Unter Windows kann niemand die Sperrdatei lesen, solange die
  Sperre gehalten wird. Heute ohne Wirkung (nur der Web-Server sperrt). Falls die
  Desktop-App die Sperre einmal übernimmt, braucht die Anzeige des Besitzers einen
  anderen Weg.
- **B3** Bekannter offener Punkt H2: `path_key` gleicht nur Groß/Klein an, nicht die
  Unicode-Schreibweise. Auf macOS (eine Datei, zwei Schreibweisen) kann der Abgleich
  eine vorhandene Datei als neu und die Zeile als fehlend anbieten. Nicht Teil von P0c.
- **B4** Ablehnungstests, die Pfade als SQL-Text enthalten (`'/tmp/x.3mf'`), bleiben
  unverändert. Unter Windows schlagen sie schon am Pfad fehl, prüfen dort also nicht den
  Grund, den ihr Name nennt. Unter Linux und macOS unverändert.
- **B5** `import::sources::expand_sensitive_dirs` und `catalog_restore` (`fs.resolve`)
  erweitern Schutzordner auf dieselbe alte Art wie `placement` vor K10. Dort hat kein
  Test angeschlagen; gleiche Prüfung empfohlen.

## Lokale Nachstellung

Windows-Verhalten lässt sich auf ObiWan näherungsweise prüfen: Testprogramme mit
`x86_64-pc-windows-gnu` und MinGW im Container bauen, unter Wine ausführen. Das traf 42
der 48 Kernfehler; die übrigen sechs (K6, K8, K9) zeigt nur echtes Windows. Wine weicht
bei Groß/Klein-Umbenennung, Erstellzeiten und Prozessnummern ab, das Ergebnis der CI
entscheidet.
