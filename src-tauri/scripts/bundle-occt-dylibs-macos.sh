#!/usr/bin/env bash
#
# Relocates the OCCT (and transitive Homebrew) .dylibs that
# `cargo build --features step-preview` linked against their absolute
# Homebrew install path into the .app bundle itself, so the resulting
# .app/.dmg runs on a machine without Homebrew OCCT installed.
#
# Unlike Windows (which just needs the DLLs copied next to the .exe -
# see stage-occt-dlls.ps1), macOS bakes the *absolute* link path into
# the binary at compile time. `dylibbundler` copies every (transitive)
# non-system dependency into Contents/Frameworks and rewrites the
# binary's and each copied dylib's load commands to
# @executable_path/../Frameworks/<name> via install_name_tool.
#
# Run this AFTER `npm run tauri build -- --bundles app` (the .app must
# already exist) and BEFORE packaging a .dmg - re-running
# `npm run tauri build -- --bundles app` afterwards would recreate the
# .app from the cargo-built binary and wipe this step's changes, so
# package the .dmg directly from the already-fixed .app (see
# vendor/opencascade-sys/VENDORING.md for the `hdiutil` invocation used
# instead of a second `tauri build` call).
#
# Usage:
#   ./src-tauri/scripts/bundle-occt-dylibs-macos.sh <path-to-.app>
#
# Verified (2026-09-21, Intel docker-osx Sonoma VM): app launches and
# renders a STEP file's 3D preview with the real Homebrew OCCT install
# renamed out of the way (i.e. genuinely self-contained), not just
# incidentally still resolving against a local Homebrew install.

set -euo pipefail

APP_PATH="${1:?Usage: $0 <path-to-.app>}"

if [ ! -d "$APP_PATH" ]; then
    echo "Error: '$APP_PATH' is not a directory (expected a .app bundle)." >&2
    exit 1
fi

if ! command -v dylibbundler >/dev/null 2>&1; then
    echo "Error: dylibbundler not found. Install it via 'brew install dylibbundler'." >&2
    exit 1
fi

BINARY="$(find "$APP_PATH/Contents/MacOS" -maxdepth 1 -type f -perm -u+x | head -n1)"
if [ -z "$BINARY" ]; then
    echo "Error: no executable found under '$APP_PATH/Contents/MacOS'." >&2
    exit 1
fi

FRAMEWORKS_DIR="$APP_PATH/Contents/Frameworks"
mkdir -p "$FRAMEWORKS_DIR"

dylibbundler -od -b \
    -x "$BINARY" \
    -d "$FRAMEWORKS_DIR/" \
    -p "@executable_path/../Frameworks/"

count=$(find "$FRAMEWORKS_DIR" -name '*.dylib' | wc -l | tr -d ' ')
echo "Bundled $count dylibs into '$FRAMEWORKS_DIR'."
