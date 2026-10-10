#!/usr/bin/env bash
# Starts a built image the way a user would and checks what must hold for every release:
# login, generated and supplied passwords, one supervised app, restart, clean stop.
# Usage: docker/scripts/ci-smoke.sh IMAGE
set -euo pipefail
image=${1:?usage: ci-smoke.sh IMAGE}
work=$(mktemp -d)
names=()
cleanup() {
    for n in "${names[@]:-}"; do [ -n "$n" ] && docker rm -f "$n" >/dev/null 2>&1 || :; done
    docker run --rm -v "$work":/w --entrypoint sh "$image" -c 'rm -rf /w/* /w/.[!.]* 2>/dev/null' >/dev/null 2>&1 || :
    rm -rf "$work" 2>/dev/null || :
}
trap cleanup EXIT
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
ok() { printf 'OK: %s\n' "$*"; }

start() { # name config-dir [docker run args...]
    local name=$1 dir=$2; shift 2
    names+=("$name")
    docker run -d --name "$name" -p 127.0.0.1::3001 -v "$dir":/config \
        -e PUID="$(id -u)" -e PGID="$(id -g)" --shm-size=1g "$@" "$image" >/dev/null
}
port_of() { docker port "$1" 3001/tcp | head -1 | sed 's/.*://'; }
wait_healthy() {
    for _ in $(seq 1 90); do
        [ "$(docker inspect -f '{{.State.Health.Status}}' "$1")" = healthy ] && return 0
        sleep 3
    done
    docker logs "$1" 2>&1 | tail -20 >&2
    fail "$1 did not become healthy within 270 s"
}
status() { curl -sk -o /dev/null -w '%{http_code}' "$@"; }

# 1. Generated password: shown once, kept in /config, reused after a restart.
mkdir -p "$work/gen"
start smoke-gen "$work/gen"
wait_healthy smoke-gen
p=$(port_of smoke-gen)
[ "$(status "https://127.0.0.1:$p/")" = 401 ] || fail "login is not required"
pw=$(cat "$work/gen/.kasmvnc-password")
[ "${#pw}" -ge 16 ] || fail "generated password is too short"
[ "$(status -u "abc:$pw" "https://127.0.0.1:$p/")" = 200 ] || fail "generated password is rejected"
[ "$(status -u "abc:wrong" "https://127.0.0.1:$p/")" = 401 ] || fail "a wrong password is accepted"
logs=$(docker logs smoke-gen 2>&1)
grep -q -- "$pw" <<<"$logs" || fail "first start did not show the password"
[ "$(stat -c %a "$work/gen/.kasmvnc-password")" = 600 ] || fail "password file is not 600"
env_json=$(docker inspect smoke-gen --format '{{json .Config.Env}}')
if grep -q -- "$pw" <<<"$env_json"; then fail "password leaked into the container environment"; fi
ok "generated password"

# 2. One supervised app that comes back after a crash.
count() { docker exec smoke-gen sh -c 'pgrep -fc "[m]f-katalog-manager" || true'; }
[ "$(count)" -ge 1 ] || fail "the app is not running"
docker exec -u 0 smoke-gen sh -c 'pkill -KILL -f "[m]f-katalog-manager"' || :
for _ in $(seq 1 40); do [ "$(count)" -ge 1 ] && break; sleep 2; done
[ "$(count)" -ge 1 ] || fail "the app did not restart after a crash"
ok "supervised restart"

# 3. Clean stop and restart keep the password and print it again never.
lines_before=$(docker logs smoke-gen 2>&1 | wc -l)
start_ts=$(date +%s)
docker stop smoke-gen >/dev/null
[ $(( $(date +%s) - start_ts )) -le 20 ] || fail "docker stop took longer than 20 s"
[ "$(docker inspect -f '{{.State.ExitCode}}' smoke-gen)" = 0 ] || fail "stop did not exit with 0"
docker start smoke-gen >/dev/null
wait_healthy smoke-gen
logs=$(docker logs smoke-gen 2>&1 | tail -n +$((lines_before + 1)))
if grep -q -- "$pw" <<<"$logs"; then fail "password was printed again after a restart"; fi
p=$(port_of smoke-gen)
[ "$(status -u "abc:$pw" "https://127.0.0.1:$p/")" = 200 ] || fail "password changed after a restart"
ok "clean stop and restart"

# 4. A supplied password wins and is not logged; no-auth needs the explicit switch.
mkdir -p "$work/own"
own="ci-own-$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
start smoke-own "$work/own" -e PASSWORD="$own"
wait_healthy smoke-own
p=$(port_of smoke-own)
[ "$(status -u "abc:$own" "https://127.0.0.1:$p/")" = 200 ] || fail "supplied password is rejected"
logs=$(docker logs smoke-own 2>&1)
if grep -q -- "$own" <<<"$logs"; then fail "supplied password was logged"; fi
ok "supplied password"
mkdir -p "$work/open"
start smoke-open "$work/open" -e ALLOW_NO_AUTH=true
wait_healthy smoke-open
p=$(port_of smoke-open)
[ "$(status "https://127.0.0.1:$p/")" = 200 ] || fail "ALLOW_NO_AUTH did not open the login"
logs=$(docker logs smoke-open 2>&1)
grep -q "Authentication disabled" <<<"$logs" || fail "no warning for ALLOW_NO_AUTH"
ok "explicit no-auth with warning"
printf 'All image smoke checks passed.\n'
