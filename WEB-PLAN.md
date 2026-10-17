# WEB-PLAN: Schlankes Web-Frontend für den 3MF Katalog Manager

Interner Plan. Dieses Repository ist privat und wird bewusst still entwickelt: keine öffentlichen Releases, keine Roadmap-Einträge, keine Ankündigungen, GitHub Actions ist abgeschaltet.

- Grundlage: Stand `dev/0.16.0` der Desktop-App (RC 12, Commit `ac1d434c`) mit voller Historie. Die Desktop-App bleibt in `upstream` (lokaler Pfad `/mnt/claude/projekte/3mf-katalog-manager`), Korrekturen lassen sich dort per Merge übernehmen.
- Aufbau (Architekturdiagramm, im Browser öffnen): `docs/web/architecture.html` (Quelle: `docs/web/architecture.json`).
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
(Entschieden am 16.10.2026: Betrieb zuerst A und B, siehe Abschnitt 11.)

## 10a. Abstriche der Webversion und Folgestufen (Stand 16.10.2026, Vorschlag, User entscheidet)

**Abstriche gegenüber Desktop-App und Docker-Stream** (der Stream zeigt die ganze Desktop-App):

| Bereich | Webversion | Grund |
|---|---|---|
| Dateien umbenennen, löschen, Papierkorb | gesperrt | greift ins Dateisystem des Servers |
| Ordner anlegen, umbenennen, verschieben, löschen | gesperrt | dasselbe; Uploads bleiben im Upload-Ordner |
| Archive (zip, 7z, rar) | beim Upload abgelehnt (415) | Entpacklimits und Zip-Bomben-Schutz brauchen eigenes Review (Stufe 5b) |
| Ordner-Import vom Server, Drag aus dem Dateimanager, Setup-Adopt, Katalog-Wurzel wechseln | entfällt | im Browser nur Upload einzelner Dateien |
| Druckeranbindung (Bambu, Moonraker, OctoPrint) | nicht dabei | Netzzugriff auf Drucker (SSRF-Grenze); nur gespeicherter Verlauf |
| Spule auffüllen, Harz verbrauchen | vorerst gesperrt | DB-only, aber noch nicht freigegeben |
| Eigenes Titelbild, Render-Snapshot, Neuscan | gesperrt | schreiben Bilder/scannen Dateien |
| Slicer starten, Update aus der App, native Dialoge, Fehlerbericht mit Logdatei | entfällt | Desktop-Funktionen; im Web Download statt Slicer-Start |
| Katalog sichern/exportieren, Abgleich Katalog gegen Platte | ungeprüft, vermutlich nicht dabei | in der Webversion nicht untersucht |

Technische Grenzen: ein Benutzer, ein Passwort, kein HTTPS im Server (Proxy oder VPN); kein Parallelbetrieb mit der Desktop-App auf derselben Datenbank; Upload standardmäßig 1 GiB je Datei, kein Wiederaufnehmen; `file_modified_at` ist der Upload-Zeitpunkt; Server ohne STEP-Bibliothek (OCCT), daher voraussichtlich keine STEP-Vorschau und leere STEP-Metadaten; Parsergrenzen im Web strenger als in der App (Speicher); geslicte `.gcode.3mf` enthalten kein 3D-Modell.

**Bedingung, um den Docker-Stream abzulösen:** Stufe 5 (Upload) allein genügt nicht. Wer Dateien umbenennen, verschieben oder Archive entpacken will, braucht weiter die App. Vorgeschlagene Folgestufen (Reihenfolge und Umfang entscheidet der User; jede mit Spec, Codex-Planprüfung und Opus-Review wie Stufe 5):
- **Stufe 5b: Archive** (zip zuerst; 7z/rar prüfen). Entpacken mit harten Limits (Gesamtgröße, Dateizahl, Verschachtelung, Pfadtraversal, Symlinks), Zip-Bomben-Schutz, Ergebnis wie Desktop (importiert/Duplikate/übersprungen).
- **Stufe 7: Dateiverwaltung im Web.** Umbenennen, Ordner anlegen/verschieben, Löschen mit Papierkorb und Bestätigung, jeweils nur innerhalb der Roots und des Upload-Ordners, mit Journal und Folgenhinweis; das ist die größte Sicherheitsfrage nach dem Upload (Schreib- und Löschrechte auf Modelldateien).
- **Stufe 8: Kleinigkeiten.** Spule auffüllen, Harz verbrauchen, eigenes Titelbild, Neuscan, optionale STEP-Vorschau (nur wenn ein eigenes OCCT-Image vertretbar ist).
- **Stufe 9: Druckeranbindung** (optional, zuletzt): Netzzugriff auf Drucker mit SSRF-Grenze und Allowlist.
- Offen zu klären: Katalog-Sicherung/Export und Abgleich gegen die Platte im Web; Ablösung des Docker-Streams erst, wenn Stufe 5b und Stufe 7 abgenommen sind.

## 11. Betrieb: eigener Server oder Docker lokal (entschieden 16.10.2026)
**Entscheidung des Users:** Zuerst Betriebsart A (Heimnetz) und B (Heimnetz plus VPN). Betriebsart C (eigener Server mit Reverse Proxy und HTTPS) nur bei Bedarf und später. Kein von uns gehosteter Dienst.

Grundlage der Entscheidung: **nur selbst betriebenes Docker-Image, kein von uns gehosteter Dienst.** Dasselbe Image läuft zu Hause (Heimnetz, NAS, Mini-PC) oder auf einem eigenen Server des Nutzers; der Unterschied ist nur die Erreichbarkeit.

| Betriebsart | Erreichbar von | Sicherheit | Aufwand für den Nutzer |
|---|---|---|---|
| A. Heimnetz (Standard) | nur im eigenen Netz | am einfachsten, Anmeldung bleibt Pflicht | gering |
| B. Heimnetz plus VPN (WireGuard, Tailscale) | von unterwegs, ohne offene Ports | sehr gut | mittel |
| C. Eigener Server (VPS) mit Reverse Proxy und HTTPS | überall | nur mit Härtung (Proxy, HTTPS, Anmeldung, Updates) | hoch |

Gründe gegen einen zentral von uns gehosteten Dienst: Die App verspricht „alles bleibt auf deinem Rechner“; fremde, oft urheberrechtlich geschützte 3D-Modelle auf unserem Server brächten Haftung, Moderation, Datenschutz und laufende Kosten; Mehrmandantenfähigkeit wäre ein anderes Produkt.
Bis auf Weiteres gilt: Standard bindet nur an das lokale Netz, ohne Anmeldung startet der Server nicht, HTTPS übernimmt der Reverse Proxy.

## 12. Fortschritt
- **16.10.2026, Stufe 0 Teil 1 erledigt (Commit `9fa65905`):** `crates/katalog-core` enthält Datenbank, Parser (3MF, STL, OBJ, STEP), Archive, Druckeranbindung (ohne Hintergrunddienst), Schema-Sicherung und Dateihilfen, ohne Tauri-Abhängigkeit. Die Desktop-App re-exportiert die Module unter den alten Pfaden. 1021 Tests bestanden (448 Core, 569 Desktop, 4 ignoriert), Clippy ohne Warnungen. Offen: STEP-Feature ist hier ohne `cmake` nicht prüfbar (erst in CI oder VM mit OCCT).
- Als Nächstes (Stufe 0 Teil 2): `AppState`/Context und `ImportJobs` ohne Tauri-Event-Sender, Fachkörper der Commands nach Bedarf der Stufe 1 in den Core, eigener Read-only-Öffnungspfad für die Datenbank.
- **16.10.2026, Stufe 0 Teil 2 erledigt:** Lesepfad im Core. `db::open_read_only` (nur lesend, keine Migration, Sicherung, Bereinigung), `katalog_core::read` mit den Lese-Abfragen und DTOs für Liste, Bilder, Tags, Ordner, Sammlungen, Papierkorb-Ansicht, Druckprotokoll und die Geometrie-Kodierung (Laden und Rechnen getrennt), optionale erlaubte Wurzeln für den Dateizugriff. 1031 Tests (10 neu, keiner entfallen), Clippy ohne Warnungen. **Offen:** STEP bleibt aus dem Lesepfad (kein Datei-Handle-Schutz), AppState/ImportJobs ohne Tauri erst mit Stufe 4/5, nächster Schritt Stufe 1 (Server nur lesend).

## 13. Entscheidungen und Stand (16.10.2026, Nachtrag)
- **Ziel:** Die Webversion soll bei stabilem Betrieb den Docker-Stream (Desktop im Browser per Selkies) ablösen. Der Aufwand bleibt so klein wie möglich, trotzdem wird auf Nutzerwünsche eingegangen. Bis dahin bleibt sie still und nichts Offizielles.
- **Folge für die Reihenfolge:** Um den Stream abzulösen, braucht die Webversion mindestens Stufe 5 (Import per Upload, Download, Ordner), denn Docker-Nutzer bringen ihre Modelle heute über die Begleitseite und den gestreamten Desktop ein. Material Manager und Printer Manager nach den Wünschen der Docker-Nutzer.
- **Sicherheit von Anfang an (Stufe 1):** Passwort als Argon2id-Hash in eigener Datei, Änderung in der Oberfläche, `set-password` auf der Kommandozeile, Sperre je Client, CSRF-Token, Host- und Origin-Prüfung.
- **Stufe 1 gebaut** (`katalog-server`, nur lesend): 31 Server-Tests, Release-Binary 4,1 MB, keine Tauri-, GTK- oder WebKit-Abhängigkeit; Browser-Smoke-Test mit Chromium gegen eine migrierte Kopie eines Testkatalogs bestanden (Anmeldung, falsches Passwort, 15 Modelle, keine externen Anfragen, Katalogdatei byteidentisch).
- **16.10.2026, Stufe 1 committet (`98454260`):** `katalog-server` nur lesend mit sicherer Anmeldung (Argon2id, Passwortänderung in der Oberfläche, `set-password`, Sperre je Client mit IPv6-/64-Gruppierung, CSRF, Host-/Origin-Prüfung, Verbindungsgrenzen und Timeouts). Unabhängiges Sicherheits-Review (Opus): kein Blocker, die Befunde (Slowloris, IPv6-Umgehung, gleichzeitige Versuche, Tabellenwachstum, Proxy-Header, Fehlertexte) sind behoben. Offen und bewusst: TOCTOU bei Verzeichnisanteilen der Geometrie, Eingabe ohne Echo bei `set-password`, mtime-Prüfung der Passwortdatei. 51 Server-Tests, Binary 4,1 MB.
- **Nächster Schritt: Stufe 2a** (Transportschicht, Web-Build, Start und Anmeldung im Frontend, Server liefert die Oberfläche aus). Spec: `/mnt/claude/claudex-runs/docs/spec-web-frontend-2a.md`, Grundlage: Frontend-Audit von Codex.
- **16.10.2026, Stufe 2a committet (`45d9a775`):** Transportschicht (`src/lib/transport.ts`, Auswahl zur Bauzeit über `VITE_TARGET=web`), Web-Build (`npm run build:web`, `dist-web`), Anmeldung, erzwungene Passwortänderung und Abmelden in der React-Oberfläche (vier Sprachen), Web-Start ohne Desktop-Effekte, `KATALOG_STATIC_DIR` im Server. 2098 Frontend-Tests, 56 Server-Tests. Echter Browser-Test (Chromium) gegen den Server mit einer migrierten Testkopie: Raster mit Vorschaubildern, Ordner, Tags, Filter; beim Start nur freigegebene Commands, keine externen Anfragen, Katalogdatei byteidentisch. Offen aus 2a: Schreib-Bedienelemente noch sichtbar (zeigen eine Meldung statt zu sperren), das ist Teil 2b.
- **16.10.2026, Stufe 2b committet (`947ef5ae`):** Oberfläche read-only (Schreibaktionen versteckt oder gesperrt, `useReadOnly`). Docker-Slim-Image `docker-web/` (`dfee64cc`): ca. 47 MB statt 4,2 GB (distroless, Nutzer 65532, gleiches /config- und /models-Layout).
- **16.10.2026, Stufe 2c committet (`2cb98f99`):** Download-Endpunkt mit Range/If-Range (Handle-basiertes Last-Modified), Idle-Timeout mit 64-KiB-Fortschritt, Entfernen von Format-/Bidi-Zeichen im Dateinamen, unauflösbare Wurzeln werden übersprungen. Zwei Codex-Runden plus Prüfung: 1080 Rust-Tests grün (Sandbox-Socketfehler entfallen lokal), Clippy `-D warnings` sauber, 2137 Frontend-Tests, Server-Baum ohne Tauri. Nächster Schritt: Stufe 4 (Bearbeiten), dann Stufe 5 (Import per Upload).
- **16.10.2026, Entscheidung (User):** Kein gleichzeitiger Betrieb von Desktop-App und Webversion auf derselben Datenbank. Der Nutzer entscheidet sich für Web oder lokal; beides parallel ergibt keinen Sinn. Das wird in der Dokumentation (README, docker-web, später Wiki) ausdrücklich festgehalten. Der Schreibmodus (`KATALOG_WRITE=1`) ist standardmäßig aus.
- **16.10.2026, Entscheidung (User):** Modellwahl bleibt wie bisher (Codex baut, Claude prüft, Opus nur für Security). **Stufe 5 (Import per Upload) wird zusätzlich von Opus geprüft** (Spec/Plan und Review vor dem Commit): Uploads, Pfadsicherheit, Speicherlimits, Abbruch mitten im Import, Archive.

### Optionaler Schreibmodus (Stufe 4a und Stufe 6 Teil A)

`KATALOG_WRITE=1` aktiviert die 32 Commands für Metadaten sowie Material- und Druckerverwaltung aus
`katalog_server::WRITE_COMMANDS`. Standard bleibt lesend; Lese-Routen behalten
64 KiB Body-Limit. Schreib-Requests benötigen Anmeldung, CSRF, einen vorhandenen
passenden Origin und eine explizite, nicht leere `KATALOG_ALLOWED_HOSTS`-Liste.
`KATALOG_ALLOW_NO_AUTH=1` ist damit unvereinbar. `writable` in Session-, Login-
und Passwortantworten ist erst nach dem erzwungenen Passwortwechsel wahr.
Die Oberfläche (Teil B, Commit `e3381f74`) verwendet diesen Status: Bearbeiten nur bei `writable`, Ordner, Umbenennen, Löschen, Papierkorb und Import bleiben gesperrt.

Die einzige Schreibverbindung öffnet nur eine vorhandene DB mit exakt passender
Schemaversion, ohne Migration, Sicherung oder Bereinigung. SQLite kann dabei ein
heißes Journal wiederherstellen. `/config` muss schreibbar sein (DB, SQLite-
Nebendateien und Auth-Datei); Modellfreigaben bleiben read-only. **Desktop-App und
Web-Server dürfen nicht gleichzeitig dieselbe Datenbank verwenden, auch nicht
im Lesemodus: entweder Web oder lokal.**

Änderungen laufen unter `BEGIN IMMEDIATE`; die bestehende Lizenzfunktion
behält als ausdrückliche Ausnahme ihre eigene Transaktion mit Zeilenprüfung und
Rollback. Existenzprüfungen dafür erfolgen davor auf derselben Verbindung.
Maximal ein Worker plus 32 wartende Requests; pro Sitzung Token-Bucket mit 60/s,
Burst 120. Rate-Token und Warteschlangenplatz werden vor dem Body-Lesen reserviert;
Abbruch und Fehler geben den Platz frei. 429 und 503 liefern `Retry-After`. Schreib-Bodies: 16 KiB, Druckprotokoll mit Foto und Spulenanlage/-änderung mit Bild: `MAX_CUSTOM_IMAGE_BYTES * 4 / 3 + 16 KiB`; decodiertes Foto höchstens 5 MiB.

Das HTTP-Zeitlimit beträgt 30 Sekunden. Für Foto-/Spulenbild-Bodies gilt ebenfalls eine
Lesefrist von 30 Sekunden; übrige Body-Lesefristen bleiben bei 15 Sekunden. Bereits gestartete Blocking-Arbeit läuft
auch nach Timeout oder Verbindungsabbruch zu Ende; eine Antwort kann verloren gehen,
obwohl die Änderung gespeichert wurde. Wiederholte Neuanlagen können Duplikate erzeugen, insbesondere bei
`create_collection`, `add_print_log_entry`, `add_filament_spool`, `add_printer` und `add_unit`: dabei können Duplikate entstehen.
Nach unklarer Antwort zuerst neu laden. Das Audit enthält nur Zeitpunkt,
gekürzten Sitzungshash, Command, HTTP-Status und betroffene Zeilen: Summe der `changes()` der
Hauptstatements, ohne Tag-Bereinigung, Kaskaden oder Trigger. Bei Manager-Commands
zählen zurückgelegte/verdrängte Spulen und automatisch angelegte Halter/Wannen
nicht mit; Laden auf dasselbe Ziel zählt als No-op 0. Fehlgeschlagene,
zurückgerollte und vorab abgewiesene Aufrufe zählen 0; unveränderte UPDATE-Zeilen
zählt SQLite als betroffen. Nur im Schreibmodus wird nach den Zugriffsprüfungen
ein Audit angelegt; läuft Blocking-Arbeit nach HTTP-Timeout weiter, enthält die
Zeile den HTTP-Status 408 und die abschließende Änderungszahl. Bei Abbruch ohne
HTTP-Antwort verwendet nur das Log den Status 499. Unerwartete Fehler
werden zusätzlich nach Fehlerart ohne Inhalte protokolliert.

- **16.10.2026, Stufe 4 abgenommen:** Backend (`e75d7d76`) und Oberfläche (`e3381f74`). Browsertest per Chromium/CDP gegen Server mit `KATALOG_WRITE=1`: Favorit, Druckstatus, Warteschlange, Tag hinzufügen/entfernen, Sammlung anlegen, Druckprotokoll mit Foto (5-MiB-Grenze sichtbar abgelehnt) schreiben mit Status 200; Kontextmenü ohne Löschen/Umbenennen/Verschieben, Entf-Taste ohne Wirkung; abgelaufene Sitzung führt zur Anmeldung ohne Schreibzugriff; ohne Schreibmodus bleibt die Oberfläche lesend. Kleinigkeiten: Startmeldung sagt auch im Schreibmodus „nur lesend“; Foto-Größenfehler trägt den allgemeinen Präfix „Änderung oder Neuladen fehlgeschlagen“.


Stufe 6 Teil A: Sechs zusätzliche DB-only-Reads sind auch ohne Schreibmodus
verfügbar. Web-Spulenlisten enthalten `hasImage` statt `imagePng`; Bilder werden
über `get_filament_spool_image(id)` einzeln gelesen. Beim Web-Update erhält ein
fehlender Schlüssel `imagePng` das gespeicherte Bild (UPDATE ohne Bildspalte),
`null` löscht es, ein validierter String ersetzt es. Vier parallele Manager-Reads,
maximal 1000 `check_filament.fileIds`, maximal 500 neueste Verlaufseinträge.
Neue Web-Anlagen prüfen atomar 5000 Spulen, 200 Drucker und 50 Einheiten pro Drucker
(einschließlich automatisch angelegter Halter/Wannen); größere Desktop-Bestände
bleiben les-, bearbeit- und reduzierbar. `restock_filament_spool`, `consume_resin`
und die Druckeranbindung bleiben im Web gesperrt. Manager-Oberflächen folgen in Teil B.

English: `KATALOG_WRITE=1` enables 32 commands for metadata and material/printer
management. Six new DB-only reads work without write mode. Web spool lists return
`hasImage` instead of image data; `get_filament_spool_image(id)` loads one image.
Manager reads are limited to four concurrent requests; filament checks accept
1000 file IDs, history returns the latest 500 jobs. Atomic creation-only quotas
are 5000 spools, 200 printers and 50 units per printer, including default holders
and vats. Larger desktop inventories remain readable, editable and reducible.
Restocking, resin consumption and printer networking remain unavailable on the web.
Manager views are deferred to part B.

Web manager audit counts only main-statement rows, excluding cascades, returned/displaced
spools and automatically created holders/vats. Loading into the same slot counts 0.
Web spool updates preserve the image when `imagePng` is omitted, clear it on `null`,
and validate and replace it for a string.
- **16.10.2026, Stufe 6 abgenommen:** Backend (`c1874ad8`) und Oberfläche (`3f65712c`). Material Manager und Drucker Manager im Browser: 14 Schreib- und 6 Lese-Commands, strikte Web-Varianten in einer Transaktion, Web-Spulenliste ohne Bilddaten (`hasImage`, Einzelabruf `get_filament_spool_image`, lazy), Bild bleibt beim Bearbeiten erhalten (Schlüssel fehlt = behalten, `null` = löschen), Quoten bei Neuanlage (5000/200/50), Löschfolgen vor dem Bestätigen sichtbar. Unabhängige Prüfung (Opus): kein Blocker, ein mittlerer Befund (Bildverlust beim Bearbeiten) behoben. Browsertest per Chromium/CDP: Spule mit Bild anlegen, bearbeiten ohne Bildverlust, Drucker anlegen, Löschdialog mit Folgen, ohne Schreibmodus alle Schreibknöpfe gesperrt; Druckeranbindung und „Drucke bestätigen“ verborgen. Tests: 1.158 Rust (0 Fehler außerhalb der Sandbox), 2.202 Frontend, Web-Bundle ohne Tauri. `restock_filament_spool` und `consume_resin` bleiben gesperrt (DB-only, später).
- **Nächster Schritt: Stufe 5** (Import per Upload, SSE, Upload- und Entpacklimits). Spec und Review vor dem Commit durch Opus (Entscheidung 16.10.).

### Browser-Upload und Import (Backend Stufe 5, Deutsch)

Uploads sind standardmäßig aus. `KATALOG_UPLOAD_DIR=/models/uploads` aktiviert das
Backend zusammen mit `KATALOG_WRITE=1`, Anmeldung und einer expliziten
`KATALOG_ALLOWED_HOSTS`-Liste. Die Browser-Oberfläche folgt separat in Teil C.
**Upload legt Dateien dauerhaft auf dem Server ab: nur im Heimnetz oder über VPN
betreiben.** Desktop und Webserver dürfen niemals gleichzeitig dieselbe DB nutzen.
Nur `.3mf`, `.stl`, `.obj`, `.stp` und `.step` sind erlaubt; Archive (ZIP/7z/RAR)
bleiben der Desktop-App vorbehalten. Dateien bleiben im Upload-Ordner, ohne
Zielordner-Auswahl und ohne Kopieren/Verschieben beim Import. STEP wird ohne
OCCT/`step-preview` importiert: keine STEP-Metadaten oder Vorschau im Server.

Den Upload-Ordner vorher anlegen (0755) und gezielt schreibbar einbinden; er muss
unter `KATALOG_ROOTS` liegen. Der Prozess mit `PUID`/`PGID` (Standard 1000:1000)
braucht dort sowie bei DB und Auth-Datei Schreibrechte. Upload-Dateien erhalten
0644. Ein Symlink als Upload-Ordner und Überschneidungen (gleich, Eltern-/Kindpfad)
mit DB-, Auth-, Trash- oder Static-Verzeichnissen verhindern den Start. Andere
Modellfreigaben können lesend bleiben.

`KATALOG_UPLOAD_MAX_MB` ist die Dateigrenze in MiB (Standard 1024, 1–4096),
`KATALOG_UPLOAD_MAX_FILES` begrenzt gleichzeitig laufende Uploads (Standard 2,
1–32). STL/OBJ sind zusätzlich auf 512 MiB begrenzt. Niedrigere Parsergrenzen:
`KATALOG_IMPORT_MAX_MESH_MB=512`, `KATALOG_IMPORT_MAX_UNPACKED_MB=1024`,
`KATALOG_IMPORT_MAX_VERTICES=2000000`, `KATALOG_IMPORT_MAX_TRIANGLES=3000000`.
Diese vier Werte sind positive ganze Zahlen und dürfen die genannten Standards
nicht überschreiten. Falsche Zahlen verhindern den Start, auch bei deaktiviertem
Upload. Ein Import-Job verarbeitet sequenziell; die Grenzen sind keine RSS-Grenze:
Parser, XML, Mesh und Metadaten können mehrere Kopien benötigen. Mehrere GiB freien
RAM einplanen und bei knappen Containern die Grenzen senken; gemessener
Spitzenspeicher hängt von den Dateien ab.

Nicht importierte Uploads sind auf 200 Dateien/20 GiB begrenzt, zusätzlich bleiben
256 MiB Speicherplatzreserve. Das dauerhafte Journal `web-uploads.json` liegt neben
`KATALOG_AUTH_FILE` (0600); zusammen mit DB und Upload-Dateien sichern. Alle
angemeldeten Sitzungen teilen Uploads und Jobs. Logout und Passwortwechsel löschen
keine Uploads und stoppen keine Import-Jobs. Duplikate und ungültige Dateien bleiben
liegen und können verworfen werden; ein abgebrochener Job kann bereits importierte
Dateien enthalten. Beim Neustart gleicht der Server Journal und Katalog ab.

Reverse Proxy: Body-Limit mindestens auf die gewünschte Upload-Grenze setzen
(sonst HTTP 413); Streaming und SSE ohne Pufferung erlauben. Uploads haben maximal
eine Stunde Gesamtdauer und 60 Sekunden ohne 64 KiB Fortschritt. SSE-Pings kommen
alle 15 Sekunden, verlängern aber nicht die Sitzung. Uploads erfordern CSRF,
passenden Origin und Content-Length; Proxy darf diese Header nicht entfernen.

### Browser upload and import (stage 5 backend, English)

Uploads are disabled by default. Set `KATALOG_UPLOAD_DIR=/models/uploads` together
with `KATALOG_WRITE=1`, authentication and an explicit `KATALOG_ALLOWED_HOSTS` list.
The browser interface follows separately in part C. **Uploads store files permanently
on the server: use a home network or VPN only.** Never run the desktop app and web
server against the same database concurrently. Supported files are `.3mf`, `.stl`,
`.obj`, `.stp`, `.step`; ZIP/7z/RAR archives require the desktop app. Files remain in
the upload directory: import neither copies nor moves them and has no target-folder
selection. The server excludes OCCT/`step-preview`, so STEP metadata and preview
are unavailable.

Create the upload directory first (0755) and mount it writable. It must be under
`KATALOG_ROOTS`; the process UID/GID (`PUID`/`PGID`, default 1000:1000) needs write
access there and to DB/auth storage. Uploaded files use 0644. Startup rejects an
upload directory symlink or any ancestor/descendant overlap with DB, auth, trash
or static directories. Other model shares can remain read-only.

`KATALOG_UPLOAD_MAX_MB` sets the per-file MiB limit (default 1024, range 1–4096);
`KATALOG_UPLOAD_MAX_FILES` limits concurrent uploads (default 2, range 1–32).
STL/OBJ also have a 512 MiB parser limit. Lower-only parser settings are
`KATALOG_IMPORT_MAX_MESH_MB=512`, `KATALOG_IMPORT_MAX_UNPACKED_MB=1024`,
`KATALOG_IMPORT_MAX_VERTICES=2000000`, `KATALOG_IMPORT_MAX_TRIANGLES=3000000`.
They require positive integers no larger than those defaults. Invalid numeric
configuration fails startup even with uploads disabled. Only one import job runs,
parsing sequentially. These limits do not cap RSS: parser/XML/mesh/metadata copies
can require several GiB RAM. Lower limits for small containers; measured peak
memory depends on the files.

Pending uploads are limited to 200 files/20 GiB with a 256 MiB disk-space reserve.
The durable `web-uploads.json` journal (0600) sits beside `KATALOG_AUTH_FILE`; back
it up with the database and uploaded files. All authenticated sessions share jobs
and uploads. Logout/password changes neither delete uploads nor stop import jobs.
Duplicates and invalid files remain available for discard; cancellation can leave
already committed imports. Startup reconciles the journal with the catalog.

Set reverse-proxy body limits high enough (otherwise HTTP 413), enable streaming
and disable SSE buffering. Upload deadlines are one hour total and 60 seconds
without 64 KiB progress. SSE pings every 15 seconds do not refresh session activity.
Preserve CSRF, Origin and Content-Length headers at the proxy.

SSE-Neuverbindungen erhalten einen Snapshot der aktiven Jobs, der letzten 32
Job-Ergebnisse und der offenen Uploads (höchstens 200). Ältere Ergebnisse bleiben
über `get_import_result { jobId }` dauerhaft abrufbar. Die Puffer enthalten höchstens
32 Ereignisse je Verbindung; erlaubt sind vier Verbindungen je Sitzung und 32 insgesamt.
Web-Import-Batches enthalten eine Datei, damit große Metadaten sich nicht aufsummieren.

SSE reconnections receive active jobs, the latest 32 job results, and pending uploads
(up to 200). Older results remain available through `get_import_result { jobId }`.
Each connection buffers at most 32 events; limits are four connections per session
and 32 overall. Web import batches contain one file to bound retained metadata.

- **16.10.2026, Stufe 5 abgenommen (Import per Upload):** Teil A (`230d5f54`, Importmaschine im Core, verhaltensgleich), Teil B (`65891bf5`, Server: `POST /api/upload`, Journal, Import-Commands, SSE), Nachtrag aus dem Review von Teil A (`f4c54cf7`), Runde 3 (`df9ec449`, Härtung nach Opus-Sicherheitsreview), Teil C (`7d3e2248`, Oberfläche), Titelkorrektur (`f179e711`). Reviews: Codex-Planprüfung und Opus-Sicherheitsreview der Spec (v1 verworfen, v2), Opus-Regressionsreview von Teil A (kein Rückschritt), Opus-Sicherheitsreview von Teil B (kein hoher Befund, drei mittlere behoben). Tests: 1.231 Rust (0 Fehler außerhalb der Sandbox), 2.240 Frontend, Web-Bundle ohne Tauri, Server ohne OCCT.
- **Browsertest (Chromium/CDP, echter Server, Testordner):** Upload von sieben Dateien: vier importiert (3MF, STL, OBJ, geslicte `.gcode.3mf`), Duplikat erkannt und verwerfbar, Fake-3MF (400) und Archiv (nur Desktop) mit sichtbaren Fehlermeldungen; Ordner „Eingang“ im Katalog angelegt; Fortschritt per SSE; zu großes Mesh → `invalid`; **Absturz mitten im Import** (Server eingefroren und mit SIGKILL beendet): nach Neustart Eintrag wieder `uploaded`, Job `failed`, Datei erneut importierbar. Start verweigert, wenn der Upload-Ordner unter dem DB-/Auth-Ordner liegt (Docker-Layout `/config` getrennt von `/models`).
- **Offen:** Spitzenspeicher beim Import nicht gemessen; Verhalten bei vollem Datenträger und echtem Stromausfall nicht geprüft; Archive (Stufe 5b), Dateiverwaltung (Stufe 7) und Rest laut Abschnitt 10a.

- **16.10.2026, Stufe 6c abgenommen (Render-Snapshots im Browser):** `80a91806`. Der Browser erzeugt fehlende 3D-Vorschaubilder im Hintergrund (nur Schreibmodus mit WebGL, ein Modell zur Zeit, Pause bei unsichtbarem Tab, Fehler überspringen) und speichert sie über `set_render_snapshot` in der Datenbank. Ursache des Befunds: In der Webversion lief der Hintergrund-Renderer der Desktop-App nicht, neue Uploads blieben bei ihrem Titelbild. Bei der Einstellung „Bild bevorzugen“ (Standard) entstehen Snapshots nur für Modelle ohne Titelbild, bei „3D bevorzugen“ für alle. Browsertest mit Software-WebGL: Kürbis-Upload bekam seinen Snapshot (53 KB), Nutzer bestätigt. Opus-Review: kein Blocker. **Nachbesserung (niedrig, offen):** eigene kleinere Grenze für Web-Snapshots (z. B. 1 MiB statt 5 MiB, wegen Lese-Verstärkung über `list_file_images`) und Base64-Dekodierung vor `BEGIN IMMEDIATE`.
- **16.10.2026, Nutzerwunsch:** Dateiverwaltung im Web (Umbenennen, Verschieben, Papierkorb, Ordner) gilt als essentiell: Stufe 7 wird vorgezogen (Spec `spec-web-fileops-7.md`, Codex- und Opus-Prüfung liegen vor, v2 in Arbeit). Zusätzlich 6d „Aus dem Katalog entfernen“ (DB-only) vorab.

- **16.10.2026, Stufe 6d abgenommen („Aus dem Katalog entfernen“):** `020a4f12`. `remove_files_from_catalog` (nur Katalogzeilen, Datei bleibt), Sperre bei laufendem Import (503 mit `Retry-After`), Upload-Journal-Rücksetzung `imported → uploaded`, Knöpfe in Kontextmenü, Sammelleiste und beiden Detailansichten. Browsertest: hochgeladene Datei importiert, entfernt (DB-Zeile weg, Datei bleibt, erscheint unter „Offene Uploads“), verworfen (Datei vom Server gelöscht, Journal-Eintrag weg). Opus-Review: kein Blocker, vier niedrige Hinweise (fehlender 413-Test für den neuen Command, `job_id` beim Start-Abgleich nicht geleert, Duplikat-Einträge bleiben `duplicate`, kein Hinweis nach Journalfehler); Hinweistext nennt jetzt Druckprotokoll/Tags/Sammlungen.
- **16.10.2026, Oberfläche (Nutzertest):** Konto-Menü in der Kopfzeile statt schwebender Leiste (`0dcb6481`), Anbindungsstatus („Inaktiv“, „Keine Anbindung“) in der Druckerliste im Web ausgeblendet (`7115603f`), Detailbereich standardmäßig festgehalten, damit das Raster nicht springt (`e13cbdf3`).
- **Stufe 7 (Dateiverwaltung):** Spec `spec-web-fileops-7.md` v2 plus verbindliche v3-Nachträge (Papierkorb pro Wurzel im selben Dateisystem, nur `renameat2(NOREPLACE)`, Schutzmatrix je Ort, Intent-Journal mit Entscheidungstabelle, Passwort-Wiederholung und Tagesgrenzen für unwiderrufliche Operationen). Aufteilung A1 (Primitive, Journal, Dateien, Papierkorb), A2 (Ordner), B (Server), C (Oberfläche).
- **16.10.2026, Raster springt (Nutzertest):** Ursache war das Raster selbst: `repeat(n, minmax(0, 1fr))` verteilte den Rest der Zeilenbreite auf die Kacheln, deshalb wurden sie beim Öffnen und Schließen des Detailbereichs breiter oder schmaler (auch in der Desktop-App so). Meine ersten Versuche (Detailbereich standardmäßig oder immer festhalten, Umschalter ausblenden) behandelten das Symptom und sind zurückgenommen. Lösung: im Web feste Kachelbreite (190 Pixel kompakt, 232 komfortabel), nur die Spaltenzahl wechselt, Rest der Zeile bleibt rechts frei; bei zwei oder weniger Spalten (schmale Fenster, Handy) bleibt das Dehnen. Desktop unverändert, kann das später übernehmen.

- **16.10.2026, Entscheidung zum Docker-Stream:** Bleibt in 0.16.0 als experimentell und im Wartungsmodus (nur Fehler- und Sicherheitskorrekturen); die Webversion wird dort nicht erwähnt. Ablösung erst, wenn die Webversion die Stream-Funktionen abdeckt (Stufen 5b und 7) und ein Umstieg erklärt ist.

### Stufe 7 / Stage 7 — Teil B Server

Der Server stellt die Datei-/Ordner- und Snapshot-Papierkorb-Commands mit separater
Freigabe `KATALOG_FILE_OPS` bereit. Teil C (Oberfläche) bleibt ein eigener Auftrag.
Fileops nutzen das Import-`rt.active`-Permit plus eigenen Mutex, niemals
`app.changes`; Upload-Journal wird nach DB-Commit ohne verschachtelten DB-Lock
abgetrennt. Start: Upload laden → Fileops-Recovery/ACK → DB neu lesen → Upload
abgleichen. N6/N7-Limits und N8-Sicherungen gelten gemeinsam auch für DB-only-
Katalogentfernung. Sicherungstage sind UTC-Kalendertage, Reserve 64 MiB.

The server exposes gated file/folder operations and snapshot-driven trash blocks;
stage C UI remains separate. Admission uses the import permit and fileops mutex,
not the authentication changes lock. Destructive limits and backups include
DB-only catalog removal. Backup days use UTC and require 64 MiB reserve.

Counting rules (Bfix/Bfix2): folder removal charges at most one full window;
a batch above a window limit gets 422 `limit_exceeded` without `Retry-After`;
empty-trash blocks count only against the daily limit. `remove_files_from_catalog`
(6d) changes behaviour even without `KATALOG_FILE_OPS` on purpose: it shares the
limits (200 per 10 minutes, daily limit) and triggers backups, so it may answer
429 or 503 `backup_failed` (space DB + 64 MiB, `<db>/backups` not 0700/owner,
foreign `web-fileops-*.db`); backups live in `<db directory>/backups/`.

Betrieb/Operations: Warnungen und ACK-Verfahren in README.de.md, README.md und
`docker-web/README.md` beachten: kein paralleler Desktop, Desktop-Purge nach 7 Tagen,
Root-Papierkorb ohne Web-Automatik, schreibbare Volumes und passende Rechte,
NOREPLACE-fähiges Dateisystem, keine Cross-Filesystem-Moves, Heimnetz/VPN und
externe Backups. / Read those warnings before enabling file management.

- **17.10.2026, Stufe 7 abgenommen (Dateiverwaltung):** B-/C-Review-Korrekturen und Tageslimit für Papierkorb-Blöcke umgesetzt (Sonnet, da Codex-Limit bis 22.10.), unabhängig geprüft (Clippy sauber, Rust-Tests grün, Frontend 2350 Tests, kein Tauri im Server/Bundle). Browser-/API-Abnahme gegen die Testumgebung: Umbenennen, Verschieben, Papierkorb/Wiederherstellen, endgültig löschen mit Passwort, Papierkorb leeren in Blöcken, Ordner anlegen/umbenennen/verschieben/aus Katalog entfernen, Schutzorte (409), Zyklus (409), falscher Origin (403), unbekannte Argumente (422), Batch >200 (422); vier Absturzrunden (SIGKILL mitten in Schreibvorgängen) ohne Inkonsistenz zwischen DB und Platte. Offen/klein: verwaiste `.fsops-*.part` im Konfigurationsordner nach hartem Abbruch wird nicht aufgeräumt; Startmeldung/Hilfetext nennt noch „Katalog nur lesend“. Merge `f849428e`.

- **17.10.2026, Gesamt-Sicherheitsreview (4 Opus-Gebiete: Anmeldung/HTTP, Dateisystem, Dateien/Upload/Befehle, Frontend/Docker/Abhängigkeiten):** keine kritischen oder hohen Befunde. Umgesetzt: Hostliste Pflicht bei Nicht-Loopback-Bindung (Docker-Voreinstellung gesetzt), Zähler/Grenzen im 3MF-Parser und Web-Grenzen für die 3D-Vorschau, `list_files_by_ids` ≤ 500, kurzes Startpasswort erzwingt Änderung, Aufräumen verwaister Temp-Dateien, Integritätsprüfung des Upload-Journals, `O_NOCTTY`, Verbindungsgrenze pro IP (32, aus bei TRUST_PROXY) und 5 s Header-Frist, Startwarnung für Proxy-Modus, richtige Startmeldung, SECURITY.md-Restrisiken. **Offen (bewusst später):** `zip` 2.4.2 (yanked; betroffene Funktionen ungenutzt) anheben, CI-Pfadfilter/cargo-deny für den Server, Lizenzliste und Lizenzen im Image, Image-Digests/Healthcheck, Mengenquoten für Fotos/Tags/Spulen, Speicherbudget und Semaphor für schwere Lesebefehle (`list_trash`, `list_collection_files`), Passwortabfrage für „Ordner aus Katalog entfernen“ ab Schwelle, TRUSTED_PROXIES (IP/CIDR), Argon2-Parameter-Obergrenze, `Cross-Origin-Resource-Policy`.
