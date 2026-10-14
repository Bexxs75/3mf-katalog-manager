#!/usr/bin/env bash
# Update test: an older image creates the catalog, the newer image takes over the same /config.
# Checks that data survives, the schema is migrated, a copy is taken before the migration
# and the password stays the same.
# Usage: docker/tests/upgrade.sh OLD_IMAGE NEW_IMAGE
set -euo pipefail
old=${1:?usage: upgrade.sh OLD_IMAGE NEW_IMAGE}
new=${2:?usage: upgrade.sh OLD_IMAGE NEW_IMAGE}
work=$(mktemp -d)
name=upgrade-test
cleanup() {
    docker rm -f "$name" >/dev/null 2>&1 || :
    docker run --rm -v "$work":/w --entrypoint sh "$new" -c 'rm -rf /w/* /w/.[!.]* 2>/dev/null' >/dev/null 2>&1 || :
    rm -rf "$work" 2>/dev/null || :
}
trap cleanup EXIT
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
ok() { printf 'OK: %s\n' "$*"; }
run_once() { # image
    docker rm -f "$name" >/dev/null 2>&1 || :
    docker run -d --name "$name" -p 127.0.0.1::3001 -v "$work":/config \
        -e PUID="$(id -u)" -e PGID="$(id -g)" --shm-size=1g "$1" >/dev/null
    for _ in $(seq 1 90); do
        [ "$(docker inspect -f '{{.State.Health.Status}}' "$name")" = healthy ] && break
        sleep 3
    done
    [ "$(docker inspect -f '{{.State.Health.Status}}' "$name")" = healthy ] || { docker logs "$name" 2>&1 | tail -15 >&2; fail "$1 did not become healthy"; }
    sleep 5
    docker stop "$name" >/dev/null
    [ "$(docker inspect -f '{{.State.ExitCode}}' "$name")" = 0 ] || fail "$1 did not stop cleanly"
}
db() { find "$work/.local/share" -name catalog.db 2>/dev/null | head -1; }
sql() { python3 - "$(db)" "$1" <<'PY'
import sqlite3, sys
con = sqlite3.connect(sys.argv[1])
rows = con.execute(sys.argv[2]).fetchall()
con.commit()
print(rows[0][0] if rows and len(rows[0]) == 1 else rows)
PY
}

run_once "$old"
[ -n "$(db)" ] || fail "the old image created no catalog"
old_version=$(sql 'PRAGMA user_version')
pw_before=$(cat "$work/.kasmvnc-password")
sql "INSERT INTO printers (name) VALUES ('Upgrade-Test-Drucker')" >/dev/null
ok "old image ran (schema $old_version) and a sentinel printer was added"

run_once "$new"
new_version=$(sql 'PRAGMA user_version')
[ "$new_version" -ge "$old_version" ] || fail "schema went backwards ($old_version to $new_version)"
[ "$(sql "SELECT count(*) FROM printers WHERE name = 'Upgrade-Test-Drucker'")" = 1 ] || fail "the printer is gone after the update"
[ "$(sql 'PRAGMA integrity_check')" = ok ] || fail "the catalog fails the integrity check"
[ "$(cat "$work/.kasmvnc-password")" = "$pw_before" ] || fail "the password changed"
ok "new image took over (schema $old_version to $new_version), data and password intact"
if [ "$new_version" -gt "$old_version" ]; then
    backups=$(find "$work/.local/share" -path '*update-backups*' -type f | wc -l)
    [ "$backups" -ge 1 ] || fail "no copy was taken before the migration"
    ok "copy before the migration exists ($backups file)"
else
    printf 'NOTE: both images use schema %s, no migration copy expected.\n' "$old_version"
fi
printf 'Update test passed.\n'
