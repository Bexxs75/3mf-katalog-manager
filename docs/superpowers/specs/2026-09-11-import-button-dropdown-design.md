# Import-Button: Direkt Dropdown statt Split-Button

**Datum:** 2026-09-11
**Status:** Genehmigt, bereit für Implementierungsplan

## Problem

Der Import-Button in der Kopfzeile ist aktuell ein Split-Button: der große
"+ Importieren"-Teil links löst sofort den Dateien-Dialog aus
(`onImportFiles`), ein kleiner, unbeschrifteter ▾-Button rechts öffnet das
eigentliche Dropdown-Menü mit "Dateien...", "Ordner..." und ggf. "aus Cloud
importieren...". Die Ordner-Import-Option (die bereits rekursiv durch
Unterordner geht, siehe `collect_supported_files` in `commands.rs:558-569`)
ist dadurch schwer auffindbar — genau das löste das ursprüngliche
Nutzer-Feedback aus, obwohl die Funktion selbst bereits vorhanden und
funktionsfähig ist.

## Lösung

Der Split-Button wird zu einem einzigen Button zusammengeführt. Klick auf
"+ Importieren" öffnet immer das Dropdown-Menü — keine Sofort-Aktion mehr.
Menüinhalt (Reihenfolge: Dateien.../Ordner.../ggf. aus Cloud
importieren...), Positionierung und Schließverhalten (Menüpunkt klicken
schließt es, erneuter Klick auf den Button togglet es) bleiben exakt wie
heute.

## Betroffene Datei

Ausschließlich `src/components/Header.tsx`. Die zwei nebeneinanderliegenden
`<button>`-Elemente (aktuell: `rounded-l-[3px]` mit `onImportFiles`-Klick
und `rounded-r-[3px]` mit `setImportMenuOpen`-Toggle) werden zu einem
`<button>` mit `rounded-[3px]`, dessen `onClick` nur noch
`setImportMenuOpen((o) => !o)` aufruft und der sowohl das "+"-Icon, das
Label als auch den ▾-Pfeil enthält. Das Dropdown selbst (`importMenuOpen &&
(...)`-Block) bleibt unverändert. `onImportFiles` wird dadurch nur noch aus
dem Dropdown-Menüpunkt "Dateien..." heraus aufgerufen, nicht mehr direkt
vom Hauptbutton.

## Nicht-Ziele

- Keine Änderung an `import_files`/`import_folder`/`import_from_cloud`
  selbst (Backend-Logik bleibt unangetastet).
- Keine Änderung an Reihenfolge oder Beschriftung der Menüpunkte.
- Kein Klick-außerhalb-schließt-Verhalten — das existiert für kein anderes
  Dropdown in der App (Einstellungen-Panel, Slicer-Menü) und wird hier
  konsistent nicht eingeführt.

## Verifikation

Kein Frontend-Test-Framework vorhanden. Verifikation: `npm run build`
(TypeScript) plus manuelle Prüfung im laufenden `npm run tauri dev`
(Button klicken → Menü öffnet sich direkt, Menüpunkt "Dateien..." öffnet
weiterhin den Datei-Dialog, "Ordner..." weiterhin den Ordner-Dialog).
