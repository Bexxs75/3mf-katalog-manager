# Security Policy

🇩🇪 **Deutsch:** [weiter unten](#sicherheitsrichtlinie-deutsch)

## Supported versions

Security fixes are made for the **latest release** only. Please update to the newest version from the [releases page](https://github.com/Bexxs75/3mf-katalog-manager/releases/latest) before reporting.

| Version | Security fixes |
|---|---|
| latest stable release | yes |
| older versions | no, please update |

## Reporting a vulnerability

**Please do not open a public issue, pull request or Discord post for security problems.**

Send an email to **info@3mfkatalog.de** with the subject `Security`. Please include:

- the app version (Settings → Info) and your operating system,
- what the problem is and what an attacker could do with it,
- steps to reproduce, ideally with a small example file or proof of concept,
- whether you want to be credited, and under which name.

## What happens next

- You get a first reply, usually within **7 days**.
- We confirm the problem, fix it, and publish a release with the fix.
- After the release we publish the details (for example in the changelog or a GitHub issue) and credit you if you wish.

Please give us a reasonable amount of time to release a fix before you disclose the problem publicly.

## Scope

The desktop app (this repository) and the website [3mfkatalog.de](https://3mfkatalog.de). The app works locally; it only goes online to check GitHub for a new version, to download it when you choose to update, and, if you switch it on, to read printers on your home network.

## Web server: residual risks

These apply to the optional `katalog-server` (browser access). They are known limits, not vulnerabilities to report:

- **Do not share a model root, the upload directory or the config folder with untrusted local writers.** Path checks guard against traversal and symlinks, but they cannot protect against another local user who may rename or replace directories in the same tree at the same time.
- **Initial password file.** Without a supplied initial password, the generated password is written to `web-initial-password.txt` next to the password file (mode 0600, owner only) and is not printed to the log; the start notice only names the path. Change it at first sign-in: the file is then removed. If the file cannot be written, setup stops instead of printing the password. A copy you made of the file or of the password, and backups of the config folder taken before the change, keep the old password.
- **Download and slicer hand-off path.** The path of a download or of a file handed to a slicer is checked by resolving it (`canonicalize`), testing that it lies inside an allowed root and comparing device and inode of the opened file with the checked one. Unlike newer write paths (upload, file management, source folders), it does not open directory by directory with directory handles, so a directory component replaced by a local writer between the check and the open is not detected. Only trusted local users may write to the roots (see above). This limit is accepted on purpose and is not planned to change without a concrete exploit.
- **Removing catalog entries without the password.** Removing catalog entries needs the password for a folder that holds entries, for a disk comparison that removes 50 or more entries and for a single "remove from catalog" call of 50 or more models. Below 50 per call it does not; what a signed-in user can remove that way is limited by the budget of 200 entries per 10 minutes (and the daily limit of file actions), so a folder can be emptied in steps of at most 49 without the password, but not faster. A folder larger than the daily limit is refused outright (a deliberate hard limit).
- **Hard links.** Path containment cannot identify an in-root hard link to a file outside the allowed roots. Only trusted local users may write to the roots.
- **Cookies are not separated by port.** The session cookie (`katalog_session`) is also sent to other HTTP services on the same host name. Use a dedicated host or domain name for the catalog.
- **Older kernels without mount IDs.** File management (`KATALOG_FILE_OPS=1`) detects mount-point crossings via the mount ID. Where the kernel does not provide it, only the device ID is compared, so a bind mount of the same device inside a root is not detected.
- **Do not expose the server directly to the internet.** It is built for a home network, a VPN (for example Tailscale) or an HTTPS reverse proxy that shields it. It has no HTTPS of its own, so without one the password and the session cookie travel unencrypted. Do not forward its port on your router and do not publish it through a public tunnel (for example Tailscale Funnel).
- **Login lock behind a proxy.** The lock after failed logins counts per client address. Behind a reverse proxy that connects from a single address, all users share one counter: someone who can reach the login page can keep the owner locked out for a minute at a time (this exposes no data). Set `KATALOG_TRUST_PROXY=1` only if the proxy shields the server and sets `X-Forwarded-For`; it also requires `KATALOG_TRUSTED_PROXIES` (IP/CIDR of the proxies; without a valid list the server does not start). Forwarded headers are believed only from those direct peers, the chain is read from the right and the first non-proxy address is the client, so a forged header from any other peer or a forged left-hand entry is ignored.

---

## Sicherheitsrichtlinie (Deutsch)

### Unterstützte Versionen

Sicherheitskorrekturen gibt es nur für die **neueste Version**. Bitte aktualisiere vor einer Meldung auf die neueste Version von der [Release-Seite](https://github.com/Bexxs75/3mf-katalog-manager/releases/latest).

| Version | Sicherheitskorrekturen |
|---|---|
| neueste stabile Version | ja |
| ältere Versionen | nein, bitte aktualisieren |

### Sicherheitslücke melden

**Bitte melde Sicherheitsprobleme nicht öffentlich**, also nicht als Issue, Pull Request oder Discord-Beitrag.

Schick eine E-Mail an **info@3mfkatalog.de** mit dem Betreff `Security`. Hilfreich sind:

- die App-Version (Einstellungen → Info) und dein Betriebssystem,
- worin das Problem besteht und was ein Angreifer damit tun könnte,
- Schritte zum Nachstellen, am besten mit einer kleinen Beispieldatei oder einem Proof of Concept,
- ob und unter welchem Namen du genannt werden möchtest.

### Wie es weitergeht

- Du bekommst eine erste Antwort, in der Regel innerhalb von **7 Tagen**.
- Wir prüfen das Problem, beheben es und veröffentlichen eine Version mit der Korrektur.
- Nach dem Release veröffentlichen wir die Details (zum Beispiel im Changelog oder in einem GitHub-Issue) und nennen dich auf Wunsch.

Bitte gib uns angemessen Zeit für eine Korrektur, bevor du das Problem öffentlich machst.

### Geltungsbereich

Die Desktop-App (dieses Repository) und die Webseite [3mfkatalog.de](https://3mfkatalog.de). Die App arbeitet lokal; online geht sie nur, um auf GitHub nach einer neuen Version zu sehen, um sie herunterzuladen, wenn du aktualisierst, und, wenn du es einschaltest, um Drucker im Heimnetz auszulesen.

### Webserver: Restrisiken

Diese gelten für den optionalen `katalog-server` (Zugriff per Browser). Es sind bekannte Grenzen, keine zu meldenden Sicherheitslücken:

- **Modellwurzeln, Upload-Ordner und Konfigurationsordner nicht mit nicht vertrauenswürdigen lokalen Schreibern teilen.** Pfadprüfungen schützen vor Pfad-Tricks und Symlinks, nicht aber vor einem anderen lokalen Benutzer, der im selben Baum gleichzeitig Verzeichnisse umbenennen oder ersetzen darf.
- **Erstpasswort im Protokoll.** Ohne vorgegebenes Erstpasswort bleibt das erzeugte Passwort auch nach einer Änderung im Server-/Containerprotokoll. Nach der ersten Anmeldung sofort ändern; Protokollzugriff und Aufbewahrung beschränken. Eine Passwortänderung löscht alte Protokollkopien nicht.
- **Hardlinks.** Die Pfadprüfung erkennt nicht, dass ein Hardlink innerhalb der erlaubten Wurzeln auf eine Datei außerhalb verweist. Nur vertrauenswürdige lokale Benutzer dürfen die Wurzeln beschreiben.
- **Cookies sind nicht nach Port getrennt.** Das Sitzungscookie (`katalog_session`) geht auch an andere HTTP-Dienste auf demselben Hostnamen. Für den Katalog einen eigenen Host- oder Domainnamen verwenden.
- **Ältere Kernel ohne Mount-ID.** Die Dateiverwaltung (`KATALOG_FILE_OPS=1`) erkennt Mountpunkt-Übergänge über die Mount-ID. Liefert der Kernel sie nicht, wird nur die Geräte-ID verglichen; ein Bind-Mount desselben Geräts innerhalb einer Wurzel wird dann nicht erkannt.
- **Den Server nicht direkt ins Internet stellen.** Er ist für ein Heimnetz, ein VPN (zum Beispiel Tailscale) oder einen abschirmenden HTTPS-Reverse-Proxy gedacht. Er hat kein eigenes HTTPS; ohne eines laufen Passwort und Sitzungs-Cookie unverschlüsselt. Seinen Port nicht im Router weiterleiten und nicht über einen öffentlichen Tunnel (zum Beispiel Tailscale Funnel) veröffentlichen.
- **Anmeldesperre hinter einem Proxy.** Die Sperre nach Fehlversuchen zählt je Client-Adresse. Hinter einem Reverse-Proxy, der sich von einer einzigen Adresse verbindet, teilen sich alle Nutzer einen Zähler: Wer die Anmeldeseite erreicht, kann den Besitzer jeweils eine Minute lang aussperren (es werden keine Daten preisgegeben). `KATALOG_TRUST_PROXY=1` nur setzen, wenn der Proxy den Server abschirmt und `X-Forwarded-For` setzt; zusätzlich ist `KATALOG_TRUSTED_PROXIES` Pflicht (IP/CIDR der Proxys; ohne gültige Liste startet der Server nicht). Forwarded-Header zählen nur von diesen direkten Peers, die Kette wird von rechts gelesen und die erste Nicht-Proxy-Adresse ist der Client; ein gefälschter Header anderer Peers oder ein gefälschter linker Eintrag wird ignoriert.
