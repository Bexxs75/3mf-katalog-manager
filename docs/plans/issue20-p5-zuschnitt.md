# Issue #20, Paket P5: Datei(en) verschieben im Kern (Zuschnitt)

Ziel: `move_file_core` samt Stapel (`move_files_to_folder_with`, Grenze 5000) und allen Rückroll-Zweigen
aus `src-tauri/src/commands/files.rs` (nur der Verschieben-Block) wandert nach
`crates/katalog-core/src/file_move.rs`. Tauri-Befehle, Signaturen der `*_with_conn`-Funktionen,
Fehlercodes/-texte, Ablehnungscodes des Stapels (`name_exists`, `source_missing` ...), Logzeilen und die
Reihenfolge der Dateisystem-Aufrufe bleiben gleich. `import::placement::move_file` bleibt das Primitiv;
die Fassung am Desktop ist weiter `fs_boundary::move_file_with` (P0a: beide synchron halten).

## Was wandert (Kern) / was bleibt (Desktop)

| Kern (`file_move`) | Desktop (`files.rs`) |
|---|---|
| `move_file` (= `move_file_core_fs`: Prüfungen, Zielordner, Sperrliste, Kollision, Verschieben, Mitnahme, DB, Rückrollen) | `#[tauri::command]` `move_file_to_folder`, `move_files_to_folder` (Gate, Parsen der IDs) |
| `MoveOutcome`, `MoveRefusal` (Code + Fehler), `MoveFileResult` (Serde, `camelCase`), `MAX_BATCH_MOVE_FILES` | `move_file_to_folder_with_conn(_fs)`, `move_files_to_folder_with` als dünne Hüllen mit alter Signatur |
| Stapel `move_files`: Grenze, Zielordner einmal prüfen, Schleife, Ergebnis je Datei | `FileMoveFs`-Adapter über `CompanionFsAdapter(&dyn Fs)` |

Begleitdateien nur über die P3-Kern-API (`companion_actions::carry`, `Carried::finish/undo`); `companions.rs`,
`companion_actions.rs`, `rename.rs`, `trash.rs` bleiben unberührt.

## Trait und Parameter

`FileMoveFs: CompanionFs` (kein `fsops`/`upload_fs`, baut auf Windows/macOS) ergänzt nur `is_dir` und `is_file`
(wie `Path::is_dir/is_file`: folgen Links, Fehler = `false`). Verschieben (`move_file`, Hartlink, Kopier-Fallback)
kommt aus `CompanionFs`. Der Desktop-Adapter leitet 1:1 an `Fs::is_dir/is_file` weiter, damit das Aufrufprotokoll
des `FaultFs` unverändert bleibt. `case_insensitive: bool` ist Parameter (Desktop: `cfg!(any(windows, macos))`).
Der Stapel ist über den Fehlertyp des Sperr-Closures `E: From<ImportError>` generisch; der Desktop nutzt `CmdError`.

## Festlegungen (Befund R1 des Plans)

- Verschieben über Laufwerke bleibt eine Folge des Primitivs (Hartlink scheitert, Kopie + Quelle löschen). Keine neue
  Richtlinie, kein Journal. Ein Fehler nach der Kopie entfernt die Kopie, die Quelle bleibt; unverändert.
- Fehler entstehen im Kern als `ImportError` und werden an der Grenze in `CmdError` gewandelt (P3-Fix: Log-Parameter
  bleiben). Im Stapel wird ein verworfener Fehler ebenfalls gewandelt, damit die Logzeile wie vorher entsteht.

## Szenarien der Goldens (alter Code zuerst, `golden_move_tests.rs`)

1. `file_move_single`: Erfolg, schon im Ziel, Zielordner `None`, nur Katalog-Zuordnung, jede Ablehnung (Datei/Ordner
   unbekannt, im Papierkorb, Sperrliste Ziel/Quelle, ungültiger Pfad, Ziel/Quelle fehlt auf der Platte, Name belegt,
   Name nur im Katalog belegt), Mitnahme der Begleitdateien (`finish` räumt den alten Ordner).
2. `file_move_batch`: gemischter Stapel (moved/skipped/failed je Code), `invalid_id`, Grenze 5000 (genau 5000 ok, 5001 abgelehnt),
   Zielordner unbekannt/gesperrt/fehlt.
3. `file_move_faults` (`FaultFs`, Aufrufprotokoll je Schritt): Hartlink, Kopier-Fallback („anderes Laufwerk“), Fehler
   nach der Kopie, Quelle nicht löschbar, Begleitdatei scheitert (Rückroll ok / Rückroll scheitert auch), DB scheitert
   (Rückroll ok / scheitert auch, jeweils Hartlink und Kopierweg), Pfad im Katalog belegt, Groß/Klein-Zwilling.
4. `file_move_symlink` (nur Unix): Quelle ist Symlink (kein Hartlink, Kopie).

Alle Szenarien schreiben Rückgabe/Fehler-JSON, alle `GOLDEN_TABLES` und den Dateibaum.
Danach: Umbau, Goldens byte-gleich, vorhandene Tests unverändert, neue Kern-Tests mit Fake-`FileMoveFs`.

## Risiken

- Windows/macOS nur per CI belegt (Sperren, Groß/Klein, anderes Laufwerk laufen hier über `FaultFs`).
- `error_code_tests.rs` prüft Quellen nach Textfehlern: `file_move.rs` kommt in die Liste, `move_file_core` fällt aus `files.rs`.
