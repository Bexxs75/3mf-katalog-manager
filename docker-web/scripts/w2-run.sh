#!/usr/bin/env bash
# Repeatable browser test of W2 "Bereinigung und Abgleich im Web" against the real web image (spec 7.5, Forgejo #39).
# Run it on a Linux host with Docker, python3 and the Playwright image (the `pw` wrapper's image); not part of CI.
# Everything is throw-away: invented 3MF files, a fresh empty catalog, a random password, the port only on 127.0.0.1.
#
#   IMAGE=3mfkatalog-web:local  PW_TOOLS=/path/to/playwright-tools  CATALOG_TEMPLATE=/path/to/config-with-empty-catalog \
#   OUT=/path/to/screenshots  bash docker-web/scripts/w2-run.sh
#
# PW_TOOLS: folder with node_modules/{playwright,@axe-core/playwright} (the `pw` wrapper mounts the same one).
# CATALOG_TEMPLATE: a /config tree holding .local/share/com.thebexxs.mfkatalogmanager/catalog.db, created with
#   cargo run -p katalog-core --no-default-features --example make_smoke_catalog -- <path>   (see ci-smoke.sh).
# Optional: PORT (18090), KEEP_IMAGE=1 (do not remove IMAGE at the end), WORK (scratch dir, removed at the end).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
IMAGE=${IMAGE:?IMAGE}; PW_TOOLS=${PW_TOOLS:?PW_TOOLS}; CATALOG_TEMPLATE=${CATALOG_TEMPLATE:?CATALOG_TEMPLATE}; OUT=${OUT:?OUT}
PORT=${PORT:-18090}
WORK=${WORK:-$(mktemp -d)}
NAME=w2-browser-test
PW_IMAGE=mcr.microsoft.com/playwright:v1.49.0-jammy
PASSWORD=$(od -An -N12 -tx1 /dev/urandom | tr -d ' \n')
DB="$WORK/config/.local/share/com.thebexxs.mfkatalogmanager/catalog.db"
mkdir -p "$OUT" "$WORK/models"
cp -r "$CATALOG_TEMPLATE" "$WORK/config"; chmod -R u+rwX "$WORK/config"
checks=0; failed=0

cleanup() {
    docker rm -f "$NAME" >/dev/null 2>&1 || :
    rm -rf "$WORK" 2>/dev/null || :
    [ -n "${KEEP_IMAGE:-}" ] || docker rmi "$IMAGE" >/dev/null 2>&1 || :
}
trap cleanup EXIT

start_container() {
    docker rm -f "$NAME" >/dev/null 2>&1 || :
    docker run -d --name "$NAME" --read-only --tmpfs /tmp --cap-drop ALL --security-opt no-new-privileges:true \
        -u "$(id -u):$(id -g)" -p "127.0.0.1:$PORT:8080" \
        -v "$WORK/config":/config -v "$WORK/models":/data \
        -e KATALOG_WRITE=1 -e KATALOG_FILE_OPS=1 -e KATALOG_ROOTS=/data/alpha:/data/beta \
        -e KATALOG_PASSWORD="$PASSWORD" -e KATALOG_ALLOWED_HOSTS="127.0.0.1:$PORT,localhost:$PORT" \
        "$IMAGE" >/dev/null || { echo 'container could not be started' >&2; exit 1; }
    for _ in $(seq 1 40); do
        [ "$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/api/health")" = 200 ] && return 0
        sleep 1
    done
    docker logs "$NAME" 2>&1 | tail -20 >&2; echo 'server not ready' >&2; exit 1
}
phase() { # name  -> runs w2-browser-test.mjs inside the Playwright image on the host network
    docker run --rm --init --network host --user "$(id -u):$(id -g)" -e HOME=/tmp -e PLAYWRIGHT_BROWSERS_PATH=/ms-playwright \
        -e NODE_PATH=/tools/node_modules -v "$PW_TOOLS":/tools -v "$here":/scripts:ro -v "$OUT":/out -w /scripts \
        "$PW_IMAGE" node w2-browser-test.mjs "$1" "http://127.0.0.1:$PORT" "$PASSWORD" /out 2>&1 | tee "$OUT/log-$1.txt" | grep -v '^OK  ' || :
    tail -1 "$OUT/log-$1.txt"
}
db() { python3 - "$DB" "$1" <<'PY'
import sqlite3, sys
con = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
print(con.execute(sys.argv[2]).fetchone()[0])
PY
}
expect() { # label actual expected
    checks=$((checks + 1))
    if [ "$2" = "$3" ]; then printf 'OK   DB %s = %s\n' "$1" "$2"; else printf 'FAIL DB %s = %s (expected %s)\n' "$1" "$2" "$3"; failed=$((failed + 1)); fi
}
active="SELECT count(*) FROM files WHERE deleted_at IS NULL"
trashed="SELECT count(*) FROM files WHERE deleted_at IS NOT NULL"
named() { echo "SELECT count(*) FROM files WHERE path LIKE '%$1%' AND deleted_at IS ${2:-NULL}"; }

# 1. base state: 30 + 6 models on disk, the catalog takes them over through the dialog's own commands
bash "$here/w2-fixture.sh" base "$WORK/models"
start_container
phase seed
expect 'active models after seeding (36 files, duplicates are taken over)' "$(db "$active")" 36
expect 'nothing in the trash after seeding' "$(db "$trashed")" 0

# 2. disk changes behind the catalog's back, then the whole dialog run in the browser
bash "$here/w2-fixture.sh" mutate "$WORK/models"
bash "$here/w2-fixture.sh" bulk "$WORK/models"
phase ui
expect 'Ordner-weg: 4 models in the trash' "$(db "$(named Ordner-weg 'NOT NULL')")" 4
expect 'Archiv-alt: 3 models in the trash' "$(db "$(named Archiv-alt 'NOT NULL')")" 3
expect 'trashed in total (4 + 3 folder rows + 2 single rows)' "$(db "$trashed")" 9
expect 'all 67 rows still in the table (36 seeded + 31 taken over; nothing hard-deleted)' "$(db "SELECT count(*) FROM files")" 67
expect 'active models after the dialogs (67 - 9 in the trash)' "$(db "$active")" 58
expect 'date rename: 2 files in beta carry the date prefix on disk' "$(find "$WORK/models/beta" -name '20??-??-??_??-??_Beta-[23].3mf' | wc -l | tr -d ' ')" 2
expect 'date rename: the catalog knows the new names' "$(db "SELECT count(*) FROM files WHERE path GLOB '/data/beta/20??-??-??_??-??_Beta-[23].3mf'")" 2
expect 'unclear models untouched (Sub, Verlinkt)' "$(db "SELECT count(*) FROM files WHERE (path LIKE '%Sub/%' OR path LIKE '%Verlinkt/%') AND deleted_at IS NULL")" 2
expect 'beta/Beta-1 still active (start check must not remove)' "$(db "$(named Beta-1)")" 1

# 3. restart with "check at start" + "take over new files" saved by the UI; 3 new files appear meanwhile
bash "$here/w2-fixture.sh" late "$WORK/models"
before=$(db "$active")
start_container
phase startup
expect 'start check took over exactly the 3 late files' "$(( $(db "$active") - before ))" 3
expect 'active models after the start check (58 + 3)' "$(db "$active")" 61
expect 'start check removed nothing: trash unchanged' "$(db "$trashed")" 9
expect 'beta/Beta-1 still active after the start check' "$(db "$(named Beta-1)")" 1

# 4. empty root with the server's own .katalog-papierkorb (P5e): must count as unreachable, not as content
docker stop "$NAME" >/dev/null
find "$WORK/models/beta" -mindepth 1 -maxdepth 1 ! -name '.katalog-papierkorb' -exec rm -rf {} +
mkdir -p "$WORK/models/beta/.katalog-papierkorb"
ls -a "$WORK/models/beta" | tr '\n' ' ' | sed 's/^/beta before the restart: /'; echo
active_before=$(db "$active")
start_container
phase emptyroot
expect 'empty root: nothing taken over or removed (active unchanged)' "$(db "$active")" "$active_before"
expect 'empty root: trash unchanged' "$(db "$trashed")" 9

echo "DB checks: $((checks - failed))/$checks ok"
[ "$failed" -eq 0 ]
