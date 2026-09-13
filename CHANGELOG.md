# Changelog

Alle nennenswerten Änderungen an diesem Projekt werden hier dokumentiert.
Format angelehnt an [Keep a Changelog](https://keepachangelog.com/de/1.0.0/), Versionierung nach [Semantic Versioning](https://semver.org/lang/de/).

Rückwirkend versioniert am 2026-09-12: das Projekt lief bis dahin komplett unter der Scaffold-Versionsnummer `0.1.0`, ohne dass Meilensteine markiert wurden. Die folgenden Versionsgrenzen wurden nachträglich anhand von Entwicklungs-Tagen und natürlichen Feature-Abschlüssen (jeweils an einem Dokumentations-Commit) gezogen, keine davon wurde zum jeweiligen Zeitpunkt live getaggt oder veröffentlicht.

## [Unreleased]

## [0.6.0] - 2026-09-13

Erster getaggter Release. Die einzelnen Abschnitte unten stammen aus mehreren Entwicklungstagen (2026-09-12 und 2026-09-13), siehe Hinweis oben zur rückwirkenden Versionierung.

### Added

- Filamentverbrauch aus gesliceten OrcaSlicer/Bambu-Studio-3mf-Dateien: liest `Metadata/slice_info.config` beim Import mit aus und zeigt auf der Modell-Detailseite das echte, vom Slicer berechnete Gewicht statt der bisherigen groben Schätzung aus Volumen × Materialdichte — inklusive Aufschlüsselung pro Druckplatte und Filament (Typ, Farbe, Gramm, Meter). Neuer Button "Metadaten neu einlesen" liest eine bereits katalogisierte Datei erneut vom Pfad ein, falls sie inzwischen in OrcaSlicer/Bambu Studio gesliced und überschrieben wurde (inkl. sichtbarer Erfolgsmeldung, Fehler/Erfolg sind pro Modell gescoped). Bewusst außerhalb des Scopes: Live-Drucker-Anbindung, Headless-Slicing, automatischer Abzug vom Filament-Lager
- Filament-Lager, Neugestaltung (nach Nutzer-Feedback "nicht gut/intuitiv" + Mockup-Review mit 3 Vorschlägen): neues Feld **Lagerort** (Autocomplete aus bereits verwendeten Standorten); lokaler **Dashboard/Liste**-Umschalter direkt im Filament-Lager (Kartenraster mit Bestandsbalken/Statusfarbe bzw. sortierbare Tabelle im Stil einer Lagerverwaltungssoftware); Statistik-Leiste (Spulen gesamt, Restbestand, belegte Lagerplätze, niedrig/leer) und Status-Filter-Chips (Niedrig/Leer); neue Theme-Tokens `--good`/`--warn`/`--crit` für eigenständige Statusfarben; Anlage-Formular als seitliches Panel mit Abschnitten (Bild/Identifikation/Lagerung/Bestand) statt der bisherigen gequetschten 7-Felder-Leiste, inkl. Live-Vorschau des Bestandsbalkens; "Anzahl Spulen"-Stepper beim Neuanlegen legt mehrere unabhängige Spulen mit denselben Werten gleichzeitig an
- Filament-Lager: kuratierte Autocomplete-Vorschlagsliste gängiger FDM-Materialien (PLA, PETG, ABS, ASA, TPU, Nylon, PC, PEEK, Carbon-Fiber-Varianten u. a.) und Filament-Hersteller (Bambu Lab, Prusament, Polymaker, eSUN, SUNLU, Fillamentum, ColorFabb u. a.) für Material-/Hersteller-Felder; eigene, themekonforme `AutocompleteInput`-Komponente
- Neues App-Icon: isometrischer 3D-Druck-Layer-Würfel, Farben direkt aus den echten Theme-Tokens (oklch → sRGB) berechnet, ersetzt das generische Tauri-Standard-Icon auf allen Plattformen
- Content-Security-Policy aktiviert (`tauri.conf.json`)
- Windows-`.msi`-Build auf einer Windows-11-VM (QEMU/KVM) end-to-end verifiziert: Build, Installation, Icon-Extraktion aus der installierten `.exe` zur visuellen Kontrolle

### Changed

- App-Anzeigename (Fenstertitel, Installer-/Taskleisten-Name) von "mf-katalog-manager" auf "3MF Katalog Manager" korrigiert — ein Cargo-Crate-Name darf nicht mit einer Ziffer beginnen, weshalb beim allerersten Projekt-Scaffold "mf-katalog-manager" statt "3mf-katalog-manager" gewählt wurde. App-Identifier und Datenordner sind unverändert
- Quell-URL-Bearbeitung im Detailbereich (Seitenpanel kompakt/komfortabel, Modell-Detailseite) in einen gemeinsamen `useEditableSourceUrl`-Hook zusammengeführt, vorher dreifach mit identischer Logik dupliziert

### Security

- Vollständiges Code-Review (Senior-Dev-Review gegen OWASP Top 10 / CWE / ISO 27002 A.8.28), dokumentiert in [GitHub Issue #1](https://github.com/Bexxs75/3mf-katalog-manager/issues/1): Content-Security-Policy war komplett deaktiviert (`csp: null`) — jetzt auf eine restriktive Policy (`default-src 'self'`, `img-src` erlaubt `data:` für Base64-Thumbnails) gesetzt; die Quell-URL eines Modells akzeptiert serverseitig nur noch `http(s)://`-Links (verhinderte, dass ein `javascript:`/`data:`-Wert als klickbarer `<a href>` im WebView landet); "In Slicer öffnen" prüft vor dem Start, dass der übergebene Pfad auf eine existierende, ausführbare Datei zeigt, statt jeden String klaglos an `process::Command` zu übergeben
- App-Datenverzeichnis (`0700`) und `catalog.db` (`0600`) werden unter Unix beim Start gehärtet (ISO 27002 A.8.28)
- Diverses Aufräumen aus demselben Review (niedrige Priorität, keine Sicherheitswirkung): toter Tauri-Demo-Command (`greet`) entfernt, deprecated `quick_xml`-Attribut-API (`unescape_value` → `normalized_value`) ersetzt, STL-Parser liest den Facet-Count jetzt über denselben bounds-geprüften Zugriff wie die restlichen Werte statt sich implizit auf die Aufrufreihenfolge zu verlassen, mehrere ungenutzte Codepfade entfernt oder als "nur Test"/"bewusst beibehalten" markiert

### Fixed

- Dreh-Buttons im 3D-Viewer der Detailseite: Hover-Effekt (`bg-white/10`) im hellen Theme praktisch unsichtbar, jetzt themekonform über einen Token; fehlendes `cursor-pointer` an allen drei Buttons ergänzt
- Sammlungen, sechs zurückgestellte Minor-Findings: Plural-Anzeige der Modell-Kartenanzahl nutzte keine echten Pluralformen; toter i18n-Schlüssel `backToCollectionsLabel` entfernt; Mehrfachauswahl blieb beim Wechsel zwischen Ansichten/Sammlungen bestehen; "Ordner als Sammlung importieren" erfasste Datei-Duplikate an einem anderen Pfad nicht; Drag-Umsortieren startete ohne Toleranzschwelle von jeder Stelle der Karte aus; `list_collection_files` lud pro Modell einzeln (N+1) statt gebündelt
- Filament-Autocomplete: erste Version nutzte ein natives `<input list>`/`<datalist>`, dessen Vorschlags-Popup vom Betriebssystem/WebKit gerendert wird und sich nicht an das dunkle App-Theme anpassen lässt (erschien als weißes System-Popup) — durch die eigene, themekonforme `AutocompleteInput`-Komponente ersetzt
- "Anzahl Spulen"-Feld im Filament-Anlage-Formular zeigte gleichzeitig die eigenen -/+-Buttons und die nativen Browser-Spinner-Pfeile des Zahlenfelds — native Spinner per CSS ausgeblendet

## [0.5.0] - 2026-09-12

### Added

- Datei-/Ordnerdialoge nutzen das native XDG-Desktop-Portal statt eines generischen GTK-Dialogs — auf KDE erscheint z. B. der echte Kirigami-Dialog mit Dolphins Ordnersortierung
- Modell-Detailseite: vollflächige Ansicht (Doppelklick auf ein Modell) mit großer 3D-Vorschau, allen Metadaten und Druckplatten-Anzahl bei Bambu-Studio-/OrcaSlicer-3MF-Dateien (Best-Effort-Erkennung); das bisherige Seitenpanel bleibt für schnelle Einzelauswahl weiter bestehen
- Dreh-Steuerelemente im 3D-Viewer der Detailseite: Play/Pause-Button für automatische Dauerdrehung plus ←/→-Buttons für 15°-Schritte, zusätzlich zum freien Maus-Ziehen; Auto-Rotation pausiert automatisch bei eigener Maus-Interaktion
- Papierkorb statt sofortigem Hart-Löschen: gelöschte Modelle werden zunächst in ein Papierkorb-Verzeichnis verschoben und bleiben dort wiederherstellbar (eigene Papierkorb-Ansicht über neues Header-Icon mit Mengen-Badge, inkl. 3D-Vorschau); endgültiges Löschen sowie "Papierkorb leeren" räumen Datei und Katalog-Eintrag danach dauerhaft weg
- Mehrfachauswahl in der Katalogübersicht: Checkbox je Karte/Zeile, "Alle auswählen" (respektiert aktive Filter), Aktionsleiste für Warteschlange, Druckstatus und Löschen (mit Bestätigung) über mehrere markierte Modelle gleichzeitig
- Sidebar: Tags und Creators starten eingeklappt und zeigen sich als kompakte, umbrechende Chips statt langer Zeilenlisten; Tags mit nur einem Treffer werden standardmäßig ausgeblendet (aktiv ausgewählte Tags bleiben sichtbar)
- Kontextmenü (Rechtsklick auf eine Karte) bietet jetzt auch "Gedruckt"/"Nicht gedruckt" direkt an
- Automatische Slicer-Erkennung: durchsucht beim Start bekannte Installationsorte für Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer und UltiMaker Cura (PATH, `/opt`, Flatpak-Exports, gängige AppImage-Ablageorte auf Linux; `Program Files`/`Program Files (x86)` auf Windows) und ergänzt Treffer automatisch (Label "automatisch erkannt"); manuelles Hinzufügen für Custom-Forks bleibt bestehen
- Einstellung "Bevorzugte Ansicht" (Zahnrad-Menü): legt fest, ob Katalog-Karten und die Modell-Detailseite standardmäßig das eingebettete Datei-Bild oder eine gerenderte 3D-Ansicht bevorzugen; bei "Gerenderte Ansicht" rendert die App fehlende 3D-Schnappschüsse automatisch und sequenziell im Hintergrund nach, eine einzelne nicht ladbare Datei blockiert dabei nicht die restliche Warteschlange
- Sammlungen: dritter Organisationsmechanismus neben Ordnern und Tags — viele-zu-viele Zuordnung wie Tags, aber mit manuell festlegbarer Reihenfolge (Drag & Drop). Erstellung über die Mehrfachauswahl-Aktionsleiste ("Zu Sammlung hinzufügen") oder per neuer Import-Option "Ordner als Sammlung importieren". Neuer Reiter "Sammlungen" neben "Alle Modelle" öffnet eine Kartenübersicht aller Sammlungen (umbenennen/löschen möglich); Klick auf eine Sammlung zeigt ihre Modelle in fester Reihenfolge. In den Papierkorb verschobene Modelle werden in Sammlungen korrekt ausgeblendet und tauchen nach dem Wiederherstellen automatisch wieder auf

### Changed

- Bildquellen einer Datei (eigenes Upload, eingebettetes 3MF-/STL-Thumbnail, gerenderter 3D-Snapshot) werden nicht mehr serverseitig zu einem festen `displayImage` priorisiert, sondern getrennt ans Frontend geliefert — die Priorisierung entscheidet die neue "Bevorzugte Ansicht"-Einstellung
- Modell-Detailseite: Betrachter-Spalte (Bild/3D-Vorschau) bei sehr breiten Bildschirmen (z. B. 3440×1440) ca. 25 % größer, Metadaten-Spalte rechts bleibt dabei unverändert
- Sortieren-Dropdown eigenständig im dunklen Theme gestylt (Popover-Muster wie die bestehende Slicer-Auswahl) statt eines nativen `<select>`-Elements mit hellem Systemhintergrund und falscher Schriftart

### Removed

- Cloud-Anbindung (Google Drive) komplett entfernt: Backend-Modul `src-tauri/src/cloud/` inkl. aller 7 Tauri-Commands, `tauri-plugin-opener`-Abhängigkeit sowie `oauth2`/`keyring`/`reqwest`/`async-trait`/`tokio` aus `Cargo.toml`; Frontend-UI (Cloud-Konten-Sidebar-Sektion, Cloud-Import-Option, Sync-Status-Anzeigen, Herkunfts-Badges) und zugehörige i18n-Keys entfernt. `cloud_accounts`-Tabelle und der `sync_status`/`cloud_id`-Teil des `origin`-Wertebereichs aus `schema.sql` entfernt bzw. auf `'local'` reduziert (nur für Neuinstallationen wirksam — bestehende Datenbanken behalten die inerten Spalten `sync_status`/`cloud_id`, da dieses Projekt kein `DROP COLUMN`-Migrationsmuster hat). Grund: die Google-Drive-Integration war trotz mehrfacher Nacharbeit im Alltag zu instabil/fehleranfällig und band zu viel Aufmerksamkeit von wichtigeren Themen ab

### Fixed

- Weißer Bildschirm beim Start auf Systemen mit NVIDIA-Grafikkarte (proprietärer Treiber): WebKitGTKs standardmäßiges DMA-BUF-Hardware-Rendering ist mit dem NVIDIA-Treiber inkompatibel (`Failed to create GBM buffer`) — App setzt jetzt automatisch beim Start die nötige Umgebungsvariable
- Externe Slicer (z. B. OrcaSlicer) starteten nicht zuverlässig aus der App heraus, wenn der hinterlegte Pfad auf ein `AppRun`-Startskript zeigte: die App vererbte ihre eigene AppImage-interne Umgebung ungefiltert an den gestarteten Prozess — diese Variablen werden jetzt vor dem Start externer Programme entfernt
- Bambu Studio wurde auf Arch/CachyOS-AUR-Installationen von der automatischen Slicer-Erkennung nicht gefunden, da das AUR-Paket die Binärdatei als `bambustudio` installiert — als dritte Namensvariante ergänzt
- Papierkorb-Bug: zeigte der Katalog-Pfad einer Datei nicht mehr auf einen erreichbaren Ort, landete die Datei beim Löschen bisher sofort unwiderruflich verloren statt im Papierkorb
- Automatisches Hintergrund-Nachrendern fehlender 3D-Schnappschüsse: eine einzelne nicht ladbare Datei blockierte zuvor die gesamte Warteschlange dauerhaft; fehlerhafte Dateien werden jetzt übersprungen. Jede beendete Hintergrund-Rendering-Instanz gibt ihren WebGL-Kontext jetzt explizit frei
- Sammlungen: in den Papierkorb verschobene Modelle blieben in der Sammlungs-Detailansicht sichtbar und wurden in der Modellanzahl mitgezählt; Sidebar-Filter verließen die Sammlungsansicht nicht beim Anklicken; die Listenansicht zeigte bei aktiver Sammlung weiterhin den kompletten Katalog

## [0.4.0] - 2026-09-11

### Added

- Komfort-Ansicht als Alternative zur bestehenden kompakten Oberfläche: deutlich größere Schrift, Grafiken und Bedienelemente (Karten-Layout angelehnt an printables.com/model), umschaltbar im Einstellungen-Panel, Standard bleibt die kompakte Ansicht
- Favorit-Merkmal je Modell (Herz-Icon), in beiden Ansichten sichtbar
- Import-Button in der Kopfzeile öffnet jetzt immer direkt das Dropdown-Menü statt eines Split-Buttons mit Sofort-Aktion

### Security

- `quick-xml` von 0.36.2 auf 0.41.0 angehoben: schließt zwei Denial-of-Service-Schwachstellen (RUSTSEC-2026-0194, RUSTSEC-2026-0195, je CVSS 7.5/Hoch — quadratische Laufzeit bei doppelten Attributnamen bzw. unbegrenzte Speicherallokation bei Namespace-Deklarationen), erreichbar über eine präparierte `.3mf`-Datei beim normalen Import. Gefunden im Security-Review vom 2026-09-11 (`docs/security/security-review-2026-09-11.md`), per `cargo audit` bestätigt behoben

### Fixed

- Komfort-Ansicht wirkte in Kopfzeile, Seitenleiste, Listenansicht, Filament-Lager, Kontextmenü, Aufräum-Dialog und Import-Banner überhaupt nicht: Tailwind kompilierte die Klasse `text-[var(--font-size-X)]` als Text-*Farbe* statt Schriftgröße, behoben durch expliziten Typ-Hinweis `text-[length:var(--font-size-X)]`; zusätzlich nutzten Ordner-/Tag-/Creator-Namen in der Seitenleiste bislang Tailwinds feste `text-xs`-Klasse statt eines Tokens (neuer Token `--font-size-item`)
- Sidebar-Einträge (Ordner/Tags/Creators) skalierten nicht mit der Komfort-Ansicht

## [0.3.0] - 2026-09-10

### Added

- Filament-Lager: eigenständige Spulenverwaltung (Material, Hersteller, Farbe, Durchmesser, Ursprungs-/Restgewicht, Preis, Bild-Upload) über ein neues Header-Icon erreichbar, unabhängig vom Modell-Katalog
- Vier kleine Katalog-Erweiterungen: Druckstatus-Toggle + aus Volumen/Material geschätztes Gewicht pro Modell, Sortierung nach "Zuletzt angesehen" + NEU-Badge für kürzlich importierte Modelle, Creators als eigene Sidebar-Filterkategorie (aus dem beim 3MF-Import geparsten Designer-Metadatum), automatische Erkennung exakter Datei-Duplikate beim Import (SHA-256-Inhalts-Hash) mit Zusammenfassungsmeldung
- Modell-Thumbnails im Raster: Bild-Priorität eigenes Upload > eingebettetes 3MF-Thumbnail > automatisch erzeugter 3D-Snapshot > Platzhalter; zusätzlich pro Modell eine Quelle als Link hinterlegbar
- Tags- und Creators-Sektionen in der Sidebar sind einzeln einklappbar
- Warteschlange ("als Nächstes drucken"): geordnete, per Drag & Drop sortierbare Liste in eigener einklappbarer Sidebar-Sektion; automatisches Entfernen beim Markieren als gedruckt
- Gespeicherte Filter: aktuelle Kombination aus Ordner/Tag/Creator/Suche/Sortierung unter einem Namen speichern, per Klick wieder anwenden
- Aufräum-Vorschläge: manuell auslösbarer Katalog-Scan findet verwaiste Dateipfade und Bestands-Duplikate; Ergebnis-Dialog mit Einzelauswahl, ältestes Duplikat je Gruppe bleibt vorausgewählt

### Fixed

- Aufräum-Vorschläge: eine Datei, die gleichzeitig verwaist UND Teil einer Duplikat-Gruppe war, konnte im Auswahl-Dialog als "wird behalten" markiert und trotzdem gelöscht werden — verwaiste Dateien werden jetzt vor der Duplikat-Gruppierung ausgeschlossen; zusätzlich brach das Löschen bei einem Dateisystemfehler die ganze Auswahl vorzeitig ab statt einzelne Fehler zu überspringen
- Warteschlange: Drag & Drop zum Neusortieren reagierte nicht — natives HTML5-Drag&Drop kollidierte unter WebKitGTK mit Tauris Fenster-Ebene-Erkennung für Datei-Import per OS-Drop; auf reine Maus-Events umgestellt

## [0.2.0] - 2026-09-09

### Added

- Vollständige Mehrsprachigkeit (Deutsch/Englisch/Spanisch/Französisch): eigenes Context-basiertes i18n-System ohne externe Bibliothek, `Translations`-Interface erzwingt Vollständigkeit der Wörterbücher zur Compile-Zeit, Sprachumschalter im Einstellungen-Panel, Persistenz in localStorage
- Lokalisierte Formatierung (Datum, Uhrzeit relativ, Dateigröße, Volumen, Abmessungen) über `Intl`-APIs im Frontend
- Google-Drive-Anbindung: OAuth2-PKCE-Verbindung, Token-Speicherung im OS-Schlüsselbund, Datei-/Ordnerauswahl über Googles offizielles Picker-Widget, Import mit Duplikat-Erkennung und Sync-Status-Anzeige je Datei (später in 0.5.0 wieder vollständig entfernt)
- Native Rust-seitige Geometrie-Extraktion für die 3D-Vorschau: ZIP-Entpacken und Mesh-Parsing laufen jetzt vollständig im Backend statt im Frontend über three.js-Loader/`DOMParser`
- "In Slicer öffnen": Nutzer hinterlegt beliebig viele eigene Slicer-Programmpfade, Split-Button für Hauptauswahl/Wechsel, Kontextmenü-Eintrag für den zuletzt genutzten Slicer
- Hochladen zu Google Drive: lokale Dateien lassen sich zu Google Drive hochladen, inkl. Zielordner-Auswahl über das Picker-Widget

### Changed

- Backend liefert nur noch rohe, unformatierte Modelldaten (`ModelFileDto`); serverseitige, deutsch-only Formatierung entfernt und durch frontendseitige, sprachabhängige Formatierung ersetzt
- Google-Drive-OAuth-Scope `drive.readonly` entfernt, nur noch `drive.file` + `userinfo.email`: `drive.readonly` ist ein "restricted scope" und hätte für die Google-Verifizierung ein kostenpflichtiges CASA-Sicherheitsaudit erfordert

### Fixed

- CSS-`@import`-Reihenfolge und Rust-Abhängigkeiten fixiert
- Verwaiste Tags (letzte Datei mit diesem Tag gelöscht) blieben in Datenbank und Sidebar stehen
- Aus Google Drive importierte Dateien übernahmen den internen Cache-Dateinamen statt des echten Drive-Dateinamens
- Google-Drive-Konto verbinden/trennen und jeder authentifizierte Cloud-Aufruf blockierten kurzzeitig den Tokio-Worker- bzw. IPC-Dispatch-Thread durch synchrones D-Bus-IPC zum Schlüsselbund — über `spawn_blocking` entkoppelt

## [0.1.0] - 2026-09-08

Initialer Scaffold und Kern-Katalog.

### Added

- Tauri-Projektgerüst mit integriertem React/TypeScript/Tailwind-UI-Paket
- Eigenständiges 3MF-Parsing-Modul (OPC-Container, Model-XML, eingebettetes Thumbnail)
- Eigenständiges STL-Parsing-Modul (ASCII und Binär)
- SQLite-Katalogdatenbank (Schema, Modelle, Repository)
- Automatische Tagging-Heuristiken (Dateiname, Geometrie-Merkmale)
- Tauri-Command-Bridge: Frontend nutzt echte Backend-Daten statt Beispieldaten
- 3D-Live-Vorschau im Detailbereich mittels three.js
- Import-Workflow: Dateidialog, Ordnerauswahl, Drag-and-Drop
- Löschfunktion für Modelle mit Bestätigungsdialog und Kontextmenü

### Fixed

- Sortierung nach Datum und Dateigröße korrigiert

## Known Limitations (Stand 0.6.0)

- Keine Cloud-Anbindung (siehe "Removed" in 0.5.0) — nur lokaler Dateisystem-Import
- "In Slicer öffnen" unterstützt macOS nicht (`.app`-Bundles benötigen einen anderen Start-Mechanismus als Windows/Linux-Executables)
- Windows-`.msi`-Build ist manuell verifiziert, aber nicht Teil einer automatisierten Pipeline; macOS-Paket (`.dmg`) sowie Code-Signing für beide Plattformen stehen noch aus
- CI/CD-Pipeline (GitHub Actions) noch nicht eingerichtet
- Keine automatisierten Frontend-Tests (nur Backend/Rust-Tests)
