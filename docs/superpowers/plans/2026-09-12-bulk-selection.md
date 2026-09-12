# Mehrfachauswahl in der Katalogübersicht Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Checkbox-Mehrfachauswahl in Grid- und Listenansicht mit "Alle auswählen" und einer Aktionsleiste für Löschen/Warteschlange/Druckstatus.

**Architecture:** Neuer `Set<string>`-State in `App.tsx`, neue optionale Props auf `ModelGrid`/`ModelList` für die Checkbox-UI, Aktionsleiste rendert bedingt unterhalb der bestehenden Ordner-/Tag-Chip-Zeile. Löschen nutzt den bereits vorhandenen `delete_files`-Batch-Command; Warteschlange/Druckstatus laufen über parallele Aufrufe der bestehenden Einzel-Commands (der globale `Mutex<Connection>` im Backend serialisiert sie ohnehin korrekt, kein neuer Batch-Code nötig).

**Tech Stack:** React 19 + TypeScript, Tailwind (Utility-Klassen).

## Global Constraints

- `npx tsc --noEmit` (Projekt-Root) muss nach jedem Task fehlerfrei sein.
- `Translations`-Interface erzwingt Vollständigkeit — jeder neue i18n-Key MUSS in `src/i18n/types.ts` UND allen 4 Sprachdateien (`de.ts`, `en.ts`, `es.ts`, `fr.ts`) ergänzt werden.
- Checkbox-Klicks dürfen NIE den umschließenden Karten-`onClick`/`onDoubleClick` mit auslösen (`e.stopPropagation()` im Checkbox-`onClick`).
- Checkbox wird nicht gerendert, wenn `readOnly` gesetzt ist (Papierkorb-Ansicht bleibt ohne Mehrfachauswahl).
- Auswahl wird bei Filterwechsel (Ordner/Tag/Creator/Suche) automatisch zurückgesetzt.
- Kein neuer Backend-Code in diesem Plan — nur Frontend, bestehende Commands (`delete_files`, `add_to_queue`, `set_print_status`) werden wiederverwendet.
- Commit-Attribution: `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` unter jedem Commit, plus `Claude-Session: https://claude.ai/code/session_01WAQ5i49v1oEqoX5RzCBUz6`.

---

### Task 1: State, Handler und i18n-Keys in `App.tsx`

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/i18n/types.ts`, `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts`

**Interfaces:**
- Produces: `selectedForBulk: Set<string>`, `toggleBulkSelect(id: string): void`, `selectAllVisible(): void`, `clearBulkSelection(): void`, `bulkDelete(): void`, `bulkAddToQueue(): void`, `bulkSetPrintStatus(status: 'printed' | 'not_printed'): void`, `confirmBulkDelete: boolean`, `setConfirmBulkDelete: (v: boolean) => void` — für Task 3 (Aktionsleiste + Props an `ModelGrid`/`ModelList`).

- [ ] **Step 1: Neue i18n-Keys**

`src/i18n/types.ts`, im `Translations`-Interface neben `trashHeading` o. ä.:

```typescript
  bulkSelectedCount: string;
  selectAllLabel: string;
  clearSelectionLabel: string;
  bulkDeleteConfirmQuestion: string;
```

`src/i18n/de.ts`:

```typescript
  bulkSelectedCount: '{count} ausgewählt',
  selectAllLabel: 'Alle auswählen',
  clearSelectionLabel: 'Auswahl aufheben',
  bulkDeleteConfirmQuestion: '{count} Modelle löschen?',
```

`src/i18n/en.ts`:

```typescript
  bulkSelectedCount: '{count} selected',
  selectAllLabel: 'Select all',
  clearSelectionLabel: 'Clear selection',
  bulkDeleteConfirmQuestion: 'Delete {count} models?',
```

`src/i18n/es.ts`:

```typescript
  bulkSelectedCount: '{count} seleccionados',
  selectAllLabel: 'Seleccionar todo',
  clearSelectionLabel: 'Deseleccionar',
  bulkDeleteConfirmQuestion: '¿Eliminar {count} modelos?',
```

`src/i18n/fr.ts`:

```typescript
  bulkSelectedCount: '{count} sélectionné(s)',
  selectAllLabel: 'Tout sélectionner',
  clearSelectionLabel: 'Désélectionner',
  bulkDeleteConfirmQuestion: 'Supprimer {count} modèles ?',
```

- [ ] **Step 2: `tsc` prüfen**

Run: `npx tsc --noEmit`
Expected: kompiliert fehlerfrei (Keys noch nicht verwendet, aber alle 4 Sprachdateien müssen vollständig sein).

- [ ] **Step 3: State und Handler in `App.tsx`**

Neuer State neben `const [selectedId, setSelectedId] = useState<string | null>(null);`:

```typescript
  const [selectedForBulk, setSelectedForBulk] = useState<Set<string>>(new Set());
  const [confirmBulkDelete, setConfirmBulkDelete] = useState(false);
```

Neuer `useEffect`, der die Auswahl bei Filterwechsel zurücksetzt (neben anderen bestehenden `useEffect`-Aufrufen einfügen):

```typescript
  useEffect(() => {
    setSelectedForBulk(new Set());
  }, [activeFolderId, activeTag, activeCreator, query]);
```

Neue Handler (neben `deleteModel`/`togglePrintStatus`/`addToQueue` einfügen):

```typescript
  const toggleBulkSelect = (id: string) => {
    setSelectedForBulk((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  const selectAllVisible = () => setSelectedForBulk(new Set(filtered.map((m) => m.id)));
  const clearBulkSelection = () => setSelectedForBulk(new Set());

  const bulkDelete = () => {
    invoke('delete_files', { fileIds: Array.from(selectedForBulk) }).then(() => {
      setModels((prev) => prev.filter((m) => !selectedForBulk.has(m.id)));
      clearBulkSelection();
      setConfirmBulkDelete(false);
      refreshFolders();
      refreshTags();
      refreshCreators();
      refreshTrash();
    });
  };

  const bulkAddToQueue = () => {
    Promise.all(Array.from(selectedForBulk).map((id) => invoke('add_to_queue', { fileId: id }))).then(
      () => invoke<ModelFile[]>('list_files').then(setModels),
    );
  };

  const bulkSetPrintStatus = (status: 'printed' | 'not_printed') => {
    Promise.all(
      Array.from(selectedForBulk).map((id) => invoke('set_print_status', { fileId: id, status })),
    ).then(() => {
      setModels((prev) =>
        prev.map((m) =>
          selectedForBulk.has(m.id)
            ? { ...m, printStatus: status, queuePosition: status === 'printed' ? null : m.queuePosition }
            : m,
        ),
      );
    });
  };
```

`filtered` ist die bereits bestehende `useMemo`-Variable (gefilterte/sortierte Modell-Liste, die auch an `ModelGrid`/`ModelList` übergeben wird) — `selectAllVisible` nutzt exakt diese, damit nur sichtbare Modelle ausgewählt werden.

- [ ] **Step 4: `tsc` verifizieren**

Run: `npx tsc --noEmit`
Expected: kompiliert fehlerfrei. Die neuen Handler werden in diesem Task noch nirgends im JSX verwendet — falls `noUnusedLocals`/`noUnusedParameters` das als Fehler markiert (siehe Erfahrung aus dem Papierkorb-Plan), gezielt `@ts-expect-error`-Kommentare direkt über den betroffenen Deklarationen ergänzen (werden in Task 3 wieder entfernt, sobald die Werte im JSX verwendet werden).

- [ ] **Step 5: Commit**

```bash
cd /home/andreasm/Projekte/3mf-katalog-manager
git add src/App.tsx src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "$(cat <<'EOF'
Mehrfachauswahl: State, Handler und i18n-Keys in App.tsx

selectedForBulk (Set<string>), zurückgesetzt bei Filterwechsel.
bulkDelete nutzt den bestehenden delete_files-Batch-Command,
bulkAddToQueue/bulkSetPrintStatus rufen die bestehenden Einzel-
Commands parallel auf. Noch nicht im JSX verdrahtet (folgt in Task 3).

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01WAQ5i49v1oEqoX5RzCBUz6
EOF
)"
```

---

### Task 2: Checkbox-UI in `ModelGrid.tsx`/`ModelList.tsx`

**Files:**
- Modify: `src/components/ModelGrid.tsx`
- Modify: `src/components/ModelList.tsx`

**Interfaces:**
- Consumes: nichts aus Task 1 (reine Komponenten-Props-Erweiterung, unabhängig von `App.tsx`s internem State).
- Produces: neue Props `selectedForBulk: Set<string>` und `onToggleBulkSelect: (id: string) => void` auf beiden Komponenten — für Task 3 (Verdrahtung in `App.tsx`).

- [ ] **Step 1: `ModelGrid.tsx` — Props und Checkbox in beiden Karten-Renderern**

`Props`-Interface erweitern:

```typescript
  selectedForBulk: Set<string>;
  onToggleBulkSelect: (id: string) => void;
```

Funktionssignatur entsprechend erweitern: `export function ModelGrid({ models, selectedId, onSelect, onOpenDetail, onContextMenu, onToggleFavorite, readOnly, selectedForBulk, onToggleBulkSelect }: Props) {`.

In `renderCompactCard` (der erste `<div className="relative aspect-square ...">`-Thumbnail-Container, direkt vor dem `{m.displayImage ? (...) : (...)}`-Block): als allererstes Kind ergänzen (nur wenn nicht `readOnly`):

```typescript
          {!readOnly && (
            <input
              type="checkbox"
              checked={selectedForBulk.has(m.id)}
              onClick={(e) => e.stopPropagation()}
              onChange={() => onToggleBulkSelect(m.id)}
              className="absolute top-1.5 left-1.5 z-10 w-4 h-4 cursor-pointer"
            />
          )}
```

Denselben Block an der analogen Stelle im zweiten (Komfort-)Karten-Renderer ergänzen — dort beginnt der Thumbnail-Container mit `<div className="relative aspect-square bg-[var(--plate)] overflow-hidden">` (per Grep in der Datei finden, zweites Vorkommen von `relative aspect-square`).

- [ ] **Step 2: `ModelList.tsx` — Props und Checkbox-Spalte**

`Props`-Interface erweitern (analog zu Step 1):

```typescript
  selectedForBulk: Set<string>;
  onToggleBulkSelect: (id: string) => void;
```

Funktionssignatur entsprechend erweitern.

Sowohl die Header-Zeile als auch jede Modell-Zeile nutzen `gridTemplateColumns: 'minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px'` — beide auf `'24px minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px'` ändern (neue erste Spalte für die Checkbox).

In der Header-Zeile, als erstes Kind vor `<span>{t('columnName')}</span>`:

```typescript
        <span />
```

(leere Spaltenüberschrift für die Checkbox-Spalte).

In jeder Modell-Zeile, als allererstes Kind vor `<span className="text-[length:var(--font-size-body)] ...">{m.name}</span>`:

```typescript
          {!readOnly && (
            <input
              type="checkbox"
              checked={selectedForBulk.has(m.id)}
              onClick={(e) => e.stopPropagation()}
              onChange={() => onToggleBulkSelect(m.id)}
              className="w-4 h-4 cursor-pointer justify-self-center"
            />
          )}
```

Ist `readOnly` gesetzt, bleibt die Checkbox-Spalte leer (kein `<span />`-Ersatz nötig — CSS Grid lässt die Zelle einfach frei, das Spaltenraster bleibt durch die feste `24px`-Breite erhalten).

- [ ] **Step 3: `tsc` verifizieren**

Run: `npx tsc --noEmit`
Expected: kompiliert fehlerfrei. In diesem Task werden `ModelGrid`/`ModelList` isoliert geändert — ihre Aufrufer in `App.tsx` übergeben die neuen Props noch nicht (folgt in Task 3), daher schlägt `tsc` an DIESER Stelle erwartungsgemäß mit "missing property" fehl, bis Task 3 abgeschlossen ist. Notiere das im Report als bekannten Zwischenzustand statt zu versuchen, es in diesem Task zu beheben (die Aufrufer gehören nicht zu den in diesem Task genannten Dateien).

- [ ] **Step 4: Commit**

```bash
cd /home/andreasm/Projekte/3mf-katalog-manager
git add src/components/ModelGrid.tsx src/components/ModelList.tsx
git commit -m "$(cat <<'EOF'
Mehrfachauswahl: Checkbox-UI in ModelGrid/ModelList

Neue Props selectedForBulk/onToggleBulkSelect. Checkbox oben links auf
jeder Karte (beide Dichte-Varianten) bzw. als neue erste Grid-Spalte
in der Listenansicht, ausgeblendet bei readOnly (Papierkorb-Ansicht).
Klick auf die Checkbox stoppt die Event-Weitergabe, damit weder
Panel-Vorschau noch Detailseiten-Doppelklick mit ausgelöst werden.
App.tsx übergibt die neuen Props noch nicht (folgt in Task 3) - tsc
zeigt bis dahin erwartungsgemäß einen Zwischenfehler an den
Aufrufstellen.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01WAQ5i49v1oEqoX5RzCBUz6
EOF
)"
```

---

### Task 3: Aktionsleiste in `App.tsx` verdrahten

**Files:**
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: State/Handler aus Task 1, `selectedForBulk`/`onToggleBulkSelect`-Props aus Task 2.

- [ ] **Step 1: `ModelGrid`/`ModelList`-Aufrufe um die neuen Props ergänzen**

An BEIDEN Stellen, an denen `<ModelGrid .../>` bzw. `<ModelList .../>` im normalen Katalog-Zweig gerendert werden (nicht im Papierkorb-Zweig — dort bleiben sie ohne diese Props, `readOnly` blendet die Checkbox ohnehin aus, aber die Props sind laut Interface nicht optional, also müssen sie auch dort übergeben werden, einfach mit `selectedForBulk={new Set()}`/`onToggleBulkSelect={() => {}}` als No-Op, da im Papierkorb-Zweig ohnehin nie eine Checkbox gerendert wird):

```typescript
                selectedForBulk={selectedForBulk}
                onToggleBulkSelect={toggleBulkSelect}
```

- [ ] **Step 2: Aktionsleiste einfügen**

Direkt NACH der bestehenden Ordner-/Tag-Chip-Zeile (`<div className="flex-none h-[38px] flex items-center gap-2.5 px-4 border-b border-[var(--line)] bg-[var(--bg)]">...</div>`, im normalen Katalog-Hauptbereich) eine neue, bedingt gerenderte Zeile ergänzen:

```typescript
            {selectedForBulk.size > 0 && (
              <div className="flex-none flex items-center gap-2 px-4 py-2 border-b border-[var(--line)] bg-[var(--panel-2)]">
                {confirmBulkDelete ? (
                  <>
                    <span className="text-[12.5px] font-medium text-[var(--ink)]">
                      {t('bulkDeleteConfirmQuestion').replace('{count}', String(selectedForBulk.size))}
                    </span>
                    <button
                      onClick={() => setConfirmBulkDelete(false)}
                      className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                    >
                      {t('cancel')}
                    </button>
                    <button
                      onClick={bulkDelete}
                      className="h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
                    >
                      {t('delete')}
                    </button>
                  </>
                ) : (
                  <>
                    <span className="text-[12.5px] font-medium text-[var(--ink)]">
                      {t('bulkSelectedCount').replace('{count}', String(selectedForBulk.size))}
                    </span>
                    <button onClick={selectAllVisible} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
                      {t('selectAllLabel')}
                    </button>
                    <button onClick={clearBulkSelection} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
                      {t('clearSelectionLabel')}
                    </button>
                    <span className="flex-1" />
                    <button onClick={bulkAddToQueue} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
                      {t('addToQueue')}
                    </button>
                    <button onClick={() => bulkSetPrintStatus('printed')} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
                      {t('printedBadge')}
                    </button>
                    <button onClick={() => bulkSetPrintStatus('not_printed')} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
                      {t('notPrintedLabel')}
                    </button>
                    <button onClick={() => setConfirmBulkDelete(true)} className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-red-400 text-[12.5px] font-semibold cursor-pointer hover:border-red-400">
                      {t('delete')}
                    </button>
                  </>
                )}
              </div>
            )}
```

`t('addToQueue')`, `t('printedBadge')`, `t('notPrintedLabel')`, `t('delete')`, `t('cancel')` sind bereits vorhandene, verifizierte i18n-Keys (siehe vorherige Pläne dieses Projekts, u. a. `docs/superpowers/plans/2026-09-12-model-detail-page.md` Task 4 für die exakten Namen) — vor dem Schreiben trotzdem per Grep `'addToQueue'\|'printedBadge'\|'notPrintedLabel'` in `src/i18n/types.ts` verifizieren, dass sie weiterhin exakt so heißen.

- [ ] **Step 3: `tsc` verifizieren**

Run: `npx tsc --noEmit`
Expected: kompiliert fehlerfrei — der in Task 2 erwartete Zwischenfehler an den `ModelGrid`/`ModelList`-Aufrufstellen ist jetzt behoben. Alle in Task 1 ggf. gesetzten `@ts-expect-error`-Kommentare (falls welche nötig waren) MÜSSEN jetzt entfernt sein, da die Werte jetzt echt verwendet werden — sonst schlägt `tsc` mit "Unused '@ts-expect-error' directive" fehl.

- [ ] **Step 4: Live-Verifikation**

`npm run tauri dev` im Hintergrund starten, kurz auf sauberen Start ohne Laufzeitfehler in der Konsole prüfen, danach beenden. Ein echter interaktiver Klick-Test (Checkboxen anhaken, "Alle auswählen", Bulk-Löschen) ist für dich als Agent nicht möglich — das ist in Ordnung, reiner Compile-/Start-Check reicht für diesen Task.

- [ ] **Step 5: Commit**

```bash
cd /home/andreasm/Projekte/3mf-katalog-manager
git add src/App.tsx
git commit -m "$(cat <<'EOF'
Mehrfachauswahl: Aktionsleiste in App.tsx verdrahtet

ModelGrid/ModelList bekommen die neuen Bulk-Auswahl-Props. Neue
Aktionsleiste unterhalb der Ordner-/Tag-Chip-Zeile, sichtbar sobald
mindestens ein Modell angehakt ist: Alle-auswählen/Auswahl-aufheben,
Zur-Warteschlange, Als-gedruckt/nicht-gedruckt, Löschen mit
Inline-Bestätigung (gleiches Muster wie DetailPanels confirmDelete).

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01WAQ5i49v1oEqoX5RzCBUz6
EOF
)"
```

---

### Task 4: Voller Verifikationslauf

**Files:** keine Änderungen, nur Verifikation.

- [ ] **Step 1: Frontend voll bauen**

Run: `cd /home/andreasm/Projekte/3mf-katalog-manager && npm run build`
Expected: `tsc && vite build` fehlerfrei.

- [ ] **Step 2: Backend unverändert grün**

Run: `cd src-tauri && cargo test`
Expected: weiterhin 86/86 grün (dieser Plan ändert kein Rust-Backend).

- [ ] **Step 3: Live-Smoke-Test**

`npm run tauri dev` starten, kurz auf sauberen Start prüfen. Da ein echter interaktiver Test (Checkboxen klicken) für einen Agenten nicht möglich ist: den Controller (dich) bitten, im laufenden Dev-Server manuell mehrere Checkboxen anzuhaken, "Alle auswählen" zu testen (respektiert es den aktiven Filter?), einen Filterwechsel zu prüfen (Auswahl muss verschwinden), und eine Bulk-Löschung durchzuführen (Dateien müssen danach im Papierkorb landen, per `sqlite3 ~/.local/share/com.thebexxs.mfkatalogmanager/catalog.db "SELECT count(*) FROM files WHERE deleted_at IS NOT NULL;"` verifizierbar). Dev-Server danach beenden.

- [ ] **Step 4: Kein Commit in diesem Task** (reiner Verifikationsschritt; bei gefundenen Problemen zurück zum jeweiligen Task, dort fixen und dort erneut committen).
