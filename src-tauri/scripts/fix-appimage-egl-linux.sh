#!/usr/bin/env bash
#
# Entfernt die zehn Display-Stack-Bibliotheken, die Tauris Standard-
# AppImage-Bundling (ueber linuxdeploy) unnoetig mitbuendelt und die auf
# Hosts mit neuerem Mesa zum Absturz fuehren:
#
#   Could not create default EGL display: EGL_BAD_PARAMETER. Aborting...
#
# Ursache: linuxdeploy's eingebaute Ausschlussliste deckt libEGL/libGL/
# libdrm/libc bereits ab, nicht aber libwayland-*/libxkbcommon/libxcb-*/
# libXau/libXdmcp. Die im AppImage mitgelieferte, veraltete Kopie (vom
# Build-Runner, hier ubuntu-24.04) wird per LD_LIBRARY_PATH VOR der
# System-Kopie des Zielrechners geladen; WebKitGTKs EGL-Initialisierung
# scheitert dann an der Versions-Inkompatibilitaet mit dem neueren Mesa
# des Zielrechners, obwohl die Host-Bibliotheken (Wayland 1.25+,
# xkbcommon 1.13+) laengst ABI-kompatibel gewesen waeren.
#
# Verifizierter Fix (siehe github.com/tauri-apps/tauri Issue #15976,
# unabhaengig bestaetigt an einem zweiten, nicht-Tauri-Projekt mit
# demselben linuxdeploy-Bundling): genau diese zehn Bibliotheken aus dem
# AppDir entfernen, bevor `appimagetool` das finale AppImage baut - der
# Rest (glib, gstreamer, webkit2gtk selbst) bleibt unangetastet. Ein
# Tauri-eigenes `bundle.linux.appimage.excludeLibraries`-Config-Feld ist
# in Arbeit (PR #15662), aber in der aktuell verwendeten Tauri-Version
# (2.11.4) noch nicht verfuegbar - deshalb dieser Nachbearbeitungsschritt
# statt einer Config-Option.
#
# Usage:
#   ./src-tauri/scripts/fix-appimage-egl-linux.sh <pfad-zum-AppImage>
#
# Ersetzt die Datei an Ort und Stelle (extrahieren, Bibliotheken
# entfernen, mit appimagetool neu bauen, Original ueberschreiben).

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

# Die zehn Bibliotheken aus dem verifizierten Minimal-Fix (Issue #15976) -
# absichtlich nicht mehr und nicht weniger, um den Rest des Bundles
# unveraendert zu lassen.
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

# appimagetool selbst als AppImage herunterladen und ohne FUSE ausfuehren
# (GitHub-Actions-Runner haben kein FUSE fuer AppImages im Container-Sinn,
# --appimage-extract-and-run umgeht das zuverlaessig).
wget -q "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage" -O appimagetool.AppImage
chmod +x appimagetool.AppImage

rm -f "$APPIMAGE_ABS"
ARCH=x86_64 ./appimagetool.AppImage --appimage-extract-and-run squashfs-root "$APPIMAGE_ABS"

echo "AppImage neu gebaut ohne die problematischen Display-Stack-Bibliotheken: $APPIMAGE_ABS"
