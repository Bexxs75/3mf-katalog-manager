#!/usr/bin/env bash
#
# Combines two separately built and dylib-bundled single-arch .app bundles
# (arm64 + x86_64, each prepared via bundle-occt-dylibs-macos.sh) into a real
# universal .app bundle: the main binary AND every OCCT/Homebrew .dylib in
# Contents/Frameworks are combined into a fat file via `lipo -create`.
#
# Needed because Homebrew ships OCCT single-architecture only (there is no
# universal-apple-darwin bottle) - so unlike the STEP-free build, the STEP build
# can't simply be built in one go with `tauri build --target universal-apple-darwin`.
# Instead: build once for aarch64-apple-darwin (native Homebrew OCCT under
# /opt/homebrew), once for x86_64-apple-darwin (Homebrew OCCT under /usr/local,
# separate Intel runner), bundle the dylibs both times, and only then merge them
# here into a real universal bundle.
#
# Usage:
#   ./src-tauri/scripts/merge-universal-step-macos.sh \
#       <arm64.app> <x86_64.app> <output.app>
#
# <arm64.app> serves as the template (Info.plist/Resources/icons are identical
# across architectures) - only the main binary and the Frameworks/*.dylib files
# are replaced.

set -euo pipefail

ARM64_APP="${1:?Usage: $0 <arm64.app> <x86_64.app> <output.app>}"
X64_APP="${2:?Usage: $0 <arm64.app> <x86_64.app> <output.app>}"
OUTPUT_APP="${3:?Usage: $0 <arm64.app> <x86_64.app> <output.app>}"

for app in "$ARM64_APP" "$X64_APP"; do
    if [ ! -d "$app" ]; then
        echo "Error: '$app' ist kein Verzeichnis (erwartet wurde ein .app-Bundle)." >&2
        exit 1
    fi
done

rm -rf "$OUTPUT_APP"
mkdir -p "$(dirname "$OUTPUT_APP")"
cp -R "$ARM64_APP" "$OUTPUT_APP"

ARM64_BIN="$(find "$ARM64_APP/Contents/MacOS" -maxdepth 1 -type f -perm -u+x | head -n1)"
X64_BIN="$(find "$X64_APP/Contents/MacOS" -maxdepth 1 -type f -perm -u+x | head -n1)"
OUT_BIN="$OUTPUT_APP/Contents/MacOS/$(basename "$ARM64_BIN")"

echo "Fuege Hauptbinary zu Universal zusammen: $(basename "$ARM64_BIN")"
lipo -create "$ARM64_BIN" "$X64_BIN" -output "$OUT_BIN"
lipo -info "$OUT_BIN"

ARM64_FRAMEWORKS="$ARM64_APP/Contents/Frameworks"
X64_FRAMEWORKS="$X64_APP/Contents/Frameworks"
OUT_FRAMEWORKS="$OUTPUT_APP/Contents/Frameworks"
mkdir -p "$OUT_FRAMEWORKS"

# Union of all dylib names from both sides - normally identical (the same
# OCCT/tbb transitive dependencies on both architectures), but handled
# defensively as a union.
ALL_DYLIBS="$(
    { find "$ARM64_FRAMEWORKS" -maxdepth 1 -name '*.dylib' -exec basename {} \; 2>/dev/null; \
      find "$X64_FRAMEWORKS" -maxdepth 1 -name '*.dylib' -exec basename {} \; 2>/dev/null; } \
    | sort -u
)"

merged_count=0
single_arch_count=0
for dylib in $ALL_DYLIBS; do
    arm64_path="$ARM64_FRAMEWORKS/$dylib"
    x64_path="$X64_FRAMEWORKS/$dylib"
    out_path="$OUT_FRAMEWORKS/$dylib"

    if [ -f "$arm64_path" ] && [ -f "$x64_path" ]; then
        lipo -create "$arm64_path" "$x64_path" -output "$out_path"
        merged_count=$((merged_count + 1))
    elif [ -f "$arm64_path" ]; then
        echo "WARNUNG: '$dylib' existiert nur in der arm64-Frameworks-Kopie - als Single-Arch uebernommen (fehlt der x86_64-Seite entsprechendes Gegenstueck)." >&2
        cp "$arm64_path" "$out_path"
        single_arch_count=$((single_arch_count + 1))
    else
        echo "WARNUNG: '$dylib' existiert nur in der x86_64-Frameworks-Kopie - als Single-Arch uebernommen (fehlt der arm64-Seite entsprechendes Gegenstueck)." >&2
        cp "$x64_path" "$out_path"
        single_arch_count=$((single_arch_count + 1))
    fi
done

echo "Universal-Bundle fertig: $merged_count Dylibs zu Fat-Dateien zusammengefuehrt, $single_arch_count nur einzelarchitektur-rein uebernommen (siehe Warnungen oben, falls > 0)."

if [ "$single_arch_count" -gt 0 ]; then
    echo "Achtung: ein Single-Arch-Dylib in einem Universal-Bundle kann auf der jeweils anderen Architektur zur Laufzeit fehlschlagen - Warnungen oben pruefen." >&2
fi
