# Changelog

Alle nennenswerten Änderungen an diesem Projekt werden hier dokumentiert.
Format angelehnt an [Keep a Changelog](https://keepachangelog.com/de/1.0.0/), Versionierung nach [Semantic Versioning](https://semver.org/lang/de/).

## [Unreleased]

Noch kein Release getaggt — dieser Abschnitt fasst die bisherige Entwicklung seit Projektstart zusammen.

### Added

- Tauri-Projektgerüst mit integriertem React/TypeScript/Tailwind-UI-Paket
- Eigenständiges 3MF-Parsing-Modul (OPC-Container, Model-XML, eingebettetes Thumbnail)
- Eigenständiges STL-Parsing-Modul (ASCII und Binär)
- SQLite-Katalogdatenbank (Schema, Modelle, Repository) inkl. Herkunfts-/Cloud-ID-/Sync-Status-Feldern
- Automatische Tagging-Heuristiken (Dateiname, Geometrie-Merkmale)
- Tauri-Command-Bridge: Frontend nutzt echte Backend-Daten statt Beispieldaten
- 3D-Live-Vorschau im Detailbereich mittels three.js
- Import-Workflow: Dateidialog, Ordnerauswahl, Drag-and-Drop; Split-Button mit Dropdown für Importoptionen
- Löschfunktion für Modelle mit Bestätigungsdialog und Kontextmenü
- Vollständige Mehrsprachigkeit (Deutsch/Englisch/Spanisch/Französisch): eigenes Context-basiertes i18n-System ohne externe Bibliothek, `Translations`-Interface erzwingt Vollständigkeit der Wörterbücher zur Compile-Zeit, Sprachumschalter im Einstellungen-Panel, Persistenz in localStorage
- Lokalisierte Formatierung (Datum, Uhrzeit relativ, Dateigröße, Volumen, Abmessungen) über `Intl`-APIs im Frontend

### Changed

- Backend liefert nur noch rohe, unformatierte Modelldaten (`ModelFileDto`); serverseitige, deutsch-only Formatierung (`format.rs`) entfernt und durch frontendseitige, sprachabhängige Formatierung ersetzt
- Sortierung nach Datum und Dateigröße korrigiert

### Fixed

- CSS-`@import`-Reihenfolge und Rust-Abhängigkeiten fixiert
- Lesbarkeit des Sortieren-Dropdowns im dunklen Theme behoben
- Typfehler bei der Sync-Status-Übersetzung (`SYNC_KEYS`) durch präzisere Typisierung statt Type-Cast behoben
- Überlaufender Header in spanischer Sprache (Einstellungen-Zahnrad wurde abgeschnitten) durch vergrößertes Standardfenster behoben

### Known Limitations

- Cloud-Speicher-Integration (Google Drive, OneDrive, Dropbox, Proton Drive) ist im UI nur als Platzhalter vorhanden, ohne echte OAuth2-Anbindung oder Backend-Implementierung
- Plattformübergreifende Release-Builds (Windows `.msi`, macOS `.dmg`) sowie Code-Signing noch nicht eingerichtet
- CI/CD-Pipeline (GitHub Actions) noch nicht eingerichtet
