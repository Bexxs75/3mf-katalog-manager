# 3MF Katalog Manager im Container / in a container

## Deutsch

Ein Browser-Desktop für die Linux-AppImage, für x86_64/amd64. Benötigt Docker
mit Docker Compose. Diese Dateien sind statisch und mit lokalen Skripttests
geprüft; Image-Bau und Start sind noch nicht gegen das echte Basisimage geprüft.
Das offizielle GHCR-Image wird erst mit dem separaten Veröffentlichungspaket
bereitgestellt. Bis dahin ist ein lokaler Bau mit einem konkreten App-Artefakt nötig.

### Start

Im Verzeichnis `docker/`:

```bash
cp .env.example .env
mkdir -p config models
# .env bearbeiten: PUID/PGID nach `id -u` und `id -g`, TZ und MODELS_DIR setzen.
docker compose up -d
docker compose logs 3mf-katalog
```

http://127.0.0.1:3000 öffnen (3001: HTTPS-Zugang des Basisimages).
Beim ersten Start steht das zufällige Passwort gut sichtbar im Log;
Benutzername standardmäßig `abc`. Auch `docker logs` funktioniert:

```bash
docker logs "$(docker compose ps -q 3mf-katalog)"
cat config/.kasmvnc-password
```

Die Datei gehört der App-UID und hat Rechte `600`; zum Lesen ggf. den passenden
Host-Benutzer verwenden. Später wird das Passwort wiederverwendet und nicht
noch einmal ausgegeben. Container-Logs enthalten daher beim ersten Start ein
Geheimnis: Zugriff darauf beschränken. `PASSWORD` und `CUSTOM_USER` in `.env`
überschreiben die Vorgaben; ein eigenes Passwort wird nie ausgegeben und ersetzt
nicht die gespeicherte Zufallsdatei. Nach Entfernen von `PASSWORD` gilt wieder
das gespeicherte Passwort. Das erzeugte Passwort wird nur über s6-Laufzeitdateien
weitergegeben, nicht in Dockers gespeicherte Container-Umgebung geschrieben.
Ein selbst gesetztes `PASSWORD` ist dagegen in `docker inspect` sichtbar.
`ALLOW_NO_AUTH=true` schaltet die Anmeldung ausdrücklich ab, auch bei gesetztem
Passwort, und erzeugt eine Warnung. `.env` privat halten (z. B. `chmod 600 .env`).

### Lokaler Bau

`APP_URL` (HTTPS, Linux x86_64 AppImage) und die dazugehörige `APP_VERSION` in
`.env` setzen, dann:

```bash
docker compose build
docker compose up -d --no-build
```

Es gibt bewusst keinen Standarddownload der alten, veränderlichen Preview aus
dem Prototyp. URL und Version müssen zusammenpassen. `BASE_IMAGE` lässt sich
auf einen Tag oder Digest festlegen; Vorgabe ist
`ghcr.io/linuxserver/baseimage-kasmvnc:ubuntunoble`. Digest-Pinning und
Artefakt-Herkunft/Prüfsummen werden im separaten CI-Paket festgelegt.
Der Download und das Entpacken erfolgen in einer eigenen Build-Stufe. `curl`
wird der finalen Stufe nicht hinzugefügt; eine vom Basisimage bereits mitgebrachte
Installation wird nicht entfernt.

### Ordner und Rechte

```text
docker/
  .env                   persönliche Parameter, nicht einchecken
  config/                /config: App-Home, Datenbank, Einstellungen, Papierkorb
  models/                /models: Modelle (oder MODELS_DIR auf dem Host)
```

`/config` muss auf einem **lokalen** Volume liegen. Für Netzwerk-/FUSE-Dateisysteme
warnt der Start und legt `/config/.network-filesystem-warning` an; bei lokalem
Dateisystem wird ein alter Marker entfernt. Die Erkennung mit `stat -f` ist eine
Warnhilfe, keine Garantie über darunterliegende Speicher- und Sperrmechanismen.
Die spätere App-Anzeige des Markers gehört zum separaten App-Paket.

`/models` darf eine NAS-Freigabe sein; im Dateidialog heißt das Lesezeichen
„Modelle“. Für zusätzliche Bibliotheken den auskommentierten `/models2`-Mount
anpassen. **Containerpfade dauerhaft beibehalten**: Die Datenbank speichert
absolute Pfade. `MODELS_DIR` wählt nur die Host-Quelle.

`PUID`/`PGID` steuern den App-Benutzer `abc` wie im Basisimage; Host-Ordner müssen
für diese IDs schreibbar sein. NAS-ACLs zusätzlich prüfen. `CUSTOM_USER` ist der
Web-Login, kein Ersatz für `PUID`/`PGID`. `UMASK=022` ist Standard, bei gemeinsamer
Gruppe ggf. `002`; die Passwortdatei bleibt `600`. Der App-Dienst läuft als `abc`.
Keine zusätzlichen Linux-Capabilities, kein privilegierter Container, kein
Docker-Socket. `no-new-privileges:true` ist im Beispiel gesetzt; seine
Verträglichkeit mit dem Basisimage und WebKit ist **noch nicht am Image geprüft**.

### Aktualisieren und prüfen

`IMAGE_TAG` in `.env` wählt z. B. `latest`, eine feste veröffentlichte Version
oder `rc`. **Stable und rc brauchen getrennte `/config`-Ordner** und getrennte
Compose-Projekte; nicht denselben Katalog teilen. Für parallelen Betrieb auch
die Host-Ports unterscheiden. Eine automatische rc→stable-Übernahme ist hier
nicht vorgesehen.

```bash
docker compose pull
docker compose up -d
docker compose ps
docker inspect --format '{{.State.Health.Status}}' "$(docker compose ps -q 3mf-katalog)"
docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.version"}}' \
  "$(docker inspect --format '{{.Image}}' "$(docker compose ps -q 3mf-katalog)")"
```

Danach auch die Versionsanzeige in der App prüfen. `pull` allein ersetzt keinen
laufenden Container. Bei lokal gebauten Images stattdessen passende URL/Version
setzen, `docker compose build` und `docker compose up -d --no-build` ausführen;
dieselben Versions-/Health-Prüfungen anschließen. Der Healthcheck übernimmt das
Image: HTTP 200/401 auf Port 3000 **und** laufender App-Prozess; Startphase 120 s,
Intervall 30 s. Er prüft keine vollständige GUI-Bedienbarkeit.

Die App wird nach Beendigung automatisch neu gestartet. Bei schnellen Abstürzen
wächst die Pause von 2 bis 30 s; nach mindestens 60 s Laufzeit wird sie zurückgesetzt.
App-Ausgaben und Neustartmeldungen stehen in `docker logs`. `docker compose stop`
beendet App und WebKit-Prozessgruppe per SIGTERM; 30 s erlauben das Schließen von
SQLite/WAL, bevor Docker hart beendet. `docker compose down` entfernt Container
und Netz, erhält aber die Host-Ordner.

### Sicherungen

Die App-ZIP-Sicherung enthält **nur Datenbank und Einstellungen**, keine Modelle
und keinen Papierkorb. `/models` und weitere Modell-Mounts separat sichern.
Für eine vollständige Wiederherstellung auch `/config` inklusive Papierkorb und
WebView-Einstellungen erhalten/sichern. Vor einer Dateikopie von `/config` den
Container stoppen; alternativ die App-Sicherungsfunktion für die Datenbank nutzen.
Beim Wiederherstellen Mount-Ziele beibehalten und **Druckerverbindungen danach
wieder aktivieren**: Der Import pausiert sie bewusst.

### Netzwerk und Grenzen

Beide Ports sind nur an `127.0.0.1` gebunden. Für das Heimnetz die beiden
Host-Adressen in Compose auf die gewünschte LAN-IP ändern, z. B.
`192.168.1.10:3000:3000`, und die Anmeldung beibehalten. Nicht ungeschützt ins
Internet freigeben; externen Zugriff mit TLS und geeignetem Reverse Proxy planen.
Ausführliche Netzwerk-/Reverse-Proxy-Anleitungen folgen im D6-Paket im
[Projekt-Wiki](https://github.com/Bexxs75/3mf-katalog-manager/wiki).

- Kein „In Slicer öffnen“ oder „Ordner öffnen“ auf dem Host.
- Kein Drag & Drop vom Host-Desktop in die App.
- 3D-Vorschau per Software-Rendering, daher langsamer.
- Ein Benutzer gleichzeitig; nur x86_64, kein arm64.
- Updates über Images; den App-Updater nicht verwenden. Das Ausblenden von
  Host-Aktionen und Updater gehört zum separaten App-Paket (`THREEMF_CONTAINER=1`).
- Browser-Clipboard, Tastatur, Dateidialoge und WebKit können von der Desktop-App abweichen.

### Prüfumfang und offene Image-Prüfung

`bash scripts/check.sh` nutzt keinen Docker-Aufruf und kein Netzwerk. Es prüft
Shell-Syntax, ausführbare Dateien, einfache Compose-/Dockerfile-/s6-Strukturen,
Passwort-/Dateisystemlogik und den Supervisor mit Testprozessen. Shellcheck und
Hadolint laufen nur, wenn installiert. Das ersetzt keine Compose-Validierung,
keinen Image-Bau und keinen Ende-zu-Ende-Test.

**Nicht gegen das echte Image geprüft:** s6-overlay unter
`/etc/s6-overlay/s6-rc.d`, `init-adduser` (UID/GID), `init-kasmvnc`, `init-nginx`,
`svc-kasmvnc`, Import von `CUSTOM_USER`/`PASSWORD` aus
`/run/s6/container_environment`, X-Zugang als `abc` mit `DISPLAY=:1`,
`/defaults/autostart`, Log-Weiterleitung über PID 1 und Shutdown-Zeitlimits.
Der zusätzliche `init-3mf`-Dienst läuft nach `init-adduser`; beide
Web-Konfigurationsdienste erhalten ihn als Abhängigkeit. Der Bau bricht bei
fehlenden erwarteten Dienstverzeichnissen ab. Die App wartet vor dem Start mit
`xdpyinfo` auf den X-Server. Der einmalige App-Autostart wird durch `svc-3mf` ersetzt.

Claude muss Build, ersten/zweiten Start, eigene/abgeschaltete Anmeldung, geänderte
UID/GID, `no-new-privileges`, App-Absturz, Healthcheck, Logs und `docker stop`
am echten Image prüfen; bei anderem Upstream-Aufbau ist die Integration anzupassen.
[Lizenzhinweise](LICENSES.md).

### Hinweis zu alten Prototyp-Daten / Note on old prototype data
Nutzt du einen `/config`-Ordner des früheren Prototyps weiter, lösche vorher `config/.config/openbox/autostart`. Sonst startet das Basisimage die App ein zweites Mal neben dem überwachten Dienst. / If you reuse a `/config` folder from the earlier prototype, delete `config/.config/openbox/autostart` first; otherwise the base image starts a second app instance next to the supervised one.

## English

A browser desktop for the Linux AppImage, targeting x86_64/amd64. Requires Docker
and Docker Compose. These files have static and local script checks only;
the image build and startup have **not been checked against the real base image**.
The official GHCR image will be supplied by the separate publishing package.
Until then, build locally using a specific app artifact.

### Start

From `docker/`:

```bash
cp .env.example .env
mkdir -p config models
# Edit .env: set PUID/PGID from `id -u` / `id -g`, TZ and MODELS_DIR.
docker compose up -d
docker compose logs 3mf-katalog
```

Open http://127.0.0.1:3000 (3001 is the base image's HTTPS endpoint).
The first start prominently logs a random password; the default username is `abc`.
To find it:

```bash
docker logs "$(docker compose ps -q 3mf-katalog)"
cat config/.kasmvnc-password
```

The file belongs to the app UID and has mode `600`; use the appropriate host
account to read it. Subsequent starts reuse it without logging it again.
Restrict log access: the first-start log contains a secret. Set `PASSWORD` and
`CUSTOM_USER` in `.env` to override defaults. A supplied password is never logged
and does not replace the saved random password. Removing `PASSWORD` restores
the saved password. Generated credentials use s6 runtime files, not Docker's
stored container environment. A user-supplied `PASSWORD` is visible in
`docker inspect`. `ALLOW_NO_AUTH=true` explicitly disables authentication even
with a supplied password and logs a warning. Keep `.env` private, e.g. `chmod 600 .env`.

### Local build

Set `APP_URL` (HTTPS, Linux x86_64 AppImage) and matching `APP_VERSION` in `.env`:

```bash
docker compose build
docker compose up -d --no-build
```

There is deliberately no default download of the prototype's old mutable preview.
URL and version must match. `BASE_IMAGE` accepts a tag or digest; the default is
`ghcr.io/linuxserver/baseimage-kasmvnc:ubuntunoble`. Digest pinning and artifact
provenance/checksums belong to the separate CI package. Download and extraction
use a separate build stage. The final stage does not add `curl`; any copy already
included upstream is retained.

### Folders and permissions

```text
docker/
  .env                   private parameters, never commit
  config/                /config: app home, database, settings, trash
  models/                /models: models (or host MODELS_DIR)
```

`/config` needs a **local** volume. Network/FUSE filesystems trigger a startup
warning and `/config/.network-filesystem-warning`; local filesystems remove an
old marker. `stat -f` detection is a warning aid, not a guarantee about underlying
storage or locking. Displaying the marker in the app belongs to the separate app package.

`/models` may be a NAS share; its file-dialog bookmark is “Modelle”. Adapt the
commented `/models2` mount for extra libraries. **Keep container paths unchanged**:
the database stores absolute paths. `MODELS_DIR` only changes the host source.

`PUID`/`PGID` configure app user `abc` through the base image. Host folders must be
writable by those IDs; also check NAS ACLs. `CUSTOM_USER` is the web login, not a
replacement for UID/GID. Default `UMASK=022`; use e.g. `002` for group sharing.
The password stays mode `600`. The app service runs as `abc`. No added Linux
capabilities, privileged mode or Docker socket. The example sets
`no-new-privileges:true`; compatibility with the base image and WebKit is
**not yet checked against the image**.

### Update and verify

Set `IMAGE_TAG` in `.env` to `latest`, a published fixed version or `rc`.
**Stable and rc require separate `/config` folders** and Compose projects;
do not share catalogs. Parallel instances also need distinct host ports.
There is no automatic rc-to-stable migration here.

```bash
docker compose pull
docker compose up -d
docker compose ps
docker inspect --format '{{.State.Health.Status}}' "$(docker compose ps -q 3mf-katalog)"
docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.version"}}' \
  "$(docker inspect --format '{{.Image}}' "$(docker compose ps -q 3mf-katalog)")"
```

Also check the app's version display. `pull` alone does not replace a running
container. For local images, set the new matching URL/version, run
`docker compose build` then `docker compose up -d --no-build`, and perform the
same version/health checks. The image supplies the healthcheck: HTTP 200/401 on
port 3000 **and** a running app process; 120 s start period and 30 s interval.
It does not verify full GUI responsiveness.

The app restarts after exit. Rapid failures increase the delay from 2 to 30 s;
a run lasting at least 60 s resets it. App output and restart messages go to
`docker logs`. `docker compose stop` sends SIGTERM to the app and its WebKit
process group; the 30 s grace period allows SQLite/WAL to close before Docker
kills remaining processes. `docker compose down` removes containers and the
network while keeping host folders.

### Backups

The app ZIP contains **only the database and settings**, not models or trash.
Back up `/models` and other model mounts separately. For full recovery also
preserve/back up `/config`, including trash and WebView settings. Stop the
container before copying `/config`; alternatively use the app's backup feature
for the database. Preserve mount targets when restoring and **reactivate printer
connections afterwards**: import deliberately pauses them.

### Networking and limitations

Both ports bind only to `127.0.0.1`. For LAN access, replace both host addresses
in Compose with the desired LAN IP, e.g. `192.168.1.10:3000:3000`, and retain
authentication. Never expose it unprotected to the Internet; plan TLS and an
appropriate reverse proxy for external access. Detailed network/reverse-proxy
instructions will follow in package D6 in the
[project wiki](https://github.com/Bexxs75/3mf-katalog-manager/wiki).

- No “Open in slicer” or “Open folder” on the host.
- No drag and drop from the host desktop into the app.
- Slower, software-rendered 3D preview.
- One concurrent user; x86_64 only, no arm64.
- Update via images; do not use the app updater. Hiding host actions and the
  updater belongs to the separate app package (`THREEMF_CONTAINER=1`).
- Browser clipboard, keyboard, file dialogs and WebKit may differ from the desktop app.

### Verification scope and pending image tests

`bash scripts/check.sh` uses neither Docker nor networking. It checks shell
syntax, executable files, basic Compose/Dockerfile/s6 structure, authentication,
filesystem handling, and supervision with test processes. Shellcheck and Hadolint
run if installed. This does not replace Compose validation, building the image
or end-to-end tests.

**Not checked against the real image:** s6-overlay at
`/etc/s6-overlay/s6-rc.d`, `init-adduser` (UID/GID), `init-kasmvnc`, `init-nginx`,
`svc-kasmvnc`, importing `CUSTOM_USER`/`PASSWORD` from
`/run/s6/container_environment`, X access as `abc` with `DISPLAY=:1`,
`/defaults/autostart`, logging through PID 1 and shutdown timeouts.
The added `init-3mf` service follows `init-adduser`; both web configuration
services depend on it. Building fails if expected upstream service directories
are absent. The app waits for X using `xdpyinfo`. `svc-3mf` replaces the one-shot
app autostart.

Claude must check build, first/second startup, supplied/disabled authentication,
custom UID/GID, `no-new-privileges`, app crashes, health, logs and `docker stop`
against the real image; a different upstream layout requires adapting the integration.
[License notices](LICENSES.md).
