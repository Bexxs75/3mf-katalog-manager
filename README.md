# 3MF Katalog Manager

Plattformunabhängige Desktop-Anwendung zur Katalogisierung und Verwaltung von 3MF- und STL-Dateien für den 3D-Druck. Gebaut mit [Tauri](https://tauri.app/) (Rust-Backend) und React/TypeScript/Tailwind.

## Funktionen

- **3MF- und STL-Parsing** — beide Formate gleichwertig unterstützt: OPC-Container-Entpackung inkl. eingebettetem Thumbnail bei 3MF, ASCII- und Binär-STL-Parsing
- **Automatische Metadaten-Extraktion** — Abmessungen, Volumen, Objektanzahl, Material (sofern in der 3MF vorhanden)
- **Automatische Hashtag-Generierung** — Vorschläge aus Dateiname, Geometrie-Merkmalen und Slicer-Profildaten, vom Nutzer editierbar
- **3D-Live-Vorschau** — three.js-Rendering direkt aus der Mesh-Geometrie, wenn kein eingebettetes Thumbnail vorhanden ist
- **Tag- und Ordnerverwaltung**, Suche und Filterung
- **Import** einzelner Dateien oder ganzer Ordner (inkl. Unterordner) per Dialog oder Drag & Drop, über ein gemeinsames Dropdown-Menü in der Kopfzeile
- **Komfort-Ansicht** — alternative, deutlich lesbarere Oberfläche (größere Schrift, Grafiken und Bedienelemente) neben der bestehenden kompakten Ansicht, umschaltbar im Einstellungen-Panel; Favorit-Kennzeichnung je Modell in beiden Ansichten
- **In Slicer öffnen** — beliebig viele selbst hinterlegte Slicer-Programme (herstellerunabhängig) direkt aus dem Katalog heraus starten (Windows/Linux)
- **Mehrsprachige Oberfläche** — Deutsch, Englisch, Spanisch, Französisch, umschaltbar zur Laufzeit
- **Hell-/Dunkel-Theme** mit System-Erkennung und manueller Auswahl, persistiert lokal
- **Filament-Lager** — eigenständige Verwaltung deiner Filamentspulen (Material, Hersteller, Farbe, Lagerort, Durchmesser, Ursprungs-/Restgewicht, Preis) mit Autocomplete für Material/Hersteller/Lagerort, unabhängig vom Modell-Katalog; umschaltbar zwischen Karten-Dashboard (Bestandsbalken, Statusfarbe) und sortierbarer Inventarliste, inkl. Statistik-Leiste und Status-Filter; beim Neuanlegen lassen sich mehrere identische Spulen auf einmal erfassen (jede mit eigenem, unabhängig verfolgtem Restbestand)
- **Katalog-Erweiterungen** — Druckstatus-Toggle + Gewicht pro Modell (echter Wert aus dem Slicer, falls die 3mf bereits gesliced wurde, sonst grobe Schätzung aus Volumen × Materialdichte), Sortierung nach "Zuletzt angesehen", NEU-Badge für kürzlich importierte Modelle, Creators-Filter (aus 3MF-Designer-Metadatum), automatische Erkennung exakter Datei-Duplikate beim Import per Inhalts-Hash
- **Filamentverbrauch aus dem Slicer** — liest den in OrcaSlicer/Bambu Studio gesliceten Filamentverbrauch (`Metadata/slice_info.config`) mit aus: reales Gewicht statt Schätzung, Aufschlüsselung pro Druckplatte und Filament (Typ, Farbe, Gramm, Meter) auf der Modell-Detailseite; Button "Metadaten neu einlesen" holt die Werte nachträglich, wenn eine bereits katalogisierte Datei in OrcaSlicer/Bambu Studio nachgesliced wurde
- **Materialkosten-Schätzung** — bei Modellen mit echtem Slicer-Filamentverbrauch zusätzlich eine geschätzte Materialkosten-Summe auf der Detailseite, berechnet aus Verbrauch und den Preisen passender Spulen im Filament-Lager
- **Druckprotokoll** — zusätzlich zum Druckstatus-Toggle ein Protokoll mehrerer Druckversuche pro Modell (Datum, Notiz, Foto), unabhängig vom Druckstatus
- **Katalog-Backup** — kompletter Katalog (Datenbank + Einstellungen) als ZIP exportierbar und wieder importierbar, mit automatischer Sicherung der bestehenden Datenbank vor jedem Import
- **Modell-Thumbnails** — Raster-Ansicht zeigt ein echtes Bild pro Modell (eigenes Upload, eingebettetes 3MF-Thumbnail oder automatisch aus der 3D-Live-Vorschau erzeugter Snapshot), zusätzlich pro Modell eine Quelle als Link hinterlegbar
- **Warteschlange** — geordnete, per Drag & Drop sortierbare Liste ("als Nächstes drucken") in eigener Sidebar-Sektion, automatisches Entfernen beim Markieren als gedruckt
- **Gespeicherte Filter** — häufig genutzte Kombinationen aus Ordner/Tag/Creator/Suche/Sortierung unter einem Namen speichern und per Klick wieder anwenden
- **Aufräum-Vorschläge** — manuell auslösbarer Katalog-Scan findet verwaiste Dateipfade und Bestands-Duplikate, Bereinigung per Auswahl-Dialog
- **Modell-Detailseite** — vollflächige Ansicht (Doppelklick auf ein Modell) mit großer 3D-Vorschau, allen Metadaten und Druckplatten-Anzahl bei Bambu-Studio-/OrcaSlicer-Dateien; eigene Dreh-Steuerelemente (Auto-Rotation + 15°-Schritt-Buttons) zusätzlich zum freien Maus-Ziehen
- **Papierkorb** — gelöschte Modelle bleiben 7 Tage wiederherstellbar statt sofort entfernt zu werden, eigene Ansicht mit Mengen-Badge
- **Mehrfachauswahl** — Checkboxen in der Katalogübersicht, "Alle auswählen", Aktionsleiste für Warteschlange/Druckstatus/Löschen über mehrere Modelle gleichzeitig
- **Automatische Slicer-Erkennung** — durchsucht beim Start bekannte Installationsorte (Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer, UltiMaker Cura auf Linux/Windows) und ergänzt Treffer automatisch; manuelles Hinzufügen für Custom-Forks bleibt möglich
- **Bevorzugte Ansicht** — Einstellung, ob Katalog und Detailseite standardmäßig das eingebettete Datei-Bild oder eine gerenderte 3D-Ansicht zeigen; fehlende Schnappschüsse werden bei Bedarf automatisch im Hintergrund nachgerendert
- **Sammlungen** — dritter Organisationsmechanismus neben Ordnern und Tags: mehrere Modelle explizit zu einem Projekt zusammenfassen, mit manuell festlegbarer Reihenfolge (Drag & Drop); Erstellung über Mehrfachauswahl oder per "Ordner als Sammlung importieren"
- **Eigenes App-Icon** — isometrischer 3D-Druck-Layer-Würfel in den echten App-Akzentfarben
- **Content-Security-Policy** aktiv (kein `csp: null`), Quell-URL-Felder und "In Slicer öffnen" serverseitig validiert

## Status

Dieses Projekt befindet sich in aktiver Entwicklung. Der lokale Katalog (Import, Parsing, Tagging, Suche, 3D-Vorschau, Mehrsprachigkeit, Theming) und "In Slicer öffnen" sind funktionsfähig. Folgendes ist noch **nicht** umgesetzt:

- **Cloud-Anbindung** (Google Drive u. a.): war vorhanden, wurde aber wieder entfernt — zu instabil/fehleranfällig für den Alltagsgebrauch. Wird bei Gelegenheit sauber neu konzipiert, siehe CHANGELOG
- **macOS**: bisher nur unter Linux entwickelt und getestet; "In Slicer öffnen" unterstützt macOS gezielt nicht (`.app`-Bundles brauchen einen eigenen Start-Mechanismus)
- **Plattformübergreifende Release-Builds**: Windows-`.msi`-Build ist manuell auf einer Windows-11-VM verifiziert (Build + Installation + Icon-Prüfung), aber nicht Teil einer automatisierten Pipeline; macOS-Paket (`.dmg`) sowie Code-Signing für beide Plattformen stehen noch aus
- **CI/CD-Pipeline**: noch nicht eingerichtet

## Geplant

Derzeit keine offenen hochpriorisierten Punkte im Backlog.

## Tech-Stack

- **Framework:** Tauri 2 (Rust-Backend, WebView-Frontend)
- **Frontend:** React 19, TypeScript, Tailwind CSS, Vite
- **3D-Rendering:** three.js
- **Datenbank:** SQLite (`rusqlite`)

## Entwicklung

Voraussetzungen: Node.js, Rust-Toolchain (`cargo`), sowie die [Tauri-Systemabhängigkeiten](https://tauri.app/start/prerequisites/) für dein Betriebssystem.

```bash
npm install
npm run tauri dev
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
src-tauri/         Rust-Backend (Tauri-Commands, DB, Parser für 3MF/STL, Tagging)
docs/              Zusätzliche Dokumentation
```

## Lizenz

Noch nicht festgelegt.

---

# 3MF Katalog Manager (English)

Cross-platform desktop application for cataloging and managing 3MF and STL files for 3D printing. Built with [Tauri](https://tauri.app/) (Rust backend) and React/TypeScript/Tailwind.

## Features

- **3MF and STL parsing** — both formats supported equally: OPC container extraction incl. embedded thumbnail for 3MF, ASCII and binary STL parsing
- **Automatic metadata extraction** — dimensions, volume, object count, material (if present in the 3MF)
- **Automatic hashtag generation** — suggestions from filename, geometry features, and slicer profile data, editable by the user
- **Live 3D preview** — three.js rendering directly from the mesh geometry when no embedded thumbnail is available
- **Tag and folder management**, search and filtering
- **Import** of individual files or entire folders (including subfolders) via dialog or drag & drop, through a shared dropdown menu in the header
- **Comfort view** — alternative, significantly more readable interface (larger text, graphics, and controls) alongside the existing compact view, toggleable in the settings panel; favorite marking per model in both views
- **Open in slicer** — launch any number of self-configured slicer programs (vendor-agnostic) directly from the catalog (Windows/Linux)
- **Multilingual UI** — German, English, Spanish, French, switchable at runtime
- **Light/dark theme** with system detection and manual selection, persisted locally
- **Filament inventory** — standalone management of your filament spools (material, manufacturer, color, storage location, diameter, original/remaining weight, price) with autocomplete for material/manufacturer/location, independent of the model catalog; toggleable between a card dashboard (stock bar, status color) and a sortable inventory list, including a stats bar and status filters; when adding new spools, several identical ones can be created at once (each with its own independently tracked remaining stock)
- **Catalog extensions** — print-status toggle + weight per model (real value from the slicer if the 3mf has already been sliced, otherwise a rough estimate from volume × material density), sorting by "last viewed", NEW badge for recently imported models, creators filter (from the 3MF designer metadata), automatic detection of exact file duplicates on import via content hash
- **Filament usage from the slicer** — reads the filament usage sliced in OrcaSlicer/Bambu Studio (`Metadata/slice_info.config`): real weight instead of an estimate, breakdown per build plate and filament (type, color, grams, meters) on the model detail page; "Rescan metadata" button retrieves the values afterward if an already-catalogued file was re-sliced in OrcaSlicer/Bambu Studio
- **Material cost estimate** — for models with real slicer filament usage, an additional estimated material cost on the detail page, computed from consumption and the prices of matching spools in the filament inventory
- **Print log** — in addition to the print-status toggle, a log of multiple print attempts per model (date, note, photo), independent of print status
- **Catalog backup** — export and re-import the entire catalog (database + settings) as a ZIP, with automatic backup of the existing database before every import
- **Model thumbnails** — grid view shows a real image per model (own upload, embedded 3MF thumbnail, or a snapshot automatically generated from the live 3D preview), plus an optional source link per model
- **Queue** — ordered, drag-and-drop sortable list ("print next") in its own sidebar section, automatically removed when marked as printed
- **Saved filters** — save frequently used combinations of folder/tag/creator/search/sort under a name and reapply them with a click
- **Cleanup suggestions** — manually triggered catalog scan finds orphaned file paths and stock duplicates, cleanup via a selection dialog
- **Model detail page** — full-screen view (double-click a model) with a large 3D preview, all metadata, and build-plate count for Bambu Studio/OrcaSlicer files; dedicated rotation controls (auto-rotation + 15° step buttons) in addition to free mouse dragging
- **Trash** — deleted models remain recoverable for 7 days instead of being removed immediately, own view with a count badge
- **Multi-select** — checkboxes in the catalog overview, "select all", action bar for queue/print-status/delete across multiple models at once
- **Automatic slicer detection** — scans known installation locations at startup (Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer, UltiMaker Cura on Linux/Windows) and adds matches automatically; manual addition for custom forks remains possible
- **Preferred view** — setting for whether the catalog and detail page default to the embedded file image or a rendered 3D view; missing snapshots are automatically re-rendered in the background as needed
- **Collections** — a third organizational mechanism alongside folders and tags: explicitly group multiple models into a project, with a manually definable order (drag & drop); created via multi-select or via "import folder as collection"
- **Custom app icon** — isometric 3D-printing layer cube in the app's real accent colors
- **Content Security Policy** active (no `csp: null`), source URL fields and "open in slicer" validated server-side

## Status

This project is under active development. The local catalog (import, parsing, tagging, search, 3D preview, multilingual UI, theming) and "open in slicer" are functional. The following is **not** yet implemented:

- **Cloud integration** (Google Drive etc.): existed previously but was removed again — too unstable/error-prone for everyday use. Will be cleanly redesigned at some point, see CHANGELOG
- **macOS**: developed and tested on Linux only so far; "open in slicer" specifically does not support macOS (`.app` bundles need a different launch mechanism)
- **Cross-platform release builds**: the Windows `.msi` build is manually verified on a Windows 11 VM (build + installation + icon check), but not part of an automated pipeline; a macOS package (`.dmg`) and code signing for both platforms are still outstanding
- **CI/CD pipeline**: not yet set up

## Planned

No open high-priority items in the backlog at the moment.

## Tech Stack

- **Framework:** Tauri 2 (Rust backend, WebView frontend)
- **Frontend:** React 19, TypeScript, Tailwind CSS, Vite
- **3D rendering:** three.js
- **Database:** SQLite (`rusqlite`)

## Development

Prerequisites: Node.js, the Rust toolchain (`cargo`), and the [Tauri system dependencies](https://tauri.app/start/prerequisites/) for your operating system.

```bash
npm install
npm run tauri dev
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
src-tauri/         Rust backend (Tauri commands, DB, 3MF/STL parsers, tagging)
docs/              Additional documentation
```

## License

Not yet decided.
