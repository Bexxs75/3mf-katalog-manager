# GUI-Shell-Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ersetzt den vergrabenen Text-Button (Katalog↔Filament-Wechsel) durch eine feste linke Navigationsleiste und vereinheitlicht das Kartensystem zwischen Katalog- und Filament-Ansicht.

**Architektur:** Reines Frontend-Redesign (React/TSX + Tailwind). Neue Komponente `Rail.tsx`, Root-Layout von `flex-col` auf `flex-row` (Rail + Inhaltsspalte) umgestellt, zwei Buttons aus `Header.tsx` entfernt, Kartenradius in drei Komponenten vereinheitlicht.

**Tech Stack:** React 19, TypeScript, Tailwind (inline `className`), bestehendes CSS-Custom-Property-Theme (`src/styles/theme.css`).

## Global Constraints

- Design ist bereits als klickbarer Prototyp freigegeben:
  `docs/superpowers/mockups/2026-09-13-gui-redesign.html` (Rail-Abschnitt,
  Zeilen ~480-504 im Mockup-HTML) — visuelle Details (Icon-Formen, Maße,
  Tooltip-Verhalten) daraus 1:1 übernehmen, keine neue Kreativität nötig.
- Farb-/Radius-Tokens NICHT in `theme.css` unter `:root`/`[data-app]`
  ändern — nur feste Pixelwerte direkt in den betroffenen Komponenten
  (`rounded-[Npx]`), wie im Bestandscode üblich (siehe `ModelGrid.tsx`
  Zeile 95 `rounded-[4px]`, Zeile 187 `rounded-[var(--radius-card)]`).
- Keine Backend-/Tauri-Command-Änderungen in diesem Plan.
- `npx tsc --noEmit` muss nach jedem Task sauber durchlaufen.
- Bestehende Tests (`cargo test` im Rust-Teil) bleiben unberührt — dieser
  Plan rührt keine Rust-Datei an; trotzdem vor dem finalen Review einmal
  laufen lassen, um sicherzugehen, dass nichts versehentlich mit-editiert
  wurde.
- Kein AppImage-Rebuild/Deploy als Teil dieses Plans — das macht der
  Koordinator nach Abschluss beider Pläne (GUI-Shell + Ordner-Baum)
  gemeinsam.

---

### Task 1: Rail-Komponente erstellen und in App.tsx verdrahten

**Files:**
- Create: `src/components/Rail.tsx`
- Modify: `src/App.tsx:1-20` (Import), `src/App.tsx:594-641` (Root-JSX-Struktur)
- Modify: `src/components/Header.tsx:38-41` (Props-Interface), `src/components/Header.tsx:236-255` (JSX entfernen), `src/components/Header.tsx:82-85` (Destructuring)

**Interfaces:**
- Consumes: `mainView: 'catalog' | 'filament' | 'trash'`,
  `onMainViewChange: (v: 'catalog' | 'filament' | 'trash') => void`,
  `trashCount: number`, `settingsOpen: boolean`,
  `onSettingsOpenChange: (open: boolean) => void` — alle bereits als State/
  Handler in `App.tsx` vorhanden (siehe `mainView`/`setMainView`,
  `settingsOpen`/`setSettingsOpen`, `trashModels.length`), nur die
  Übergabe ändert sich vom `Header`-Aufruf auf den neuen `Rail`-Aufruf.
- Produces: Keine neuen Typen — `Rail` ist reine Präsentationskomponente
  ohne eigenen State außer Hover/Tooltip (CSS-only, kein `useState`
  nötig).

- [ ] **Step 1: `Rail.tsx` erstellen**

```tsx
import { useT } from '../i18n/LanguageContext';

type MainView = 'catalog' | 'filament' | 'trash';

interface Props {
  mainView: MainView;
  onMainViewChange: (v: MainView) => void;
  trashCount: number;
  settingsOpen: boolean;
  onSettingsOpenChange: (open: boolean) => void;
}

const railBtnBase =
  'relative w-[42px] h-[42px] rounded-[10px] border border-transparent grid place-items-center cursor-pointer text-[var(--ink-3)] hover:text-[var(--ink)] hover:bg-[var(--panel-2)]';
const railBtnActive =
  'text-[var(--accent)] bg-[var(--accent-soft)] border-[color-mix(in_oklch,var(--accent)_30%,transparent)]';

export function Rail({ mainView, onMainViewChange, trashCount, settingsOpen, onSettingsOpenChange }: Props) {
  const t = useT();

  return (
    <nav className="flex-none w-[60px] flex flex-col items-center pt-3.5 pb-2.5 bg-[var(--panel-2)] border-r border-[var(--line)]">
      <div className="w-[30px] h-[30px] rounded-[8px] bg-[var(--accent)] text-[var(--accent-ink)] grid place-items-center font-mono-ui font-bold text-[12px] mb-4.5">
        3MF
      </div>

      <div className="flex flex-col gap-1.5">
        <button
          onClick={() => onMainViewChange('catalog')}
          title={t('railCatalog')}
          aria-label={t('railCatalog')}
          className={`${railBtnBase} ${mainView === 'catalog' ? railBtnActive : ''}`}
        >
          <svg width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
            <rect x="3" y="3" width="7" height="7" rx="1.5" />
            <rect x="14" y="3" width="7" height="7" rx="1.5" />
            <rect x="3" y="14" width="7" height="7" rx="1.5" />
            <rect x="14" y="14" width="7" height="7" rx="1.5" />
          </svg>
        </button>
        <button
          onClick={() => onMainViewChange('filament')}
          title={t('railFilament')}
          aria-label={t('railFilament')}
          className={`${railBtnBase} ${mainView === 'filament' ? railBtnActive : ''}`}
        >
          <svg width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
            <circle cx="12" cy="12" r="8.5" />
            <circle cx="12" cy="12" r="3" />
          </svg>
        </button>
        <button
          onClick={() => onMainViewChange('trash')}
          title={t('trashHeading')}
          aria-label={t('trashHeading')}
          className={`${railBtnBase} ${mainView === 'trash' ? railBtnActive : ''}`}
        >
          <svg width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
            <path d="M4 7h16M9 7V4.5A1.5 1.5 0 0 1 10.5 3h3A1.5 1.5 0 0 1 15 4.5V7m-8 0 1 13.5A1.5 1.5 0 0 0 9.5 22h5a1.5 1.5 0 0 0 1.5-1.5L17 7" />
          </svg>
          {trashCount > 0 && (
            <span className="absolute -top-1 -right-1 min-w-[15px] h-[15px] px-[3px] rounded-full bg-[var(--accent)] text-[var(--accent-ink)] text-[9px] font-bold font-mono-ui grid place-items-center">
              {trashCount}
            </span>
          )}
        </button>
      </div>

      <div className="flex-1" />

      <button
        onClick={() => onSettingsOpenChange(!settingsOpen)}
        title={t('settingsTitle')}
        aria-label={t('settingsTitle')}
        className={`${railBtnBase} ${settingsOpen ? railBtnActive : ''}`}
      >
        <svg width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
          <circle cx="12" cy="12" r="3.2" />
          <path d="M19.4 13.5a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.04 1.56V19.6a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.04-1.56 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87A1.7 1.7 0 0 0 3.13 12.46H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.56-1.04 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H9a1.7 1.7 0 0 0 1.04-1.56V1a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1.04 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87V6a1.7 1.7 0 0 0 1.56 1.04H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.56 1.04Z" />
        </svg>
      </button>
    </nav>
  );
}
```

- [ ] **Step 2: i18n-Keys `railCatalog`/`railFilament` ergänzen**

In `src/i18n/` existiert pro Sprache eine Übersetzungsdatei (siehe
bestehende Keys wie `filamentNavButton`, `trashHeading` — gleiche Datei(en)
verwenden). Ergänze in JEDER vorhandenen Sprachdatei (de/en/es/fr, gleiche
Reihenfolge wie bestehende `filamentNavButton`-Zeile, direkt daneben):

```
railCatalog: 'Katalog',       // de
railFilament: 'Filament-Lager', // de
```
```
railCatalog: 'Catalog',
railFilament: 'Filament storage',
```
```
railCatalog: 'Catálogo',
railFilament: 'Almacén de filamento',
```
```
railCatalog: 'Catalogue',
railFilament: 'Stock de filament',
```

Exakte Feldnamen/Formatierung an der Struktur der jeweils existierenden
Datei ausrichten (Objekt-Literal mit `key: 'value',`-Zeilen, siehe
`filamentNavButton` als Vorlage in jeder Datei).

- [ ] **Step 3: `Header.tsx` Props-Interface bereinigen**

WICHTIG: `mainView` selbst bleibt erhalten — es steuert nicht nur den zu
entfernenden Button, sondern auch die beiden bestehenden
`{mainView === 'catalog' && (...)}`-Guards (aktuell Zeile 130 und Zeile
230), die Import-Button/Sortierung/Raster-Liste-Umschalter bzw. den
Dateizähler nur in der Katalog-Ansicht zeigen. Nur `onMainViewChange`
(Zeile 39) und `trashCount` (Zeile 40) aus dem `Props`-Interface
entfernen — `mainView: 'catalog' | 'filament' | 'trash';` (Zeile 38)
bleibt stehen. In der Funktions-Signatur (Destructuring, aktuell Zeilen
82-84) genauso: `mainView,` (Zeile 82) bleibt, `onMainViewChange,`
(Zeile 83) und `trashCount,` (Zeile 84) werden entfernt.

- [ ] **Step 4: Mode-Switch- und Papierkorb-Button aus `Header.tsx` entfernen**

Den kompletten Block von (aktuell)
```tsx
      <button
        onClick={() => onMainViewChange(mainView === 'catalog' ? 'filament' : 'catalog')}
```
bis einschließlich des schließenden `</button>` des Papierkorb-Buttons
(vor `<div className="relative shrink-0">` des Zahnrads) löschen — das
sind die beiden `<button>`-Blöcke für Modus-Wechsel und Papierkorb
(aktuell Zeilen 236-255 in `Header.tsx`). Das Zahnrad-`<div>` danach
bleibt unverändert erhalten (Zahnrad-Button-Trigger + Panel bleiben in
`Header.tsx` — nur der Öffnungs-Button dafür wird in Task 1 NICHT
verändert; die Rail bekommt einen EIGENEN Zahnrad-Button, der denselben
`settingsOpen`-State toggelt, das Panel selbst rendert weiterhin
`Header.tsx`).

Da das Zahnrad-Panel jetzt von zwei Buttons geöffnet werden kann
(Header-Zahnrad UND Rail-Zahnrad) und beide denselben `settingsOpen`-State
in `App.tsx` steuern, MUSS der Header-Zahnrad-Button ebenfalls entfernt
werden (nicht nur der Panel-Trigger duplizieren) — sonst gibt es zwei
Zahnrad-Icons gleichzeitig. Lösche daher zusätzlich den gesamten
`<div className="relative shrink-0">...</div>`-Block, der den
Header-Zahnrad-Button und das Settings-Panel enthält (aktuell Zeilen
257-475 in `Header.tsx`), UND verschiebe dessen kompletten JSX-Inhalt
(das Panel-`<div>`, beginnend bei `{settingsOpen && (`) 1:1 in eine neue,
von `Rail.tsx` gerenderte Positionierung — siehe Step 5.

- [ ] **Step 5: Settings-Panel von `Header.tsx` nach `Rail.tsx` verschieben**

Das Settings-Panel (aktuelles JSX ab `{settingsOpen && (` bis zum
zugehörigen schließenden `)}`, aktuell Zeilen 265-474 in `Header.tsx`,
mitsamt allen Props wie `themeSetting`, `onThemeChange`, `uiDensity`,
`slicers` usw.) wird nach `Rail.tsx` verschoben. Das heißt:

- `Rail.tsx` bekommt zusätzliche Props: alle Props, die `Header.tsx`
  heute für das Settings-Panel braucht (`themeSetting`, `onThemeChange`,
  `uiDensity`, `onUiDensityChange`, `displayPreference`,
  `onDisplayPreferenceChange`, `slicers`, `onAddSlicer`, `onRemoveSlicer`,
  `onScanCatalogIssues`, `cleanupScanning`, `cleanupError`,
  `onExportCatalog`, `onImportCatalog`, `catalogBackupError` — exakt die
  Liste aus dem aktuellen `Header`-Props-Interface, Zeilen 18-37, MINUS
  der bereits in `Rail` vorhandenen `mainView`/`trashCount`/
  `settingsOpen`-Familie).
- Das Panel-`<div className="absolute top-10 right-0 ...">` wird relativ
  zum Zahnrad-Button in `Rail.tsx` positioniert (`className="relative
  shrink-0"`-Wrapper um Button+Panel, wie es in `Header.tsx` heute schon
  um den Header-Zahnrad-Button existiert) — Positionierung ggf. auf
  `left-12` statt `right-0` anpassen, da die Rail links im Fenster sitzt
  und das Panel nach rechts aufklappen soll, nicht nach links aus dem
  sichtbaren Bereich heraus.
- `Header.tsx` verliert entsprechend dieselben Props aus seinem
  `Props`-Interface und der Destructuring-Zeile, `App.tsx` reicht sie
  stattdessen an `<Rail>` durch. Zusätzlich fallen `settingsOpen` (Zeile
  27) und `onSettingsOpenChange` (Zeile 28) im Interface sowie deren
  Destructuring (Zeilen 71-72) komplett weg, da der Zahnrad-Trigger-Button
  selbst jetzt nur noch in `Rail.tsx` existiert (siehe Step 4).

- [ ] **Step 6: Root-Layout in `App.tsx` auf `flex-row` umstellen**

Aktuell (Zeilen 594-597):
```tsx
    <div
      className="h-screen min-h-[620px] flex flex-col bg-[var(--bg)] text-[var(--ink)] overflow-hidden"
      style={{ fontSize: 14 }}
    >
```
wird zu:
```tsx
    <div
      className="h-screen min-h-[620px] flex flex-row bg-[var(--bg)] text-[var(--ink)] overflow-hidden"
      style={{ fontSize: 14 }}
    >
      <Rail
        mainView={mainView}
        onMainViewChange={(v) => {
          setMainView(v);
          setSelectedId(null);
          clearBulkSelection();
          if (v === 'trash') refreshTrash();
        }}
        trashCount={trashModels.length}
        settingsOpen={settingsOpen}
        onSettingsOpenChange={setSettingsOpen}
        themeSetting={setting}
        onThemeChange={setTheme}
        uiDensity={density}
        onUiDensityChange={setDensity}
        displayPreference={displayPreference}
        onDisplayPreferenceChange={setDisplayPreference}
        slicers={slicers}
        onAddSlicer={addSlicer}
        onRemoveSlicer={removeSlicer}
        onScanCatalogIssues={scanCatalogIssues}
        cleanupScanning={cleanupScanning}
        cleanupError={cleanupError}
        onExportCatalog={exportCatalog}
        onImportCatalog={importCatalog}
        catalogBackupError={catalogBackupError}
      />
      <div className="flex-1 min-w-0 flex flex-col min-h-0">
```
Der bisherige Inhalt (BackgroundSnapshotRenderer, `<Header>`, das
`mainView === 'trash' ? ... : ...`-JSX) bleibt UNVERÄNDERT als Kind dieses
neuen `<div className="flex-1 min-w-0 flex flex-col min-h-0">`. Das
bisherige äußere schließende `</div>` (Zeile, die den Root-`<div>`
schließt) bekommt ein zusätzliches schließendes `</div>` direkt davor,
für den neuen Wrapper.

Der `<Header>`-Aufruf selbst verliert die Props `onMainViewChange` und
`trashCount` (jetzt nur noch an `<Rail>` übergeben) sowie `settingsOpen`/
`onSettingsOpenChange` und alle in Step 5 nach `Rail` verschobenen
Settings-Panel-Props — `mainView` selbst UND `count`/`view`/`onViewChange`/
`sort`/`onSortChange`/`hideSortControl`/`onImportFiles`/`onImportFolder`/
`onImportFolderAsCollection` bleiben unverändert an `<Header>` übergeben
(Header rendert weiterhin die katalog-spezifischen Steuerelemente selbst,
siehe Step 3).

- [ ] **Step 7: Typecheck**

```bash
npx tsc --noEmit
```
Erwartet: keine Fehler. Insbesondere prüfen, dass `Header`s
`Props`-Interface und Destructuring exakt zu den in `App.tsx` noch
übergebenen Props passen (keine übrig gebliebenen, jetzt ungenutzten
Props-Deklarationen in `Header.tsx`).

- [ ] **Step 8: Manueller Smoke-Test**

```bash
npm run tauri dev
```
Prüfen: Rail links sichtbar (3 Icons + Zahnrad unten), Klick auf
Filament-Icon wechselt zur Filament-Ansicht, Klick auf Papierkorb-Icon
zeigt Papierkorb mit korrektem Badge-Count, Zahnrad öffnet dasselbe
Einstellungen-Panel wie vorher (alle Unterabschnitte — Aussehen, Dichte,
Sprache, Slicer, Katalog aufräumen, Katalog-Backup — vorhanden und
funktionsfähig). Kein doppeltes Zahnrad-Icon mehr im Header.

- [ ] **Step 9: Commit**

```bash
git add src/components/Rail.tsx src/components/Header.tsx src/App.tsx src/i18n/
git commit -m "Frontend: Navigationsleiste ersetzt Modus-Wechsel-Button und Papierkorb-Icon im Header"
```

---

### Task 2: Kartensystem zwischen Katalog und Filament-Lager vereinheitlichen

**Files:**
- Modify: `src/components/ModelGrid.tsx` (compact-Kartenvariante, aktuell
  `rounded-[4px]` in der `renderCompactCard`-Funktion)
- Modify: `src/components/CollectionsGallery.tsx` (aktuell
  `rounded-[var(--radius-card)]`)
- Modify: `src/components/FilamentDashboard.tsx` (aktuell bereits
  `rounded-[10px]` — Referenzwert, hier keine Änderung nötig aber als
  Vergleichsmaßstab für die anderen beiden Dateien verwenden)

**Interfaces:**
- Consumes: keine neuen Props — reine CSS-Klassen-Änderung.
- Produces: keine neuen Typen.

- [ ] **Step 1: `ModelGrid.tsx` compact-Karte anpassen**

In der `renderCompactCard`-Funktion (siehe `className={...rounded-[4px]
overflow-hidden border cursor-pointer...}` Zeile ~95) `rounded-[4px]`
durch `rounded-[10px]` ersetzen. NICHT die comfort-Variante (weiter unten
im selben File, nutzt bereits `rounded-[var(--radius-card)]`) anfassen —
die bleibt dichte-abhängig wie bisher.

- [ ] **Step 2: `CollectionsGallery.tsx` Karten-Radius fixieren**

`rounded-[var(--radius-card)]` (in der `className` des Karten-`<div>`,
Zeile ~42) durch `rounded-[10px]` ersetzen, damit Sammlungs-Karten
denselben Radius wie Katalog- und Filament-Karten haben, unabhängig von
der aktuell gewählten UI-Dichte (compact/comfort).

- [ ] **Step 3: Visueller Vergleich**

```bash
npm run tauri dev
```
Katalog (Raster, compact + comfort Dichte über Einstellungen-Panel
umschalten), Sammlungen-Galerie und Filament-Lager (Dashboard-Ansicht)
nacheinander öffnen — alle drei sollen denselben Eckenradius zeigen
(vergleichbar mit dem bereits vorhandenen `rounded-[10px]` der
Filament-Karten).

- [ ] **Step 4: Typecheck**

```bash
npx tsc --noEmit
```

- [ ] **Step 5: Commit**

```bash
git add src/components/ModelGrid.tsx src/components/CollectionsGallery.tsx
git commit -m "Frontend: Kartenradius zwischen Katalog, Sammlungen und Filament-Lager vereinheitlicht (10px)"
```
