#!/usr/bin/env bash
# STEP preview check of the web image, run the way docker-compose.yml runs it: read-only root
# filesystem, no capabilities, no-new-privileges and Docker's own default seccomp profile.
#  1. The STEP worker's probe passes, has_step_preview is true, an uploaded and imported 10 mm
#     cube returns a mesh whose bounding box is 10 mm on every axis.
#  2. A file with a .step name but no STEP content is refused at upload; a damaged STEP file
#     gives an error code, the server keeps running and the next preview still works.
#  3. Again with a seccomp profile that answers Landlock with ENOSYS (a kernel without it): the
#     server logs that the STEP preview is off, reports it unavailable and refuses STEP geometry
#     with an error code instead of crashing.
# Usage: docker-web/scripts/step-smoke.sh IMAGE
# SMOKE_DEFAULT_USER=1 runs the container as the image's own user (no -u) on named volumes
# instead of host directories, which proves the user baked into the image can do all of the above.
# The empty catalog is made like in ci-smoke.sh (cargo example, or SMOKE_CATALOG_CMD, see there).
set -euo pipefail
image=${1:?usage: step-smoke.sh IMAGE}
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cube="$root/crates/katalog-core/tests/fixtures/wuerfel-10mm.step"
work=$(mktemp -d)
name="web-step-smoke-$$"
cleanup() {
    docker rm -f "$name" >/dev/null 2>&1 || :
    if [ -n "${SMOKE_DEFAULT_USER:-}" ]; then docker volume rm -f "$name-cfg" "$name-mod" >/dev/null 2>&1 || :; fi
    rm -rf "$work" 2>/dev/null || :
}
trap cleanup EXIT
fail() { printf 'FAIL: %s\n' "$*" >&2; docker logs "$name" 2>&1 | tail -20 >&2 || :; exit 1; }
ok() { printf 'OK: %s\n' "$*"; }
# json EXPR FILE: prints EXPR evaluated with v = the JSON in FILE
json() { python3 -c "import json,sys; v=json.load(open(sys.argv[1])); print($1)" "$2"; }

[ -s "$cube" ] || fail "fixture missing: $cube"
catalog_dir="$work/config/.local/share/com.thebexxs.mfkatalogmanager"
mkdir -p "$catalog_dir" "$work/models/uploads"
chmod 0755 "$work/models" "$work/models/uploads"
export CATALOG_PATH="$catalog_dir/catalog.db"
if [ -n "${SMOKE_CATALOG_CMD:-}" ]; then
    eval "$SMOKE_CATALOG_CMD"
else
    (cd "$root" && cargo run -q -p katalog-core --no-default-features --example make_smoke_catalog -- "$CATALOG_PATH")
fi
[ -s "$CATALOG_PATH" ] || fail 'catalog database was not created'
chmod -R u+rwX "$work/config"
if [ -n "${SMOKE_DEFAULT_USER:-}" ]; then
    docker volume create "$name-cfg" >/dev/null && docker volume create "$name-mod" >/dev/null
    docker run --rm --user 0 --entrypoint sh -v "$name-cfg":/config -v "$name-mod":/models \
        -v "$work/config":/src:ro "$image" \
        -c 'cp -a /src/. /config/ && mkdir -p /models/uploads && chown -R 65532:65532 /config /models' \
        || fail 'could not prepare the volumes'
    mounts=(-v "$name-cfg":/config -v "$name-mod":/models)
    user_opt=()
else
    mounts=(-v "$work/config":/config -v "$work/models":/models)
    user_opt=(-u "$(id -u):$(id -g)")
fi
password=$(od -An -N12 -tx1 /dev/urandom | tr -d ' \n')
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
base="http://127.0.0.1:$port"
jar="$work/cookies"

# $1: extra docker run options (word-split on purpose; paths contain no spaces).
start() {
    # shellcheck disable=SC2086
    docker run -d --name "$name" --read-only --tmpfs /tmp --cap-drop ALL \
        --security-opt no-new-privileges:true $1 "${user_opt[@]}" \
        -p "127.0.0.1:$port:8080" \
        "${mounts[@]}" \
        -e KATALOG_PASSWORD="$password" \
        -e KATALOG_ALLOWED_HOSTS="127.0.0.1:$port,localhost:$port" \
        -e KATALOG_WRITE=1 -e KATALOG_UPLOAD_DIR=/models/uploads \
        "$image" >/dev/null 2>&1 || fail 'container could not be started'
    local ready=
    for _ in $(seq 1 30); do
        [ "$(docker inspect -f '{{.State.Running}}' "$name")" = true ] || fail 'container exited during startup'
        if [ "$(curl -s -o /dev/null -w '%{http_code}' "$base/api/health" || :)" = 200 ]; then ready=1; break; fi
        sleep 1
    done
    [ -n "$ready" ] || fail 'server not ready within 30 s'
    [ "$(docker inspect -f '{{.HostConfig.ReadonlyRootfs}}' "$name")" = true ] || fail 'container is not read-only'
    [ "$(docker inspect -f '{{.HostConfig.CapDrop}}' "$name")" = '[ALL]' ] || fail 'capabilities were not dropped'
    printf '%s' "{\"password\":\"$password\"}" | curl -s -o "$work/login.json" -c "$jar" \
        -X POST -H 'Content-Type: application/json' --data-binary @- "$base/api/login" >/dev/null
    csrf=$(json 'v["csrfToken"]' "$work/login.json" 2>/dev/null || :)
    [ -n "$csrf" ] || fail 'login failed'
}
stop() {
    docker stop -t 10 "$name" >/dev/null
    case "$(docker inspect -f '{{.State.ExitCode}}' "$name")" in 0|143) ;; *) fail 'unclean exit';; esac
    docker rm "$name" >/dev/null
}
# Writes need the browser's Origin header (cross-site protection), so curl sends one.
# invoke COMMAND JSON OUTFILE -> prints the HTTP status
invoke() {
    curl -s -o "$3" -w '%{http_code}' -b "$jar" -X POST -H 'Content-Type: application/json' \
        -H "X-CSRF-Token: $csrf" -H "Origin: $base" -d "$2" "$base/api/invoke/$1"
}
# upload FILE NAME OUTFILE -> prints the HTTP status
upload() {
    curl -s -o "$3" -w '%{http_code}' -b "$jar" -X POST -H 'Content-Type: application/octet-stream' \
        -H "X-CSRF-Token: $csrf" -H "Origin: $base" -H "X-Upload-Name: $2" --data-binary @"$1" "$base/api/upload"
}
# import_upload FILE NAME -> prints the result JSON path; fails unless the job ends
import_upload() {
    local code upload_id job state
    code=$(upload "$1" "$2" "$work/up.json")
    [ "$code" = 200 ] || fail "upload of $2 answered $code: $(cat "$work/up.json")"
    upload_id=$(json 'v["uploadId"]' "$work/up.json")
    code=$(invoke start_upload_import "{\"uploadIds\":[\"$upload_id\"]}" "$work/job.json")
    [ "$code" = 200 ] || fail "start_upload_import answered $code: $(cat "$work/job.json")"
    job=$(json 'v["jobId"]' "$work/job.json")
    for _ in $(seq 1 120); do
        invoke get_import_result "{\"jobId\":\"$job\"}" "$work/result-$2.json" >/dev/null
        state=$(json 'v.get("state","")' "$work/result-$2.json")
        case "$state" in completed|failed|cancelled) echo "$work/result-$2.json"; return 0;; esac
        sleep 0.5
    done
    fail "import of $2 did not finish"
}
# Checks a geometry answer: binary mesh, every axis of the bounding box 10 mm.
check_cube() {
    python3 - "$1" <<'EOF'
import json, struct, sys
data = open(sys.argv[1], "rb").read()
n = struct.unpack_from("<I", data, 0)[0]
header = json.loads(data[4:4 + n])
pos = 4 + n
lo, hi, triangles = [1e30] * 3, [-1e30] * 3, 0
for m in header["meshes"]:
    v = m["vertexCount"]
    for i in range(v):
        p = struct.unpack_from("<3f", data, pos + 12 * i)
        lo = [min(a, b) for a, b in zip(lo, p)]
        hi = [max(a, b) for a, b in zip(hi, p)]
    pos += 12 * v + (12 * v if m["hasNormal"] else 0) + 4 * m["indexCount"]
    triangles += m["indexCount"] // 3
assert pos == len(data), f"length {len(data)} != {pos}"
size = [h - l for h, l in zip(hi, lo)]
assert triangles > 0 and all(abs(s - 10.0) < 1e-3 for s in size), (triangles, size)
print(f"{len(header['meshes'])} mesh(es), {triangles} triangles, bounding box {size[0]:.3f} x {size[1]:.3f} x {size[2]:.3f} mm")
EOF
}

# --- 1 + 2: default seccomp profile -------------------------------------------------------------
start ''
if docker inspect -f '{{json .HostConfig.SecurityOpt}}' "$name" | grep -q 'seccomp'; then
    fail 'a seccomp option is set; this run must use the default profile'
fi
logs=$(docker logs "$name" 2>&1)
if printf '%s\n' "$logs" | grep -q 'STEP-Vorschau deaktiviert'; then
    fail "STEP preview is off under the default profile: $(printf '%s\n' "$logs" | grep 'STEP-Vorschau')"
fi
ok 'server started read-only, without capabilities, default seccomp profile, no "STEP-Vorschau deaktiviert" in the log'

code=$(invoke has_step_preview '{}' "$work/has.json")
[ "$code" = 200 ] && [ "$(cat "$work/has.json")" = true ] || fail "has_step_preview answered $code $(cat "$work/has.json")"
ok 'has_step_preview is true'

result=$(import_upload "$cube" wuerfel-10mm.step)
[ "$(json 'v["files"][0]["state"]' "$result")" = imported ] || fail "cube not imported: $(cat "$result")"
cube_id=$(json 'v["files"][0]["fileId"]' "$result")
ok "STEP cube uploaded and imported (fileId $cube_id)"

code=$(invoke get_model_geometry "{\"fileId\":\"$cube_id\"}" "$work/geo.bin")
[ "$code" = 200 ] || fail "get_model_geometry answered $code: $(head -c 300 "$work/geo.bin")"
summary=$(check_cube "$work/geo.bin") || fail 'geometry of the cube is wrong'
ok "STEP preview of the cube: $summary"

printf 'solid not a step file\n' >"$work/kein-step.step"
code=$(upload "$work/kein-step.step" kein-step.step "$work/nostep.json")
case "$code" in 4??) ;; *) fail "upload of a non-STEP .step answered $code";; esac
ok "file without STEP content is refused at upload ($code $(cat "$work/nostep.json"))"

printf 'ISO-10303-21;\nHEADER;\nthis is not a STEP body\n' >"$work/kaputt.step"
result=$(import_upload "$work/kaputt.step" kaputt.step)
state=$(json 'v["files"][0]["state"]' "$result")
if [ "$state" = imported ]; then
    broken_id=$(json 'v["files"][0]["fileId"]' "$result")
    code=$(invoke get_model_geometry "{\"fileId\":\"$broken_id\"}" "$work/broken.json")
    [ "$code" != 200 ] || fail 'geometry of a damaged STEP file answered 200'
    message=$(json 'v.get("message")' "$work/broken.json") || fail "no JSON error for the damaged file: $(head -c 300 "$work/broken.json")"
    ok "damaged STEP file: imported without metadata, preview refused with $code $message"
else
    ok "damaged STEP file: import ended with file state '$state'"
fi
[ "$(docker inspect -f '{{.State.Running}}' "$name")" = true ] || fail 'server stopped after the damaged file'
code=$(invoke get_model_geometry "{\"fileId\":\"$cube_id\"}" "$work/geo2.bin")
[ "$code" = 200 ] && check_cube "$work/geo2.bin" >/dev/null || fail "preview after the damaged file answered $code"
ok 'server still running, the next STEP preview works'
stop

# --- 3: no Landlock -----------------------------------------------------------------------------
# Allow everything else; only the Landlock calls fail the way an old kernel answers.
cat >"$work/no-landlock.json" <<'EOF'
{"defaultAction":"SCMP_ACT_ALLOW","syscalls":[{"names":["landlock_create_ruleset","landlock_add_rule","landlock_restrict_self"],"action":"SCMP_ACT_ERRNO","errnoRet":38}]}
EOF
start "--security-opt seccomp=$work/no-landlock.json"
line=$(docker logs "$name" 2>&1 | grep 'STEP-Vorschau deaktiviert' || :)
printf '%s\n' "$line" | grep -q 'Landlock' || fail "no log line about missing Landlock (got: '$line')"
ok "without Landlock the log says: $line"
code=$(invoke has_step_preview '{}' "$work/has.json")
[ "$code" = 200 ] && [ "$(cat "$work/has.json")" = false ] || fail "has_step_preview answered $code $(cat "$work/has.json")"
ok 'has_step_preview is false'
code=$(invoke get_model_geometry "{\"fileId\":\"$cube_id\"}" "$work/off.json")
[ "$code" != 200 ] || fail 'STEP geometry answered 200 without Landlock'
message=$(json 'v.get("message")' "$work/off.json") || fail "no JSON error without Landlock: $(head -c 300 "$work/off.json")"
[ "$(docker inspect -f '{{.State.Running}}' "$name")" = true ] || fail 'server stopped'
ok "STEP geometry refused with $code $message, server still running"
stop
echo 'Web image STEP smoke test passed'
