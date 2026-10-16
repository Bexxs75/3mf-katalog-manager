# docker-web: Web-Version des 3MF Katalog Manager (Entwurf, nicht öffentlich)

**Deutsch.** Kleines Image (ca. 47 MB) mit `katalog-server` und der Web-Oberfläche. Es zeigt einen **vorhandenen** Katalog **standardmäßig nur lesend** im Browser; kein gestreamter Desktop. Gleiche Ordner wie das Desktop-Image: `./config` (Programmdaten, Heimatordner der App) und `./models`.

```
cd docker-web
mkdir -p config models          # Katalog: config/.local/share/com.thebexxs.mfkatalogmanager/catalog.db
docker compose up -d --build
docker compose logs katalog-web # beim ersten Start steht dort einmal das Zufallspasswort
```
- Erreichbar unter `http://127.0.0.1:8080`. Für andere Geräte im Heimnetz `KATALOG_BIND_IP` auf die Adresse des Servers setzen und `KATALOG_ALLOWED_HOSTS` eintragen. Von außen nur per VPN oder Reverse Proxy mit HTTPS.
- Der Katalog muss zuvor von der Desktop-App (oder dem Desktop-Image) in der **gleichen Version** angelegt oder migriert sein; der Webserver migriert nie.
- Passwort: erster Start aus `KATALOG_PASSWORD(_FILE)` (z. B. `/config/.kasmvnc-password` des Desktop-Images) oder zufällig; danach als Argon2-Hash in `config/web-auth.json`, änderbar in der Oberfläche. Vergessen: `docker compose exec katalog-web katalog-server set-password` ist im Image ohne Shell nicht möglich, daher `docker compose run --rm -it katalog-web set-password`.
- Das Dateisystem des Containers ist schreibgeschützt, Modelle werden nur lesend eingebunden, der Prozess läuft ohne Root und ohne Linux-Fähigkeiten.

**English.** Small image (about 47 MB) with `katalog-server` and the web UI. It shows an **existing** catalog **read-only by default** in the browser; no streamed desktop. Same folders as the desktop image: `./config` (app data, the app's home) and `./models`. Run `docker compose up -d --build` in `docker-web/`; a random password is printed once in the log on first start. Reachable at `http://127.0.0.1:8080` by default; set `KATALOG_BIND_IP` and `KATALOG_ALLOWED_HOSTS` for other devices in the home network, use a VPN or an HTTPS reverse proxy for access from outside. The catalog must already exist in the same version (the web server never migrates). The password is stored as an Argon2 hash in `config/web-auth.json` and can be changed in the UI; to reset it use `docker compose run --rm -it katalog-web set-password`.

**Open:** Healthcheck (the image has no shell or curl: a server subcommand is needed), material/printer manager views (stage 6, part B), and the import interface (stage 5, part C). Downloads and the optional metadata and material/printer management backend are implemented.

**Optional write mode (English).** `KATALOG_WRITE=1` enables the 32 write commands for metadata and material/printer management (commented Compose option). Authentication and `KATALOG_ALLOWED_HOSTS` are mandatory; `/config` must be writable. **Choose web or local operation: do not run the desktop app and web server concurrently on the same database, even in read-only mode.** Started database work continues after the 30-second HTTP timeout. Reload before retrying: creating collections, print log entries, spools, printers or units again can produce duplicates. Metadata editing is available; manager views will be added in stage 6 part B.

**Offen:** Healthcheck (das Image enthält weder Shell noch curl; ein Server-Unterbefehl ist nötig), Material-/Druckerverwaltungsansichten (Stufe 6, Teil B) und die Import-Oberfläche (Stufe 5, Teil C). Downloads und das optionale Backend für Metadaten sowie Material- und Druckerverwaltung sind umgesetzt.

**Optionaler Schreibmodus (Deutsch).** `KATALOG_WRITE=1` aktiviert die 32 Schreib-Commands für Metadaten sowie Material- und Druckerverwaltung (auskommentierte Compose-Option). Anmeldung und `KATALOG_ALLOWED_HOSTS` sind Pflicht; `/config` muss schreibbar sein. **Entweder Web oder lokal: Desktop-App und Web-Server dürfen nicht parallel auf derselben Datenbank laufen, auch nicht im Lesemodus.** Gestartete DB-Arbeit läuft nach einem 30-s-HTTP-Timeout weiter. Vor Wiederholung neu laden: neue Sammlungen, Druckprotokolle, Spulen, Drucker und Einheiten können doppelt entstehen. Metadaten sind im Browser bearbeitbar; die Manager-Ansichten folgen in Stufe 6 Teil B.

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
