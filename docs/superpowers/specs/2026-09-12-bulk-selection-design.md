# Mehrfachauswahl in der Katalogübersicht — Design

## Ausgangslage

Löschen (und jede andere Aktion) funktioniert in der Haupt-Katalogübersicht
aktuell nur pro Modell einzeln (Seitenpanel-Button oder Kontextmenü-Eintrag).
Ein ähnliches Checkbox-Auswahlmuster existiert bereits im Aufräum-Vorschläge-
Dialog (`CatalogCleanupDialog.tsx`, `<input type="checkbox">` pro Zeile), der
intern bereits den vorhandenen Batch-Command `delete_files` nutzt — dieses
Feature bringt dasselbe Grundmuster in die normale Grid-/Listenansicht,
plus zwei weitere Bulk-Aktionen.

## Ziel

In Grid- und Listenansicht bekommt jede Karte/Zeile eine dauerhaft sichtbare
Checkbox (kein eigener "Auswählen"-Modus nötig). Sobald mindestens ein
Modell angehakt ist, erscheint eine Aktionsleiste mit:

- **Löschen** (mit Bestätigung, verschiebt alle Ausgewählten in den
  Papierkorb)
- **Zur Warteschlange hinzufügen**
- **Als gedruckt markieren** / **Als nicht gedruckt markieren** (zwei
  getrennte Buttons — bei gemischtem Druckstatus in der Auswahl gibt es
  sonst kein eindeutiges Verhalten)

Plus "Alle auswählen" (wählt alle aktuell **gefilterten/sichtbaren**
Modelle, nicht zwingend den gesamten Katalog) und "Auswahl aufheben".

## Architektur

### 1. Neuer Auswahl-State (`App.tsx`)

```typescript
const [selectedForBulk, setSelectedForBulk] = useState<Set<string>>(new Set());
```

Auswahl wird zurückgesetzt, sobald sich der aktive Filter ändert (Ordner,
Tag, Creator oder Suchtext) — ein neuer `useEffect` mit
`[activeFolderId, activeTag, activeCreator, query]` als Dependency-Array
ruft `setSelectedForBulk(new Set())` auf. Verhindert, dass eine Bulk-Aktion
versehentlich auf Modelle wirkt, die durch den Filterwechsel gar nicht mehr
sichtbar sind.

Neue Handler:

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
    refreshFolders();
    refreshTags();
    refreshCreators();
    refreshTrash();
  });
};

const bulkAddToQueue = () => {
  Promise.all(Array.from(selectedForBulk).map((id) => invoke('add_to_queue', { fileId: id })))
    .then(() => invoke<ModelFile[]>('list_files').then(setModels));
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

`bulkDelete` nutzt den bereits vorhandenen `delete_files`-Batch-Command
(gleicher Command wie im Aufräum-Dialog) — kein neuer Backend-Code nötig.
`bulkAddToQueue`/`bulkSetPrintStatus` rufen die bestehenden Einzel-Commands
je ausgewählter ID parallel auf (`Promise.all`) statt einen neuen
Batch-Command zu bauen — beides sind leichte lokale SQLite-Updates ohne
Datei-I/O, ein Mehrwert eines eigenen Backend-Batch-Endpunkts wäre hier
nicht erkennbar (YAGNI).

### 2. Checkbox in `ModelGrid.tsx`/`ModelList.tsx`

Neue Props auf beiden Komponenten:

```typescript
selectedForBulk: Set<string>;
onToggleBulkSelect: (id: string) => void;
```

In beiden Karten-Renderern von `ModelGrid.tsx` (Kompakt- und
Komfort-Variante) sowie in `ModelList.tsx`s Zeilen-`<div>`: eine Checkbox
oben links auf der Karte (Grid) bzw. am Zeilenanfang (Liste), analog zum
bestehenden Muster in `CatalogCleanupDialog.tsx`:

```typescript
<input
  type="checkbox"
  checked={selectedForBulk.has(m.id)}
  onClick={(e) => e.stopPropagation()}
  onChange={() => onToggleBulkSelect(m.id)}
  className="absolute top-1.5 left-1.5 z-10 w-4 h-4 cursor-pointer"
/>
```

`e.stopPropagation()` im `onClick` ist notwendig, damit ein Checkbox-Klick
nicht zusätzlich den umschließenden `onClick`/`onDoubleClick` der Karte
auslöst (Panel-Vorschau bzw. Detailseite) — das bestehende Kontextmenü-
`onContextMenu`, `onSelect` und `onOpenDetail` (Doppelklick) bleiben
unverändert bestehen und funktionieren weiter unabhängig von der
Checkbox-Auswahl. Bei `readOnly` (Papierkorb-Ansicht) wird die Checkbox
nicht gerendert — Mehrfachauswahl ist auf die normale Katalogansicht
beschränkt.

### 3. Aktionsleiste (`App.tsx`)

Ersetzt bedingt die bestehende Ordner-/Tag-Chip-Zeile im Hauptbereich,
wenn `selectedForBulk.size > 0`:

```typescript
{selectedForBulk.size > 0 && (
  <div className="flex items-center gap-2 px-4 py-2 border-b border-[var(--line)] bg-[var(--panel-2)]">
    <span className="text-[12.5px] font-medium">
      {t('bulkSelectedCount').replace('{count}', String(selectedForBulk.size))}
    </span>
    <button onClick={selectAllVisible} className="...">{t('selectAllLabel')}</button>
    <button onClick={clearBulkSelection} className="...">{t('clearSelectionLabel')}</button>
    <span className="flex-1" />
    <button onClick={bulkAddToQueue} className="...">{t('addToQueue')}</button>
    <button onClick={() => bulkSetPrintStatus('printed')} className="...">{t('printedBadge')}</button>
    <button onClick={() => bulkSetPrintStatus('not_printed')} className="...">{t('notPrintedLabel')}</button>
    <button onClick={() => setConfirmBulkDelete(true)} className="...">{t('delete')}</button>
  </div>
)}
```

`confirmBulkDelete`-Bestätigung folgt dem bestehenden Inline-Bestätigungs-
Muster aus `DetailPanel.tsx` (Text + Abbrechen/Bestätigen-Button statt
eines separaten Dialogs) — bei mehreren Dateien auf einmal ist eine
Bestätigung sinnvoll, auch wenn sie über den Papierkorb reversibel bleibt.

`t('addToQueue')`, `t('printedBadge')`, `t('notPrintedLabel')`, `t('delete')`
sind bereits vorhandene i18n-Keys (aus `DetailPanel.tsx`/`ModelDetailPage.tsx`
übernommen). Neue Keys: `bulkSelectedCount` (mit `{count}`-Platzhalter,
gleiches Ersetzungsmuster wie `trashExpiryHint`), `selectAllLabel`,
`clearSelectionLabel`.

## Fehlerbehandlung

- `bulkDelete`/`bulkAddToQueue`/`bulkSetPrintStatus` haben kein explizites
  `.catch()` — folgt dem bestehenden Muster von `deleteModel`/
  `togglePrintStatus` (Fehler werden aktuell projektweit nicht mit
  Toast/Banner angezeigt, nur bei einzelnen Stellen wie `slicerError` mit
  eigenem State). Kein neues Fehlerverhalten für dieses Feature einführen,
  das über das bestehende Niveau hinausgeht.
- `delete_files` (Backend) toleriert bereits einzelne fehlschlagende IDs
  im Batch (log-and-continue, siehe bestehender Code) — keine Änderung
  nötig.

## Testing

- Frontend: `npx tsc --noEmit` sauber. Live-Verifikation im laufenden
  `npm run tauri dev`: mehrere Checkboxen anhaken, Aktionsleiste erscheint
  mit korrekter Anzahl; "Alle auswählen" wählt nur die aktuell gefilterten
  Modelle (z. B. bei aktivem Tag-Filter); Filterwechsel setzt die Auswahl
  zurück; Löschen mehrerer Modelle verschiebt sie alle in den Papierkorb
  (per DB-Check verifizierbar, analog zu den vorherigen Feature-Tests
  dieses Projekts); Checkbox-Klick öffnet NICHT das Seitenpanel/die
  Detailseite.

## Out of Scope

- Keine weiteren Bulk-Aktionen (Tag hinzufügen, Favorit) in dieser
  Ausbaustufe — explizit auf Löschen/Warteschlange/Druckstatus begrenzt.
- Keine Mehrfachauswahl in der Papierkorb-Ansicht (`readOnly`-Karten
  zeigen keine Checkbox) — Wiederherstellen/Endgültig-löschen bleiben dort
  Einzelaktionen im Seitenpanel.
- Kein neuer Backend-Batch-Command für Warteschlange/Druckstatus — beide
  laufen über parallele Einzel-Command-Aufrufe vom Frontend aus.
- Keine Tastatur-Shortcuts (Shift-Klick für Bereichsauswahl, Strg+A) in
  dieser Ausbaustufe.
