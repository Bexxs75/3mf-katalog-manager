# docker-web: Web-Version des 3MF Katalog Manager (Entwurf, nicht öffentlich)

**Deutsch.** Kleines Image (ca. 47 MB) mit `katalog-server` und der Web-Oberfläche. Es zeigt einen **vorhandenen** Katalog **nur lesend** im Browser; kein gestreamter Desktop. Gleiche Ordner wie das Desktop-Image: `./config` (Programmdaten, Heimatordner der App) und `./models`.

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

**English.** Small image (about 47 MB) with `katalog-server` and the web UI. It shows an **existing** catalog **read-only** in the browser; no streamed desktop. Same folders as the desktop image: `./config` (app data, the app's home) and `./models`. Run `docker compose up -d --build` in `docker-web/`; a random password is printed once in the log on first start. Reachable at `http://127.0.0.1:8080` by default; set `KATALOG_BIND_IP` and `KATALOG_ALLOWED_HOSTS` for other devices in the home network, use a VPN or an HTTPS reverse proxy for access from outside. The catalog must already exist in the same version (the web server never migrates). The password is stored as an Argon2 hash in `config/web-auth.json` and can be changed in the UI; to reset it use `docker compose run --rm -it katalog-web set-password`.

**Offen / Open:** Healthcheck (the image has no shell or curl: add a `healthcheck` subcommand to the server), download route (stage 2c), edit and import (stages 4 and 5).
