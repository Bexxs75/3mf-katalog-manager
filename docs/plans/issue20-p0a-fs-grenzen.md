# Issue #20, Paket P0a: Dateisystem-Grenzen der Desktop-Dateiaktionen

Modul: `src-tauri/src/commands/fs_boundary.rs` (Trait `Fs`, `RealFs`, nur-Test `FaultFs`).
Tests: `src-tauri/src/commands/fs_fault_tests.rs`. Das Verhalten der App ist unverändert:
alle bisherigen Funktionen und Tauri-Befehle behalten ihre Signatur und rufen die
`..._fs`/`..._with`-Variante mit `RealFs` (1:1 `std::fs`) auf.

## Was injiziert ist

Operationen im Trait: `symlink_metadata`, `metadata` (daraus `is_dir`/`is_file`/`exists`),
`read_dir_names`, `canonicalize` (nur Nicht-Unix-Kollisionsprüfung), `rename`, `hard_link`,
`remove_file`, `remove_dir`, `create_dir`, `copy_new` (Kopierhälfte eines Kopier-Verschiebens).

Abläufe mit Variante `..._fs`:

| Ablauf | Datei | Injiziert |
|---|---|---|
| `move_file_core` (Einzel- und Sammel-Verschieben) | files.rs | `is_dir`, `is_file`, Verschieben samt Rückroll-Pfaden |
| `rename_file_with_conn` | files.rs | Kollisionsprüfung, Umbenennen, Rückrollen |
| `rename.rs`: `target_collides`, `rename_with`, Zwischennamen-Umbenennung, `rename_file_on_disk`, `rename_on_disk` | rename.rs | alle Metadaten-/Verzeichnis-/Rename-Zugriffe |
| `delete_file_with_conn` (Papierkorb) | trash.rs | `metadata`, Verschieben ins/aus dem Papierkorb |
| Begleitdateien: `carry`, `Carried::undo/finish`, `trash_one`, `trash_with_model`, `is_real_dir/file`, `remove_dir_if_empty` | companions.rs | Verschieben, Ordner anlegen/entfernen/umbenennen, Metadaten |
| `move_file` (hard_link, remove_file, Kopie als Rückfall) | `move_file_with` in fs_boundary.rs | alles |

`move_file_with` bildet `katalog_core::import::placement::move_file` Schritt für Schritt nach
(beide müssen synchron bleiben). Dafür wurde in katalog-core die Kopierhälfte als
`copy_file_new` abgespalten; `move_file_by_copy` ruft sie auf, Verhalten unverändert.

`FaultFs`: echte Dateien im Temp-Ordner; `fail(op, n, kind)` bzw. `fail_on(op, pfad_teil, n, kind)`
lässt den n-ten Aufruf mit einer `io::ErrorKind` scheitern, `calls()`/`count()` liefern die
Aufrufliste, `case_insensitive()` simuliert die Namenssuche von macOS/Windows (fehlender Name
trifft Geschwister mit gleichem Kleinbuchstaben-Namen; Link/Kopie darauf = `AlreadyExists`).

## Was nicht injiziert ist (Grund)

- Prüfungen ohne Dateisystem-Mutation: `reject_if_sensitive_path`, `resolve_path_for_sensitivity_check`,
  `extras_dir_problem`, `check_row_folder` (leben im Kern, nutzen `canonicalize`/`symlink_metadata`;
  gehören in spätere Kern-Pakete).
- Wiederherstellen (`restore_*`), Endgültig-Löschen/Leeren, `add_companion`, `assign_from_archive`,
  Ordner-Umbenennen/-Verschieben/-Papierkorb (`folders.rs`, `folder_trash.rs`), `date_rename`:
  rufen `rename_on_disk`/`rename_file_on_disk`/`carry` mit `RealFs` auf; eigene `_fs`-Varianten
  folgen mit dem jeweiligen Paket. Nicht Teil von P0a (kein Umbau darüber hinaus).
- Datei-Inhalte lesen/schreiben, `std::fs::File`-Handles (Import, Vorschau): keine Fehler „in Schritt n“.
- DB-Fehler: weiterhin per SQLite-Trigger in den Bestandstests.
- `Metadata` ist die echte `std::fs::Metadata`; `FaultFs` kann sie nicht fälschen, sie kommt von
  der echten Platte (Gerät/Inode-Vergleich wirkt also real).

## Szenarien, die echte Windows-/macOS-Runner brauchen

- Echte Groß-/Kleinschreibung-unempfindliche Volumes: Case-only-Umbenennung (`A.stl` -> `a.stl`) über
  Zwischennamen, Kollisionsprüfung per Datei-Identität (Windows: `canonicalize`-Zweig, kein Inode),
  `Carried`-Ordner-Umbenennung mit `CASE_INSENSITIVE_PATHS`. `FaultFs` simuliert nur die Namenssuche.
- Windows-Sperren: Datei in Benutzung beim `rename`/`remove_file` (`ERROR_SHARING_VIOLATION`), Schreibschutz,
  Zugriffsrechte; Fehlerart und Zeitpunkt (sofort/verzögert) stammen vom echten Betriebssystem.
- Hartlinks auf FAT/exFAT/SMB/Netzlaufwerken und `EXDEV`/`ERROR_NOT_SAME_DEVICE` beim echten Laufwerkswechsel
  (Kopierpfad mit echten Rechte-/Zeitübernahmen, `copied_mode`/ACL).
- Symlinks/Junctions unter Windows (Berechtigung, `is_symlink`-Erkennung), macOS-Unicode-Normalisierung
  (NFD-Dateinamen) bei `normalized_lowercase`.
- Papierkorb-Pfad bei realem Verschieben auf anderes Laufwerk (App-Papierkorb liegt im Datenordner).
- Rückroll-Fenster bei parallelem Zugriff (Virenscanner, Spotlight, OneDrive) auf den Zwischennamen.
