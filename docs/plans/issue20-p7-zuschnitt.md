# Issue #20, Paket P7: Papierkorbfamilie der Modelle im Kern (Zuschnitt)

Ziel: Löschen (einzeln/Stapel), Wiederherstellen, endgültig Löschen, Leeren und die Startbereinigung
(7 Tage) aus `src-tauri/src/commands/trash.rs` wandern nach `crates/katalog-core/src/model_trash.rs`.
Tauri-Befehle, `*_with_conn`-Signaturen, Fehlercodes/-texte, Logzeilen (Targets `datei`, `trash`, `startup`,
`cleanup`) und das Desktop-Papierkorbformat (`<id>-<name>`, §3.1) bleiben gleich; der Kern-Leser
`fsops/desktop_trash.rs` des Webs wird nicht angefasst. Begleitdateien laufen über die P3-API
(`companion_actions::trash_with_model`, `restore_with_model`, `purge_*`).

## Was wandert (Kern) / was bleibt (Desktop)

| Kern (`model_trash`) | Desktop (`trash.rs`, `folder_trash.rs`) |
|---|---|
| `delete_file`, `delete_missing_file`, `delete_files` (Stapel) | Tauri-Befehle, `gate`, `lock_db`, `list_trash`, Parsen der Id |
| `restore_file` (Modell + Begleitdateien), `restore_model`, `missing_parent_dirs`, `relink_restored_folder` | dünne Hüllen mit alter Signatur (`delete_file_with_conn[_fs]`, `restore_file_with_conn`, ...) |
| `delete_file_permanently`, `empty_trash`, `purge_expired_trash(now)` | `purge_expired_trash_on_startup` (setzt `now = Utc::now()`) |
| Prüfung geschützter Pfade beim Neuanlegen von Ordnern (Kopie von `archives::check_managed_location`, gleiche Texte) | `ModelTrashFsAdapter(&dyn Fs)` |
| | `DesktopModelTrash` (Host der `ModelTrash`-Operation) ruft die Kern-Funktion `delete_file` |

## Trait und Parameter

`ModelTrashFs` (Kern, kein `fsops`/`upload_fs`, baut auf Windows/macOS): `metadata`, `exists`, `create_dir_all`,
`remove_dir`, `remove_file`, `move_file` (Hartlink + Kopie) und `companions() -> &dyn CompanionFs` (kein Trait-Upcasting
nötig). Der Adapter leitet 1:1 an `Fs`/`move_file_with` weiter: `delete_file_with_conn_fs(&FaultFs)` ruft dieselben
`Fs`-Methoden in derselben Reihenfolge auf wie vorher. `create_dir_all` und `exists` gehen wie vorher direkt an `std::fs`
(Restore hatte nie ein `Fs`-Argument). `now` der Startbereinigung ist Parameter (feste Zeit in Kern-Tests).

## Risiken

- Fehler entstehen im Kern als `ImportError`, an der Grenze `CmdError` (Log-Parameter bleiben, Fix 677fb423); die
  `cmd`-Logzeile eines im Stapel übersprungenen Fehlers entfällt (die Zeile `übersprungen: ...` bleibt).
- Restore: `move_file` lief bisher über `placement::move_file`, jetzt `move_file_with(RealFs)` (gleiche Schritte, P0a-Vereinbarung).
- Windows/macOS: Papierkorb auf anderem Laufwerk (Hartlink scheitert, Kopierpfad), Sperren (`PermissionDenied`), Groß/Klein
  bei „(wiederhergestellt)“-Kollision und Ordner neu anlegen: Fake in Goldens; echte Läufe nur in der CI.
- Startbereinigung rechnet gegen die echte Uhr (Goldens setzen `deleted_at` relativ zu jetzt: 6 d / 8 d); exakte Grenze nur im Kern-Test.

## Szenarien der Goldens (alter Code zuerst, `golden_trash_tests.rs`)

1. `trash_delete`: einzeln (Datei, Zeile, Begleitdatei folgt), schon im Papierkorb, Datei fehlt (nur Soft-Delete),
   Pfad ein Verzeichnis/ohne Rechte-Fehler, Namenskollision im Papierkorb, DB-Fehler (Trigger) mit Rückroll, Rückroll scheitert.
2. `trash_delete_batch`: Stapel mit ungültiger/doppelter/unbekannter Id, ein Eintrag scheitert, Rest läuft.
3. `trash_delete_faults`: `FaultFs` Fehler in Schritt n (Metadata PermissionDenied, HardLink -> Kopierpfad „anderes Laufwerk“,
   RemoveFile nach Kopie, Rückroll-Move).
4. `trash_restore`: Erfolg, Name belegt (`(wiederhergestellt)`, mit/ohne Endung), nicht im Papierkorb, ohne `trash_path`,
   Ordner neu anlegen (Zeilen, Ordner-Herkunft), Ordner weg + geschützter Pfad abgelehnt, Papierkorbdatei fehlt, DB-Fehler mit
   Rückroll und Aufräumen der neu angelegten Ordner, Begleitdateien kommen mit.
5. `trash_restore_case`: Groß/Klein-Kollision (`case_insensitive` auf Linux über Fake-Namen im Zielordner).
6. `trash_purge`: endgültig (einzeln, Datei fehlt, nicht im Papierkorb, unbekannt, Begleitdatei im Papierkorb), leeren
   (mit Begleitdateien, Datei nicht entfernbar -> bleibt), Startbereinigung 6 d bleibt / 8 d weg, Begleitdateien, Fehler je Eintrag.
7. `trash_folder_models`: `trash_folder_with_conn` (Host-Operation `ModelTrash`) mit Modellen und Teilfehler, danach Restore.

Alle Szenarien schreiben Rückgabe/Fehler-JSON, alle `GOLDEN_TABLES` und den Dateibaum. Unix-spezifisches (Rechte, Symlink)
in eigene `#[cfg(unix)]`-Goldens. Danach: Umbau, Goldens byte-gleich, `trash.rs`-Tests und `fs_fault_tests` unverändert,
neue Kern-Tests mit Fake-`ModelTrashFs` (Fehler in Schritt n, feste Zeit).

## Umsetzung (Abweichungen vom Zuschnitt)

- Goldens liegen in `golden_trash_tests.rs` (13 Szenarien, 1 davon `#[cfg(target_os = "linux")]`); der Gruppe `trash_delete_faults` fehlt ein Restore-Gegenstück,
  weil Restore vorher kein `Fs`-Argument hatte; Restore-Fehler in Schritt n sind Kern-Tests (`model_trash::tests`) mit Fake.
- Tote Hüllen in `companions.rs`, `mod.rs::move_file`, `folders.rs::find_catalog_parent` sind nur noch von Tests gebraucht: `#[cfg(test)]` bzw.
  `#[cfg_attr(not(test), allow(dead_code))] mod companions;` (companions.rs bleibt unberührt).
- `error_code_tests.rs` scannt `model_trash.rs` (wie `folder_trash.rs`).
