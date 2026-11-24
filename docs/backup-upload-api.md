# Backup upload preview API

`POST /api/backup/upload` accepts a raw ZIP body (maximum 258 MiB including ZIP
metadata). Send `x-restore-password` as standard base64 of the UTF-8 password in
this same request, plus the existing session cookie, `x-csrf-token` and `Origin`.
`KATALOG_WRITE` and a session without a required password change are mandatory.
The password uses the existing verification and throttling mechanism. Do not put
it in the URL. A retry creates a separate preview; this route is not idempotent.

The ZIP must contain exactly `catalog.db` (at most 256 MiB) and `settings.json`
(at most 1 MiB). Header inconsistencies, duplicate names, links, path segments
and extra entries are rejected. Actual decompression is bounded independently
of declared sizes. No settings are applied: the web server has no counterpart
for the portable `ui_settings::BACKUP_KEYS` preferences.

Success returns HTTP 200 with `{ "token": "…", "report": { … },
"settings_ignored": true }`. `report` is the core `RestoreReport`: original
`schema_version`, `migration_needed`, `model_count`, `folder_count`,
`collection_count`, `source_folder_count`, `path_warnings` (`PathWarning` objects) and `unreachable_source_folders`.
The existing core error envelope is returned with HTTP 400 for invalid backups,
including newer schemas. No new translation/error codes are introduced.
Source directory probes are limited to `KATALOG_SOURCE_ROOTS` and refuse links.
The trash policy accepts desktop `<db-dir>/trash` and each configured catalog
root’s `.katalog-papierkorb`. Preview holds a catalog reader guard and cannot run
inside an exclusive replacement window. It never changes the active database.

`DELETE /api/backup/staging/{token}` requires the creating session, CSRF, Origin
and write permission. It returns 204; unknown, expired or foreign-session tokens
return 404. Password confirmation is required on upload, not on discard.

Storage is `<db-dir>/restore-staging/<256-bit random hex token>/`, with directory
mode 0700 and file mode 0600. There is one upload/inspection slot and at most four
outstanding previews. Tokens expire after 30 minutes (checked on access and swept
every minute). Startup removes leftover staging files. Failed requests clean up
through ownership guards. The original ZIP and database remain staged; the
inspection/migration copy is removed. Nothing is restored or exchanged here.

For the future activation adapter, use
`app.restore.checked_catalog(token, authenticated_session_token)` and keep the
returned `Arc<CheckedCatalog>` alive until consumption finishes. Its `path()`
checks the staged file and directory identities. Expiry/discard revokes further
lookup, while an existing lease keeps its files alive. The path contains the
original inspected database, not the migrated temporary copy. Activation must
still recheck session authorization, use the barrier and call the core prepare
pipeline before replacing anything. No activation route is provided here.

After a restore, the new catalog has no `files.content_hash` values. The next server
start fills them in in the background (`hash_backfill`). Until that run is complete,
`start_upload_import`, `start_source_import` and `scan_source_folders` answer
HTTP 409 with the error code `catalogHashCheckPending`; other commands are not
affected, and a rename or move during the run is followed. A server without a writer
(read-only) does not run it.
