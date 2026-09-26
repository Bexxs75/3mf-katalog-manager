# 3MF Katalog Manager

🇩🇪 **Deutsch:** [Deutsche Version weiter unten](#3mf-katalog-manager-deutsch)

[![Latest release](https://img.shields.io/github/v/release/Bexxs75/3mf-katalog-manager?label=release&color=ff7a5c)](https://github.com/Bexxs75/3mf-katalog-manager/releases/latest) [![CI](https://img.shields.io/github/actions/workflow/status/Bexxs75/3mf-katalog-manager/ci-checks.yml?branch=master&label=CI)](https://github.com/Bexxs75/3mf-katalog-manager/actions/workflows/ci-checks.yml) [![Downloads](https://img.shields.io/github/downloads/Bexxs75/3mf-katalog-manager/total?color=4cc38a)](https://github.com/Bexxs75/3mf-katalog-manager/releases) [![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-555)](#download) [![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE) [![Discord](https://img.shields.io/badge/Discord-join-5865F2?logo=discord&logoColor=white)](https://discord.gg/abfVNfFqu3)

**Your 3MF, STL and STEP collection keeps growing? The 3MF Katalog Manager catalogs the folders you already have, finds any model in seconds, shows it in 3D and connects your files with filament, print queue and print history. Everything stays on your computer: no account, no cloud.**

![Demo: search the catalog, open a model with slicer data, open it in the slicer, filament storage with AMS slots](docs/assets/demo.gif)

- **100 % local** — no sign-up, no cloud, no tracking. Your files stay where they are.
- **Your existing folders, searchable** — import folders, get automatic tags, real 3D previews and slicer data (weight, filament per plate).
- **More than a file browser** — filament and resin storage with AMS slots, print queue, print log and an optional connection to Klipper printers.

Free and open source (MIT) · Windows, macOS, Linux · English, German, Spanish, French · [Website](https://3mfkatalog.de/en/) · [User guide](docs/benutzerhandbuch/BENUTZERHANDBUCH.md)

## Download

[![Download for Windows](https://img.shields.io/badge/Download-Windows-ff7a5c?style=for-the-badge)](https://3mfkatalog.de/en/#download) [![Download for macOS](https://img.shields.io/badge/Download-macOS-ff7a5c?style=for-the-badge)](https://3mfkatalog.de/en/#download) [![Download for Linux](https://img.shields.io/badge/Download-Linux%20AppImage-ff7a5c?style=for-the-badge)](https://3mfkatalog.de/en/#download)

The buttons open the download section of the website, which always offers the newest version. All files and checksums: [latest release](https://github.com/Bexxs75/3mf-katalog-manager/releases/latest).

**Which file do I need?** Windows: the `.msi`. macOS: the `.dmg` (works on Intel and Apple Silicon). Linux: the `.AppImage`. Files **with** `-step` in the name also show a 3D preview for STEP files (`.stp`/`.step`) and are larger; if you only use 3MF, STL or OBJ, take the file **without** `-step`. Both variants are otherwise identical.

## Is it safe to install?

Windows and macOS show a warning on the first start because the packages are **not code-signed**: the certificates for that cost money every year, which a free hobby project can't cover yet. The warning says "unknown publisher", not that anything harmful was found.

What you can check yourself:

- **Open source:** every release is built from the public source code by [GitHub Actions workflows](https://github.com/Bexxs75/3mf-katalog-manager/tree/master/.github/workflows). The source of each version is the matching tag, e.g. [v0.14.0](https://github.com/Bexxs75/3mf-katalog-manager/tree/v0.14.0).
- **Checksums:** every release contains `SHA256SUMS.txt`. Compare it with the hash of your download:
  - Windows (PowerShell): `Get-FileHash .\3MF.Katalog.Manager_…msi -Algorithm SHA256`
  - macOS: `shasum -a 256 3MF.Katalog.Manager_…dmg`
  - Linux: `sha256sum -c SHA256SUMS.txt --ignore-missing`
- **Starting anyway:** Windows SmartScreen: "More info" → "Run anyway". macOS: right-click the app → "Open"; on macOS 15 or newer: System Settings → Privacy & Security → "Open Anyway".
- **Local only:** the app works offline. It only goes online to check GitHub for a newer version and, if you switch it on, to talk to printers on your home network.

## What it does

- **Find and organize models** — import files or whole folders, automatic tags, search, folders, collections, favorites, duplicates detection, trash.
- **Understand files and metadata** — 3D preview for 3MF, STL, OBJ (and STEP), dimensions, volume, build plates, filament usage and weight from OrcaSlicer/Bambu Studio.
- **Manage filament and resin** — spools and resin bottles with stock, location and price, printers with AMS/MMU slots, "Is there enough filament?" per model.
- **Plan and document prints** — print queue, print status, print log with photos, estimated material cost, open in your slicer with one click.
- **Connect printers** — optional, read-only: Klipper/Moonraker printers report the filament used, you confirm and it is deducted. OctoPrint and Bambu Lab are planned.
- **Safe and local** — catalog backup as ZIP, archives extracted safely, Content Security Policy, no cloud.

<details>
<summary><b>All features in detail</b></summary>

- **3MF, STL, OBJ, and STEP parsing** — 3MF (OPC container extraction incl. embedded thumbnail), ASCII/binary STL, and OBJ each have a 3D preview; STEP files (`.stp`/`.step`) are read through Open CASCADE and provide a 3D preview, dimensions, volume, and body count. Every platform (Linux/Windows/macOS) ships two downloads: one **with** STEP preview and one smaller one **without** (cataloging only, no 3D preview for STEP) — see [Downloads](#downloads).
- **Automatic metadata extraction** — dimensions, volume, object count, material (if present in the 3MF)
- **Automatic hashtag generation** — suggestions from filename, geometry features, and slicer profile data, editable by the user
- **Live 3D preview** — three.js rendering directly from the mesh geometry when no embedded thumbnail is available
- **Tag and folder management**, search and filtering
- **Import** of individual files or entire folders (including subfolders) via dialog or drag & drop, through a shared dropdown menu in the header
- **Comfort view** — alternative, significantly more readable interface (larger text, graphics, and controls) alongside the existing compact view, toggleable in the settings panel; favorite marking per model in both views
- **Open in slicer** — launch any number of self-configured slicer programs (vendor-agnostic) directly from the catalog
- **Multilingual UI** — German, English, Spanish, French, switchable at runtime
- **Light/dark theme** with system detection and manual selection, persisted locally
- **Filament inventory** — standalone management of your filament spools (material, manufacturer, color, storage location, diameter, original/remaining weight, price) with autocomplete for material/manufacturer/location, independent of the model catalog; toggleable between a card dashboard (stock bar, status color) and a sortable inventory list, including a stats bar and status filters; when adding new spools, several identical ones can be created at once (each with its own independently tracked remaining stock); "＋ Restock" on the card or "＋" in the list row adds 1 to 20 new, full spools with the same details; double-clicking a card or row opens the edit form; remaining weights to 0.1 g; spool image by click or drag & drop (PNG, JPG or WebP up to 5 MB); spools without an image show a spool icon in their color in the list
- **Resin in the stock** — the "Filament | Resin" switch at the top of the filament inventory changes to your resin bottles: amounts in ml, separate stats ("Total bottles"), suggestions for resin material and manufacturer, "− Use" to deduct the milliliters you used, and restocking just like spools; new entries get the type of the selected area. Resin never goes into a filament slot, only into the resin vat of a resin printer, and is left out of "Is there enough filament?" and the material cost estimate.
- **Printers & AMS slots** — add printers and their multi-material units (Bambu AMS, AMS lite, AMS HT, Creality CFS, Prusa MMU3, Anycubic ACE Pro, spool holder, or custom with any slot count) and put spools into slots by drag & drop or click. Every new printer automatically gets a spool holder, so printers without an AMS can take a spool right away. Loaded spools are shown separately from storage in their own column; when unloaded they automatically return to their home location. Spools have a real color value (palette or hex) in addition to the color name.
- **Resin printers** — when adding a printer you choose "Filament" or "Resin". A resin printer gets a fixed resin vat for one bottle instead of the spool holder; you insert the bottle by dragging or via the vat's menu, see its color and remaining ml, and deduct usage with "− Use".
- **Is there enough filament?** — for sliced 3MF files, the detail page compares the filament requirement with your spools (material and similar color) and shows whether it is enough, enough only with a spool change, or how much is missing – including the matching spool and whether it is loaded in a printer. The print queue shows the status per entry and takes the combined requirement into account.
- **Tools** — a sidebar section with the print queue, "Recently viewed", "Recently added", "Favorites" (all models marked with a heart), "Duplicates" and cleanup suggestions, each with a count; the views filter the catalog and combine with folders, tags and search.
- **Printer connection** — optional (off by default): Klipper/Moonraker printers on your home network report the filament used by finished and aborted prints; after you confirm, it is deducted from the spool, optionally with a print log entry. Read-only, only addresses you entered yourself on your home network. Tested with a Sovol SV08, an Anycubic Kobra S1 (Rinkhals) and a Qidi Smart 3; results for more printers are collected on the [test page](https://3mfkatalog.de/en/printer-test.html).
- **Catalog extensions** — print-status toggle + weight per model (real value from the slicer if the 3mf has already been sliced, otherwise a rough estimate from volume × material density), sorting by "last viewed", NEW badge for recently imported models, creators filter (from the 3MF designer metadata), automatic detection of exact file duplicates on import via content hash
- **Filament usage from the slicer** — reads the filament usage sliced in OrcaSlicer/Bambu Studio (`Metadata/slice_info.config`): real weight instead of an estimate, breakdown per build plate and filament (type, color, grams, meters) on the model detail page; "Rescan metadata" button retrieves the values afterward if an already-catalogued file was re-sliced in OrcaSlicer/Bambu Studio
- **Material cost estimate** — for models with real slicer filament usage, an additional estimated material cost on the detail page, computed from consumption and the prices of matching spools in the filament inventory
- **Print log** — in addition to the print-status toggle, a log of multiple print attempts per model (date, note, photo), independent of print status
- **Catalog backup** — export and re-import the entire catalog (database + settings) as a ZIP, with automatic backup of the existing database before every import
- **Extract archives directly** — archives you import or drag in individually (`.zip`, `.7z`, `.rar`, `.tar`, `.tar.gz`/`.tgz`, `.tar.bz2`, `.tar.xz`, `.tar.zst`) are extracted into a subfolder after a confirmation dialog, and the models inside are cataloged. You choose the target folder, what happens when the folder already exists, and whether the original archive is deleted; nothing is ever overwritten. For security reasons, programs, scripts, and shortcuts inside archives are never extracted. Password-protected and multi-part archives are detected but not extracted.
- **Model thumbnails** — grid view shows a real image per model (own upload, embedded 3MF thumbnail, or a snapshot automatically generated from the live 3D preview), plus an optional source link per model
- **Queue** — ordered, drag-and-drop sortable list ("print next") in its own sidebar section, automatically removed when marked as printed
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

</details>

## Screenshots

| Catalog with previews | Model with slicer data |
|---|---|
| ![Catalog in grid view](docs/benutzerhandbuch/bilder/en/02-katalog-grid.png) | ![Model detail page](docs/benutzerhandbuch/bilder/en/04-modell-detailseite.png) |
| **Printers and AMS slots** | **Confirming prints from a Klipper printer** |
| ![Printers with AMS slots in the filament storage](docs/benutzerhandbuch/bilder/en/22-drucker-ams.png) | !["New prints" dialog](docs/benutzerhandbuch/bilder/en/27-neue-drucke.png) |

## Help and documentation

- [User guide](docs/benutzerhandbuch/BENUTZERHANDBUCH.md) (DE + EN), step by step with screenshots
- [Website](https://3mfkatalog.de/en/) with FAQ
- [Discord](https://discord.gg/abfVNfFqu3) for questions and ideas
- Bugs: [open an issue](https://github.com/Bexxs75/3mf-katalog-manager/issues/new/choose) or ask on Discord

## Contributing

Help is welcome, and not only with code: test your printer on the [test page](https://3mfkatalog.de/en/printer-test.html), check a translation, try the installation on your system or report a bug. See [CONTRIBUTING.md](CONTRIBUTING.md), issues labeled [good first issue](https://github.com/Bexxs75/3mf-katalog-manager/labels/good%20first%20issue) and the [translation guide](docs/TRANSLATING.md).

## Status and roadmap

This project is under active development. The local catalog (import, parsing, tagging, search, 3D preview, multilingual UI, theming) and "open in slicer" are functional. The following is **not** yet implemented:

- **Cloud integration** (Google Drive etc.): existed previously but was removed again — too unstable/error-prone for everyday use. Will be cleanly redesigned at some point, see CHANGELOG
- **"Open in slicer" for manually added slicers on macOS**: opening works reliably for automatically detected slicers (Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer, UltiMaker Cura), since detection already finds the correct, directly executable program file inside the `.app` bundle. For manually added slicers, that same file needs to be selected in the file picker, not the `.app` bundle itself
- **STEP preview as a separate download**: all three platforms ship two package variants for this instead of a single one with STEP preview baked in — see [Downloads](#downloads).
- **Code signing**: the macOS `.dmg` and Windows `.msi` packages are unsigned (no Apple Developer or Windows code-signing certificate) — Gatekeeper/SmartScreen will warn accordingly on first launch

The public [roadmap](https://github.com/users/Bexxs75/projects/1/views/1?groupedBy%5BcolumnId%5D=416999698) shows what comes next.

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

---

# 3MF Katalog Manager (Deutsch)

[![Neueste Version](https://img.shields.io/github/v/release/Bexxs75/3mf-katalog-manager?label=Version&color=ff7a5c)](https://github.com/Bexxs75/3mf-katalog-manager/releases/latest) [![CI](https://img.shields.io/github/actions/workflow/status/Bexxs75/3mf-katalog-manager/ci-checks.yml?branch=master&label=CI)](https://github.com/Bexxs75/3mf-katalog-manager/actions/workflows/ci-checks.yml) [![Downloads](https://img.shields.io/github/downloads/Bexxs75/3mf-katalog-manager/total?color=4cc38a)](https://github.com/Bexxs75/3mf-katalog-manager/releases) [![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-555)](#download) [![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE) [![Discord](https://img.shields.io/badge/Discord-join-5865F2?logo=discord&logoColor=white)](https://discord.gg/abfVNfFqu3)

**Deine 3MF-, STL- und STEP-Sammlung wächst? Der 3MF Katalog Manager katalogisiert deine vorhandenen Ordner, findet jedes Modell in Sekunden, zeigt es in 3D und verbindet deine Dateien mit Filament, Druckwarteschlange und Druckprotokoll. Alles bleibt auf deinem Rechner: kein Konto, keine Cloud.**

![Demo: Katalog durchsuchen, Modell mit Slicer-Daten öffnen, im Slicer öffnen, Filamentlager mit AMS-Fächern](docs/assets/demo.gif)

- **100 % lokal** — keine Anmeldung, keine Cloud, kein Tracking. Deine Dateien bleiben, wo sie sind.
- **Deine vorhandenen Ordner, durchsuchbar** — Ordner importieren, automatische Tags, echte 3D-Vorschau und Slicer-Daten (Gewicht, Filament je Platte).
- **Mehr als ein Dateibrowser** — Filament- und Resin-Lager mit AMS-Fächern, Druckwarteschlange, Druckprotokoll und optionale Anbindung von Klipper-Druckern.

Kostenlos und Open Source (MIT) · Windows, macOS, Linux · Deutsch, Englisch, Spanisch, Französisch · [Webseite](https://3mfkatalog.de) · [Benutzerhandbuch](docs/benutzerhandbuch/BENUTZERHANDBUCH.md)

## Download

[![Download für Windows](https://img.shields.io/badge/Download-Windows-ff7a5c?style=for-the-badge)](https://3mfkatalog.de/#download) [![Download für macOS](https://img.shields.io/badge/Download-macOS-ff7a5c?style=for-the-badge)](https://3mfkatalog.de/#download) [![Download für Linux](https://img.shields.io/badge/Download-Linux%20AppImage-ff7a5c?style=for-the-badge)](https://3mfkatalog.de/#download)

Die Knöpfe öffnen den Download-Bereich der Webseite, dort gibt es immer die neueste Version. Alle Dateien und Prüfsummen: [neueste Version](https://github.com/Bexxs75/3mf-katalog-manager/releases/latest).

**Welche Datei brauche ich?** Windows: die `.msi`. macOS: die `.dmg` (für Intel und Apple Silicon). Linux: das `.AppImage`. Dateien **mit** `-step` im Namen zeigen zusätzlich eine 3D-Vorschau für STEP-Dateien (`.stp`/`.step`) und sind größer; wer nur 3MF, STL oder OBJ nutzt, nimmt die Datei **ohne** `-step`. Sonst sind beide Varianten gleich.

## Ist die Installation sicher?

Windows und macOS zeigen beim ersten Start eine Warnung, weil die Pakete **nicht signiert** sind: Die Zertifikate dafür kosten jedes Jahr Geld, das kann ein kostenloses Hobbyprojekt noch nicht tragen. Die Warnung bedeutet „unbekannter Herausgeber“, nicht, dass etwas Schädliches gefunden wurde.

Was du selbst prüfen kannst:

- **Open Source:** Jede Version wird aus dem öffentlichen Quellcode von [GitHub-Actions-Workflows](https://github.com/Bexxs75/3mf-katalog-manager/tree/master/.github/workflows) gebaut. Der Quellstand jeder Version ist das passende Tag, z. B. [v0.14.0](https://github.com/Bexxs75/3mf-katalog-manager/tree/v0.14.0).
- **Prüfsummen:** Jede Version enthält `SHA256SUMS.txt`. Vergleiche sie mit dem Hash deines Downloads:
  - Windows (PowerShell): `Get-FileHash .\3MF.Katalog.Manager_…msi -Algorithm SHA256`
  - macOS: `shasum -a 256 3MF.Katalog.Manager_…dmg`
  - Linux: `sha256sum -c SHA256SUMS.txt --ignore-missing`
- **Trotzdem starten:** Windows-SmartScreen: „Weitere Informationen“ → „Trotzdem ausführen“. macOS: Rechtsklick auf die App → „Öffnen“; ab macOS 15: Systemeinstellungen → Datenschutz & Sicherheit → „Trotzdem öffnen“.
- **Nur lokal:** Die App arbeitet offline. Online geht sie nur, um auf GitHub nach einer neuen Version zu sehen, und, wenn du es einschaltest, um mit Druckern im Heimnetz zu sprechen.

## Was die App kann

- **Modelle finden und ordnen** — Dateien oder ganze Ordner importieren, automatische Tags, Suche, Ordner, Sammlungen, Favoriten, Duplikaterkennung, Papierkorb.
- **Dateien und Metadaten verstehen** — 3D-Vorschau für 3MF, STL, OBJ (und STEP), Maße, Volumen, Druckplatten, Filamentverbrauch und Gewicht aus OrcaSlicer/Bambu Studio.
- **Filament und Resin verwalten** — Spulen und Resin-Flaschen mit Bestand, Lagerort und Preis, Drucker mit AMS-/MMU-Fächern, „Reicht das Filament?“ je Modell.
- **Drucke planen und dokumentieren** — Warteschlange, Druckstatus, Druckprotokoll mit Fotos, geschätzte Materialkosten, mit einem Klick im Slicer öffnen.
- **Drucker anbinden** — optional und nur lesend: Klipper/Moonraker-Drucker melden den Filamentverbrauch, du bestätigst, dann wird abgebucht. OctoPrint und Bambu Lab sind geplant.
- **Sicher und lokal** — Katalog-Sicherung als ZIP, Archive werden sicher entpackt, Content Security Policy, keine Cloud.

<details>
<summary><b>Alle Funktionen im Detail</b></summary>

- **3MF-, STL-, OBJ- und STEP-Parsing** — 3MF (OPC-Container-Entpackung inkl. eingebettetem Thumbnail), ASCII-/Binär-STL und OBJ jeweils mit 3D-Vorschau; STEP-Dateien (`.stp`/`.step`) werden über Open CASCADE gelesen und liefern 3D-Vorschau, Abmessungen, Volumen und Körperzahl. Für jede Plattform (Linux/Windows/macOS) gibt es zwei Downloads: eine Variante **mit** STEP-Vorschau und eine kleinere Variante **ohne** (nur Katalogisierung ohne 3D-Vorschau für STEP) — siehe [Downloads](#downloads-1).
- **Automatische Metadaten-Extraktion** — Abmessungen, Volumen, Objektanzahl, Material (sofern in der 3MF vorhanden)
- **Automatische Hashtag-Generierung** — Vorschläge aus Dateiname, Geometrie-Merkmalen und Slicer-Profildaten, vom Nutzer editierbar
- **3D-Live-Vorschau** — three.js-Rendering direkt aus der Mesh-Geometrie, wenn kein eingebettetes Thumbnail vorhanden ist
- **Tag- und Ordnerverwaltung**, Suche und Filterung
- **Import** einzelner Dateien oder ganzer Ordner (inkl. Unterordner) per Dialog oder Drag & Drop, über ein gemeinsames Dropdown-Menü in der Kopfzeile
- **Komfort-Ansicht** — alternative, deutlich lesbarere Oberfläche (größere Schrift, Grafiken und Bedienelemente) neben der bestehenden kompakten Ansicht, umschaltbar im Einstellungen-Panel; Favorit-Kennzeichnung je Modell in beiden Ansichten
- **In Slicer öffnen** — beliebig viele selbst hinterlegte Slicer-Programme (herstellerunabhängig) direkt aus dem Katalog heraus starten
- **Mehrsprachige Oberfläche** — Deutsch, Englisch, Spanisch, Französisch, umschaltbar zur Laufzeit
- **Hell-/Dunkel-Theme** mit System-Erkennung und manueller Auswahl, persistiert lokal
- **Filament-Lager** — eigenständige Verwaltung deiner Filamentspulen (Material, Hersteller, Farbe, Lagerort, Durchmesser, Ursprungs-/Restgewicht, Preis) mit Autocomplete für Material/Hersteller/Lagerort, unabhängig vom Modell-Katalog; umschaltbar zwischen Karten-Dashboard (Bestandsbalken, Statusfarbe) und sortierbarer Inventarliste, inkl. Statistik-Leiste und Status-Filter; beim Neuanlegen lassen sich mehrere identische Spulen auf einmal erfassen (jede mit eigenem, unabhängig verfolgtem Restbestand); „＋ Nachkaufen“ auf der Karte bzw. „＋“ in der Listenzeile legt 1 bis 20 neue, volle Spulen mit denselben Daten an; ein Doppelklick auf Karte oder Zeile öffnet das Bearbeiten-Formular; Restgewichte auf 0,1 g genau; Spulenbild per Klick oder Drag & Drop (PNG, JPG oder WebP bis 5 MB); Spulen ohne Bild zeigen in der Liste ein Spulen-Symbol in ihrer Farbe
- **Resin im Lager** — der Umschalter „Filament | Resin“ oben im Filament-Lager wechselt zu deinen Resin-Flaschen: Mengen in ml, eigene Kennzahlen („Flaschen gesamt“), Vorschläge für Resin-Material und -Hersteller, „− Verbrauch“ zum Abbuchen verbrauchter Milliliter und Nachkaufen wie bei Spulen; neue Einträge bekommen die Art des gewählten Bereichs. Resin kommt nie in ein Filament-Fach, sondern nur in die Harzwanne eines Resin-Druckers, und zählt nicht bei „Reicht das Filament?“ und der Materialkosten-Schätzung.
- **Drucker & AMS-Fächer** — Drucker und ihre Mehrfarbeinheiten (Bambu AMS, AMS lite, AMS HT, Creality CFS, Prusa MMU3, Anycubic ACE Pro, Spulenhalter oder eigene mit frei wählbarer Fachanzahl) anlegen und Spulen per Drag & Drop oder Klick in die Fächer legen. Jeder neue Drucker bekommt automatisch einen Spulenhalter, sodass auch Drucker ohne AMS sofort eine Spule aufnehmen. Spulen im Drucker erscheinen getrennt vom Lager in einer eigenen Spalte; beim Herausnehmen kehren sie automatisch an ihren Stammplatz zurück. Spulen haben einen echten Farbwert (Palette oder Hex) zusätzlich zum Farbnamen.
- **Resin-Drucker** — beim Anlegen eines Druckers wählst du „Filament“ oder „Resin“. Ein Resin-Drucker bekommt statt des Spulenhalters eine feste Harzwanne für eine Flasche; du setzt sie per Ziehen oder über das Menü der Wanne ein, siehst Farbe und Rest in ml und buchst mit „− Verbrauch“ ab.
- **Reicht das Filament?** — für geslicete 3MF-Dateien vergleicht die Detailseite den Filamentbedarf mit deinen Spulen (Material und ähnliche Farbe) und zeigt, ob er reicht, nur mit Spulenwechsel reicht oder wie viel fehlt – samt passender Spule und ob sie im Drucker steckt. Die Druck-Warteschlange zeigt den Status je Eintrag und berücksichtigt den Gesamtbedarf.
- **Werkzeuge** — Abschnitt in der Seitenleiste mit Warteschlange, „Zuletzt angesehen“, „Neu hinzugefügt“, „Favoriten“ (alle mit Herz markierten Modelle), „Duplikate“ und Aufräum-Vorschlägen, jeweils mit Anzahl; die Ansichten filtern den Katalog und lassen sich mit Ordnern, Tags und Suche kombinieren.
- **Druckeranbindung** — optional (standardmäßig aus): Klipper/Moonraker-Drucker im Heimnetz melden den Filamentverbrauch fertiger und abgebrochener Drucke; nach deiner Bestätigung wird er von der Spule abgebucht, auf Wunsch mit Druckprotokoll-Eintrag. Nur lesend, nur selbst eingetragene Adressen im Heimnetz. Getestet mit Sovol SV08, Anycubic Kobra S1 (Rinkhals) und Qidi Smart 3; Ergebnisse für weitere Drucker sammelt die [Testseite](https://3mfkatalog.de/druckertest.html).
- **Katalog-Erweiterungen** — Druckstatus-Toggle + Gewicht pro Modell (echter Wert aus dem Slicer, falls die 3mf bereits gesliced wurde, sonst grobe Schätzung aus Volumen × Materialdichte), Sortierung nach "Zuletzt angesehen", NEU-Badge für kürzlich importierte Modelle, Creators-Filter (aus 3MF-Designer-Metadatum), automatische Erkennung exakter Datei-Duplikate beim Import per Inhalts-Hash
- **Filamentverbrauch aus dem Slicer** — liest den in OrcaSlicer/Bambu Studio gesliceten Filamentverbrauch (`Metadata/slice_info.config`) mit aus: reales Gewicht statt Schätzung, Aufschlüsselung pro Druckplatte und Filament (Typ, Farbe, Gramm, Meter) auf der Modell-Detailseite; Button "Metadaten neu einlesen" holt die Werte nachträglich, wenn eine bereits katalogisierte Datei in OrcaSlicer/Bambu Studio nachgesliced wurde
- **Materialkosten-Schätzung** — bei Modellen mit echtem Slicer-Filamentverbrauch zusätzlich eine geschätzte Materialkosten-Summe auf der Detailseite, berechnet aus Verbrauch und den Preisen passender Spulen im Filament-Lager
- **Druckprotokoll** — zusätzlich zum Druckstatus-Toggle ein Protokoll mehrerer Druckversuche pro Modell (Datum, Notiz, Foto), unabhängig vom Druckstatus
- **Katalog-Backup** — kompletter Katalog (Datenbank + Einstellungen) als ZIP exportierbar und wieder importierbar, mit automatischer Sicherung der bestehenden Datenbank vor jedem Import
- **Archive direkt entpacken** — einzeln importierte oder hineingezogene Archive (`.zip`, `.7z`, `.rar`, `.tar`, `.tar.gz`/`.tgz`, `.tar.bz2`, `.tar.xz`, `.tar.zst`) werden nach Rückfrage in einen Unterordner entpackt und die enthaltenen Modelle katalogisiert. Zielordner, Umgang mit bereits vorhandenen Ordnern und das Löschen des Original-Archivs entscheidest du im Dialog; nichts wird überschrieben. Aus Sicherheitsgründen werden Programme, Skripte und Verknüpfungen aus Archiven nie entpackt. Passwortgeschützte und mehrteilige Archive werden erkannt, aber nicht entpackt.
- **Modell-Thumbnails** — Raster-Ansicht zeigt ein echtes Bild pro Modell (eigenes Upload, eingebettetes 3MF-Thumbnail oder automatisch aus der 3D-Live-Vorschau erzeugter Snapshot), zusätzlich pro Modell eine Quelle als Link hinterlegbar
- **Warteschlange** — geordnete, per Drag & Drop sortierbare Liste ("als Nächstes drucken") in eigener Sidebar-Sektion, automatisches Entfernen beim Markieren als gedruckt
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

</details>

## Screenshots

| Katalog mit Vorschau | Modell mit Slicer-Daten |
|---|---|
| ![Katalog in der Kachelansicht](docs/benutzerhandbuch/bilder/02-katalog-grid.png) | ![Modell-Detailseite](docs/benutzerhandbuch/bilder/04-modell-detailseite.png) |
| **Drucker und AMS-Fächer** | **Drucke vom Klipper-Drucker bestätigen** |
| ![Drucker mit AMS-Fächern im Filament-Lager](docs/benutzerhandbuch/bilder/22-drucker-ams.png) | ![Dialog „Neue Drucke“](docs/benutzerhandbuch/bilder/27-neue-drucke.png) |

## Hilfe und Dokumentation

- [Benutzerhandbuch](docs/benutzerhandbuch/BENUTZERHANDBUCH.md) (DE + EN), Schritt für Schritt mit Bildern
- [Webseite](https://3mfkatalog.de) mit FAQ
- [Discord](https://discord.gg/abfVNfFqu3) für Fragen und Ideen
- Fehler: [Issue anlegen](https://github.com/Bexxs75/3mf-katalog-manager/issues/new/choose) oder auf Discord fragen

## Mitmachen

Hilfe ist willkommen, und nicht nur beim Code: Teste deinen Drucker auf der [Testseite](https://3mfkatalog.de/druckertest.html), prüfe eine Übersetzung, probier die Installation auf deinem System aus oder melde einen Fehler. Siehe [CONTRIBUTING.md](CONTRIBUTING.md), Issues mit dem Label [good first issue](https://github.com/Bexxs75/3mf-katalog-manager/labels/good%20first%20issue) und die [Übersetzungsanleitung](docs/TRANSLATING.md).

## Stand und Roadmap

Dieses Projekt befindet sich in aktiver Entwicklung. Der lokale Katalog (Import, Parsing, Tagging, Suche, 3D-Vorschau, Mehrsprachigkeit, Theming) und "In Slicer öffnen" sind funktionsfähig. Folgendes ist noch **nicht** umgesetzt:

- **Cloud-Anbindung** (Google Drive u. a.): war vorhanden, wurde aber wieder entfernt — zu instabil/fehleranfällig für den Alltagsgebrauch. Wird bei Gelegenheit sauber neu konzipiert, siehe CHANGELOG
- **"In Slicer öffnen" bei manuell hinzugefügten Slicern unter macOS**: für automatisch erkannte Slicer (Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer, UltiMaker Cura) funktioniert das Öffnen zuverlässig, da die Erkennung bereits die richtige, direkt ausführbare Programmdatei innerhalb des `.app`-Bundles findet. Bei manuell hinzugefügten Slicern muss im Dateidialog gezielt diese Datei ausgewählt werden, nicht das `.app`-Bundle selbst
- **STEP-Vorschau als separater Download**: Auf allen drei Plattformen gibt es dafür zwei Paketvarianten statt einer einzigen mit fest eingebauter STEP-Vorschau — siehe [Downloads](#downloads-1).
- **Code-Signing**: die macOS-`.dmg`- und Windows-`.msi`-Pakete sind unsigniert (kein Apple-Developer- bzw. Windows-Code-Signing-Zertifikat) — beim ersten Start warnen Gatekeeper bzw. SmartScreen entsprechend

Die öffentliche [Roadmap](https://github.com/users/Bexxs75/projects/1/views/1?groupedBy%5BcolumnId%5D=416999698) zeigt, was als Nächstes kommt.

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
