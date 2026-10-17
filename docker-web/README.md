# docker-web: Web-Version des 3MF Katalog Manager (Entwurf, nicht öffentlich)

**Deutsch.** Kleines Image (ca. 47 MB) mit `katalog-server` und der Web-Oberfläche. Es zeigt einen **vorhandenen** Katalog **standardmäßig nur lesend** im Browser; kein gestreamter Desktop. Gleiche Ordner wie das Desktop-Image: `./config` (Programmdaten, Heimatordner der App) und `./models`.

```
cd docker-web
mkdir -p config models          # Katalog: config/.local/share/com.thebexxs.mfkatalogmanager/catalog.db
docker compose up -d --build
docker compose logs katalog-web # beim ersten Start steht dort einmal das Zufallspasswort
```
- Erreichbar unter `http://127.0.0.1:8080`. Für andere Geräte im Heimnetz `KATALOG_BIND_IP` auf die Adresse des Servers setzen und `KATALOG_ALLOWED_HOSTS` eintragen (Pflicht, sonst startet der Server nicht; Compose leitet für `localhost`/`127.0.0.1` den Standard aus `KATALOG_PORT` ab). Von außen nur per VPN oder Reverse Proxy mit HTTPS.
- Der Katalog muss zuvor von der Desktop-App (oder dem Desktop-Image) in der **gleichen Version** angelegt oder migriert sein; der Webserver migriert nie.
- Passwort: erster Start aus `KATALOG_PASSWORD(_FILE)` (z. B. `/config/.kasmvnc-password` des Desktop-Images) oder zufällig; danach als Argon2-Hash in `config/web-auth.json`, änderbar in der Oberfläche. Vergessen: `docker compose exec katalog-web katalog-server set-password` ist im Image ohne Shell nicht möglich, daher `docker compose run --rm -it katalog-web set-password`.
- Das Dateisystem des Containers ist schreibgeschützt, Modelle werden nur lesend eingebunden, der Prozess läuft ohne Root und ohne Linux-Fähigkeiten.

**English.** Small image (about 47 MB) with `katalog-server` and the web UI. It shows an **existing** catalog **read-only by default** in the browser; no streamed desktop. Same folders as the desktop image: `./config` (app data, the app's home) and `./models`. Run `docker compose up -d --build` in `docker-web/`; a random password is printed once in the log on first start. Reachable at `http://127.0.0.1:8080` by default; set `KATALOG_BIND_IP` and `KATALOG_ALLOWED_HOSTS` (mandatory, the server refuses to start otherwise; Compose derives the `localhost`/`127.0.0.1` default from `KATALOG_PORT`) for other devices in the home network, use a VPN or an HTTPS reverse proxy for access from outside. The catalog must already exist in the same version (the web server never migrates). The password is stored as an Argon2 hash in `config/web-auth.json` and can be changed in the UI; to reset it use `docker compose run --rm -it katalog-web set-password`.

**Open:** Healthcheck (the image has no shell or curl: a server subcommand is needed), material/printer manager views (stage 6, part B), and the import interface (stage 5, part C). Downloads and the optional metadata and material/printer management backend are implemented.

**Optional write mode (English).** `KATALOG_WRITE=1` enables the 32 write commands for metadata and material/printer management (commented Compose option). Authentication and `KATALOG_ALLOWED_HOSTS` are mandatory; `/config` must be writable. **Choose web or local operation: do not run the desktop app and web server concurrently on the same database, even in read-only mode.** Started database work continues after the 30-second HTTP timeout. Reload before retrying: creating collections, print log entries, spools, printers or units again can produce duplicates. Metadata editing is available; manager views will be added in stage 6 part B.

**Network binding and proxies (English).** If `KATALOG_BIND` is not a loopback address, `KATALOG_ALLOWED_HOSTS` is required and the server refuses to start without it (DNS-rebinding protection); list every name and `host:port` browsers use. The server limits connections to 32 per client address (IPv6: per /64) in addition to 256 in total. `KATALOG_TRUST_PROXY=1` makes the server believe `X-Forwarded-For`/`Forwarded` headers: use it only behind a proxy that shields the server and sets these headers itself. Then every client appears with the proxy's address, so the per-address connection limit is not applied (only the global limit); with a non-loopback bind the server prints a warning at startup.

**Offen:** Healthcheck (das Image enthält weder Shell noch curl; ein Server-Unterbefehl ist nötig), Material-/Druckerverwaltungsansichten (Stufe 6, Teil B) und die Import-Oberfläche (Stufe 5, Teil C). Downloads und das optionale Backend für Metadaten sowie Material- und Druckerverwaltung sind umgesetzt.

**Optionaler Schreibmodus (Deutsch).** `KATALOG_WRITE=1` aktiviert die 32 Schreib-Commands für Metadaten sowie Material- und Druckerverwaltung (auskommentierte Compose-Option). Anmeldung und `KATALOG_ALLOWED_HOSTS` sind Pflicht; `/config` muss schreibbar sein. **Entweder Web oder lokal: Desktop-App und Web-Server dürfen nicht parallel auf derselben Datenbank laufen, auch nicht im Lesemodus.** Gestartete DB-Arbeit läuft nach einem 30-s-HTTP-Timeout weiter. Vor Wiederholung neu laden: neue Sammlungen, Druckprotokolle, Spulen, Drucker und Einheiten können doppelt entstehen. Metadaten sind im Browser bearbeitbar; die Manager-Ansichten folgen in Stufe 6 Teil B.

**Netzbindung und Proxy (Deutsch).** Ist `KATALOG_BIND` keine Loopback-Adresse, ist `KATALOG_ALLOWED_HOSTS` Pflicht, und der Server startet sonst nicht (Schutz vor DNS-Rebinding); alle Namen und `Host:Port`, unter denen Browser den Server erreichen, eintragen. Der Server begrenzt Verbindungen auf 32 pro Client-Adresse (IPv6: pro /64) zusätzlich zu insgesamt 256. `KATALOG_TRUST_PROXY=1` lässt den Server den Headern `X-Forwarded-For`/`Forwarded` glauben: nur hinter einem Proxy verwenden, der den Server abschirmt und diese Header selbst setzt. Dann erscheinen alle Clients mit der Adresse des Proxys, daher greift die Pro-Adresse-Grenze nicht (nur das globale Limit); bei Nicht-Loopback-Bindung gibt der Server beim Start eine Warnung aus.

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
network/VPN and maintain external backups. Never run desktop and web against the
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

### Optionale Dateiverwaltung im Web (Stufe 7, Server)

**Warnung:** `KATALOG_FILE_OPS=1` erlaubt dem Webserver, Dateien umzubenennen,
zu verschieben und zu löschen. Standard ist aus. Erforderlich sind
`KATALOG_WRITE=1`, Anmeldung und explizite `KATALOG_ALLOWED_HOSTS`. Die
Browseroberfläche folgt separat. Nur im Heimnetz/VPN betreiben; regelmäßige
externe Sicherungen anlegen. Desktop und Webserver niemals parallel auf derselben
Datenbank betreiben. Auch späteres Öffnen derselben DB im Desktop ist relevant:
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
