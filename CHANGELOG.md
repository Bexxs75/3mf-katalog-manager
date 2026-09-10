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
- Google-Drive-Anbindung: OAuth2-PKCE-Verbindung (Verbinden/Trennen per Klick auf die jeweilige Zeile), Token-Speicherung im OS-Schlüsselbund, Datei-Browser mit Ordner-Navigation, Import mit Duplikat-Erkennung und Sync-Status-Anzeige je Datei
- Native Rust-seitige Geometrie-Extraktion für die 3D-Vorschau: ZIP-Entpacken und Mesh-Parsing (3MF inkl. Multi-Part-"Production Extension"-Dateien mit `p:path`-Referenzen, STL) laufen jetzt vollständig im Backend statt im Frontend über three.js-Loader/`DOMParser`
- "In Slicer öffnen": Nutzer hinterlegt beliebig viele eigene Slicer-Programmpfade (statt fest codierter Einzelintegrationen für Bambu Studio, PrusaSlicer, OrcaSlicer, ...), Split-Button für Hauptauswahl/Wechsel, Kontextmenü-Eintrag für den zuletzt genutzten Slicer
- Hochladen zu Google Drive: bisher rein lokale Dateien lassen sich über die beschriftete "↑ Hochladen"-Schaltfläche im Detailbereich zu Google Drive hochladen (multipart/related-Upload mit Name/Inhalt in einer Anfrage); ein Zielordner-Dialog (gleiche Ordner-Navigation wie beim Import) lässt den Nutzer vor dem Hochladen einen Drive-Ordner auswählen statt immer ins Wurzelverzeichnis zu laden; die Datei wird danach automatisch mit dem entstandenen Drive-Eintrag verknüpft (Herkunft/Sync-Status/Cloud-ID) und über den bestehenden Sync-Check aktuell gehalten

### Changed

- Backend liefert nur noch rohe, unformatierte Modelldaten (`ModelFileDto`); serverseitige, deutsch-only Formatierung (`format.rs`) entfernt und durch frontendseitige, sprachabhängige Formatierung ersetzt
- Sortierung nach Datum und Dateigröße korrigiert

### Fixed

- CSS-`@import`-Reihenfolge und Rust-Abhängigkeiten fixiert
- Lesbarkeit des Sortieren-Dropdowns im dunklen Theme behoben
- Typfehler bei der Sync-Status-Übersetzung (`SYNC_KEYS`) durch präzisere Typisierung statt Type-Cast behoben
- Überlaufender Header in spanischer Sprache (Einstellungen-Zahnrad wurde abgeschnitten) durch vergrößertes Standardfenster behoben
- 3D-Vorschau blockierte bei großen Multi-Part-3MF-Dateien (mehrere hundert MB) die komplette Oberfläche für mehrere Minuten — behoben durch native Geometrie-Extraktion (siehe oben)
- Größe/Volumen/Material wurden bei Multi-Part-3MF-Dateien nicht angezeigt ("–"), weil der Metadaten-Parser referenzierte Objekt-Dateien nicht auflöste
- WebGL-Kontext der 3D-Vorschau wurde bei jedem Modellwechsel komplett neu aufgebaut statt nur das angezeigte Objekt in der bestehenden Szene auszutauschen (spürbare Verzögerung)
- Verwaiste Tags (letzte Datei mit diesem Tag gelöscht) blieben in Datenbank und Sidebar stehen, statt automatisch entfernt zu werden
- Aus Google Drive importierte Dateien übernahmen den internen Cache-Dateinamen statt des echten Drive-Dateinamens (dadurch auch sinnfreie automatische Hashtags)
- Google-Drive-Konto verbinden/trennen und jeder authentifizierte Cloud-Aufruf blockierten kurzzeitig den Tokio-Worker- bzw. IPC-Dispatch-Thread durch synchrones D-Bus-IPC zum Schlüsselbund — jetzt über `spawn_blocking` entkoppelt (gleiche Fehlerklasse wie das bereits gefixte `accept()` beim OAuth-Login)

### Known Limitations

- Cloud-Speicher: nur Google Drive implementiert (Verbinden/Trennen, Datei-Browser, Import, Upload inkl. Zielordner-Auswahl); OneDrive, Dropbox und Proton Drive sind im UI weiterhin nur als Platzhalter vorhanden. Upload deckt keinen erneuten Upload/keine Konfliktauflösung bereits verknüpfter Dateien ab
- Google-Drive-Anbindung funktioniert aktuell nur auf der Entwicklungsmaschine (OAuth-Client-Konfiguration ist lokal, App im Google-Cloud-Testmodus) — für andere Nutzer nach einem Release noch nicht nutzbar
- "In Slicer öffnen" unterstützt macOS nicht (`.app`-Bundles benötigen einen anderen Start-Mechanismus als Windows/Linux-Executables)
- Plattformübergreifende Release-Builds (Windows `.msi`, macOS `.dmg`) sowie Code-Signing noch nicht eingerichtet — bisher nur unter Linux entwickelt und getestet
- CI/CD-Pipeline (GitHub Actions) noch nicht eingerichtet
