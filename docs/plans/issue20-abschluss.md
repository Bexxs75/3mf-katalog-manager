# Issue #20: Abschluss (Weg A, Stand 25.11.2026)

## Was erreicht ist

Die Dateiaktionen des Desktops liegen nicht mehr in `src-tauri/src/commands/*`, sondern in `katalog-core`.
Der Desktop behält dünne Hüllen (Tauri-Befehle, Signaturen, Fehlercodes und Texte, Log-Zeilen).

| Paket | Kernmodul | Inhalt |
|---|---|---|
| P2 | `folder_create` | Ordner anlegen |
| P3 | `companion_actions` | Begleitdateien hinzufügen, löschen, wiederherstellen, mitnehmen |
| P4 | `file_rename` | Datei umbenennen, Datumsumbenennung |
| P5 | `file_move` | Dateien verschieben (einzeln und Stapel) |
| P6 | `folder_move` | Ordner umbenennen und verschieben |
| P7 | `model_trash` | Papierkorb für Modelle |
| P8 | `disk_sync` | Abgleich mit der Platte: Scan-Planung und Anwenden |

Jedes Paket wurde so umgebaut: Zuschnitt, Dateisystemgrenze im alten Code, Golden-Szenarien mit dem alten
Code aufgenommen, Umstellung, Goldens byte-gleich, danach eine Codex-Prüfung (P3, P4, P5 mit behobenen
Befunden; P6, P7, P8 ohne Befund).

## Zwei Wirkungsversionen (ehrlich)

Es gibt weiterhin **zwei** Fassungen derselben Wirkung:

- **Desktop** (`katalog-core`-Module oben): absolute Pfade, Verhalten wie in 0.18.x.
- **Web/Server** (`katalog-core::fsops`, `upload_fs`): Verhalten wie bisher, eigene Regeln (z. B. Journal,
  Katalogwurzel, Upload).

Beide teilen reine Bausteine (Namensregeln, Klassifizierung, Pfadprüfungen), aber nicht den Ablauf. Eine
Vereinheitlichung war nicht Ziel von #20, weil sie sichtbares Verhalten ändern würde.

## Nicht Teil von #20

- **P9** (Archiv-Ablauf) bleibt Desktop-eigen; Folge-Issue.
- Wettlauf zwischen Prüfung und `rename` (Forgejo #81, v0.19.1).
- Auffälligkeiten aus P8, unverändert übernommen: A1 `move_emptied_folders` nutzt `CASE_INSENSITIVE_PATHS`
  statt des Parameters, A2 Größe 0 bei Metadatenfehler in `classify_by_content`, A3 stille Toleranz von
  `symlink_metadata`-Fehlern beim Anwenden.

## Prüfung auf drei Systemen

Die Goldens laufen in der CI unter Linux, Windows und macOS (`desktop-compile`). Plattformabhängiges ist im
Harness normalisiert (`golden_harness.rs`):

- keine Größen von Symlinks im Dateibaum (Länge des Zielpfads),
- Windows-Wurzel ohne `\\?\`-Präfix, `//?/` in Meldungen entfernt,
- Fixture-Pfade nativ gebaut (`nat()`), weil `join("a/b")` unter Windows gemischte Trenner ergibt,
- Fehlertexte des Betriebssystems auf Kategorien abgebildet.

Drei Szenarien laufen nur unter Unix, weil der Aufrufverlauf unter Windows durch die Kollisionsprüfung mit
`canonicalize` abweicht: `golden_file_rename_case_only`, `golden_folder_rename_case_only` und
`golden_folder_move_faults_resolving` (nummerierte `canonicalize`-Fehler). Die Groß/Klein-Regeln selbst
laufen in den `katalog-core`-Tests mit `case_insensitive` als Parameter auch unter Windows.

## Bekannte Zufallsfehler in der CI

Im Linux-Job (`ci-checks`) fielen wiederholt Zeit- und Reihenfolgetests aus, jeweils ein anderer, und liefen beim
erneuten Start durch: `issue_scan_runs_on_its_own_thread_and_answers_a_timeout`, `rescanRunning` (503) in den
`katalog-server`-Tests, Fokus-Tests der Web-Oberfläche. Sie hängen nicht an diesen Paketen. Der Umbau auf
Zählgrößen statt Uhrzeit ist vorgemerkt (Memory „Zeittests umbauen nach 0.18.0“).
