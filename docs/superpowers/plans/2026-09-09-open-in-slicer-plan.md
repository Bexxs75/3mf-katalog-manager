# "In Slicer öffnen" Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Den bestehenden Stub `openInSlicer` durch eine funktionierende, herstellerunabhängige "In Slicer öffnen"-Funktion ersetzen, bei der der Nutzer selbst einen oder mehrere Slicer-Pfade konfiguriert.

**Architecture:** Frontend verwaltet die Slicer-Konfiguration (`localStorage`, nach dem Muster von `useTheme.ts`) und den UI-Zustand (Split-Button, Einstellungen-Panel); zwei neue schlanke Tauri-Commands übernehmen den nativen Datei-Dialog zur Pfad-Auswahl und das Starten des gewählten Slicers als externen Prozess.

**Tech Stack:** Tauri v2 (`tauri_plugin_dialog`, `std::process::Command`), React 19/TypeScript, `localStorage`.

## Global Constraints

- Deutsche Kommentare nur wo das WARUM nicht aus dem Code ersichtlich ist.
- Windows und Linux werden unterstützt, **macOS explizit nicht** (`.app`-Bundles bräuchten einen eigenen Start-Mechanismus - bewusst zurückgestellt, siehe Spec).
- Kein neues JS-Test-Framework (Projekt hat aktuell keins) - Frontend-Tasks werden über `npx tsc --noEmit` und den abschließenden manuellen Test verifiziert, nicht über automatisierte Unit-Tests.
- Rust-Seite: `pick_slicer_executable` öffnet einen echten, blockierenden OS-Dialog und lässt sich nicht sinnvoll automatisiert testen - dafür gibt es bewusst keinen Test. `open_in_slicer` lässt sich dagegen deterministisch testen (Fehlerfall: nicht existierender Pfad; Erfolgsfall: `/usr/bin/true`, auf jedem Unix-System vorhanden und beendet sich sofort) und bekommt echte Tests.
- Git-Commits auf Deutsch, mit gezieltem `git add <Datei>` (nie `-A`/`.`), jeder Commit endet mit:
  ```
  Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
  ```
- Nach jedem Rust-Task: `cd src-tauri && cargo test --no-default-features` muss vollständig grün sein.
- Kein `git commit --amend`, keine `--no-verify`.

---

### Task 1: `SlicerConfig`-Typ und `useSlicers`-Hook

**Files:**
- Modify: `src/types/index.ts`
- Create: `src/hooks/useSlicers.ts`

**Interfaces:**
- Consumes: nichts.
- Produces:
  - `export interface SlicerConfig { id: string; name: string; path: string }` (in `src/types/index.ts`)
  - `export function useSlicers(): { slicers: SlicerConfig[]; lastUsedId: string | null; addSlicer: (name: string, path: string) => void; removeSlicer: (id: string) => void; setLastUsed: (id: string) => void }` (in `src/hooks/useSlicers.ts`)

  Tasks 4, 5 und 6 importieren `SlicerConfig` aus `../types`; Task 6 nutzt `useSlicers()`.

- [ ] **Step 1: `SlicerConfig` zu `src/types/index.ts` hinzufügen**

Füge am Ende von `src/types/index.ts` an:

```ts
export interface SlicerConfig {
  id: string;
  name: string;
  path: string;
}
```

- [ ] **Step 2: `useSlicers`-Hook schreiben**

Erstelle `src/hooks/useSlicers.ts`:

```ts
import { useCallback, useState } from 'react';
import type { SlicerConfig } from '../types';

const STORAGE_KEY = '3mf-katalog-slicers';

interface StoredState {
  slicers: SlicerConfig[];
  lastUsedId: string | null;
}

function loadStored(): StoredState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { slicers: [], lastUsedId: null };
    const parsed = JSON.parse(raw) as StoredState;
    return {
      slicers: Array.isArray(parsed.slicers) ? parsed.slicers : [],
      lastUsedId: typeof parsed.lastUsedId === 'string' ? parsed.lastUsedId : null,
    };
  } catch {
    return { slicers: [], lastUsedId: null };
  }
}

/**
 * Verwaltet die vom Nutzer konfigurierten Slicer-Programme (Name + Pfad).
 * Persistiert in localStorage nach demselben Muster wie Theme/Sprache
 * (useTheme.ts) - keine Backend-/DB-Beteiligung noetig fuer eine kurze
 * Konfigurationsliste.
 */
export function useSlicers() {
  const [state, setState] = useState<StoredState>(loadStored);

  const addSlicer = useCallback((name: string, path: string) => {
    setState((prev) => {
      const next: StoredState = {
        ...prev,
        slicers: [...prev.slicers, { id: crypto.randomUUID(), name, path }],
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const removeSlicer = useCallback((id: string) => {
    setState((prev) => {
      const next: StoredState = {
        slicers: prev.slicers.filter((s) => s.id !== id),
        lastUsedId: prev.lastUsedId === id ? null : prev.lastUsedId,
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

  return {
    slicers: state.slicers,
    lastUsedId: state.lastUsedId,
    addSlicer,
    removeSlicer,
    setLastUsed,
  };
}
```

- [ ] **Step 3: TypeScript-Kompilierung verifizieren**

Run: `npx tsc --noEmit`
Expected: keine neuen Fehler (dieser Task fügt nur neuen, noch ungenutzten Code hinzu - keine bestehende Datei wird auf eine Weise geändert, die etwas anderes bricht).

- [ ] **Step 4: Commit**

```bash
git add src/types/index.ts src/hooks/useSlicers.ts
git commit -m "$(cat <<'EOF'
Frontend: SlicerConfig-Typ und useSlicers-Hook

Verwaltet vom Nutzer konfigurierte Slicer-Programme (Name + Pfad,
mehrere moeglich) in localStorage - gleiches Persistenzmuster wie
Theme/Sprache (useTheme.ts). Noch nicht verdrahtet, das folgt in
spaeteren Commits.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

---

### Task 2: Backend-Commands `pick_slicer_executable` und `open_in_slicer`

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `CmdResult<T>` (bereits vorhandener Typalias `Result<T, String>`), `tauri_plugin_dialog::DialogExt` (bereits importiert).
- Produces: Tauri-Commands `pick_slicer_executable() -> CmdResult<Option<String>>` und `open_in_slicer(slicer_path: String, file_path: String) -> CmdResult<()>`, registriert in `lib.rs`.

  Task 6 (App.tsx) ruft beide über `invoke('pick_slicer_executable')` bzw.
  `invoke('open_in_slicer', { slicerPath, filePath })` auf.

- [ ] **Step 1: Failing tests schreiben**

`src-tauri/src/commands.rs` hat bereits ein `#[cfg(test)] mod tests`-Block
(für `encode_render_meshes`-Tests). Füge dort **zusätzlich** diese zwei
Tests ein (nicht einen neuen `mod tests`-Block anlegen):

```rust
    #[test]
    fn open_in_slicer_returns_error_for_nonexistent_executable() {
        let result = open_in_slicer(
            "/definitely/does/not/exist/xyz123".to_string(),
            "/tmp/model.3mf".to_string(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn open_in_slicer_spawns_successfully_for_a_real_executable() {
        // "/usr/bin/true" ist auf jedem Unix-System vorhanden und beendet
        // sich sofort mit Exit-Code 0 - deterministischer Erfolgstest ohne
        // einen echten Slicer zu benoetigen.
        let result = open_in_slicer("/usr/bin/true".to_string(), "/tmp/model.3mf".to_string());
        assert!(result.is_ok());
    }
```

- [ ] **Step 2: Tests laufen lassen, Fehlschlag verifizieren**

Run: `cd src-tauri && cargo test --no-default-features commands:: -- --nocapture`
Expected: FAIL mit `cannot find function 'open_in_slicer'`

- [ ] **Step 3: Commands implementieren**

Füge in `src-tauri/src/commands.rs` nach der bestehenden `open_dropped`-
bzw. letzten Import-Funktion (direkt vor `fn encode_render_meshes` ist ein
guter, thematisch passender Ort - beide sind Datei-System-nahe Commands)
ein:

```rust
#[tauri::command]
pub fn pick_slicer_executable(app: tauri::AppHandle) -> CmdResult<Option<String>> {
    let dialog = app.dialog().file();
    // #[cfg] direkt auf dem let-Statement (Shadowing) statt "let mut" +
    // bedingter Neuzuweisung: unter Linux faellt diese Zeile komplett weg,
    // ein "mut"-Binding waere dort nie mutiert und wuerde eine
    // unused_mut-Warnung ausloesen.
    #[cfg(target_os = "windows")]
    let dialog = dialog.add_filter("Programme", &["exe"]);
    let picked = dialog.blocking_pick_file();
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub fn open_in_slicer(slicer_path: String, file_path: String) -> CmdResult<()> {
    std::process::Command::new(&slicer_path)
        .arg(&file_path)
        .spawn()
        .map_err(|e| format!("Slicer konnte nicht gestartet werden: {e}"))?;
    Ok(())
}
```

- [ ] **Step 4: Tests laufen lassen, Erfolg verifizieren**

Run: `cd src-tauri && cargo test --no-default-features -- --nocapture`
Expected: PASS für alle Tests im Projekt (inklusive der 2 neuen).

- [ ] **Step 5: Commands in `lib.rs` registrieren**

In `src-tauri/src/lib.rs`, im `tauri::generate_handler![...]`-Aufruf, nach
`commands::get_model_geometry,` ergänzen:

```rust
            commands::get_model_geometry,
            commands::pick_slicer_executable,
            commands::open_in_slicer,
```

- [ ] **Step 6: Kompilierung verifizieren**

Run: `cd src-tauri && cargo check --no-default-features`
Expected: nur die bekannten, bereits bestehenden `dead_code`-Warnungen, keine neuen Fehler.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
Rust: Commands zum Waehlen und Starten eines Slicer-Programms

pick_slicer_executable oeffnet den nativen Datei-Dialog (unter
Windows mit .exe-Filter, unter Linux ohne - dort haben Executables
meist keine Dateiendung) zur Auswahl eines Slicer-Pfads.
open_in_slicer startet die gewaehlte Datei mit dem Modellpfad als
Argument (std::process::Command, keine neue Abhaengigkeit noetig).
Herstellerunabhaengig - keine slicer-spezifische Integration.

Noch nicht ans Frontend angebunden, das folgt in einem spaeteren
Commit.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

---

### Task 3: i18n-Schlüssel für die neuen UI-Texte

**Files:**
- Modify: `src/i18n/types.ts`
- Modify: `src/i18n/de.ts`
- Modify: `src/i18n/en.ts`
- Modify: `src/i18n/es.ts`
- Modify: `src/i18n/fr.ts`

**Interfaces:**
- Consumes: nichts.
- Produces: 7 neue Felder im `Translations`-Interface (siehe unten), in allen 4 Sprachen befüllt. Tasks 4 und 5 verwenden diese Schlüssel via `t('...')`.

Neue Schlüssel: `slicerSectionTitle`, `noSlicersConfigured`, `addSlicer`,
`confirmSlicerName`, `removeSlicerAria`, `chooseSlicerAria`,
`slicerLaunchError`.

- [ ] **Step 1: Schlüssel zum `Translations`-Interface hinzufügen**

In `src/i18n/types.ts`, nach dem Feld `openInSlicer: string;` (im Block mit
`cancel`/`delete`/`openInSlicer`/`deleteConfirmQuestion`/`deleteAriaLabel`)
ergänzen:

```ts
  cancel: string;
  delete: string;
  openInSlicer: string;
  deleteConfirmQuestion: string;
  deleteAriaLabel: string;
  slicerSectionTitle: string;
  noSlicersConfigured: string;
  addSlicer: string;
  confirmSlicerName: string;
  removeSlicerAria: string;
  chooseSlicerAria: string;
  slicerLaunchError: string;
```

(Ersetzt den bestehenden 5-Zeilen-Block durch denselben Block plus die 7
neuen Zeilen direkt danach - `cancel` bis `deleteAriaLabel` bleiben
unverändert an ihrer Stelle.)

- [ ] **Step 2: Deutsche Übersetzungen**

In `src/i18n/de.ts`, nach `deleteAriaLabel: 'Eintrag löschen',` ergänzen:

```ts
  deleteAriaLabel: 'Eintrag löschen',
  slicerSectionTitle: 'Slicer',
  noSlicersConfigured: 'Kein Slicer konfiguriert.',
  addSlicer: 'Hinzufügen',
  confirmSlicerName: 'Übernehmen',
  removeSlicerAria: 'Slicer entfernen',
  chooseSlicerAria: 'Anderen Slicer wählen',
  slicerLaunchError: 'Slicer konnte nicht gestartet werden:',
```

- [ ] **Step 3: Englische Übersetzungen**

In `src/i18n/en.ts`, nach `deleteAriaLabel: 'Delete entry',` ergänzen:

```ts
  deleteAriaLabel: 'Delete entry',
  slicerSectionTitle: 'Slicer',
  noSlicersConfigured: 'No slicer configured.',
  addSlicer: 'Add',
  confirmSlicerName: 'Confirm',
  removeSlicerAria: 'Remove slicer',
  chooseSlicerAria: 'Choose a different slicer',
  slicerLaunchError: 'Could not start slicer:',
```

- [ ] **Step 4: Spanische Übersetzungen**

In `src/i18n/es.ts`, nach `deleteAriaLabel: 'Eliminar entrada',` ergänzen:

```ts
  deleteAriaLabel: 'Eliminar entrada',
  slicerSectionTitle: 'Laminador',
  noSlicersConfigured: 'Ningún laminador configurado.',
  addSlicer: 'Añadir',
  confirmSlicerName: 'Confirmar',
  removeSlicerAria: 'Eliminar laminador',
  chooseSlicerAria: 'Elegir otro laminador',
  slicerLaunchError: 'No se pudo iniciar el laminador:',
```

- [ ] **Step 5: Französische Übersetzungen**

In `src/i18n/fr.ts`, nach `deleteAriaLabel: "Supprimer l'entrée",`
ergänzen:

```ts
  deleteAriaLabel: "Supprimer l'entrée",
  slicerSectionTitle: 'Slicer',
  noSlicersConfigured: 'Aucun slicer configuré.',
  addSlicer: 'Ajouter',
  confirmSlicerName: 'Confirmer',
  removeSlicerAria: 'Supprimer le slicer',
  chooseSlicerAria: 'Choisir un autre slicer',
  slicerLaunchError: 'Impossible de démarrer le slicer :',
```

- [ ] **Step 6: TypeScript-Kompilierung verifizieren**

Run: `npx tsc --noEmit`
Expected: keine Fehler (alle 4 Sprachdateien implementieren jetzt wieder
vollständig das `Translations`-Interface - ein fehlender Schlüssel in
irgendeiner der 4 Dateien wäre hier ein Kompilierfehler).

- [ ] **Step 7: Commit**

```bash
git add src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "$(cat <<'EOF'
i18n: Texte fuer die Slicer-Konfiguration (DE/EN/ES/FR)

7 neue Schluessel fuer den Einstellungen-Abschnitt und den Split-
Button (Titel, Leerzustand, Hinzufuegen/Bestaetigen, zwei Aria-Labels,
Fehlermeldung beim Start).

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

---

### Task 4: Einstellungen-UI in `Header.tsx`

**Files:**
- Modify: `src/components/Header.tsx`

**Interfaces:**
- Consumes: `SlicerConfig` aus `../types` (Task 1); i18n-Schlüssel aus Task 3; Backend-Command `pick_slicer_executable` (Task 2).
- Produces: `Header` erwartet ab jetzt 5 zusätzliche Props:
  `settingsOpen: boolean`, `onSettingsOpenChange: (open: boolean) => void`,
  `slicers: SlicerConfig[]`, `onAddSlicer: (name: string, path: string) => void`,
  `onRemoveSlicer: (id: string) => void`.

**Hinweis:** Dieser Task macht `App.tsx`s Aufruf von `<Header ... />`
vorübergehend nicht typkonform (die neuen Pflicht-Props fehlen dort noch) -
das wird in Task 6 behoben. `npx tsc --noEmit` zeigt bis dahin Fehler in
`App.tsx`, nicht in `Header.tsx` selbst - das ist erwartet.

- [ ] **Step 1: Imports und Props-Interface anpassen**

Ersetze in `src/components/Header.tsx` den Import-Block und das
`Props`-Interface:

```tsx
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ViewMode, SortKey, SlicerConfig } from '../types';
import type { ThemeSetting } from '../hooks/useTheme';
import type { Language } from '../i18n/types';
import { formatCount } from '../i18n/types';
import { useLanguage, useT } from '../i18n/LanguageContext';

interface Props {
  view: ViewMode;
  onViewChange: (v: ViewMode) => void;
  sort: SortKey;
  onSortChange: (s: SortKey) => void;
  count: number;
  themeSetting: ThemeSetting;
  onThemeChange: (t: ThemeSetting) => void;
  onImportFiles: () => void;
  onImportFolder: () => void;
  cloudDriveConnected: boolean;
  onImportFromCloud: () => void;
  settingsOpen: boolean;
  onSettingsOpenChange: (open: boolean) => void;
  slicers: SlicerConfig[];
  onAddSlicer: (name: string, path: string) => void;
  onRemoveSlicer: (id: string) => void;
}
```

- [ ] **Step 2: Komponentensignatur, lokalen State und Handler anpassen**

Ersetze die Funktionssignatur und den bisherigen
`const [settingsOpen, setSettingsOpen] = useState(false);`:

```tsx
export function Header({
  view,
  onViewChange,
  sort,
  onSortChange,
  count,
  themeSetting,
  onThemeChange,
  onImportFiles,
  onImportFolder,
  cloudDriveConnected,
  onImportFromCloud,
  settingsOpen,
  onSettingsOpenChange,
  slicers,
  onAddSlicer,
  onRemoveSlicer,
}: Props) {
  const t = useT();
  const { language, setLanguage } = useLanguage();
  const [importMenuOpen, setImportMenuOpen] = useState(false);
  const [pendingSlicerPath, setPendingSlicerPath] = useState<string | null>(null);
  const [pendingSlicerName, setPendingSlicerName] = useState('');

  const handlePickSlicer = () => {
    invoke<string | null>('pick_slicer_executable').then((path) => {
      if (!path) return;
      const fileName = path.split(/[/\\]/).pop() ?? path;
      const suggested = fileName.replace(/\.[^./\\]+$/, '');
      setPendingSlicerPath(path);
      setPendingSlicerName(suggested);
    });
  };

  const confirmAddSlicer = () => {
    if (!pendingSlicerPath || !pendingSlicerName.trim()) return;
    onAddSlicer(pendingSlicerName.trim(), pendingSlicerPath);
    setPendingSlicerPath(null);
    setPendingSlicerName('');
  };
```

(Die restlichen Zeilen der Komponente - `return (...)` bis zum Ende - bleiben
zunächst unverändert, werden aber in den nächsten zwei Schritten
angepasst.)

- [ ] **Step 3: Settings-Button und -Panel auf die neuen Props umstellen**

Ersetze im `return`-Block:

```tsx
      <div className="relative shrink-0">
        <button
          onClick={() => onSettingsOpenChange(!settingsOpen)}
          className="w-8 h-8 grid place-items-center rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)] text-[15px] cursor-pointer hover:text-[var(--ink)] hover:border-[var(--line-strong)]"
        >
          ⚙
        </button>
```

(nur die `onClick`-Zeile ändert sich: `setSettingsOpen((o) => !o)` →
`onSettingsOpenChange(!settingsOpen)`, der Rest des Buttons bleibt gleich).

- [ ] **Step 4: Neuen Slicer-Abschnitt im Settings-Panel ergänzen**

Füge im Settings-Panel, direkt nach dem schließenden `</div>` des
Sprache-Grids und vor dem schließenden `</div>` des gesamten Panels, den
neuen Abschnitt ein:

```tsx
            <div className="text-[13px] font-semibold mt-4 mb-2">{t('slicerSectionTitle')}</div>
            {slicers.length === 0 ? (
              <div className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">
                {t('noSlicersConfigured')}
              </div>
            ) : (
              <div className="flex flex-col gap-1.5">
                {slicers.map((s) => (
                  <div key={s.id} className="flex items-center gap-2">
                    <div className="flex-1 min-w-0">
                      <div className="text-[12.5px] text-[var(--ink)] truncate">{s.name}</div>
                      <div className="font-mono-ui text-[10px] text-[var(--ink-3)] truncate">
                        {s.path}
                      </div>
                    </div>
                    <span
                      onClick={() => onRemoveSlicer(s.id)}
                      aria-label={t('removeSlicerAria')}
                      className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[10px] text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                    >
                      ✕
                    </span>
                  </div>
                ))}
              </div>
            )}
            {pendingSlicerPath ? (
              <div className="flex items-center gap-1.5 mt-2">
                <input
                  value={pendingSlicerName}
                  onChange={(e) => setPendingSlicerName(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && confirmAddSlicer()}
                  autoFocus
                  className="flex-1 h-7 px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-[12.5px]"
                />
                <button
                  onClick={confirmAddSlicer}
                  className="h-7 px-2.5 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[11.5px] font-semibold cursor-pointer"
                >
                  {t('confirmSlicerName')}
                </button>
              </div>
            ) : (
              <button
                onClick={handlePickSlicer}
                className="mt-2 h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                + {t('addSlicer')}
              </button>
            )}
          </div>
        )}
      </div>
    </header>
  );
}
```

(Die letzten 5 Zeilen hier - `</div>` bis `}` - sind identisch mit dem
bisherigen Dateiende und dienen nur als Anker, damit klar ist, wo der neue
Block endet.)

- [ ] **Step 5: Kompilierung prüfen**

Run: `npx tsc --noEmit`
Expected: Fehler ausschließlich in `App.tsx` (fehlende neue Props beim
`<Header ... />`-Aufruf dort), keine Fehler in `Header.tsx` selbst.

- [ ] **Step 6: Commit**

```bash
git add src/components/Header.tsx
git commit -m "$(cat <<'EOF'
Frontend: Slicer-Verwaltung im Einstellungen-Panel

Neuer Abschnitt "Slicer" nach dem Sprache-Abschnitt: Liste der
konfigurierten Slicer mit Entfernen-Button pro Eintrag, "+
Hinzufuegen" ruft den nativen Datei-Dialog auf und laesst den
vorgeschlagenen Namen vor dem Speichern editieren. settingsOpen ist
jetzt von App.tsx kontrollierter State statt lokal, damit auch
DetailPanel/ContextMenu das Panel oeffnen koennen, wenn noch kein
Slicer konfiguriert ist.

Macht App.tsx voruebergehend nicht kompilierbar (folgt in einem
spaeteren Commit).

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

---

### Task 5: Split-Button in `DetailPanel.tsx`

**Files:**
- Modify: `src/components/DetailPanel.tsx`

**Interfaces:**
- Consumes: `SlicerConfig` aus `../types` (Task 1); i18n-Schlüssel aus Task 3.
- Produces: `DetailPanel` erwartet ab jetzt `onOpenInSlicer: (slicerId?: string) => void` (Signatur geändert - vorher `() => void`), plus zwei neue Props `slicers: SlicerConfig[]` und `slicerError: string | null`.

**Hinweis:** Wie Task 4 macht auch dieser Task `App.tsx` vorübergehend
nicht kompilierbar (weitere fehlende Props am `<DetailPanel ... />`-Aufruf)
- wird in Task 6 behoben.

- [ ] **Step 1: Datei komplett ersetzen**

Ersetze den kompletten Inhalt von `src/components/DetailPanel.tsx`:

```tsx
import { useEffect, useState } from 'react';
import type { ModelFile, SlicerConfig, SyncStatus } from '../types';
import type { Language, Translations } from '../i18n/types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes, formatDate, formatDimensions, formatRelativeTime, formatVolumeCm3 } from '../i18n/format';
import { ModelViewer } from './ModelViewer';

interface Props {
  model: ModelFile | null;
  onAddTag: (tag: string) => void;
  onRemoveTag: (tag: string) => void;
  onDelete: () => void;
  onOpenInSlicer: (slicerId?: string) => void;
  slicers: SlicerConfig[];
  slicerError: string | null;
}

type TFunction = <K extends keyof Translations>(key: K) => Translations[K];

const SYNC_KEYS: Record<SyncStatus, 'syncSynced' | 'syncOutdated' | 'syncLocalOnly' | 'syncCloudOnly'> = {
  synced: 'syncSynced',
  outdated: 'syncOutdated',
  'local-only': 'syncLocalOnly',
  'cloud-only': 'syncCloudOnly',
};

function buildMetaRows(model: ModelFile, t: TFunction, language: Language): { label: string; value: string }[] {
  const materialsValue =
    model.materials.length === 0
      ? t('noValue')
      : model.materials.map((m) => m.name).join(', ');
  const objectCountValue = model.objectCount === null ? t('noValue') : String(model.objectCount);

  return [
    { label: t('metaDimensions'), value: formatDimensions(model.dimensionsMm, language) },
    { label: t('metaVolume'), value: formatVolumeCm3(model.volumeCm3, language) },
    { label: t('metaObjectCount'), value: objectCountValue },
    { label: t('metaMaterial'), value: materialsValue },
    { label: t('metaFileSize'), value: formatBytes(model.fileSizeBytes, language) },
    { label: t('metaImported'), value: formatDate(model.importedAt, language) },
  ];
}

export function DetailPanel({
  model,
  onAddTag,
  onRemoveTag,
  onDelete,
  onOpenInSlicer,
  slicers,
  slicerError,
}: Props) {
  const { language } = useLanguage();
  const t = useT();
  const [draft, setDraft] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [slicerMenuOpen, setSlicerMenuOpen] = useState(false);

  useEffect(() => {
    setConfirmDelete(false);
    setSlicerMenuOpen(false);
  }, [model?.id]);

  if (!model) {
    return (
      <aside className="flex-none w-[336px] flex items-center justify-center bg-[var(--panel)] border-l border-[var(--line)] text-[var(--ink-3)] text-[13px] px-6 text-center">
        {t('emptyStateText')}
      </aside>
    );
  }

  const submitDraft = () => {
    const value = draft.trim().replace(/^#/, '');
    if (value) onAddTag(value);
    setDraft('');
  };

  const hasSlicers = slicers.length > 0;

  return (
    <aside className="flex-none w-[336px] flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
      <div className="flex-none px-4 pt-3.5 pb-3 border-b border-[var(--line)]">
        <div className="text-[14.5px] font-semibold leading-tight break-words">{model.name}</div>
        <div className="font-mono-ui text-[10.5px] text-[var(--ink-3)] pt-1.5">{model.path}</div>
      </div>

      <div className="flex-1 overflow-y-auto">
        <div className="relative aspect-[4/3] bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
          <div
            className="absolute inset-0"
            style={{
              backgroundImage:
                'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 11px)',
            }}
          />
          <ModelViewer fileId={model.id} />
          <div className="absolute left-2.5 bottom-2 font-mono-ui text-[9.5px] tracking-[0.08em] uppercase text-[var(--ink-3)] pointer-events-none">
            {t('dragToRotate')}
          </div>
        </div>

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span
            className={`w-2 h-2 rounded-full ${
              model.sync === 'synced' ? 'bg-[var(--accent)]' : 'bg-[var(--ink-3)]'
            }`}
          />
          <span className="flex-1 text-[12.5px] font-medium">{t(SYNC_KEYS[model.sync])}</span>
          <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">
            {formatRelativeTime(model.importedAt, language)}
          </span>
        </div>

        <div className="px-4 pt-3.5 pb-1">
          <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2">
            {t('metadataHeading')}
          </div>
          {buildMetaRows(model, t, language).map((row) => (
            <div
              key={row.label}
              className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]"
            >
              <span className="flex-none w-[108px] text-[12.5px] text-[var(--ink-2)]">
                {row.label}
              </span>
              <span className="flex-1 font-mono-ui text-xs text-right">{row.value}</span>
            </div>
          ))}
        </div>

        <div className="px-4 pt-[18px] pb-5">
          <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2.5">
            {t('hashtagsHeading')}
          </div>
          <div className="flex flex-wrap gap-1.5">
            {model.tags.map((tag) => (
              <span
                key={tag}
                className="inline-flex items-center gap-1.5 h-6 pl-2.5 pr-1 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11.5px]"
              >
                #{tag}
                <span
                  onClick={() => onRemoveTag(tag)}
                  className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[10px] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                >
                  ✕
                </span>
              </span>
            ))}
            <input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && submitDraft()}
              placeholder={t('addTagPlaceholder')}
              className="h-6 w-[118px] px-2.5 rounded-full border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 font-mono-ui text-[11.5px]"
            />
          </div>
        </div>
      </div>

      <div className="flex-none px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
        {slicerError && (
          <div className="pb-2 font-mono-ui text-[10px] text-[var(--accent)] break-words">
            {t('slicerLaunchError')} {slicerError}
          </div>
        )}
        <div className="flex gap-2">
          {confirmDelete ? (
            <>
              <span className="flex-1 flex items-center text-[12.5px] font-medium text-[var(--ink)]">
                {t('deleteConfirmQuestion')}
              </span>
              <button
                onClick={() => setConfirmDelete(false)}
                className="flex-none h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('cancel')}
              </button>
              <button
                onClick={() => {
                  setConfirmDelete(false);
                  onDelete();
                }}
                className="flex-none h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
              >
                {t('delete')}
              </button>
            </>
          ) : (
            <>
              <div className="relative flex flex-1">
                <button
                  onClick={() => onOpenInSlicer()}
                  className={`flex-1 h-8 border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] ${
                    hasSlicers ? 'rounded-l-[3px] border-r-0' : 'rounded-[3px]'
                  }`}
                >
                  {t('openInSlicer')}
                </button>
                {hasSlicers && (
                  <button
                    onClick={() => setSlicerMenuOpen((o) => !o)}
                    aria-label={t('chooseSlicerAria')}
                    className="flex items-center justify-center w-6 h-8 rounded-r-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                  >
                    <span className="text-[9px] leading-none">▾</span>
                  </button>
                )}
                {slicerMenuOpen && (
                  <div className="absolute bottom-10 left-0 w-[176px] py-1 bg-[var(--panel)] border border-[var(--line)] rounded-[3px] shadow-[var(--shadow)] z-40">
                    {slicers.map((s) => (
                      <button
                        key={s.id}
                        onClick={() => {
                          setSlicerMenuOpen(false);
                          onOpenInSlicer(s.id);
                        }}
                        className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
                      >
                        {s.name}
                      </button>
                    ))}
                  </div>
                )}
              </div>
              <button className="flex-none w-[34px] h-8 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] font-mono-ui cursor-pointer">
                ↻
              </button>
              <button
                onClick={() => setConfirmDelete(true)}
                aria-label={t('deleteAriaLabel')}
                className="flex-none w-[34px] h-8 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] font-mono-ui cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                ✕
              </button>
            </>
          )}
        </div>
      </div>
    </aside>
  );
}
```

- [ ] **Step 2: Kompilierung prüfen**

Run: `npx tsc --noEmit`
Expected: Fehler ausschließlich in `App.tsx` (fehlende neue Props beim
`<DetailPanel ... />`-Aufruf, plus die bereits aus Task 4 bekannten
`<Header ... />`-Fehler), keine Fehler in `DetailPanel.tsx` selbst.

- [ ] **Step 3: Commit**

```bash
git add src/components/DetailPanel.tsx
git commit -m "$(cat <<'EOF'
Frontend: "In Slicer oeffnen" als Split-Button

Hauptklick startet den zuletzt genutzten Slicer direkt, der Pfeil
daneben (nur sichtbar wenn mindestens ein Slicer konfiguriert ist)
oeffnet ein Dropdown zur Auswahl eines anderen - gleiches visuelles
Muster wie der Import-Button im Header, oeffnet aber nach oben statt
nach unten, da der Button im Footer sitzt. Fehler beim Start werden
jetzt angezeigt statt verschluckt.

Macht App.tsx weiterhin nicht kompilierbar (behoben im naechsten
Commit).

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

---

### Task 6: `App.tsx` verdrahten, Live-Test

**Files:**
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `useSlicers` (Task 1), `pick_slicer_executable`/`open_in_slicer`-Commands (Task 2), `Header`s neue Props (Task 4), `DetailPanel`s neue Props (Task 5).
- Produces: nichts (Blatt-Task, letzter Schritt des Plans).

- [ ] **Step 1: Hook importieren, neuen State ergänzen**

In `src/App.tsx`, den Import-Block erweitern:

```tsx
import { useTheme } from './hooks/useTheme';
import { useSlicers } from './hooks/useSlicers';
```

Innerhalb von `export default function App() {`, nach der Zeile
`const { setting, setTheme } = useTheme();`, ergänzen:

```tsx
  const { slicers, lastUsedId, addSlicer, removeSlicer, setLastUsed } = useSlicers();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [slicerError, setSlicerError] = useState<string | null>(null);
```

- [ ] **Step 2: `openInSlicer` neu implementieren**

Ersetze den bisherigen Stub:

```tsx
  const openInSlicer = (_id: string) => {
    // Tauri: Pfad an registrierten Slicer übergeben
  };
```

durch:

```tsx
  const openInSlicer = (id: string, slicerId?: string) => {
    const model = models.find((m) => m.id === id);
    if (!model) return;
    if (slicers.length === 0) {
      setSettingsOpen(true);
      return;
    }
    const target = slicerId
      ? slicers.find((s) => s.id === slicerId)
      : slicers.find((s) => s.id === lastUsedId) ?? slicers[0];
    if (!target) {
      setSettingsOpen(true);
      return;
    }
    setLastUsed(target.id);
    setSlicerError(null);
    invoke('open_in_slicer', { slicerPath: target.path, filePath: model.path }).catch((e) => {
      console.error('[slicer] Start fehlgeschlagen:', e);
      setSlicerError(String(e));
    });
  };
```

- [ ] **Step 3: Neue Props an `Header` übergeben**

Im `<Header ... />`-Aufruf, nach `onImportFromCloud={() => setCloudBrowserOpen(true)}`,
ergänzen:

```tsx
        onImportFromCloud={() => setCloudBrowserOpen(true)}
        settingsOpen={settingsOpen}
        onSettingsOpenChange={setSettingsOpen}
        slicers={slicers}
        onAddSlicer={addSlicer}
        onRemoveSlicer={removeSlicer}
      />
```

- [ ] **Step 4: Neue Props an `DetailPanel` übergeben**

Ersetze im `<DetailPanel ... />`-Aufruf die Zeile
`onOpenInSlicer={() => selected && openInSlicer(selected.id)}` und ergänze
die zwei neuen Props:

```tsx
        <DetailPanel
          model={selected}
          onAddTag={(t) => selected && addTag(selected.id, t)}
          onRemoveTag={(t) => selected && removeTag(selected.id, t)}
          onDelete={() => selected && deleteModel(selected.id)}
          onOpenInSlicer={(slicerId) => selected && openInSlicer(selected.id, slicerId)}
          slicers={slicers}
          slicerError={slicerError}
        />
```

- [ ] **Step 5: `ContextMenu`-Aufrufstelle prüfen**

Die bestehende Zeile

```tsx
          onOpenInSlicer={() => openInSlicer(contextMenu.modelId)}
```

im `<ContextMenu ... />`-Aufruf **bleibt unverändert** - `openInSlicer`
akzeptiert jetzt ein optionales zweites Argument, der bestehende
Ein-Argument-Aufruf ruft es weiterhin korrekt mit `slicerId === undefined`
auf (nutzt also automatisch den zuletzt verwendeten Slicer). Kein Edit
nötig, nur zur Bestätigung hier aufgeführt.

- [ ] **Step 6: TypeScript-Kompilierung verifizieren**

Run: `npx tsc --noEmit`
Expected: keine Fehler.

- [ ] **Step 7: Commit**

```bash
git add src/App.tsx
git commit -m "$(cat <<'EOF'
Frontend: "In Slicer oeffnen" vollstaendig verdrahtet

openInSlicer nutzt jetzt useSlicers() statt eines leeren Stubs:
startet den per slicerId gewaehlten oder sonst den zuletzt genutzten
Slicer, oeffnet bei noch keinem konfigurierten Slicer stattdessen das
Einstellungen-Panel. settingsOpen ist jetzt App-weiter State, damit
DetailPanel/ContextMenu das Panel darueber oeffnen koennen. Fehler
beim Start landen in slicerError und werden im DetailPanel angezeigt.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Tq9SLWAyBWqksEvLX3a4r7
EOF
)"
```

- [ ] **Step 8: Manueller Live-Test (Abschlusskriterium des gesamten Plans)**

Dieser Schritt lässt sich nicht automatisieren:

1. `npm run tauri dev` starten.
2. Im ⚙-Einstellungsmenü den neuen "Slicer"-Abschnitt prüfen: leer beim
   ersten Start, "+ Hinzufügen" öffnet den nativen Datei-Dialog. Einen
   beliebigen ausführbaren Pfad wählen (z. B. `/usr/bin/true` zum Testen,
   oder einen echten installierten Slicer), Name bestätigen, Eintrag
   erscheint in der Liste.
3. Ein Modell auswählen, "In Slicer öffnen" im DetailPanel klicken - der
   gewählte Slicer sollte starten (bei `/usr/bin/true` passiert sichtbar
   nichts, aber es darf auch keine Fehlermeldung erscheinen).
4. Einen zweiten Slicer-Pfad hinzufügen, im DetailPanel den Pfeil neben
   "In Slicer öffnen" klicken, den zweiten aus dem Dropdown wählen -
   startet ihn und wird ab jetzt beim Hauptklick verwendet.
5. Rechtsklick auf ein Modell im Grid/in der Liste → "In Slicer öffnen" im
   Kontextmenü - startet den zuletzt genutzten Slicer direkt, kein
   Untermenü.
6. Alle konfigurierten Slicer im Einstellungsmenü wieder entfernen, dann
   erneut "In Slicer öffnen" klicken (egal an welcher Stelle) - öffnet
   automatisch das Einstellungsmenü statt einer Fehlermeldung.
7. Einen Slicer mit einem absichtlich falschen/gelöschten Pfad
   konfigurieren und "In Slicer öffnen" klicken - die Fehlermeldung
   erscheint im DetailPanel statt stillschweigend nichts zu tun.
