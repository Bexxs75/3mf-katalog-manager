# Automatische Slicer-Erkennung Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die App durchsucht beim Start bekannte Installationsorte für Bambu Studio, OrcaSlicer, PrusaSlicer, SuperSlicer und UltiMaker Cura auf Linux und Windows und ergänzt automatisch gefundene Slicer in die bestehende, manuell gepflegte Slicer-Liste.

**Architecture:** Ein neues, reines Rust-Modul (`src-tauri/src/slicers.rs`) kapselt die gesamte Such-/Dedupe-Logik als testbare, von Tauri unabhängige Funktionen. Ein neuer Tauri-Command (`scan_installed_slicers`) ruft diese Logik auf und liefert das Ergebnis als JSON an das Frontend. `useSlicers.ts` bekommt eine `mergeDetected`-Funktion, die Treffer gegen vorhandene Einträge und eine „Dismissed"-Liste abgleicht, bevor sie zur persistierten Liste hinzugefügt werden. `App.tsx` ruft den Command einmalig beim Mount auf.

**Tech Stack:** Rust (std::env, std::fs, keine neuen Cargo-Dependencies), Tauri 2 Commands, React/TypeScript, localStorage (bestehendes Muster aus `useSlicers.ts`).

## Global Constraints

- Backend-Suche ist immer best-effort: kein einzelner Scan-Schritt darf einen Fehler propagieren oder einen Panic auslösen; `detect_slicers()` gibt immer ein `Vec` zurück (ggf. leer), nie ein `Result`.
- Bekannte Slicer und ihre Linux-Binärnamen: Bambu Studio (`bambu-studio`, `BambuStudio`), OrcaSlicer (`orca-slicer`, `OrcaSlicer`), PrusaSlicer (`prusa-slicer`, `prusaslicer`), SuperSlicer (`superslicer`), UltiMaker Cura (`cura`, `UltiMaker-Cura`).
- Windows-Suchorte: `Bambu Studio\bambu-studio.exe`, `OrcaSlicer\orca-slicer.exe`, `Prusa3D\PrusaSlicer\prusa-slicer-console.exe` (Fallback `prusa-slicer.exe`), `SuperSlicer\superslicer.exe`, jeweils unter `%ProgramFiles%` und `%ProgramFiles(x86)%`; Cura zusätzlich per Ordner-Präfix-Scan („Ultimaker Cura"/„UltiMaker Cura").
- macOS wird nicht unterstützt.
- Bestehendes manuelles Hinzufügen von Slicern (Datei-Dialog in `Header.tsx`, `pick_slicer_executable`-Command) bleibt unverändert funktionsfähig.
- `SlicerConfig.source` unterscheidet `'manual'` (über Datei-Dialog hinzugefügt) und `'auto'` (automatisch gefunden). Entfernt der Nutzer einen `'auto'`-Eintrag, landet dessen Pfad dauerhaft in einer `dismissedPaths`-Liste und wird bei künftigen Scans übersprungen.
- Kein Test-Framework im Frontend vorhanden — Verifikation über `npx tsc --noEmit` + `npm run build` + manuellen Smoke-Test im AppImage (etabliertes Projektmuster).
- **Abweichung von der Spec bei den Rust-Tests:** die Spec schlägt `tempfile` als neue Dev-Dependency für die Dedupe- und Cura-Ordner-Tests vor. Dieser Plan vermeidet das bewusst, indem die Such-Logik in reine, dateisystemfreie Funktionen (`dedupe_by_path`, `is_cura_folder_name`) zerlegt wird, die direkt mit synthetischen Werten getestet werden — gleiches Testziel, keine neue Abhängigkeit, keine Dateisystem-Testinfrastruktur nötig.

---

### Task 1: Rust-Erkennungslogik (`src-tauri/src/slicers.rs`)

**Files:**
- Create: `src-tauri/src/slicers.rs`
- Modify: `src-tauri/src/lib.rs:1-6` (Modul registrieren)

**Interfaces:**
- Produces: `pub struct DetectedSlicer { pub name: String, pub path: String }` (mit `#[derive(Debug, Clone, serde::Serialize)]` und `#[serde(rename_all = "camelCase")]`), `pub fn detect_slicers() -> Vec<DetectedSlicer>`. Beide werden von Task 2 importiert (`crate::slicers::{detect_slicers, DetectedSlicer}`).

- [ ] **Step 1: Modul-Grundgerüst mit Datentyp anlegen**

Erstelle `src-tauri/src/slicers.rs`:

```rust
// Sucht bekannte 3D-Drucker-Slicer (Bambu Studio, OrcaSlicer, PrusaSlicer,
// SuperSlicer, UltiMaker Cura) an typischen Installationsorten des
// Betriebssystems. Rein lesend, best-effort: ein nicht lesbarer oder nicht
// existierender Ordner wird uebersprungen, nie propagiert - detect_slicers()
// gibt daher immer ein Vec zurueck (leer, wenn nichts gefunden wurde), nie
// ein Result.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedSlicer {
    pub name: String,
    pub path: String,
}

struct SlicerDefinition {
    display_name: &'static str,
    binary_names: &'static [&'static str],
}

const SLICER_DEFINITIONS: &[SlicerDefinition] = &[
    SlicerDefinition {
        display_name: "Bambu Studio",
        binary_names: &["bambu-studio", "BambuStudio"],
    },
    SlicerDefinition {
        display_name: "OrcaSlicer",
        binary_names: &["orca-slicer", "OrcaSlicer"],
    },
    SlicerDefinition {
        display_name: "PrusaSlicer",
        binary_names: &["prusa-slicer", "prusaslicer"],
    },
    SlicerDefinition {
        display_name: "SuperSlicer",
        binary_names: &["superslicer"],
    },
    SlicerDefinition {
        display_name: "UltiMaker Cura",
        binary_names: &["cura", "UltiMaker-Cura"],
    },
];
```

- [ ] **Step 2: PATH-Suche implementieren**

Direkt unter dem Block aus Step 1 anfügen:

```rust
fn search_path_env(binary_names: &[&str]) -> Option<String> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in binary_names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
            #[cfg(target_os = "windows")]
            {
                let candidate_exe = dir.join(format!("{name}.exe"));
                if candidate_exe.is_file() {
                    return Some(candidate_exe.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}
```

- [ ] **Step 3: Linux-spezifische feste Suchorte implementieren**

```rust
fn check_fixed_dir(dir: &Path, binary_names: &[&str]) -> Option<PathBuf> {
    for name in binary_names {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn check_opt_dirs(binary_names: &[&str]) -> Option<PathBuf> {
    for subfolder in binary_names {
        let dir = PathBuf::from("/opt").join(subfolder);
        if let Some(found) = check_fixed_dir(&dir, binary_names) {
            return Some(found);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn linux_fixed_search_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/var/lib/flatpak/exports/bin"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/share/flatpak/exports/bin"));
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join("Applications"));
        dirs.push(home.join("AppImages"));
    }
    dirs
}

#[cfg(target_os = "linux")]
fn detect_linux_fixed_locations() -> Vec<DetectedSlicer> {
    let mut results = Vec::new();
    let fixed_dirs = linux_fixed_search_dirs();
    for def in SLICER_DEFINITIONS {
        for dir in &fixed_dirs {
            if let Some(found) = check_fixed_dir(dir, def.binary_names) {
                results.push(DetectedSlicer {
                    name: def.display_name.to_string(),
                    path: found.to_string_lossy().to_string(),
                });
            }
        }
        if let Some(found) = check_opt_dirs(def.binary_names) {
            results.push(DetectedSlicer {
                name: def.display_name.to_string(),
                path: found.to_string_lossy().to_string(),
            });
        }
    }
    results
}
```

- [ ] **Step 4: Windows-spezifische feste Suchorte implementieren**

```rust
#[cfg(target_os = "windows")]
const WINDOWS_FIXED_LOCATIONS: &[(&str, &[&str])] = &[
    ("Bambu Studio", &["Bambu Studio\\bambu-studio.exe"]),
    ("OrcaSlicer", &["OrcaSlicer\\orca-slicer.exe"]),
    (
        "PrusaSlicer",
        &[
            "Prusa3D\\PrusaSlicer\\prusa-slicer-console.exe",
            "Prusa3D\\PrusaSlicer\\prusa-slicer.exe",
        ],
    ),
    ("SuperSlicer", &["SuperSlicer\\superslicer.exe"]),
];

#[cfg(target_os = "windows")]
fn windows_program_files_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        dirs.push(PathBuf::from(pf));
    }
    if let Some(pf86) = std::env::var_os("ProgramFiles(x86)") {
        dirs.push(PathBuf::from(pf86));
    }
    dirs
}

#[cfg(target_os = "windows")]
fn detect_windows_fixed_locations() -> Vec<DetectedSlicer> {
    let mut results = Vec::new();
    for (display_name, relative_paths) in WINDOWS_FIXED_LOCATIONS {
        for pf in &windows_program_files_dirs() {
            for rel in *relative_paths {
                let candidate = pf.join(rel);
                if candidate.is_file() {
                    results.push(DetectedSlicer {
                        name: display_name.to_string(),
                        path: candidate.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }
    results
}

#[cfg(target_os = "windows")]
fn detect_windows_cura() -> Vec<DetectedSlicer> {
    let mut results = Vec::new();
    for pf in windows_program_files_dirs() {
        let Ok(entries) = std::fs::read_dir(&pf) else {
            continue;
        };
        for entry in entries.flatten() {
            let folder_name = entry.file_name();
            let folder_name = folder_name.to_string_lossy();
            if !is_cura_folder_name(&folder_name) {
                continue;
            }
            let dir = entry.path();
            for exe in ["UltiMaker-Cura.exe", "Cura.exe"] {
                let candidate = dir.join(exe);
                if candidate.is_file() {
                    results.push(DetectedSlicer {
                        name: "UltiMaker Cura".to_string(),
                        path: candidate.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }
    results
}

fn is_cura_folder_name(name: &str) -> bool {
    name.starts_with("Ultimaker Cura") || name.starts_with("UltiMaker Cura")
}
```

Hinweis: `is_cura_folder_name` bewusst NICHT hinter `#[cfg(target_os = "windows")]` - eine reine String-Funktion, die auf jeder Plattform kompiliert und getestet werden kann (siehe Step 6).

- [ ] **Step 5: Dedupe-Logik und öffentliche `detect_slicers()`-Funktion**

```rust
fn dedupe_by_path(items: Vec<DetectedSlicer>) -> Vec<DetectedSlicer> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for item in items {
        if seen.insert(item.path.clone()) {
            result.push(item);
        }
    }
    result
}

pub fn detect_slicers() -> Vec<DetectedSlicer> {
    let mut results = Vec::new();

    for def in SLICER_DEFINITIONS {
        if let Some(path) = search_path_env(def.binary_names) {
            results.push(DetectedSlicer {
                name: def.display_name.to_string(),
                path,
            });
        }
    }

    #[cfg(target_os = "linux")]
    results.extend(detect_linux_fixed_locations());

    #[cfg(target_os = "windows")]
    {
        results.extend(detect_windows_fixed_locations());
        results.extend(detect_windows_cura());
    }

    dedupe_by_path(results)
}
```

- [ ] **Step 6: Tests schreiben**

Ganz am Ende von `src-tauri/src/slicers.rs` anfügen:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_by_path_removes_duplicates_keeping_first() {
        let items = vec![
            DetectedSlicer {
                name: "Bambu Studio".to_string(),
                path: "/usr/bin/bambu-studio".to_string(),
            },
            DetectedSlicer {
                name: "Bambu Studio (opt)".to_string(),
                path: "/usr/bin/bambu-studio".to_string(),
            },
            DetectedSlicer {
                name: "OrcaSlicer".to_string(),
                path: "/usr/bin/orca-slicer".to_string(),
            },
        ];

        let result = dedupe_by_path(items);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, "/usr/bin/bambu-studio");
        assert_eq!(result[0].name, "Bambu Studio");
        assert_eq!(result[1].path, "/usr/bin/orca-slicer");
    }

    #[test]
    fn dedupe_by_path_keeps_all_when_paths_differ() {
        let items = vec![
            DetectedSlicer {
                name: "Bambu Studio".to_string(),
                path: "/usr/bin/bambu-studio".to_string(),
            },
            DetectedSlicer {
                name: "OrcaSlicer".to_string(),
                path: "/usr/bin/orca-slicer".to_string(),
            },
        ];

        let result = dedupe_by_path(items);

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn is_cura_folder_name_matches_expected_prefixes() {
        assert!(is_cura_folder_name("Ultimaker Cura 5.7"));
        assert!(is_cura_folder_name("UltiMaker Cura 5.8.0"));
        assert!(is_cura_folder_name("Ultimaker Cura"));
    }

    #[test]
    fn is_cura_folder_name_rejects_unrelated_names() {
        assert!(!is_cura_folder_name("Bambu Studio"));
        assert!(!is_cura_folder_name("Cura"));
        assert!(!is_cura_folder_name("Some Other App"));
    }

    #[test]
    fn detect_slicers_returns_without_panicking() {
        // Best-effort auf dem echten System, in dem die Tests laufen: darf
        // leer sein, muss aber immer zurueckkehren statt zu paniken, egal
        // welche Slicer lokal installiert sind.
        let _ = detect_slicers();
    }
}
```

- [ ] **Step 7: Modul in `lib.rs` registrieren**

In `src-tauri/src/lib.rs`, Zeile 1-6, `mod slicers;` alphabetisch einsortieren:

```rust
mod commands;
mod db;
mod geometry;
mod slicers;
mod stl;
mod tagging;
mod threemf;
```

- [ ] **Step 8: Tests ausführen**

Run: `cd src-tauri && cargo test --lib slicers`
Expected: alle 5 Tests aus Step 6 bestehen (`dedupe_by_path_removes_duplicates_keeping_first`, `dedupe_by_path_keeps_all_when_paths_differ`, `is_cura_folder_name_matches_expected_prefixes`, `is_cura_folder_name_rejects_unrelated_names`, `detect_slicers_returns_without_panicking`).

Run: `cd src-tauri && cargo build --release`
Expected: baut ohne Fehler (Warnungen zu bislang ungenutzten Funktionen wie `detect_slicers` sind an dieser Stelle normal - Task 2 verdrahtet sie).

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/slicers.rs src-tauri/src/lib.rs
git commit -m "feat: Rust-Erkennungslogik fuer installierte Slicer"
```

---

### Task 2: Tauri-Command `scan_installed_slicers`

**Files:**
- Modify: `src-tauri/src/commands.rs` (neuer Command, am Ende der Datei vor dem `#[cfg(test)]`-Block einfügen, z. B. direkt nach `open_in_slicer`)
- Modify: `src-tauri/src/lib.rs` (Command im `invoke_handler!`-Makro registrieren)

**Interfaces:**
- Consumes: `crate::slicers::{detect_slicers, DetectedSlicer}` aus Task 1.
- Produces: Tauri-Command `scan_installed_slicers() -> CmdResult<Vec<DetectedSlicer>>`, aufrufbar vom Frontend als `invoke<DetectedSlicer[]>('scan_installed_slicers')` mit `DetectedSlicer = { name: string; path: string }` (camelCase durch `#[serde(rename_all = "camelCase")]` aus Task 1 bereits sichergestellt).

- [ ] **Step 1: Import ergänzen**

In `src-tauri/src/commands.rs`, im bestehenden `use crate::...`-Import-Block am Dateikopf (neben `use crate::geometry::RenderMesh;`), einfügen:

```rust
use crate::slicers::{detect_slicers, DetectedSlicer};
```

- [ ] **Step 2: Command hinzufügen**

Direkt nach der bestehenden `open_in_slicer`-Funktion (aktuell endend mit `Ok(())\n}` nach dem in einem vorherigen Fix ergänzten `APPIMAGE_ENV_VARS_TO_STRIP`-Block), einfügen:

```rust
#[tauri::command]
pub fn scan_installed_slicers() -> CmdResult<Vec<DetectedSlicer>> {
    Ok(detect_slicers())
}
```

- [ ] **Step 3: Command in `lib.rs` registrieren**

In `src-tauri/src/lib.rs`, im `tauri::generate_handler![...]`-Makro-Aufruf, nach der Zeile `commands::open_in_slicer,` einfügen:

```rust
            commands::open_in_slicer,
            commands::scan_installed_slicers,
```

- [ ] **Step 4: Build verifizieren**

Run: `cd src-tauri && cargo build --release`
Expected: baut ohne Fehler. Die vorherige Warnung „function detect_slicers is never used" (falls in Task 1 Step 8 aufgetreten) ist jetzt verschwunden, da der neue Command sie verwendet.

Run: `cd src-tauri && cargo test --lib`
Expected: alle bestehenden Tests plus die 5 aus Task 1 bestehen weiterhin (keine Regression).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: Tauri-Command scan_installed_slicers registrieren"
```

---

### Task 3: Frontend-Datenmodell (`SlicerConfig.source`, `useSlicers.ts`, i18n)

**Files:**
- Modify: `src/types/index.ts:61-65` (`SlicerConfig`-Interface)
- Modify: `src/hooks/useSlicers.ts` (komplette Datei)
- Modify: `src/i18n/types.ts:60` (nach `slicerLaunchError`)
- Modify: `src/i18n/de.ts:55`, `src/i18n/en.ts:55`, `src/i18n/es.ts:55`, `src/i18n/fr.ts:55` (jeweils nach `slicerLaunchError`)

**Interfaces:**
- Consumes: nichts aus vorherigen Tasks (reines Frontend-Datenmodell).
- Produces: `SlicerConfig { id: string; name: string; path: string; source: 'manual' | 'auto' }`, `useSlicers()` liefert zusätzlich `mergeDetected: (detected: { name: string; path: string }[]) => void` neben den bestehenden `slicers`, `lastUsedId`, `addSlicer`, `removeSlicer`, `setLastUsed`. Beides wird von Task 4 (`App.tsx`, `Header.tsx`) konsumiert.

- [ ] **Step 1: `SlicerConfig` um `source` erweitern**

In `src/types/index.ts`, das bestehende Interface ersetzen:

```ts
export interface SlicerConfig {
  id: string;
  name: string;
  path: string;
  source: 'manual' | 'auto';
}
```

- [ ] **Step 2: `useSlicers.ts` um Dismiss-Liste und `mergeDetected` erweitern**

Ersetze den kompletten Inhalt von `src/hooks/useSlicers.ts`:

```ts
import { useCallback, useState } from 'react';
import type { SlicerConfig } from '../types';

const STORAGE_KEY = '3mf-katalog-slicers';

interface DetectedSlicerInput {
  name: string;
  path: string;
}

interface StoredState {
  slicers: SlicerConfig[];
  lastUsedId: string | null;
  dismissedPaths: string[];
}

function loadStored(): StoredState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { slicers: [], lastUsedId: null, dismissedPaths: [] };
    const parsed = JSON.parse(raw) as Partial<StoredState>;
    const slicers = Array.isArray(parsed.slicers) ? parsed.slicers : [];
    return {
      // Aeltere, vor diesem Feature persistierte Eintraege haben noch kein
      // source-Feld - werden als 'manual' behandelt, da sie ausschliesslich
      // ueber den Datei-Dialog entstanden sein koennen.
      slicers: slicers.map((s) => ({ ...s, source: s.source ?? 'manual' })),
      lastUsedId: typeof parsed.lastUsedId === 'string' ? parsed.lastUsedId : null,
      dismissedPaths: Array.isArray(parsed.dismissedPaths) ? parsed.dismissedPaths : [],
    };
  } catch {
    return { slicers: [], lastUsedId: null, dismissedPaths: [] };
  }
}

/**
 * Verwaltet die konfigurierten Slicer-Programme (Name + Pfad), sowohl
 * manuell hinzugefuegte als auch automatisch erkannte. Persistiert in
 * localStorage nach demselben Muster wie Theme/Sprache (useTheme.ts) -
 * keine Backend-/DB-Beteiligung noetig fuer eine kurze Konfigurationsliste.
 */
export function useSlicers() {
  const [state, setState] = useState<StoredState>(loadStored);

  const addSlicer = useCallback((name: string, path: string) => {
    setState((prev) => {
      const next: StoredState = {
        ...prev,
        slicers: [...prev.slicers, { id: crypto.randomUUID(), name, path, source: 'manual' }],
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const removeSlicer = useCallback((id: string) => {
    setState((prev) => {
      const removed = prev.slicers.find((s) => s.id === id);
      const next: StoredState = {
        slicers: prev.slicers.filter((s) => s.id !== id),
        lastUsedId: prev.lastUsedId === id ? null : prev.lastUsedId,
        dismissedPaths:
          removed && removed.source === 'auto'
            ? [...prev.dismissedPaths, removed.path]
            : prev.dismissedPaths,
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const setLastUsed = useCallback((id: string) => {
    setState((prev) => {
      const next: StoredState = { ...prev, lastUsedId: id };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const mergeDetected = useCallback((detected: DetectedSlicerInput[]) => {
    setState((prev) => {
      const knownPaths = new Set(prev.slicers.map((s) => s.path));
      const dismissed = new Set(prev.dismissedPaths);
      const additions: SlicerConfig[] = detected
        .filter((d) => !knownPaths.has(d.path) && !dismissed.has(d.path))
        .map((d) => ({ id: crypto.randomUUID(), name: d.name, path: d.path, source: 'auto' }));
      if (additions.length === 0) return prev;
      const next: StoredState = { ...prev, slicers: [...prev.slicers, ...additions] };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  return {
    slicers: state.slicers,
    lastUsedId: state.lastUsedId,
    addSlicer,
    removeSlicer,
    setLastUsed,
    mergeDetected,
  };
}
```

- [ ] **Step 3: i18n-Schlüssel `slicerAutoDetectedLabel` hinzufügen**

In `src/i18n/types.ts`, Zeile 60 (direkt nach `slicerLaunchError: string;`), einfügen:

```ts
  slicerAutoDetectedLabel: string;
```

In `src/i18n/de.ts`, Zeile 55 (direkt nach `slicerLaunchError: 'Slicer konnte nicht gestartet werden:',`), einfügen:

```ts
  slicerAutoDetectedLabel: 'automatisch erkannt',
```

In `src/i18n/en.ts`, Zeile 55 (direkt nach `slicerLaunchError: 'Could not start slicer:',`), einfügen:

```ts
  slicerAutoDetectedLabel: 'auto-detected',
```

In `src/i18n/es.ts`, Zeile 55 (direkt nach `slicerLaunchError: 'No se pudo iniciar el laminador:',`), einfügen:

```ts
  slicerAutoDetectedLabel: 'detectado automáticamente',
```

In `src/i18n/fr.ts`, Zeile 55 (direkt nach `slicerLaunchError: 'Impossible de démarrer le slicer :',`), einfügen:

```ts
  slicerAutoDetectedLabel: 'détecté automatiquement',
```

- [ ] **Step 4: TypeScript verifizieren**

Run: `npx tsc --noEmit`
Expected: 0 Fehler. Falls Fehler zu fehlendem `source`-Feld an anderen Stellen auftreten (z. B. Testdaten oder Mock-Objekte, die `SlicerConfig` konstruieren), diese Stellen mit `source: 'manual'` ergänzen.

- [ ] **Step 5: Commit**

```bash
git add src/types/index.ts src/hooks/useSlicers.ts src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "feat: SlicerConfig.source, dismissedPaths und mergeDetected in useSlicers"
```

---

### Task 4: Verdrahtung in `App.tsx` und Badge in `Header.tsx`

**Files:**
- Modify: `src/App.tsx:29` (Destrukturierung von `useSlicers()`)
- Modify: `src/App.tsx:110-120` (bestehender Mount-`useEffect`)
- Modify: `src/components/Header.tsx:302-309` (Slicer-Listen-Rendering)

**Interfaces:**
- Consumes: `mergeDetected` aus `useSlicers()` (Task 3), Tauri-Command `scan_installed_slicers` (Task 2), `DetectedSlicer { name: string; path: string }`-Shape vom Backend, i18n-Schlüssel `slicerAutoDetectedLabel` (Task 3).

- [ ] **Step 1: `mergeDetected` aus `useSlicers()` destrukturieren**

In `src/App.tsx`, Zeile 29, ersetzen:

```tsx
  const { slicers, lastUsedId, addSlicer, removeSlicer, setLastUsed } = useSlicers();
```

durch:

```tsx
  const { slicers, lastUsedId, addSlicer, removeSlicer, setLastUsed, mergeDetected } = useSlicers();
```

- [ ] **Step 2: Scan beim Mount auslösen**

In `src/App.tsx`, im bestehenden Mount-`useEffect` (aktuell Zeilen 110-120), nach `refreshTrash();` einfügen:

```tsx
  useEffect(() => {
    invoke<ModelFile[]>('list_files').then((files) => {
      setModels(files);
      setSelectedId((prev) => prev ?? files[0]?.id ?? null);
    });
    refreshFolders();
    refreshTags();
    refreshCreators();
    refreshSavedFilters();
    refreshTrash();
    invoke<{ name: string; path: string }[]>('scan_installed_slicers')
      .then(mergeDetected)
      .catch((e) => {
        // Rein komfortsteigerndes Feature - ein Fehlschlag (z.B. Command
        // aus irgendeinem Grund nicht verfuegbar) darf die App nicht
        // beeintraechtigen, nur geloggt werden.
        console.warn('[slicer-scan] Automatische Slicer-Erkennung fehlgeschlagen:', e);
      });
  }, []);
```

- [ ] **Step 3: Badge für automatisch erkannte Slicer in `Header.tsx`**

In `src/components/Header.tsx`, den bestehenden Block (aktuell Zeilen 302-309):

```tsx
                {slicers.map((s) => (
                  <div key={s.id} className="flex items-center gap-2">
                    <div className="flex-1 min-w-0">
                      <div className="text-[length:var(--font-size-title)] text-[var(--ink)] truncate">{s.name}</div>
                      <div className="font-mono-ui text-[length:var(--font-size-meta)] text-[var(--ink-3)] truncate">
                        {s.path}
                      </div>
                    </div>
```

ersetzen durch:

```tsx
                {slicers.map((s) => (
                  <div key={s.id} className="flex items-center gap-2">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-1.5">
                        <div className="text-[length:var(--font-size-title)] text-[var(--ink)] truncate">{s.name}</div>
                        {s.source === 'auto' && (
                          <span className="flex-none font-mono-ui text-[9px] tracking-[0.08em] uppercase text-[var(--ink-3)]">
                            {t('slicerAutoDetectedLabel')}
                          </span>
                        )}
                      </div>
                      <div className="font-mono-ui text-[length:var(--font-size-meta)] text-[var(--ink-3)] truncate">
                        {s.path}
                      </div>
                    </div>
```

(Der Rest des Blocks - das Entfernen-Kreuz `<span onClick={() => onRemoveSlicer(s.id)} ...>` - bleibt unverändert bestehen.)

- [ ] **Step 4: TypeScript und Build verifizieren**

Run: `npx tsc --noEmit`
Expected: 0 Fehler.

Run: `npm run build`
Expected: baut ohne Fehler.

- [ ] **Step 5: Manueller Smoke-Test**

Run: `cd src-tauri && cargo build --release && cd .. && npm run tauri dev`
Erwartet: App startet, Einstellungen öffnen (Zahnrad-Icon im Header) zeigt den Slicer-Abschnitt. Falls auf der Entwicklungsmaschine ein Slicer aus der bekannten Liste installiert ist (z. B. via PATH auffindbar), erscheint er automatisch mit dem Label „automatisch erkannt". Manuelles Hinzufügen über „+ Hinzufügen" funktioniert weiterhin unverändert und zeigt kein Label. Ein automatisch erkannter Eintrag lässt sich per ✕ entfernen und taucht nach einem Neustart der App nicht wieder auf (Dismiss-Liste greift).

- [ ] **Step 6: Commit**

```bash
git add src/App.tsx src/components/Header.tsx
git commit -m "feat: automatische Slicer-Erkennung in App.tsx und Header.tsx verdrahten"
```
