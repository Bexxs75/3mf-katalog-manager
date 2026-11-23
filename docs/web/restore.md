# Restore a catalog on the web server

POST /api/backup/restore accepts JSON containing "token" (from the checked
backup upload) and "password" (confirmed again in this request). It requires
KATALOG_WRITE=1, an authenticated session, the same-origin/CSRF checks and
a token owned by that session. Backup settings do not change server configuration.

Restore closes writer admission and lets existing jobs cancel cooperatively.
Readers remain available while writers drain, file-operation recovery finishes,
the incoming copy is prepared by the core, and the old catalog is backed up.
Each barrier wait has a 30-second deadline; on timeout admission reopens and
the catalog is not replaced. Unresolved filesystem intents and pending uploads
reject the operation. Discard pending uploads explicitly before retrying;
imported upload records are retired with the old catalog, without deleting files.

Automatic backups are stored in `<db-dir>/backups/restore-<id>.db` and .zip.
The ZIP uses the normal catalog export format and is checked with the existing
upload validation before exchange. If it cannot be restored (including the
existing 256 MiB database limit), restoreBackupInvalid rejects the replacement.
The identifier starts with the hexadecimal Unix timestamp and includes random
bytes. The previous catalog also remains as `<db-dir>/.bak-<id>`. These backups
have no automatic retention policy. Backups contain catalog data, not model files.

The short exclusive window closes readers, closes the original writer, renames
the databases, initializes empty filesystem/upload journals, and revokes all
sessions. No validation, migration or export runs in this window.

A successful response is {"restored":true,"restart":true}. The server requests
a graceful transport shutdown: it stops accepting connections and drains the
response before the main process returns. A disconnected requester does not
cancel a restore already started. The supplied Compose file uses
restart: unless-stopped. **Without a restart policy, start the server manually
after restoring.** Clients must log in again after restart. A failure after the
durable replacement intent also leaves admission closed and requests restart.

web-restore.json is stored at the reserved path beside the authentication file.
Startup resolves it under the catalog instance lock, before opening the database
or recovering filesystem/upload journals:

| Durable phase | Recovery |
| --- | --- |
| Prepared | Keep the old catalog, or copy .bak back if the first rename already happened. |
| OldMoved | Copy .bak back, even if the new database rename already happened. Use the automatic database snapshot if .bak is unavailable. |
| Installed | Keep the new catalog and finish initializing both empty journals. |

The journal is removed only after the directory changes are synced.
Recovery retains .bak and the automatic backup so it can itself be interrupted.
Malformed or inaccessible recovery state stops startup with restoreRecoveryFailed;
it must not be bypassed by creating an empty catalog.

# Katalog auf dem Webserver einspielen

POST /api/backup/restore erwartet JSON mit "token" aus der geprüften
Sicherungsdatei und dem erneut bestätigten "password". Schreibmodus, gültige
Sitzung, Origin-/CSRF-Prüfung und ein Token derselben Sitzung sind erforderlich.
Einstellungen aus der Sicherung verändern die Serverkonfiguration nicht.

Während des Drainierens bleiben Leser zugelassen. Laufende Jobs brechen
kooperativ ab. Jede Barrier-Wartephase hat 30 Sekunden Zeit; bei Überschreitung
öffnet die Zulassung wieder, ohne den Katalog auszutauschen. Ungeklärte
Dateioperationen und offene Uploads verhindern das Einspielen. Offene Uploads
müssen ausdrücklich verworfen werden. Bereits importierte Dateien bleiben erhalten.

Vor dem Austausch entstehen eine Datenbanksicherung und eine erneut einspielbare
ZIP-Datei unter `<db-dir>/backups/restore-<id>`. Der alte Katalog bleibt zusätzlich
als `<db-dir>/.bak-<id>` erhalten. Es gibt keine automatische Aufbewahrungsfrist.
Modelldateien sind nicht Teil dieser Sicherungen. Die Rückfallsicherung durchläuft
vor dem Austausch dieselbe Prüfung wie ein Upload. Verletzt sie die bestehenden
Grenzen (insbesondere 256 MiB Datenbankgröße) oder Pfadregeln, verhindert
restoreBackupInvalid den Austausch.

Nach erfolgreichem Austausch widerruft der Server alle Sitzungen und beendet sich
kontrolliert, nachdem laufende Antworten abgeschlossen wurden. Compose verwendet
restart: unless-stopped. **Ohne Restart-Policy muss der Server nach dem
Einspielen manuell gestartet werden.** Anschließend ist eine neue Anmeldung nötig.
Ein abgebrochener Browserrequest beendet den bereits gestarteten Austausch nicht.

Das Journal web-restore.json liegt neben der Authentifizierungsdatei. Beim Start
wird es unter der Instanzsperre vor dem Öffnen der Datenbank aufgelöst: Prepared
und OldMoved führen zum alten Katalog zurück, Installed stellt den neuen
Katalog mit leeren Datei- und Upload-Journalen fertig. Auch ein unterbrochener
Recovery-Lauf kann wiederholt werden. Ungültiger Recovery-Zustand verhindert den
Start mit dem Code restoreRecoveryFailed.
