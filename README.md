# 3MF Katalog Manager

Plattformunabhängige Desktop-Anwendung zur Katalogisierung und Verwaltung von 3MF-, STL-, OBJ- und STEP-Dateien für den 3D-Druck. Gebaut mit [Tauri](https://tauri.app/) (Rust-Backend) und React/TypeScript/Tailwind.

**Webseite:** [3mfkatalog.de](https://3mfkatalog.de) – Überblick, Download und Screenshots.

**Neu hier?** Das [Benutzerhandbuch](docs/benutzerhandbuch/BENUTZERHANDBUCH.md) (DE + EN) erklärt alle Funktionen bebildert und Schritt für Schritt, ganz ohne Vorwissen über die App.

[![Webseite](https://img.shields.io/badge/Webseite-3mfkatalog.de-ff7a5c)](https://3mfkatalog.de) [![Join Discord](https://img.shields.io/badge/Join-Discord-5865F2?logo=discord&logoColor=white)](https://discord.gg/abfVNfFqu3)

## Screenshots

| Katalog (Raster) | Nach Ordnern gruppiert | Einstellungen (Info & Update-Check) |
|---|---|---|
| ![Katalog in der Raster-Ansicht](docs/benutzerhandbuch/bilder/02-katalog-grid.png) | ![Nach Ordnern gruppierte Ansicht](docs/benutzerhandbuch/bilder/16-ordner-ansicht.png) | ![Einstellungen mit Update-Check](docs/benutzerhandbuch/bilder/20-einstellungen-info-update.png) |

## Downloads

Fertige Pakete gibt es auf der [Releases-Seite](https://github.com/Bexxs75/3mf-katalog-manager/releases). Für Linux, Windows und macOS steht jeweils **eine von zwei Varianten** zur Wahl:

| Variante | Enthält | Größe | Für wen |
|---|---|---|---|
| **mit STEP-Vorschau** (`-step`) | 3D-Vorschau + Abmessungen/Volumen/Körperzahl auch für `.stp`/`.step`-Dateien (via Open CASCADE) | größer (OCCT-Bibliotheken mit eingepackt) | wer STEP-Dateien katalogisiert und die Vorschau braucht |
| **ohne STEP-Vorschau** (Standardname, kein Suffix) | STEP-Dateien lassen sich weiterhin katalogisieren (Tags, Suche, Umbenennen, Papierkorb), aber ohne 3D-Vorschau/CAD-Metadaten | kleiner | wer nur 3MF/STL/OBJ nutzt oder STEP nur ablegen, nicht ansehen will |

Beide Varianten sind ansonsten funktionsgleich. Die Downloads sind unsigniert (siehe [Status](#status)).

## Funktionen

- **3MF-, STL-, OBJ- und STEP-Parsing** — 3MF (OPC-Container-Entpackung inkl. eingebettetem Thumbnail), ASCII-/Binär-STL und OBJ jeweils mit 3D-Vorschau; STEP-Dateien (`.stp`/`.step`) werden über Open CASCADE gelesen und liefern 3D-Vorschau, Abmessungen, Volumen und Körperzahl. Für jede Plattform (Linux/Windows/macOS) gibt es zwei Downloads: eine Variante **mit** STEP-Vorschau und eine kleinere Variante **ohne** (nur Katalogisierung ohne 3D-Vorschau für STEP) — siehe [Downloads](#downloads).
- **Automatische Metadaten-Extraktion** — Abmessungen, Volumen, Objektanzahl, Material (sofern in der 3MF vorhanden)
- **Automatische Hashtag-Generierung** — Vorschläge aus Dateiname, Geometrie-Merkmalen und Slicer-Profildaten, vom Nutzer editierbar
- **3D-Live-Vorschau** — three.js-Rendering direkt aus der Mesh-Geometrie, wenn kein eingebettetes Thumbnail vorhanden ist
- **Tag- und Ordnerverwaltung**, Suche und Filterung
- **Import** einzelner Dateien oder ganzer Ordner (inkl. Unterordner) per Dialog oder Drag & Drop, über ein gemeinsames Dropdown-Menü in der Kopfzeile
- **Komfort-Ansicht** — alternative, deutlich lesbarere Oberfläche (größere Schrift, Grafiken und Bedienelemente) neben der bestehenden kompakten Ansicht, umschaltbar im Einstellungen-Panel; Favorit-Kennzeichnung je Modell in beiden Ansichten
- **In Slicer öffnen** — beliebig viele selbst hinterlegte Slicer-Programme (herstellerunabhängig) direkt aus dem Katalog heraus starten
- **Mehrsprachige Oberfläche** — Deutsch, Englisch, Spanisch, Französisch, umschaltbar zur Laufzeit
- **Hell-/Dunkel-Theme** mit System-Erkennung und manueller Auswahl, persistiert lokal
- **Filament-Lager** — eigenständige Verwaltung deiner Filamentspulen (Material, Hersteller, Farbe, Lagerort, Durchmesser, Ursprungs-/Restgewicht, Preis) mit Autocomplete für Material/Hersteller/Lagerort, unabhängig vom Modell-Katalog; umschaltbar zwischen Karten-Dashboard (Bestandsbalken, Statusfarbe) und sortierbarer Inventarliste, inkl. Statistik-Leiste und Status-Filter; beim Neuanlegen lassen sich mehrere identische Spulen auf einmal erfassen (jede mit eigenem, unabhängig verfolgtem Restbestand)
- **Drucker & AMS-Fächer** — Drucker und ihre Mehrfarbeinheiten (Bambu AMS, AMS lite, AMS HT, Creality CFS, Prusa MMU3, Anycubic ACE Pro, Spulenhalter oder eigene mit frei wählbarer Fachanzahl) anlegen und Spulen per Drag & Drop oder Klick in die Fächer legen. Jeder neue Drucker bekommt automatisch einen Spulenhalter, sodass auch Drucker ohne AMS sofort eine Spule aufnehmen. Spulen im Drucker erscheinen getrennt vom Lager in einer eigenen Spalte; beim Herausnehmen kehren sie automatisch an ihren Stammplatz zurück. Spulen haben einen echten Farbwert (Palette oder Hex) zusätzlich zum Farbnamen.
- **Katalog-Erweiterungen** — Druckstatus-Toggle + Gewicht pro Modell (echter Wert aus dem Slicer, falls die 3mf bereits gesliced wurde, sonst grobe Schätzung aus Volumen × Materialdichte), Sortierung nach "Zuletzt angesehen", NEU-Badge für kürzlich importierte Modelle, Creators-Filter (aus 3MF-Designer-Metadatum), automatische Erkennung exakter Datei-Duplikate beim Import per Inhalts-Hash
- **Filamentverbrauch aus dem Slicer** — liest den in OrcaSlicer/Bambu Studio gesliceten Filamentverbrauch (`Metadata/slice_info.config`) mit aus: reales Gewicht statt Schätzung, Aufschlüsselung pro Druckplatte und Filament (Typ, Farbe, Gramm, Meter) auf der Modell-Detailseite; Button "Metadaten neu einlesen" holt die Werte nachträglich, wenn eine bereits katalogisierte Datei in OrcaSlicer/Bambu Studio nachgesliced wurde
- **Materialkosten-Schätzung** — bei Modellen mit echtem Slicer-Filamentverbrauch zusätzlich eine geschätzte Materialkosten-Summe auf der Detailseite, berechnet aus Verbrauch und den Preisen passender Spulen im Filament-Lager
- **Druckprotokoll** — zusätzlich zum Druckstatus-Toggle ein Protokoll mehrerer Druckversuche pro Modell (Datum, Notiz, Foto), unabhängig vom Druckstatus
- **Katalog-Backup** — kompletter Katalog (Datenbank + Einstellungen) als ZIP exportierbar und wieder importierbar, mit automatischer Sicherung der bestehenden Datenbank vor jedem Import
- **Archive direkt entpacken** — einzeln importierte oder hineingezogene Archive (`.zip`, `.7z`, `.rar`, `.tar`, `.tar.gz`/`.tgz`, `.tar.bz2`, `.tar.xz`, `.tar.zst`) werden nach Rückfrage in einen Unterordner entpackt und die enthaltenen Modelle katalogisiert. Zielordner, Umgang mit bereits vorhandenen Ordnern und das Löschen des Original-Archivs entscheidest du im Dialog; nichts wird überschrieben. Aus Sicherheitsgründen werden Programme, Skripte und Verknüpfungen aus Archiven nie entpackt. Passwortgeschützte und mehrteilige Archive werden erkannt, aber nicht entpackt.
- **Modell-Thumbnails** — Raster-Ansicht zeigt ein echtes Bild pro Modell (eigenes Upload, eingebettetes 3MF-Thumbnail oder automatisch aus der 3D-Live-Vorschau erzeugter Snapshot), zusätzlich pro Modell eine Quelle als Link hinterlegbar
- **Warteschlange** — geordnete, per Drag & Drop sortierbare Liste ("als Nächstes drucken") in eigener Sidebar-Sektion, automatisches Entfernen beim Markieren als gedruckt
- **Gespeicherte Filter** — häufig genutzte Kombinationen aus Ordner/Tag/Creator/Suche/Sortierung unter einem Namen speichern und per Klick wieder anwenden
- **Aufräum-Vorschläge** — manuell auslösbarer Katalog-Scan findet verwaiste Dateipfade und Bestands-Duplikate, Bereinigung per Auswahl-Dialog
- **Modell-Detailseite** — vollflächige Ansicht (Doppelklick auf ein Modell) mit großer 3D-Vorschau, allen Metadaten und Druckplatten-Anzahl bei Bambu-Studio-/OrcaSlicer-Dateien; eigene Dreh-Steuerelemente (Auto-Rotation + 15°-Schritt-Buttons) zusätzlich zum freien Maus-Ziehen
- **Papierkorb** — gelöschte Modelle bleiben 7 Tage wiederherstellbar statt sofort entfernt zu werden, eigene Ansicht mit Mengen-Badge
- **Mehrfachauswahl** — Checkboxen in der Katalogübersicht, "Alle auswählen", Aktionsleiste für Warteschlange/Sammlung/Druckstatus/Tags/Löschen über mehrere Modelle gleichzeitig
- **Tastaturkürzel** — `/` fokussiert die Suche, Pfeiltasten navigieren räumlich im Raster (Hoch/Runter springt zur nächsten Zeile), Leertaste schaltet die Mehrfachauswahl-Checkbox um, Entf/Rücktaste öffnet bei aktiver Mehrfachauswahl die Löschen-Bestätigung
- **Umbenennen** — Modelle direkt per Kontextmenü umbenennen; die Dateiendung bleibt dabei fest
- **Automatische Slicer-Erkennung** — durchsucht beim Start bekannte Installationsorte (Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer, UltiMaker Cura auf Linux/Windows) und ergänzt Treffer automatisch; manuelles Hinzufügen für Custom-Forks bleibt möglich
- **Bevorzugte Ansicht** — Einstellung, ob Katalog und Detailseite standardmäßig das eingebettete Datei-Bild oder eine gerenderte 3D-Ansicht zeigen; fehlende Schnappschüsse werden bei Bedarf automatisch im Hintergrund nachgerendert
- **Sammlungen** — dritter Organisationsmechanismus neben Ordnern und Tags: mehrere Modelle explizit zu einem Projekt zusammenfassen, mit manuell festlegbarer Reihenfolge (Drag & Drop); Erstellung über Mehrfachauswahl
- **Eigenes App-Icon** — isometrischer 3D-Druck-Layer-Würfel in den echten App-Akzentfarben
- **Content-Security-Policy** aktiv (kein `csp: null`), Quell-URL-Felder und "In Slicer öffnen" serverseitig validiert

## Status

Dieses Projekt befindet sich in aktiver Entwicklung. Der lokale Katalog (Import, Parsing, Tagging, Suche, 3D-Vorschau, Mehrsprachigkeit, Theming) und "In Slicer öffnen" sind funktionsfähig. Folgendes ist noch **nicht** umgesetzt:

- **Cloud-Anbindung** (Google Drive u. a.): war vorhanden, wurde aber wieder entfernt — zu instabil/fehleranfällig für den Alltagsgebrauch. Wird bei Gelegenheit sauber neu konzipiert, siehe CHANGELOG
- **"In Slicer öffnen" bei manuell hinzugefügten Slicern unter macOS**: für automatisch erkannte Slicer (Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer, UltiMaker Cura) funktioniert das Öffnen zuverlässig, da die Erkennung bereits die richtige, direkt ausführbare Programmdatei innerhalb des `.app`-Bundles findet. Bei manuell hinzugefügten Slicern muss im Dateidialog gezielt diese Datei ausgewählt werden, nicht das `.app`-Bundle selbst
- **STEP-Vorschau als separater Download**: Auf allen drei Plattformen gibt es dafür zwei Paketvarianten statt einer einzigen mit fest eingebauter STEP-Vorschau — siehe [Downloads](#downloads).
- **Code-Signing**: die macOS-`.dmg`- und Windows-`.msi`-Pakete sind unsigniert (kein Apple-Developer- bzw. Windows-Code-Signing-Zertifikat) — beim ersten Start warnen Gatekeeper bzw. SmartScreen entsprechend

## Geplant

- **Live-Anbindung an den Drucker** (z. B. Bambu Lab per MQTT im lokalen Netz): AMS-Belegung und Restmengen automatisch übernehmen
- **Automatisches Abbuchen** des Filamentverbrauchs nach einem Druck
- **„Reicht das Filament?“** – Bedarf eines Modells mit den verfügbaren bzw. eingelegten Spulen vergleichen
- **Historie der Fachbelegung** – welche Spule wann in welchem Fach steckte

## Tech-Stack

- **Framework:** Tauri 2 (Rust-Backend, WebView-Frontend)
- **Frontend:** React 19, TypeScript, Tailwind CSS, Vite
- **3D-Rendering:** three.js
- **Datenbank:** SQLite (`rusqlite`)

## Entwicklung

Voraussetzungen: Node.js, Rust-Toolchain (`cargo`), sowie die [Tauri-Systemabhängigkeiten](https://tauri.app/start/prerequisites/) für dein Betriebssystem. Für die standardmäßig aktivierte STEP-Vorschau wird zusätzlich Open CASCADE 7.8 oder 7.9 als dynamische Systembibliothek einschließlich Entwicklungsdateien benötigt.

```bash
npm install
npm run tauri dev
```

Ohne OCCT beziehungsweise für eine Fassung ohne STEP-Vorschau:

```bash
npm run tauri dev -- -- --no-default-features
# oder: npm run tauri build -- -- --no-default-features
```

Backend-Tests:

```bash
cd src-tauri
cargo test
```

Produktions-Build (Typprüfung + Vite-Build):

```bash
npm run build
```

## Projektstruktur

```
src/               React-Frontend (Komponenten, i18n, Hooks, Typen)
src-tauri/         Rust-Backend (Tauri-Commands, DB, Parser für 3MF/STL/OBJ/STEP, Tagging)
docs/              Zusätzliche Dokumentation
```

## Lizenz

Der Anwendungscode steht unter MIT — siehe [LICENSE](LICENSE). Die optionale STEP-Vorschau bindet Open CASCADE dynamisch unter LGPL-2.1 mit Open-CASCADE-Ausnahme ein. Details, Lizenztexte und Quellenhinweise stehen in [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md).

---

# 3MF Katalog Manager (English)

Cross-platform desktop application for cataloging and managing 3MF, STL, OBJ, and STEP files for 3D printing. Built with [Tauri](https://tauri.app/) (Rust backend) and React/TypeScript/Tailwind.

**Website:** [3mfkatalog.de/en](https://3mfkatalog.de/en/) – overview, download and screenshots.

**New here?** The [User Guide](docs/benutzerhandbuch/BENUTZERHANDBUCH.md) (DE + EN) explains every feature with screenshots, step by step, no prior knowledge of the app required.

[![Website](https://img.shields.io/badge/Website-3mfkatalog.de%2Fen-ff7a5c)](https://3mfkatalog.de/en/) [![Join Discord](https://img.shields.io/badge/Join-Discord-5865F2?logo=discord&logoColor=white)](https://discord.gg/abfVNfFqu3)

## Screenshots

| Catalog (grid) | Grouped by folder | Settings (Info & update check) |
|---|---|---|
| ![Catalog in grid view](docs/benutzerhandbuch/bilder/02-katalog-grid.png) | ![Folder-grouped view](docs/benutzerhandbuch/bilder/16-ordner-ansicht.png) | ![Settings with update check](docs/benutzerhandbuch/bilder/20-einstellungen-info-update.png) |

## Downloads

Prebuilt packages are on the [Releases page](https://github.com/Bexxs75/3mf-katalog-manager/releases). For Linux, Windows, and macOS, there are **two variants** to choose from:

| Variant | Includes | Size | For |
|---|---|---|---|
| **with STEP preview** (`-step`) | 3D preview + dimensions/volume/body count for `.stp`/`.step` files too (via Open CASCADE) | larger (bundles the OCCT libraries) | anyone cataloging STEP files who needs the preview |
| **without STEP preview** (plain name, no suffix) | STEP files can still be cataloged (tags, search, rename, trash), just without a 3D preview/CAD metadata | smaller | anyone who only uses 3MF/STL/OBJ, or just wants to store STEP files without viewing them |

Both variants are otherwise feature-identical. Downloads are unsigned (see [Status](#status-1)).

## Features

- **3MF, STL, OBJ, and STEP parsing** — 3MF (OPC container extraction incl. embedded thumbnail), ASCII/binary STL, and OBJ each have a 3D preview; STEP files (`.stp`/`.step`) are read through Open CASCADE and provide a 3D preview, dimensions, volume, and body count. Every platform (Linux/Windows/macOS) ships two downloads: one **with** STEP preview and one smaller one **without** (cataloging only, no 3D preview for STEP) — see [Downloads](#downloads-1).
- **Automatic metadata extraction** — dimensions, volume, object count, material (if present in the 3MF)
- **Automatic hashtag generation** — suggestions from filename, geometry features, and slicer profile data, editable by the user
- **Live 3D preview** — three.js rendering directly from the mesh geometry when no embedded thumbnail is available
- **Tag and folder management**, search and filtering
- **Import** of individual files or entire folders (including subfolders) via dialog or drag & drop, through a shared dropdown menu in the header
- **Comfort view** — alternative, significantly more readable interface (larger text, graphics, and controls) alongside the existing compact view, toggleable in the settings panel; favorite marking per model in both views
- **Open in slicer** — launch any number of self-configured slicer programs (vendor-agnostic) directly from the catalog
- **Multilingual UI** — German, English, Spanish, French, switchable at runtime
- **Light/dark theme** with system detection and manual selection, persisted locally
- **Filament inventory** — standalone management of your filament spools (material, manufacturer, color, storage location, diameter, original/remaining weight, price) with autocomplete for material/manufacturer/location, independent of the model catalog; toggleable between a card dashboard (stock bar, status color) and a sortable inventory list, including a stats bar and status filters; when adding new spools, several identical ones can be created at once (each with its own independently tracked remaining stock)
- **Printers & AMS slots** — add printers and their multi-material units (Bambu AMS, AMS lite, AMS HT, Creality CFS, Prusa MMU3, Anycubic ACE Pro, spool holder, or custom with any slot count) and put spools into slots by drag & drop or click. Every new printer automatically gets a spool holder, so printers without an AMS can take a spool right away. Loaded spools are shown separately from storage in their own column; when unloaded they automatically return to their home location. Spools have a real color value (palette or hex) in addition to the color name.
- **Catalog extensions** — print-status toggle + weight per model (real value from the slicer if the 3mf has already been sliced, otherwise a rough estimate from volume × material density), sorting by "last viewed", NEW badge for recently imported models, creators filter (from the 3MF designer metadata), automatic detection of exact file duplicates on import via content hash
- **Filament usage from the slicer** — reads the filament usage sliced in OrcaSlicer/Bambu Studio (`Metadata/slice_info.config`): real weight instead of an estimate, breakdown per build plate and filament (type, color, grams, meters) on the model detail page; "Rescan metadata" button retrieves the values afterward if an already-catalogued file was re-sliced in OrcaSlicer/Bambu Studio
- **Material cost estimate** — for models with real slicer filament usage, an additional estimated material cost on the detail page, computed from consumption and the prices of matching spools in the filament inventory
- **Print log** — in addition to the print-status toggle, a log of multiple print attempts per model (date, note, photo), independent of print status
- **Catalog backup** — export and re-import the entire catalog (database + settings) as a ZIP, with automatic backup of the existing database before every import
- **Extract archives directly** — archives you import or drag in individually (`.zip`, `.7z`, `.rar`, `.tar`, `.tar.gz`/`.tgz`, `.tar.bz2`, `.tar.xz`, `.tar.zst`) are extracted into a subfolder after a confirmation dialog, and the models inside are cataloged. You choose the target folder, what happens when the folder already exists, and whether the original archive is deleted; nothing is ever overwritten. For security reasons, programs, scripts, and shortcuts inside archives are never extracted. Password-protected and multi-part archives are detected but not extracted.
- **Model thumbnails** — grid view shows a real image per model (own upload, embedded 3MF thumbnail, or a snapshot automatically generated from the live 3D preview), plus an optional source link per model
- **Queue** — ordered, drag-and-drop sortable list ("print next") in its own sidebar section, automatically removed when marked as printed
- **Saved filters** — save frequently used combinations of folder/tag/creator/search/sort under a name and reapply them with a click
- **Cleanup suggestions** — manually triggered catalog scan finds orphaned file paths and stock duplicates, cleanup via a selection dialog
- **Model detail page** — full-screen view (double-click a model) with a large 3D preview, all metadata, and build-plate count for Bambu Studio/OrcaSlicer files; dedicated rotation controls (auto-rotation + 15° step buttons) in addition to free mouse dragging
- **Trash** — deleted models remain recoverable for 7 days instead of being removed immediately, own view with a count badge
- **Multi-select** — checkboxes in the catalog overview, "select all", action bar for queue/collection/print-status/tags/delete across multiple models at once
- **Keyboard shortcuts** — `/` focuses search, arrow keys navigate the grid spatially (Up/Down jumps to the next row), Space toggles the multi-select checkbox, Delete/Backspace opens the delete confirmation when a multi-selection is active
- **Rename** — rename models directly from the context menu; the file extension stays fixed
- **Automatic slicer detection** — scans known installation locations at startup (Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer, UltiMaker Cura on Linux/Windows) and adds matches automatically; manual addition for custom forks remains possible
- **Preferred view** — setting for whether the catalog and detail page default to the embedded file image or a rendered 3D view; missing snapshots are automatically re-rendered in the background as needed
- **Collections** — a third organizational mechanism alongside folders and tags: explicitly group multiple models into a project, with a manually definable order (drag & drop); created via multi-select
- **Custom app icon** — isometric 3D-printing layer cube in the app's real accent colors
- **Content Security Policy** active (no `csp: null`), source URL fields and "open in slicer" validated server-side

## Status

This project is under active development. The local catalog (import, parsing, tagging, search, 3D preview, multilingual UI, theming) and "open in slicer" are functional. The following is **not** yet implemented:

- **Cloud integration** (Google Drive etc.): existed previously but was removed again — too unstable/error-prone for everyday use. Will be cleanly redesigned at some point, see CHANGELOG
- **"Open in slicer" for manually added slicers on macOS**: opening works reliably for automatically detected slicers (Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer, UltiMaker Cura), since detection already finds the correct, directly executable program file inside the `.app` bundle. For manually added slicers, that same file needs to be selected in the file picker, not the `.app` bundle itself
- **STEP preview as a separate download**: all three platforms ship two package variants for this instead of a single one with STEP preview baked in — see [Downloads](#downloads-1).
- **Code signing**: the macOS `.dmg` and Windows `.msi` packages are unsigned (no Apple Developer or Windows code-signing certificate) — Gatekeeper/SmartScreen will warn accordingly on first launch

## Planned

- **Live printer connection** (e.g. Bambu Lab via MQTT on the local network): take over AMS slot contents and remaining amounts automatically
- **Automatic deduction** of filament usage after a print
- **“Is there enough filament?”** – compare a model's requirement with available or loaded spools
- **Slot history** – which spool was in which slot and when

## Tech Stack

- **Framework:** Tauri 2 (Rust backend, WebView frontend)
- **Frontend:** React 19, TypeScript, Tailwind CSS, Vite
- **3D rendering:** three.js
- **Database:** SQLite (`rusqlite`)

## Development

Prerequisites: Node.js, the Rust toolchain (`cargo`), and the [Tauri system dependencies](https://tauri.app/start/prerequisites/) for your operating system. The default STEP-preview build additionally requires Open CASCADE 7.8 or 7.9 as dynamic system libraries, including development files.

```bash
npm install
npm run tauri dev
```

Without OCCT, or to build a variant without STEP preview:

```bash
npm run tauri dev -- -- --no-default-features
# or: npm run tauri build -- -- --no-default-features
```

Backend tests:

```bash
cd src-tauri
cargo test
```

Production build (type checking + Vite build):

```bash
npm run build
```

## Project Structure

```
src/               React frontend (components, i18n, hooks, types)
src-tauri/         Rust backend (Tauri commands, DB, 3MF/STL/OBJ/STEP parsers, tagging)
docs/              Additional documentation
```

## License

The application code is licensed under MIT — see [LICENSE](LICENSE). The optional STEP preview dynamically links Open CASCADE under LGPL-2.1 with the Open CASCADE exception. See [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) for details, full license texts, and source information.
