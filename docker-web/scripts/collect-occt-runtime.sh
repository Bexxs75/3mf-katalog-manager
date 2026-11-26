#!/usr/bin/env bash
# Runs in the build stage of docker-web/Dockerfile. Finds out from `ldd` what the linked server
# binary loads at run time, so the slim run-time stage gets exactly that and no hand-kept list:
#   - shared libraries owned by Debian's libocct-* packages are copied (installing those packages
#     would pull in about 400 MB of LLVM, Mesa and OCCT extras the reader never loads),
#   - every other library is named by its Debian package, which the run-time stage installs with apt,
#   - the copyright files and exact versions of the copied OCCT packages are kept for the LGPL notice.
# Usage: collect-occt-runtime.sh BINARY OUTDIR
# OUTDIR receives lib/ (the copied .so files under their soname), apt-packages.txt,
# doc/ (copyright files, SOURCES.txt) and ldd.txt (what was read, for the build log).
set -euo pipefail
bin=${1:?usage: collect-occt-runtime.sh BINARY OUTDIR}
out=${2:?usage: collect-occt-runtime.sh BINARY OUTDIR}
mkdir -p "$out/lib" "$out/doc"

ldd "$bin" >"$out/ldd.txt"
if grep -q 'not found' "$out/ldd.txt"; then
    grep 'not found' "$out/ldd.txt" >&2
    echo 'collect-occt-runtime: unresolved libraries in the build stage' >&2
    exit 1
fi

# The dynamic loader may report /lib/... while dpkg records /usr/lib/... (merged /usr), so all
# spellings are tried; a library no package owns is an error, not something to guess about.
owner() {
    local path=$1 real candidate
    real=$(readlink -f "$path")
    for candidate in "$path" "$real" "/usr$path" "/usr$real"; do
        if pkg=$(dpkg-query -S "$candidate" 2>/dev/null | head -n1); then
            printf '%s\n' "${pkg%%:*}"
            return 0
        fi
    done
    return 1
}

: >"$out/apt-packages.txt"
: >"$out/occt-packages.txt"
copied=0
while read -r soname arrow path _; do
    # Lines without "=> /path" are the vDSO and the loader itself.
    [ "$arrow" = '=>' ] && [ "${path#/}" != "$path" ] || continue
    pkg=$(owner "$path") || { echo "collect-occt-runtime: no package owns $path" >&2; exit 1; }
    case "$pkg" in
        libocct-*)
            cp -L "$path" "$out/lib/$soname"
            copied=$((copied + 1))
            printf '%s\n' "$pkg" >>"$out/occt-packages.txt"
            ;;
        *) printf '%s\n' "$pkg" >>"$out/apt-packages.txt" ;;
    esac
done <"$out/ldd.txt"
[ "$copied" -gt 0 ] || { echo 'collect-occt-runtime: no OCCT library linked (built without step-preview?)' >&2; exit 1; }

sort -u -o "$out/apt-packages.txt" "$out/apt-packages.txt"
sort -u -o "$out/occt-packages.txt" "$out/occt-packages.txt"
{
    echo 'Open CASCADE Technology libraries in /usr/local/lib, copied unchanged from these Debian packages.'
    echo 'License: LGPL 2.1 with the Open CASCADE exception (see the *.copyright files next to this one).'
    echo 'Source: the Debian source package named below, e.g. https://sources.debian.org/src/opencascade/'
    echo 'or https://snapshot.debian.org/package/opencascade/ for that exact version.'
    echo
    while read -r pkg; do
        dpkg-query -W -f='${Package} ${Version} (source: ${source:Package} ${source:Version})\n' "$pkg"
        cp "/usr/share/doc/$pkg/copyright" "$out/doc/$pkg.copyright"
    done <"$out/occt-packages.txt"
} >"$out/doc/SOURCES.txt"

echo "collect-occt-runtime: $copied OCCT libraries copied, $(wc -l <"$out/apt-packages.txt") packages for apt:"
tr '\n' ' ' <"$out/apt-packages.txt"
echo
