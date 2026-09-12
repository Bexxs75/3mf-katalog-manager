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
- Import-Button in der Kopfzeile öffnet jetzt immer direkt das Dropdown-Menü (Dateien/Ordner/Cloud) statt eines Split-Buttons mit Sofort-Aktion — macht die bereits vorhandene, rekursive lokale Ordner-Import-Option leichter auffindbar
- Papierkorb statt sofortigem Hart-Löschen: gelöschte Modelle werden zunächst in ein Papierkorb-Verzeichnis verschoben und bleiben dort wiederherstellbar (eigene Papierkorb-Ansicht über neues Header-Icon mit Mengen-Badge, inkl. 3D-Vorschau); endgültiges Löschen sowie "Papierkorb leeren" räumen Datei und Katalog-Eintrag danach dauerhaft weg
- Datei-/Ordnerdialoge nutzen das native XDG-Desktop-Portal statt eines generischen GTK-Dialogs — auf KDE erscheint z. B. der echte Kirigami-Dialog mit Dolphins Ordnersortierung, statt eine Desktop-Umgebung selbst zu erkennen
- Modell-Detailseite: vollflächige Ansicht (Doppelklick auf ein Modell) mit großer 3D-Vorschau, allen Metadaten und Druckplatten-Anzahl bei Bambu-Studio-/OrcaSlicer-3MF-Dateien (Best-Effort-Erkennung); das bisherige Seitenpanel bleibt für schnelle Einzelauswahl per Klick weiter bestehen
- Dreh-Steuerelemente im 3D-Viewer der Detailseite: Play/Pause-Button für automatische Dauerdrehung um die vertikale Achse plus ←/→-Buttons für 15°-Schritte, zusätzlich zum weiterhin unveränderten freien Maus-Ziehen; Auto-Rotation pausiert automatisch bei eigener Maus-Interaktion
- Mehrfachauswahl in der Katalogübersicht: Checkbox je Karte/Zeile, "Alle auswählen" (respektiert aktive Filter), Aktionsleiste für Warteschlange, Druckstatus und Löschen (mit Bestätigung) über mehrere markierte Modelle gleichzeitig
- Sidebar: Tags und Creators starten eingeklappt und zeigen sich als kompakte, umbrechende Chips statt langer Zeilenlisten; Tags mit nur einem Treffer werden standardmäßig ausgeblendet (aktiv ausgewählte Tags bleiben sichtbar)
- Kontextmenü (Rechtsklick auf eine Karte) bietet jetzt auch "Gedruckt"/"Nicht gedruckt" direkt an
- Automatische Slicer-Erkennung: durchsucht beim Start bekannte Installationsorte für Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer und UltiMaker Cura (PATH, `/opt`, Flatpak-Exports, gängige AppImage-Ablageorte auf Linux; `Program Files`/`Program Files (x86)` auf Windows) und ergänzt Treffer automatisch in die Einstellungen-Liste (Label "automatisch erkannt"); manuelles Hinzufügen für Custom-Forks/Herstellervarianten bleibt bestehen, entfernte Auto-Treffer tauchen bei künftigen Scans nicht wieder auf
- Einstellung "Bevorzugte Ansicht" (Zahnrad-Menü): legt fest, ob Katalog-Karten und die Modell-Detailseite standardmäßig das eingebettete Datei-Bild oder eine gerenderte 3D-Ansicht bevorzugen; bei "Gerenderte Ansicht" rendert die App fehlende 3D-Schnappschüsse automatisch und sequenziell im Hintergrund nach (unabhängig von der aktiven Ansicht), eine einzelne nicht ladbare Datei blockiert dabei nicht die restliche Warteschlange
- Sammlungen: dritter Organisationsmechanismus neben Ordnern und Tags — viele-zu-viele Zuordnung wie Tags, aber mit manuell festlegbarer Reihenfolge (Drag & Drop) als eigentlichem Mehrwert. Erstellung über die Mehrfachauswahl-Aktionsleiste ("Zu Sammlung hinzufügen") oder per neuer Import-Option "Ordner als Sammlung importieren" (praktisch für Modelle, die auf mehrere Dateien in einem Ordner aufgeteilt sind — jede Datei im Ordner wird zugeordnet, auch bereits vorher katalogisierte Duplikate). Neuer Reiter "Sammlungen" neben "Alle Modelle" öffnet eine Kartenübersicht aller Sammlungen (umbenennen/löschen möglich); Klick auf eine Sammlung zeigt ihre Modelle in fester Reihenfolge, Sortieren-Dropdown ist dabei ausgeblendet. In den Papierkorb verschobene Modelle werden in Sammlungen korrekt ausgeblendet und tauchen nach dem Wiederherstellen automatisch wieder auf

### Changed

- Backend liefert nur noch rohe, unformatierte Modelldaten (`ModelFileDto`); serverseitige, deutsch-only Formatierung (`format.rs`) entfernt und durch frontendseitige, sprachabhängige Formatierung ersetzt
- Sortierung nach Datum und Dateigröße korrigiert
- Google-Drive-OAuth-Scope `drive.readonly` entfernt, nur noch `drive.file` + `userinfo.email`: `drive.readonly` ist ein "restricted scope" und würde für die Google-Verifizierung ein kostenpflichtiges, jährlich zu wiederholendes CASA-Sicherheitsaudit erfordern, `drive.file` (nicht sensibel) nicht. Die eigenen In-App-Dialoge zum Durchstöbern des gesamten Drives (`CloudBrowserDialog`, `CloudFolderPickerDialog`) sind dafür entfallen — Datei-Import und Upload-Zielordner laufen jetzt über Googles offizielles Picker-Widget (öffnet sich im System-Browser, analog zum bestehenden OAuth-Login-Flow, da Google eingebettete WebViews auch hierfür nicht zuverlässig unterstützt)
- Bildquellen einer Datei (eigenes Upload, eingebettetes 3MF-/STL-Thumbnail, gerenderter 3D-Snapshot) werden nicht mehr serverseitig zu einem festen `displayImage` priorisiert, sondern getrennt ans Frontend geliefert — die Priorisierung entscheidet jetzt die neue "Bevorzugte Ansicht"-Einstellung
- Modell-Detailseite: Betrachter-Spalte (Bild/3D-Vorschau) bei sehr breiten Bildschirmen (z. B. 3440×1440) ca. 25 % größer, Metadaten-Spalte rechts bleibt dabei unverändert; zuvor wurde der gesamte Inhaltsbereich auf max. 1400px begrenzt, was auf breiten Monitoren insgesamt zu klein wirkte
- Sortieren-Dropdown eigenständig im dunklen Theme gestylt (Popover-Muster wie die bestehende Slicer-Auswahl) statt eines nativen `<select>`-Elements mit hellem Systemhintergrund und falscher Schriftart

### Removed

- Cloud-Anbindung (Google Drive) komplett entfernt: Backend-Modul `src-tauri/src/cloud/` inkl. aller 7 Tauri-Commands, `tauri-plugin-opener`-Abhängigkeit (Rust + npm, nur für den Picker-Browser-Start gebraucht) sowie `oauth2`/`keyring`/`reqwest`/`async-trait`/`tokio` aus `Cargo.toml` (waren ausschließlich Cloud-Abhängigkeiten); Frontend-UI (Cloud-Konten-Sidebar-Sektion, Cloud-Import-Option, Sync-Status-Anzeigen, Herkunfts-Badges) und zugehörige i18n-Keys in allen vier Sprachen entfernt. `cloud_accounts`-Tabelle und der `sync_status`/`cloud_id`-Teil des `origin`-Wertebereichs aus `schema.sql` entfernt bzw. auf `'local'` reduziert (nur für Neuinstallationen wirksam — bestehende Datenbanken behalten die inerten Spalten `sync_status`/`cloud_id`, da dieses Projekt kein `DROP COLUMN`-Migrationsmuster hat und das Risiko für Bestandsdaten den Aufwand nicht wert war). Grund: die Google-Drive-Integration war trotz mehrfacher Nacharbeit (Picker-Migration, Timeout-Handling, `setAppId`-Fix) im Alltag zu instabil/fehleranfällig (u. a. ungeklärte Timeouts bei der Drive-Auswahl) und band zu viel Aufmerksamkeit von wichtigeren Themen ab. Wird bei Gelegenheit sauber neu konzipiert statt weiter geflickt.

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
- Komfort-Ansicht wirkte in Kopfzeile, Seitenleiste, Listenansicht, Filament-Lager, Kontextmenü, Aufräum-Dialog und Import-Banner überhaupt nicht: Tailwind kompilierte die Klasse `text-[var(--font-size-X)]` als Text-*Farbe* statt Schriftgröße (mehrdeutiger `text-`-Präfix), behoben durch expliziten Typ-Hinweis `text-[length:var(--font-size-X)]`; zusätzlich nutzten die eigentlichen Ordner-/Tag-/Creator-Namen in der Seitenleiste bislang Tailwinds feste `text-xs`-Klasse statt eines Tokens und wuchsen dadurch auch nach diesem Fix noch nicht mit (neuer Token `--font-size-item`)
- Weißer Bildschirm beim Start auf Systemen mit NVIDIA-Grafikkarte (proprietärer Treiber): WebKitGTKs standardmäßiges DMA-BUF-Hardware-Rendering ist mit dem NVIDIA-Treiber inkompatibel (`Failed to create GBM buffer`) — App setzt jetzt automatisch beim Start die nötige Umgebungsvariable, keine manuelle Konfiguration mehr nötig
- Externe Slicer (z. B. OrcaSlicer) starteten nicht zuverlässig aus der App heraus, wenn der hinterlegte Pfad auf ein `AppRun`-Startskript zeigte (typisch bei als AppImage vertriebenen Slicern): die App vererbte ihre eigene AppImage-interne Umgebung (u. a. `LD_LIBRARY_PATH`, `APPDIR`) ungefiltert an den gestarteten Prozess, wodurch dieser seine eigenen Bibliotheken/Hilfsprozesse an falschen Pfaden suchte — diese Variablen werden jetzt vor dem Start externer Programme entfernt
- Bambu Studio wurde auf Arch/CachyOS-AUR-Installationen von der automatischen Slicer-Erkennung nicht gefunden, da das AUR-Paket die Binärdatei als `bambustudio` (klein geschrieben, ohne Bindestrich) installiert — als dritte Namensvariante ergänzt
- Papierkorb-Bug: zeigte der Katalog-Pfad einer Datei nicht mehr auf einen erreichbaren Ort (z. B. weil ein Cloud-Ordner umbenannt wurde), landete die Datei beim Löschen bisher sofort unwiderruflich verloren statt im Papierkorb — genau das Gegenteil vom Zweck des Features
- Automatisches Hintergrund-Nachrendern fehlender 3D-Schnappschüsse: eine einzelne nicht ladbare Datei blockierte zuvor die gesamte Warteschlange dauerhaft, da immer nur die erste Datei erneut versucht wurde; fehlerhafte Dateien werden jetzt übersprungen. Zusätzlich gibt jede beendete Hintergrund-Rendering-Instanz ihren WebGL-Kontext jetzt explizit frei, statt auf Garbage Collection zu warten (Risiko von "Too many active WebGL contexts" bei großen Katalogen)
- Sammlungen: in den Papierkorb verschobene Modelle blieben in der Sammlungs-Detailansicht sichtbar und wurden in der Modellanzahl mitgezählt; Sidebar-Filter (Ordner/Tags/Creator/Suche) verließen die Sammlungsansicht nicht beim Anklicken; die Listenansicht zeigte bei aktiver Sammlung weiterhin den kompletten Katalog statt der Sammlungs-Mitglieder

### Known Limitations

- Keine Cloud-Anbindung (siehe "Removed" oben) — nur lokaler Dateisystem-Import
- "In Slicer öffnen" unterstützt macOS nicht (`.app`-Bundles benötigen einen anderen Start-Mechanismus als Windows/Linux-Executables)
- Plattformübergreifende Release-Builds (Windows `.msi`, macOS `.dmg`) sowie Code-Signing noch nicht eingerichtet — bisher nur unter Linux entwickelt und getestet
- CI/CD-Pipeline (GitHub Actions) noch nicht eingerichtet
