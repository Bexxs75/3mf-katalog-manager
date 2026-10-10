#!/usr/bin/env bash
# Permission test: foreign PUID/PGID on a fresh Docker volume, and a read-only /models mount.
# Usage: docker/tests/permissions.sh IMAGE
set -euo pipefail
image=${1:?usage: permissions.sh IMAGE}
vol=perm-test-config-$$
name=perm-test
models=$(mktemp -d)
cleanup() {
    docker rm -f "$name" >/dev/null 2>&1 || :
    docker volume rm -f "$vol" >/dev/null 2>&1 || :
    rm -rf "$models" 2>/dev/null || :
}
trap cleanup EXIT
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
ok() { printf 'OK: %s\n' "$*"; }
uid=4242 gid=4343
docker volume create "$vol" >/dev/null
docker run -d --name "$name" -p 127.0.0.1::3001 -v "$vol":/config -v "$models":/models:ro \
    -e PUID=$uid -e PGID=$gid --shm-size=1g "$image" >/dev/null
for _ in $(seq 1 90); do
    [ "$(docker inspect -f '{{.State.Health.Status}}' "$name")" = healthy ] && break
    sleep 3
done
[ "$(docker inspect -f '{{.State.Health.Status}}' "$name")" = healthy ] || { docker logs "$name" 2>&1 | tail -15 >&2; fail "container did not become healthy"; }
sleep 5
who=$(docker exec "$name" sh -c 'ps -o uid=,gid= -p "$(pgrep -f "[m]f-katalog-manager" | head -1)"' | tr -s ' ' | sed 's/^ //')
[ "$who" = "$uid $gid" ] || fail "the app runs as '$who', expected '$uid $gid'"
ok "the app runs as $uid:$gid"
foreign=$(docker exec -u 0 "$name" sh -c "find /config -mindepth 1 -maxdepth 5 \\( ! -uid $uid \\) | head -5")
[ -z "$foreign" ] || fail "files in /config belong to someone else: $foreign"
ok "everything in /config belongs to $uid"
db=$(docker exec -u 0 "$name" sh -c 'find /config/.local/share -name catalog.db | head -1')
[ -n "$db" ] || fail "no catalog was created"
mode=$(docker exec -u 0 "$name" stat -c %a "$db")
[ "$mode" = 600 ] || fail "catalog.db has mode $mode, expected 600"
ok "catalog.db is private (600)"
if docker exec -u 0 "$name" touch /models/probe >/dev/null 2>&1; then fail "/models is not read-only"; fi
ok "/models is read-only and the app still runs"
[ "$(docker inspect -f '{{.State.Running}}' "$name")" = true ] || fail "the container stopped"
printf 'Permission test passed.\n'
