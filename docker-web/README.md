# docker-web: Web-Version des 3MF Katalog Manager (Entwurf, nicht öffentlich)

**Deutsch.** Image (ca. 69 MB Download, Summe der komprimierten Schichten; entpackt ca. 180 MB) mit `katalog-server`, STEP-Vorschau und der Web-Oberfläche. Es zeigt einen **vorhandenen** Katalog **standardmäßig nur lesend** im Browser; kein gestreamter Desktop. Gleiche Ordner wie das Desktop-Image: `./config` (Programmdaten, Heimatordner der App) und `./models`.

```
cd docker-web
mkdir -p config models          # Katalog: config/.local/share/com.thebexxs.mfkatalogmanager/catalog.db
docker compose up -d --build
cat config/web-initial-password.txt # beim ersten Start steht dort (nicht im Log) das Zufallspasswort
```
- Erreichbar unter `http://127.0.0.1:8080`. Für andere Geräte im Heimnetz `KATALOG_BIND_IP` auf die Adresse des Servers setzen und `KATALOG_ALLOWED_HOSTS` eintragen (Pflicht, sonst startet der Server nicht; Compose leitet für `localhost`/`127.0.0.1` den Standard aus `KATALOG_PORT` ab). Von außen nur per VPN oder Reverse Proxy mit HTTPS.
- Der Katalog muss zuvor von der Desktop-App (oder dem Desktop-Image) in der **gleichen Version** angelegt oder migriert sein; der Webserver migriert nie.
- Passwort: erster Start aus `KATALOG_PASSWORD(_FILE)` (z. B. `/config/.kasmvnc-password` des Desktop-Images) oder zufällig; ein erzeugtes Zufallspasswort steht **nicht** im Log, sondern in `config/web-initial-password.txt` (Rechte 0600, der Start-Hinweis im Log nennt nur den Pfad); nach dem ersten Anmelden ändern, die Datei wird dann entfernt. Kann sie nicht geschrieben werden, bricht die Ersteinrichtung ab. Danach als Argon2-Hash in `config/web-auth.json`, änderbar in der Oberfläche. Vergessen: `docker compose exec katalog-web katalog-server set-password` ist im Image ohne Shell nicht möglich, daher `docker compose run --rm -it katalog-web set-password`.
- Das Dateisystem des Containers ist schreibgeschützt, Modelle werden nur lesend eingebunden, der Prozess läuft ohne Root und ohne Linux-Fähigkeiten. Bei `read_only: true` braucht SQLite ein beschreibbares `/tmp` (tmpfs, in `docker-compose.yml` gesetzt), sonst meldet der Server beim ersten Laden „disk I/O error“.

**English.** Image (about 69 MB download, sum of the compressed layers; about 180 MB unpacked) with `katalog-server`, the STEP preview and the web UI. It shows an **existing** catalog **read-only by default** in the browser; no streamed desktop. Same folders as the desktop image: `./config` (app data, the app's home) and `./models`. Run `docker compose up -d --build` in `docker-web/`; a random password is written on first start to `config/web-initial-password.txt` (mode 0600, owner only; it is not printed to the log, which only names the path), and the file is removed once the password has been changed. Reachable at `http://127.0.0.1:8080` by default; set `KATALOG_BIND_IP` and `KATALOG_ALLOWED_HOSTS` (mandatory, the server refuses to start otherwise; Compose derives the `localhost`/`127.0.0.1` default from `KATALOG_PORT`) for other devices in the home network, use a VPN or an HTTPS reverse proxy for access from outside. The catalog must already exist in the same version (the web server never migrates). The password is stored as an Argon2 hash in `config/web-auth.json` and can be changed in the UI; to reset it use `docker compose run --rm -it katalog-web set-password`. With `read_only: true` SQLite needs a writable `/tmp` (tmpfs, set in `docker-compose.yml`), otherwise the server reports "disk I/O error" on the first load.

**Healthcheck:** the image has no shell or curl, so `katalog-server health` asks the server's own `/api/health` (exit code 0 = healthy); the Dockerfile `HEALTHCHECK` and the Compose `healthcheck:` use it.

**Open:** Material/printer manager views (stage 6, part B), and the import interface (stage 5, part C). Downloads and the optional metadata and material/printer management backend are implemented.

**Optional write mode (English).** `KATALOG_WRITE=1` enables the 32 write commands for metadata and material/printer management (commented Compose option). Authentication and `KATALOG_ALLOWED_HOSTS` are mandatory; `/config` must be writable. **Desktop and web now refuse concurrent operation on the same catalog, including read-only mode (see Catalog instance lock below).** Started database work continues after the 30-second HTTP timeout. Reload before retrying: creating collections, print log entries, spools, printers or units again can produce duplicates. Metadata editing is available; manager views will be added in stage 6 part B.

**Network binding and proxies (English).** If `KATALOG_BIND` is not a loopback address, `KATALOG_ALLOWED_HOSTS` is required and the server refuses to start without it (DNS-rebinding protection); list every name and `host:port` browsers use. The server limits connections to 32 per client address (IPv6: per /64) in addition to 256 in total. `KATALOG_TRUST_PROXY=1` makes the server believe `X-Forwarded-For` and `X-Forwarded-Proto`, but only from direct peers listed in `KATALOG_TRUSTED_PROXIES` (comma-separated IPs or CIDR networks, IPv4 and IPv6, e.g. `172.18.0.5,::1`; `/0` is refused; the server logs a warning for ranges wider than /24 (IPv4) or /64 (IPv6)). **`KATALOG_TRUSTED_PROXIES` is mandatory with `KATALOG_TRUST_PROXY=1`: without a valid list the server does not start.** Headers from any other peer are dropped and that peer counts with its own address. The proxy must append the connecting address to `X-Forwarded-For`; the server reads the chain from the right and takes the first address that is not itself a trusted proxy as the client, so entries a client forges on the left are never used. Trusted proxies are exempt from the per-address connection limit (only the global limit applies to them); every other peer keeps it. **List only the proxy's fixed address (`/32` for IPv4, `/128` for IPv6), not a whole network.** Everyone who can connect from a trusted range can set `X-Forwarded-For` freely, so a broad range such as the Compose subnet `172.18.0.0/16` is NOT recommended: an attacker in that range gets a new "client address" per request and bypasses the per-client login lock (only the global limit of 100 requests per minute and the two simultaneous password checks remain). For the same reason do not publish port 8080 (remove the `ports:` entry of the server in `docker-compose.yml`) when a proxy in front of it is the only way in. Example (Docker, Caddy/nginx on the same Compose network; give the proxy a fixed address with `ipv4_address:` in the Compose file, here `172.18.0.5`): `KATALOG_TRUST_PROXY=1`, `KATALOG_TRUSTED_PROXIES=172.18.0.5/32`, `KATALOG_ALLOWED_HOSTS=katalog.example`, and in nginx `proxy_set_header X-Forwarded-For $remote_addr; proxy_set_header X-Forwarded-Proto $scheme;` (Caddy `reverse_proxy` sets both by default).

**Healthcheck:** Das Image enthält weder Shell noch curl; `katalog-server health` fragt deshalb das eigene `/api/health` ab (Exit-Code 0 = gesund). Dockerfile-`HEALTHCHECK` und Compose-`healthcheck:` nutzen ihn.

**Offen:** Material-/Druckerverwaltungsansichten (Stufe 6, Teil B) und die Import-Oberfläche (Stufe 5, Teil C). Downloads und das optionale Backend für Metadaten sowie Material- und Druckerverwaltung sind umgesetzt.

**Optionaler Schreibmodus (Deutsch).** `KATALOG_WRITE=1` aktiviert die 32 Schreib-Commands für Metadaten sowie Material- und Druckerverwaltung (auskommentierte Compose-Option). Anmeldung und `KATALOG_ALLOWED_HOSTS` sind Pflicht; `/config` muss schreibbar sein. **Desktop und Web verweigern jetzt den gleichzeitigen Betrieb auf demselben Katalog, auch im Lesemodus (siehe Katalog-Instanzsperre unten).** Gestartete DB-Arbeit läuft nach einem 30-s-HTTP-Timeout weiter. Vor Wiederholung neu laden: neue Sammlungen, Druckprotokolle, Spulen, Drucker und Einheiten können doppelt entstehen. Metadaten sind im Browser bearbeitbar; die Manager-Ansichten folgen in Stufe 6 Teil B.

**Netzbindung und Proxy (Deutsch).** Ist `KATALOG_BIND` keine Loopback-Adresse, ist `KATALOG_ALLOWED_HOSTS` Pflicht, und der Server startet sonst nicht (Schutz vor DNS-Rebinding); alle Namen und `Host:Port`, unter denen Browser den Server erreichen, eintragen. Der Server begrenzt Verbindungen auf 32 pro Client-Adresse (IPv6: pro /64) zusätzlich zu insgesamt 256. `KATALOG_TRUST_PROXY=1` lässt den Server `X-Forwarded-For` und `X-Forwarded-Proto` glauben, aber nur von direkten Peers aus `KATALOG_TRUSTED_PROXIES` (kommagetrennte IPs oder CIDR-Netze, IPv4 und IPv6, z. B. `172.18.0.5,::1`; `/0` wird abgelehnt; für Bereiche breiter als /24 (IPv4) bzw. /64 (IPv6) schreibt der Server eine Warnung ins Log). **`KATALOG_TRUSTED_PROXIES` ist bei `KATALOG_TRUST_PROXY=1` Pflicht: ohne gültige Liste startet der Server nicht.** Header jedes anderen Peers werden verworfen, dieser Peer gilt mit seiner eigenen Adresse. Der Proxy muss die Verbindungsadresse an `X-Forwarded-For` anhängen; der Server liest die Kette von rechts und nimmt die erste Adresse, die nicht selbst ein vertrauenswürdiger Proxy ist, als Client – links vom Client gefälschte Einträge werden nie benutzt. Vertrauenswürdige Proxys sind von der Pro-Adresse-Verbindungsgrenze ausgenommen (für sie gilt nur das globale Limit), alle anderen Peers behalten sie. **Nur die feste Adresse des Proxys eintragen (`/32` bei IPv4, `/128` bei IPv6), kein ganzes Netz.** Jeder, der aus einem vertrauten Bereich verbinden kann, setzt `X-Forwarded-For` frei; ein breiter Bereich wie das Compose-Subnetz `172.18.0.0/16` wird deshalb ausdrücklich NICHT empfohlen: Ein Angreifer in diesem Bereich bekommt pro Anfrage eine neue „Client-Adresse“ und umgeht die Anmeldesperre pro Client (es bleiben nur das globale Limit von 100 Anfragen pro Minute und die zwei gleichzeitigen Passwortprüfungen). Aus demselben Grund Port 8080 nicht veröffentlichen (den `ports:`-Eintrag des Servers in der `docker-compose.yml` entfernen), wenn nur ein davorstehender Proxy den Zugang bildet. Beispiel (Docker, Caddy/nginx im selben Compose-Netz; dem Proxy mit `ipv4_address:` in der Compose-Datei eine feste Adresse geben, hier `172.18.0.5`): `KATALOG_TRUST_PROXY=1`, `KATALOG_TRUSTED_PROXIES=172.18.0.5/32`, `KATALOG_ALLOWED_HOSTS=katalog.example`, in nginx `proxy_set_header X-Forwarded-For $remote_addr; proxy_set_header X-Forwarded-Proto $scheme;` (Caddy `reverse_proxy` setzt beides standardmäßig).

**Smoke-Test (Deutsch).** `bash docker-web/scripts/ci-smoke.sh IMAGE` startet das Image schreibgeschützt mit einem frisch erzeugten, leeren Katalog und prüft Web-Oberfläche, Abweisung ohne Anmeldung, Anmeldung, eine lesende Katalogabfrage und sauberes Stoppen. Der Katalog entsteht mit `cargo run -p katalog-core --no-default-features --example make_smoke_catalog -- <Pfad>`; ohne lokales Rust kann `SMOKE_CATALOG_CMD` das ersetzen (siehe Skriptkopf). Der Workflow `.github/workflows/docker-web.yml` baut das Image und führt diesen Test aus.

**Smoke test (English).** `bash docker-web/scripts/ci-smoke.sh IMAGE` starts the image read-only with a freshly created empty catalog and checks the web UI, rejection without login, login, one catalog read and a clean stop. The catalog is created with `cargo run -p katalog-core --no-default-features --example make_smoke_catalog -- <path>`; without a local Rust toolchain `SMOKE_CATALOG_CMD` can replace that (see the script header). The workflow `.github/workflows/docker-web.yml` builds the image and runs this test.

**STEP-Vorschau (Deutsch).** Das Image ist mit `--features step-preview` gebaut und zeigt STEP-Dateien in der 3D-Ansicht. Jede STEP-Datei liest ein eigener, begrenzter Hilfsprozess des Servers. Er braucht **Linux-Landlock** (Kernel ab 5.13, im Kernel aktiviert) und darf es unter dem Seccomp-Profil des Containers aufrufen; Dockers Standardprofil erlaubt das (geprüft mit Docker 29.10 auf Kernel 7.3). Fehlt Landlock oder schlägt die Startprüfung fehl, bleibt die STEP-Vorschau aus: Der Server läuft normal weiter, schreibt einmal `STEP-Vorschau deaktiviert: …` ins Log (`docker compose logs katalog-web`) und meldet STEP-Dateien als nicht darstellbar. Kein eigenes Seccomp-Profil, `seccomp=unconfined` oder zusätzliche Fähigkeiten nötig; die Compose-Einstellungen (`read_only`, `cap_drop: [ALL]`, `no-new-privileges`) bleiben. Nur **amd64**: Mit STEP-Vorschau ist arm64 (z. B. Raspberry Pi) noch nicht gebaut und nicht geprüft.

**Lizenz Open CASCADE (Deutsch).** Die STEP-Vorschau nutzt Open CASCADE Technology (OCCT) 7.8.1 unter der **LGPL 2.1 mit der Open-CASCADE-Ausnahme**, dynamisch gelinkt. Die Bibliotheken (`/usr/local/lib/libTK*.so.7.8`) sind unverändert aus den Debian-trixie-Paketen `libocct-*-7.8` (Quellpaket `opencascade` 7.8.1+dfsg1-3) übernommen. Paketliste mit genauen Versionen und die Debian-Copyright-Dateien liegen im Image unter `/usr/share/doc/occt-runtime/` (`SOURCES.txt`, `*.copyright`). Quellcode: <https://sources.debian.org/src/opencascade/7.8.1+dfsg1-3/>, dauerhaft auch über <https://snapshot.debian.org/package/opencascade/7.8.1%2Bdfsg1-3/>. Welche Bibliotheken kopiert und welche Pakete per apt installiert werden, ermittelt `docker-web/scripts/collect-occt-runtime.sh` beim Bauen per `ldd` aus dem gelinkten Server.

**STEP preview (English).** The image is built with `--features step-preview` and shows STEP files in the 3D view. Each STEP file is read by a separate, limited helper process of the server. It needs **Linux Landlock** (kernel 5.13 or newer, enabled in the kernel) and must be allowed to call it under the container's seccomp profile; Docker's default profile allows this (checked with Docker 29.10 on kernel 7.3). Without Landlock, or if the start-up probe fails, the STEP preview stays off: the server keeps running, logs `STEP-Vorschau deaktiviert: …` once (`docker compose logs katalog-web`) and reports STEP files as not viewable. No custom seccomp profile, `seccomp=unconfined` or extra capabilities are needed; the Compose settings (`read_only`, `cap_drop: [ALL]`, `no-new-privileges`) stay. **amd64 only**: arm64 (e.g. Raspberry Pi) with the STEP preview is not built or tested yet.

**Open CASCADE license (English).** The STEP preview uses Open CASCADE Technology (OCCT) 7.8.1 under the **LGPL 2.1 with the Open CASCADE exception**, linked dynamically. The libraries (`/usr/local/lib/libTK*.so.7.8`) are taken unchanged from the Debian trixie packages `libocct-*-7.8` (source package `opencascade` 7.8.1+dfsg1-3). The package list with exact versions and Debian's copyright files are in the image under `/usr/share/doc/occt-runtime/` (`SOURCES.txt`, `*.copyright`). Source code: <https://sources.debian.org/src/opencascade/7.8.1+dfsg1-3/>, permanently also at <https://snapshot.debian.org/package/opencascade/7.8.1%2Bdfsg1-3/>. Which libraries are copied and which packages apt installs is worked out at build time by `docker-web/scripts/collect-occt-runtime.sh` from `ldd` of the linked server.

**STEP-Rauchtest und Größe (Deutsch).** `bash docker-web/scripts/step-smoke.sh IMAGE` startet das Image wie `docker-compose.yml` (schreibgeschützt, ohne Fähigkeiten, Dockers Standard-Seccomp-Profil), lädt einen STEP-Würfel hoch, importiert ihn und prüft die Vorschau (10 mm auf jeder Achse), die Ablehnung einer Datei ohne STEP-Inhalt, eine beschädigte STEP-Datei und mit einem Seccomp-Profil ohne Landlock die Logzeile und die abgeschaltete Vorschau. `bash docker-web/scripts/image-size.sh IMAGE` misst die Summe der komprimierten Schichten (Maß für die 250-MB-Grenze), ohne etwas hochzuladen.

**STEP smoke test and size (English).** `bash docker-web/scripts/step-smoke.sh IMAGE` starts the image like `docker-compose.yml` (read-only, no capabilities, Docker's default seccomp profile), uploads and imports a STEP cube and checks the preview (10 mm on every axis), the refusal of a file without STEP content, a damaged STEP file and, with a seccomp profile without Landlock, the log line and the preview being off. `bash docker-web/scripts/image-size.sh IMAGE` measures the sum of the compressed layers (the measure for the 250 MB budget) without uploading anything.

**Browser-Test W2 (Deutsch).** `bash docker-web/scripts/w2-run.sh` (Skriptkopf nennt die Umgebungsvariablen) startet das Image mit `KATALOG_WRITE=1` und `KATALOG_FILE_OPS=1` auf zwei Wegwerf-Modellordnern (erfundene 3MF-Dateien aus `w2-fixture.sh`, Port nur auf 127.0.0.1) und spielt `w2-browser-test.mjs` im Playwright-Image durch: Bereinigungs- und Abgleich-Dialog (Auswahl, Vorschau, blockweises Anwenden, Abbrechen, Ergebnis), Datumsumbenennung, Einstellungen, Start-Abgleich mit Hinweisleiste, leere Wurzel; dazu Screenshots hell/dunkel bei 1280 und 400 px, axe-Prüfung und Datenbank-Gegenproben. Nicht Teil der CI; Builds und Läufe nur auf ObiWan.

**Browser test W2 (English).** `bash docker-web/scripts/w2-run.sh` (the script header lists the environment variables) starts the image with `KATALOG_WRITE=1` and `KATALOG_FILE_OPS=1` on two throw-away model folders (invented 3MF files from `w2-fixture.sh`, port on 127.0.0.1 only) and runs `w2-browser-test.mjs` in the Playwright image: cleanup and reconciliation dialogs (selection, preview, applying in blocks, cancel, result), date rename, settings, start check with its notice, an emptied root; plus light/dark screenshots at 1280 and 400 px, axe checks and database cross-checks. Not part of CI.

### Quellordner und Druckeradressen (Deutsch)

`KATALOG_SOURCE_ROOTS` nennt die Ordner auf dem Server, aus denen die Oberfläche Quellordner durchsuchen und importieren darf (Unix `:`, Windows `;`; leer = aus). Es müssen bestehende Verzeichnisse sein, getrennt von Papierkorb, Upload und Anmeldedatei. Im Compose-Beispiel am besten schreibgeschützt einbinden (`- /pfad/zu/quellen:/sources:ro`) und `KATALOG_SOURCE_ROOTS=/sources` setzen; die Quelldateien bleiben unverändert, es wird nur in den Katalog kopiert. Siehe `docs/web/source-dirs.md`.

`KATALOG_PRINTER_ALLOW` erlaubt zusätzlich Druckeradressen: kommagetrennte Hosts, IPs oder CIDR-Netze ohne Port; leer = aus. Die Liste ergänzt die Heimnetz-Prüfung und ersetzt sie nicht. Metadaten-Adressen werden immer abgelehnt, ebenso Loopback-Adressen. Ohne diese Einstellung meldet die Druckerverwaltung im Browser, dass der Betreiber zuerst Adressen freigeben muss.

### Browser-Upload und Import (Backend Stufe 5, Deutsch)

Uploads sind standardmäßig aus. `KATALOG_UPLOAD_DIR=/models/uploads` aktiviert das
Backend zusammen mit `KATALOG_WRITE=1`, Anmeldung und einer expliziten
`KATALOG_ALLOWED_HOSTS`-Liste. Die Browser-Oberfläche folgt separat in Teil C.
**Upload legt Dateien dauerhaft auf dem Server ab: nur im Heimnetz oder über VPN
betreiben.** Die Instanzsperre verhindert den gleichzeitigen Start auf derselben DB.
Erlaubt sind `.3mf`, `.stl`, `.obj`, `.stp`, `.step` sowie die Archive `.zip` und
`.7z`; RAR und tar bleiben der Desktop-App vorbehalten (Fehler
`archive_format_unsupported`). Aus einem Archiv werden nur die darin enthaltenen
Modelle übernommen und wie einzelne Uploads importiert (Parsergrenzen,
Duplikaterkennung, Journal); alle anderen Dateien, ausführbare Dateien, Links und
verschachtelte Archive werden übersprungen und im Ergebnis gezählt. Verschlüsselte
Archive (`archive_encrypted`) werden abgelehnt. Das Archiv selbst bleibt als Upload
liegen, wird nicht automatisch gelöscht und lässt sich in der Liste verwerfen (löscht nur das
Archiv, nicht die Modelle). Dateien bleiben im Upload-Ordner, ohne
Zielordner-Auswahl und ohne Kopieren/Verschieben beim Import. STEP-Dateien werden
beim Import ohne Maße übernommen; die 3D-Vorschau und die Maße beim „Neu einlesen“
liefert die STEP-Vorschau des Images (siehe „STEP-Vorschau“ oben).

Den Upload-Ordner vorher anlegen (0755) und gezielt schreibbar einbinden; er muss
unter `KATALOG_ROOTS` liegen. Der Prozess mit `PUID`/`PGID` (Standard 1000:1000)
braucht dort sowie bei DB und Auth-Datei Schreibrechte. Upload-Dateien erhalten
0644. Ein Symlink als Upload-Ordner und Überschneidungen (gleich, Eltern-/Kindpfad)
mit DB-, Auth-, Trash- oder Static-Verzeichnissen verhindern den Start. Andere
Modellfreigaben können lesend bleiben.

`KATALOG_COMPANION_MAX_MB` ist die Größengrenze einer einzelnen Begleitdatei
(Hochladen und Herunterladen) in MiB (Standard 1024, 1–4096); sie gilt nur mit
`KATALOG_FILE_OPS=1`. Begleitdateien werden beim Hochladen direkt in den
`<Modell>_extras`-Ordner gestreamt und nicht im Speicher gesammelt; es läuft
höchstens ein Begleitdatei-Upload gleichzeitig, und der freie Platz muss die
Dateigröße plus 256 MiB Reserve übersteigen.

Begleitdatei-Upload (Deutsch): Das Hochladen einzelner Begleitdateien (Bilder, PDF, TXT, MD)
braucht `KATALOG_WRITE=1` und `KATALOG_FILE_OPS=1`, aber keinen `[upload]`-Bereich. Die Route
`POST /api/companion-upload` nimmt den Inhalt als rohen Body; die Größengrenze nennt der Server
dem Browser, der zu große oder leere Dateien vorher abweist. Ein Reverse Proxy muss den Body
bis zu dieser Grenze durchlassen und darf ihn nicht puffern, sonst scheitert der Upload mit 413
oder bricht ab (nginx: `client_max_body_size` mindestens `KATALOG_COMPANION_MAX_MB`, dazu
`proxy_request_buffering off`; Caddy: `request_body { max_size … }`; Traefik: Buffering-
Middleware mit `maxRequestBodyBytes` oder ohne Puffer). Ein Upload bricht nach 60 Sekunden ohne
Fortschritt ab und räumt seine Teildatei weg; die Sitzung wird vor dem Abschluss noch einmal
geprüft. Einzelne Begleitdateien lassen sich im Web entfernen und wiederherstellen: „Entfernen“
verschiebt die Datei in den Web-Papierkorb (`.katalog-papierkorb/companion-<id>-…`), „Wiederherstellen“
holt sie zurück (ein belegter Name bekommt eine Nummer). Beides ist journalisiert; hartes
Löschen gibt es nur über „Papierkorb leeren“, das auch einzeln entfernte Begleitdateien
mitnimmt (mit Passwort wie bei Modellen, in Blöcken). Der Abgleich am Desktop
meldet im Bericht „Begleitdateien ohne Datei“ auch Web-Begleitdateien, deren Datei außerhalb
fehlt.

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

`KATALOG_ARCHIVE_MAX_MB` (Standard 1024, 1–4096) begrenzt, wie groß ein Archiv
entpackt höchstens sein darf; dazu gilt die Grenze von 10.000 Einträgen. Das Archiv
wird vor dem Entpacken geprüft (Größen laut Kopfdaten, Eintragszahl, Verschlüsselung);
Plattenreserve und das Kontingent offener Uploads werden vorab auf die enthaltenen
Modelle angerechnet. ZIP wird Eintrag für Eintrag gelesen, 7z als Datenstrom; ein 7z
mit mehr als 256 MiB Wörterbuch- bzw. PPMd-Speicher je Dekoder wird abgelehnt. Es
wird nur ein Archiv gleichzeitig entpackt. Dabei entsteht ein versteckter Ordner
`.archive-staging-…` im Upload-Ordner; Reste eines Absturzes entfernt der nächste Start.
Ein Archiv darf höchstens 30 Minuten entpackt werden (`archive_timeout`).
Die Modelle erscheinen anschließend als normale Uploads (höchstens 200 offene Uploads)
und liegen flach im Upload-Ordner, die Ordnerstruktur des Archivs entfällt; bei gleichem
Namen folgt der Zusatz „ (2)“.

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

Fortschrittsanzeige (`/api/events`) funktioniert auch über unverschlüsseltes HTTP mit
IP-Adresse (z. B. `http://192.168.1.50:8080` oder Tailscale). Browser senden
`Sec-Fetch-Site` nur an HTTPS bzw. localhost; fehlt der Header, entscheidet die
Origin-Prüfung (kein Origin oder passender Origin: erlaubt, fremder Origin: 403).
Ist `Sec-Fetch-Site` vorhanden, gilt nur `same-origin`/`none`.

### Source folders and printer addresses (English)

`KATALOG_SOURCE_ROOTS` lists the server folders from which the UI may browse and import source folders (Unix `:`, Windows `;`; empty = off). They must be existing directories, separate from trash, upload and the auth file. In the Compose example, mount them read-only (`- /path/to/sources:/sources:ro`) and set `KATALOG_SOURCE_ROOTS=/sources`; source files stay untouched, they are only copied into the catalog. See `docs/web/source-dirs.md`.

`KATALOG_PRINTER_ALLOW` additionally permits printer addresses: comma-separated hosts, IPs or CIDR networks without ports; empty = off. The list adds to the home-network check and does not replace it. Metadata addresses are always refused, as are loopback addresses. Without this setting the printer management in the browser reports that the operator has to release addresses first.

### Browser upload and import (stage 5 backend, English)

Uploads are disabled by default. Set `KATALOG_UPLOAD_DIR=/models/uploads` together
with `KATALOG_WRITE=1`, authentication and an explicit `KATALOG_ALLOWED_HOSTS` list.
The browser interface follows separately in part C. **Uploads store files permanently
on the server: use a home network or VPN only.** The instance lock prevents desktop and web
from starting concurrently on the same database. Supported files are `.3mf`, `.stl`,
`.obj`, `.stp`, `.step` plus `.zip` and `.7z` archives; RAR and tar require the
desktop app (error `archive_format_unsupported`). Only the models inside an archive
are taken over and imported like single uploads (parser limits, duplicate detection,
journal); every other file, executables, links and nested archives are skipped and
counted in the result. Encrypted archives (`archive_encrypted`) are refused. The
archive itself stays as an upload, is never deleted automatically and can be discarded
from the list (removes only the archive, not the models). Files remain in
the upload directory: import neither copies nor moves them and has no target-folder
selection. STEP files are imported without dimensions; the 3D preview and the
dimensions on "Rescan" come from the image's STEP preview (see "STEP preview" above).

Create the upload directory first (0755) and mount it writable. It must be under
`KATALOG_ROOTS`; the process UID/GID (`PUID`/`PGID`, default 1000:1000) needs write
access there and to DB/auth storage. Uploaded files use 0644. Startup rejects an
upload directory symlink or any ancestor/descendant overlap with DB, auth, trash
or static directories. Other model shares can remain read-only.

`KATALOG_COMPANION_MAX_MB` sets the size limit of one companion file (upload and
download) in MiB (default 1024, range 1–4096); it applies with `KATALOG_FILE_OPS=1`
only. Companion files are streamed straight into the `<model>_extras` folder and are
not collected in memory; at most one companion upload runs at a time, and free space
must exceed the file size plus a 256 MiB reserve.

Companion upload (English): uploading single companion files (images, PDF, TXT, MD) needs
`KATALOG_WRITE=1` and `KATALOG_FILE_OPS=1` but no `[upload]` section. `POST /api/companion-upload`
takes the content as a raw body; the server tells the browser the size limit, and the browser
rejects oversized or empty files before sending. A reverse proxy must let the body through up to
that limit and must not buffer it, otherwise the upload fails with 413 or breaks off (nginx:
`client_max_body_size` at least `KATALOG_COMPANION_MAX_MB`, plus `proxy_request_buffering off`;
Caddy: `request_body { max_size … }`; Traefik: buffering middleware with `maxRequestBodyBytes`
or none). An upload stops after 60 seconds without progress and removes its partial file; the
session is checked once more before the file is finalised. Single companion files can be
removed and restored on the web: "Remove" moves the file into the web trash
(`.katalog-papierkorb/companion-<id>-…`), "Restore" brings it back (a taken name gets a
number). Both are journalled; permanent deletion only happens through "Empty trash", which
also takes individually removed companion files along (with the password, in blocks, as for
models). The desktop's disk sync report "Companion files without a file" also
lists web companions whose file went missing outside.

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

`KATALOG_ARCHIVE_MAX_MB` (default 1024, range 1–4096) caps what one archive may
unpack to, on top of the 10,000-entry limit. An archive is checked before unpacking
(header sizes, entry count, encryption); free-space reserve and the open-upload quota
are charged up front for the models it contains. ZIP is read entry by entry and 7z
as a stream; a 7z whose decoder dictionary or PPMd memory exceeds 256 MiB is refused.
Only one archive unpacks at a time, into a hidden `.archive-staging-…` folder inside
the upload directory; the next start removes leftovers of a crash. Unpacking one archive may take 30 minutes
at most (`archive_timeout`). The models then
appear as ordinary uploads (at most 200 open uploads) and sit flat in the upload
directory, the archive's folder structure is dropped; equal names get a " (2)" suffix.

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

The progress view (`/api/events`) also works over plain HTTP with an IP address
(for example `http://192.168.1.50:8080` or Tailscale). Browsers send
`Sec-Fetch-Site` only to HTTPS or localhost; without it the Origin check decides
(no Origin or a matching Origin: allowed, foreign Origin: 403). When
`Sec-Fetch-Site` is present, only `same-origin`/`none` is accepted.

SSE-Neuverbindungen erhalten einen Snapshot der aktiven Jobs, der letzten 32
Job-Ergebnisse und der offenen Uploads (höchstens 500). Insgesamt bleiben die letzten
100 abgeschlossenen Jobs über `get_import_result { jobId }` abrufbar. Die Puffer enthalten höchstens
32 Ereignisse je Verbindung; erlaubt sind vier Verbindungen je Sitzung und 32 insgesamt.
Web-Import-Batches enthalten höchstens 20 Dateien und werden auch ab 16 MiB
gesammelten Metadaten abgeschlossen. Das Journal wird einmal je Batch geschrieben.

SSE reconnections receive active jobs, the latest 32 job results, and pending uploads
(up to 500). The latest 100 completed jobs remain available through
`get_import_result { jobId }`.
Each connection buffers at most 32 events; limits are four connections per session
and 32 overall. Web import batches contain at most 20 files and also flush once
retained metadata reaches 16 MiB. The journal is written once per batch.

The journal retains at most 1,000 imported entries; pruning never deletes model
files or catalog records. `GET /api/uploads` lists only open entries (at most 500;
normal admission remains 200). Startup removes only regular `.journal-*.part`
files older than one hour from auth storage. Untracked upload files and hardlinked
`.upload-*.part` files are kept; one startup warning reports their count without
paths. Administrators should inspect these leftovers manually before removing
anything. After a journal failure, `restart_required` requires repair of storage
and a server restart; it has no `Retry-After`. 3MF containers are limited to 10,000
ZIP entries in both web and desktop import, checked before the ZIP index is built.

Das Journal behält höchstens 1.000 importierte Einträge; beim Kürzen werden weder
Modelldateien noch Katalogeinträge gelöscht. `GET /api/uploads` zeigt nur offene
Einträge (höchstens 500; normale Annahmegrenze weiterhin 200). Beim Start werden
nur reguläre `.journal-*.part`-Dateien im Auth-Ordner entfernt, die älter als eine
Stunde sind. Upload-Dateien ohne Journal-Eintrag und hartverlinkte
`.upload-*.part` bleiben erhalten; ein einmaliger Starthinweis nennt ihre Anzahl
ohne Pfade. Administratoren sollten solche Reste vor dem Löschen manuell prüfen.
Nach einem Journalfehler verlangt `restart_required` eine Behebung des
Speicherproblems und einen Serverneustart; es gibt kein `Retry-After`.
3MF-Container sind im Web und auf dem Desktop auf 10.000 ZIP-Einträge begrenzt;
die Prüfung erfolgt vor dem Aufbau des ZIP-Index.

### Optional web file management (stage 7 server)

**Warning:** `KATALOG_FILE_OPS=1` lets the web server rename, move and delete
files. It defaults to off and requires `KATALOG_WRITE=1`, authentication and an
explicit `KATALOG_ALLOWED_HOSTS`. Browser controls follow separately. Use a home
network/VPN and maintain external backups. The instance lock prevents desktop and web from starting against the
same database concurrently. Opening that DB in desktop later also matters:
**Desktop startup purges trash entries after 7 days**
(`purge_expired_trash_on_startup`); do not mix desktop and web trash.

File roots and `/models` must be writable (volumes must **not use `:ro`**).
Match Docker `PUID`/`PGID` and filesystem ownership/permissions. Uploaded files
belong to the server user; rename preserves inode, permissions and timestamps.
Each root has its private `<root>/.katalog-papierkorb/` directory (0700). The web
server **never empties it automatically**. Startup rejects overlapping roots,
protected database/auth/static locations, unwritable volumes and filesystems
without `renameat2(RENAME_NOREPLACE)`; some NAS/SMB/NFS systems are unsuitable.
Moves stay inside one root and filesystem, with no copy fallback. Moving folders
also moves uncatalogued contents. Restore never overwrites or creates missing
parents. Existing `list_trash`/`list_folders` absolute path fields remain an API
privacy exception.

Destructive commands share instance-wide budgets: 200 files per 10 minutes and
`KATALOG_FILE_OPS_DAILY_LIMIT=2000` per rolling 24 hours, separately for trash and
permanent removals (including DB-only removals). Reserved attempts conservatively
count even if they subsequently fail. Empty-trash starts are limited to once per
minute; snapshot blocks are at least 2 seconds apart. Calls stop starting files
after 20 seconds. Permanent deletion and empty-trash start recheck the password.
Logout/password changes stop before the next file; already started work finishes.
Counting rules: removing a folder from the catalog counts at most one full
window (200 per 10 minutes, the daily limit) however many models it holds, so
large folders stay removable; a single batch above a window limit is rejected
with 422 `limit_exceeded` (no `Retry-After`). Empty-trash blocks count only against
the daily limit, not against the 10-minute pot of single permanent deletions.
`remove_files_from_catalog` also works without `KATALOG_FILE_OPS`: it obeys the
same limits (200 per 10 minutes, daily limit) and triggers backups, so it can fail
with 429 or with 503 `backup_failed` (too little space: DB size + 64 MiB;
`<database directory>/backups` not mode 0700 or not owned by the server user;
foreign `web-fileops-*.db` files there). Backups are stored in
`<database directory>/backups/`.

SQLite backups in `<database directory>/backups/` precede destructive changes:
one per **UTC calendar day**, kept for 7 days; separate event backups before
empty-trash and catalog removal, keeping 5 with a 10-minute minimum interval.
No affected catalog entries means no backup; removing an empty folder still
creates a backup. Free space must cover the DB plus a 64 MiB
reserve; backups have mode 0600. Backup failure rejects the change
(`backup_failed`). These copies do not replace external backups.

Ambiguous journal states block file management (`restart_required`) while reads
and metadata editing remain available. An administrator must manually investigate
the **opId** and local `web-fsops.json`, then restart once with
`KATALOG_FILE_OPS_ACK=<opId[,<opId>…]>` and remove the variable afterward. ACK is
not automatic repair. Without acknowledgement the block survives restarts.
Read-only startup refuses an outstanding journal.

Legacy upload journals may contain imported or importing entries without `fileId`.
With uploads disabled, startup refuses these entries because their catalog identity
cannot be reconstructed safely. Enable the original upload configuration, including
`KATALOG_UPLOAD_DIR`, once to complete recovery.

### Catalog check after a restored backup (English)

Restoring a backup drops the content hashes of the files. After the restart the server
fills them in again in the background (it reads each model file once). Until that run
has finished, the browser-upload import, the source-folder import and the search for new
files are refused with HTTP 409, and the web interface shows "The catalog is still being
checked after the restore. Please wait a moment."; repeat the action afterwards.
Without this wait every known file would count as new and be copied again. Everything
else (viewing, searching, editing, file management) stays available meanwhile; renaming
or moving a file during the run is fine, its hash is stored for the new path. The log
reports start and end ("Prüfsummen: …"). A server without write mode (`KATALOG_WRITE`
not set) does not run the check and has no imports that it would have to block.


### Disk sync and cleanup in the web interface (English)

The web app can compare the catalog with the disk and tidy it up, as the desktop
app does. Requirements: `KATALOG_FILE_OPS=1` and `KATALOG_WRITE=1` (see above)
and a signed-in session. Without write mode the sync report stays display-only
and says that `KATALOG_FILE_OPS=1` is needed.

- **Disk sync (disk to catalog).** The report lists new files on the disk, models
  and companion files whose file is gone, and entries it cannot judge. You tick
  what to apply, read a preview and confirm. New files are imported in place (the
  file stays where it is); the server re-checks every row against the disk right
  before it acts and skips rows that changed meanwhile (the result lists them with
  a reason).
- **Removing entries.** Missing models are only ever removed from the catalog
  (they go to the catalog trash and can be restored there); no file is moved or
  deleted by the sync. Removing needs an explicit confirmation tick and counts
  against the daily limit of the file management.
- **"Unclear" list.** Entries below a link, an unreachable or empty-looking root
  or after an I/O error are not treated as missing. They are shown for
  information only and can neither be selected nor removed.
- **Cleanup ("Check catalog for problems").** Lists models whose file is missing
  (a fully deleted folder is shown as one line "folder missing, N models") and
  duplicate files. What you remove goes to the trash. The same limits as the
  desktop apply: at most 500 missing models, 200 duplicate groups, 20 s; above
  that the list is marked as truncated.
- **Date prefix.** "Rename with date" works in the web interface (preview, then
  apply; up to 200 files per request; companion files are renamed along).
- **Progress and cancel.** The work runs in blocks of at most 10 rows. "Cancel"
  stops after the running block; what was already applied stays. A preview expires
  after 60 minutes. While a backup is being restored the run stops and says so.
- **Limits.** Report: 500 rows per section, 200,000 files, 20 s.
- **Check at start (opt-in, off by default).** Settings has "Compare catalog
  folders after start". Needs `KATALOG_FILE_OPS=1` and write mode. The server then
  checks once per start in the background, about 5 s after the start, and only
  reports the numbers. Only with the second switch ("Add new
  files automatically") it adds at most 200 new files per start, after the day copy of the
  catalog; the rest stays "still open". It never removes entries. If a root is
  unreachable or empty nothing is applied. The result is kept in memory only
  until the next restart. Without the setting nothing is changed.

### Katalogprüfung nach dem Einspielen einer Sicherung (Deutsch)

Beim Einspielen einer Sicherung gehen die Prüfsummen der Dateien verloren. Nach dem
Neustart ergänzt der Server sie im Hintergrund (er liest dafür jede Modelldatei einmal).
Solange dieser Lauf nicht fertig ist, lehnen der Import aus dem Browser-Upload, der Import
aus einem Quellordner und die Suche nach neuen Dateien mit HTTP 409 ab; die Oberfläche
zeigt „Der Katalog wird nach dem Einspielen noch geprüft, bitte kurz warten.“ und der
Vorgang lässt sich danach wiederholen. Ohne diese Wartezeit würde jede bereits bekannte
Datei als neu gelten und noch einmal kopiert. Alles andere (Ansehen, Suchen, Ändern,
Dateiverwaltung) bleibt in dieser Zeit verfügbar; Umbenennen oder Verschieben während
des Laufs ist erlaubt, die Prüfsumme wird zum neuen Pfad gespeichert. Das Log nennt
Beginn und Ende („Prüfsummen: …“). Ein Server ohne Schreibmodus (`KATALOG_WRITE` nicht
gesetzt) führt den Lauf nicht aus und kennt auch keine Importe, die er sperren müsste.


### Abgleich und Bereinigung im Web (Deutsch)

Die Web-App kann den Katalog mit der Festplatte abgleichen und aufräumen, wie die
Desktop-App. Voraussetzungen: `KATALOG_FILE_OPS=1` und `KATALOG_WRITE=1` (siehe
oben) und eine angemeldete Sitzung. Ohne Schreibmodus bleibt der Abgleichsbericht
eine reine Anzeige und weist auf `KATALOG_FILE_OPS=1` hin.

- **Abgleich (Platte zum Katalog).** Der Bericht listet neue Dateien auf der
  Platte, Modelle und Begleitdateien, deren Datei fehlt, und Einträge, die er
  nicht beurteilen kann. Du hakst an, was übernommen werden soll, liest die
  Vorschau und bestätigst. Neue Dateien werden an Ort und Stelle aufgenommen (die
  Datei bleibt liegen); der Server prüft vor jeder Zeile den Zustand auf der
  Platte neu und überspringt Zeilen, die sich inzwischen geändert haben (das
  Ergebnis nennt sie mit Grund).
- **Einträge entfernen.** Fehlende Modelle werden nur aus dem Katalog entfernt (sie
  landen im Papierkorb des Katalogs und lassen sich dort zurückholen); der
  Abgleich verschiebt oder löscht keine Datei. Entfernen braucht einen
  ausdrücklichen Bestätigungshaken und zählt gegen das Tageslimit der
  Dateiverwaltung.
- **Unklar-Liste.** Einträge unter einem Link, bei nicht erreichbarer oder leer
  wirkender Wurzel oder nach einem Lesefehler gelten nicht als fehlend. Sie werden
  nur angezeigt und lassen sich weder auswählen noch entfernen.
- **Bereinigung („Katalog auf Probleme prüfen“).** Zeigt Modelle, deren Datei
  fehlt (ein ganz gelöschter Ordner erscheint als eine Zeile „Ordner fehlt, N
  Modelle“), und doppelte Dateien. Was du entfernst, kommt in den Papierkorb.
  Grenzen wie am Desktop: höchstens 500 fehlende Modelle, 200 Duplikatgruppen,
  20 s; darüber wird die Liste als gekürzt markiert.
- **Datumspräfix.** „Mit Datum umbenennen“ funktioniert im Web (Vorschau, dann
  anwenden; bis zu 200 Dateien je Anfrage; Begleitdateien ziehen mit).
- **Fortschritt und Abbrechen.** Gearbeitet wird in Blöcken mit höchstens 10
  Zeilen. „Abbrechen“ stoppt nach dem laufenden Block; schon Übernommenes bleibt.
  Eine Vorschau verfällt nach 60 Minuten. Wird gerade eine Sicherung
  eingespielt, bricht der Lauf ab und sagt es.
- **Grenzen.** Bericht: 500 Zeilen je Abschnitt, 200 000 Dateien, 20 s.
- **Abgleich beim Start (Opt-in, standardmäßig aus).** In den Einstellungen gibt
  es „Katalogordner nach dem Start abgleichen“. Braucht `KATALOG_FILE_OPS=1` und
  Schreibmodus. Der Server prüft dann einmal je Start im Hintergrund, etwa 5 s
  nach dem Start, und meldet nur die Zahlen. Erst mit dem zweiten Schalter („Neue
  Dateien automatisch aufnehmen“) nimmt er höchstens 200 neue Dateien je Start
  auf, nach der Tageskopie des Katalogs; der Rest bleibt „weiter offen“. Einträge
  entfernt er nie. Ist eine Wurzel nicht erreichbar oder leer, wird nichts
  angewendet. Das Ergebnis liegt nur im Speicher, bis zum nächsten Neustart. Ohne
  die Einstellung ändert sich nichts.

### Optionale Dateiverwaltung im Web (Stufe 7, Server)

**Warnung:** `KATALOG_FILE_OPS=1` erlaubt dem Webserver, Dateien umzubenennen,
zu verschieben und zu löschen. Standard ist aus. Erforderlich sind
`KATALOG_WRITE=1`, Anmeldung und explizite `KATALOG_ALLOWED_HOSTS`. Die
Browseroberfläche folgt separat. Nur im Heimnetz/VPN betreiben; regelmäßige
externe Sicherungen anlegen. Die Instanzsperre verhindert den gleichzeitigen Start von Desktop und Webserver
auf derselben Datenbank. Auch späteres Öffnen derselben DB im Desktop ist relevant:
**Die Desktop-App entfernt Papierkorbeinträge beim Start nach 7 Tagen**
(`purge_expired_trash_on_startup`); Web- und Desktop-Papierkorb nicht mischen.

Dateiwurzeln und `/models` müssen schreibbar sein (Volumes **nicht `:ro`**).
Der Serverbenutzer braucht passende Eigentümer-/Verzeichnisrechte; Docker
`PUID`/`PGID` passend setzen. Hochgeladene Dateien gehören dem Serverbenutzer.
Rename bewahrt Inode, Rechte und Zeitstempel. Jede Wurzel bekommt einen privaten
Papierkorb `<Wurzel>/.katalog-papierkorb/` (0700), der im Web **nie automatisch
geleert** wird. Startprüfungen verweigern überlappende Wurzeln, geschützte
DB-/Auth-/Static-Verzeichnisse und unbeschreibbare oder inkompatible Dateisysteme.
`renameat2(RENAME_NOREPLACE)` muss unterstützt sein; manche NAS/SMB/NFS-Systeme
sind ungeeignet. Verschieben geht nur innerhalb derselben Wurzel und desselben
Dateisystems, ohne Kopierausweg. Beim Ordnerverschieben wandern auch nicht
katalogisierte Inhalte mit. Restore überschreibt nichts und legt fehlende
Elternordner nicht an. `list_trash`/`list_folders` enthalten weiterhin absolute
Pfadfelder als bestehende API-Ausnahme.

Destruktive Befehle teilen instanzweite Grenzen: jeweils 200 Dateien je 10 Minuten
und `KATALOG_FILE_OPS_DAILY_LIMIT=2000` je rollenden 24 Stunden, getrennt für
Papierkorb und endgültige Entfernung (einschließlich DB-only-Entfernung).
Reservierte Versuche zählen konservativ auch bei späterem Fehler.
Papierkorbleerung startet höchstens einmal je Minute; Snapshot-Blöcke folgen
frühestens alle 2 Sekunden. Nach 20 Sekunden werden keine weiteren Dateien eines
Aufrufs begonnen. Endgültiges Löschen und Leerungsstart prüfen das Passwort erneut.
Logout/Passwortwechsel stoppt vor der nächsten Datei; gestartete Arbeit wird beendet.
Zählregeln: Das Entfernen eines Ordners aus dem Katalog zählt höchstens ein volles
Fenster (200 je 10 Minuten bzw. die Tagesgrenze), egal wie viele Modelle er enthält;
große Ordner bleiben entfernbar. Ein einzelner Stapel über der Fenstergrenze wird mit
422 `limit_exceeded` (ohne `Retry-After`) abgelehnt. Blöcke der Papierkorbleerung
zählen nur gegen die Tagesgrenze, nicht gegen den 10-Minuten-Topf des einzelnen
endgültigen Löschens. `remove_files_from_catalog` wirkt auch ohne `KATALOG_FILE_OPS`:
Es unterliegt denselben Grenzen (200 je 10 Minuten, Tagesgrenze) und löst Sicherungen
aus, kann also mit 429 oder mit 503 `backup_failed` scheitern (zu wenig Platz:
Datenbankgröße + 64 MiB; `<db-Ordner>/backups` nicht 0700 bzw. nicht im Besitz des
Serverbenutzers; fremde `web-fileops-*.db` dort). Die Sicherungen liegen in
`<db-Ordner>/backups/`.

Vor destruktiven Änderungen liegen SQLite-Sicherungen in `<db-Ordner>/backups/`:
eine je **UTC-Kalendertag**, 7 Tage behalten; zusätzliche Ereignissicherungen vor
Papierkorbleerung und Katalogentfernung, 5 behalten, mindestens 10 Minuten Abstand.
Ohne betroffene Katalogeinträge wird keine Sicherung erzeugt; auch das Entfernen
eines leeren Ordners wird gesichert. Freier Platz muss für die
DB plus 64 MiB Reserve reichen; Dateien haben Modus 0600. Sicherungsfehler
verhindern die Änderung (`backup_failed`). Dies ersetzt keine externen Backups.

Nach einem unklaren Journalzustand bleiben Dateioperationen blockiert
(`restart_required`), Lesen und Metadatenbearbeitung bleiben möglich.
Admin prüft die betroffene Operation anhand der **opId** und des lokalen Journals
`web-fsops.json` manuell. Erst danach einmalig mit
`KATALOG_FILE_OPS_ACK=<opId[,<opId>…]>` neu starten und die Variable wieder entfernen.
ACK ist keine automatische Reparatur; ohne bestätigte Klärung bleibt die Sperre
über Neustarts erhalten. Reiner Lesemodus verweigert den Start bei offenem Journal.


Alte Upload-Journale können importierte oder gerade importierende Einträge ohne
`fileId` enthalten. Sind Uploads dabei deaktiviert, verweigert der Server den Start,
weil die Zuordnung nicht sicher rekonstruiert werden kann. Für die Recovery einmalig
die ursprüngliche Upload-Konfiguration einschließlich `KATALOG_UPLOAD_DIR` aktivieren.

### Slicer-Links: „In Slicer öffnen“ (Deutsch)

`KATALOG_SLICER_LINKS=1` schaltet ein Menü „In Slicer öffnen“ ein (Standard: aus; jeder andere Wert außer `0` oder `1` verhindert den Start). Der Browser kann kein Programm starten, deshalb erzeugt der Server auf Wunsch einen **signierten Download-Link** (`/dl/<id>/<Ablauf>.<Revision>.<Signatur>/<Dateiname>`), und die Seite übergibt ihn über das Link-Schema des Slicers (OrcaSlicer, Bambu Studio nur für 3MF, Creality Print, ElegooSlicer, Cura, Lychee, SuperSlicer). Das Feature braucht keinen Schreibmodus.

- **Der Link ist ein Zugangsschlüssel.** Wer ihn kennt, kann die eine Datei bis zum Ablauf ohne Anmeldung herunterladen; er taucht im Verlauf des Slicers und in Proxy-Protokollen auf. Deshalb gilt er nur kurz (`KATALOG_SLICER_LINK_TTL_SECONDS`, Standard 3600, 60–86400), wird nie verlängert und ist an die Datei und ihren Zustand gebunden: Ändert oder ersetzt jemand die Datei, wird der Link ungültig. Der Signaturschlüssel liegt nur im Arbeitsspeicher; **ein Neustart macht alle Links ungültig.** Ein Passwortwechsel erneuert den Schlüssel ebenfalls: alle bis dahin ausgestellten Links sind danach ungültig. Abmelden ändert nichts an bestehenden Links.
- **Grenzen der Bindung an die Datei:** Wird dieselbe Datei wieder an ihren Platz gelegt (Wiederherstellen aus dem Papierkorb per Umbenennen) oder an Ort und Stelle mit gleicher Größe und gleichem Änderungszeitpunkt ersetzt, bleiben Inode, Größe, Zeit und Hash gleich, und ein früher ausgestellter Link gilt bis zu seinem Ablauf wieder. Der Ablauf nutzt die Systemuhr des Servers: wird sie zurückgestellt, verlängern sich die Links entsprechend.
- Abgerufen werden kann nur über `GET`/`HEAD` (mit Bereichsanfragen) und nur für Hosts aus `KATALOG_ALLOWED_HOSTS`. Papierkorb, fehlende Dateien, Symlinks und Pfade außerhalb von `KATALOG_ROOTS` werden nicht ausgeliefert. Ungültige Anfragen werden je Quelladresse begrenzt (20 pro Minute, danach 429); gültige Links derselben Adresse werden dadurch nicht gesperrt. Höchstens 4 solche Downloads gleichzeitig; ein einzelner Download endet nach 2 Stunden insgesamt oder 120 Sekunden ohne Lesefortschritt; 60 neue Links pro Minute und Sitzung. Hinter einem Proxy `KATALOG_TRUST_PROXY=1` nur mit korrektem `X-Forwarded-For` setzen, sonst teilen sich alle Clients ein Limit.
- Das Betriebsprotokoll (stderr) enthält je Abruf eine Zeile `slicer_dl time=… file_id=… status=… size=…` – ohne Link, Signatur oder Pfad.
- **Voraussetzung:** Der Slicer läuft auf dem PC des Benutzers und muss den Server unter derselben Adresse erreichen wie der Browser. Unter Linux muss das URL-Schema des Slicers registriert sein.
  - Unter macOS versteht Bambu Studio nur `bambustudioopen://`; die Seite verwendet dort automatisch dieses Schema.
  - Bambu Studio fragt bei Adressen außerhalb seiner Vertrauensliste nach und braucht zum Import das **Network-Plug-in**.
  - Hinter einem Reverse Proxy mit HTTPS muss `KATALOG_TRUST_PROXY=1` mit `KATALOG_TRUSTED_PROXIES=<Proxy-IP/CIDR>` gesetzt sein und der Proxy `X-Forwarded-For` sowie `X-Forwarded-Proto` senden; sonst scheitert bereits die Anmeldung mit „Origin nicht zugelassen“.
  - Bambu Studio lehnt unbekannte (selbstsignierte) Zertifikate ab, OrcaSlicer nicht. Ein Zertifikat einer im System vertrauten CA funktioniert.
- **Getestet mit Bambu Studio 2.9.1.70 und OrcaSlicer 2.4.2/2.5.0 unter Windows 11, macOS 15 und Ubuntu 24.04 (Firefox, Chrome/Chromium, Edge, Safari).**

### Slicer links: "Open in slicer" (English)

`KATALOG_SLICER_LINKS=1` enables an "Open in slicer" menu (default: off; any value other than `0` or `1` stops the server from starting). A browser cannot start a program, so on request the server creates a **signed download link** (`/dl/<id>/<expiry>.<revision>.<signature>/<file name>`), and the page hands it to the slicer through its link scheme (OrcaSlicer, Bambu Studio for 3MF only, Creality Print, ElegooSlicer, Cura, Lychee, SuperSlicer). The feature does not need write mode.

- **The link is an access key.** Whoever knows it can download that one file without signing in until it expires; it shows up in the slicer's history and in proxy logs. It is therefore short-lived (`KATALOG_SLICER_LINK_TTL_SECONDS`, default 3600, 60-86400), never extended, and bound to the file and its state: if the file is changed or replaced, the link stops working. The signing key lives in memory only; **a restart invalidates every link.** A password change also renews the key: every link issued before it is invalid afterwards. Logging out does not affect existing links.
- **Limits of the binding to the file:** if the same file is put back (restored from the trash by renaming) or replaced in place with the same size and modification time, inode, size, time and hash stay the same, and an earlier link works again until it expires. Expiry uses the server's system clock: setting it back extends the links accordingly.
- Links are served for `GET`/`HEAD` only (range requests included) and only for hosts in `KATALOG_ALLOWED_HOSTS`. Trash entries, missing files, symlinks and paths outside `KATALOG_ROOTS` are not served. Invalid requests are limited per source address (20 per minute, then 429); valid links from the same address are not blocked by this. At most 4 such downloads run at once; a single download ends after 2 hours in total or 120 seconds without read progress; 60 new links per minute and session. Behind a proxy set `KATALOG_TRUST_PROXY=1` only with a correct `X-Forwarded-For`, otherwise all clients share one limit.
- The operation log (stderr) has one line per request, `slicer_dl time=... file_id=... status=... size=...`, without link, signature or path.
- **Requirement:** the slicer runs on the user's PC and must reach the server at the same address as the browser. On Linux the slicer's URL scheme must be registered.
  - On macOS, Bambu Studio only understands `bambustudioopen://`; the page automatically uses this scheme there.
  - Bambu Studio asks for confirmation for addresses outside its trusted list and needs the **Network plug-in** to import the file.
  - Behind an HTTPS reverse proxy, set `KATALOG_TRUST_PROXY=1` with `KATALOG_TRUSTED_PROXIES=<proxy IP/CIDR>` and have the proxy send `X-Forwarded-For` and `X-Forwarded-Proto`; otherwise even signing in fails with “Origin not allowed”.
  - Bambu Studio rejects unknown (self-signed) certificates; OrcaSlicer does not. A certificate from a CA trusted by the system works.
- **Tested with Bambu Studio 2.9.1.70 and OrcaSlicer 2.4.2/2.5.0 on Windows 11, macOS 15 and Ubuntu 24.04 (Firefox, Chrome/Chromium, Edge, Safari).**

### Catalog instance lock (English)

Desktop and web now take an exclusive operating-system lock on `<catalog.db>.lock`
before opening the catalog, including web read-only mode. A second start fails with
`catalogInUse` (the server exits with a nonzero status). Close the other instance
before switching between desktop and web. The catalog directory must be writable
so the sidecar can be created and updated, even for a read-only web catalog.

The sidecar contains the owner (`desktop` or `server`), PID, hostname and start time
for display only. It remains on disk after exit or a crash; the operating system
releases the lock automatically. Do not delete or replace it while an instance is
running. Catalog backup export excludes it, and restoring a catalog leaves it intact.
Older app versions without this lock cannot participate in the coordination.

SMB/NFS mounts remain allowed, but operating-system file locks are not reliable on
every network filesystem or mount configuration. If necessary, set `KATALOG_LOCK=off`
in the environment of each affected desktop/server process (Docker Compose:
`KATALOG_LOCK: "off"` under `environment`). This disables the lock and logs a warning;
you must then ensure that only one instance uses the catalog. Unset the variable to
restore the default locking behavior. No automatic fallback disables the lock.

### Katalog-Instanzsperre (Deutsch)

Desktop und Web nehmen vor dem Öffnen des Katalogs eine exklusive Betriebssystem-
Sperre auf `<catalog.db>.lock`, auch im Web-Lesemodus. Ein zweiter Start scheitert mit
`catalogInUse` (der Server endet mit einem Exit-Code ungleich null). Beende die andere
Instanz vor dem Wechsel zwischen Desktop und Web. Das Katalogverzeichnis muss auch
im Web-Lesemodus zum Anlegen und Aktualisieren der Sperrdatei beschreibbar sein.

Die Datei enthält Eigentümer (`desktop` oder `server`), PID, Hostname und Startzeit
nur zur Anzeige. Sie bleibt nach dem Beenden oder einem Absturz bestehen; das
Betriebssystem gibt die Sperre automatisch frei. Lösche oder ersetze sie nicht bei
laufender Instanz. Katalog-Sicherungen exportieren sie nicht, und beim Einspielen
einer Sicherung bleibt sie erhalten. Ältere App-Versionen ohne diese Sperre können
nicht an der Koordination teilnehmen.

SMB/NFS bleiben erlaubt, aber Betriebssystem-Dateisperren funktionieren nicht bei
jedem Netzwerkdateisystem und jeder Mount-Konfiguration zuverlässig. Falls nötig,
setze `KATALOG_LOCK=off` in der Umgebung jedes betroffenen Desktop-/Serverprozesses
(Docker Compose: `KATALOG_LOCK: "off"` unter `environment`). Das schaltet die Sperre
ab und protokolliert eine Warnung. Du musst dann selbst sicherstellen, dass nur eine
Instanz den Katalog nutzt. Entferne die Variable, um die Standardsperre wieder zu
aktivieren. Fehler schalten die Sperre nicht automatisch ab.
