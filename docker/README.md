# 3MF Katalog Manager in a container / im Container

## English

> **Still unstable:** The Docker version works, but may still contain bugs and change. It is experimental and in maintenance mode: only bug and security fixes, no new features. Feedback via [GitHub Issues](https://github.com/bexxs75/3mf-katalog-manager/issues) is welcome.

A browser desktop for the Linux AppImage, targeting x86_64/amd64. Requires Docker
and Docker Compose. The workflow `.github/workflows/docker-image.yml` builds the
GHCR image from a published Linux-x86_64 AppImage. Before pushing, it checks local
scripts, Python unit tests, startup and authentication against the built image.
It publishes exactly that tested candidate, without rebuilding.
A successful workflow run provides the evidence for each image.

### Start

For every release (stable and RC), the image workflow provides a ready-to-use
`docker-compose.yml` download after successful publication.
For stable releases: [latest stable Compose file](https://github.com/Bexxs75/3mf-katalog-manager/releases/latest/download/docker-compose.yml).
For RCs, download the file **from the asset list of the respective release**.
It uses the published image with that exact release version as its default;
`IMAGE_TAG` can override it. Start without cloning:

```bash
mkdir -p 3mf-katalog/config 3mf-katalog/models
cd 3mf-katalog
# Save the downloaded docker-compose.yml here.
# Optional: create .env with your parameters such as PUID, PGID, TZ or MODELS_DIR.
docker compose up -d
```

When using the files from the repository instead:

`.env.example` and the Compose fallback use `latest`, the newest stable image
(0.16.0 or later). Set `IMAGE_TAG=rc` only if you want pre-releases, or a specific
published version without `v` to stay on it. `latest` follows only the highest
published stable version; `MAJOR.MINOR` follows
only the highest stable version in its series. RC releases publish their specific
version and `rc`.

From `docker/`:

```bash
cp .env.example .env
mkdir -p config models
# Edit .env: set IMAGE_TAG, PUID/PGID from `id -u` / `id -g`, TZ and MODELS_DIR.
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

For supplied credentials, `PASSWORD` must not contain spaces, tabs, line breaks
or any of `*`, `?`, `[`, `:`, `%`, `\`, and must not start with `-`.
`CUSTOM_USER` allows only `A-Z`, `a-z`, `0-9`, `.`, `_`, `-`, also without a
leading `-`. Invalid values abort container startup with an error, even with
`ALLOW_NO_AUTH=true`. Empty values still use the defaults; generated passwords
remain 48 hex characters long. An invalid saved password file aborts startup
when reused and remains unchanged. To recover, delete the file (a new password is generated) or set `PASSWORD`.
These restrictions prevent the base image from
altering credentials when setting up authentication.

### Local build

Set `APP_URL` (HTTPS, Linux x86_64 AppImage) and matching `APP_VERSION` in `.env`.
Both lines are already in the file, empty, after you copy `.env.example`: put the
value **right after the equals sign** and do not repeat `APP_URL=` inside the value
(right: `APP_URL=https://github.com/…`, wrong: `APP_URL=APP_URL=https://github.com/…`).
Otherwise the build stops with an error message:

```bash
docker compose build
docker compose up -d --no-build
```

There is deliberately no default download of the prototype's old mutable preview.
URL and version must match. `BASE_IMAGE` accepts a tag or digest; an older `.env` containing `BASE_IMAGE=…baseimage-kasmvnc…` overrides this default and
makes the build fail: remove the line or set the new value from `.env.example`. The default is
`ghcr.io/linuxserver/baseimage-selkies:ubuntunoble` (LinuxServer base with Selkies,
X11/Openbox), already fixed by SHA256 digest in the Dockerfile and in
`docker-compose.yml`, so a later build does not point to a different base image.
The release workflow pins the same base image by digest and computes the release download's SHA256;
the Dockerfile checks it on the second download through `APP_SHA256`. This checks
consistency, not independent provenance. The revision label records the checked-out
source commit from the default branch; the version label records the app version.
The push does not generate SBOM/provenance attestations. Download and extraction
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

**Umlauts in file names:** If `ls` on the server shows a name like `K$'\303\266'rper.obj` instead of `Körper.obj`, only the terminal display is affected, not the file name (`\303\266` is an ö). The shell then has no UTF-8 setting. A tester reported that `LANG=de_DE.UTF-8` in their environment fixed the display (not verified by me). Set the time zone with `TZ` in `.env` as above.

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

**Upload from your computer:** Open `/katalog/`, choose files under “Upload files”
or drag them onto the drop area. They arrive in **Eingang** (`/models/Eingang`).
Then in the app choose **Import → Folder… → Eingang**. The companion never writes
to the catalog database; the app does not automatically watch the folder. Uploads
require JavaScript and the same login as the companion. Each file has progress
and cancellation; the browser sends files sequentially, the service allows two
uploads in total.

Prerequisite: `/models` must be **writable** by service user `abc` (Compose
PUID/PGID). Eingang is created on the first upload only (0755, files 0644).
If permissions are missing, the page shows a hint instead of the upload area:
mount without `:ro`, set host ownership and write/search permissions on the target
or its existing parent to match PUID/PGID. A file path instead of a directory
also cannot be used.

Optional: `KATALOG_UPLOAD_DIR` (default `/models/Eingang`) and
`KATALOG_UPLOAD_MAX_MB` (default **2048 MiB per file**, positive integer).
Uncomment the lines in **both** `.env` and Compose. Empty values use service
defaults. The destination must stay inside an allowed root after resolving
symlinks (`KATALOG_ALLOWED_ROOTS`, otherwise existing `/models`, `/models2`, etc.).
Allowed: `.3mf`, `.stl`, `.obj`, `.stp`, `.step`, `.zip`, `.7z`, `.rar`; filenames
and initial signatures are checked. Uploads do not extract archives. This is not
full format validation or a virus scan. Existing files remain intact; duplicate
names get ` (2)` through ` (100)`, then a conflict error. Available space must
exceed the file size plus 256 MiB. Uploads abort after 60 seconds without body
data. Interrupted temporary files are removed; after a service crash the next
startup removes leftovers older than an hour. HTTP body length bounds the upload;
immediately observable excess bytes are rejected and the connection is closed.

The built-in nginx allows **4 GiB**; the service sets the actual limit, so keep
`KATALOG_UPLOAD_MAX_MB` at **4096 or below**. An upstream **reverse proxy** also
needs a higher size limit and streaming. A tester uploaded two large files through his Nginx Proxy Manager **without any
change**. If your proxy rejects large uploads (error "413"), set
`client_max_body_size 4g;` and `proxy_request_buffering off;` in the **Advanced** tab.
Proxy timeouts must accommodate the transfer (internal timeouts: 3600 s).

**Existing folder, network drive or `scp` method:** Drag and drop directly into
the streamed desktop is not intended: stream file transfer is switched off
(`HARDEN_DESKTOP=true`). To transfer via the server:

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

### Download and slicer

The **“Download & slicer”** sidebar section provides the catalog list link, a short guide, and the requirements for local slicers.

In the container app's detail view, **“Open in browser”** shows a link to the
selected file with its slicer menu expanded. Copy it into the browser where you
use the app to download the file or open it in your local slicer. The app in the
streamed desktop does not open a browser tab itself.
After the first visit to `/katalog/`, the app automatically knows the address
(from `Host` and `X-Forwarded-Proto`); further visits update it.
Until then, the dialog shows the path to append to your browser address.
To fix the address, set `KATALOG_PUBLIC_URL`, e.g.
`KATALOG_PUBLIC_URL=https://katalog.example.de` behind a reverse proxy or
`http://192.168.1.10:3000` on a home network. Use the address you actually open in
your browser, not a placeholder. Uncomment the lines in **both** `.env` and
`docker-compose.yml`; an empty value in `.env` counts as not set. This variable takes priority.
Only `http://` or `https://` with a host and optional port are accepted, without
a path, query, fragment or trailing slash. Invalid values are ignored with a log message.

Open **http://<Host>:3000/katalog/** in a normal browser tab alongside the
browser desktop. The companion uses the same login and displays thumbnails,
name search, type/folder/tag filters, sorting and pagination. “Download” saves
a file to your PC; “Open in slicer” passes it to a slicer installed there.
Search and downloads work without JavaScript; on macOS JavaScript adapts the
Bambu link. The page speaks German,
English, Spanish and French: the language comes from `Accept-Language` (German by
default) and can be chosen in the header bar (`?lang=` parameter, works without
JavaScript); light/dark appearance follows the system preference.

**Hiding files from the list:** “Remove from list” asks for confirmation and only
hides the file on this companion page (downloads and slicer links then return 404,
including links issued earlier). The catalog, its database and the storage stay
untouched; the companion never writes to the catalog and never deletes files. The
list of hidden files is kept in `/config/.katalog-web-hidden.json` (part of the
`/config` folder, preserved across updates). To show a file again, open “Show hidden
files (N)” below the filters and choose “Show again”; right after removing there is
also “Undo”.

| Slicer | File types |
|---|---|
| OrcaSlicer | 3MF, STL, OBJ, STP/STEP |
| Bambu Studio | 3MF only (special link on macOS); STL does not open through the link, confirmed by a tester |
| Creality Print | 3MF, STL, OBJ, STP/STEP |
| ElegooSlicer | 3MF, STL, OBJ, STP/STEP |
| Cura | 3MF, STL, OBJ |
| Lychee | 3MF, STL, OBJ |
| SuperSlicer | 3MF, STL, OBJ, STP/STEP |

PrusaSlicer: download the file and open it in the slicer. The slicer must be
installed on your **PC** and able to reach the container's address from that PC.
On Linux its URL scheme must be registered. Slicers often reject self-signed
certificates at `https://<Host>:3001`: use `http://<Host>:3000` on your home
network or a reverse proxy with a valid certificate. The reverse proxy must
forward `Host` (including the public port) and `X-Forwarded-Proto`; enable
“Websockets” in Nginx Proxy Manager.

Slicer links remain valid for **one hour** and allow downloads without login
during that time. Treat them as credentials; reload the page if a link fails.
Only catalog files outside the trash and inside `/models` or `/models<number>`
are served; outward symlinks and paths containing `..` are blocked. Unavailable
or disallowed files remain visible without buttons. The service only reads the
database, runs as `abc` and listens on loopback. Its key is stored in
`/config/.katalog-web-secret` (32 random bytes, mode 600).

Optional container environment variables (add to Compose `environment`):
`CATALOG_DB` selects a fixed database path; otherwise the newest
`/config/.local/share/*/catalog.db` by modification time is used.
`KATALOG_ALLOWED_ROOTS` replaces the model roots with a colon-separated list of
absolute paths. `KATALOG_WEB_PORT` sets the internal port (default 8099); startup
also adjusts the nginx proxy. Do not publish this internal port on the host.

`src-tauri/src/lib.rs` creates `catalog.db` under `app.path().app_data_dir()`;
`src-tauri/tauri.conf.json` specifies `com.thebexxs.mfkatalogmanager`. Different
build identifiers use different data folders. The assumed Linux path below
`/config/.local/share` has **not been checked against the real image**.

The build checks nginx template anchors and the s6 service layout; missing
anchors fail the build. Before pushing, the workflow runs
`bash docker/tests/katalog-web.sh IMAGE` against the candidate. A fresh test volume
checks unauthenticated GET/POST on `/katalog/`, `/katalog/upload` and
`/katalog/remove` (401) on both ports, signed downloads without login and
service restart. The script creates test data and a random password and removes
the test container afterwards. Local unit tests make no network connections.
ZIP batch download (stage B) is not included yet.

### Networking and limitations

Published ports 3000/3001 bind only to `127.0.0.1` on the host. The internal
services (Selkies data connection 8082 and companion page 8099) listen exclusively
on `127.0.0.1` inside the container; access goes through nginx on 3000/3001.
For LAN access, replace both host addresses
in Compose with the desired LAN IP, e.g. `192.168.1.10:3000:3000`, and retain
authentication. Never expose it unprotected to the Internet; plan TLS and an
appropriate reverse proxy for external access. Detailed network/reverse-proxy
instructions will follow in package D6 in the
[project wiki](https://github.com/Bexxs75/3mf-katalog-manager/wiki).

- The desktop app cannot open host slicers or folders; slicer links are available on the companion page.
- No drag and drop from the host desktop into the app.
- Slower, software-rendered 3D preview.
- One concurrent user; x86_64 only, no arm64.
- Update via images (`docker compose pull`, then `docker compose up -d`). In the
  container the app hides slicer, “Show in file manager”, folder/link opening and its
  own updater (`THREEMF_CONTAINER=1`); links appear as text with “Copy link”.
- Browser clipboard, keyboard, file dialogs and WebKit may differ from the desktop app.

### Docker in a Proxmox LXC container

If you run Docker in an LXC container instead of a VM, a tester (Proxmox, from their own
experience) reports that the container needs the options `nesting=1` and `keyctl=1`. This
is a hint from a tester's practice; we have not tested running it in an LXC container
ourselves.

### What has been tested

- `bash scripts/check.sh` checks the scripts, Compose and Dockerfile structure and the password, file-system and supervisor logic without Docker.
- `bash scripts/ci-smoke.sh IMAGE` starts the built image and checks login, generated and supplied passwords, loopback binding of 8082/8099, app restart after a crash, clean stop and `ALLOW_NO_AUTH`. The release workflow runs it before publishing.
- `bash tests/upgrade.sh OLD NEW` checks an update on the same volume (schema migration with a copy beforehand); `bash tests/permissions.sh IMAGE` checks foreign `PUID`/`PGID` and a read-only models folder.
- Checked by hand in the container: import from the file dialog, folder adoption, the archive dialog, catalog export and import, the printer link to a Moonraker test server, `no-new-privileges:true`.
- Not yet tested: 3D preview with many models, restore with trash files, arm64, running in an LXC container.

[License notices](LICENSES.md).

## Deutsch

> **Noch instabil:** Die Docker-Version läuft, kann aber noch Fehler enthalten und sich ändern. Sie ist experimentell und befindet sich im Wartungsmodus: Es gibt nur noch Fehler- und Sicherheitskorrekturen, keine neuen Funktionen. Rückmeldungen über [GitHub-Issues](https://github.com/bexxs75/3mf-katalog-manager/issues) sind willkommen.

Ein Browser-Desktop für die Linux-AppImage, für x86_64/amd64. Benötigt Docker
mit Docker Compose. Der Workflow `.github/workflows/docker-image.yml` baut das
GHCR-Image aus einer veröffentlichten Linux-x86_64-AppImage. Vor dem Push prüft
er lokale Skripte, Python-Unit-Tests sowie Start und Anmeldung am gebauten Image.
Veröffentlicht wird exakt dieser getestete Kandidat, ohne erneuten Build.
Ein erfolgreicher Workflow-Lauf ist der Nachweis für das jeweilige Image.

### Start

Zu jedem Release (stabil und RC) stellt der Image-Workflow nach erfolgreicher
Veröffentlichung eine fertige `docker-compose.yml` als Download bereit.
Für stabile Releases: [neueste stabile Compose-Datei](https://github.com/Bexxs75/3mf-katalog-manager/releases/latest/download/docker-compose.yml).
Für RCs die Datei **aus der Dateiliste des jeweiligen Releases** herunterladen.
Sie verwendet das veröffentlichte Image mit genau dieser Release-Version als
Vorgabe; `IMAGE_TAG` kann sie überschreiben. Start ohne Klonen:

```bash
mkdir -p 3mf-katalog/config 3mf-katalog/models
cd 3mf-katalog
# Die heruntergeladene docker-compose.yml hier speichern.
# Optional: .env mit eigenen Parametern wie PUID, PGID, TZ oder MODELS_DIR anlegen.
docker compose up -d
```

Bei Verwendung der Dateien aus dem Repository gilt dagegen:

`.env.example` und der Compose-Fallback verwenden `latest`, das neueste stabile
Image (ab 0.16.0). `IMAGE_TAG=rc` brauchst du nur für Vorabversionen; eine
konkrete veröffentlichte Version ohne `v` hält dich dauerhaft auf diesem Stand.
`latest` folgt nur der höchsten veröffentlichten stabilen Version; `MAJOR.MINOR` folgt nur der höchsten stabilen Version seiner
Reihe. RC-Versionen veröffentlichen ihre konkrete Version und `rc`.

Im Verzeichnis `docker/`:

```bash
cp .env.example .env
mkdir -p config models
# .env bearbeiten: IMAGE_TAG, PUID/PGID nach `id -u` und `id -g`, TZ und MODELS_DIR setzen.
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

Für eigene Zugangsdaten gilt: `PASSWORD` darf keine Leerzeichen, Tabs,
Zeilenumbrüche oder die Zeichen `*`, `?`, `[`, `:`, `%`, `\` enthalten und nicht
mit `-` beginnen. `CUSTOM_USER` erlaubt ausschließlich `A-Z`, `a-z`, `0-9`,
`.`, `_`, `-`, ebenfalls ohne führendes `-`. Ungültige Werte brechen den
Containerstart mit einer Fehlermeldung ab, auch bei `ALLOW_NO_AUTH=true`.
Leere Werte verwenden weiterhin die Vorgaben; erzeugte Passwörter bleiben
48 Hex-Zeichen lang. Eine ungültige gespeicherte Passwortdatei bricht bei
Wiederverwendung den Start ab und bleibt unverändert. Zum Beheben die Datei löschen (neues Passwort wird erzeugt) oder `PASSWORD` setzen.
Die Einschränkungen verhindern, dass das Basisimage
Zugangsdaten beim Einrichten der Anmeldung verändert.

### Lokaler Bau

`APP_URL` (HTTPS, Linux x86_64 AppImage) und die dazugehörige `APP_VERSION` in
`.env` setzen. Beide Zeilen stehen nach dem Kopieren von `.env.example` schon leer
in der Datei: Trage den Wert **direkt hinter dem Gleichheitszeichen** ein und
wiederhole `APP_URL=` nicht noch einmal im Wert (richtig:
`APP_URL=https://github.com/…`, falsch: `APP_URL=APP_URL=https://github.com/…`).
Der Bau bricht sonst mit einer Fehlermeldung ab. Dann:

```bash
docker compose build
docker compose up -d --no-build
```

Es gibt bewusst keinen Standarddownload der alten, veränderlichen Preview aus
dem Prototyp. URL und Version müssen zusammenpassen. `BASE_IMAGE` lässt sich
auf einen Tag oder Digest festlegen; Eine ältere `.env` mit `BASE_IMAGE=…baseimage-kasmvnc…` überschreibt diese Vorgabe
und lässt den Bau scheitern: Entferne die Zeile oder setze den neuen Wert aus `.env.example`.
Vorgabe ist
`ghcr.io/linuxserver/baseimage-selkies:ubuntunoble` (LinuxServer-Basis mit Selkies,
X11/Openbox), im Dockerfile und in `docker-compose.yml` bereits per SHA256-Digest
festgelegt, damit ein späterer Bau nicht auf ein anderes Basisimage zeigt. Der
Release-Workflow pinnt dasselbe Basisimage per Digest und berechnet die SHA256-Prüfsumme
des Release-Downloads; der Dockerfile prüft sie beim erneuten Download über
`APP_SHA256`. Das ist ein Konsistenzcheck, keine unabhängige Herkunftsbestätigung.
Das Revision-Label nennt den ausgecheckten Quellcommit vom Standardbranch,
das Versionslabel die App-Version. Der Push erzeugt keine SBOM-/Provenance-Attestierungen.
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

**Umlaute in Dateinamen:** Zeigt `ls` auf dem Server einen Namen wie `K$'\303\266'rper.obj` statt `Körper.obj`, ist nur die Anzeige im Terminal betroffen, nicht der Dateiname (`\303\266` ist ein ö). Die Shell hat dann keine UTF-8-Einstellung. Ein Tester hat berichtet, dass `LANG=de_DE.UTF-8` in seiner Umgebung die Anzeige korrigiert hat (von mir nicht geprüft). Die Zeitzone stellst du wie oben mit `TZ` in der `.env` ein.

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

**Vom eigenen Rechner hochladen:** Öffne `/katalog/`, wähle unter „Dateien
hochladen“ Dateien aus oder ziehe sie auf die Ablagefläche. Sie landen im Ordner
**Eingang** (`/models/Eingang`). Danach in der App **Importieren → Ordner… → Eingang**
wählen. Die Begleitseite schreibt nie in die Katalogdatenbank; die App überwacht
den Ordner nicht automatisch. Upload braucht JavaScript und denselben Login wie
die Begleitseite. Fortschritt und Abbrechen sind für jede Datei verfügbar; der
Browser überträgt nacheinander, der Dienst erlaubt insgesamt zwei Uploads.

Voraussetzung: `/models` ist für den Dienstbenutzer `abc` (PUID/PGID aus Compose)
**schreibbar**. Eingang wird erst beim ersten Upload angelegt (0755, Dateien 0644).
Bei fehlenden Rechten zeigt die Seite einen Hinweis statt der Upload-Fläche:
Mount ohne `:ro` einbinden, Besitzer und Schreib-/Suchrechte des Zielordners bzw.
seines vorhandenen Elternordners auf dem Host passend zu PUID/PGID setzen. Ein
Dateipfad statt eines Ordners funktioniert ebenfalls nicht.

Optional: `KATALOG_UPLOAD_DIR` (Standard `/models/Eingang`) und
`KATALOG_UPLOAD_MAX_MB` (Standard **2048 MiB pro Datei**, positive ganze Zahl).
Dazu die Zeilen in `.env` **und** Compose einkommentieren. Leere Werte gelten als
nicht gesetzt. Das Ziel muss auch nach Auflösen von Symlinks unter einer erlaubten
Wurzel liegen (`KATALOG_ALLOWED_ROOTS`, sonst vorhandene `/models`, `/models2` usw.).
Erlaubt: `.3mf`, `.stl`, `.obj`, `.stp`, `.step`, `.zip`, `.7z`, `.rar`; Namen und
Anfangssignatur werden geprüft, Archive werden beim Upload nicht entpackt. Dies
ist keine vollständige Format- oder Virenprüfung. Bestehende Dateien bleiben
erhalten; gleiche Namen bekommen ` (2)` bis ` (100)`, danach einen Konfliktfehler.
Mindestens Dateigröße plus 256 MiB freier Platz muss verfügbar sein. Nach 60 Sekunden
ohne Körperdaten bricht der Dienst ab. Abgebrochene temporäre Dateien werden
entfernt; nach einem Dienstabsturz räumt der nächste Start Reste älter als eine
Stunde auf. Die HTTP-Körperlänge begrenzt den Upload; unmittelbar erkennbare
Zusatzbytes werden abgelehnt, die Verbindung wird anschließend geschlossen.

Der eingebaute nginx erlaubt **4 GiB**, die eigentliche Grenze setzt der Dienst;
`KATALOG_UPLOAD_MAX_MB` daher höchstens **4096** setzen. Ein davorliegender
**Reverse Proxy** braucht ebenfalls ein höheres Größenlimit und Streaming.
Ein Tester hat über seinen Nginx Proxy Manager zwei große Dateien **ohne Änderung**
hochgeladen. Lehnt dein Proxy große Uploads ab (Fehler „413“), im Reiter
**Advanced** `client_max_body_size 4g;` und `proxy_request_buffering off;` setzen.
Proxy-Zeitlimits müssen zur Übertragungsdauer passen (intern 3600 s).

**Bisheriger Weg über Ordner, Netzlaufwerk oder `scp`:** Drag & Drop direkt in den
gestreamten Desktop ist nicht vorgesehen: Die Dateiübertragung des Streams ist
abgeschaltet (`HARDEN_DESKTOP=true`). So kommen Dateien über den Server in die App:

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

### Download und Slicer

Der Seitenleisten-Bereich **„Download & Slicer“** bietet den Link zur Katalogliste, eine kurze Anleitung und die Voraussetzungen für lokale Slicer.

In der Detailansicht der Container-App zeigt **„Im Browser öffnen“** einen Link
zur ausgewählten Datei mit aufgeklapptem Slicer-Menü. Kopiere ihn in den Browser,
in dem du die App bedienst; dort kannst du die Datei herunterladen oder in deinem
lokalen Slicer öffnen. Die App im gestreamten Desktop öffnet selbst keinen Browser-Tab.
Nach dem ersten Besuch von `/katalog/` kennt die App die Adresse automatisch
(aus `Host` und `X-Forwarded-Proto`); weitere Besuche aktualisieren sie.
Bis dahin zeigt der Dialog den Pfad zum Anhängen an die Browser-Adresse.
Mit `KATALOG_PUBLIC_URL` lässt sich die Adresse fest setzen, z. B.
`KATALOG_PUBLIC_URL=https://katalog.example.de` hinter einem Reverse Proxy oder
`http://192.168.1.10:3000` im Heimnetz. Eingetragen wird die Adresse, die du im
Browser benutzt, nicht ein Platzhalter. Dazu die auskommentierten Zeilen in `.env`
**und** `docker-compose.yml` aktivieren; ist der Wert in `.env` leer, gilt die
Variable als nicht gesetzt.
Die Variable hat Vorrang. Erlaubt sind nur `http://` oder `https://` mit Host und
optionalem Port, ohne Pfad, Query, Fragment oder abschließenden Slash. Ungültige
Werte werden mit einem Log-Hinweis ignoriert.

Öffne **http://<Host>:3000/katalog/** in einem normalen Browser-Tab neben dem
Browser-Desktop. Die Begleitseite nutzt dieselbe Anmeldung und zeigt den Katalog
mit Vorschaubildern, Namenssuche, Typ-/Ordner-/Tag-Filtern, Sortierung und Seiten.
„Herunterladen“ speichert eine Datei auf deinem PC; „In Slicer öffnen“ übergibt
sie an einen dort installierten Slicer. Suche und Download funktionieren ohne
JavaScript; auf macOS passt JavaScript den Bambu-Link an. Die Seite spricht Deutsch,
Englisch, Spanisch und Französisch: Die Sprache kommt aus `Accept-Language` (sonst
Deutsch) und lässt sich in der Kopfleiste wählen (Parameter `?lang=`, funktioniert
ohne JavaScript); hell/dunkel nach Systemeinstellung.

**Dateien aus der Liste ausblenden:** „Aus Liste entfernen“ fragt nach und blendet die
Datei nur in dieser Begleitseite aus (Download und Slicer-Links liefern dann 404, auch
bereits ausgegebene). Katalog, Datenbank und Speicher bleiben unberührt; die
Begleitseite schreibt nie in den Katalog und löscht nie Dateien. Die Liste der
ausgeblendeten Dateien steht in `/config/.katalog-web-hidden.json` (Teil des
`/config`-Ordners, bleibt bei Updates erhalten). Wieder einblenden: unter den Filtern
„Ausgeblendete anzeigen (N)“ öffnen und bei der Datei „Wieder anzeigen“ wählen; direkt
nach dem Entfernen gibt es außerdem „Rückgängig“.

| Slicer | Dateitypen |
|---|---|
| OrcaSlicer | 3MF, STL, OBJ, STP/STEP |
| Bambu Studio | nur 3MF (eigener Link unter macOS); STL lässt sich über den Link nicht öffnen, von einem Tester bestätigt |
| Creality Print | 3MF, STL, OBJ, STP/STEP |
| ElegooSlicer | 3MF, STL, OBJ, STP/STEP |
| Cura | 3MF, STL, OBJ |
| Lychee | 3MF, STL, OBJ |
| SuperSlicer | 3MF, STL, OBJ, STP/STEP |

PrusaSlicer: Datei herunterladen und im Slicer öffnen. Der Slicer muss auf deinem
**PC** installiert sein und die Adresse des Containers vom PC erreichen können.
Unter Linux muss das jeweilige URL-Schema registriert sein. Selbstsignierte
Zertifikate bei `https://<Host>:3001` lehnen Slicer häufig ab: im Heimnetz
`http://<Host>:3000` oder einen Reverse Proxy mit gültigem Zertifikat verwenden.
Der Reverse Proxy muss `Host` (einschließlich öffentlichem Port) und
`X-Forwarded-Proto` durchreichen; in Nginx Proxy Manager „Websockets“ aktivieren.

Slicer-Links gelten **eine Stunde** und erlauben bis dahin den Download ohne
Anmeldung. Behandle sie wie Zugangsdaten; bei Fehlern die Seite neu laden.
Ausgeliefert werden ausschließlich nicht gelöschte Katalogdateien unter `/models`
oder `/models<Zahl>`; Symlinks nach außen und Pfade mit `..` sind gesperrt.
Nicht erreichbare oder nicht erlaubte Dateien bleiben ohne Knöpfe sichtbar.
Der Dienst liest die Datenbank nur; er läuft als `abc` und lauscht auf Loopback.
Sein Schlüssel liegt in `/config/.katalog-web-secret` (32 Zufallsbytes, Rechte 600).

Optionale Container-Umgebungsvariablen (in Compose unter `environment` ergänzen):
`CATALOG_DB` wählt einen festen Datenbankpfad, sonst die neueste
`/config/.local/share/*/catalog.db` nach Änderungszeit. `KATALOG_ALLOWED_ROOTS`
ersetzt die Modell-Wurzeln durch eine mit `:` getrennte Liste absoluter Pfade.
`KATALOG_WEB_PORT` setzt den internen Port (Standard 8099); der Start passt den
Nginx-Proxy mit an. Den internen Port nicht am Host veröffentlichen.

`src-tauri/src/lib.rs` legt `catalog.db` unter `app.path().app_data_dir()` an;
`src-tauri/tauri.conf.json` nennt `com.thebexxs.mfkatalogmanager`. Andere
Build-Kennungen bekommen andere Datenordner. Der angenommene Linux-Pfad unter
`/config/.local/share` ist **nicht gegen das echte Image geprüft**.

Der Bau prüft die Nginx-Template-Anker und den s6-Dienstaufbau und bricht bei
fehlenden Ankern ab. Der Workflow führt `bash docker/tests/katalog-web.sh IMAGE`
vor dem Push gegen den Kandidaten aus. Ein frisches Test-Volume prüft auf beiden
Ports GET/POST ohne Login für `/katalog/`, `/katalog/upload` und `/katalog/remove`
(401), signierte Downloads ohne Login sowie den Dienstneustart. Das Skript erzeugt
Testdaten und ein zufälliges Passwort und räumt den Testcontainer wieder auf.
Lokale Unit-Tests öffnen keine Netzwerkverbindungen. ZIP-Sammeldownload
(Stufe B) ist noch nicht enthalten.

### Netzwerk und Grenzen

Die veröffentlichten Ports 3000/3001 sind nur an `127.0.0.1` auf dem Host gebunden.
Die internen Dienste (Selkies-Datenverbindung 8082 und Begleitseite 8099) lauschen
ausschließlich auf `127.0.0.1` im Container; der Zugriff erfolgt über nginx auf 3000/3001.
Für das Heimnetz die beiden
Host-Adressen in Compose auf die gewünschte LAN-IP ändern, z. B.
`192.168.1.10:3000:3000`, und die Anmeldung beibehalten. Nicht ungeschützt ins
Internet freigeben; externen Zugriff mit TLS und geeignetem Reverse Proxy planen.
Ausführliche Netzwerk-/Reverse-Proxy-Anleitungen folgen im D6-Paket im
[Projekt-Wiki](https://github.com/Bexxs75/3mf-katalog-manager/wiki).

- In der Desktop-App kein „In Slicer öffnen“ oder „Ordner öffnen“ auf dem Host; Slicer-Links gibt es auf der Begleitseite.
- Kein Drag & Drop vom Host-Desktop in die App.
- 3D-Vorschau per Software-Rendering, daher langsamer.
- Ein Benutzer gleichzeitig; nur x86_64, kein arm64.
- Updates über Images (`docker compose pull`, dann `docker compose up -d`). Die App
  blendet im Container Slicer, „Im Dateimanager anzeigen“, Ordner-/Link-Öffnen und
  ihren eigenen Updater aus (`THREEMF_CONTAINER=1`); Links erscheinen als Text mit „Link kopieren“.
- Browser-Clipboard, Tastatur, Dateidialoge und WebKit können von der Desktop-App abweichen.

### Docker im Proxmox-LXC-Container

Wer Docker in einem LXC-Container statt in einer VM betreibt, braucht laut einem Tester
(Proxmox, aus eigener Erfahrung) für den Container die Optionen `nesting=1` und
`keyctl=1`. Das ist ein Hinweis aus der Praxis eines Testers; wir haben den Betrieb im
LXC-Container selbst nicht geprüft.

### Was geprüft ist

- `bash scripts/check.sh` prüft ohne Docker die Skripte, Compose- und Dockerfile-Struktur sowie Passwort-, Dateisystem- und Supervisor-Logik.
- `bash scripts/ci-smoke.sh IMAGE` startet das fertige Image und prüft Anmeldung, erzeugtes und eigenes Passwort, Loopback-Bindung von 8082/8099, Neustart der App nach einem Absturz, sauberes Beenden und `ALLOW_NO_AUTH`. Der Release-Workflow führt ihn vor dem Veröffentlichen aus.
- `bash tests/upgrade.sh ALT NEU` prüft ein Update auf demselben Volume (Schema-Migration mit Sicherung vorher), `bash tests/permissions.sh IMAGE` fremde `PUID`/`PGID` und einen schreibgeschützten Modellordner.
- Von Hand im Container geprüft: Import aus dem Dateidialog, Ordnerübernahme, Archiv-Dialog, Katalog-Export und -Import, Druckeranbindung an einen Moonraker-Testserver, `no-new-privileges:true`.
- Noch nicht geprüft: 3D-Vorschau mit vielen Modellen, Wiederherstellung mit Papierkorbdateien, arm64, Betrieb im LXC-Container.

[Lizenzhinweise](LICENSES.md).

### Note on old prototype data / Hinweis zu alten Prototyp-Daten
If you reuse a `/config` folder from the earlier prototype, delete `config/.config/openbox/autostart` first; otherwise the base image starts a second app instance next to the supervised one. / Nutzt du einen `/config`-Ordner des früheren Prototyps weiter, lösche vorher `config/.config/openbox/autostart`. Sonst startet das Basisimage die App ein zweites Mal neben dem überwachten Dienst.
