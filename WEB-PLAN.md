# WEB-PLAN: Schlankes Web-Frontend für den 3MF Katalog Manager

Interner Plan. Dieses Repository ist privat und wird bewusst still entwickelt: keine öffentlichen Releases, keine Roadmap-Einträge, keine Ankündigungen, GitHub Actions ist abgeschaltet.

- Grundlage: Stand `dev/0.16.0` der Desktop-App (RC 12, Commit `ac1d434c`) mit voller Historie. Die Desktop-App bleibt in `upstream` (lokaler Pfad `/mnt/claude/projekte/3mf-katalog-manager`), Korrekturen lassen sich dort per Merge übernehmen.
- Mockup (klickbar, mit Katalog, Detail, Import, Material, Resin, Spule anlegen, Drucker, Anmeldung): `docs/web/mockup.html` (im Browser öffnen).
- Prüfung: Plan von Codex gegen den Code geprüft (16.10.2026); Befunde sind unten eingearbeitet.

## 1. Ziel und Abgrenzung
- Ein Web-Frontend, das der Katalog Manager **zusätzlich** zur Desktop-App anbietet. Die Desktop-App bleibt in ihrem Verhalten unverändert.
- **Schlank:** kein gestreamter Desktop mehr (kein Selkies, kein WebKit im Container). Der Browser des Nutzers zeichnet Oberfläche und 3D-Vorschau (mit der GPU des Nutzers).
- **Optional für Docker:** ein zweites, kleines Image bzw. ein Compose-Profil neben dem Desktop-Image.
- Nicht-Ziele: Updater, „In Slicer öffnen“ auf dem Server, native Dateidialoge, Betriebssystem-Drag-and-drop, Mehrbenutzer-Rechte.

## 2. Befund aus dem Code (dev/0.16.0; von Codex gegengeprüft)
- Frontend (React/TypeScript, rund 25.000 Zeilen ohne Tests): spricht fast ausschließlich über `invoke`. **138 Commands sind registriert, 137 davon referenziert das Frontend** (35 Dateien; nur `rename_folder` nicht). Signaturen: **42 mit AppHandle/Window, 80 nur mit AppState, 16 sonstige.** Auch Commands ohne Handle können Desktop-Aktionen sein (z. B. in `commands/files.rs`). Wiederverwendbarkeit also **nach Verhalten bewerten, nicht nach Signatur**.
- Ereignisse: `archive-progress`, `import://progress`, `import://finished`, `printer-jobs-changed`; dazu der Updater-Channel. Native Webview-Drops in `useFileImport.ts` und `useImageDropZone.ts`; Fenster-Aufrufe über `frontend_ready` und `save_window_theme`.
- Bilder kommen als Base64-Daten-URLs (mit Batch-Abruf), Geometrie als **Binärantwort** (`tauri::ipc::Response`, im Frontend `ArrayBuffer`). Kein `convertFileSrc`.
- Backend (Rust, rund 41.000 Zeilen): Die Module `db`, `threemf`, `stl`, `obj`, `step`, `archive` sind frei von direkten Tauri-Aufrufen. **Nicht frei:** `printer_link` (nutzt AppHandle, Emitter, `commands::AppState`), `db` braucht die Schema-Sicherung aus `updater`. Tauri und seine Plugins sind **nicht optional** in `Cargo.toml`, `build.rs` ruft `tauri_build` zwingend auf. STEP braucht dynamisches OpenCASCADE (OCCT): „ein einziges Binary“ gilt damit nicht.
- `AppState` enthält `ImportJobs`, `Mutex<Connection>`, Papierkorb-/DB-Pfade und geschützte Verzeichnisse; `ImportJobs::for_app` bindet einen Tauri-Event-Sender ein.
- „Nur Lesen“ ist heute **kein nebenwirkungsfreier Modus:** Modellauswahl schreibt den Sichtungszeitpunkt, Vorschauen speichern Snapshots, das Öffnen der Datenbank migriert und bereinigt, der Desktop-Start bereinigt Tags/Papierkorb und startet Backfills. Die Begleitseite öffnet die Datenbank dagegen ausdrücklich mit `mode=ro` (`3mf-katalog-web`, 1.434 Zeilen, Python).
- Messwerte Docker (Selkies-Desktop): Image 4,21 GB, RAM 0,8 bis 1,4 GiB. Zielwerte eines Web-Images sind **Schätzungen und nicht gemessen** (mit OCCT und großen Modellen erst messen).

## 3. Varianten
- **A. Schlanker Server mit eigener API, stufenweise (Ziel).** Gemeinsamer Core, Desktop-Adapter und Server getrennt; Frontend mit austauschbarer Transportschicht. **Erster Lieferumfang bewusst nur Ansicht** (Lesen).
- **B. Alle Commands 1:1 über HTTP:** verworfen (Sicherheitsfläche, jeder Dialog-/Ereignis-Command braucht ohnehin eine Web-Entsprechung).
- **C. Begleitseite (Python) ausbauen:** nur als dauerhaft kleine Ansicht sinnvoll; sonst doppelte Fachlogik.

## 4. Stufen (überarbeitete Reihenfolge)
0. **Core- und Build-Entkopplung, Funktionsmatrix.** Gemeinsamen Core herauslösen (inkl. Sicherungshelfer aus `updater`, `printer_link` entkoppeln); Desktop-Adapter und Server getrennt bauen, Tauri-Abhängigkeit des Servers entfernen. Funktionsmatrix: jeder der 138 Commands → Web / nur Desktop / ersetzt / entfällt, nach Verhalten.
1. **Authentifizierter Read-only-Server** mit echtem Browser-Smoke-Test, eigener Read-only-Startpfad (kein Migrieren, kein Bereinigen, keine Backfills), automatische UI-Schreibzugriffe (Sichtungszeit, Snapshots) abschaltbar.
2. **Liste, Details, Bilder, Download** (signierte Downloads als bewusste Ausnahme von der Anmeldung).
3. **Begrenzte 3D-Vorschau** (Binärtransport erhalten, Parallelität der Parser begrenzen, Größenlimits).
4. **Metadatenänderungen** (Tags, Favoriten, Druckstatus, Lizenz, Sammlungen); Papierkorb mit endgültigem Löschen separat.
5. **Import und Ereignisse** (Upload im Browser, Fortschritt per SSE mit Wiederverbindung, Zustandsabgleich, begrenzten Warteschlangen; Upload- und Entpacklimits).
6. **Printer Manager und Material Manager** (Vorschlag: nach Stufe 4 vorziehen, denn `printers.rs` und `filament.rs` sind fast reine Datenbanklogik; 14 Commands ohne AppHandle, 7 in `filament.rs` mit nur technischem AppHandle). **Druckeranbindung** (`printer_link.rs`, 11 Commands, 7 mit AppHandle, Hintergrunddienst, Ereignisse, Netzzugriff auf den Drucker, SSRF-Grenze) bleibt bei Stufe 5.
- Nicht umgesetzt: Updater, Slicer-Start (stattdessen Download und Link), native Dialoge.

## 5. Technik
- **Transportschicht** `src/lib/api.ts` mit der Signatur von `invoke`, Auswahl über `VITE_TARGET=web|desktop`. Ereignisse über `subscribe(name, cb)` (Tauri-Event oder SSE). Binäre Antworten und Fehlerformat als **Vertrag** festschreiben. Tauri-spezifisches (Logging, Channel, Webview-Drops, Fenster) gezielt ersetzen oder abschalten.
- **Server:** Rust, kleiner HTTP-Stack, JSON plus Binärantworten, Upload mit Limits, serverseitige Pfadfreigaben statt Dialognachweisen.
- **Sicherheit:** Anmeldung auch für Lesen und SSE; CSRF-Token und Same-Origin wie in der Begleitseite; Pfadprüfung; Rate Limit; nur Loopback ohne Proxy.
- **Datenbank:** dieselbe SQLite-Datei. **Einzelschreiber technisch erzwingen** (Sperre über Prozessgrenzen), nicht nur dokumentieren. Schema-Kompatibilität und Wiederherstellung bei Migrationswechsel planen.

## 6. Docker (optional)
- Neues Image `3mf-katalog-manager:web` neben dem Desktop-Image; Compose-Profil `web`. Gleiche Ordner `/config` und `/models` und gleiche Passwortdatei für einen Wechsel ohne Datenverlust. Die Begleitseite entfällt oder geht im Server auf.
- Enthält OCCT als dynamische Bibliothek, falls STEP-Vorschau serverseitig nötig ist (sonst STEP im Web nur als Download).

## 7. Risiken
- Zwei Oberflächen paritätisch pflegen; 42 Commands mit Handle und weitere mit Desktop-Aktionen brauchen je eine Entscheidung.
- Entkopplung von Core und Tauri ist der größte Posten (Build, `AppState`, Ereignisse), nicht die HTTP-Schicht.
- Große Kataloge: Listen, Bilder (Base64-Größe), Paging, Zwischenspeicher messen.
- Parser-/Geometrie-Parallelität, Upload-/Entpacklimits, Archiv-Bomben.
- Nebenläufigkeit und Datensicherung; Wiederherstellung nach fehlgeschlagener Migration.
- Aufwand ist ohne Stufe 0 nicht seriös zu schätzen. Relative Größen: Stufe 0 groß (Core-Entkopplung), 1 mittel, 2 mittel, 3 mittel, 4 mittel, 5 groß, 6 mittel bis groß.

## 8. Tests
- Frontend: Tests mit HTTP-Mock der Transportschicht; Verträge für Binär- und Fehlerformat.
- Server: je Route; **nachweislich schreibfreier Ansichtsmodus**; parallele Requests; SSE-Abbruch und Wiederaufnahme; Migrationswechsel.
- **Echte Browserausführung ohne Tauri** (E2E gegen das Web-Image).
- Image- und RAM-Ziele erst mit OCCT und großen Modellen messen.

## 9. Zeitaufwand (Schätzung, nicht aus einem Spike abgeleitet)
Gemessene Richtwerte: kleine Korrektur mit Codex gebaut in ca. 4 Minuten, bis zum geprüften Commit 35 bis 45 Minuten; VM-Test pro System 15 bis 35 Minuten; Codex-Planprüfung ca. 2 Minuten; CI-Build mit allen Paketen 45 bis 60 Minuten.

Reine Arbeitszeit von Claude und Codex (Stunden): Stufe 0 4 bis 8, Stufe 1 2 bis 4, Stufe 2 4 bis 6, Stufe 3 2 bis 3, Docker 2 bis 4, Stufe 4 3 bis 5, Material und Printer Manager 4 bis 6, Import und Ereignisse 6 bis 10, Druckeranbindung 3 bis 5; dazu ca. 12 Stunden Tests und Release-Zyklen.

- Erste nutzbare Fassung (Stufen 0 bis 3 plus Docker, nur Ansicht): 18 bis 29 Stunden, 2 bis 4 Arbeitstage, in der Kalenderzeit 2 bis 4 Wochen.
- Alles aus dem Mockup: 40 bis 60 Stunden, 5 bis 8 Arbeitstage, Kalenderzeit 4 bis 8 Wochen.
- Größte Unbekannte: Stufe 0 (Tauri ist nicht optional, `build.rs` ruft `tauri_build`, `printer_link` und `AppState` hängen an Tauri). Ein Spike von ein bis zwei Tagen macht die Zahlen belastbar.
- Ab 27.10.2026 baut Codex nicht mehr (nur Prüfung); dann bauen Sonnet- und Haiku-Worker, was das Claude-Limit stärker belastet.

## 10. Offene Entscheidungen (User)
1. Reihenfolge der Stufen: Material Manager und Printer Manager nach dem Bearbeiten (Vorschlag), Import und Druckeranbindung zuletzt.
2. Zeitpunkt des Spikes (Stufe 0): nach der stabilen 0.16.0.
3. Namen des Repositorys: `3mfKatalog-Web` (Alternative: `3mfWEBKatalog`).
4. Wann (und ob) etwas davon öffentlich wird (Roadmap-Eintrag, Ankündigung).
