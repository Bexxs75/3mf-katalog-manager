# Sammlungen — Design

## Kontext

Aus `README.md`, Abschnitt „Geplant": „zusätzlich zu Tags eine Möglichkeit,
mehrere Modelle explizit zu einem eigenen Projekt zusammenzufassen (z. B.
alle Drucke für ein bestimmtes Bauvorhaben), unabhängig von der
Hashtag-Logik."

Das Projekt hat bereits zwei Organisationsmechanismen:
- **Ordner** (`folders`-Tabelle, `files.folder_id`): eine physische, beim
  Import gespiegelte Verzeichnisstruktur — ein Modell gehört zu genau
  einem Ordner.
- **Tags** (viele-zu-viele, unsortiert): freie Hashtag-Markierung ohne
  Reihenfolge.

**Sammlungen** sind ein dritter Mechanismus: viele-zu-viele wie Tags,
aber mit einer **manuell festlegbaren Reihenfolge** pro Sammlung — das
ist der eigentliche Mehrwert gegenüber einem Tag. Typischer Anwendungsfall:
ein Bauvorhaben, das aus mehreren Druckteilen (unterschiedliche Dateien,
teils sogar aus verschiedenen Ordnern) besteht und in einer bestimmten
Druckreihenfolge abgearbeitet werden soll.

## Datenmodell

Zwei neue Tabellen in `src-tauri/src/db/schema.sql`:

```sql
CREATE TABLE IF NOT EXISTS collections (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS collection_files (
    collection_id INTEGER NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    UNIQUE (collection_id, file_id)
);

CREATE INDEX IF NOT EXISTS idx_collection_files_collection_id
    ON collection_files (collection_id);
CREATE INDEX IF NOT EXISTS idx_collection_files_file_id
    ON collection_files (file_id);
```

`ON DELETE CASCADE` sorgt dafür, dass beim Löschen einer Sammlung nur die
Zuordnungen verschwinden (nie die Modelle selbst — `files` ist davon nicht
betroffen), und dass beim Löschen eines Modells (auch beim Wandern in den
Papierkorb *nicht*, da das kein `DELETE`, sondern ein Soft-Delete-Update
ist) automatisch aufgeräumt wird, falls eine Datei doch einmal
unwiderruflich per `delete_file_permanently` entfernt wird.

## Backend-Commands

Neue Datei `src-tauri/src/db/collections.rs` (Repository-Funktionen,
analog zum bestehenden `src-tauri/src/db/repository.rs`-Muster) plus
neue Tauri-Commands in `commands.rs`:

- `list_collections() -> Vec<CollectionDto>` — `CollectionDto { id, name, modelCount }`, sortiert nach `created_at`.
- `create_collection(name: String) -> CollectionDto` — legt eine neue, leere Sammlung an.
- `rename_collection(id: String, name: String) -> CmdResult<()>`.
- `delete_collection(id: String) -> CmdResult<()>` — löscht nur die Sammlung + ihre Zuordnungen (Cascade), keine Dateien.
- `add_files_to_collection(collection_id: String, file_ids: Vec<String>) -> CmdResult<()>` — hängt die übergebenen Dateien ans Ende der bestehenden Reihenfolge an; bereits enthaltene Dateien werden übersprungen (kein Duplikat, keine Neu-Positionierung).
- `remove_file_from_collection(collection_id: String, file_id: String) -> CmdResult<()>`.
- `reorder_collection(collection_id: String, ordered_file_ids: Vec<String>) -> CmdResult<()>` — setzt `position` für jede ID entsprechend ihres Index in der übergebenen Liste (gleiches Muster wie das bestehende `reorder_queue`).
- `list_collection_files(collection_id: String) -> Vec<ModelFileDto>` — Modelle der Sammlung, sortiert nach `position`.
- `import_folder_as_collection() -> CmdResult<ImportResultDto>` — öffnet denselben Ordner-Auswahldialog wie das bestehende `import_folder`, importiert identisch (ruft intern dieselbe `import_many`-Logik), legt danach automatisch eine neue Sammlung an (Name = Ordnername aus dem gewählten Pfad) und ordnet ihr alle aus diesem Import resultierenden Dateien zu (in der Reihenfolge, in der `import_many` sie zurückgibt) — inklusive bereits zuvor katalogisierter Dateien im selben Ordner (Duplikate werden zwar nicht erneut importiert, aber trotzdem der neuen Sammlung zugeordnet, damit „alle Dateien aus diesem Ordner" wirklich vollständig in der Sammlung landen).

## Frontend-Datenmodell

Neuer Typ in `src/types/index.ts`:

```ts
export interface Collection {
  id: string;
  name: string;
  modelCount: number;
}
```

`App.tsx` hält `collections: Collection[]` (geladen via `list_collections`
im bestehenden Mount-`useEffect`, analog zu `refreshFolders`/`refreshTags`)
sowie `activeCollection: string | null` (analog zu `activeTag`).

## Navigation & Ansicht

Neben dem bestehenden Breadcrumb-Label (`folders.find(...).name` /
„Alle Modelle") erscheint ein zusätzlicher Reiter „Sammlungen"
(`t('collectionsTab')`). Neuer State in `App.tsx`:
`const [collectionsGalleryOpen, setCollectionsGalleryOpen] = useState(false);`

- **Reiter „Sammlungen" angeklickt:** `collectionsGalleryOpen = true`, `activeCollection = null`. Der Hauptbereich zeigt statt `ModelGrid` eine neue Komponente `CollectionsGallery` — Karten pro Sammlung (Name, `t('modelCountLabel')` mit `modelCount`, Lösch-/Umbenennen-Icons wie bei den bestehenden ✕-Mustern in `Header.tsx`), plus eine „+ Neue Sammlung"-Karte (Klick öffnet ein Inline-Textfeld für den Namen, analog zum bestehenden Muster für neue Slicer/Tags).
- **Sammlungs-Karte angeklickt:** `activeCollection = <id>`, `collectionsGalleryOpen = false`. Der Hauptbereich zeigt wieder `ModelGrid`, aber gespeist aus `list_collection_files(activeCollection)` statt der normalen gefilterten `models`-Liste. Das Sortieren-Dropdown im Header wird ausgeblendet (`activeCollection` als zusätzliche Bedingung neben der bestehenden `view`-Prüfung), stattdessen ist Drag-Umsortieren aktiv (siehe unten). Ein Klick auf „Sammlungen" im Breadcrumb (jetzt als Link/Zurück-Pfad sichtbar, sobald `activeCollection` gesetzt ist) führt zurück zur Kartenübersicht.
- Normale Filter (`activeFolderId`, `activeTag`, `activeCreator`, Suche) sind währenddessen deaktiviert/ausgeblendet — eine Sammlung ist ein eigener, expliziter Anzeigemodus, keine zusätzliche UND-Bedingung zu den bestehenden Filtern.

## Modelle zu einer Sammlung hinzufügen

Über die bestehende Mehrfachauswahl-Aktionsleiste (`src/App.tsx`, dort wo
bereits „Zur Warteschlange hinzufügen"/„Als gedruckt markieren" sitzen):
neuer Button „Zu Sammlung hinzufügen" (`t('addToCollectionLabel')`), öffnet
ein kleines Dropdown/Popover mit der Liste bestehender Sammlungen plus
einer „+ Neue Sammlung..."-Option (Inline-Namensfeld) — Klick auf eine
Sammlung ruft `add_files_to_collection` mit den aktuell markierten
Datei-IDs auf.

Ist `activeCollection` aktiv (man befindet sich in einer
Sammlungs-Detailansicht), zeigt die Aktionsleiste zusätzlich „Aus
Sammlung entfernen" statt/neben dem normalen Löschen — ruft
`remove_file_from_collection` für jede markierte Datei auf (entfernt nur
die Zuordnung, keine Datei).

## Reihenfolge per Drag & Drop

**Wichtige bestehende Einschränkung** (aus `Sidebar.tsx`, Warteschlange):
natives HTML5-Drag&Drop (`draggable`/`onDragStart`/`onDragOver`/`onDrop`)
funktioniert in dieser App nicht zuverlässig, weil Tauris
`dragDropEnabled` (Standard, wird für den OS-Datei-Import-Drop benötigt)
native Drag-Sessions auf Fenster-Ebene abfängt und dadurch In-Page-HTML5-DnD
unter WebKitGTK blockiert. Die Warteschlange in `Sidebar.tsx` löst das
über einen rein JS-gesteuerten Maus-Event-Ablauf
(`mousedown`→`dragIndex` setzen, `mouseenter`→`overIndex` setzen,
globaler `mouseup`-Listener→Reihenfolge tauschen + `onQueueReorder`
aufrufen).

Die Sammlungs-Detailansicht (`ModelGrid` mit `activeCollection` gesetzt)
übernimmt **exakt dasselbe Muster**, nicht natives HTML5-DnD: die Karten
bekommen `onMouseDown`/`onMouseEnter`, ein `useEffect` mit
`document.addEventListener('mouseup', ...)` berechnet die neue
Reihenfolge und ruft `reorder_collection` auf. `ModelGrid` bekommt dafür
neue optionale Props `reorderable?: boolean` und
`onReorder?: (orderedIds: string[]) => void`, aktiv nur wenn
`activeCollection` gesetzt ist (die normale Katalogansicht bleibt
unverändert, kein Drag-Verhalten dort).

## i18n

Neue Schlüssel (`types.ts` + alle 4 Sprachdateien): `collectionsTab`,
`modelCountLabel` (mit Pluralformen nach bestehendem `formatCount`-Muster,
z. B. `PluralForms`), `addToCollectionLabel`, `newCollectionPlaceholder`,
`removeFromCollectionLabel`, `deleteCollectionConfirmQuestion`,
`renameCollectionAria`, `noCollectionsEmptyState`, `backToCollectionsLabel`.

## Fehlerbehandlung

- `create_collection` mit leerem/nur-Leerzeichen-Namen: Frontend
  verhindert das Absenden (wie bei Tag-/Slicer-Namensfeldern bereits
  üblich), kein serverseitiger Zwang nötig.
- `add_files_to_collection`/`remove_file_from_collection` auf eine
  nicht (mehr) existierende Sammlung/Datei: `CmdResult`-Fehler, im
  Frontend wie bei bestehenden Mutationen nur geloggt
  (`console.error`), UI bleibt benutzbar (Ledger-Refresh holt den
  echten Zustand nach).
- `import_folder_as_collection`: bricht der Ordner-Dialog ab (kein Ordner
  gewählt), passiert nichts (wie beim bestehenden `import_folder`) — keine
  leere Sammlung wird angelegt.

## Testing

- Rust: Repository-Funktionen in `collections.rs` gegen eine echte
  In-Memory-SQLite-Verbindung getestet (bestehendes Testmuster in
  `repository.rs` verwendet ebenfalls `rusqlite::Connection::open_in_memory()`
  bzw. das Projekt-eigene Test-Schema-Setup) — insbesondere: Reihenfolge
  nach `add_files_to_collection` (Anhängen ans Ende), `reorder_collection`
  setzt Positionen korrekt, `delete_collection` löscht per Cascade nur
  Zuordnungen, nicht `files`-Zeilen.
- Frontend: kein Testframework im Projekt vorhanden (etabliertes Muster:
  `npx tsc --noEmit` + `npm run build` + manueller Smoke-Test im
  gebauten AppImage) — insbesondere manuell prüfen: Maus-basiertes
  Umsortieren in der Sammlungs-Detailansicht funktioniert zuverlässig
  (kein natives Browser-Drag-Icon erscheint, kein Hängenbleiben).

## Out of Scope

- Keine Beschreibungs-/Notizfelder pro Sammlung (bewusst nicht
  gewünscht laut Brainstorming — Sortierbarkeit ist der einzige
  Mehrwert gegenüber Tags).
- Keine verschachtelten Sammlungen (Sammlung in Sammlung).
- Kein Cover-/Vorschaubild pro Sammlungs-Karte in der Galerie (nur
  Name + Anzahl Modelle) — kann als spätere Erweiterung ergänzt werden.
- `import_folder_as_collection` importiert exakt wie das bestehende
  `import_folder` (inkl. dessen bisherigem Rekursionsverhalten für
  Unterordner, unverändert) — keine neue Scoping-Option „nur direkte
  Dateien, keine Unterordner".
