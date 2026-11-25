# Issue #20, Paket P3: Begleitdatei-Aktionen im Kern (Zuschnitt)

Ziel: die Begleitdatei-Aktionen aus `src-tauri/src/commands/companions.rs` wandern in
`crates/katalog-core/src/companion_actions.rs`. Die Tauri-Befehle, `*_with_conn`-Funktionen,
Fehlercodes/-texte, Logzeilen (Targets `datei`, `trash`, `archive`, `startup`) und das
Desktop-Papierkorbformat (`companion-<id>-<name>`, §3.1) bleiben gleich.

## Was wandert (Kern) / was bleibt (Desktop)

| Kern (`companion_actions`) | Desktop (`companions.rs`) |
|---|---|
| `add_companion` (Prüfungen, Kopie, Zeile, Rückroll) | Tauri-Befehle, `blocking`, `CompanionDto`, `to_dto` |
| `trash_one`, `trash_with_model` | Lesen/Öffnen/Vorschau (`open_checked`, `open_info_*`, `image_data_*`), bereits in `companion_read` |
| `restore_one`, `restore_companion`, `restore_with_model` | `CASE_INSENSITIVE_PATHS` (`cfg!`), wird als Parameter übergeben |
| `remove_trash_file`, `purge_*` (Purge, endgültig löschen) | dünne Hüllen mit alter Signatur (`pub(super)`), `Carried`-Hülle |
| `assign_from_archive`, `move_into_unique` | |
| `carry` + `Carried` (`finish_with`/`undo_with`), `create_unique`, `resolve_extras_target`, `row_error`/`folder_error_code`, Text zu `Refusal` | |

Vorhandene Bausteine werden genutzt (`companions`, `companion_read`, `import::placement`,
`import::file_names::validate_name`, `folder_create::find_catalog_parent`, `safe_file`); an
`companion_read` und am Web-Verhalten ändert sich nichts. `reject_if_outside_trash_dir` zieht mit
(einzige Nutzung war `companions.rs`) und verschwindet aus `commands/mod.rs`.

## Trait und Parameter

`CompanionFs` (Kern, kein `fsops`/`upload_fs`, baut auf Windows/macOS): `symlink_metadata`, `exists`,
`canonicalize`, `create_dir`, `remove_dir`, `remove_file`, `rename`, `create_new` (für `create_unique`),
`move_file` (Hartlink + Kopie, Fallback wie `placement::move_file`). Der Desktop-Adapter `CompanionFsAdapter(&dyn Fs)`
leitet 1:1 an `Fs`/`move_file_with` weiter, damit die Aufruffolge des `FaultFs` unverändert bleibt
(`create_new` geht direkt an `std::fs`, wie vorher). `case_insensitive: bool` ist Parameter von `carry`
(Vorbild `companions.rs:759`); der Desktop übergibt `CASE_INSENSITIVE_PATHS`.

## Risiken

- Fehler werden im Kern als `ImportError` gebildet und an der Grenze in `CmdError` gewandelt; die
  Log-Zeile beim Erzeugen (`target: "cmd"`) entsteht nur noch für zurückgegebene Fehler (wie bei P2).
- `restore_*`/`purge_*`/`add_*` hatten kein `Fs`-Argument (nutzten `std::fs`/`RealFs`); Verhalten bleibt, der Kern bekommt
  den Adapter mit `RealFs`.
- Windows/macOS: Groß/Klein (`carry` mit `case_insensitive=true`), Papierkorb auf anderem Laufwerk
  (Hartlink scheitert -> Kopierpfad), Sperren (`PermissionDenied` beim Verschieben): per Fake, echte Läufe erst in CI.
- `move_file_with` und `placement::move_file` müssen synchron bleiben (P0a-Vereinbarung).

## Szenarien der Goldens (alter Code zuerst, `golden_tests.rs`)

1. `companion_add`: Erfolg, doppelter Name (`name (2)`), Modell im Papierkorb, unbekanntes Modell, Pfad relativ/`..`,
   Symlink, Verzeichnis, fehlende Datei, Typ nicht erlaubt (exe als png), zu viele (Limit), Ordner `_extras` ist Symlink.
2. `companion_trash_restore`: Entfernen (Zeile+Datei im Papierkorb), schon im Papierkorb, Datei fehlt (Zeile fällt weg),
   Wiederherstellen, Name belegt (`(2)`), Modell im Papierkorb, Datei nicht Papierkorb, DB-Fehler im Rückroll (Trigger).
3. `companion_with_model`: `trash_with_model_fs`/`restore_with_model` (Stempel `deleted_at`), Papierkorb auf „anderem Laufwerk“
   (`FaultFs` HardLink schlägt fehl -> Kopierpfad), Sperrfehler `PermissionDenied` im Verschieben (Datei bleibt aktiv).
4. `companion_purge`: Purge einzeln/je Modell/alle/abgelaufen, Pfad außerhalb des Papierkorbs (verweigert), Web-Papierkorb-Datei.
5. `companion_assign_archive`: ein Modell, mehrere Modelle (je Ordner), nicht erlaubter Typ (ignoriert), Limit, Namenskollision.
6. `companion_carry`: Verschieben (Erfolg), Namenskollision im Ziel (`companionNameTaken`, Rückroll), Ziel-Ordner blockiert/Anlage scheitert,
   Quellordner Symlink, Groß/Klein (`case_insensitive=true` auf Linux: Ordner umbenannt, Ordner mit Fremdzeilen bleibt),
   Fehler in Schritt n über `FaultFs` (Rename/HardLink/CreateDir/RemoveDir), Rückroll (`undo`), `finish` räumt alten Ordner.

Alle Szenarien schreiben Rückgabe/Fehler-JSON, alle `GOLDEN_TABLES` (u. a. `companion_files`) und den Dateibaum.
Danach: Umbau, Goldens byte-gleich, `companions_tests.rs` unverändert, neue Kern-Tests mit Fake-`CompanionFs` (Fehler in Schritt n).
