# Warteschlange, Gespeicherte Filter, Aufräum-Vorschläge — Design

## Ausgangslage

Aus der ursprünglichen Konkurrenz-App-Analyse (WhatsApp-Screenshots, Session
vom 2026-09-10) sind noch drei der als "mittel" eingestuften Punkte offen -
die vier "leicht" eingestuften Punkte (Ordner-Druckstatus, Neu-Badge,
Creators-Filter, Duplikat-Erkennung beim Import) sind bereits umgesetzt.
Dieses Spec bündelt die drei verbleibenden, unabhängigen Punkte in einem
gemeinsamen Plan - gleiches Muster wie beim bereits umgesetzten
"easy-wins"-Plan.

**Wichtige Abgrenzung zur bestehenden Duplikat-Erkennung:** Der beim Import
bereits vorhandene Mechanismus (`content_hash`-Vergleich in `import_many`)
verhindert, dass eine NEU importierte Datei ein bereits im Katalog
vorhandenes Duplikat wird. Die hier geplanten Aufräum-Vorschläge sind ein
eigenständiger, nachträglicher Scan des GESAMTEN Bestands - sie finden
Duplikate, die z. B. vor Einführung der Duplikat-Erkennung importiert
wurden, oder Einträge, deren Datei seither vom Dateisystem gelöscht oder
verschoben wurde.

## Ziel

1. **Warteschlange**: Nutzer markiert Modelle als "als Nächstes drucken",
   in einer vom Nutzer per Drag & Drop sortierbaren Reihenfolge.
2. **Gespeicherte Filter**: Nutzer speichert eine Kombination aus
   Ordner/Tag/Creator/Suche/Sortierung unter einem Namen und wendet sie
   später mit einem Klick wieder an.
3. **Aufräum-Vorschläge**: Nutzer startet manuell einen Scan des Bestands,
   der verwaiste Pfade (Datei existiert nicht mehr) und Duplikat-Gruppen
   (gleicher `content_hash`, mehrere Katalog-Einträge) findet und zur
   gezielten Bereinigung vorschlägt.

## Architektur

### 1. Datenmodell (`src-tauri/src/db/schema.sql`)

```sql
ALTER TABLE files ADD COLUMN queue_position INTEGER;

CREATE TABLE IF NOT EXISTS saved_filters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    folder_id INTEGER,
    tag TEXT,
    creator TEXT,
    query TEXT,
    sort TEXT NOT NULL,
    created_at TEXT NOT NULL
);
```

`queue_position` ist `NULL`, wenn ein Modell nicht in der Warteschlange
ist, sonst eine fortlaufende Ganzzahl beginnend bei 1 (kleinste Zahl =
nächstes zu drucken). Bei `saved_filters` sind `folder_id`, `tag`,
`creator` und `query` nullable (ein gespeicherter Filter muss nicht jede
Kategorie einschränken); `sort` ist immer gesetzt (Standardwert `'name'`,
falls beim Speichern keine andere Sortierung aktiv war).

Migration nach dem etablierten Muster: `CREATE TABLE IF NOT EXISTS` für
Neuinstallationen, zusätzlich fehlertolerante `ALTER TABLE` in
`repository.rs`s `init()` für die reale, bereits befüllte Produktions-DB.
Für Aufräum-Vorschläge wird keine neue Tabelle/Spalte gebraucht - der Scan
liest bei jedem Aufruf live den kompletten Bestand.

### 2. Backend (`src-tauri/src/db/repository.rs`, `commands.rs`)

**Warteschlange:**

```rust
pub fn set_queue_position(conn: &Connection, file_id: i64, position: Option<i64>) -> Result<(), DbError>;
pub fn max_queue_position(conn: &Connection) -> Result<i64, DbError>; // für "ans Ende anhängen"
```

Neue Reihenfolge nach Drag & Drop: das Frontend berechnet lokal die neue
lückenlose Reihenfolge (1..N) für alle Warteschlangen-Einträge und ruft
`set_queue_position` einmal für JEDES Modell auf, dessen Position sich
durch den Drop geändert hat (bei einer einzelnen Verschiebung sind das
alle Einträge zwischen alter und neuer Position, nicht nur das
gezogene Modell selbst) - kein eigener Batch-Command nötig, da die
Listen in der Praxis kurz sind (einstellige bis niedrige zweistellige
Anzahl Modelle).

`set_print_status` (bestehend, `repository.rs`) wird erweitert: wechselt
`status` auf `"printed"`, wird `queue_position` in derselben Anweisung
auf `NULL` gesetzt (`UPDATE files SET print_status = ?1, queue_position = CASE WHEN ?1 = 'printed' THEN NULL ELSE queue_position END WHERE id = ?2`).

`ModelFileDto`/`to_dto` bekommen `queue_position: Option<i64>`.

Commands: `set_queue_position(file_id: String, position: Option<i64>) -> CmdResult<()>`.

**Gespeicherte Filter:**

```rust
pub fn insert_saved_filter(conn: &Connection, filter: &NewSavedFilter) -> Result<i64, DbError>;
pub fn list_saved_filters(conn: &Connection) -> Result<Vec<SavedFilterRecord>, DbError>;
pub fn delete_saved_filter(conn: &Connection, id: i64) -> Result<(), DbError>;
```

Commands: `save_filter(name, folderId, tag, creator, query, sort) -> CmdResult<SavedFilterDto>`,
`list_saved_filters() -> CmdResult<Vec<SavedFilterDto>>`,
`delete_saved_filter(id: String) -> CmdResult<()>`.

**Aufräum-Vorschläge:**

```rust
#[derive(Serialize)]
pub struct CatalogIssuesDto {
    pub orphaned: Vec<ModelFileDto>,           // path existiert nicht mehr
    pub duplicate_groups: Vec<Vec<ModelFileDto>>, // je Gruppe: gleicher content_hash, sortiert nach imported_at aufsteigend
}

#[tauri::command]
pub fn scan_catalog_issues(state: State<AppState>) -> CmdResult<CatalogIssuesDto>;

#[tauri::command]
pub fn delete_files(state: State<AppState>, file_ids: Vec<String>) -> CmdResult<()>;
```

`scan_catalog_issues` lädt `db::list_files`, prüft je Datei
`std::fs::metadata(&file.path).is_err()` für "verwaist", und gruppiert
alle Dateien mit `content_hash.is_some()` nach Hash-Wert (nur Gruppen mit
≥2 Einträgen werden aufgenommen). Ein Katalog-Eintrag kann in beiden
Kategorien auftauchen (verwaist UND Teil einer Duplikat-Gruppe) - das ist
kein Sonderfall, beide Listen werden unabhängig gebildet.

`delete_files` ruft für jede ID die bestehende Lösch-Logik auf (Datei
vom Dateisystem entfernen, DB-Eintrag löschen, verwaiste Tags
aufräumen) - ein fehlgeschlagener Einzel-Löschversuch (z. B. Datei
bereits verwaist, `fs::remove_file` schlägt fehl) wird wie beim
bestehenden `delete_file`-Command behandelt: `NotFound`-Fehler beim
Datei-Löschen werden ignoriert (die Datei ist ja ohnehin weg), der
DB-Eintrag wird trotzdem entfernt.

### 3. Frontend

**Sidebar** (`src/components/Sidebar.tsx`): zwei neue, einklappbare
Sektionen nach dem bestehenden Tags/Creators-Muster (Überschrift mit
▴/▾-Pfeil).

- **Warteschlange**: Liste sortiert nach `queuePosition`, jede Zeile mit
  `draggable`-Attribut und `onDragStart`/`onDragOver`/`onDrop`-Handlern
  (natives HTML5-Drag&Drop, keine neue Abhängigkeit). Klick auf eine
  Zeile wählt das Modell aus (wie bei Tags kein zusätzlicher Klick-Filter
  nötig, da die Reihenfolge selbst schon die Information trägt).
- **Gespeicherte Filter**: Liste der gespeicherten Namen, Klick wendet
  alle enthaltenen States auf einmal an (`setActiveFolderId`,
  `setActiveTag`, `setActiveCreator`, `setQuery`, `setSort` in einem
  Rutsch). `+`-Button (gleiches Muster wie der bestehende
  Cloud-Konten-`+`) blendet ein Namensfeld ein, das beim Bestätigen
  `save_filter` mit den aktuell aktiven States aufruft. `✕` pro Zeile
  löscht.

**DetailPanel** (`src/components/DetailPanel.tsx`): neuer Button neben
dem bestehenden Druckstatus-Toggle, Text abhängig von
`model.queuePosition` ("Zur Warteschlange hinzufügen" /
"Aus Warteschlange entfernen").

**ContextMenu** (`src/components/ContextMenu.tsx`): neuer Menüpunkt mit
demselben Toggle, gleiches Muster wie der bestehende
"In Slicer öffnen"-Eintrag.

**Header/Settings** (`src/components/Header.tsx`): neuer Abschnitt
"Katalog prüfen" im bestehenden Einstellungen-Panel, mit einem Button,
der `scan_catalog_issues` aufruft und bei Ergebnissen den neuen
`CatalogCleanupDialog` öffnet.

**Neue Komponente `src/components/CatalogCleanupDialog.tsx`**: modales
Overlay (gleiches Grundmuster wie bestehende Dialoge), zeigt zwei
Abschnitte:

- "Verwaiste Einträge" - Liste mit Checkbox pro Eintrag, alle
  vorausgewählt.
- "Duplikate" - je Gruppe eine Unterliste, das älteste Exemplar
  (`importedAt` aufsteigend, erstes Element) ist nicht anwählbar
  (bleibt immer erhalten) und optisch als "wird behalten" markiert, alle
  anderen Exemplare der Gruppe sind zum Löschen vorausgewählt.

Ein "Ausgewählte löschen"-Button ruft `delete_files` mit den
angehakten IDs auf, aktualisiert danach die Modell-Liste/Ordner/Tags wie
beim bestehenden Löschen üblich.

## Fehlerbehandlung

- `set_queue_position`/`save_filter`/`delete_saved_filter` folgen dem
  bestehenden `CmdResult<T>`-Muster.
- `scan_catalog_issues` schlägt nicht fehl, wenn einzelne Dateien nicht
  lesbar sind - ein `fs::metadata`-Fehler IST das positive Scan-Ergebnis
  ("verwaist"), kein Command-Fehler.
- `delete_files` bricht nicht beim ersten Fehler ab - jede ID wird
  einzeln verarbeitet, Fehler pro Datei werden geloggt (gleiches Muster
  wie `import_many`s Einzel-Fehler-Toleranz), der Command selbst liefert
  am Ende trotzdem `Ok(())` zurück, solange keine ID einen harten
  Rust-Panic auslöst.

## Testing

- Rust: Roundtrip-Tests für `set_queue_position` (inkl. automatisches
  Zurücksetzen bei `set_print_status("printed", ...)`),
  `insert_saved_filter`/`list_saved_filters`/`delete_saved_filter`, und
  `scan_catalog_issues`-Gruppierungslogik (verwaist erkannt, Duplikate
  korrekt nach Hash gruppiert und nach `imported_at` sortiert, Gruppen
  mit nur einem Eintrag werden nicht aufgenommen).
- Frontend: `tsc --noEmit` sauber. Live-Verifikation im laufenden
  `npm run tauri dev` vor Abschluss: Modell zur Warteschlange
  hinzufügen, per Drag & Drop umsortieren, als gedruckt markieren und
  automatisches Entfernen prüfen; einen Filter speichern, Auswahl
  ändern, gespeicherten Filter erneut anwenden; zwei identische Dateien
  unter verschiedenen Pfaden importieren (vor Einführung der
  Duplikat-Erkennung wäre das möglich gewesen - zu Testzwecken kann
  direkt in der DB ein zweiter Eintrag mit gleichem `content_hash`
  angelegt werden), Scan starten, Duplikat-Gruppe im Dialog prüfen.

## Out of Scope

- Keine Mehrfachauswahl/Stapel-Aktionen außerhalb des Cleanup-Dialogs
  (z. B. kein "alle markierten Modelle gleichzeitig zur Warteschlange
  hinzufügen").
- Keine automatische Neu-Verknüpfung verwaister Pfade (z. B. Datei am
  neuen Ort wiederfinden) - nur Erkennung und Löschen des
  Katalog-Eintrags, die Originaldatei bleibt unangetastet.
- Kein automatischer Hintergrund-Scan beim Start (bewusst manuell
  ausgelöst, siehe Nutzerentscheidung).
- Gespeicherte Filter validieren nicht, ob referenzierte Tags/Creators/
  Ordner zwischenzeitlich gelöscht wurden - ein gespeicherter Filter mit
  einem nicht mehr existierenden Tag filtert beim Anwenden einfach auf
  eine leere Ergebnismenge (kein Fehler, kein Aufräum-Mechanismus für
  verwaiste Filter-Referenzen in dieser Ausbaustufe).
