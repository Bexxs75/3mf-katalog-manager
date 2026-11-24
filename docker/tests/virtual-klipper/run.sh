#!/usr/bin/env bash
# Web printer link against a virtual Klipper/Moonraker printer (Forgejo #42, C6). Test machine only (ObiWan), never the home network.
# Starts two throw-away printers on a private Docker network, runs the ignored Rust tests from tests/virtual_klipper.rs
# against them and removes everything again. Needs Docker and a Rust toolchain; run it from the repository root:
#   NAME=w2c bash docker/tests/virtual-klipper/run.sh
# NAME (default vk) prefixes every container, image and network, so the clean-up touches nothing else.
set -uo pipefail
NAME=${NAME:-vk}; SUBNET=${SUBNET:-172.29.252.0/24}; BASE=${SUBNET%.0/24}
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
KEY=$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')
cleanup() {
    docker rm -f "$NAME-printer" "$NAME-redirect" >/dev/null 2>&1 || :
    docker network rm "$NAME-net" >/dev/null 2>&1 || :
    docker rmi "$NAME-image" >/dev/null 2>&1 || :
}
trap cleanup EXIT
docker build -q -t "$NAME-image" "$here" >/dev/null || { echo 'image build failed'; exit 1; }
docker network create --subnet "$SUBNET" "$NAME-net" >/dev/null || { echo 'network failed'; exit 1; }
docker run -d --name "$NAME-printer" --network "$NAME-net" --ip "$BASE.10" --read-only --cap-drop ALL \
    -e PRINTER_API_KEY="$KEY" "$NAME-image" >/dev/null || exit 1
docker run -d --name "$NAME-redirect" --network "$NAME-net" --ip "$BASE.11" --read-only --cap-drop ALL \
    -e PRINTER_MODE=redirect "$NAME-image" >/dev/null || exit 1
for _ in $(seq 1 20); do
    curl -s -o /dev/null "http://$BASE.10:7125/_sim/ping" && break; sleep 0.5
done
VK_PRINTER="$BASE.10:7125" VK_REDIRECT="$BASE.11:7125" VK_KEY="$KEY" VK_ALLOW="$SUBNET" \
    cargo test -p katalog-server --test virtual_klipper -- --ignored --test-threads=1
code=$?
echo "virtual klipper test exit code: $code"
exit $code
