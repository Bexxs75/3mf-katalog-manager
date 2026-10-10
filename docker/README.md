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
Die App zeigt bei einem Treffer einen schließbaren Hinweis.

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
Verträglichkeit mit dem Basisimage und WebKit ist geprüft: Der Container startet, die Anmeldung und die App laufen.

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

### Modelle in die App bringen

Drag & Drop vom Desktop in das Browserfenster geht nicht (der Browser nimmt dort
keine Dateien an). So kommen Modelle stattdessen in die App:

1. **Ordner einbinden.** Lege die Modelle in den Ordner, der in Compose als
   `/models` eingebunden ist (Standard `./models` oder `MODELS_DIR`). Der Ordner
   muss schreibbar sein, wenn die App Dateien ablegen oder Archive entpacken soll.
2. **Erster Start:** „Bestehende Ordnerstruktur übernehmen“, im Dateidialog links
   „Modelle“ wählen und bestätigen. Alle Modelle kommen mit ihrer Ordnerstruktur
   in den Katalog.
3. **Später neue Dateien:** auf dem Rechner in den eingebundenen Ordner kopieren,
   dann oben links „Importieren“, „Dateien…“ oder „Ordner…“, im Dialog „Modelle“.
   Liegen die Dateien in einem anderen eingebundenen Ordner, im Dialog **Strg+L**
   drücken und den Pfad tippen, z. B. `/import/Datei.stl`.
4. **ZIP-Archive:** wie Dateien importieren. Im Fenster „Archive entpacken“ bei
   „Ändern…“ einen **schreibbaren** Zielordner wählen, z. B. unterhalb von „Modelle“.

Hinweis: Nach einem Neustart des Containers die Browserseite neu laden (F5), sonst
kann KasmVNC mit einem Skriptfehler stehen bleiben.

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
- Updates über Images (`docker compose pull`, dann `docker compose up -d`). Die App
  blendet im Container Slicer, „Im Dateimanager anzeigen“, Ordner-/Link-Öffnen und
  ihren eigenen Updater aus (`THREEMF_CONTAINER=1`); Links erscheinen als Text mit „Link kopieren“.
- Browser-Clipboard, Tastatur, Dateidialoge und WebKit können von der Desktop-App abweichen.

### Was geprüft ist

- `bash scripts/check.sh` prüft ohne Docker die Skripte, Compose- und Dockerfile-Struktur sowie Passwort-, Dateisystem- und Supervisor-Logik.
- `bash scripts/ci-smoke.sh IMAGE` startet das fertige Image und prüft Anmeldung, erzeugtes und eigenes Passwort, Neustart der App nach einem Absturz, sauberes Beenden und `ALLOW_NO_AUTH`. Der Release-Workflow führt ihn vor dem Veröffentlichen aus.
- `bash tests/upgrade.sh ALT NEU` prüft ein Update auf demselben Volume (Schema-Migration mit Sicherung vorher), `bash tests/permissions.sh IMAGE` fremde `PUID`/`PGID` und einen schreibgeschützten Modellordner.
- Von Hand im Container geprüft: Import aus dem Dateidialog, Ordnerübernahme, Archiv-Dialog, Katalog-Export und -Import, Druckeranbindung an einen Moonraker-Testserver, `no-new-privileges:true`.
- Noch nicht geprüft: 3D-Vorschau mit vielen Modellen, Wiederherstellung mit Papierkorbdateien, arm64.

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
checked: the container starts, and login and app work.

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

### Getting models into the app

Drag and drop from the desktop into the browser window does not work (the browser
does not accept files there). Use this instead:

1. **Mount a folder.** Put the models in the folder mounted as `/models` in Compose
   (default `./models` or `MODELS_DIR`). The folder must be writable if the app is
   to store files or unpack archives.
2. **First start:** “Adopt existing folder structure”, pick “Modelle” (models) on
   the left of the file dialog and confirm. All models enter the catalog with their
   folder structure.
3. **New files later:** copy them into the mounted folder on your computer, then
   “Import” at the top left, “Files…” or “Folder…”, and “Modelle” in the dialog. If
   the files sit in another mounted folder, press **Ctrl+L** in the dialog and type
   the path, e.g. `/import/file.stl`.
4. **ZIP archives:** import them like files. In the “Unpack archives” window use
   “Change…” to choose a **writable** target folder, e.g. below “Modelle”.

Note: after a container restart reload the browser page (F5); otherwise KasmVNC may
stop with a script error.

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
- Update via images (`docker compose pull`, then `docker compose up -d`). In the
  container the app hides slicer, “Show in file manager”, folder/link opening and its
  own updater (`THREEMF_CONTAINER=1`); links appear as text with “Copy link”.
- Browser clipboard, keyboard, file dialogs and WebKit may differ from the desktop app.

### What has been tested

- `bash scripts/check.sh` checks the scripts, Compose and Dockerfile structure and the password, file-system and supervisor logic without Docker.
- `bash scripts/ci-smoke.sh IMAGE` starts the built image and checks login, generated and supplied passwords, app restart after a crash, clean stop and `ALLOW_NO_AUTH`. The release workflow runs it before publishing.
- `bash tests/upgrade.sh OLD NEW` checks an update on the same volume (schema migration with a copy beforehand); `bash tests/permissions.sh IMAGE` checks foreign `PUID`/`PGID` and a read-only models folder.
- Checked by hand in the container: import from the file dialog, folder adoption, the archive dialog, catalog export and import, the printer link to a Moonraker test server, `no-new-privileges:true`.
- Not yet tested: 3D preview with many models, restore with trash files, arm64.

[License notices](LICENSES.md).
