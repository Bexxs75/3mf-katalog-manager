#!/usr/bin/env bash
#
# Removes the ten display stack libraries that Tauri's default AppImage bundling
# (via linuxdeploy) needlessly bundles and that crash on hosts with a newer Mesa:
#
#   Could not create default EGL display: EGL_BAD_PARAMETER. Aborting...
#
# Cause: linuxdeploy's built-in exclude list already covers libEGL/libGL/libdrm/
# libc, but not libwayland-*/libxkbcommon/libxcb-*/libXau/libXdmcp. The outdated
# copy bundled in the AppImage (from the build runner, here ubuntu-24.04) is
# loaded via LD_LIBRARY_PATH BEFORE the target machine's system copy; WebKitGTK's
# EGL initialization then fails on the version mismatch with the newer Mesa,
# although the host libraries (Wayland 1.25+, xkbcommon 1.13+) would have been
# ABI-compatible long ago.
#
# Verified fix (see github.com/tauri-apps/tauri issue #15976, independently
# confirmed on a second, non-Tauri project with the same linuxdeploy bundling):
# remove exactly these ten libraries from the AppDir before `appimagetool` builds
# the final AppImage - the rest (glib, gstreamer, webkit2gtk itself) stays
# untouched. A Tauri config field `bundle.linux.appimage.excludeLibraries` is in
# progress (PR #15662) but not available in the Tauri version in use (2.11.4) -
# hence this post-processing step instead of a config option.
#
# Usage:
#   ./src-tauri/scripts/fix-appimage-egl-linux.sh <path-to-AppImage>
#
# Replaces the file in place (extract, remove libraries, rebuild with
# appimagetool, overwrite the original).

set -euo pipefail

APPIMAGE_PATH="${1:?Usage: $0 <pfad-zum-AppImage>}"

if [ ! -f "$APPIMAGE_PATH" ]; then
    echo "Error: '$APPIMAGE_PATH' ist keine Datei (erwartet wurde ein .AppImage)." >&2
    exit 1
fi

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

APPIMAGE_ABS="$(readlink -f "$APPIMAGE_PATH")"
chmod +x "$APPIMAGE_ABS"

cd "$WORKDIR"
"$APPIMAGE_ABS" --appimage-extract >/dev/null

# The ten libraries from the verified minimal fix (issue #15976) - deliberately
# no more and no less, to leave the rest of the bundle unchanged.
LIBS_TO_REMOVE=(
    libwayland-client.so.0
    libwayland-cursor.so.0
    libwayland-egl.so.1
    libwayland-server.so.0
    libxkbcommon.so.0
    libxcb-randr.so.0
    libxcb-render.so.0
    libxcb-shm.so.0
    libXau.so.6
    libXdmcp.so.6
)

for lib in "${LIBS_TO_REMOVE[@]}"; do
    if [ -f "squashfs-root/usr/lib/$lib" ]; then
        rm -f "squashfs-root/usr/lib/$lib"
        echo "Entfernt: usr/lib/$lib"
    fi
done

# Download appimagetool itself as an AppImage and run it without FUSE (GitHub
# Actions runners have no FUSE for AppImages; --appimage-extract-and-run works
# around that reliably).
wget -q "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage" -O appimagetool.AppImage
chmod +x appimagetool.AppImage

rm -f "$APPIMAGE_ABS"
ARCH=x86_64 ./appimagetool.AppImage --appimage-extract-and-run squashfs-root "$APPIMAGE_ABS"

echo "AppImage neu gebaut ohne die problematischen Display-Stack-Bibliotheken: $APPIMAGE_ABS"
