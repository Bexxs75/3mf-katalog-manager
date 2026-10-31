#!/usr/bin/env bash
# Starts the slim web image the way a user would and checks what must hold for every build:
# web UI served, API closed without login, login works, a catalog read answers with JSON,
# read-only container filesystem, clean stop. Nothing is published.
# Usage: docker-web/scripts/ci-smoke.sh IMAGE
# The empty catalog is created with the cargo example make_smoke_catalog (Rust must be available).
# Override for machines without a local Rust toolchain: SMOKE_CATALOG_CMD is a shell command
# (evaluated with eval) that must create the empty catalog at the path in $CATALOG_PATH, e.g.
#   SMOKE_CATALOG_CMD='docker run --rm -u $(id -u):$(id -g) -e HOME=/tmp -v "$PWD":/w ... sh -c "cd /w && cargo run ... -- /data/catalog.db"'
set -euo pipefail
image=${1:?usage: ci-smoke.sh IMAGE}
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
work=$(mktemp -d)
name="web-smoke-$$"
cleanup() {
    docker rm -f "$name" >/dev/null 2>&1 || :
    rm -rf "$work" 2>/dev/null || :
}
trap cleanup EXIT
fail() { printf 'FAIL: %s\n' "$*" >&2; docker logs "$name" 2>&1 | tail -20 >&2 || :; exit 1; }
ok() { printf 'OK: %s\n' "$*"; }

# Catalog in the layout of the image: /config/.local/share/com.thebexxs.mfkatalogmanager/catalog.db
catalog_dir="$work/config/.local/share/com.thebexxs.mfkatalogmanager"
mkdir -p "$catalog_dir" "$work/models"
export CATALOG_PATH="$catalog_dir/catalog.db"
if [ -n "${SMOKE_CATALOG_CMD:-}" ]; then
    eval "$SMOKE_CATALOG_CMD"
else
    (cd "$root" && cargo run -q -p katalog-core --no-default-features --example make_smoke_catalog -- "$CATALOG_PATH")
fi
[ -s "$CATALOG_PATH" ] || fail 'catalog database was not created'
chmod -R u+rwX "$work/config"

# Random password, 24 hex characters (above the 12 character minimum, so no forced change).
# It is only passed through the environment and never printed.
password=$(od -An -N12 -tx1 /dev/urandom | tr -d ' \n')

# Free local port, chosen up front because the host list must contain it.
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
docker run -d --name "$name" --read-only --tmpfs /tmp --cap-drop ALL \
    --security-opt no-new-privileges:true -u "$(id -u):$(id -g)" \
    -p "127.0.0.1:$port:8080" \
    -v "$work/config":/config -v "$work/models":/models:ro \
    -e KATALOG_PASSWORD="$password" \
    -e KATALOG_ALLOWED_HOSTS="127.0.0.1:$port,localhost:$port" \
    "$image" >/dev/null 2>&1 || fail 'container could not be started'
base="http://127.0.0.1:$port"

ready=
for _ in $(seq 1 30); do
    [ "$(docker inspect -f '{{.State.Running}}' "$name")" = true ] || fail 'container exited during startup'
    if [ "$(curl -s -o /dev/null -w '%{http_code}' "$base/api/health" || :)" = 200 ]; then ready=1; break; fi
    sleep 1
done
[ -n "$ready" ] || fail 'server not ready within 30 s'
ok 'server is ready'

code=$(curl -s -o /dev/null -w '%{http_code}' "$base/")
[ "$code" = 200 ] || fail "web UI answered $code instead of 200"
ok 'web UI is served (200)'

code=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' -d '{}' "$base/api/invoke/list_tag_counts")
[ "$code" = 401 ] || fail "API without login answered $code instead of 401"
ok 'API without login is rejected (401)'

jar="$work/cookies"
login_body=$(printf '{"password":"%s"}' "$password")
code=$(printf '%s' "$login_body" | curl -s -o "$work/login.json" -w '%{http_code}' -c "$jar" \
    -X POST -H 'Content-Type: application/json' --data-binary @- "$base/api/login")
[ "$code" = 200 ] || fail "login answered $code instead of 200"
csrf=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["csrfToken"])' "$work/login.json")
[ -n "$csrf" ] || fail 'login returned no CSRF token'
ok 'login with the password works (200)'

code=$(curl -s -o "$work/tags.json" -w '%{http_code}' -b "$jar" \
    -X POST -H 'Content-Type: application/json' -H "X-CSRF-Token: $csrf" -d '{}' "$base/api/invoke/list_tag_counts")
[ "$code" = 200 ] || fail "list_tag_counts answered $code instead of 200"
python3 -c 'import json,sys; v=json.load(open(sys.argv[1])); assert isinstance(v,(list,dict)), v' "$work/tags.json" \
    || fail 'list_tag_counts returned no valid JSON'
ok 'catalog read list_tag_counts answers with valid JSON (200)'

[ "$(docker inspect -f '{{.HostConfig.ReadonlyRootfs}}' "$name")" = true ] || fail 'container is not read-only'
start=$(date +%s)
docker stop -t 10 "$name" >/dev/null
elapsed=$(( $(date +%s) - start ))
exit_code=$(docker inspect -f '{{.State.ExitCode}}' "$name")
[ "$elapsed" -lt 10 ] || fail "container needed ${elapsed} s to stop (killed?)"
case "$exit_code" in 0|143) ;; *) fail "unclean exit code $exit_code";; esac
ok "read-only container stopped cleanly (exit $exit_code, ${elapsed} s)"
echo 'Web image smoke test passed'
