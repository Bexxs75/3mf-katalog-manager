# Issue #20, Paket P6: Ordner umbenennen und verschieben im Kern (Zuschnitt)

Ziel: `rename_folder_with_conn` und `move_folder_with_conn` (`src-tauri/src/commands/folders.rs`: Namens-/Schutzprüfung,
Zyklus-Prüfung, `canonicalize`-Vergleich, Umbenennen/Verschieben auf der Platte, Transaktion mit
`db::update_paths_under_folder`, beide Rückrollzweige) wandern nach `crates/katalog-core/src/folder_move.rs`.
Tauri-Befehle, `*_with_conn`-Signaturen, Fehlercodes/-texte, Logzeilen und Rückroll-Zweige bleiben gleich.
Nicht angefasst: `files.rs`, `rename.rs`, `trash.rs`, `companions*.rs`, `file_rename.rs`, `file_move.rs`, `model_trash.rs`.

## Was wandert (Kern) / was bleibt (Desktop)

| Kern (`folder_move`) | Desktop (`folders.rs`) |
|---|---|
| `rename_folder`, `move_folder`, `is_descendant`, `check_managed_location`, `reject_catalog_root_relocation` (Kopien der Desktop-Texte, über `placement::*`) | `#[tauri::command]` `rename_folder`/`move_folder` (Gate, Parsen der IDs, Sperre) |
| Trait `FolderMoveFs: FileRenameFs` (nur `is_dir`, `exists` kommen dazu; `canonicalize`/`rename`/`symlink_metadata`/`read_dir_names` stammen aus P4) | `rename_folder_with_conn(_fs)`, `move_folder_with_conn(_fs)` als dünne Hüllen, Adapter über `rename::FileRenameFsAdapter(&dyn Fs)` |

Kein `fsops`/`upload_fs`, baut auf Windows/macOS. `case_insensitive` gibt es hier nicht als Parameter: die Ordner-Umbenennung
entscheidet wie heute über `file_rename::target_collides`/`rename_with` (Zwischennamen-Weg), der Pfadvergleich über
`placement::comparison_path` (nur Windows klein geschrieben). Das bleibt unverändert; `FaultFs::case_insensitive` simuliert es auf Linux.
Fehler entstehen im Kern als `ImportError` und werden an der Grenze in `CmdError` gewandelt (Logzeile bleibt: Code + Parameter bei
Ablehnungen, ein Fehlertext bei Platten-/DB-Fehlern). Heute hat die DB-Fehlermeldung bei gescheitertem Rückroll keine eigene
Ursprungszeile; nur der kombinierte Text wird einmal geloggt. Das belegt `folder_log_tests.rs` vor und nach dem Umbau.

## Vorbereitung des alten Codes (kein Verhalten)

Der alte Ordnerablauf rief `std::fs`/`RealFs` fest auf. Damit Goldens Fehler in Schritt n über `FaultFs` einspielen können, bekommen
beide Abläufe wie `create_folder_with_conn_fs` eine `_fs`-Fassung (`is_dir`, `canonicalize`, `exists`, `rename` über `&dyn Fs`);
die alten Namen rufen sie mit `RealFs`. Eigener Commit, alle `folders.rs`-Tests unverändert grün.

## R5: Papierkorbpfade der Begleitdateien (am alten Code gemessen)

`update_paths_under_folder` schreibt `companion_files.path` aller Zeilen unter dem Ordner um, auch gelöschter (Ziel einer
Wiederherstellung), und ebenso `files.path` gelöschter Modelle. **`trash_path`** (`companion-<id>-<name>` bzw. Modell-Papierkorbpfad
im App-Papierkorb, außerhalb des Ordners) bleibt **unverändert** (SQL fasst nur `path` an). Folge: Wiederherstellen nach Umbenennen
oder Verschieben legt die Datei am neuen Ort ab. Das ist heutiges Verhalten und gilt (Golden `folder_paths_trashed_companions_r5`,
mit Wiederherstellen nach dem Verschieben). Keine Verhaltensänderung.

## Szenarien der Goldens (alter Code zuerst, `golden_folder_tests.rs`)

1. `folder_rename_rules`: jede Namensablehnung, unbekannte ID, gleicher Name, Basisordner/Vorfahr der Basis, Schutzpfad (neu/enthält),
   Zielname belegt (Ordner/Datei), nur im Katalog belegt (UNIQUE, Rückroll), Quelle fehlt, Erfolg verschachtelt (Modelle, Begleitdateien,
   Papierkorbzeilen), Nicht-ASCII.
2. `folder_rename_faults`: `FaultFs`: rename scheitert, Prüfung scheitert, Ziel erscheint, DB scheitert (Rückroll ok / scheitert auch).
3. `folder_rename_case_only`: Zwischennamen-Weg, Fehler im 1./2. Schritt, Rückroll scheitert, DB-Fehler danach (Rückroll über Zwischennamen).
4. `folder_move_rules`: kein Ziel, unbekannt, in sich selbst/Nachfahr (Ids und echte Orte), Basis, Schutzpfad, schon dort, Name belegt
   (Ordner/Datei), Quelle/Ziel fehlt, Pfad ohne Namen, Erfolg (hin, zurück, Kind).
5. `folder_move_faults`, `folder_move_faults_resolving`: rename/Metadaten/DB/Rückroll scheitern, `canonicalize` scheitert (Prüfung entfällt).
6. `folder_paths_trashed_companions_r5`, `folder_move_symlinks` (nur Unix).
Alle schreiben Rückgabe/Fehler-JSON, Aufruffolge des `FaultFs`, Zustand je Schritt und alle `GOLDEN_TABLES`.

## Risiken

- Windows/macOS nur per CI belegt (`std::fs::rename` über Laufwerke, Groß/Klein, Sperren laufen hier über `FaultFs`).
- Die Prüfung-dann-rename-Lücke (Forgejo #81) bleibt unverändert.
- `check_managed_location` gibt es in `archives.rs` (Desktop, von Abgleich/Ordner genutzt) und künftig im Kern (Kopie des Textes).
- `rename.rs::target_collides/rename_on_disk` verlieren ihren Aufrufer im Produktivcode (nur Tests): minimale `#[cfg(test)]`-Markierung, falls nötig.
