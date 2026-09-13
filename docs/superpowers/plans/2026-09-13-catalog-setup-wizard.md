# Katalog-Speicherort-Einrichtung Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ein Ersteinrichtungsdialog erklärt die neue Ordner-Bedeutung und lässt den Nutzer zwischen "bestehende Struktur übernehmen", "neuen Speicherort einrichten" oder "später entscheiden" wählen. Ab dann berücksichtigt "Dateien importieren" den aktiven Ordner bzw. den konfigurierten Speicherort statt blind in die Wurzel zu importieren.

**Architektur:** Zwei kleine neue Tauri-Commands (`pick_folder_path`, `register_catalog_base_dir`), ein neuer `localStorage`-Hook (`useCatalogBaseDir`, Muster wie `useDisplayPreference`), ein neuer Modal-Dialog (`CatalogSetupDialog.tsx`, Muster wie `CatalogCleanupDialog.tsx`), Verdrahtung in `App.tsx` (Erststart-Trigger + Platzierungslogik nach `import_files`) und ein neuer Abschnitt im Einstellungen-Panel (`Rail.tsx`).

**Tech Stack:** Rust (Tauri Commands, bereits vorhandene `db::ensure_folder_path`), React/TypeScript, `localStorage`.

## Global Constraints

- Design ist bereits als Spec verabschiedet:
  `docs/superpowers/specs/2026-09-13-catalog-setup-wizard-design.md` —
  Ablauf, Command-Signaturen und deutsche Dialogtexte sind dort
  wörtlich vorgegeben und 1:1 zu übernehmen.
- Kein neuer Backend-Persistenzmechanismus — `catalogBaseDir` lebt
  ausschließlich in `localStorage`, exakt nach dem Muster von
  `src/hooks/useDisplayPreference.ts`.
- "Ordner importieren"/"Ordner als Sammlung importieren" bleiben
  unverändert — nur "Dateien importieren" (`import_files`) bekommt neues
  Platzierungsverhalten, ausschließlich frontend-seitig nach dem
  bestehenden `invoke('import_files')`-Aufruf.
- Ohne konfigurierten Speicherort (`catalogBaseDir === null`) ändert sich
  am Verhalten nichts — keine Überraschung für Nutzer, die den Dialog
  wegklicken.
- `npx tsc --noEmit` und `cargo test` (aus `src-tauri/`) müssen nach
  jedem Task sauber durchlaufen.
- Alle neuen Nutzertexte in allen vier Sprachdateien
  (`de.ts`/`en.ts`/`es.ts`/`fr.ts`) plus `types.ts` ergänzen — deutsche
  Texte aus der Spec wörtlich übernehmen, die anderen drei Sprachen
  sinngemäß und natürlich übersetzen (keine Wort-für-Wort-Übertragung).
- Kein AppImage-Rebuild als Teil dieses Plans.

---

### Task 1: Backend-Commands `pick_folder_path` und `register_catalog_base_dir`

**Files:**
- Modify: `src-tauri/src/commands.rs` (zwei neue `#[tauri::command]`-Funktionen)
- Modify: `src-tauri/src/lib.rs` (Registrierung in `tauri::generate_handler![...]`)

**Interfaces:**
- Consumes: `db::ensure_folder_path` (bereits vorhanden, aus dem
  Ordnerstruktur-Plan), `FolderDto` (bereits vorhanden).
- Produces: `pick_folder_path() -> CmdResult<Option<String>>`,
  `register_catalog_base_dir(path: String) -> CmdResult<FolderDto>` —
  von Task 3 (Frontend-Verdrahtung) konsumiert.

- [ ] **Step 1: `pick_folder_path` implementieren**

Direkt nach `pick_slicer_executable` (commands.rs, ca. Zeile 1494-1506)
einfügen:

```rust
#[tauri::command]
pub async fn pick_folder_path(app: tauri::AppHandle) -> CmdResult<Option<String>> {
    let picked = app.dialog().file().blocking_pick_folder();
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().to_string()))
}
```

- [ ] **Step 2: `register_catalog_base_dir` implementieren**

```rust
#[tauri::command]
pub fn register_catalog_base_dir(state: State<AppState>, path: String) -> CmdResult<FolderDto> {
    let dir = std::path::PathBuf::from(&path);
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let conn = lock_db(&state)?;
    let id = db::ensure_folder_path(&conn, &dir, &dir).map_err(|e| e.to_string())?;

    let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
    let folder = folders.iter().find(|f| f.id == id).ok_or_else(|| "folder not found".to_string())?;

    Ok(FolderDto {
        id: folder.id.to_string(),
        name: folder.name.clone(),
        path: folder.path.clone(),
        parent_id: folder.parent_id.map(|p| p.to_string()),
        count: 0,
    })
}
```

- [ ] **Step 3: `open_in_file_manager` implementieren**

Direkt nach `register_catalog_base_dir` einfügen (analog zum
Rohmuster von `open_in_slicer`, keine neue Plugin-Abhängigkeit,
`#[cfg]` direkt auf dem `let`-Statement wie beim bereits vorhandenen
`pick_slicer_executable`-Windows-Sonderfall):

```rust
#[tauri::command]
pub fn open_in_file_manager(path: String) -> CmdResult<()> {
    #[cfg(target_os = "linux")]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("explorer");

    cmd.arg(&path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 4: Alle drei Commands in `lib.rs` registrieren**

In der `tauri::generate_handler![...]`-Liste (dort, wo `create_folder`/
`move_file_to_folder`/`list_folders` bereits eingetragen sind)
`pick_folder_path`, `register_catalog_base_dir` und
`open_in_file_manager` ergänzen.

- [ ] **Step 5: Rust-Test für `register_catalog_base_dir`**

Im bestehenden `#[cfg(test)] mod tests`-Block in `commands.rs` (siehe
`unique_test_dir`-Helper aus vorherigen Tasks):

```rust
#[test]
fn register_catalog_base_dir_creates_missing_directory_and_is_idempotent() {
    let base = unique_test_dir("register-base-dir");
    let path_str = base.to_string_lossy().to_string();
    assert!(!base.exists());

    let state = test_app_state();
    let first = register_catalog_base_dir(state_ref(&state), path_str.clone()).unwrap();
    assert!(base.exists());
    assert_eq!(first.name, base.file_name().unwrap().to_string_lossy());
    assert_eq!(first.parent_id, None);

    let second = register_catalog_base_dir(state_ref(&state), path_str).unwrap();
    assert_eq!(first.id, second.id);

    let conn = state.db.lock().unwrap();
    assert_eq!(db::list_folders(&conn).unwrap().len(), 1);

    std::fs::remove_dir_all(&base).ok();
}
```

Implementierer: exakten Aufruf-Stil für `test_app_state()`/`state_ref()`
an die tatsächlich in `commands.rs` vorhandenen Test-Helper anpassen
(dieselben, die die Tests aus dem Ordnerstruktur-Plan bereits nutzen,
z. B. bei `move_file_to_folder_updates_path_and_db`).

- [ ] **Step 6: Tests + Commit**

```bash
cd src-tauri && cargo test
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "Backend: pick_folder_path, register_catalog_base_dir und open_in_file_manager Commands"
```

---

### Task 2: `useCatalogBaseDir`-Hook und i18n-Texte

**Files:**
- Create: `src/hooks/useCatalogBaseDir.ts`
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`, `src/i18n/types.ts`

**Interfaces:**
- Produces: `useCatalogBaseDir() -> { catalogBaseDir: string | null, setCatalogBaseDir: (path: string | null) => void, setupSeen: boolean, markSetupSeen: () => void }`
  — von Task 3 (Dialog-Komponente) und Task 4 (App.tsx-Verdrahtung)
  konsumiert.

- [ ] **Step 1: Hook erstellen**

```ts
import { useCallback, useState } from 'react';

const BASE_DIR_KEY = '3mf-katalog-base-dir';
const SETUP_SEEN_KEY = '3mf-katalog-setup-seen';

/**
 * Verwaltet den optionalen Katalog-Speicherort (Zielordner fuer neu
 * importierte Einzeldateien) und ob der Ersteinrichtungsdialog schon
 * gesehen/entschieden wurde. Persistiert in localStorage nach demselben
 * Muster wie Theme/Dichte/Anzeige-Praeferenz (siehe useDisplayPreference.ts).
 */
export function useCatalogBaseDir() {
  const [catalogBaseDir, setCatalogBaseDirState] = useState<string | null>(() =>
    localStorage.getItem(BASE_DIR_KEY),
  );
  const [setupSeen, setSetupSeenState] = useState<boolean>(
    () => localStorage.getItem(SETUP_SEEN_KEY) === '1',
  );

  const setCatalogBaseDir = useCallback((path: string | null) => {
    setCatalogBaseDirState(path);
    if (path) localStorage.setItem(BASE_DIR_KEY, path);
    else localStorage.removeItem(BASE_DIR_KEY);
  }, []);

  const markSetupSeen = useCallback(() => {
    setSetupSeenState(true);
    localStorage.setItem(SETUP_SEEN_KEY, '1');
  }, []);

  return { catalogBaseDir, setCatalogBaseDir, setupSeen, markSetupSeen };
}
```

- [ ] **Step 2: i18n-Keys ergänzen**

In jeder der vier Sprachdateien plus `types.ts` (Position: nach den
`catalogBackup*`-Keys, gleiches Namens-Präfix-Muster):

```
catalogSetupTitle: 'Wie soll dein Katalog organisiert sein?',
catalogSetupIntro: 'Ordner im Katalog sind jetzt echte Verzeichnisse auf deiner Festplatte. Verschiebst du eine Datei im Programm in einen anderen Ordner, wird sie dort auch tatsächlich abgelegt – nicht nur im Katalog umsortiert. Damit neu importierte Dateien sinnvoll einsortiert werden, legen wir jetzt einen festen Speicherort fest.',
catalogSetupFileTypesNote: 'Erfasst werden ausschließlich .3mf- und .stl-Dateien. Bereits gepackte Archive (z. B. .zip) werden dabei nicht berücksichtigt und bleiben unverändert im Ordner liegen – entpacke sie bei Bedarf vorher, oder öffne den Ordner nach der Einrichtung direkt über den Katalog im Dateimanager.',
catalogSetupAdoptTitle: 'Bestehende Ordnerstruktur übernehmen',
catalogSetupAdoptDescription: 'Du organisierst deine Druckdateien schon in Ordnern? Wähle den obersten Ordner aus – der Katalog übernimmt die komplette Struktur inklusive aller Unterordner und importiert alle enthaltenen 3mf-/STL-Dateien.',
catalogSetupNewTitle: 'Neuen Ort einrichten',
catalogSetupNewDescription: 'Leg einen (auch leeren) Ordner fest, in dem der Katalog ab jetzt neu importierte Dateien ablegt. Unterordner kannst du später jederzeit im Programm anlegen und Dateien per Drag & Drop einsortieren.',
catalogSetupLater: 'Später einrichten',
catalogSetupFootnote: 'Du kannst diese Wahl jederzeit in den Einstellungen unter „Katalog-Speicherort" ändern.',
catalogSetupImporting: 'Importiere…',
catalogSetupSettingUp: 'Richte ein…',
catalogSetupAdoptSummary: '{files} Dateien in {folders} Ordnern importiert.',
catalogSetupNewSummary: '„{path}" als Speicherort eingerichtet.',
catalogSetupOpenFolderButton: 'Ordner im Dateimanager öffnen',
catalogSetupDoneButton: 'Fertig',
catalogSetupError: 'Fehler:',
catalogBaseDirSectionTitle: 'Katalog-Speicherort',
catalogBaseDirNotSet: 'Nicht eingerichtet',
catalogBaseDirChangeButton: 'Ändern',
catalogBaseDirSetupButton: 'Einrichten',
catalogBaseDirOpenButton: 'Ordner öffnen',
```

Englische, spanische und französische Entsprechungen sinngemäß (nicht
wörtlich) formulieren, gleiche Tonalität (klar, direkt, keine
Fachbegriffe ohne Erklärung). `{files}`/`{folders}`/`{path}` per
`String.replace('{files}', ...)` usw. befüllen, gleiches Platzhalter-
Muster wie das bereits vorhandene `bulkSelectedCount` (`{count}`) in
`de.ts`/`App.tsx` — dort nachschlagen statt zu raten.

- [ ] **Step 3: Typecheck + Commit**

```bash
npx tsc --noEmit
git add src/hooks/useCatalogBaseDir.ts src/i18n/
git commit -m "Frontend: useCatalogBaseDir-Hook und i18n-Texte fuer Katalog-Einrichtung"
```

---

### Task 3: `CatalogSetupDialog.tsx`

**Files:**
- Create: `src/components/CatalogSetupDialog.tsx`

**Interfaces:**
- Consumes: `useCatalogBaseDir()` (Task 2), `pick_folder_path`/
  `register_catalog_base_dir`/`import_dropped`/`list_folders`/
  `open_in_file_manager` (Tauri-Commands, Task 1 bzw. bereits vorhanden).
- Produces: `<CatalogSetupDialog onClose={() => void} onImported={(result: ImportResultDto) => void} onBaseDirSet={(path: string) => void} onLater={() => void} />`
  — von Task 4 (App.tsx-Verdrahtung) konsumiert.

- [ ] **Step 1: Komponente erstellen**

Visuelles Muster wie `CatalogCleanupDialog.tsx` (Overlay + zentriertes
Panel), aber `w-[560px]`. Zustandsmaschine: `idle` (beide Karten aktiv)
→ `busy` (eine Karte lädt) → `done` (Karten werden durch eine
Zusammenfassung + "Ordner öffnen"/"Fertig" ersetzt) ODER zurück zu
`idle` mit `error` gesetzt (Karten bleiben nutzbar, erneuter Versuch
möglich):

```tsx
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n/LanguageContext';
import type { ImportResultDto, Folder } from '../types';

interface Props {
  onClose: () => void;
  onLater: () => void;
  onImported: (result: ImportResultDto) => void;
  onBaseDirSet: (path: string) => void;
}

type Done =
  | { kind: 'adopt'; path: string; files: number; folders: number }
  | { kind: 'new'; path: string };

export function CatalogSetupDialog({ onClose, onLater, onImported, onBaseDirSet }: Props) {
  const t = useT();
  const [busy, setBusy] = useState<'adopt' | 'new' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<Done | null>(null);

  const adoptExisting = async () => {
    setError(null);
    const path = await invoke<string | null>('pick_folder_path');
    if (!path) return;
    setBusy('adopt');
    try {
      const before = await invoke<Folder[]>('list_folders');
      const result = await invoke<ImportResultDto>('import_dropped', { paths: [path] });
      const after = await invoke<Folder[]>('list_folders');
      const newFolders = after.filter((f) => !before.some((b) => b.id === f.id)).length;

      onBaseDirSet(path);
      onImported(result);
      setDone({ kind: 'adopt', path, files: result.imported.length, folders: newFolders });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const setupNew = async () => {
    setError(null);
    const path = await invoke<string | null>('pick_folder_path');
    if (!path) return;
    setBusy('new');
    try {
      await invoke('register_catalog_base_dir', { path });
      onBaseDirSet(path);
      setDone({ kind: 'new', path });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const openFolder = (path: string) => {
    invoke('open_in_file_manager', { path }).catch((e) => console.error('[catalog-setup] Ordner oeffnen fehlgeschlagen:', e));
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-[560px] max-h-[80vh] flex flex-col bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] overflow-y-auto">
        <div className="px-5 py-4">
          <div className="text-[16px] font-semibold mb-2.5">{t('catalogSetupTitle')}</div>
          <p className="text-[13px] leading-relaxed text-[var(--ink-2)] mb-4">{t('catalogSetupIntro')}</p>

          {!done && (
            <div className="mb-4 p-3 rounded-[8px] border border-dashed border-[var(--line-strong)] text-[12px] leading-relaxed text-[var(--ink-2)]">
              {t('catalogSetupFileTypesNote')}
            </div>
          )}

          {done ? (
            <div className="mb-4 p-4 rounded-[10px] border-2 border-[var(--good)] bg-[var(--good-soft)]">
              <div className="text-[13px] font-semibold text-[var(--good)] mb-3">
                ✓{' '}
                {done.kind === 'adopt'
                  ? t('catalogSetupAdoptSummary').replace('{files}', String(done.files)).replace('{folders}', String(done.folders))
                  : t('catalogSetupNewSummary').replace('{path}', done.path)}
              </div>
              <div className="flex gap-2">
                <button
                  onClick={() => openFolder(done.path)}
                  className="h-8 px-3 rounded-[6px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                >
                  {t('catalogSetupOpenFolderButton')}
                </button>
                <button
                  onClick={onClose}
                  className="h-8 px-3 rounded-[6px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
                >
                  {t('catalogSetupDoneButton')}
                </button>
              </div>
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-3 mb-4">
              <button
                onClick={adoptExisting}
                disabled={busy !== null}
                className="text-left p-4 rounded-[10px] border-2 border-[var(--line)] hover:border-[var(--accent)] disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer bg-[var(--panel-2)]"
              >
                <div className="text-[13.5px] font-semibold mb-1.5">{t('catalogSetupAdoptTitle')}</div>
                <div className="text-[12px] leading-relaxed text-[var(--ink-2)]">
                  {busy === 'adopt' ? t('catalogSetupImporting') : t('catalogSetupAdoptDescription')}
                </div>
              </button>
              <button
                onClick={setupNew}
                disabled={busy !== null}
                className="text-left p-4 rounded-[10px] border-2 border-[var(--line)] hover:border-[var(--accent)] disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer bg-[var(--panel-2)]"
              >
                <div className="text-[13.5px] font-semibold mb-1.5">{t('catalogSetupNewTitle')}</div>
                <div className="text-[12px] leading-relaxed text-[var(--ink-2)]">
                  {busy === 'new' ? t('catalogSetupSettingUp') : t('catalogSetupNewDescription')}
                </div>
              </button>
            </div>
          )}

          {error && (
            <div className="mb-3 font-mono-ui text-[11px] text-[var(--accent)] break-words">
              {t('catalogSetupError')} {error}
            </div>
          )}

          {!done && (
            <>
              <div className="flex items-center justify-between">
                <span
                  onClick={busy === null ? onLater : undefined}
                  className={`text-[12px] text-[var(--ink-3)] underline ${busy === null ? 'cursor-pointer hover:text-[var(--accent)]' : 'opacity-50'}`}
                >
                  {t('catalogSetupLater')}
                </span>
              </div>
              <p className="mt-3 text-[11px] text-[var(--ink-3)]">{t('catalogSetupFootnote')}</p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
```

Implementierer: `ImportResultDto`-/`Folder`-Typ-Import-Pfad an die tatsächliche
Stelle in `src/types.ts` (bzw. `src/types/index.ts`) anpassen, falls
abweichend.

- [ ] **Step 2: Typecheck + Commit**

```bash
npx tsc --noEmit
git add src/components/CatalogSetupDialog.tsx
git commit -m "Frontend: CatalogSetupDialog-Komponente"
```

---

### Task 4: Verdrahtung in `App.tsx` und `Rail.tsx`

**Files:**
- Modify: `src/App.tsx` (Dialog-Trigger, Platzierungslogik nach `import_files`)
- Modify: `src/components/Rail.tsx` (neuer Einstellungen-Abschnitt)

**Interfaces:**
- Consumes: `useCatalogBaseDir` (Task 2), `CatalogSetupDialog` (Task 3),
  bestehende `folders`/`activeFolderId`/`importFiles`-States/Handler aus
  `App.tsx`, bestehender `move_file_to_folder`-Command.

- [ ] **Step 1: Hook einbinden, Dialog-State**

In `App.tsx`:
```ts
const { catalogBaseDir, setCatalogBaseDir, setupSeen, markSetupSeen } = useCatalogBaseDir();
const [setupDialogOpen, setSetupDialogOpen] = useState(!setupSeen);
```

- [ ] **Step 2: Platzierungslogik nach `import_files`**

Implementierer: die bestehende `importFiles`-Funktion in `App.tsx`
lokalisieren (ruft aktuell `invoke('import_files')` auf und verarbeitet
das `ImportResultDto` über `mergeImported` o. ä. — exakten Namen aus dem
tatsächlichen Code übernehmen). Direkt nach dem bestehenden
Erfolgspfad ergänzen:

```ts
if (catalogBaseDir) {
  const targetFolder =
    activeFolderId !== 'all'
      ? activeFolderId
      : folders.find((f) => f.path === catalogBaseDir && !f.parentId)?.id;
  if (targetFolder) {
    result.imported.forEach((file) => {
      invoke('move_file_to_folder', { fileId: file.id, folderId: targetFolder }).catch((e) =>
        console.error('[import] Einsortieren fehlgeschlagen:', e),
      );
    });
    refreshFolders();
  }
}
```

Implementierer: exakten Variablennamen für das `ImportResultDto`-Ergebnis
und den bestehenden `refreshFolders`-Aufruf an den tatsächlichen Code
anpassen (nicht raten — `App.tsx` vor dieser Änderung lesen).

- [ ] **Step 3: Dialog rendern**

Am Ende des Haupt-JSX (Geschwister-Ebene zu anderen Overlay-Dialogen wie
`CatalogCleanupDialog`, falls vorhanden — sonst direkt vor dem
schließenden Root-`</div>`):

```tsx
{setupDialogOpen && (
  <CatalogSetupDialog
    onClose={() => setSetupDialogOpen(false)}
    onLater={() => {
      markSetupSeen();
      setSetupDialogOpen(false);
    }}
    onImported={(result) => {
      markSetupSeen();
      mergeImported(result); // bestehende Funktion, exakten Namen pruefen
    }}
    onBaseDirSet={(path) => {
      setCatalogBaseDir(path);
      markSetupSeen();
    }}
  />
)}
```

- [ ] **Step 4: `Rail.tsx` — Einstellungen-Abschnitt**

Neue Props `catalogBaseDir: string | null`, `onOpenCatalogSetup: () => void`
im `Rail`-Props-Interface ergänzen, in `App.tsx`s `<Rail>`-Aufruf
durchreichen (`onOpenCatalogSetup={() => setSetupDialogOpen(true)}`).

Im Einstellungen-Panel, nach dem "Katalog-Backup"-Abschnitt (letzter
Abschnitt vor dem schließenden `</div>` des Panels):

```tsx
<div className="text-[length:var(--font-size-body)] font-semibold mt-4 mb-2">{t('catalogBaseDirSectionTitle')}</div>
<div className="font-mono-ui text-[10.5px] text-[var(--ink-3)] truncate mb-1.5">
  {catalogBaseDir ?? t('catalogBaseDirNotSet')}
</div>
<div className="flex gap-1.5">
  <button
    onClick={onOpenCatalogSetup}
    className="flex-1 h-7 rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
  >
    {catalogBaseDir ? t('catalogBaseDirChangeButton') : t('catalogBaseDirSetupButton')}
  </button>
  {catalogBaseDir && (
    <button
      onClick={() => invoke('open_in_file_manager', { path: catalogBaseDir })}
      className="flex-1 h-7 rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
    >
      {t('catalogBaseDirOpenButton')}
    </button>
  )}
</div>
```

`invoke` ist in `Rail.tsx` bereits importiert (wird für
`pick_slicer_executable` genutzt) — kein neuer Import nötig.

- [ ] **Step 5: Typecheck**

```bash
npx tsc --noEmit
```

- [ ] **Step 6: Manueller Test (isolierte `XDG_DATA_HOME`)**

```bash
XDG_DATA_HOME=/tmp/setup-wizard-test npm run tauri dev
```
Prüfen: Dialog erscheint beim ersten Start. "Später einrichten" schließt
ihn dauerhaft (App neu starten → kein erneutes Aufpoppen). "Neuen Ort
einrichten" mit einem leeren Testordner zeigt die Erfolgszusammenfassung
mit dem gewählten Pfad (kein Datei-/Ordner-Zähler, da nichts importiert
wurde); "Ordner im Dateimanager öffnen" öffnet nachweislich einen echten
Dateimanager (z. B. Dolphin) an diesem Pfad. Anschließend "Dateien
importieren" (Einzeldatei aus einem anderen Ort) → Datei landet
nachweislich physisch im gewählten Ordner (`ls` im Zielordner prüfen).

"Bestehende Struktur übernehmen" mit einem vorbereiteten zweistufigen
Testordner, der ZUSÄTZLICH eine `.zip`-Datei auf oberster Ebene enthält
→ komplette `.3mf`/`.stl`-Struktur erscheint im Ordner-Baum, die
Erfolgsmeldung zeigt korrekte Datei- UND Ordneranzahl, die `.zip`-Datei
bleibt unangetastet am Ursprungsort liegen (`ls`/`md5sum` vorher/nachher
vergleichen) und taucht nirgends im Katalog auf.

Einstellungen-Panel: "Ordner öffnen" neben "Ändern" erscheint nur, wenn
ein Speicherort konfiguriert ist, und öffnet denselben Pfad.

- [ ] **Step 7: Commit**

```bash
git add src/App.tsx src/components/Rail.tsx
git commit -m "Frontend: Katalog-Einrichtungsdialog verdrahtet (Erststart, Einstellungen, importbewusste Platzierung)"
```
