# Automatische Slicer-Erkennung — Design

## Kontext

Aktuell muss der Nutzer jeden Slicer (Bambu Studio, OrcaSlicer, ...) manuell
über einen Datei-Dialog in den Einstellungen hinzufügen (`SlicerConfig` in
`src/types/index.ts`, verwaltet über `src/hooks/useSlicers.ts`,
persistiert in `localStorage`). Tester-Feedback: die App soll stattdessen
selbst im System nach installierten Slicern suchen und sie automatisch
einbinden.

## Ziel

Beim App-Start durchsucht die App bekannte Installationsorte für
Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer und UltiMaker Cura auf
Linux und Windows und ergänzt automatisch gefundene Slicer in die
bestehende Slicer-Liste. Das bestehende manuelle Hinzufügen bleibt
unverändert erhalten — für Custom-Forks/Herstellervarianten, die die
automatische Suche nicht kennt.

## Architektur

Neues Rust-Modul `src-tauri/src/slicers.rs`:

- Statische Liste `SlicerDefinition { display_name: &str, binary_names: &[&str] }`
  für die fünf bekannten Slicer.
- Funktion `pub fn detect_slicers() -> Vec<DetectedSlicer>` (mit
  `DetectedSlicer { name: String, path: String }`), die:
  1. Für jeden `binary_names`-Eintrag im System-`PATH` nach einer
     ausführbaren Datei sucht.
  2. Zusätzlich bekannte, plattformspezifische Installationsordner
     durchsucht (siehe unten).
  3. Ergebnisse nach absolutem Pfad dedupliziert zurückgibt.
- Jeder Scan-Schritt ist best-effort: ein nicht lesbarer/nicht
  existierender Ordner wird übersprungen, nie ein Fehler propagiert. Die
  Funktion selbst gibt keinen `Result` zurück — ein leeres `Vec` ist ein
  gültiges Ergebnis (z. B. kein Slicer installiert).

Neuer Tauri-Command in `src-tauri/src/commands.rs`:

```rust
#[tauri::command]
pub fn scan_installed_slicers() -> CmdResult<Vec<DetectedSlicer>> {
    Ok(slicers::detect_slicers())
}
```

(in `src-tauri/src/lib.rs` im `invoke_handler`-Makro registriert, wie alle
anderen Commands).

## Suchorte

**PATH** (alle Plattformen): für jeden `binary_names`-Eintrag wird jeder
Ordner in `$PATH`/`%PATH%` nach einer passenden ausführbaren Datei
durchsucht (unter Windows zusätzlich mit `.exe`-Endung versucht).

**Linux, feste Ordner:**
- `/usr/bin`, `/usr/local/bin`
- `/opt/<BinaryName>/` (z. B. `/opt/BambuStudio/`, `/opt/OrcaSlicer/`)
- Flatpak: `~/.local/share/flatpak/exports/bin`,
  `/var/lib/flatpak/exports/bin`
- Gängige AppImage-Ablageorte: `~/Applications`, `~/AppImages`,
  `~/.local/bin`

Binärnamen pro Slicer (Linux):
- Bambu Studio: `bambu-studio`, `BambuStudio`
- OrcaSlicer: `orca-slicer`, `OrcaSlicer`
- PrusaSlicer: `prusa-slicer`, `prusaslicer`
- SuperSlicer: `superslicer`
- UltiMaker Cura: `cura`, `UltiMaker-Cura`

**Windows, feste Ordner** (`C:\Program Files\` und
`C:\Program Files (x86)\`, beide Varianten geprüft):
- `Bambu Studio\bambu-studio.exe`
- `OrcaSlicer\orca-slicer.exe`
- `Prusa3D\PrusaSlicer\prusa-slicer-console.exe` (Fallback: `prusa-slicer.exe`)
- `SuperSlicer\superslicer.exe`
- Cura: kein fester Ordnername, da versioniert (z. B.
  „Ultimaker Cura 5.7"). Stattdessen wird `Program Files` /
  `Program Files (x86)` per `read_dir` durchsucht und jeder Ordner
  geprüft, dessen Name mit „Ultimaker Cura" oder „UltiMaker Cura"
  beginnt; darin wird nach `UltiMaker-Cura.exe` bzw. `Cura.exe` gesucht.

macOS wird nicht berücksichtigt (App läuft aktuell nicht auf macOS).

## Datenmodell & Listen-Verwaltung (Frontend)

`SlicerConfig` (`src/types/index.ts`) bekommt ein neues Pflichtfeld:

```ts
export interface SlicerConfig {
  id: string;
  name: string;
  path: string;
  source: 'manual' | 'auto';
}
```

`useSlicers.ts` (`src/hooks/useSlicers.ts`):

- Die persistierte Struktur (`StoredState`) bekommt ein zusätzliches Feld
  `dismissedPaths: string[]` — Pfade automatisch gefundener Slicer, die
  der Nutzer explizit entfernt hat und die bei künftigen Scans nicht
  erneut vorgeschlagen werden sollen.
- Bestehendes `addSlicer(name, path)` (manueller Weg über den
  Datei-Dialog) setzt `source: 'manual'`.
- Neue Funktion `mergeDetected(detected: DetectedSlicer[])`: für jeden
  Eintrag aus `detected`, dessen `path` weder in der aktuellen
  Slicer-Liste (beliebige `source`) noch in `dismissedPaths` vorkommt,
  wird ein neuer Eintrag mit `source: 'auto'` und frischer `id`
  hinzugefügt. Ergebnis wird wie gehabt in `localStorage` persistiert.
- `removeSlicer(id)` wird erweitert: wird ein Eintrag mit
  `source === 'auto'` entfernt, wird sein `path` zusätzlich in
  `dismissedPaths` aufgenommen. Bei `source === 'manual'` verhält sich
  die Funktion wie bisher (kein Dismiss-Eintrag).

`App.tsx`: ein `useEffect` beim Mount ruft
`invoke<DetectedSlicer[]>('scan_installed_slicers')` auf und übergibt das
Ergebnis an `mergeDetected`. Fehler beim Invoke werden nur geloggt
(`console.warn`), nicht dem Nutzer angezeigt — das Feature ist rein
komfortsteigernd, kein kritischer Pfad.

`Header.tsx` (Slicer-Einstellungsbereich): jeder Listeneintrag mit
`source === 'auto'` bekommt ein kleines Label „automatisch erkannt" neben
dem Namen (analog zu bestehenden Badge-Mustern der App, z. B.
`font-mono-ui text-[10px] uppercase text-[var(--ink-3)]`).

## i18n

Neuer Übersetzungsschlüssel `slicerAutoDetectedLabel` („automatisch
erkannt" / „auto-detected" / entsprechend ES/FR) in allen vier
Sprachdateien plus `src/i18n/types.ts`.

## Fehlerbehandlung

- Backend: jeder einzelne Verzeichniszugriff ist in sich fehlertolerant
  (nicht lesbare Ordner, fehlende Berechtigungen, nicht existierende
  Pfade werden übersprungen). Die Gesamtfunktion gibt immer ein `Vec`
  zurück, nie einen Fehler.
- Frontend: ein fehlschlagender `invoke`-Aufruf (z. B. Command nicht
  registriert) wird geloggt und ignoriert; die App funktioniert exakt wie
  vor diesem Feature weiter, nur ohne automatische Ergänzung.

## Testing

- Rust: Unit-Tests in `src-tauri/src/slicers.rs` für die
  Dedupe-Logik (gleicher Pfad über PATH und festen Ordner gefunden →
  nur einmal im Ergebnis) und für die Cura-Versionsordner-Erkennung
  (Verzeichnisname mit Präfix „Ultimaker Cura" wird erkannt, andere
  Ordnernamen nicht) — jeweils gegen eine im Test angelegte temporäre
  Verzeichnisstruktur (`tempfile`-Crate wird als neue Dev-Dependency
  ergänzt, falls noch nicht vorhanden), nicht gegen das echte System.
- Frontend: kein Testframework im Projekt vorhanden (etabliertes Muster
  dieses Projekts: `npx tsc --noEmit` + `npm run build` +
  manueller Smoke-Test im gebauten AppImage), wie bei allen bisherigen
  Features dieser Session.

## Out of Scope

- macOS-Erkennung.
- Erkennung von Slicern, die nicht in der bekannten Liste (Bambu Studio,
  OrcaSlicer, PrusaSlicer, SuperSlicer, Cura) stehen — dafür bleibt der
  manuelle Weg.
- Eine sichtbare Benachrichtigung/Toast bei neu gefundenen Slicern (sie
  erscheinen still in der Liste, analog zum bisherigen manuellen
  Hinzufügen ohne Bestätigungs-Toast).
- Erneutes/periodisches Scannen während der Laufzeit (nur einmal beim
  Start).
