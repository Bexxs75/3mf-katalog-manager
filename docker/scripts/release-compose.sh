#!/usr/bin/env bash
set -euo pipefail
if [[ $# != 1 || ! $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9]+)?$ ]]; then
    printf 'Usage: %s <version: X.Y.Z or X.Y.Z-N>\n' "$0" >&2
    exit 1
fi
source_file="$(dirname -- "${BASH_SOURCE[0]}")/../docker-compose.yml"
python3 - "$source_file" "$1" <<'PY'
from pathlib import Path
import re
import sys

lines = Path(sys.argv[1]).read_text(encoding="utf-8").splitlines(keepends=True)
builds = [i for i, line in enumerate(lines) if re.match(r"^ +build:\s*(?:#.*)?$", line)]
images = [i for i, line in enumerate(lines) if re.match(r"^ +image:", line)]
if len(builds) != 1 or len(images) != 1:
    sys.exit("Expected exactly one build block and one image entry")
start = builds[0]
indent = len(lines[start]) - len(lines[start].lstrip(" "))
image_indent = len(lines[images[0]]) - len(lines[images[0]].lstrip(" "))
end = start + 1
while end < len(lines):
    line = lines[end]
    if line.strip() and len(line) - len(line.lstrip(" ")) <= indent:
        break
    end += 1
if end == start + 1 or image_indent != indent or start <= images[0] < end:
    sys.exit("Unrecognized build/image structure")
# Preserve the source text so operational comments and settings survive unchanged.
lines[images[0]] = " " * image_indent + (
    "image: ghcr.io/bexxs75/3mf-katalog-manager:${IMAGE_TAG:-" + sys.argv[2] + "}\n"
)
del lines[start:end]
header = """# Fertige Release-Compose-Datei; noch möglicherweise instabil.
# Start: docker compose up -d — kein Klonen erforderlich.
# Anleitung: https://github.com/Bexxs75/3mf-katalog-manager/blob/master/docker/README.md
# Ready-to-use release Compose file; possibly still unstable.
# Start: docker compose up -d — no clone required.
# Guide: https://github.com/Bexxs75/3mf-katalog-manager/blob/master/docker/README.md

"""
sys.stdout.write(header + "".join(lines))
PY
