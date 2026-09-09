# 3MF Katalog Manager

Plattformunabhängige Desktop-Anwendung zur Katalogisierung und Verwaltung von 3MF- und STL-Dateien für den 3D-Druck. Gebaut mit [Tauri](https://tauri.app/) (Rust-Backend) und React/TypeScript/Tailwind.

## Funktionen

- **3MF- und STL-Parsing** — beide Formate gleichwertig unterstützt: OPC-Container-Entpackung inkl. eingebettetem Thumbnail bei 3MF, ASCII- und Binär-STL-Parsing
- **Automatische Metadaten-Extraktion** — Abmessungen, Volumen, Objektanzahl, Material (sofern in der 3MF vorhanden)
- **Automatische Hashtag-Generierung** — Vorschläge aus Dateiname, Geometrie-Merkmalen und Slicer-Profildaten, vom Nutzer editierbar
- **3D-Live-Vorschau** — three.js-Rendering direkt aus der Mesh-Geometrie, wenn kein eingebettetes Thumbnail vorhanden ist
- **Tag- und Ordnerverwaltung**, Suche und Filterung
- **Import** einzelner Dateien oder ganzer Ordner per Dialog oder Drag & Drop
- **Google-Drive-Import** — OAuth2-Verbindung, Datei-Browser mit Ordner-Navigation, Import mit Duplikat-Erkennung und Sync-Status je Datei
- **In Slicer öffnen** — beliebig viele selbst hinterlegte Slicer-Programme (herstellerunabhängig) direkt aus dem Katalog heraus starten (Windows/Linux)
- **Mehrsprachige Oberfläche** — Deutsch, Englisch, Spanisch, Französisch, umschaltbar zur Laufzeit
- **Hell-/Dunkel-Theme** mit System-Erkennung und manueller Auswahl, persistiert lokal

## Status

Dieses Projekt befindet sich in aktiver Entwicklung. Der lokale Katalog (Import, Parsing, Tagging, Suche, 3D-Vorschau, Mehrsprachigkeit, Theming), der Google-Drive-Import und "In Slicer öffnen" sind funktionsfähig. Folgendes ist noch **nicht** umgesetzt:

- **Weitere Cloud-Anbieter** (OneDrive, Dropbox, Proton Drive): nur in der Oberfläche als Platzhalter vorbereitet, keine echte Anbindung. Hochladen zu Google Drive ist ebenfalls noch nicht implementiert (nur Import)
- **Google-Drive-Nutzung durch andere Nutzer nach einem Release**: OAuth-Client-Konfiguration ist aktuell nur auf der Entwicklungsmaschine hinterlegt, die App läuft im Google-Cloud-Testmodus
- **macOS**: bisher nur unter Linux entwickelt und getestet; "In Slicer öffnen" unterstützt macOS gezielt nicht (`.app`-Bundles brauchen einen eigenen Start-Mechanismus)
- **Plattformübergreifende Release-Builds**: Windows-/macOS-Pakete (msi/dmg) sowie Code-Signing stehen noch aus
- **CI/CD-Pipeline**: noch nicht eingerichtet

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
