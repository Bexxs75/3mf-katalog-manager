# Komfort-UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a second, deutlich lesbarere "Komfort"-Ansicht neben der bestehenden kompakten Oberfläche, umschaltbar im Einstellungsmenü, plus ein neues Favorit-Datenfeld, das in beiden Ansichten sichtbar ist.

**Architecture:** Eine neue, per `localStorage` persistierte Dichte-Einstellung (`compact`/`comfort`) steuert ein `data-density`-Attribut auf `<html>`, orthogonal zum bestehenden Hell/Dunkel-Attribut. Eine neue CSS-Token-Ebene in `theme.css` liefert Komfort-Werte für Schrift/Abstand/Radius; Komponenten, die eine eigene Komfort-Optik bekommen (Karte, Detail-Panel), lesen den Density-Wert direkt und rendern zwei JSX-Zweige im selben Component (kein Duplizieren von State/Handlern). Ein neues `favorite`-Boolean-Feld läuft End-zu-Ende durch SQLite → Tauri-Command → Frontend-State, analog zum bestehenden `printStatus`-Muster.

**Tech Stack:** React 19 + TypeScript, Tailwind CSS 3 (arbitrary values inkl. `var()`), Tauri 2 (Rust-Backend), rusqlite/SQLite (kein Migrations-Framework, ALTER-TABLE-Pattern), kein Frontend-Testrahmen (nur `cargo test` im Backend).

## Global Constraints

- Default bleibt `compact` — niemand bekommt die neue Ansicht ungefragt.
- Die bestehende Kompakt-Ansicht darf sich **pixelgenau nicht ändern**, außer durch das neue Favorit-Badge (Nicht-Ziel aus der Spec). Wo ein Tailwind-Wert nicht exakt einem der vier neuen Font-Tokens entspricht, bleibt er unangetastet (siehe Task 6).
- Alle Farben kommen aus den bestehenden `--bg`/`--panel`/`--ink`/`--accent`-Tokens aus `theme.css` — keine neuen hartkodierten Hex-Werte in Komponenten.
- Kein neues Test-Framework fürs Frontend. Verifikation dort ist `tsc`-Typprüfung + manuelle Sichtprüfung im Dev-Server.
- Rust-Migrationen laufen nach dem bestehenden Muster in `src-tauri/src/db/repository.rs::init()`: `CREATE TABLE IF NOT EXISTS` in `schema.sql` für Neuinstallationen, zusätzlich ein ignoriertes `ALTER TABLE ... ADD COLUMN` in `init()` für bestehende Datenbanken.
- Neue SQL-Spalten werden an das Ende der bestehenden Spaltenliste angehängt (nicht mittendrin einfügen), damit die positionsbasierten `row.get(N)`-Indizes der bestehenden Spalten unverändert bleiben.
- CHANGELOG.md wird unter `## [Unreleased]` → `### Added` ergänzt (Projektkonvention, kein Versions-Bump — `[Unreleased]` sammelt seit Projektstart).

---

### Task 1: Backend — Favorit-Datenfeld

**Files:**
- Modify: `src-tauri/src/db/models.rs:33-59` (structs `NewFile`, `FileRecord`)
- Modify: `src-tauri/src/db/schema.sql:12-41` (Tabelle `files`)
- Modify: `src-tauri/src/db/repository.rs:26-75` (`init()` Migration)
- Modify: `src-tauri/src/db/repository.rs:219-243` (`insert_file`, SQL + params)
- Modify: `src-tauri/src/db/repository.rs:308-395` (`get_file`, `list_files`, `row_to_file`)
- Modify: `src-tauri/src/db/repository.rs:493-501` (neue Funktion `set_favorite` nach `set_print_status`)
- Modify: `src-tauri/src/db/mod.rs:5-13` (Re-Export), `src-tauri/src/db/mod.rs:28-56` (`sample_file()`)
- Modify: `src-tauri/src/commands.rs:23-43` (`ModelFileDto`), `src-tauri/src/commands.rs:161-198` (`to_dto`), `src-tauri/src/commands.rs:630-655` (Import-NewFile), `src-tauri/src/commands.rs:1227-1255` (`sample_file_record`)
- Modify: `src-tauri/src/commands.rs:398-404` (neuer Command `set_favorite` nach `set_print_status`)
- Modify: `src-tauri/src/lib.rs:52` (Command-Registrierung)
- Test: `src-tauri/src/db/mod.rs` (neuer Test in `mod tests`)

**Interfaces:**
- Produces: `db::set_favorite(conn: &Connection, file_id: i64, favorite: bool) -> Result<(), DbError>`; Tauri-Command `set_favorite(state: State<AppState>, file_id: String, favorite: bool) -> CmdResult<()>`; `ModelFileDto.favorite: bool` (serialisiert als `"favorite"` durch `camelCase`-Rename, da bereits ein Wort); `FileRecord.favorite: bool`, `NewFile.favorite: bool`.

- [ ] **Step 1: Neues Feld zu den Structs hinzufügen**

In `src-tauri/src/db/models.rs`, `NewFile` (nach `queue_position: Option<i64>,` in Zeile 58) und `FileRecord` (nach `queue_position: Option<i64>,` in Zeile 88) jeweils ergänzen:

```rust
    pub favorite: bool,
```

- [ ] **Step 2: Spalte in Schema und Migration ergänzen**

In `src-tauri/src/db/schema.sql`, Zeile 40 (`queue_position INTEGER`) durch:

```sql
    queue_position INTEGER,
    favorite INTEGER NOT NULL DEFAULT 0
```

In `src-tauri/src/db/repository.rs`, nach Zeile 73 (`let _ = conn.execute("ALTER TABLE files ADD COLUMN queue_position INTEGER", []);`) ergänzen:

```rust
    let _ = conn.execute(
        "ALTER TABLE files ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0",
        [],
    );
```

- [ ] **Step 3: SQL-Statements und `row_to_file` erweitern**

In `insert_file` (repository.rs): Spaltenliste und `VALUES`-Platzhalter um `favorite` erweitern (als 25. Spalte/Parameter `?25`), sowie `file.favorite` ans Ende des `params![...]`-Blocks anhängen.

In `get_file` und `list_files`: `favorite` ans Ende der jeweiligen `SELECT`-Spaltenliste anhängen (nach `queue_position`).

In `row_to_file`: nach `queue_position: row.get(24)?,` ergänzen:

```rust
        favorite: row.get(25)?,
```

- [ ] **Step 4: Neue Funktion `set_favorite`**

In `src-tauri/src/db/repository.rs`, direkt nach `set_print_status` (nach dessen schließender `}` in Zeile 501):

```rust
pub fn set_favorite(conn: &Connection, file_id: i64, favorite: bool) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET favorite = ?1 WHERE id = ?2",
        params![favorite, file_id],
    )?;
    Ok(())
}
```

In `src-tauri/src/db/mod.rs`, `set_favorite` zur `pub use repository::{...}`-Liste (Zeile 5-13) hinzufügen.

- [ ] **Step 5: Bestehende Struct-Literale anpassen (Kompilierfähigkeit herstellen)**

Jeweils `favorite: false,` ergänzen in:
- `src-tauri/src/db/mod.rs:56` (`sample_file()`, nach `queue_position: None,`)
- `src-tauri/src/commands.rs:655` (Import-`NewFile`, nach `queue_position: None,`)
- `src-tauri/src/commands.rs:1254` (`sample_file_record`, nach `queue_position: None,`)

- [ ] **Step 6: Bauen und bestehende Tests grün halten**

Run: `cd src-tauri && cargo test`
Expected: alle bisherigen Tests weiterhin PASS (Regressionscheck vor der neuen Funktionalität).

- [ ] **Step 7: Fehlschlagenden Test für `set_favorite` schreiben**

In `src-tauri/src/db/mod.rs`, `mod tests`, nach `set_print_status_updates_the_status` (nach Zeile 74):

```rust
    #[test]
    fn set_favorite_toggles_the_flag() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        let before = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(before.favorite, false);

        set_favorite(&conn, id, true).expect("set favorite");
        let after = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(after.favorite, true);

        set_favorite(&conn, id, false).expect("unset favorite");
        let reverted = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(reverted.favorite, false);
    }
```

Run: `cargo test set_favorite_toggles_the_flag -- --nocapture`
Expected: PASS bereits nach Step 4 (die Funktion existiert schon) — falls Step 4 übersprungen wurde, schlägt der Build mit `cannot find function 'set_favorite'` fehl. Diese Reihenfolge dient der Absicherung: den Test jetzt zu isolieren bestätigt, dass genau diese Funktion (und keine zufällig grün gewordene andere Stelle) das Verhalten trägt.

- [ ] **Step 8: Alle Backend-Tests laufen lassen**

Run: `cargo test`
Expected: PASS, inkl. `set_favorite_toggles_the_flag` und der bereits existierenden Suite (Regressionscheck).

- [ ] **Step 9: Tauri-Command, DTO und Registrierung**

In `src-tauri/src/commands.rs`, `ModelFileDto` (nach `pub queue_position: Option<i64>,` in Zeile 42):

```rust
    pub favorite: bool,
```

In `to_dto` (nach `queue_position: file.queue_position,` in Zeile 197):

```rust
        favorite: file.favorite,
```

Nach `set_print_status` (nach Zeile 404):

```rust
#[tauri::command]
pub fn set_favorite(state: State<AppState>, file_id: String, favorite: bool) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_favorite(&conn, id, favorite).map_err(|e| e.to_string())
}
```

In `src-tauri/src/lib.rs`, nach `commands::set_print_status,` (Zeile 52):

```rust
            commands::set_favorite,
```

- [ ] **Step 10: Build und volle Testsuite final prüfen**

Run: `cargo test`
Expected: PASS (Backend-Teil des Features vollständig).

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src/db/models.rs src-tauri/src/db/schema.sql src-tauri/src/db/repository.rs src-tauri/src/db/mod.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: Favorit-Datenfeld im Backend (Spalte, Command set_favorite)"
```

---

### Task 2: Frontend — Favorit-Feld verdrahten + Kompakt-Badge

**Files:**
- Modify: `src/types/index.ts:5-26` (`ModelFile`)
- Modify: `src/i18n/types.ts:118-149` (Translations-Interface)
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts` (neue Keys)
- Modify: `src/App.tsx:291-305` (neue Funktion `toggleFavorite`), `src/App.tsx:532-537` (`<ModelGrid>`-Aufruf), `src/App.tsx:549-568` (`<DetailPanel>`-Aufruf)
- Modify: `src/components/ModelGrid.tsx` (Props, Badge-Rendering)
- Modify: `src/components/DetailPanel.tsx:8-25` (Props), `src/components/DetailPanel.tsx:181-203` (neue Aktionszeile)

**Interfaces:**
- Consumes: Tauri-Command `set_favorite` aus Task 1, aufgerufen als `invoke('set_favorite', { fileId, favorite })`.
- Produces: `ModelFile.favorite: boolean`; `App.tsx`-Funktion `toggleFavorite(id: string): void`; Props `favorite: boolean` und `onToggleFavorite: () => void` auf `ModelGrid`- und `DetailPanel`-Karten-Ebene (bei `ModelGrid` pro Karte über die bestehende `models`-Liste, kein Extra-Prop auf Komponentenebene nötig, da `m.favorite` direkt aus dem Model gelesen wird — nur `onToggleFavorite: (id: string) => void` kommt als neues Prop dazu); i18n-Keys `favoriteAdd`, `favoriteRemove`.

- [ ] **Step 1: Typ erweitern**

In `src/types/index.ts`, nach `queuePosition: number | null;` (Zeile 25):

```ts
  favorite: boolean;
```

- [ ] **Step 2: i18n-Keys ergänzen (alle 4 Sprachen)**

In `src/i18n/types.ts`, am Ende des `Translations`-Interface, nach `cleanupDeleteSelected: string;` (Zeile 148):

```ts

  favoriteAdd: string;
  favoriteRemove: string;
```

In `src/i18n/de.ts`, vor der abschließenden `};`:

```ts

  favoriteAdd: 'Zu Favoriten hinzufügen',
  favoriteRemove: 'Aus Favoriten entfernen',
```

In `src/i18n/en.ts`:

```ts

  favoriteAdd: 'Add to favorites',
  favoriteRemove: 'Remove from favorites',
```

In `src/i18n/es.ts`:

```ts

  favoriteAdd: 'Añadir a favoritos',
  favoriteRemove: 'Quitar de favoritos',
```

In `src/i18n/fr.ts`:

```ts

  favoriteAdd: 'Ajouter aux favoris',
  favoriteRemove: 'Retirer des favoris',
```

- [ ] **Step 3: `toggleFavorite` in App.tsx**

In `src/App.tsx`, nach `togglePrintStatus` (nach Zeile 305):

```ts
  const toggleFavorite = (id: string) => {
    const current = models.find((m) => m.id === id);
    if (!current) return;
    const next = !current.favorite;
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, favorite: next } : m)));
    invoke('set_favorite', { fileId: id, favorite: next }).catch((e) => {
      console.error('[favorite] Aktualisieren fehlgeschlagen:', e);
    });
  };
```

In `src/App.tsx`, `<ModelGrid>`-Aufruf (Zeile 532-537) um `onToggleFavorite={toggleFavorite}` ergänzen:

```tsx
                <ModelGrid
                  models={filtered}
                  selectedId={selectedId}
                  onSelect={selectModel}
                  onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                  onToggleFavorite={toggleFavorite}
                />
```

In `src/App.tsx`, `<DetailPanel>`-Aufruf (Zeile 549-568) nach `onTogglePrintStatus={...}` ergänzen:

```tsx
            onToggleFavorite={() => selected && toggleFavorite(selected.id)}
```

- [ ] **Step 4: Kompakt-Badge in `ModelGrid.tsx`**

In `src/components/ModelGrid.tsx`, `Props`-Interface um `onToggleFavorite: (id: string) => void;` ergänzen. Nach dem bestehenden `printStatus`-Badge-Block (Zeile 71-75) ergänzen:

```tsx
            <button
              onClick={(e) => {
                e.stopPropagation();
                onToggleFavorite(m.id);
              }}
              aria-label={m.favorite ? t('favoriteRemove') : t('favoriteAdd')}
              className={`absolute left-[7px] bottom-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border cursor-pointer ${
                m.favorite
                  ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                  : 'border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]'
              }`}
            >
              {m.favorite ? '♥' : '♡'}
            </button>
```

(Platz ist frei: der `printStatus`-Badge sitzt rechts unten, `NEU` links oben, Herkunft rechts oben — links unten war bisher unbelegt.)

- [ ] **Step 5: Favorit-Button im DetailPanel (kompakter Stil)**

In `src/components/DetailPanel.tsx`, `Props`-Interface (nach `onTogglePrintStatus: () => void;` in Zeile 13) ergänzen:

```ts
  onToggleFavorite: () => void;
```

Und in der Funktionssignatur (nach Zeile 61) als Parameter ergänzen: `onToggleFavorite,`.

Als neue Zeile direkt nach dem bestehenden Druckstatus-Block (nach Zeile 191, vor der Warteschlangen-Zeile):

```tsx
        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span className="flex-1 text-[12.5px] font-medium">
            {model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
          </span>
          <button
            onClick={onToggleFavorite}
            className="h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {model.favorite ? '♥' : '♡'}
          </button>
        </div>
```

- [ ] **Step 6: Typprüfung**

Run: `npm run build`
Expected: PASS (keine TypeScript-Fehler mehr).

- [ ] **Step 7: Manuelle Verifikation**

Run: `npm run tauri dev`
Erwartet: Herz-Icon links unten auf jeder Karte im Raster sowie eine neue Zeile im Detail-Panel; Klick schaltet um, bleibt nach Neustart der App erhalten (Backend-Persistenz).

- [ ] **Step 8: Commit**

```bash
git add src/types/index.ts src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts src/App.tsx src/components/ModelGrid.tsx src/components/DetailPanel.tsx
git commit -m "feat: Favorit-Toggle im Frontend (Grid-Badge + Detail-Panel), beide UI-Dichten"
```

---

### Task 3: Dichte-Einstellung (Hook, Root-Attribut, Header-Umschalter, Token-Ebene)

**Files:**
- Create: `src/hooks/useUiDensity.ts`
- Modify: `src/App.tsx:13` (Import), `src/App.tsx:46` (Hook-Aufruf), `src/App.tsx:444-466` (`<Header>`-Props)
- Modify: `src/components/Header.tsx:1-31` (Imports, Props), `src/components/Header.tsx:45-67` (Funktionssignatur), `src/components/Header.tsx:240-241` (neuer Abschnitt im Einstellungs-Panel)
- Modify: `src/i18n/types.ts` (neue Keys), `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`
- Modify: `src/styles/theme.css:53-55` (neue Token-Ebene)

**Interfaces:**
- Produces: `type UiDensity = 'compact' | 'comfort'` und Hook `useUiDensity(): { density: UiDensity; setDensity: (d: UiDensity) => void }` (Datei `src/hooks/useUiDensity.ts`); Root-Attribut `document.documentElement.setAttribute('data-density', density)`; CSS-Variablen `--font-size-title`, `--font-size-body`, `--font-size-meta`, `--font-size-label`, `--space-card-pad`, `--radius-card`, `--icon-badge-size` (Basiswerte auf `:root`, Komfort-Overrides unter `[data-density="comfort"]`); Header-Props `uiDensity: UiDensity`, `onUiDensityChange: (d: UiDensity) => void`.
- Consumes: nichts aus vorherigen Tasks.

- [ ] **Step 1: Hook erstellen**

Neue Datei `src/hooks/useUiDensity.ts`, nach exaktem Muster von `useTheme.ts`:

```ts
import { useEffect, useState, useCallback } from 'react';

export type UiDensity = 'compact' | 'comfort';

const STORAGE_KEY = '3mf-katalog-density';

/**
 * Verwaltet die UI-Dichte der Anwendung ("compact" = bestehende, dichte
 * Ansicht; "comfort" = größere Schrift/Grafiken). Persistiert in
 * localStorage nach demselben Muster wie useTheme.
 */
export function useUiDensity() {
  const [density, setDensityState] = useState<UiDensity>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === 'comfort' ? 'comfort' : 'compact';
  });

  useEffect(() => {
    document.documentElement.setAttribute('data-density', density);
  }, [density]);

  const setDensity = useCallback((next: UiDensity) => {
    setDensityState(next);
    localStorage.setItem(STORAGE_KEY, next);
  }, []);

  return { density, setDensity };
}
```

- [ ] **Step 2: In App.tsx verdrahten**

In `src/App.tsx`, nach `import { useTheme } from './hooks/useTheme';` (Zeile 13):

```ts
import { useUiDensity } from './hooks/useUiDensity';
```

Nach `const { setting, setTheme } = useTheme();` (Zeile 46):

```ts
  const { density, setDensity } = useUiDensity();
```

In `<Header>` (nach `onThemeChange={setTheme}` in Zeile 451):

```tsx
        uiDensity={density}
        onUiDensityChange={setDensity}
```

- [ ] **Step 3: i18n-Keys**

In `src/i18n/types.ts`, nach `languageTitle: string;` (Zeile 32):

```ts
  densityTitle: string;
  densityCompact: string;
  densityComfort: string;
  densityDescriptionCompact: string;
  densityDescriptionComfort: string;
```

In `src/i18n/de.ts`, an der zu `languageTitle` korrespondierenden Stelle:

```ts
  densityTitle: 'Ansicht',
  densityCompact: 'Kompakt',
  densityComfort: 'Komfort',
  densityDescriptionCompact: 'Dichte, kleine Schrift — möglichst viel auf einen Blick.',
  densityDescriptionComfort: 'Größere Schrift und Grafiken — besser lesbar.',
```

In `src/i18n/en.ts`:

```ts
  densityTitle: 'View',
  densityCompact: 'Compact',
  densityComfort: 'Comfort',
  densityDescriptionCompact: 'Dense, small text — fits as much as possible on screen.',
  densityDescriptionComfort: 'Larger text and graphics — easier to read.',
```

In `src/i18n/es.ts`:

```ts
  densityTitle: 'Vista',
  densityCompact: 'Compacta',
  densityComfort: 'Confort',
  densityDescriptionCompact: 'Densa, texto pequeño — muestra lo máximo posible.',
  densityDescriptionComfort: 'Texto y gráficos más grandes — más fácil de leer.',
```

In `src/i18n/fr.ts`:

```ts
  densityTitle: 'Affichage',
  densityCompact: 'Compact',
  densityComfort: 'Confort',
  densityDescriptionCompact: 'Dense, petit texte — affiche un maximum à l\'écran.',
  densityDescriptionComfort: 'Texte et graphismes plus grands — plus lisible.',
```

- [ ] **Step 4: Umschalter im Einstellungs-Panel**

In `src/components/Header.tsx`, `Props`-Interface (nach `onThemeChange: (t: ThemeSetting) => void;` in Zeile 16):

```ts
  uiDensity: UiDensity;
  onUiDensityChange: (d: UiDensity) => void;
```

Import ergänzen (nach Zeile 4):

```ts
import type { UiDensity } from '../hooks/useUiDensity';
```

Funktionsparameter ergänzen (nach `onThemeChange,` in Zeile 52): `uiDensity, onUiDensityChange,`.

Im JSX, nach dem Theme-Beschreibungstext (nach Zeile 240, vor der `languageTitle`-Sektion in Zeile 242):

```tsx
            <div className="text-[13px] font-semibold mt-4 mb-2">{t('densityTitle')}</div>
            <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
              {(['compact', 'comfort'] as UiDensity[]).map((opt) => (
                <button
                  key={opt}
                  onClick={() => onUiDensityChange(opt)}
                  className={`${segBase} flex-1 ${uiDensity === opt ? segActive : segInactive}`}
                >
                  {opt === 'compact' ? t('densityCompact') : t('densityComfort')}
                </button>
              ))}
            </div>
            <div className="mt-2 font-mono-ui text-[10.5px] leading-relaxed text-[var(--ink-3)]">
              {uiDensity === 'compact' ? t('densityDescriptionCompact') : t('densityDescriptionComfort')}
            </div>
```

- [ ] **Step 5: Token-Ebene in theme.css**

In `src/styles/theme.css`, am Dateiende (nach Zeile 55, `.font-mono-ui { ... }`):

```css

:root {
  --font-size-title: 12.5px;
  --font-size-body: 13px;
  --font-size-meta: 10px;
  --font-size-label: 9px;
  --space-card-pad: 0.625rem;
  --radius-card: 4px;
  --icon-badge-size: 1.125rem;
}

[data-density="comfort"] {
  --font-size-title: 17px;
  --font-size-body: 14.5px;
  --font-size-meta: 13px;
  --font-size-label: 11px;
  --space-card-pad: 1rem;
  --radius-card: 14px;
  --icon-badge-size: 2.375rem;
}
```

- [ ] **Step 6: Typprüfung und manuelle Verifikation**

Run: `npm run build`
Expected: PASS.

Run: `npm run tauri dev`
Erwartet: Neuer "Ansicht"-Umschalter im Einstellungs-Panel unter dem Theme-Umschalter; Wahl bleibt nach Neuladen/Neustart erhalten (`localStorage`-Key `3mf-katalog-density` prüfen); optisch ändert sich noch nichts (Tokens werden erst ab Task 4 konsumiert) — das ist zu diesem Zeitpunkt korrekt.

- [ ] **Step 7: Commit**

```bash
git add src/hooks/useUiDensity.ts src/App.tsx src/components/Header.tsx src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts src/styles/theme.css
git commit -m "feat: Dichte-Einstellung (Kompakt/Komfort) mit Token-Ebene"
```

---

### Task 4: Komfort-Karte in ModelGrid

**Files:**
- Modify: `src/components/ModelGrid.tsx` (komplette Kartendarstellung, zweiter JSX-Zweig)

**Interfaces:**
- Consumes: `useUiDensity` aus Task 3 (liest `density` direkt, kein neues Prop nötig — Hook wird lokal in `ModelGrid.tsx` aufgerufen, da `App.tsx` den Wert ohnehin nur an `Header` weiterreicht); `m.favorite`/`onToggleFavorite` aus Task 2; CSS-Tokens `--font-size-title`, `--font-size-meta`, `--space-card-pad`, `--radius-card`, `--icon-badge-size` aus Task 3.
- Produces: keine neuen Exporte, nur visuelle Erweiterung.

**Hinweis zu Meta-Zeile:** Die im Brainstorming gezeigte Mockup-Zeile ("6 Std.") war illustrativ — eine Druckzeit-Angabe existiert im Datenmodell nicht. Die reale Komfort-Karte zeigt stattdessen echte vorhandene Felder: erstes Material (`m.materials[0]?.name`) und geschätztes Gewicht (`m.estimatedWeightG`, über `formatWeightG` wie im Detail-Panel).

- [ ] **Step 1: Hook einbinden und Kartenwahl verzweigen**

In `src/components/ModelGrid.tsx`, Imports ergänzen:

```ts
import { useUiDensity } from '../hooks/useUiDensity';
import { formatWeightG } from '../i18n/format';
```

Und die bestehende Zeile `import { useT } from '../i18n/LanguageContext';` erweitern zu:

```ts
import { useT, useLanguage } from '../i18n/LanguageContext';
```

`Props`-Interface um `onToggleFavorite: (id: string) => void;` ergänzen (siehe Task 2, Step 5 — falls dort schon erledigt, hier nur die Hook-Nutzung ergänzen).

Im Komponentenkörper, nach `const t = useT();`:

```ts
  const { density } = useUiDensity();
  const { language } = useLanguage();
```

Der bisherige Karten-JSX-Block (Zeilen 25-93 vor dieser Änderung, inklusive des in Task 2 ergänzten Favorit-Badges) wird unverändert in eine lokale Funktion `renderCompactCard` im Komponentenkörper gezogen (oberhalb des `return`), die dieselben Closures (`t`, `selectedId`, `onSelect`, `onContextMenu`, `onToggleFavorite`) nutzt:

```tsx
  function renderCompactCard(m: ModelFile) {
    return (
      <div
        key={m.id}
        onClick={() => onSelect(m.id)}
        onContextMenu={(e) => {
          e.preventDefault();
          onSelect(m.id);
          onContextMenu(m.id, e.clientX, e.clientY);
        }}
        className={`rounded-[4px] overflow-hidden border cursor-pointer ${
          m.id === selectedId ? 'border-[var(--accent)]' : 'border-[var(--line)]'
        }`}
      >
        <div className="relative aspect-square bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
          {m.displayImage ? (
            <img src={m.displayImage} alt="" className="absolute inset-0 w-full h-full object-cover" />
          ) : (
            <>
              <div
                className="absolute inset-0 opacity-90"
                style={{
                  backgroundImage:
                    'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 9px)',
                }}
              />
              <div className="absolute inset-0 grid place-items-center">
                <div className="w-[52px] h-[52px] border border-dashed border-[var(--line-strong)] rotate-45" />
              </div>
              <div className="absolute left-2 bottom-[7px] font-mono-ui text-[9px] tracking-[0.08em] uppercase text-[var(--ink-3)]">
                {t('previewLabel3d')}
              </div>
            </>
          )}
          {Date.now() - new Date(m.importedAt).getTime() < 24 * 60 * 60 * 1000 && (
            <div className="absolute left-[7px] top-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]">
              {t('newBadge')}
            </div>
          )}
          {originAbbr[m.origin] && (
            <div className="absolute right-[7px] top-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]">
              {originAbbr[m.origin]}
            </div>
          )}
          {m.printStatus === 'printed' && (
            <div className="absolute right-[7px] bottom-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]">
              ✓ {t('printedBadge')}
            </div>
          )}
          <button
            onClick={(e) => {
              e.stopPropagation();
              onToggleFavorite(m.id);
            }}
            aria-label={m.favorite ? t('favoriteRemove') : t('favoriteAdd')}
            className={`absolute left-[7px] bottom-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border cursor-pointer ${
              m.favorite
                ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                : 'border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]'
            }`}
          >
            {m.favorite ? '♥' : '♡'}
          </button>
        </div>
        <div className="flex flex-col gap-1.5 px-2.5 py-2.5 bg-[var(--panel)]">
          <div className="text-[12.5px] font-semibold overflow-hidden text-ellipsis whitespace-nowrap">
            {m.name}
          </div>
          <div className="flex flex-wrap gap-1">
            {m.tags.map((tag) => (
              <span
                key={tag}
                className="font-mono-ui text-[10px] px-1.5 py-0.5 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)]"
              >
                #{tag}
              </span>
            ))}
          </div>
        </div>
      </div>
    );
  }
```

**Hinweis:** Falls Task 2 Step 4 bereits ausgeführt wurde, existiert der Favorit-Button oben schon im bestehenden JSX — dann hier nur die vorhandene Karte 1:1 in `renderCompactCard` verschieben, nicht erneut hinzufügen.

Direkt danach die Komponente wie folgt zurückgeben — `density === 'comfort'` rendert die neue Karte, sonst `renderCompactCard(m)`:

```tsx
  return (
    <div className="grid gap-3.5" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(178px, 1fr))' }}>
      {models.map((m) =>
        density === 'comfort' ? (
          <div
            key={m.id}
            onClick={() => onSelect(m.id)}
            onContextMenu={(e) => {
              e.preventDefault();
              onSelect(m.id);
              onContextMenu(m.id, e.clientX, e.clientY);
            }}
            className={`rounded-[var(--radius-card)] overflow-hidden cursor-pointer bg-[var(--panel)] shadow-[var(--shadow)] border-2 ${
              m.id === selectedId ? 'border-[var(--accent)]' : 'border-transparent'
            }`}
          >
            <div className="h-[5px]" style={{ background: 'linear-gradient(90deg, var(--accent), var(--accent-soft))' }} />
            <div className="relative aspect-square bg-[var(--plate)] overflow-hidden">
              {m.displayImage ? (
                <img src={m.displayImage} alt="" className="absolute inset-0 w-full h-full object-cover" />
              ) : (
                <>
                  <div
                    className="absolute inset-0 opacity-90"
                    style={{
                      backgroundImage:
                        'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 12px)',
                    }}
                  />
                  <div className="absolute inset-0 grid place-items-center">
                    <div className="w-[64px] h-[64px] border border-dashed border-[var(--line-strong)] rotate-45" />
                  </div>
                </>
              )}
              {Date.now() - new Date(m.importedAt).getTime() < 24 * 60 * 60 * 1000 && (
                <div className="absolute left-2.5 top-2.5 font-semibold text-[11px] px-2.5 py-1 rounded-lg bg-[var(--panel)] text-[var(--accent)] shadow-[var(--shadow)]">
                  {t('newBadge')}
                </div>
              )}
              {originAbbr[m.origin] && (
                <div className="absolute right-2.5 top-2.5 font-semibold text-[10px] px-2 py-1 rounded-md bg-[var(--panel)]/90 text-[var(--ink-2)]">
                  {originAbbr[m.origin]}
                </div>
              )}
              {m.printStatus === 'printed' && (
                <div className="absolute left-2.5 bottom-2.5 font-semibold text-[11px] px-2.5 py-1 rounded-lg bg-[var(--panel)] text-[var(--accent)] shadow-[var(--shadow)]">
                  ✓ {t('printedBadge')}
                </div>
              )}
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onToggleFavorite(m.id);
                }}
                aria-label={m.favorite ? t('favoriteRemove') : t('favoriteAdd')}
                style={{ width: 'var(--icon-badge-size)', height: 'var(--icon-badge-size)' }}
                className={`absolute right-2.5 bottom-2.5 rounded-full grid place-items-center bg-[var(--panel)]/90 shadow-[var(--shadow)] cursor-pointer text-[17px] ${
                  m.favorite ? 'text-[var(--accent)]' : 'text-[var(--ink-3)]'
                }`}
              >
                {m.favorite ? '♥' : '♡'}
              </button>
            </div>
            <div className="flex flex-col gap-2" style={{ padding: 'var(--space-card-pad)' }}>
              <div
                className="font-bold overflow-hidden text-ellipsis whitespace-nowrap"
                style={{ fontSize: 'var(--font-size-title)' }}
              >
                {m.name}
              </div>
              <div className="flex flex-wrap gap-1.5">
                {m.tags.map((tag) => (
                  <span
                    key={tag}
                    className="px-2.5 py-1 rounded-full bg-[var(--panel-2)] text-[var(--ink-2)]"
                    style={{ fontSize: 'var(--font-size-meta)' }}
                  >
                    #{tag}
                  </span>
                ))}
              </div>
              <div
                className="flex items-center gap-3 text-[var(--ink-3)] font-medium"
                style={{ fontSize: 'var(--font-size-meta)' }}
              >
                {m.materials[0] && <span>📦 {m.materials[0].name}</span>}
                {m.estimatedWeightG !== null && <span>⚖ ≈ {formatWeightG(m.estimatedWeightG, language)}</span>}
              </div>
            </div>
          </div>
        ) : (
          renderCompactCard(m)
        ),
      )}
    </div>
  );
```

Diese Extraktion vermeidet doppelten Code und hält die Kompakt-Darstellung garantiert unverändert — `renderCompactCard` ist byte-identisch zum bisherigen JSX.

- [ ] **Step 2: Typprüfung**

Run: `npm run build`
Expected: PASS.

- [ ] **Step 3: Manuelle Verifikation**

Run: `npm run tauri dev`, im Einstellungs-Panel auf "Komfort" umschalten.
Erwartet: Raster zeigt größere, abgerundete Karten mit Akzentbalken, größerem Favorit-Button, Material-/Gewichts-Zeile; Umschalten zurück auf "Kompakt" zeigt exakt die alte Karte (nur mit dem neuen Favorit-Badge aus Task 2). Prüfen in Hell **und** Dunkel.

- [ ] **Step 4: Commit**

```bash
git add src/components/ModelGrid.tsx
git commit -m "feat: Komfort-Kartenlayout im Modell-Raster"
```

---

### Task 5: Komfort-Ansicht im DetailPanel

**Files:**
- Modify: `src/components/DetailPanel.tsx`

**Interfaces:**
- Consumes: `useUiDensity` aus Task 3; `model.favorite`/`onToggleFavorite` aus Task 2; alle bestehenden Handler/States der Komponente (unverändert, gemeinsam von beiden Render-Zweigen genutzt).
- Produces: keine neuen Exporte.

- [ ] **Step 1: Hook einbinden**

Import ergänzen:

```ts
import { useUiDensity } from '../hooks/useUiDensity';
```

Im Komponentenkörper, nach `const t = useT();` (Zeile 75):

```ts
  const { density } = useUiDensity();
```

- [ ] **Step 2: Rückgabe verzweigen**

Der bestehende `return (...)`-Block (Zeilen 137-395) wird zu `if (density === 'compact') { return ( ...unverändert... ); }`, gefolgt von einem neuen `return (...)` für `comfort`:

```tsx
  if (density === 'compact') {
    return (
      // unveränderter bestehender JSX-Baum aus Zeile 137-395
    );
  }

  return (
    <aside
      className="flex-none flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]"
      style={{ width: '380px' }}
    >
      <div className="flex-1 overflow-y-auto">
        <div className="relative aspect-[4/3] bg-[var(--plate)] overflow-hidden">
          <div
            className="absolute inset-0"
            style={{
              backgroundImage:
                'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 12px)',
            }}
          />
          <ModelViewer
            fileId={model.id}
            needsSnapshot={model.displayImage === null}
            onSnapshotCaptured={onSnapshotCaptured}
          />
          <button
            onClick={onUploadImage}
            className="absolute right-3 top-3 h-9 px-3.5 rounded-lg bg-[var(--panel)] shadow-[var(--shadow)] text-[var(--ink-2)] font-semibold cursor-pointer hover:text-[var(--accent)]"
            style={{ fontSize: 'var(--font-size-meta)' }}
          >
            {t('uploadModelImageLabel')}
          </button>
        </div>

        <div className="px-[18px] pt-4 pb-1 flex items-start justify-between gap-3">
          <div className="font-extrabold leading-tight break-words" style={{ fontSize: 'var(--font-size-title)' }}>
            {model.name}
          </div>
          <button
            onClick={onToggleFavorite}
            aria-label={model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
            className={`flex-none text-[19px] ${model.favorite ? 'text-[var(--accent)]' : 'text-[var(--ink-3)]'}`}
          >
            {model.favorite ? '♥' : '♡'}
          </button>
        </div>
        <div className="px-[18px] pb-3.5 flex flex-wrap gap-1.5">
          {model.tags.map((tag) => (
            <span
              key={tag}
              className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
              style={{ fontSize: 'var(--font-size-meta)' }}
            >
              #{tag}
              <span
                onClick={() => onRemoveTag(tag)}
                className="w-4 h-4 grid place-items-center rounded-full cursor-pointer hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                style={{ fontSize: 'var(--font-size-label)' }}
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
            className="px-2.5 py-1 rounded-full border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0"
            style={{ fontSize: 'var(--font-size-meta)' }}
          />
        </div>

        <div className="px-[18px] pb-4 border-t border-[var(--line)] pt-3.5">
          <div
            className="font-bold uppercase tracking-wide text-[var(--ink-3)] mb-2.5"
            style={{ fontSize: 'var(--font-size-label)' }}
          >
            {t('metadataHeading')}
          </div>
          {buildMetaRows(model, t, language).map((row) => (
            <div
              key={row.label}
              className="flex justify-between py-2 border-b border-[var(--line)]"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              <span className="text-[var(--ink-2)]">{row.label}</span>
              <span className="font-semibold">{row.value}</span>
            </div>
          ))}
          <div className="flex justify-between py-2 border-b border-[var(--line)]" style={{ fontSize: 'var(--font-size-body)' }}>
            <span className="text-[var(--ink-2)]">{t(SYNC_KEYS[model.sync])}</span>
            <span className="font-semibold">{formatRelativeTime(model.importedAt, language)}</span>
          </div>
          <div className="flex justify-between items-center gap-2 py-2" style={{ fontSize: 'var(--font-size-body)' }}>
            <span className="text-[var(--ink-2)]">{t('metaSourceUrl')}</span>
            {editingSourceUrl ? (
              <input
                value={sourceUrlDraft}
                onChange={(e) => setSourceUrlDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') submitSourceUrl();
                  if (e.key === 'Escape') {
                    cancelingSourceUrlRef.current = true;
                    setEditingSourceUrl(false);
                  }
                }}
                onBlur={() => {
                  if (cancelingSourceUrlRef.current) {
                    cancelingSourceUrlRef.current = false;
                    return;
                  }
                  submitSourceUrl();
                }}
                autoFocus
                placeholder={t('sourceUrlPlaceholder')}
                className="flex-1 min-w-0 px-2 py-1 rounded-md border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0"
              />
            ) : model.sourceUrl ? (
              <a
                href={model.sourceUrl}
                target="_blank"
                rel="noreferrer"
                onClick={() => startEditingSourceUrl()}
                className="flex-1 min-w-0 truncate text-right text-[var(--accent)] hover:underline"
              >
                {model.sourceUrl}
              </a>
            ) : (
              <span onClick={startEditingSourceUrl} className="flex-1 text-right text-[var(--ink-3)] cursor-pointer">
                {t('noValue')}
              </span>
            )}
          </div>
        </div>
      </div>

      <div className="flex-none px-[18px] py-4 border-t border-[var(--line)] bg-[var(--panel-2)] flex flex-col gap-2">
        {slicerError && (
          <div className="text-[var(--accent)] break-words" style={{ fontSize: 'var(--font-size-meta)' }}>
            {t('slicerLaunchError')} {slicerError}
          </div>
        )}
        {cloudUploadError && (
          <div className="text-[var(--accent)] break-words" style={{ fontSize: 'var(--font-size-meta)' }}>
            {t('cloudUploadError')} {cloudUploadError}
          </div>
        )}
        {confirmDelete ? (
          <div className="flex items-center gap-2">
            <span className="flex-1 font-medium" style={{ fontSize: 'var(--font-size-body)' }}>
              {t('deleteConfirmQuestion')}
            </span>
            <button
              onClick={() => setConfirmDelete(false)}
              className="h-10 px-4 rounded-lg border border-[var(--line-strong)] bg-[var(--panel)] font-semibold cursor-pointer"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              {t('cancel')}
            </button>
            <button
              onClick={() => {
                setConfirmDelete(false);
                onDelete();
              }}
              className="h-10 px-4 rounded-lg bg-[var(--accent)] text-[var(--accent-ink)] font-semibold cursor-pointer"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              {t('delete')}
            </button>
          </div>
        ) : (
          <>
            <div className="relative flex">
              <button
                onClick={() => onOpenInSlicer()}
                className={`flex-1 h-11 px-4 flex items-center gap-2.5 justify-center font-bold cursor-pointer bg-[var(--accent)] text-[var(--accent-ink)] ${
                  hasSlicers ? 'rounded-l-lg' : 'rounded-lg'
                }`}
                style={{ fontSize: 'var(--font-size-body)' }}
              >
                🖨 {t('openInSlicer')}
              </button>
              {hasSlicers && (
                <button
                  onClick={() => setSlicerMenuOpen((o) => !o)}
                  aria-label={t('chooseSlicerAria')}
                  className="w-11 h-11 grid place-items-center rounded-r-lg bg-[var(--accent)] text-[var(--accent-ink)] cursor-pointer border-l border-[var(--accent-ink)]/20"
                >
                  ▾
                </button>
              )}
              {slicerMenuOpen && (
                <div className="absolute bottom-12 left-0 right-0 py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded-lg shadow-[var(--shadow)] z-40">
                  {slicers.map((s) => (
                    <button
                      key={s.id}
                      onClick={() => {
                        setSlicerMenuOpen(false);
                        onOpenInSlicer(s.id);
                      }}
                      className="w-full text-left px-4 py-2 hover:bg-[var(--panel-2)] cursor-pointer"
                      style={{ fontSize: 'var(--font-size-body)' }}
                    >
                      {s.name}
                    </button>
                  ))}
                </div>
              )}
            </div>
            <button
              onClick={() => !uploadDisabled && onUploadToCloud()}
              disabled={uploadDisabled}
              aria-label={uploadAria}
              title={uploadAria}
              className={`h-11 px-4 flex items-center justify-center gap-2.5 rounded-lg bg-[var(--panel-2)] font-semibold ${
                uploadDisabled ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer hover:text-[var(--accent)]'
              }`}
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              <span className={uploading ? 'inline-block animate-spin' : 'inline-block'}>☁</span>
              {uploadLabel}
            </button>
            <button
              onClick={onToggleQueue}
              className="h-11 px-4 flex items-center justify-center gap-2.5 rounded-lg bg-[var(--panel-2)] font-semibold cursor-pointer hover:text-[var(--accent)]"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              🎞 {model.queuePosition !== null ? t('removeFromQueue') : t('addToQueue')}
            </button>
            <button
              onClick={onTogglePrintStatus}
              className="h-11 px-4 flex items-center justify-center gap-2.5 rounded-lg bg-[var(--panel-2)] font-semibold cursor-pointer hover:text-[var(--accent)]"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              {model.printStatus === 'printed' ? `✓ ${t('markAsNotPrinted')}` : `${t('markAsPrinted')}`}
            </button>
            <button
              onClick={() => setConfirmDelete(true)}
              aria-label={t('deleteAriaLabel')}
              className="h-9 rounded-lg text-[var(--ink-3)] hover:text-[var(--accent)] cursor-pointer"
              style={{ fontSize: 'var(--font-size-meta)' }}
            >
              {t('deleteAriaLabel')}
            </button>
          </>
        )}
      </div>
    </aside>
  );
```

Damit ist die bestätigte Reihenfolge umgesetzt: Vorschau → Titel/Tags (inkl. Favorit-Button) → Eigenschaften (inkl. Sync-Status als zusätzliche Zeile) → Aktions-Buttons (Slicer, Cloud-Upload, Warteschlange, Druckstatus, Löschen als dezenter Text-Button statt Icon).

- [ ] **Step 3: Leerer Zustand prüfen**

Der `if (!model) { return ...; }`-Block am Anfang der Funktion (Zeilen 91-97) bleibt unverändert und gilt für beide Dichten — keine Änderung nötig.

- [ ] **Step 4: Typprüfung**

Run: `npm run build`
Expected: PASS.

- [ ] **Step 5: Manuelle Verifikation**

Run: `npm run tauri dev`. In "Komfort" ein Modell auswählen: Vorschau groß, Titel+Favorit-Herz, Tags, Eigenschaften-Liste inkl. Sync-Zeile, darunter die vier Aktions-Buttons plus dezenter Löschen-Button. Alle bisherigen Aktionen (Slicer-Dropdown, Cloud-Upload mit Ladezustand/Fehler, Löschen mit Bestätigung, Quell-URL bearbeiten) einzeln durchklicken — nichts darf in Komfort weniger können als in Kompakt. Danach zurück auf "Kompakt" wechseln und bestätigen, dass sich dort nichts verändert hat außer der neuen Favorit-Zeile aus Task 2.

- [ ] **Step 6: Commit**

```bash
git add src/components/DetailPanel.tsx
git commit -m "feat: Komfort-Ansicht im Detail-Panel (Vorschau, Eigenschaften, Aktions-Buttons)"
```

---

### Task 6: Token-Übernahme in den restlichen Komponenten

**Files:**
- Modify: `src/components/Header.tsx`, `src/components/Sidebar.tsx`, `src/components/ModelList.tsx`, `src/components/FilamentView.tsx`, `src/components/ContextMenu.tsx`, `src/components/CatalogCleanupDialog.tsx`, `src/components/ImportSummaryBanner.tsx`

**Interfaces:**
- Consumes: CSS-Tokens aus Task 3 (`--font-size-title`, `--font-size-body`, `--font-size-meta`, `--font-size-label`).
- Produces: keine neuen Exporte.

**Scope-Entscheidung:** Nur Tailwind-Arbitrary-Values, die **exakt** einem der vier Font-Tokens entsprechen (`text-[9px]`→Label, `text-[10px]`→Meta, `text-[12.5px]`→Titel, `text-[13px]`→Body), werden ersetzt. Andere Zwischenwerte (9.5/10.5/11/11.5/12/14/14.5/15px) bleiben unangetastet — sie stammen aus derselben Kompakt-Ästhetik, aber ein Snap auf eines der vier Tokens würde die Kompakt-Ansicht sichtbar verändern (verboten laut Nicht-Ziel der Spec). Diese verbleibenden Stellen wachsen unter Komfort schlicht nicht mit; das ist ein bewusster Scope-Schnitt, kein Versehen. Jede Ersetzung ist eine reine 1:1-Textersetzung ohne visuelle Wirkung in Kompakt (Token-Default entspricht exakt dem bisherigen Pixelwert).

- [ ] **Step 1: Ersetzung ausführen**

```bash
cd src/components
for f in Header.tsx Sidebar.tsx ModelList.tsx FilamentView.tsx ContextMenu.tsx CatalogCleanupDialog.tsx ImportSummaryBanner.tsx; do
  sed -i \
    -e 's/text-\[9px\]/text-[var(--font-size-label)]/g' \
    -e 's/text-\[10px\]/text-[var(--font-size-meta)]/g' \
    -e 's/text-\[12\.5px\]/text-[var(--font-size-title)]/g' \
    -e 's/text-\[13px\]/text-[var(--font-size-body)]/g' \
    "$f"
done
```

- [ ] **Step 2: Ergebnis stichprobenartig prüfen**

Run: `grep -c "var(--font-size" Header.tsx Sidebar.tsx ModelList.tsx FilamentView.tsx ContextMenu.tsx CatalogCleanupDialog.tsx ImportSummaryBanner.tsx`
Expected: Header.tsx 19, Sidebar.tsx 20, ModelList.tsx 4, FilamentView.tsx 11, ContextMenu.tsx 3, CatalogCleanupDialog.tsx 8, ImportSummaryBanner.tsx 2 (Summe der zuvor gezählten exakten Treffer je Datei).

Run: `grep -rn "text-\[9px\]\|text-\[10px\]\|text-\[12\.5px\]\|text-\[13px\]" src/components/`
Expected: keine Treffer mehr außer in `ModelGrid.tsx`/`DetailPanel.tsx` (die haben in Task 4/5 bereits eigene, dichte-verzweigte Werte bekommen und sind hier nicht Teil des sed-Laufs).

- [ ] **Step 3: Typprüfung**

Run: `npm run build`
Expected: PASS — reine Textersetzung innerhalb bestehender Template-Strings, keine strukturelle Änderung.

- [ ] **Step 4: Manuelle Verifikation — Kompakt unverändert**

Run: `npm run tauri dev`, Dichte auf "Kompakt" belassen. Kopfzeile, Sidebar, Listenansicht, Filament-Lager, Kontextmenü, Aufräum-Dialog, Import-Banner optisch mit dem Stand vor diesem Task vergleichen (z. B. via vorher gemachtem Screenshot oder `git stash`/`git stash pop` A/B-Vergleich) — es darf keine sichtbare Änderung geben.

- [ ] **Step 5: Manuelle Verifikation — Komfort skaliert**

Dichte auf "Komfort" umschalten. Die durch das Token-System betroffenen Textstellen (u. a. Sidebar-Navigation, Kopfzeilen-Labels, Listenansicht-Zeilen, Filament-Karten, Kontextmenü-Einträge, Aufräum-Dialog-Text, Import-Banner-Text) müssen sichtbar größer sein als in Kompakt.

- [ ] **Step 6: Commit**

```bash
git add src/components/Header.tsx src/components/Sidebar.tsx src/components/ModelList.tsx src/components/FilamentView.tsx src/components/ContextMenu.tsx src/components/CatalogCleanupDialog.tsx src/components/ImportSummaryBanner.tsx
git commit -m "refactor: restliche Komponenten auf Font-Size-Tokens umgestellt (Komfort-Skalierung)"
```

---

### Task 7: Abschluss-Verifikation und Dokumentation

**Files:**
- Modify: `CHANGELOG.md` (Abschnitt `## [Unreleased]` → `### Added`)
- Modify: `README.md` (falls dort UI-Ansichten erwähnt werden — kurzer Hinweis auf die neue Einstellung)

**Interfaces:** keine (Doku- und Verifikationsabschluss).

- [ ] **Step 1: Vollständige Backend-Testsuite**

Run: `cd src-tauri && cargo test`
Expected: PASS, alle Tests inkl. der neuen `set_favorite_toggles_the_flag`.

- [ ] **Step 2: Vollständige Frontend-Typprüfung und Build**

Run: `npm run build`
Expected: PASS.

- [ ] **Step 3: Manuelle Prüfung aller 4 Kombinationen**

Run: `npm run tauri dev`. Für jede der 4 Kombinationen (Hell/Kompakt, Hell/Komfort, Dunkel/Kompakt, Dunkel/Komfort) einmal durchklicken: Modell-Raster, Detail-Panel, Listenansicht, Filament-Lager, Aufräum-Dialog, Kontextmenü, Import-Banner. Auf Clipping (siehe Spec-Hinweis zu `overflow:hidden`+`line-height:1`), Kontrast und Lesbarkeit achten.

- [ ] **Step 4: Screenshot-Vergleich**

Je einen Screenshot von Modell-Raster und Detail-Panel in Kompakt und in Komfort anfertigen (gleiches Modell, gleiches Theme) und lokal ablegen (z. B. `docs/superpowers/plans/2026-09-11-comfort-ui-screenshots/`), damit der Unterschied dokumentiert ist.

- [ ] **Step 5: CHANGELOG ergänzen**

In `CHANGELOG.md`, unter `### Added` (nach dem letzten bestehenden Eintrag zu den Aufräum-Vorschlägen), neue Zeile:

```markdown
- Komfort-Ansicht als Alternative zur bestehenden kompakten Oberfläche: deutlich größere Schrift, Grafiken und Bedienelemente (Karten-Layout angelehnt an printables.com/model), umschaltbar im Einstellungen-Panel unter "Ansicht", Standard bleibt die kompakte Ansicht. Neues Favorit-Merkmal je Modell (Herz-Icon), in beiden Ansichten sichtbar
```

- [ ] **Step 6: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: Komfort-Ansicht und Favorit-Feature im Changelog dokumentiert"
```
