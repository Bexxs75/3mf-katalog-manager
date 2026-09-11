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
- Google-Drive-Anbindung: OAuth2-PKCE-Verbindung (Verbinden/Trennen per Klick auf die jeweilige Zeile), Token-Speicherung im OS-Schlüsselbund, Datei-/Ordnerauswahl über Googles offizielles Picker-Widget (im System-Browser), Import mit Duplikat-Erkennung und Sync-Status-Anzeige je Datei
- Native Rust-seitige Geometrie-Extraktion für die 3D-Vorschau: ZIP-Entpacken und Mesh-Parsing (3MF inkl. Multi-Part-"Production Extension"-Dateien mit `p:path`-Referenzen, STL) laufen jetzt vollständig im Backend statt im Frontend über three.js-Loader/`DOMParser`
- "In Slicer öffnen": Nutzer hinterlegt beliebig viele eigene Slicer-Programmpfade (statt fest codierter Einzelintegrationen für Bambu Studio, PrusaSlicer, OrcaSlicer, ...), Split-Button für Hauptauswahl/Wechsel, Kontextmenü-Eintrag für den zuletzt genutzten Slicer
- Hochladen zu Google Drive: bisher rein lokale Dateien lassen sich über die beschriftete "↑ Hochladen"-Schaltfläche im Detailbereich zu Google Drive hochladen (multipart/related-Upload mit Name/Inhalt in einer Anfrage); Googles Picker-Widget lässt den Nutzer vor dem Hochladen einen Drive-Zielordner auswählen statt immer ins Wurzelverzeichnis zu laden; die Datei wird danach automatisch mit dem entstandenen Drive-Eintrag verknüpft (Herkunft/Sync-Status/Cloud-ID) und über den bestehenden Sync-Check aktuell gehalten
- Filament-Lager: eigenständige Spulenverwaltung (Material, Hersteller, Farbe, Durchmesser, Ursprungs-/Restgewicht, Preis) über ein neues Header-Icon erreichbar, unabhängig vom Modell-Katalog - Verbrauchstracking pro Druck ist bewusst nicht Teil dieser Ausbaustufe
- Vier kleine Katalog-Erweiterungen: Druckstatus-Toggle + aus Volumen/Material geschätztes Gewicht pro Modell, Sortierung nach "Zuletzt angesehen" + NEU-Badge für kürzlich importierte Modelle, Creators als eigene Sidebar-Filterkategorie (aus dem beim 3MF-Import bereits geparsten Designer-Metadatum), automatische Erkennung exakter Datei-Duplikate beim Import (SHA-256-Inhalts-Hash) mit kurzer Zusammenfassungsmeldung
- Modell-Thumbnails im Raster: Bild-Priorität eigenes Upload > eingebettetes 3MF-Thumbnail > automatisch erzeugter 3D-Snapshot (einmalig beim ersten Ansehen im Detailbereich, client-seitig aus der bestehenden Live-Vorschau erzeugt) > Platzhalter; zusätzlich pro Modell eine Quelle als Link hinterlegbar
- Tags- und Creators-Sektionen in der Sidebar sind einzeln einklappbar
- Warteschlange ("als Nächstes drucken"): geordnete, per Drag & Drop sortierbare Liste in eigener einklappbarer Sidebar-Sektion; Hinzufügen/Entfernen über Detailbereich oder Kartei-Kontextmenü, automatisches Entfernen beim Markieren als gedruckt
- Gespeicherte Filter: aktuelle Kombination aus Ordner/Tag/Creator/Suche/Sortierung unter einem Namen speichern, per Klick wieder anwenden, in eigener einklappbarer Sidebar-Sektion verwalten
- Aufräum-Vorschläge: manuell auslösbarer Katalog-Scan (Einstellungen-Panel) findet verwaiste Dateipfade und Bestands-Duplikate (gleicher Inhalts-Hash, bereits im Katalog vorhanden); Ergebnis-Dialog mit Einzelauswahl, ältestes Duplikat je Gruppe bleibt vorausgewählt erhalten
- Komfort-Ansicht als Alternative zur bestehenden kompakten Oberfläche: deutlich größere Schrift, Grafiken und Bedienelemente (Karten-Layout angelehnt an printables.com/model), umschaltbar im Einstellungen-Panel unter "Ansicht", Standard bleibt die kompakte Ansicht. Neues Favorit-Merkmal je Modell (Herz-Icon), in beiden Ansichten sichtbar
- Import-Button in der Kopfzeile öffnet jetzt immer direkt das Dropdown-Menü (Dateien/Ordner/Cloud) statt eines Split-Buttons mit Sofort-Aktion — macht die bereits vorhandene, rekursive Ordner-Import-Option leichter auffindbar
- Ordner-Import auch aus Google Drive: neuer Menüpunkt "Ordner aus Drive importieren...", durchsucht den per Picker gewählten Ordner rekursiv nach `.3mf`/`.stl`-Dateien (inkl. aller Unterordner) und importiert sie wie beim Einzeldatei-Import

### Changed

- Backend liefert nur noch rohe, unformatierte Modelldaten (`ModelFileDto`); serverseitige, deutsch-only Formatierung (`format.rs`) entfernt und durch frontendseitige, sprachabhängige Formatierung ersetzt
- Sortierung nach Datum und Dateigröße korrigiert
- Google-Drive-OAuth-Scope `drive.readonly` entfernt, nur noch `drive.file` + `userinfo.email`: `drive.readonly` ist ein "restricted scope" und würde für die Google-Verifizierung ein kostenpflichtiges, jährlich zu wiederholendes CASA-Sicherheitsaudit erfordern, `drive.file` (nicht sensibel) nicht. Die eigenen In-App-Dialoge zum Durchstöbern des gesamten Drives (`CloudBrowserDialog`, `CloudFolderPickerDialog`) sind dafür entfallen — Datei-Import und Upload-Zielordner laufen jetzt über Googles offizielles Picker-Widget (öffnet sich im System-Browser, analog zum bestehenden OAuth-Login-Flow, da Google eingebettete WebViews auch hierfür nicht zuverlässig unterstützt)

### Security

- `quick-xml` von 0.36.2 auf 0.41.0 angehoben: schließt zwei Denial-of-Service-Schwachstellen (RUSTSEC-2026-0194, RUSTSEC-2026-0195, je CVSS 7.5/Hoch — quadratische Laufzeit bei doppelten Attributnamen bzw. unbegrenzte Speicherallokation bei Namespace-Deklarationen), erreichbar über eine präparierte `.3mf`-Datei beim normalen Import. Gefunden im Security-Review vom 2026-09-11 (`docs/security/security-review-2026-09-11.md`), per `cargo audit` bestätigt behoben; alle 106 Backend-Tests weiterhin grün

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
- Aufräum-Vorschläge: eine Datei, die gleichzeitig verwaist UND Teil einer Duplikat-Gruppe war, konnte im Auswahl-Dialog als "wird behalten" markiert und trotzdem gelöscht werden (die einzige noch vorhandene Kopie ging dadurch verloren) — verwaiste Dateien werden jetzt vor der Duplikat-Gruppierung ausgeschlossen; zusätzlich brach das Löschen bei einem Dateisystemfehler die ganze Auswahl vorzeitig ab statt einzelne Fehler zu überspringen, und ein blockiertes/nicht eingehängtes Laufwerk markierte fälschlich den gesamten Katalog als verwaist statt nur wirklich fehlende Dateien
- Warteschlange: Drag & Drop zum Neusortieren reagierte nicht — natives HTML5-Drag&Drop kollidierte unter WebKitGTK mit Tauris für den Datei-Import per OS-Drop aktivierter Fenster-Ebene-Erkennung; auf reine Maus-Events umgestellt
- Import aus Google Drive schlug für jede ausgewählte Datei mit HTTP 404 fehl — dem Picker-Widget fehlte `setAppId()` (Google-Cloud-Projektnummer), ohne den Google für den `drive.file`-Scope keine Zugriffsfreigabe auf per Picker ausgewählte, nicht von der App selbst erstellte Dateien registriert; neues Config-Feld `google_cloud_project_number` in `cloud.config.json`

### Known Limitations

- Cloud-Speicher: nur Google Drive implementiert (Verbinden/Trennen, Picker-basierter Import/Upload inkl. Zielordner-Auswahl); OneDrive, Dropbox und Proton Drive sind im UI weiterhin nur als Platzhalter vorhanden. Upload deckt keinen erneuten Upload/keine Konfliktauflösung bereits verknüpfter Dateien ab
- Google-Drive-Anbindung funktioniert aktuell nur auf der Entwicklungsmaschine (OAuth-Client-Konfiguration ist lokal, App im Google-Cloud-Testmodus) — für andere Nutzer nach einem Release noch nicht nutzbar
- `google_picker_api_key` in `cloud.config.json` muss manuell in der Google Cloud Console erzeugt werden (Picker API aktivieren, API-Key ohne HTTP-Referrer-Einschränkung anlegen) — noch nicht live gegen einen echten Key getestet
- "In Slicer öffnen" unterstützt macOS nicht (`.app`-Bundles benötigen einen anderen Start-Mechanismus als Windows/Linux-Executables)
- Plattformübergreifende Release-Builds (Windows `.msi`, macOS `.dmg`) sowie Code-Signing noch nicht eingerichtet — bisher nur unter Linux entwickelt und getestet
- CI/CD-Pipeline (GitHub Actions) noch nicht eingerichtet
