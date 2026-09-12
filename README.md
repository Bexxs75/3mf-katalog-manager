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
- **Katalog-Erweiterungen** — Druckstatus-Toggle + geschätztes Gewicht (aus Volumen × Materialdichte) pro Modell, Sortierung nach "Zuletzt angesehen", NEU-Badge für kürzlich importierte Modelle, Creators-Filter (aus 3MF-Designer-Metadatum), automatische Erkennung exakter Datei-Duplikate beim Import per Inhalts-Hash
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
