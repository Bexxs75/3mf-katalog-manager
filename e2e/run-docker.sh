#!/usr/bin/env bash
# Runs the E2E suite in the Ubuntu 24.04 image against an extracted AppImage
# (or any app directory). Usage:
#   e2e/run-docker.sh [--app <dir containing AppRun>] [-- wdio args, e.g. --spec specs/01-start.e2e.ts]
# Default app directory: $MFK_E2E_APPDIR or /mnt/claude/e2e-work/squashfs-root.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(dirname "$here")"
appdir="${MFK_E2E_APPDIR:-/mnt/claude/e2e-work/squashfs-root}"
image=mfk-e2e-ubuntu2404
if [[ "${1:-}" == "--app" ]]; then appdir="$2"; shift 2; fi
[[ "${1:-}" == "--" ]] && shift
docker image inspect "$image" >/dev/null 2>&1 || docker build -t "$image" "$here/docker"
name="mfk-e2e-run-$$"
# --init: xvfb-run waits for a signal that PID 1 would swallow.
exec docker run --rm --init --name "$name" --user "$(id -u):$(id -g)" \
  -e HOME=/tmp/runner-home -e MFK_E2E_APP=/app/AppRun -e MFK_E2E_WORK=/tmp \
  -v "$repo":/work -v "$appdir":/app:ro -w /work/e2e \
  "$image" /work/e2e/docker/entrypoint.sh npx wdio run wdio.conf.ts "$@"
