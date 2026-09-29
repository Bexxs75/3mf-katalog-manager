#!/usr/bin/env bash
#
# Finds LC_RPATH entries that occur more than once in the same Mach-O file
# of a .app bundle (main binary + Contents/Frameworks) and either removes
# the duplicates or, with --check, fails.
#
# dyld on newer macOS versions (seen on Sequoia 15.7) refuses to load a
# library that carries the same LC_RPATH twice and reports it as
# "Library missing ... (duplicate LC_RPATH '...')", although the file is
# present. dylibbundler adds its -p path to libtbb/libtbbmalloc twice, so
# the STEP build did not start there.
#
# Usage:
#   ./src-tauri/scripts/dedupe-rpaths-macos.sh <path-to-.app>          # fix
#   ./src-tauri/scripts/dedupe-rpaths-macos.sh --check <path-to-.app>  # verify
#
# OTOOL, LIPO and INSTALL_NAME_TOOL may point to the llvm-* variants to run
# this outside macOS.

set -euo pipefail

CHECK=0
if [ "${1:-}" = "--check" ]; then
    CHECK=1
    shift
fi
APP_PATH="${1:?Usage: $0 [--check] <path-to-.app>}"

OTOOL="${OTOOL:-otool}"
LIPO="${LIPO:-lipo}"
INSTALL_NAME_TOOL="${INSTALL_NAME_TOOL:-install_name_tool}"

if [ ! -d "$APP_PATH" ]; then
    echo "Error: '$APP_PATH' is not a directory (expected a .app bundle)." >&2
    exit 1
fi

# Prints every LC_RPATH path of one architecture, one per line.
rpaths() {
    "$OTOOL" -arch "$2" -l "$1" | awk '
        $1 == "cmd" { in_rpath = ($2 == "LC_RPATH") }
        in_rpath && $1 == "path" { print $2 }'
}

problems=0
while IFS= read -r -d '' file; do
    archs="$("$LIPO" -archs "$file" 2>/dev/null)" || continue
    for arch in $archs; do
        dups="$(rpaths "$file" "$arch" | sort | uniq -d)"
        [ -z "$dups" ] && continue
        while IFS= read -r rp; do
            if [ "$CHECK" -eq 1 ]; then
                echo "Duplicate LC_RPATH '$rp' in $file ($arch)" >&2
                problems=$((problems + 1))
                continue
            fi
            # install_name_tool removes one entry per call (and applies to
            # every slice of a fat file): delete until none is left, then
            # add the path back exactly once.
            while rpaths "$file" "$arch" | grep -qxF -- "$rp"; do
                "$INSTALL_NAME_TOOL" -delete_rpath "$rp" "$file"
            done
            "$INSTALL_NAME_TOOL" -add_rpath "$rp" "$file"
            echo "Removed duplicate LC_RPATH '$rp' from $(basename "$file")."
        done <<< "$dups"
    done
done < <(find "$APP_PATH/Contents/MacOS" "$APP_PATH/Contents/Frameworks" -type f -print0 2>/dev/null)

if [ "$problems" -gt 0 ]; then
    echo "Error: $problems duplicate LC_RPATH entries - macOS refuses to load such libraries." >&2
    exit 1
fi
