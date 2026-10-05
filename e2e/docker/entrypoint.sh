#!/bin/sh
# Virtual display, window manager and private session bus for one test run.
# The bus matters: the app's single-instance lock lives on D-Bus, so every run
# needs its own. Without a window manager the webview gets a 0x0 viewport.
exec xvfb-run -a -s "-screen 0 1600x1000x24 -nolisten tcp" dbus-run-session -- sh -c 'openbox >/dev/null 2>&1 & sleep 1; exec "$@"' sh "$@"
