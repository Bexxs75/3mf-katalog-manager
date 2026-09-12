# Papierkorb (Soft-Delete mit 7-Tage-Aufbewahrung) — Design

## Ausgangslage

`delete_file`/`delete_files` (`src-tauri/src/commands.rs`) löschen aktuell
sowohl den Katalog-Eintrag als auch die **echte Datei auf der Festplatte**
sofort und unwiderruflich (`std::fs::remove_file`, danach `DELETE FROM
files`). Ein versehentlicher Klick auf "Löschen" (Einzelmodell, oder als
Batch über die Aufräum-Vorschläge-Bereinigung, `commands.rs`s
`delete_files`) hat damit keinen Rückweg. Angeregt während der
Detailseiten-Brainstorming-Session (siehe
[[2026-09-12-model-detail-page-design]]), da dort ein "Löschen"-Button im
Footer geplant ist.

## Ziel

Gelöschte Dateien landen in einem Papierkorb: die echte Datei wird in ein
App-eigenes Papierkorb-Verzeichnis verschoben (nicht entfernt), der
Katalog-Eintrag bleibt als "gelöscht" markiert erhalten. Der Nutzer kann
sie 7 Tage lang wiederherstellen. Danach (oder durch manuelles Leeren)
werden sie endgültig entfernt — Datei und Katalog-Eintrag.

## Architektur

### 1. Datenmodell (`src-tauri/src/db/schema.sql`)

Zwei neue nullable Spalten auf `files` (gleiches Migrations-Muster wie
alle bisherigen Erweiterungen: Teil von `CREATE TABLE IF NOT EXISTS` für
Neuinstallationen, zusätzlich fehlertolerantes `ALTER TABLE` in
`repository.rs`s `init()` für Bestands-DBs):

```sql
ALTER TABLE files ADD COLUMN deleted_at TEXT;
ALTER TABLE files ADD COLUMN trash_path TEXT;
```

`deleted_at IS NULL` = normaler Katalog-Eintrag. Alle bestehenden
Lese-Abfragen (`list_files` und jede andere Stelle, die die `files`-
Tabelle ohne Weiteres abfragt — per Grep zu verifizieren, u. a.
Duplikat-Prüfung, Ordner-/Tag-/Creator-Zählungen) bekommen zusätzlich
`AND deleted_at IS NULL`, damit gelöschte Einträge in der normalen
Ansicht unsichtbar sind.

### 2. Papierkorb-Verzeichnis

`app_data_dir/trash/` (liegt neben `catalog.db`, gleiche Stelle wie
heute schon die Datenbank — `app.path().app_data_dir()` ist in `lib.rs`
bereits aufgelöst, wird beim Start wie der `app_data_dir`-Ordner selbst
per `create_dir_all` sichergestellt).

Dateiname im Papierkorb: `{file_id}-{urspruenglicher_dateiname}` —
kollisionsfrei durch die eindeutige DB-ID als Präfix, ohne dass eine
eigene Namenslogik nötig wird.

### 3. Backend: geänderter/neuer Ablauf

**Löschen** (`delete_file`, `delete_files` in `commands.rs`): statt
`fs::remove_file` wird die Datei nach `trash_path` verschoben
(`std::fs::rename`, mit Fallback auf Kopieren+Löschen für den seltenen
Fall eines Verschiebens über Dateisystemgrenzen hinweg — `rename` schlägt
dann mit `ErrorKind::CrossesDevices` fehl). `deleted_at`/`trash_path`
werden gesetzt statt die DB-Zeile zu löschen. Ausnahme: existiert die
Datei am Originalpfad bereits nicht mehr (z. B. Bereinigung eines
verwaisten Pfads über die Aufräum-Vorschläge) — dann gibt es nichts zu
verschieben, der Katalog-Eintrag wird direkt hart gelöscht wie bisher,
kein Papierkorb-Durchlauf für etwas, das ohnehin schon weg ist.

**Neue Repository-Funktionen** (`db/repository.rs`), analog zu
bestehenden Status-Setter-Mustern:

```rust
pub fn soft_delete_file(conn: &Connection, id: i64, trash_path: &str, deleted_at: &str) -> Result<(), DbError>;
pub fn restore_file(conn: &Connection, id: i64) -> Result<(), DbError>;
pub fn list_trash(conn: &Connection) -> Result<Vec<FileRecord>, DbError>;
pub fn purge_expired_trash(conn: &Connection, older_than: &str) -> Result<Vec<FileRecord>, DbError>;
// gibt die betroffenen Zeilen zurueck, BEVOR sie hart geloescht werden -
// der Aufrufer braucht trash_path, um zuerst die echten Dateien zu
// entfernen, dann erst die DB-Zeilen (gleiche Reihenfolge wie beim
// bestehenden delete_file: Datei zuerst, DB-Zeile danach).
```

**Neue Tauri-Commands** (`commands.rs`):

```rust
#[tauri::command]
pub fn list_trash(state: State<AppState>) -> CmdResult<Vec<ModelFileDto>>;

#[tauri::command]
pub fn restore_file(state: State<AppState>, file_id: String) -> CmdResult<()>;
// Verschiebt die Datei von trash_path zurueck an path. Existiert am
// Zielpfad bereits eine andere Datei, wird automatisch ein Suffix
// " (wiederhergestellt)" vor der Dateiendung eingefuegt und `path` in
// der DB entsprechend aktualisiert - kein Fehlerfall, siehe
// Fehlerbehandlung unten.

#[tauri::command]
pub fn delete_file_permanently(state: State<AppState>, file_id: String) -> CmdResult<()>;
// Manuelles "Endgueltig loeschen" fuer einen einzelnen Papierkorb-Eintrag.

#[tauri::command]
pub fn empty_trash(state: State<AppState>) -> CmdResult<()>;
// "Papierkorb leeren" - entfernt alle aktuell im Papierkorb befindlichen
// Dateien sofort, unabhaengig vom 7-Tage-Limit.
```

**Automatisches Aufräumen**: kein Hintergrund-Timer (Desktop-App läuft
nicht dauerhaft) — beim App-Start wird, analog zum bestehenden
`content_hash`-Backfill in `lib.rs`, einmalig `purge_expired_trash` mit
`jetzt - 7 Tage` aufgerufen und alle betroffenen Dateien+Zeilen entfernt.
Fehler beim Entfernen einer einzelnen Datei (z. B. Papierkorb-Verzeichnis
manuell manipuliert) werden geloggt und übersprungen, brechen den
Startup nicht ab — gleiches Toleranz-Muster wie der bestehende Backfill.

### 4. Frontend

**Header** (`Header.tsx`): neues Papierkorb-Icon-Button neben dem
Einstellungen-Icon, mit kleinem Zahl-Badge bei Inhalt (gleiches
Badge-Muster wie die bestehende Warteschlange-Anzeige in der Sidebar).
Öffnet einen neuen `mainView`-Wert (`'catalog' | 'filament' | 'trash'` —
Erweiterung des bereits vorhandenen `mainView`-States in `App.tsx`, statt
eines separaten neuen State-Felds).

**Papierkorb-Ansicht**: keine neue Grid-/Listen-Komponente — `ModelGrid`/
`ModelList` werden mit den Daten aus `list_trash` wiederverwendet (neue
Prop `readOnly?: boolean`, unterdrückt Favoriten-Stern und
Doppelklick-Öffnen der neuen Detailseite aus
[[2026-09-12-model-detail-page-design]]). Kopfzeile der Ansicht: Titel
"Papierkorb", "Papierkorb leeren"-Button mit
Bestätigungsdialog (bestehendes `ConfirmDialog`-Muster wie beim
Einzel-Löschen).

**`DetailPanel.tsx`** im Papierkorb-Modus: gleiche Metadaten-Zeilen und
3D-Vorschau wie gewohnt, aber die Aktionsleiste (Favorit, Druckstatus,
Warteschlange, In-Slicer-öffnen, Bild-Upload) wird durch zwei Buttons
ersetzt: "Wiederherstellen" und "Endgültig löschen", plus ein
Hinweistext "Wird automatisch entfernt am {deleted_at + 7 Tage,
lokalisiert formatiert}".

## Fehlerbehandlung

- Verschieben über Dateisystemgrenzen (`rename` schlägt mit
  `CrossesDevices` fehl): Fallback auf `fs::copy` + `fs::remove_file` des
  Originals — funktional gleichwertig, nur langsamer bei sehr großen
  Dateien.
- Wiederherstellen bei Namenskonflikt am Zielpfad: automatischer Suffix
  " (wiederhergestellt)" vor der Dateiendung, `path` in der DB
  entsprechend aktualisiert — kein Nutzer-Eingriff nötig (siehe
  Klärung oben).
- Papierkorb-Datei am erwarteten `trash_path` nicht mehr auffindbar (z. B.
  manuell vom Nutzer im Dateisystem entfernt): Wiederherstellen/
  endgültiges Löschen behandelt das wie ein bereits erledigtes Löschen
  (DB-Zeile wird trotzdem bereinigt), kein blockierender Fehler.
- `purge_expired_trash` beim Start: einzelne fehlschlagende Datei wird
  geloggt und übersprungen, bricht den restlichen Batch/Startup nicht ab
  (gleiches Muster wie der bestehende `content_hash`-Backfill).

## Testing

- Rust: Roundtrip-Tests für `soft_delete_file`/`restore_file`/
  `list_trash`/`purge_expired_trash` in `db/mod.rs`. Test für den
  Namenskonflikt-Suffix beim Restore. Test, dass ein bereits verwaister
  Pfad (Datei existiert nicht mehr) beim Löschen direkt hart gelöscht
  wird statt einen Papierkorb-Eintrag zu erzeugen.
- Frontend: `tsc --noEmit` sauber. Live-Verifikation im laufenden
  `npm run tauri dev`: Löschen → Datei liegt im Papierkorb-Verzeichnis,
  nicht mehr am Originalpfad; Wiederherstellen bringt sie zurück;
  "Papierkorb leeren" entfernt sie endgültig; App-Neustart mit einem
  künstlich auf 8 Tage zurückdatierten `deleted_at`-Eintrag räumt ihn
  automatisch weg.

## Out of Scope

- Kein konfigurierbares Aufbewahrungs-Zeitfenster (fest 7 Tage, kein
  Einstellungen-Panel-Eintrag dafür in dieser Ausbaustufe).
- Keine Papierkorb-Größenbegrenzung (z. B. maximale Gesamtgröße) — nur
  zeitbasiertes Aufräumen.
- Cloud-Löschung entfällt ohnehin (Cloud-Anbindung wurde entfernt, siehe
  CHANGELOG) — der Papierkorb betrifft ausschließlich lokale Dateien.
- Keine Wiederherstellung, wenn der ursprüngliche Ordner selbst
  inzwischen gelöscht wurde (Datei landet dann im Papierkorb-Zielordner
  mit einer Fehlermeldung statt eines stillen Fallback-Orts) — bewusst
  einfach gehalten für diese erste Ausbaustufe.
