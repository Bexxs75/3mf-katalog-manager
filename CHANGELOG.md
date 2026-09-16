# Changelog

Alle nennenswerten Änderungen an diesem Projekt werden hier dokumentiert.
Format angelehnt an [Keep a Changelog](https://keepachangelog.com/de/1.0.0/), Versionierung nach [Semantic Versioning](https://semver.org/lang/de/).

Rückwirkend versioniert am 2026-09-12: das Projekt lief bis dahin komplett unter der Scaffold-Versionsnummer `0.1.0`, ohne dass Meilensteine markiert wurden. Die folgenden Versionsgrenzen wurden nachträglich anhand von Entwicklungs-Tagen und natürlichen Feature-Abschlüssen (jeweils an einem Dokumentations-Commit) gezogen, keine davon wurde zum jeweiligen Zeitpunkt live getaggt oder veröffentlicht.

## [Unreleased]

## [0.7.4] - 2026-09-16

### Fixed

- Touchpad-Pinch-Geste zoomte das ganze Anwendungsfenster statt nur die 3D-Vorschau ([GitHub Issue #6](https://github.com/Bexxs75/3mf-katalog-manager/issues/6)): Pinch-to-Zoom kommt im WebView als `wheel`-Event mit `ctrlKey: true` an (Browser-Konvention), das der Viewer im `ModelViewer.tsx` per `OrbitControls` bereits selbst auf seinem Canvas abfängt, aber überall sonst in der App ohne Gegenmaßnahme zum nativen Seiten-Zoom führte. Ein globaler `wheel`-Listener verhindert jetzt das Standardverhalten bei `ctrlKey`, unabhängig davon wo in der App gerade gescrollt wird
- Listen- und Grid-Ansicht: Scrollen bis ans Ende ließ sich unter Linux (WebKitGTK) noch weiter über den Inhalt hinaus ziehen (elastischer Rubber-Band-Overscroll), statt am Ende zu stoppen ([GitHub Issue #5](https://github.com/Bexxs75/3mf-katalog-manager/issues/5)). `overscroll-behavior: contain` an den beiden Haupt-Scroll-Containern (Katalog- und Papierkorb-Ansicht in `App.tsx`) unterbindet das Nachgeben über die Content-Grenze hinaus

## [0.7.3] - 2026-09-13

### Fixed

- 3D-Vorschau: Modelle ließen sich per Maus nicht mehr sauber um die eigene stehende Achse drehen — 3MF/STL-Geometrie liegt Z-up vor (Druckplatte = XY-Ebene, Z = Druckhöhe), Three.js/OrbitControls gehen aber von Y-up aus. Dadurch lag die stehende Achse der Figur quer zur Kamera-Drehachse: freies Ziehen kippte sie seitlich um, statt sie wie einen Drehteller zu drehen. Die Modell-Geometrie wird beim Laden jetzt einmalig um -90° um die X-Achse gedreht, wodurch jede Drehung (Maus-Drag, Auto-Rotation, Pfeil-Buttons) die Figur aufrecht hält und dabei frei geneigt werden kann

## [0.7.2] - 2026-09-13

### Fixed

- Import vieler Dateien auf einmal (Ordner-Import) war spürbar langsam: jede importierte Datei öffnete und committete bisher ihre eigene Datenbank-Transaktion, und SQLite fsynct bei jedem Commit — bei einem Ordner mit vielen Modellen summierte sich das zu vielen einzelnen Festplatten-Synchronisationen nacheinander. Der komplette Batch läuft jetzt in einer einzigen Transaktion mit einem Commit am Ende

## [0.7.1] - 2026-09-13

### Security

- Path-Traversal in `create_folder`/`rename_folder` behoben (CWE-22, gefunden bei Review gegen ISO/IEC 27002 A.8.28): der Ordnername wurde ungeprüft in Pfad-Operationen übernommen, wodurch z. B. `../../etc/x` oder ein absoluter Pfad einen echten Verzeichnis-Vorgang weit außerhalb des Katalog-Ordnerbaums hätte auslösen können — bei `create_folder` direkt über das "+ Neuer Ordner"-Eingabefeld erreichbar. Neuer gemeinsamer Validierungs-Helfer lehnt Pfad-Trenner, `.`/`..` sowie leere Namen ab. Zusätzlich prüft `open_in_file_manager` jetzt, dass der übergebene Pfad ein echtes existierendes Verzeichnis ist

## [0.7.0] - 2026-09-13

### Added

- Materialkosten-Schätzung: bei Modellen mit echtem Slicer-Filamentverbrauch zeigt die Detailseite jetzt zusätzlich eine geschätzte Materialkosten-Summe, berechnet aus dem Verbrauch je Filament und dem Durchschnittspreis passender Spulen im Filament-Lager (Materialtyp-Abgleich, Farbe wird bewusst nicht berücksichtigt); fehlt ein Preis, wird das klar als "unbekannt" ausgewiesen statt als 0
- Druckprotokoll: auf der Modell-Detailseite lässt sich jetzt zusätzlich zum bestehenden Druckstatus-Toggle ein Protokoll mehrerer Druckversuche führen — Datum, optionale Notiz, optionales Foto pro Eintrag, bewusst unabhängig vom Druckstatus (keine automatische Ableitung in beide Richtungen)
- Katalog-Backup (Export/Import): neue Sektion im Einstellungen-Panel sichert die komplette Katalog-Datenbank plus Einstellungen (Theme, Sprache, Slicer-Liste, Ansicht) als ZIP-Datei; Import (mit Bestätigungsabfrage, da destruktiv) ersetzt den aktuellen Katalog sicher — die alte Datenbank wird nie gelöscht, nur als `.bak-<Zeitstempel>` beiseitegelegt, mit automatischer Wiederherstellung bei einem fehlgeschlagenen Import; nach erfolgreichem Import lädt die App automatisch neu, damit keine veralteten Modell-IDs im Katalog stehen bleiben
- Navigationsleiste (links, feste Icon-Leiste) ersetzt den bisherigen Katalog/Filament-Lager-Wechsel-Button im Header sowie das Papierkorb-Icon; die Zahnrad-Einstellungen wandern ebenfalls dorthin. Design als klickbarer HTML-Prototyp mit Nutzer-Feedback abgestimmt
- Echte Ordnerstruktur: Ordner im Katalog bilden jetzt echte Verzeichnisse auf der Platte ab. Import einer Ordnerstruktur legt automatisch eine passende Hierarchie in der Datenbank an (rekursiv, inkl. Unterordner), die Sidebar zeigt einen auf-/zuklappbaren Ordner-Baum statt der bisherigen (praktisch immer leeren) flachen Liste. Dateien und Ordner lassen sich per Drag & Drop **physisch** verschieben — inklusive rekursivem Pfad-Update für alle betroffenen Unterordner/Dateien und Schutz vor Verschieben in den eigenen Unterordner; neuer "+ Neuer Ordner"-Button legt echte Verzeichnisse an
- Sammlungen sind jetzt Teil der linken Sidebar (mit "alle anzeigen"-Galerie und Inline-Anlage) statt eines separaten Reiters im Inhaltsbereich
- Katalog-Speicherort-Ersteinrichtung: Dialog beim ersten Start (und jederzeit über die Einstellungen erreichbar) erklärt die neue Ordner-Bedeutung und bietet an, eine bestehende Ordnerstruktur zu übernehmen oder einen neuen, auch leeren Speicherort einzurichten — mit Hinweis, welche Dateitypen erfasst werden (.3mf/.stl) und dass bereits gepackte Archive (z. B. .zip) nicht berücksichtigt werden. "Dateien importieren" platziert Einzeldateien danach im gerade aktiven Ordner bzw. im konfigurierten Speicherort, statt immer in der Wurzel zu landen. Neuer Button "Ordner im Dateimanager öffnen" (Dialog und Einstellungen)

### Changed

- Kartenradius zwischen Katalog, Sammlungen-Galerie und Filament-Lager vereinheitlicht (10px, unabhängig von der UI-Dichte)
- Kopfzeile ("3MF Katalog Manager") geht jetzt über die volle Fensterbreite, die Navigationsleiste beginnt erst darunter
- Sidebar: Abschnitte "Creators" und "Gespeicherte Filter" entfernt (unnötig geworden), "Tags" steht jetzt vor der Warteschlange

### Fixed

- 3D-Live-Vorschau schlug bei sehr großen Modellen (mehrere hunderttausend bis über eine Million Vertices) im Release-Build stumm fehl ("Vorschau nicht verfügbar"): Tauri liefert große IPC-Binärantworten über einen separaten internen `fetch()` aus, den die Content-Security-Policy ohne explizite `connect-src`-Direktive blockierte. Betraf nur außergewöhnlich große Dateien, kleinere 3mf-Modelle waren nie betroffen

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

---

# Changelog (English)

All notable changes to this project are documented here.
Format based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), versioning follows [Semantic Versioning](https://semver.org/).

Versioned retroactively on 2026-09-12: the project ran entirely under the scaffold version number `0.1.0` until then, without marked milestones. The following version boundaries were drawn afterward based on development days and natural feature completions (each at a documentation commit); none of them was actually tagged or released live at the time.

## [Unreleased]

## [0.7.4] - 2026-09-16

### Fixed

- Touchpad pinch gesture zoomed the entire application window instead of just the 3D preview ([GitHub Issue #6](https://github.com/Bexxs75/3mf-katalog-manager/issues/6)): pinch-to-zoom arrives in the WebView as a `wheel` event with `ctrlKey: true` (browser convention), which the viewer already intercepts on its own canvas via `OrbitControls` in `ModelViewer.tsx`, but everywhere else in the app it fell through to native page zoom with no countermeasure. A global `wheel` listener now prevents the default behavior on `ctrlKey`, regardless of where in the app the gesture happens
- List and grid view: scrolling to the end could still be dragged further past the content on Linux (WebKitGTK), an elastic rubber-band overscroll instead of stopping at the end ([GitHub Issue #5](https://github.com/Bexxs75/3mf-katalog-manager/issues/5)). `overscroll-behavior: contain` on the two main scroll containers (catalog and trash view in `App.tsx`) now prevents scrolling past the content boundary

## [0.7.3] - 2026-09-13

### Fixed

- 3D preview: models could no longer be cleanly rotated around their own upright axis by dragging — 3MF/STL geometry is Z-up (build plate = XY plane, Z = print height), but Three.js/OrbitControls assume Y-up. This left the model's upright axis crosswise to the camera's rotation axis: free dragging tipped it onto its side instead of spinning it like a turntable. Model geometry is now rotated by -90° around the X axis once on load, so every rotation (mouse drag, auto-rotation, arrow buttons) keeps the figure upright while still allowing free tilting

## [0.7.2] - 2026-09-13

### Fixed

- Importing many files at once (folder import) was noticeably slow: every imported file used to open and commit its own database transaction, and SQLite fsyncs on every commit — for a folder with many models this added up to many individual disk syncs one after another. The whole batch now runs inside a single transaction with one commit at the end

## [0.7.1] - 2026-09-13

### Security

- Fixed path traversal in `create_folder`/`rename_folder` (CWE-22, found during a review against ISO/IEC 27002 A.8.28): the folder name was used in path operations without validation, so a value like `../../etc/x` or an absolute path could have triggered a real directory operation far outside the catalog's folder tree — directly reachable via the existing "+ New folder" input field for `create_folder`. A new shared validation helper now rejects path separators, `.`/`..`, and empty names. `open_in_file_manager` additionally now verifies the given path is a real, existing directory before opening it

## [0.7.0] - 2026-09-13

### Added

- Material cost estimate: for models with real slicer filament usage, the detail page now also shows an estimated material cost, computed from the consumption per filament and the average price of matching spools in the filament inventory (matched by material type; color is deliberately not considered); if no price is found it's clearly shown as "unknown" rather than 0
- Print log: the model detail page can now keep a log of multiple print attempts in addition to the existing print-status toggle — date, optional note, optional photo per entry, deliberately independent of print status (no automatic derivation in either direction)
- Catalog backup (export/import): a new section in the settings panel backs up the entire catalog database plus settings (theme, language, slicer list, view) as a ZIP file; import (confirmation-gated, since destructive) safely replaces the current catalog — the old database is never deleted, only set aside as a timestamped `.bak-<timestamp>` file, with automatic restoration if the import fails; after a successful import the app automatically reloads so no stale model IDs remain in the catalog
- Navigation rail (fixed left icon bar) replaces the previous catalog/filament-storage switch button in the header as well as the trash icon; the settings gear moved there too. Design was aligned with the user via a clickable HTML prototype
- Real folder structure: folders in the catalog now mirror real directories on disk. Importing a folder structure automatically builds a matching hierarchy in the database (recursive, including subfolders); the sidebar shows an expandable/collapsible folder tree instead of the previous (practically always-empty) flat list. Files and folders can be dragged to **physically** move them — including recursive path updates for every affected subfolder/file and protection against moving a folder into its own descendant; a new "+ New folder" button creates real directories
- Collections are now part of the left sidebar (with a "view all" gallery and inline creation) instead of a separate tab in the content area
- Catalog storage location setup: a dialog on first launch (reachable anytime afterward via settings) explains the new meaning of folders and offers to adopt an existing folder structure or set up a new, even empty, storage location — noting which file types are captured (.3mf/.stl) and that already-packed archives (e.g. .zip) are not considered. "Import files" then places single files into the currently active folder or the configured storage location, instead of always landing at the root. New "Open folder in file manager" button (in the dialog and in settings)

### Changed

- Unified card corner radius across the catalog grid, collections gallery, and filament inventory (10px, regardless of UI density)
- The header ("3MF Katalog Manager") now spans the full window width; the navigation rail starts only below it
- Sidebar: removed the "Creators" and "Saved filters" sections (no longer needed); "Tags" now appears before the queue

### Fixed

- Live 3D preview silently failed for very large models (several hundred thousand to over a million vertices) in the release build ("preview unavailable"): Tauri delivers large binary IPC responses via a separate internal `fetch()`, which the Content Security Policy blocked without an explicit `connect-src` directive. Only affected exceptionally large files; smaller 3mf models were never affected

## [0.6.0] - 2026-09-13

First tagged release. The individual sections below stem from several development days (2026-09-12 and 2026-09-13), see the note above on retroactive versioning.

### Added

- Filament usage from sliced OrcaSlicer/Bambu Studio 3mf files: reads `Metadata/slice_info.config` on import and shows the real, slicer-calculated weight on the model detail page instead of the previous rough estimate from volume × material density — including a breakdown per build plate and filament (type, color, grams, meters). New "Rescan metadata" button re-reads an already-catalogued file from its path if it has since been re-sliced and overwritten in OrcaSlicer/Bambu Studio (incl. a visible success message, error/success scoped per model). Deliberately out of scope: live printer integration, headless slicing, automatic deduction from the filament inventory
- Filament inventory redesign (following user feedback that it was "not good/intuitive" + a mockup review with 3 proposals): new **storage location** field (autocomplete from previously used locations); local **dashboard/list** toggle directly in the filament inventory (card grid with stock bar/status color, or a sortable table in the style of warehouse management software); stats bar (total spools, remaining stock, occupied storage slots, low/empty) and status filter chips (low/empty); new theme tokens `--good`/`--warn`/`--crit` for standalone status colors; creation form as a side panel with sections (image/identification/storage/stock) instead of the previous cramped 7-field bar, incl. a live preview of the stock bar; "spool count" stepper when creating new spools creates several independent spools with the same values at once
- Filament inventory: curated autocomplete suggestion list of common FDM materials (PLA, PETG, ABS, ASA, TPU, Nylon, PC, PEEK, carbon-fiber variants, etc.) and filament manufacturers (Bambu Lab, Prusament, Polymaker, eSUN, SUNLU, Fillamentum, ColorFabb, etc.) for the material/manufacturer fields; own theme-consistent `AutocompleteInput` component
- New app icon: isometric 3D-printing layer cube, colors computed directly from the real theme tokens (oklch → sRGB), replaces the generic Tauri default icon on all platforms
- Content Security Policy enabled (`tauri.conf.json`)
- Windows `.msi` build verified end-to-end on a Windows 11 VM (QEMU/KVM): build, installation, icon extraction from the installed `.exe` for visual verification

### Changed

- App display name (window title, installer/taskbar name) corrected from "mf-katalog-manager" to "3MF Katalog Manager" — a Cargo crate name may not start with a digit, which is why the very first project scaffold chose "mf-katalog-manager" instead of "3mf-katalog-manager". App identifier and data folder are unchanged
- Source-URL editing in the detail area (compact/comfort side panel, model detail page) consolidated into a shared `useEditableSourceUrl` hook, previously triplicated with identical logic

### Security

- Full code review (senior-dev review against OWASP Top 10 / CWE / ISO 27002 A.8.28), documented in [GitHub Issue #1](https://github.com/Bexxs75/3mf-katalog-manager/issues/1): the Content Security Policy was completely disabled (`csp: null`) — now set to a restrictive policy (`default-src 'self'`, `img-src` allows `data:` for base64 thumbnails); a model's source URL now only accepts `http(s)://` links server-side (previously allowed a `javascript:`/`data:` value to land as a clickable `<a href>` in the WebView); "open in slicer" now checks before launching that the given path points to an existing, executable file instead of passing any string to `process::Command` unchecked
- App data directory (`0700`) and `catalog.db` (`0600`) are hardened on startup under Unix (ISO 27002 A.8.28)
- Various cleanup from the same review (low priority, no security impact): removed the dead Tauri demo command (`greet`), replaced the deprecated `quick_xml` attribute API (`unescape_value` → `normalized_value`), the STL parser now reads the facet count via the same bounds-checked access as the other values instead of implicitly relying on call order, several unused code paths removed or marked "test-only"/"kept deliberately"

### Fixed

- Rotation buttons in the detail page's 3D viewer: hover effect (`bg-white/10`) was practically invisible in the light theme, now theme-consistent via a token; missing `cursor-pointer` added to all three buttons
- Collections, six deferred minor findings: model card count display didn't use real plural forms; dead i18n key `backToCollectionsLabel` removed; multi-select persisted when switching between views/collections; "import folder as collection" didn't detect file duplicates cataloged under a different path; drag-to-reorder started without a tolerance threshold from anywhere on the card; `list_collection_files` loaded per model individually (N+1) instead of batched
- Filament autocomplete: the first version used a native `<input list>`/`<datalist>`, whose suggestion popup is rendered by the operating system/WebKit and doesn't adapt to the dark app theme (appeared as a white system popup) — replaced with the app's own theme-consistent `AutocompleteInput` component
- The "spool count" field in the filament creation form showed both its own -/+ buttons and the number field's native browser spinner arrows at the same time — native spinners hidden via CSS

## [0.5.0] - 2026-09-12

### Added

- File/folder dialogs use the native XDG desktop portal instead of a generic GTK dialog — on KDE, for example, the real Kirigami dialog with Dolphin's folder sorting now appears
- Model detail page: full-screen view (double-click a model) with a large 3D preview, all metadata, and build-plate count for Bambu Studio/OrcaSlicer 3MF files (best-effort detection); the existing side panel remains for quick single selection
- Rotation controls in the detail page's 3D viewer: play/pause button for continuous auto-rotation plus ←/→ buttons for 15° steps, in addition to free mouse dragging; auto-rotation automatically pauses on manual mouse interaction
- Trash instead of immediate hard delete: deleted models are first moved to a trash directory and remain recoverable there (own trash view via a new header icon with a count badge, incl. 3D preview); permanent delete and "empty trash" then permanently remove the file and catalog entry
- Multi-select in the catalog overview: checkbox per card/row, "select all" (respects active filters), action bar for queue, print status, and delete (with confirmation) across multiple selected models at once
- Sidebar: tags and creators now start collapsed and show as compact, wrapping chips instead of long line lists; tags with only one match are hidden by default (actively selected tags remain visible)
- Context menu (right-click on a card) now also offers "printed"/"not printed" directly
- Automatic slicer detection: scans known installation locations for Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer, and UltiMaker Cura at startup (PATH, `/opt`, Flatpak exports, common AppImage locations on Linux; `Program Files`/`Program Files (x86)` on Windows) and adds matches automatically (labeled "auto-detected"); manual addition for custom forks remains available
- "Preferred view" setting (gear menu): determines whether catalog cards and the model detail page default to the embedded file image or a rendered 3D view; with "rendered view", the app automatically and sequentially re-renders missing 3D snapshots in the background — a single file that fails to load doesn't block the rest of the queue
- Collections: a third organizational mechanism alongside folders and tags — a many-to-many association like tags, but with a manually definable order (drag & drop). Created via the multi-select action bar ("add to collection") or via the new "import folder as collection" import option. A new "Collections" tab next to "All Models" opens a card overview of all collections (rename/delete possible); clicking a collection shows its models in a fixed order. Models moved to the trash are correctly hidden in collections and automatically reappear after being restored

### Changed

- A file's image sources (own upload, embedded 3MF/STL thumbnail, rendered 3D snapshot) are no longer prioritized server-side into a fixed `displayImage`, but delivered separately to the frontend — the new "preferred view" setting decides the priority
- Model detail page: viewer column (image/3D preview) is about 25% larger on very wide screens (e.g. 3440×1440), the metadata column on the right stays unchanged
- Sort dropdown styled independently in the dark theme (popover pattern like the existing slicer selector) instead of a native `<select>` element with a light system background and wrong font

### Removed

- Cloud integration (Google Drive) removed entirely: backend module `src-tauri/src/cloud/` incl. all 7 Tauri commands, the `tauri-plugin-opener` dependency, and `oauth2`/`keyring`/`reqwest`/`async-trait`/`tokio` from `Cargo.toml`; frontend UI (cloud accounts sidebar section, cloud import option, sync status displays, origin badges) and related i18n keys removed. `cloud_accounts` table and the `sync_status`/`cloud_id` part of the `origin` value range removed from `schema.sql` or reduced to `'local'` (only effective for new installations — existing databases keep the inert `sync_status`/`cloud_id` columns, since this project has no `DROP COLUMN` migration pattern). Reason: the Google Drive integration remained too unstable/error-prone for everyday use despite repeated rework, and consumed too much attention from more important topics

### Fixed

- White screen on startup on systems with an NVIDIA graphics card (proprietary driver): WebKitGTK's default DMA-BUF hardware rendering is incompatible with the NVIDIA driver (`Failed to create GBM buffer`) — the app now automatically sets the required environment variable on startup
- External slicers (e.g. OrcaSlicer) didn't start reliably from the app when the configured path pointed to an `AppRun` launch script: the app was passing its own AppImage-internal environment unfiltered to the launched process — these variables are now stripped before launching external programs
- Bambu Studio wasn't found by automatic slicer detection on Arch/CachyOS AUR installations, since the AUR package installs the binary as `bambustudio` — added as a third name variant
- Trash bug: if a file's catalog path no longer pointed to a reachable location, deleting it previously caused an immediate, unrecoverable loss instead of moving it to the trash
- Automatic background re-rendering of missing 3D snapshots: a single file that failed to load previously blocked the entire queue permanently; failing files are now skipped. Each finished background rendering instance now explicitly releases its WebGL context
- Collections: models moved to the trash remained visible in the collection detail view and were still counted in the model count; sidebar filters didn't leave the collection view when clicked; the list view still showed the entire catalog while a collection was active

## [0.4.0] - 2026-09-11

### Added

- Comfort view as an alternative to the existing compact interface: significantly larger text, graphics, and controls (card layout inspired by printables.com/model), toggleable in the settings panel, compact view remains the default
- Favorite flag per model (heart icon), visible in both views
- The import button in the header now always opens the dropdown menu directly instead of a split button with an immediate default action

### Security

- `quick-xml` bumped from 0.36.2 to 0.41.0: closes two denial-of-service vulnerabilities (RUSTSEC-2026-0194, RUSTSEC-2026-0195, each CVSS 7.5/High — quadratic runtime on duplicate attribute names and unbounded memory allocation on namespace declarations, respectively), reachable via a crafted `.3mf` file during a normal import. Found in the 2026-09-11 security review (`docs/security/security-review-2026-09-11.md`), confirmed fixed via `cargo audit`

### Fixed

- Comfort view had no visible effect at all in the header, sidebar, list view, filament inventory, context menu, cleanup dialog, and import banner: Tailwind compiled the class `text-[var(--font-size-X)]` as a text *color* instead of font size, fixed with an explicit type hint `text-[length:var(--font-size-X)]`; additionally, folder/tag/creator names in the sidebar previously used Tailwind's fixed `text-xs` class instead of a token (new token `--font-size-item`)
- Sidebar entries (folders/tags/creators) didn't scale with the comfort view

## [0.3.0] - 2026-09-10

### Added

- Filament inventory: standalone spool management (material, manufacturer, color, diameter, original/remaining weight, price, image upload) accessible via a new header icon, independent of the model catalog
- Four small catalog extensions: print-status toggle + weight per model estimated from volume/material, sorting by "last viewed" + NEW badge for recently imported models, creators as their own sidebar filter category (from the designer metadata parsed on 3MF import), automatic detection of exact file duplicates on import (SHA-256 content hash) with a summary message
- Model thumbnails in the grid: image priority own upload > embedded 3MF thumbnail > automatically generated 3D snapshot > placeholder; plus an optional source link per model
- Tags and creators sections in the sidebar are individually collapsible
- Queue ("print next"): ordered, drag-and-drop sortable list in its own collapsible sidebar section; automatically removed when marked as printed
- Saved filters: save the current combination of folder/tag/creator/search/sort under a name, reapply with a click
- Cleanup suggestions: manually triggered catalog scan finds orphaned file paths and stock duplicates; result dialog with individual selection, the oldest duplicate per group stays pre-selected

### Fixed

- Cleanup suggestions: a file that was simultaneously orphaned AND part of a duplicate group could be marked "keep" in the selection dialog and still get deleted — orphaned files are now excluded before duplicate grouping; additionally, a filesystem error during deletion previously aborted the entire selection early instead of skipping individual errors
- Queue: drag & drop reordering didn't respond — native HTML5 drag & drop collided under WebKitGTK with Tauri's window-level detection for OS file-drop import; switched to plain mouse events

## [0.2.0] - 2026-09-09

### Added

- Full multilingual support (German/English/Spanish/French): own context-based i18n system without an external library, `Translations` interface enforces dictionary completeness at compile time, language switcher in the settings panel, persisted in localStorage
- Localized formatting (date, relative time, file size, volume, dimensions) via `Intl` APIs in the frontend
- Google Drive integration: OAuth2 PKCE connection, token storage in the OS keyring, file/folder selection via Google's official picker widget, import with duplicate detection and per-file sync status display (later removed entirely again in 0.5.0)
- Native Rust-side geometry extraction for the 3D preview: ZIP extraction and mesh parsing now run entirely in the backend instead of in the frontend via three.js loaders/`DOMParser`
- "Open in slicer": user configures any number of custom slicer program paths, split button for main selection/switching, context menu entry for the most recently used slicer
- Upload to Google Drive: local files can be uploaded to Google Drive, including target folder selection via the picker widget

### Changed

- Backend now only delivers raw, unformatted model data (`ModelFileDto`); server-side, German-only formatting removed and replaced with frontend-side, language-dependent formatting
- Google Drive OAuth scope `drive.readonly` removed, only `drive.file` + `userinfo.email` remain: `drive.readonly` is a "restricted scope" and would have required a paid CASA security audit for Google verification

### Fixed

- Fixed CSS `@import` order and Rust dependencies
- Orphaned tags (last file with that tag deleted) remained stuck in the database and sidebar
- Files imported from Google Drive used the internal cache filename instead of the real Drive filename
- Connecting/disconnecting a Google Drive account and every authenticated cloud call briefly blocked the Tokio worker/IPC dispatch thread via synchronous D-Bus IPC to the keyring — decoupled via `spawn_blocking`

## [0.1.0] - 2026-09-08

Initial scaffold and core catalog.

### Added

- Tauri project scaffold with an integrated React/TypeScript/Tailwind UI package
- Standalone 3MF parsing module (OPC container, model XML, embedded thumbnail)
- Standalone STL parsing module (ASCII and binary)
- SQLite catalog database (schema, models, repository)
- Automatic tagging heuristics (filename, geometry features)
- Tauri command bridge: frontend uses real backend data instead of sample data
- Live 3D preview in the detail area via three.js
- Import workflow: file dialog, folder selection, drag-and-drop
- Delete function for models with a confirmation dialog and context menu

### Fixed

- Fixed sorting by date and file size

## Known Limitations (as of 0.6.0)

- No cloud integration (see "Removed" in 0.5.0) — local filesystem import only
- "Open in slicer" doesn't support macOS (`.app` bundles need a different launch mechanism than Windows/Linux executables)
- The Windows `.msi` build is manually verified but not part of an automated pipeline; a macOS package (`.dmg`) and code signing for both platforms are still outstanding
- CI/CD pipeline (GitHub Actions) not yet set up
- No automated frontend tests (backend/Rust tests only)
