# Changelog

Alle nennenswerten Änderungen an diesem Projekt werden hier dokumentiert.
Format angelehnt an [Keep a Changelog](https://keepachangelog.com/de/1.0.0/), Versionierung nach [Semantic Versioning](https://semver.org/lang/de/).

Rückwirkend versioniert am 2026-09-12: das Projekt lief bis dahin komplett unter der Scaffold-Versionsnummer `0.1.0`, ohne dass Meilensteine markiert wurden. Die folgenden Versionsgrenzen wurden nachträglich anhand von Entwicklungs-Tagen und natürlichen Feature-Abschlüssen (jeweils an einem Dokumentations-Commit) gezogen, keine davon wurde zum jeweiligen Zeitpunkt live getaggt oder veröffentlicht.

## [Unreleased]

### Added

- **Druckeranbindung (Klipper/Moonraker):** Neuer Einstellungs-Reiter „Drucker“ mit dem Schalter „Druckeranbindung“ (standardmäßig aus). Pro Drucker lassen sich unter „Drucker verwalten“ Typ und Adresse eintragen und testen. Die App fragt angebundene Drucker beim Start, alle 5 Minuten und per Knopf nach beendeten Drucken, liest den tatsächlich geförderten Filamentverbrauch (bei abgebrochenen Drucken nur bis zum Abbruch) und rechnet ihn in Gramm um. Im Filament-Lager erscheint dann „N neue Drucke warten auf Bestätigung“: Im Dialog lassen sich pro Druck Spule (vorgeschlagen: die im Drucker eingelegte) und Katalogmodell (Vorschlag über den Dateinamen) prüfen, bestätigen oder ignorieren. Erst beim Bestätigen wird abgebucht; mit Modell entsteht zusätzlich ein Druckprotokoll-Eintrag, und das Modell wird als gedruckt markiert. Abgebucht werden nur Drucke, die nach dem ersten Verbinden enden. Die App spricht nur mit selbst eingetragenen Adressen im Heimnetz, liest nur und verändert am Drucker nichts. Getestet mit einem Sovol SV08. Resin-Flaschen werden von der Druckeranbindung nie vorgeschlagen oder abgebucht.

### Changed

- Druckeranbindung: Das passende Katalogmodell wird auch vorgeschlagen, wenn der Slicer Druckermodell oder Datum vor den Dateinamen setzt oder die Platte als „plate(01)“ anhängt (z. B. Anycubic Kobra S1 mit Rinkhals, Dank an einen anonymen Tester).
- Die Update-Prüfung erkennt jetzt auch Vorschauversionen (z. B. „0.14.0-gharac") korrekt als älter als das zugehörige fertige Release, statt nie ein Update zu melden.
- Interne Qualitätsprüfungen: Testcode ist jetzt clippy-sauber, dazu ein automatischer CI-Prüflauf (TypeScript, Tests, Clippy) sowie eine wöchentliche Abhängigkeits-Sicherheitsprüfung (cargo-deny, npm audit).

## [0.13.1] - 2026-09-25

### Added

- Filament-Lager: **Resin**. Oben im Lager schaltet „Filament | Resin" zwischen Spulen und Resin-Flaschen um; die App merkt sich die Auswahl. Übersicht, Liste, Suche, Filter und Kennzahlen gelten für die gewählte Art („Flaschen gesamt", Mengen in ml). Resin-Flaschen zeigen ein Flaschen-Symbol, den Rest in ml und die Flaschengröße statt des Durchmessers; der Status (vorrätig/niedrig/leer) folgt denselben Regeln. Über „− Verbrauch" buchst du verbrauchte Milliliter ab (auf 0,1 ml genau, nie unter 0). Neue Einträge bekommen die Art des gerade gewählten Bereichs (kein Umschalter im Formular), Material und Hersteller schlagen passende Werte vor; bei Resin heißt die Menge „Inhalt (ml)", der Durchmesser entfällt. Resin kommt nie in ein Druckerfach und zählt nicht bei „Reicht das Filament?", der Materialkosten-Schätzung, dem Filament-Symbol der Warteschlange und den Gramm-Summen. Alle bisherigen Einträge bleiben Filament; auch ältere Sicherungen lassen sich weiter einspielen.
- Filament-Lager: **Nachkaufen**. Jede Karte hat unten links den Knopf „＋ Nachkaufen", jede Zeile der Listenansicht einen „＋"-Knopf. Ein kleines Fenster legt 1 bis 20 neue, volle Spulen bzw. Flaschen mit denselben Daten an (Art, Material, Hersteller, Farbname, Farbwert, Bild, Durchmesser). Menge, Preis je Stück und Lagerort sind mit den Werten der Vorlage vorbelegt (Lagerort: ihr Stammplatz, falls sie gerade im Drucker steckt) und lassen sich vorher ändern. Neue Einträge liegen immer im Lager. Alles wird in einem Schritt angelegt: Schlägt etwas fehl (z. B. weil die Vorlage inzwischen gelöscht wurde), entsteht kein einziger. Die neuen Karten sind danach kurz grün umrandet, eine Meldung nennt die Anzahl. Escape oder ein Klick daneben schließt das Fenster, ohne etwas anzulegen.
- Filament-Lager: Ein **Doppelklick** auf eine Karte oder eine Tabellenzeile öffnet das Bearbeiten-Formular (wie ✎). Doppelklicks auf Knöpfe lösen das nicht aus.
- Filament-Lager: Restgewichte werden auf 0,1 g genau gespeichert und angezeigt.

### Changed

- Die 3D-Vorschau zeigt Modelle jetzt in Koralle statt in Beige. Sie heben sich damit im hellen wie im dunklen Design deutlich vom Hintergrund ab.

### Fixed

- Filament-Lager: Das Bildfeld im Spulenformular („Bild hierher ziehen oder klicken") nimmt jetzt auch per **Drag & Drop** hineingezogene Bilder an; bisher funktionierte nur Klicken. Erlaubt ist genau eine PNG-, JPG- oder WebP-Datei bis 5 MB (wie beim Klick-Upload). Das Feld hebt sich beim Darüberziehen hervor. Bei mehreren Dateien, einer anderen Datei oder einem zu großen Bild erscheint der Hinweis direkt unter dem Bildfeld, und „Speichern" lässt das Formular offen, bis ein anderes Bild gewählt oder der Hinweis geschlossen wird. Ein Drop im Filament-Lager startet nie einen Modell-Import.
- Filament-Lager, Listenansicht: Spulen ohne eigenes Bild zeigen ein Spulen-Symbol in der Spulenfarbe statt eines leeren Kästchens, das wie eine Checkbox aussah.

## [0.13.0] - 2026-09-24

### Added

- Archive direkt entpacken: Über „Importieren → Dateien…" oder per Drag & Drop hinzugefügte Archive (`.zip`, `.7z`, `.rar` (RAR4/RAR5), `.tar`, `.tar.gz`/`.tgz`, `.tar.bz2`/`.tbz2`, `.tar.xz`/`.txz`, `.tar.zst`/`.tzst`) öffnen einen Dialog: Zielordner (vorbelegt mit dem aktiven Katalogordner bzw. dem Speicherort), je Archiv ein eigener Unterordner, bei bereits vorhandenem Ordner Wahl zwischen „neuer nummerierter Ordner" und „zusammenführen" (vorhandene Dateien bleiben unverändert), optional Löschen der Original-Archive nach dem Entpacken (standardmäßig aus; ein Archiv wird nur gelöscht, wenn alle Einträge entpackt wurden – wurde etwas übersprungen, bleibt es liegen und das Ergebnis-Banner nennt den Grund). Enthaltene Bilder, Anleitungen und Lizenzdateien bleiben neben den Modellen erhalten. Der Ordner-Import entpackt weiterhin nichts.
- Schutz beim Entpacken: Einträge mit `..`, absoluten Pfaden oder Laufwerksbuchstaben (Zip-Slip) sowie Symlinks werden übersprungen. Ausführbare Dateien, Skripte und Verknüpfungen (`.exe`, `.bat`, `.ps1`, `.lnk`, `.url`, `desktop.ini`, `.desktop`, `.app` u. a.) werden grundsätzlich nicht entpackt. Kein Eintrag darf in geschützte Systembereiche schreiben, entpackte Dateien sind nie ausführbar, und die „Aus dem Internet"-Markierung des Downloads (Windows Mark-of-the-Web, macOS-Quarantäne) wird auf die entpackten Dateien übertragen. Pro Archiv gelten höchstens 2 GB entpackt und 10 000 Einträge (auch gegen gefälschte Größenangaben), dazu Speichergrenzen für die Dekompression. RAR-Einträge werden einzeln im Speicher entpackt; ein Eintrag über 1 GB lässt das ganze Archiv scheitern (bereits Entpacktes wird entfernt). Datei-Verweise in RAR-Archiven (mit `rar -oi` erzeugte Datei-Kopie-/Hardlink-Einträge) übernehmen nie den Inhalt der referenzierten Datei: Sie werden übersprungen, ergeben bei Größe 0 eine leere Datei oder lassen bei einem Prüfsummenfehler das ganze Archiv scheitern. Entpackt werden nur Archive, die tatsächlich über den Dateidialog oder per Drag & Drop hereingekommen sind (vom Backend selbst geprüft). Als Zielordner sind nur Katalogordner oder ein in einem Ordner-Auswahldialog der App gewählter Ordner möglich; Ordner, die geschützte Bereiche enthalten (z. B. das Home-Verzeichnis selbst), sind als Ziel gesperrt. Bei einem Fehler wird alles bereits Entpackte wieder entfernt.
- Filament-Lager: **Drucker & AMS-Fächer**. Drucker und ihre Mehrfarbeinheiten lassen sich über „Drucker verwalten" anlegen – mit Vorlagen für Bambu AMS/AMS lite/AMS HT, Creality CFS, Prusa MMU3, Anycubic ACE Pro und Spulenhalter oder als eigene Einheit mit 1–16 Fächern; Name und Reihenfolge änderbar; jeder neue Drucker erhält automatisch einen Spulenhalter (1 Fach), damit auch Drucker ohne AMS direkt eine Spule aufnehmen. Spulen kommen per Drag & Drop oder über das Menü am Fach hinein; bei einem belegten Fach kehrt die Spule, die schon im Fach steckt, an ihren Stammplatz zurück, von Fach zu Fach wird verschoben. Eingelegte Spulen stehen in einer eigenen Spalte rechts neben dem Lager und zählen weiter im Gesamtbestand. Beim Herausnehmen kehrt eine Spule automatisch an ihren Stammplatz zurück (Hinweis mit „Ändern"). Löschen einer Einheit oder eines Druckers schickt deren Spulen ebenfalls zurück.
- Filament-Lager: Spulen haben jetzt einen **Farbwert** (Palette mit 16 gängigen Filamentfarben oder Hex-Eingabe) zusätzlich zum Farbnamen; bekannte Farbnamen bestehender Spulen (z. B. „Schwarz", „Galaxy Black", „Dunkelblau") werden beim Update automatisch umgesetzt.
- Automatische Tags in der Oberflächensprache: Die beim Import vergebenen Tags „mehrteilig", „miniatur", „grossformat" und „mehrfarbig" erscheinen jetzt auf Englisch (multipart, mini, large, multicolor), Spanisch und Französisch, wenn die Oberfläche so eingestellt ist. Die Suche findet sie unter beiden Namen. Wer einen dieser Namen in einer anderen Sprache von Hand eingibt (z. B. „Multipart"), bekommt den vorhandenen Tag statt eines zweiten. Bereits vorhandene Tags mit solchen Namen (etwa aus Dateinamen) werden beim nächsten Start einmalig mit dem automatischen Tag zusammengelegt, auch in gespeicherten Filtern – außer den mehrdeutigen Wörtern „mini", „large" und „grande", die nur bei Eingabe von Hand dem automatischen Tag zugeordnet werden.
- Beim Vorschlagen von Tags aus Dateinamen werden jetzt auch englische, spanische und französische Füllwörter ignoriert (z. B. „untitled", „copia", „nouveau").
- Filament-Lager: **„Reicht das Filament?“** – für in Bambu Studio oder OrcaSlicer geslicete 3MF-Dateien vergleicht die Detailseite den Filamentbedarf (über alle Platten zusammengezählt) mit deinen Spulen: gleiches Material und ähnliche Farbe (Farbabstand CIEDE2000). Pro Filament zeigt sie „reicht“, „reicht mit Spulenwechsel“, „reicht nicht“ (mit Fehlmenge) oder „unklar“, dazu die passende Spule mit Restgewicht und ob sie im Drucker steckt (Drucker · Einheit · Fach) oder wo sie liegt. Die Druck-Warteschlange zeigt den Status als Symbol je Eintrag und rechnet von oben nach unten mit dem Gesamtbedarf. Es wird nichts abgebucht.
- **Werkzeuge in der Seitenleiste:** Ein neuer Abschnitt bündelt die Warteschlange (aufklappbar, wie bisher mit Ziehen zum Umsortieren), die Ansichten „Zuletzt angesehen“ (die 20 zuletzt angesehenen Modelle), „Neu hinzugefügt“ (letzte 7 Tage), „Favoriten“ (alle mit Herz markierten Modelle, alphabetisch) und „Duplikate“ (Modelle mit identischem Dateiinhalt, gruppiert) sowie die Aufräum-Vorschläge – jeweils mit Anzahl. Filament-Lager und Papierkorb bleiben wie bisher in der linken Leiste. Eine gewählte Ansicht erscheint oben als Filter-Chip und lässt sich mit Ordnern, Tags und Suche kombinieren.

### Fixed

- Robustheit: Das Umbenennen oder Verschieben von Ordnern beendet die App bei einer inkonsistenten Ordnerhierarchie (z. B. aus einem präparierten Katalog-Backup) nicht mehr abrupt, sondern bricht mit einer Fehlermeldung ab.
- Katalog-Sicherung: Beim Import werden jetzt auch die Datentypen aller Spalten geprüft; eine präparierte Sicherung mit falschen Werten (z. B. Text statt Zahl) wird abgelehnt, statt danach Ladefehler zu verursachen.
- Die Sortierung „Zuletzt angesehen“ berücksichtigt jetzt alle Modelle, nicht nur die in dieser Sitzung bereits geöffneten.

## [0.12.1] - 2026-09-23

### Fixed

- Die App startete nicht mehr (Absturz direkt nach dem Öffnen, ohne Fehlermeldung), wenn der Katalog noch Einträge aus der früheren Google-Drive-Anbindung enthielt (entfernt in v0.5.0). Die mit v0.11.0 eingeführte Datenbank-Migration für STEP/OBJ baut die Dateitabelle mit einer strengeren Prüfung neu auf und scheiterte an diesen Einträgen („CHECK constraint failed: origin IN ('local')"). Solche Einträge werden jetzt während der Migration in normale lokale Einträge umgewandelt – Tags, Sammlungen und alle übrigen Daten bleiben erhalten. Der Katalog selbst war nie beschädigt: die fehlgeschlagene Migration wurde jeweils vollständig zurückgerollt.
- Enthält außerdem die nach v0.12.0 bereits still in die Downloads eingespielten Build-Korrekturen: Linux-AppImage startet wieder auf Systemen mit neuerem Mesa (EGL-Absturz durch mitgebündelte, veraltete Wayland-/X11-Bibliotheken) und ein echtes Universal-DMG (Apple Silicon + Intel) für macOS.

## [0.12.0] - 2026-09-21

### Added

- STEP-Vorschau (`.stp`/`.step`) gibt es jetzt auf **allen drei Plattformen** — Linux, Windows und macOS. Für jede Plattform stehen zwei Downloadvarianten bereit: eine mit STEP-Vorschau (Dateiname mit `-step`-Zusatz, enthält Open CASCADE Technology/OCCT) und eine kleinere ohne (STEP-Dateien lassen sich weiterhin katalogisieren — Tags, Suche, Umbenennen, Papierkorb —, nur eben ohne 3D-Vorschau und automatisch ermittelte Abmessungen/Volumen/Körperzahl). Baugruppen werden bewusst als ein gemeinsames Vorschaunetz dargestellt; STEP lässt sich weiterhin nicht direkt im Slicer öffnen. Fehlerhafte oder zu große STEP-Dateien bleiben ohne automatisch ermittelte Metadaten im Katalog, statt den Import abzubrechen.
- Windows: MSI mit gebündelten OCCT-DLLs (`build-windows-step.yml`, OCCT-Build über ein gepinntes vcpkg-Manifest). macOS: DMG mit den OCCT-`.dylib`s direkt im App-Bundle (`build-macos-step.yml`, per `dylibbundler` auf `@executable_path`-relative Pfade umgeschrieben). Beide auf echten Zielsystemen ohne vorinstalliertes OCCT verifiziert — die App startet und die STEP-Vorschau funktioniert, auch ohne dass OCCT auf dem Zielrechner separat installiert ist.
- Die OCCT-Anbindung ist ein abschaltbares Cargo-Feature (`step-preview`, per Default aktiv) und lässt sich mit `--no-default-features` vollständig aus dem Build entfernen — genau das erzeugt die kleinere Downloadvariante ohne STEP-Vorschau.
- Automatische Slicer-Erkennung durchsucht jetzt auch unter macOS bekannte Installationsorte (Bambu Studio, OrcaSlicer u. a.), analog zur bisherigen Linux-/Windows-Unterstützung.
- Open CASCADE wird dynamisch unter LGPL-2.1 mit Open-CASCADE-Ausnahme eingebunden. Vollständige Lizenztexte, Quellenhinweise und der dokumentierte `opencascade-sys`-Kompatibilitäts-Fork werden mit dem Quellcode beziehungsweise Programmpaket bereitgestellt; Details stehen in `THIRD-PARTY-LICENSES.md`.

### Fixed

- Die Zeile "Drittanbieter-Lizenzen" im Info-Tab der Einstellungen brach bei Label und Wert auf zwei Zeilen um, anders als die übrigen Zeilen dort — Text auf allen vier Sprachen gekürzt.
- Vier Security-Unit-Tests für die Prüfung auf geschützte Systemverzeichnisse schlugen auf macOS fehl, weil `/home` dort ein Automounter-Symlink auf `/System/Volumes/Data/home` ist und `/var` ein Symlink auf `/private/var` — ein reiner Testbug (nicht-existente Testpfade wurden nur einseitig kanonisiert), kein Fehler in der eigentlichen Prüflogik.

## [0.11.0] - 2026-09-20

### Added

- STEP-Dateien (`.stp`/`.step`) können jetzt katalogisiert werden — wie 3MF/STL mit Tags, Suche, Umbenennen, Verschieben und Papierkorb, allerdings (noch) ohne 3D-Vorschau, da STEP parametrische CAD-Geometrie statt eines Dreiecksnetzes ist. STEP-Dateien lassen sich bewusst nicht direkt im Slicer öffnen, da die meisten Slicer kein rohes STEP importieren können.
- OBJ-Dateien (`.obj`) können jetzt katalogisiert werden — inklusive vollwertiger 3D-Vorschau, da OBJ (anders als STEP) ein reines Dreiecksnetz ist. OBJ-Dateien lassen sich wie 3MF/STL direkt im Slicer öffnen.

## [0.10.1] - 2026-09-20

### Fixed

- Pfeiltasten-Navigation im Raster scrollte nicht mit, wenn die neu ausgewählte Kachel den sichtbaren Bereich verließ — man navigierte "blind" weiter, ohne zu sehen, welches Modell gerade ausgewählt ist.

## [0.10.0] - 2026-09-20

### Added

- Modelle lassen sich jetzt direkt umbenennen: Kontextmenü → "Umbenennen", inline im selben Popup wie die Löschen-Bestätigung. Die Dateiendung (.3mf/.stl) wird dabei fest angezeigt und ist nicht editierbar. Verschiebt eine Umbenennung das Modell (Standardsortierung nach Name) aus dem sichtbaren Bereich, scrollt die Ansicht automatisch dorthin.
- Echte Pfeiltasten-Navigation im Raster: Hoch/Runter springt jetzt anhand der tatsächlich gerenderten Kachel-Position zur nächsten Zeile, statt nur zum nächsten/vorherigen Element in der Liste (vorher verhielt sich Runter wie Rechts).
- Leertaste schaltet die Mehrfachauswahl-Checkbox des aktuell ausgewählten Modells um.

## [0.9.0] - 2026-09-20

### Added

- Tastaturkürzel: `/` fokussiert die Suche, Pfeiltasten wechseln die Auswahl innerhalb der aktuell gefilterten/sortierten Modell-Liste, Entf/Rücktaste öffnet bei aktiver Mehrfachauswahl die bestehende Löschen-Bestätigung (löscht nicht direkt). Alle drei greifen nicht, solange der Fokus in einem Eingabefeld liegt.
- Mehrfachauswahl-Leiste: Tags lassen sich jetzt für mehrere ausgewählte Modelle gleichzeitig hinzufügen ("Tag hinzufügen") oder entfernen ("Tag entfernen", Dropdown mit allen in der Auswahl vorkommenden Tags) — bisher musste dafür jedes Modell einzeln geöffnet werden.
- Suche durchsucht jetzt zusätzlich zum Dateinamen auch Tags, Ersteller und den Dateipfad.

### Fixed

- Slicer-Name in den Einstellungen konnte bei langen Namen fast vollständig verschwinden (z. B. auf "B.." verkürzt), weil er sich mit dem "Standard"-Badge eine Zeile ohne Mindestbreite teilte ([GitHub Issue #10](https://github.com/Bexxs75/3mf-katalog-manager/issues/10)). Der Name steht jetzt immer in einer eigenen, vollbreiten Zeile; das Badge steht in der Zeile darunter vor dem Programmpfad.
- Der Creator-Filter (Sidebar) lief für jedes nur per schlanker Katalog-Übersicht geladene Modell ins Leere, da `creator` in dieser Projektion bislang fehlte und im Frontend hartcodiert `null` war — dieselbe Fehlerklasse wie der bereits behobene 3D-Vorschau-Bug. Betraf effektiv den gesamten Katalog, bis ein Modell einzeln geöffnet wurde.

### Removed

- Import-Option "Ordner als Sammlung importieren" entfernt: seit Ordner echte Dateisystem-Verzeichnisse sind, legte diese Option ohnehin schon zusätzlich einen normalen Ordner an (identisch zu "Ordner...") und packte die Dateien zusätzlich in eine Sammlung — der ursprüngliche Zweck (irgendeine Gruppierung erzwingen, als Ordner noch rein virtuell waren) ist damit entfallen. Sammlungen lassen sich weiterhin über die Mehrfachauswahl-Aktionsleiste ("Zu Sammlung hinzufügen") anlegen.

## [0.8.1] - 2026-09-20

### Security

- Interne Härtung nach dem [Senior-Code-Review vom 2026-09-19](docs/superpowers/plans/2026-09-19-senior-code-review-fixes.md) (14 Aufgaben plus Abschluss-Review): Datenbank-Migrationen laufen jetzt über ein richtiges, versioniertes Migrations-System (`db/migrations.rs`) statt stillschweigend ausgeführter `ALTER TABLE`-Anweisungen; mehrere Fehlerpfade beim Löschen/Wiederherstellen aus dem Papierkorb (Einzel- und Mehrfachauswahl) holen eine bereits physisch verschobene Datei jetzt zuverlässig zurück, falls der zugehörige Datenbank-Eintrag nicht aktualisiert werden konnte, statt sie verwaist liegen zu lassen. Keine sichtbare Funktionsänderung, rein interne Robustheit.

### Changed

- Die Slicer-Registrierung (Name, Programmpfad, automatisch erkannt oder manuell hinzugefügt) lebt jetzt in der Katalog-Datenbank statt in localStorage. Beim ersten Start nach dem Update werden nur automatisch erkennbare Slicer (Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer, UltiMaker Cura an bekannten Installationsorten) erneut per Startup-Scan gefunden; manuell hinzugefügte, individuelle Programmpfade werden beim Upgrade NICHT übernommen und müssen bei Bedarf neu eingerichtet werden. Beim Wiederherstellen eines Katalog-Backups bleiben lokal registrierte Slicer unverändert erhalten; Slicer-Einträge, die im Backup selbst enthalten waren (z.B. von einem anderen Rechner), werden dabei NICHT übernommen und müssen bei Bedarf manuell neu hinzugefügt werden — lokale Slicer-Konfiguration geht durch ein Backup-Restore also nie verloren.
- Der zuvor beim Export exportierte, aber nirgends mehr benötigte localStorage-Schlüssel für die alte, lokale Slicer-Liste wird nicht mehr in Katalog-Backups aufgenommen (konnte dort einen maschinenlokalen Programmpfad hinterlassen).

### Fixed

- Katalog-Übersicht (Raster/Ordner/Liste) zeigte bereits im Hintergrund gerenderte 3D-Vorschauen nicht an, solange ein Modell nicht einzeln geöffnet wurde: die schlanke Summary-Abfrage für die Übersicht ließ den gerenderten Snapshot bewusst weg (in der Annahme, er sei "der große Blob"), obwohl er im Schnitt kleiner ist als das ohnehin mitgelieferte eingebettete Vorschaubild (~8,9 KB vs. ~54 KB). Modelle ohne eigenes Vorschaubild (z. B. STL-Dateien) zeigten deshalb dauerhaft nur den leeren "3D Vorschau"-Platzhalter, obwohl in der Datenbank längst ein fertiger Snapshot lag.

## [0.8.0] - 2026-09-19

### Added

- Neue Ansicht "Ordner": Modelle können nach Ordnern gruppiert angezeigt werden (als Kachel-Raster oder Zeilen-Liste), verschachtelte Unterordner eingerückt mit Verbindungslinie, einzeln einklappbar (Zustand bleibt erhalten). Ordner-Kopfzeilen und Dateien lassen sich innerhalb dieser Ansicht direkt per Maus verschieben, genau wie bisher schon in der Seitenleiste.
- Update-Check: Beim Start wird einmalig still geprüft, ob auf GitHub eine neuere Version vorliegt (kein automatischer Download); bei neuerer Version erscheint ein wegklickbarer Hinweis unten rechts, zusätzlich manuell im neuen "Info"-Tab der Einstellungen abrufbar.

### Changed

- Einstellungen in 4 Reiter aufgeteilt (Allgemein, Slicer, Katalog, Info) statt einer einzigen scrollenden Liste; die Dichte-Einstellung (Kompakt/Komfort) ist jetzt Teil von "Erscheinungsbild" statt eines eigenen Abschnitts.

## [0.7.8] - 2026-09-19

### Changed

- Frontend-Refactor: `App.tsx` (vormals 1148 Zeilen, 48 State-/Effect-Hooks, 29 inline `invoke()`-Aufrufe) in fokussierte Hooks (`useCatalogStore`, `useCatalogFilters`, `useCollections`, `useBulkSelection`, `useFileImport`, `useFolderDragAndDrop`, `useCatalogBackup`, `useCatalogCleanup`, `useSlicerLauncher`), einen typisierten `src/lib/api/*`-Wrapper um alle Tauri-Commands und drei ausgelagerte Komponenten (`TrashView`, `CatalogWorkspace`, `BulkActionToolbar`) aufgeteilt. `App.tsx` ist damit auf rund 310 Zeilen reine Komposition geschrumpft. Erste automatisierte Frontend-Testsuite (Vitest + Testing Library) eingeführt. Keine Verhaltensänderung

### Security

- Allgemeine ISO-27000-orientierte Sicherheitsprüfung des gesamten Codebase durchgeführt ([Security-Review 2026-09-19](docs/security/security-review-2026-09-19.md)). Vier Hoch-Findings behoben, alle auf dieselbe Ursache zurückgehend: importierte Katalog-Backups wurden nur teilweise als nicht vertrauenswürdig behandelt.
  - Slicer-Startliste aus fremdem Backup konnte beliebiges Programm startbar machen (z.B. `/bin/sh`) — wird beim Import jetzt übersprungen, übrige Einstellungen gegen eine Werte-Whitelist geprüft (CWE-829)
  - `files.name`/`trash_path` aus importiertem Katalog konnten beliebiges Schreiben bzw. automatisches Löschen fremder Dateien auslösen — jetzt vollständig validiert bzw. auf das echte Trash-Verzeichnis eingegrenzt (CWE-22, CWE-829)
  - `source_url` wurde nur beim Schreiben, nicht beim Lesen validiert — ein `javascript:`-Link hätte beim Klick Code im App-Kontext ausführen können, jetzt beidseitig (Backend + Frontend) gefiltert
  - Kein Größenlimit beim Entpacken von 3MF-/Backup-Zip-Einträgen (Zip-Bomb-Risiko) — jetzt auf 16-256 MB je nach Eintragstyp begrenzt, Prüfung vor dem vollständigen Dekomprimieren
  - Zusätzlich behoben: Pfadprüfung ohne Kanonisierung (umgehbar via `../`/Symlinks), fehlende Validierung des Slicer-Datei-Arguments, fehlendes Größenlimit bei Render-Snapshots
  - Ungenutzte Datei mit Klartext-Google-OAuth-Credentials (Rest der 2026-09-12 entfernten Cloud-Integration) gelöscht

### Fixed

- Ordner per Drag & Drop verschieben markierte ungewollt Text in der Sidebar (mausbasiertes Drag&Drop statt natives HTML5-DnD); der gezogene Ordner war zudem visuell nicht von den anderen zu unterscheiden — zeigt jetzt reduzierte Deckkraft während des Ziehens

## [0.7.7] - 2026-09-18

### Security

- Importierte Katalog-Backups (`import_catalog`) konnten `folders.path`/`files.path` auf beliebige Orte setzen, die anschließend ungeprüft an `fs::rename`/`fs::create_dir` weitergereicht wurden (`create_folder`, `rename_folder`, `move_folder`, `move_file_to_folder`) — ein präpariertes Backup-ZIP hätte so z.B. Autostart-Verzeichnisse als Ziel unterschieben können ([Security-Review 2026-09-18](docs/security/security-review-2026-09-18.md), Finding 1, CWE-829). Neue Prüfung `reject_if_sensitive_path` lehnt Ziele in bekannten sensiblen Systemverzeichnissen (Config-/Autostart-/SSH-/Systemverzeichnisse) ab, sowohl direkt beim Import als auch bei jeder späteren Ordner-/Datei-Verschiebung
- Temporäre Katalog-Datenbank-Kopien beim Export/Import (`export_catalog`, `import_catalog`) landeten mit vorhersagbaren, PID-basierten Namen und Standard-Berechtigungen im geteilten `/tmp` (CWE-377) — auf Mehrbenutzer-Systemen von anderen lokalen Nutzern mitlesbar bzw. per Symlink-Race angreifbar. Werden jetzt exklusiv angelegt (`create_new`, schlägt fehl statt einem vorhandenen Symlink zu folgen) und wie `catalog.db` auf `0600` gehärtet

### Added

- GitHub-Actions-Workflows `build-macos.yml` und `build-windows.yml` (nur manuell per `workflow_dispatch` auslösbar): bauen unsignierte `.dmg`- bzw. `.msi`-Pakete komplett auf GitHubs eigenen Cloud-Runnern, ohne dass der lokale Rechner eine Mac- oder Windows-Toolchain braucht. Beide Ausgaben wurden nachträglich dem bestehenden [v0.7.6-Release](https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.7.6) hinzugefügt, das damit jetzt Linux, macOS und Windows abdeckt
- Bebildertes Benutzerhandbuch (`docs/benutzerhandbuch/BENUTZERHANDBUCH.md`, zweisprachig DE+EN) für Einsteiger ohne Vorwissen über die App, von der README aus verlinkt. Alle Screenshots zeigen frei erfundene Beispieldaten (Modelle, Filamentspulen) mit deutscher UI-Beschriftung, keine echten Nutzerdaten

## [0.7.6] - 2026-09-16

### Added

- Standard-Slicer: ein Radio-Button pro Zeile in den Einstellungen legt fest, welcher Slicer "In Slicer öffnen" verwendet ([GitHub Issue #7](https://github.com/Bexxs75/3mf-katalog-manager/issues/7)). Ersetzt die bisherige implizite "zuletzt verwendet"-Logik (`lastUsedId`, bei jedem Klick still überschrieben) durch eine sichtbare, selbst gewählte Voreinstellung (`primaryId`). Bestehende `lastUsedId`-Werte aus localStorage werden beim ersten Laden einmalig als initialer Standard übernommen. Der allererste konfigurierte Slicer wird automatisch zum Standard

### Changed

- `DetailPanel.tsx`: der Split-Button+Dropdown zum spontanen Umschalten des Ziel-Slicers entfällt zugunsten des neuen Standard-Slicers — konsistentes Verhalten an allen drei Einstiegspunkten (Kontextmenü, Detailpanel, Detailseite), die jetzt alle denselben, in den Einstellungen festgelegten Slicer verwenden. Das Kontextmenü brauchte dafür keine Änderung, da es schon vorher ohne feste Slicer-Auswahl aufgerufen wurde

## [0.7.5] - 2026-09-16

### Fixed

- Listen- und Grid-Ansicht: Scrollen bis ans Ende ließ sich unter Linux (WebKitGTK) noch weiter über den Inhalt hinaus ziehen (elastischer Rubber-Band-Overscroll), statt am Ende zu stoppen ([GitHub Issue #5](https://github.com/Bexxs75/3mf-katalog-manager/issues/5)). Der v0.7.4-Fix (`overscroll-behavior: contain` an den beiden Haupt-Scroll-Containern) reichte nicht aus, da dabei der komplette Fensterinhalt (auch Header/Sidebar außerhalb dieser Container) sichtbar mitwanderte — laut Spezifikation verhindert `contain` nur die Weitergabe an Eltern-Elemente, nicht den Bounce-Effekt auf der Seite selbst. Jetzt zusätzlich `overscroll-behavior: none` auf `html`/`body` gesetzt, was den Seiten-Bounce auf Root-Ebene unterbindet

### Reverted

- Der Pinch-Zoom-Fix aus v0.7.4 ([GitHub Issue #6](https://github.com/Bexxs75/3mf-katalog-manager/issues/6)) wurde zurückgenommen: nach weiterer Analyse stellte sich heraus, dass die beobachtete Vergrößerung eine Bildschirm-/Trackpad-Zoomfunktion des Notebooks selbst war (Betriebssystem-Ebene), kein Bug der App. Der globale `wheel`-Listener in `App.tsx` löste ohnehin nicht die tatsächliche Ursache und wurde daher wieder entfernt, statt unnötig Komplexität für ein Nicht-Problem der App zu behalten

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

### Added

- **Printer connection (Klipper/Moonraker):** New “Printers” settings tab with the “Printer connection” switch (off by default). Under “Manage printers” you can enter and test type and address per printer. The app asks connected printers at startup, every 5 minutes and on demand for finished prints, reads the filament actually extruded (for aborted prints only up to the abort) and converts it to grams. The filament storage then shows “N new prints waiting for confirmation”: in the dialog you check the spool (suggested: the one loaded in the printer) and the catalog model (suggested from the file name) per print, then confirm or ignore it. Nothing is deducted before you confirm; with a model, a print log entry is added and the model is marked as printed. Only prints ending after the first connection are deducted. The app only talks to addresses you entered yourself on your home network, only reads, and never changes anything on the printer. Tested with a Sovol SV08. Resin bottles are never suggested or deducted by the printer connection.

### Changed

- Printer connection: the matching catalog model is also suggested when the slicer puts the printer model or a date in front of the file name or appends the plate as “plate(01)” (e.g. Anycubic Kobra S1 with Rinkhals, thanks to an anonymous tester).
- The update check now correctly recognizes preview versions (e.g. "0.14.0-gharac") as older than the matching final release, instead of never reporting an update.
- Internal quality checks: test code is now clippy-clean, plus an automated CI check (TypeScript, tests, clippy) and a weekly dependency security audit (cargo-deny, npm audit).

## [0.13.1] - 2026-09-25

### Added

- Filament stock: **Resin**. "Filament | Resin" at the top of the stock switches between spools and resin bottles; the app remembers your choice. Overview, list, search, filters and stats apply to the selected type ("Total bottles", amounts in ml). Resin bottles show a bottle icon, the remaining amount in ml and the bottle size instead of the diameter; the status (in stock/low/empty) follows the same rules. "− Use" deducts the milliliters you used (to 0.1 ml, never below 0). New entries get the type of the currently selected area (no switch in the form), and material and manufacturer suggest matching values; for resin the amount is called "Volume (ml)" and there is no diameter. Resin never goes into a printer slot and is left out of "Is there enough filament?", the material cost estimate, the queue's filament symbol and gram totals. All existing entries stay filament, and older backups can still be restored.
- Filament stock: **Restock**. Every card has a "＋ Restock" button at the bottom left, and every row in the list view has a "＋" button. A small window adds 1 to 20 new, full spools or bottles with the same details (type, material, manufacturer, color name, color value, image, diameter). Amount, price per item and storage location are prefilled from the template (storage location: its home location if it currently sits in a printer) and can be changed first. New entries always go to storage. Everything is created in one step: if anything fails (for example because the template was deleted in the meantime), nothing is created. The new cards get a brief green outline, and a message states how many were added. Escape or a click outside closes the window without adding anything.
- Filament stock: **Double-clicking** a card or a table row opens the edit form (same as ✎). Double-clicks on buttons don't trigger it.
- Filament stock: Remaining weights are stored and shown to 0.1 g.

### Changed

- The 3D preview now shows models in coral instead of beige, so they stand out clearly from the background in both the light and the dark theme.

### Fixed

- Filament stock: The image field in the spool form ("Drop an image here or click") now also accepts images dropped onto it via **drag & drop**; previously only clicking worked. Exactly one PNG, JPG or WebP file up to 5 MB is accepted (same as the click upload). The field is highlighted while you drag over it. Several files, a different file type or an image that is too large show a notice right below the image field, and saving keeps the form open until another image is chosen or the notice is closed. A drop in the filament stock never starts a model import.
- Filament stock, list view: spools without their own image show a spool icon in the spool color instead of an empty square that looked like a checkbox.

## [0.13.0] - 2026-09-24

### Added

- Extract archives directly: archives added via "Import → Files…" or drag & drop (`.zip`, `.7z`, `.rar` (RAR4/RAR5), `.tar`, `.tar.gz`/`.tgz`, `.tar.bz2`/`.tbz2`, `.tar.xz`/`.txz`, `.tar.zst`/`.tzst`) open a dialog: target folder (prefilled with the active catalog folder or the storage location), one subfolder per archive, a choice between "new numbered folder" and "merge" when the folder already exists (existing files stay unchanged), and optional deletion of the original archives after extraction (off by default; an archive is only deleted when every entry was extracted – if anything was skipped, it stays in place and the result banner names the reason). Images, instructions, and license files inside stay next to the models. Folder import still does not extract anything.
- Extraction safety: entries with `..`, absolute paths, or drive letters (zip slip) and symlinks are skipped. Executables, scripts, and shortcuts (`.exe`, `.bat`, `.ps1`, `.lnk`, `.url`, `desktop.ini`, `.desktop`, `.app`, etc.) are never extracted. No entry may write into protected system locations, extracted files are never executable, and the download's "from the internet" mark (Windows Mark-of-the-Web, macOS quarantine) is carried over to the extracted files. Each archive is limited to 2 GB unpacked and 10,000 entries (also against forged size headers), plus memory limits for decompression. RAR entries are extracted one at a time in memory; an entry over 1 GB makes the whole archive fail (anything already extracted is removed). File references in RAR archives (file-copy/hardlink entries created with `rar -oi`) never take over the content of the referenced file: they are skipped, yield an empty file when their size is 0, or make the whole archive fail on a checksum error. Only archives that actually came in through the file dialog or drag & drop are extracted (verified by the backend itself). Only catalog folders or a folder chosen in one of the app's folder-selection dialogs can be the target; folders that contain protected locations (e.g. the home directory itself) are blocked as a target. On any error everything already extracted is removed again.
- Filament storage: **printers & AMS slots**. Printers and their multi-material units can be added via "Manage printers" – with templates for Bambu AMS/AMS lite/AMS HT, Creality CFS, Prusa MMU3, Anycubic ACE Pro and spool holder, or as a custom unit with 1–16 slots; name and order can be changed; every new printer automatically gets a spool holder (1 slot), so printers without an AMS can take a spool right away. Spools go in by drag & drop or via the slot menu; loading into an occupied slot sends the spool already there back to its home location, slot to slot moves the spool. Loaded spools are listed in their own column to the right of the storage and still count towards the total stock. When unloaded, a spool automatically returns to its home location (notice with "Change"). Deleting a unit or printer sends its spools back as well.
- Filament storage: spools now have a **color value** (palette of 16 common filament colors or hex input) in addition to the color name; known color names of existing spools (e.g. "Black", "Galaxy Black", "Dark blue") are converted automatically on update.
- Automatic tags in the interface language: the tags assigned on import ("mehrteilig", "miniatur", "grossformat", "mehrfarbig") now appear in English (multipart, mini, large, multicolor), Spanish and French when the interface is set to that language. Search finds them under both names. Typing one of these names in another language by hand (e.g. "Multipart") adds the existing tag instead of a second one. Existing tags with such names (e.g. from file names) are merged into the automatic tag once on the next start, including in saved filters, except the ambiguous words "mini", "large" and "grande", which are only mapped to the automatic tag when typed by hand.
- Tag suggestions from file names now also ignore English, Spanish and French filler words (e.g. "untitled", "copia", "nouveau").
- Filament inventory: **"Is there enough filament?"** – for 3MF files sliced in Bambu Studio or OrcaSlicer, the detail page compares the filament requirement (summed over all plates) with your spools: same material and similar color (CIEDE2000 color distance). For each filament it shows "enough", "enough with spool change", "not enough" (with the missing amount) or "unclear", plus the matching spool with its remaining weight and whether it is loaded in a printer (printer · unit · slot) or where it is stored. The print queue shows the status as a symbol per entry and works top to bottom with the combined requirement. Nothing is deducted.
- **Tools in the sidebar:** a new section groups the print queue (collapsible, reorder by dragging as before), the views "Recently viewed" (the 20 most recently viewed models), "Recently added" (last 7 days), "Favorites" (all models marked with a heart, alphabetical) and "Duplicates" (models with identical file content, grouped) as well as the cleanup suggestions – each with a count. Filament inventory and trash stay in the left rail as before. A selected view appears as a filter chip at the top and can be combined with folders, tags and search.

### Fixed

- Robustness: renaming or moving folders no longer terminates the app abruptly on an inconsistent folder hierarchy (e.g. from a crafted catalog backup); it aborts with an error message instead.
- Catalog backup: importing now also checks the data types of every column; a crafted backup with wrong-typed values (e.g. text instead of a number) is rejected instead of causing load errors afterward.
- Sorting by "Recently viewed" now takes all models into account, not only those already opened in the current session.

## [0.12.1] - 2026-09-23

### Fixed

- The app no longer started (crashed right after launch, without an error message) when the catalog still contained entries from the former Google Drive integration (removed in v0.5.0). The STEP/OBJ database migration introduced in v0.11.0 rebuilds the file table with a stricter check and failed on these entries ("CHECK constraint failed: origin IN ('local')"). Such entries are now turned into regular local entries during the migration – tags, collections, and all other data are preserved. The catalog itself was never damaged: the failed migration was always rolled back completely.
- Also includes the build fixes already delivered silently into the downloads after v0.12.0: the Linux AppImage starts again on systems with newer Mesa (EGL crash caused by bundled, outdated Wayland/X11 libraries), and a true universal DMG (Apple Silicon + Intel) for macOS.

## [0.12.0] - 2026-09-21

### Added

- STEP preview (`.stp`/`.step`) is now available on **all three platforms** — Linux, Windows, and macOS. Each platform has two download variants: one with STEP preview (file name with the `-step` suffix, includes Open CASCADE Technology/OCCT) and a smaller one without (STEP files can still be cataloged — tags, search, renaming, trash — just without 3D preview and without automatically derived dimensions/volume/body count). Assemblies are intentionally shown as one combined preview mesh; STEP still cannot be opened directly in the slicer. Faulty or oversized STEP files stay in the catalog without automatically derived metadata instead of aborting the import.
- Windows: MSI with bundled OCCT DLLs (`build-windows-step.yml`, OCCT built via a pinned vcpkg manifest). macOS: DMG with the OCCT `.dylib`s directly inside the app bundle (`build-macos-step.yml`, rewritten to `@executable_path`-relative paths via `dylibbundler`). Both verified on real target systems without a preinstalled OCCT — the app starts and the STEP preview works even when OCCT is not installed separately on the target machine.
- The OCCT integration is an optional Cargo feature (`step-preview`, enabled by default) and can be removed completely from the build with `--no-default-features` — which is exactly what produces the smaller download variant without STEP preview.
- Automatic slicer detection now also searches known install locations on macOS (Bambu Studio, OrcaSlicer, etc.), matching the existing Linux/Windows support.
- Open CASCADE is dynamically linked under LGPL-2.1 with the Open CASCADE exception. Full license texts, source information, and the documented `opencascade-sys` compatibility fork are provided with the source code and the program package; details are in `THIRD-PARTY-LICENSES.md`.

### Fixed

- The "Third-party licenses" row in the Info tab of the settings wrapped label and value onto two lines, unlike the other rows there — text shortened in all four languages.
- Four security unit tests for the protected-system-directory check failed on macOS because `/home` is an automounter symlink to `/System/Volumes/Data/home` there and `/var` is a symlink to `/private/var` — a pure test bug (non-existent test paths were only canonicalized on one side), not a defect in the actual check logic.

## [0.11.0] - 2026-09-20

### Added

- STEP files (`.stp`/`.step`) can now be cataloged — like 3MF/STL with tags, search, renaming, moving, and trash, but (for now) without a 3D preview, since STEP is parametric CAD geometry rather than a triangle mesh. STEP files are deliberately not launchable directly in a slicer, as most slicers can't import raw STEP.
- OBJ files (`.obj`) can now be cataloged — including a full 3D preview, since OBJ (unlike STEP) is a plain triangle mesh. OBJ files can be opened directly in a slicer, just like 3MF/STL.

## [0.10.1] - 2026-09-20

### Fixed

- Grid arrow-key navigation didn't scroll along when the newly selected tile left the visible area — you'd keep navigating "blind" without seeing which model was currently selected.

## [0.10.0] - 2026-09-20

### Added

- Models can now be renamed directly: context menu → "Rename", inline in the same popup as the delete confirmation. The file extension (.3mf/.stl) is shown fixed and cannot be edited. If a rename moves the model out of the visible area (default sort is by name), the view scrolls to it automatically.
- Real spatial grid navigation: Up/Down now jumps to the next row based on actual rendered tile position, instead of just the next/previous item in list order (previously Down behaved like Right).
- Space bar toggles the multi-select checkbox of the currently selected model.

## [0.9.0] - 2026-09-20

### Added

- Keyboard shortcuts: `/` focuses search, arrow keys move the selection through the currently filtered/sorted model list, Delete/Backspace opens the existing bulk-delete confirmation when a multi-selection is active (does not delete directly). All three are inert while focus is inside a text field.
- Multi-select action bar: tags can now be added to ("Add tag") or removed from ("Remove tag", dropdown listing every tag present in the selection) several selected models at once — previously each model had to be opened individually for this.
- Search now also matches tags, creator and the file path, in addition to the file name.

### Fixed

- The slicer name in settings could nearly disappear entirely for long names (e.g. truncated to "B.."), because it shared a line with the "Default" badge with no minimum width for the name ([GitHub Issue #10](https://github.com/Bexxs75/3mf-katalog-manager/issues/10)). The name now always gets its own full-width line; the badge sits on the line below, in front of the executable path.
- The sidebar creator filter silently matched nothing for any model loaded only through the slim catalog summary, because `creator` was missing from that projection and hardcoded to `null` on the frontend - the same bug class already fixed for the 3D-preview snapshot. Effectively affected the entire catalog until a model was opened individually.

### Removed

- Removed the "Import folder as collection" import option: since folders became real filesystem directories, this option already created a regular folder as a side effect (identical to "Folder...") and additionally packed the files into a collection — its original purpose (forcing some grouping while folders were still purely virtual) no longer applies. Collections can still be created via the multi-select action bar ("Add to collection").

## [0.8.1] - 2026-09-20

### Security

- Internal hardening following the [senior code review from 2026-09-19](docs/superpowers/plans/2026-09-19-senior-code-review-fixes.md) (14 tasks plus a final whole-branch review): database migrations now run through a proper, versioned migration system (`db/migrations.rs`) instead of silently-executed `ALTER TABLE` statements; several error paths when deleting/restoring files from the trash (single and multi-select) now reliably move a file back if it had already been physically moved but its database entry couldn't be updated, instead of leaving it stranded. No visible functional change, purely internal robustness.

### Changed

- Slicer registration (name, executable path, auto-detected or manually added) now lives in the catalog database instead of localStorage. On first launch after the update, only automatically detectable slicers (Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer, UltiMaker Cura at known install locations) are re-found by the startup scan; manually added, custom executable paths are NOT carried over across the upgrade and must be re-added if still needed. Restoring a catalog backup leaves locally registered slicers untouched; slicer entries that were part of the backup itself (e.g. from a different machine) are NOT restored and must be re-added manually if needed — local slicer configuration is therefore never lost through a backup restore.
- The previously exported, but no-longer-used localStorage key for the old, local slicer list is no longer included in catalog backups (it could have leaked a machine-local executable path).

### Fixed

- The catalog overview (grid/folder/list) didn't show 3D previews already rendered in the background unless a model was opened individually: the slim summary query for the overview deliberately left out the rendered snapshot (assuming it was "the large blob"), even though it's on average smaller than the embedded thumbnail that was already included (~8.9 KB vs. ~54 KB). Models without their own embedded thumbnail (e.g. STL files) therefore permanently showed only the empty "3D preview" placeholder, even though a finished snapshot had long been saved in the database.

## [0.8.0] - 2026-09-19

### Added

- New "Folder" view: models can be shown grouped by folder (as a tile grid or row list), nested subfolders indented with a connecting line, individually collapsible (state persists). Folder headers and files can be dragged directly within this view, exactly like the existing sidebar behavior.
- Update check: on startup, silently checks once whether a newer version is available on GitHub (no automatic download); when found, shows a dismissible hint bottom-right, also available on demand from the new "Info" tab in settings.

### Changed

- Settings split into 4 tabs (General, Slicer, Catalog, Info) instead of one long scrolling list; the density setting (Compact/Comfort) is now part of "Appearance" instead of its own section.

## [0.7.8] - 2026-09-19

### Changed

- Frontend refactor: `App.tsx` (previously 1148 lines, 48 state/effect hooks, 29 inline `invoke()` calls) split into focused hooks (`useCatalogStore`, `useCatalogFilters`, `useCollections`, `useBulkSelection`, `useFileImport`, `useFolderDragAndDrop`, `useCatalogBackup`, `useCatalogCleanup`, `useSlicerLauncher`), a typed `src/lib/api/*` wrapper around all Tauri commands, and three extracted components (`TrashView`, `CatalogWorkspace`, `BulkActionToolbar`). `App.tsx` is now around 310 lines of pure composition. First automated frontend test suite (Vitest + Testing Library) introduced. No behavior change

### Security

- General ISO-27000-oriented security review of the entire codebase performed ([Security Review 2026-09-19](docs/security/security-review-2026-09-19.md)). Four High findings fixed, all tracing to the same root cause: imported catalog backups were only partially treated as untrusted.
  - A slicer launch list from a foreign backup could make an arbitrary program launchable (e.g. `/bin/sh`) — now skipped on import, remaining settings checked against a value whitelist (CWE-829)
  - `files.name`/`trash_path` from an imported catalog could trigger arbitrary writes or automatic deletion of unrelated files — now fully validated, or contained to the real trash directory (CWE-22, CWE-829)
  - `source_url` was only validated on write, not on read — a `javascript:` link could have executed code in the app context on click, now filtered on both backend and frontend
  - No size limit when extracting 3MF/backup zip entries (zip-bomb risk) — now capped at 16-256 MB depending on entry type, checked before full decompression
  - Also fixed: path validation without canonicalization (bypassable via `../`/symlinks), missing validation of the slicer file argument, missing size limit on render snapshots
  - Deleted an unused file containing plaintext Google OAuth credentials (leftover from the cloud integration removed 2026-09-12)

### Fixed

- Dragging a folder in the sidebar unintentionally selected text (mouse-based drag-and-drop instead of native HTML5 DnD); the dragged folder was also visually indistinguishable from the others — now shows reduced opacity while dragging

## [0.7.7] - 2026-09-18

### Security

- Imported catalog backups (`import_catalog`) could set `folders.path`/`files.path` to arbitrary locations, which were then passed unchecked to `fs::rename`/`fs::create_dir` (`create_folder`, `rename_folder`, `move_folder`, `move_file_to_folder`) — a crafted backup ZIP could have pointed these at e.g. autostart directories ([Security review 2026-09-18](docs/security/security-review-2026-09-18.md), Finding 1, CWE-829). New `reject_if_sensitive_path` check rejects targets inside known sensitive system directories (config/autostart/SSH/system directories), both directly on import and on every later folder/file move
- Temporary catalog database copies during export/import (`export_catalog`, `import_catalog`) were written to shared `/tmp` with predictable, PID-based names and default permissions (CWE-377) — readable by other local users on multi-user systems, or vulnerable to a symlink race. Now created exclusively (`create_new`, fails instead of following an existing symlink) and hardened to `0600` like `catalog.db`

### Added

- GitHub Actions workflows `build-macos.yml` and `build-windows.yml` (manual `workflow_dispatch` trigger only): build unsigned `.dmg` and `.msi` packages entirely on GitHub's own cloud runners, no local Mac/Windows toolchain required. Both outputs were added to the existing [v0.7.6 release](https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.7.6), which now covers Linux, macOS, and Windows
- Illustrated user guide (`docs/benutzerhandbuch/BENUTZERHANDBUCH.md`, bilingual DE+EN) for beginners with no prior knowledge of the app, linked from the README. All screenshots show entirely made-up sample data (models, filament spools) with German UI labels, no real user data

## [0.7.6] - 2026-09-16

### Added

- Default slicer: a radio button per row in Settings picks which slicer "Open in slicer" uses ([GitHub Issue #7](https://github.com/Bexxs75/3mf-katalog-manager/issues/7)). Replaces the previous implicit "last used" logic (`lastUsedId`, silently overwritten on every click) with a visible, user-chosen default (`primaryId`). Existing `lastUsedId` values in localStorage are adopted as the initial default on first load. The very first configured slicer becomes the default automatically

### Changed

- `DetailPanel.tsx`: the split-button-plus-dropdown for switching the target slicer on the fly is gone in favor of the new default slicer — consistent behavior across all three entry points (context menu, detail panel, detail page), which now all use the same slicer set in Settings. The context menu needed no change for this, since it already called the action without a fixed slicer choice

## [0.7.5] - 2026-09-16

### Fixed

- List and grid view: scrolling to the end could still be dragged further past the content on Linux (WebKitGTK), an elastic rubber-band overscroll instead of stopping at the end ([GitHub Issue #5](https://github.com/Bexxs75/3mf-katalog-manager/issues/5)). The v0.7.4 fix (`overscroll-behavior: contain` on the two main scroll containers) wasn't enough — the entire window content (including the header/sidebar outside those containers) visibly shifted along with the bounce, because per spec `contain` only stops chaining to ancestor elements, not the bounce effect on the page itself. Now additionally sets `overscroll-behavior: none` on `html`/`body`, which suppresses the page-level bounce at the root

### Reverted

- The pinch-zoom fix from v0.7.4 ([GitHub Issue #6](https://github.com/Bexxs75/3mf-katalog-manager/issues/6)) has been reverted: further analysis showed the observed magnification was the notebook's own screen/trackpad zoom feature (OS level), not an app bug. The global `wheel` listener in `App.tsx` never addressed the actual cause anyway, so it was removed rather than kept as unnecessary complexity for something that isn't an app problem

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
