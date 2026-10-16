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

**Open:** Healthcheck (the image has no shell or curl: a server subcommand is needed), material/printer manager views (stage 6, part B), and import (stage 5). Downloads and the optional metadata and material/printer management backend are implemented.

**Optional write mode (English).** `KATALOG_WRITE=1` enables the 32 write commands for metadata and material/printer management (commented Compose option). Authentication and `KATALOG_ALLOWED_HOSTS` are mandatory; `/config` must be writable. **Choose web or local operation: do not run the desktop app and web server concurrently on the same database, even in read-only mode.** Started database work continues after the 30-second HTTP timeout. Reload before retrying: creating collections, print log entries, spools, printers or units again can produce duplicates. Metadata editing is available; manager views will be added in stage 6 part B.

**Offen:** Healthcheck (das Image enthält weder Shell noch curl; ein Server-Unterbefehl ist nötig), Material-/Druckerverwaltungsansichten (Stufe 6, Teil B) und Import (Stufe 5). Downloads und das optionale Backend für Metadaten sowie Material- und Druckerverwaltung sind umgesetzt.

**Optionaler Schreibmodus (Deutsch).** `KATALOG_WRITE=1` aktiviert die 32 Schreib-Commands für Metadaten sowie Material- und Druckerverwaltung (auskommentierte Compose-Option). Anmeldung und `KATALOG_ALLOWED_HOSTS` sind Pflicht; `/config` muss schreibbar sein. **Entweder Web oder lokal: Desktop-App und Web-Server dürfen nicht parallel auf derselben Datenbank laufen, auch nicht im Lesemodus.** Gestartete DB-Arbeit läuft nach einem 30-s-HTTP-Timeout weiter. Vor Wiederholung neu laden: neue Sammlungen, Druckprotokolle, Spulen, Drucker und Einheiten können doppelt entstehen. Metadaten sind im Browser bearbeitbar; die Manager-Ansichten folgen in Stufe 6 Teil B.
