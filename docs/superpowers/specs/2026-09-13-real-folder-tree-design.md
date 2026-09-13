# Echte Ordnerstruktur (Verzeichnisbaum, physisches Verschieben)

## Kontext

Recherche im Code ergab: „Ordner" ist aktuell totes Gewebe.
`files.folder_id` (Spalte existiert, `src-tauri/src/db/schema.sql:17`) wird
laut Kommentar in `db/repository.rs:88` **nie** beim Import gesetzt, es gibt
keine UI zum Anlegen eines Ordners. `Sidebar.tsx` zeigt zwar eine
"Ordner"-Liste (`folders` aus `list_folders`), die ist in der Praxis aber
immer leer/nur "Alle Modelle" (`FolderRecord` hat nur `id`/`name`, keine
Hierarchie).

Was es stattdessen gibt: `import_folder_as_collection` legt beim Import
eines Ordners eine **flache Sammlung** an (Datenbank-Referenz, kein echter
Ordner). `import_folder`/`import_dropped` laufen bereits über
`collect_supported_files()` (`commands.rs:908`), die rekursiv **alle**
Unterordner nach `.3mf`/`.stl`-Dateien durchsucht — die Rekursion existiert
also schon, nur ohne die Ordnerstruktur zu speichern.

Dieser Spec macht "Ordner" zu einem echten Feature: ein Baum, der die
Verzeichnisstruktur auf der Platte abbildet (inkl. Unterordner), im
Programm navigierbar, und per Drag&Drop verschiebbar — inklusive
physischem Verschieben der Datei/des Ordners auf der Platte. Design wurde
als klickbarer Prototyp erarbeitet und vom User freigegeben:
`docs/superpowers/mockups/2026-09-13-gui-redesign.html` (Version 3,
Abschnitt "Ordner"). Baut auf der neuen Sidebar-Struktur aus
`2026-09-13-gui-shell-redesign-design.md` auf — dieser Spec sollte danach
umgesetzt werden.

## Ziel

1. Import einer Ordnerstruktur erzeugt eine passende Ordner-Hierarchie in
   der DB (nicht mehr nur eine flache Dateiliste).
2. Sidebar zeigt einen auf-/zuklappbaren Ordner-Baum statt der bisherigen
   flachen (und faktisch leeren) Liste.
3. Datei auf einen Ordner ziehen verschiebt sie **physisch** (Datei auf der
   Platte + `files.path` + `files.folder_id` in der DB).
4. Ordner auf einen anderen Ordner ziehen verschiebt den ganzen Unterbaum
   physisch (Verzeichnis-Rename) + alle enthaltenen Dateipfade in der DB.
5. Ordner lassen sich aus der App heraus neu anlegen (legt ein echtes
   Verzeichnis an).

## Nicht-Ziele

- Kein Datei-Uploader/Explorer-Ersatz — nur Organisation von bereits
  katalogisierten `.3mf`/`.stl`-Dateien.
- Kein Live-Filesystem-Watcher: Änderungen, die außerhalb der App am
  Dateisystem vorgenommen werden (z. B. im Datei-Manager), werden erst
  beim nächsten "Katalog aufräumen"/Neu-Import sichtbar — analog zum
  bestehenden Verhalten bei gelöschten/verschobenen Dateien
  (`scan_catalog_issues`).
- Keine harten Limits auf Verschachtelungstiefe — DB-Struktur ist
  rekursiv, UI-seitig reicht Einrückung pro Ebene.

## Datenmodell (Migration, additiv per `ALTER TABLE`)

Folgt dem etablierten Projektmuster (kein Migrations-Framework,
`ALTER TABLE` in `db::init()`, Fehler bei bereits vorhandener Spalte
ignoriert — siehe `db/repository.rs:27-49`):

```sql
ALTER TABLE folders ADD COLUMN parent_id INTEGER REFERENCES folders(id) ON DELETE CASCADE;
ALTER TABLE folders ADD COLUMN path TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_folders_path ON folders (path);
```

`FolderRecord` (`db/models.rs:127`) erweitert um `parent_id: Option<i64>`,
`path: String`. Kein Backfill nötig: Da `folder_id` bisher nie gesetzt
wurde, existieren in Bestands-DBs keine relevanten `folders`-Zeilen, die
migriert werden müssten — jede Katalog-DB startet mit dieser Funktion
faktisch bei null offene Altlasten.

`files.folder_id` (existiert bereits, FK `ON DELETE SET NULL`) wird ab
jetzt tatsächlich gesetzt: `NULL` bedeutet "liegt direkt im Wurzelordner
des Katalogs" (kein Unterordner) — entspricht der "Alle Modelle"-Zeile im
Mockup, die zusätzlich weiterhin *alle* Dateien rekursiv zeigt (nicht nur
Wurzel-Dateien).

## Backend: Import erzeugt Ordner-Hierarchie

`import_folder`/`import_dropped`/`import_files` rufen aktuell
`collect_supported_files()` und dann `import_many()` → `import_one()`
(`commands.rs:921`) pro Datei auf, `folder_id: None` fest verdrahtet
(`commands.rs:998`, `commands.rs:2583`, `commands.rs:2856`, `commands.rs:2897`).

Neue Funktion `db::ensure_folder_path(conn, path: &Path) -> Result<Option<i64>, DbError>`:
- Nimmt den absoluten Verzeichnispfad einer Datei (`path.parent()`).
- Läuft die Pfadkomponenten von der Katalog-Importwurzel abwärts, legt für
  jede fehlende Ebene eine `folders`-Zeile an (`name` = Verzeichnisname,
  `parent_id` = vorherige Ebene, `path` = voller Pfad), findet
  existierende Ebenen per `path`-Lookup (idempotent — mehrfacher Import
  desselben Baums legt keine Duplikate an).
- Gibt die `id` der tiefsten (Blatt-)Ebene zurück, oder `None`, wenn die
  Datei direkt im gewählten Importwurzel-Ordner liegt.

`import_one()` bekommt einen neuen Parameter `folder_id: Option<i64>`,
reicht ihn in `NewFile.folder_id` durch (statt des bisherigen impliziten
`None`). Aufrufer (`import_many`) ermitteln `folder_id` vor dem Aufruf via
`ensure_folder_path`. Einzeldatei-Import (`import_files`, kein
Ordner-Kontext) übergibt weiterhin `None` — Einzeldateien landen im
virtuellen Wurzelbereich, nicht in einem geratenen Ordner.

## Backend: neue Tauri-Commands

```rust
#[tauri::command]
pub fn list_folders(state: State<AppState>) -> CmdResult<Vec<FolderDto>>
// FolderDto: { id: String, name: String, path: String, parentId: Option<String>, count: i64 }
// count = rekursiv (dieser Ordner + alle Unterordner), analog zur
// Mockup-Semantik. Liefert eine FLACHE Liste (Frontend baut den Baum via
// parentId), nicht verschachteltes JSON — einfacher zu cachen/updaten.

#[tauri::command]
pub fn create_folder(state: State<AppState>, parent_id: Option<String>, name: String) -> CmdResult<FolderDto>
// Legt das Verzeichnis physisch an (std::fs::create_dir, Fehler bei
// bereits existierendem Namen wird als CmdResult<Err> durchgereicht) UND
// den DB-Eintrag. parent_id: None => neuer Ordner direkt unter der
// Katalog-Importwurzel (siehe "Katalog-Basisverzeichnis" unten).

#[tauri::command]
pub fn rename_folder(state: State<AppState>, folder_id: String, name: String) -> CmdResult<()>
// std::fs::rename(altes_verzeichnis, gleiches_parent/neuer_name) +
// rekursives Update von path bei diesem Ordner UND allen Nachfahren +
// files.path alle betroffenen Dateien (Pfad-Präfix-Ersetzung, siehe unten).

#[tauri::command]
pub fn move_folder(state: State<AppState>, folder_id: String, new_parent_id: Option<String>) -> CmdResult<()>
// std::fs::rename(alter_pfad, neuer_pfad) verschiebt das gesamte
// Unterverzeichnis physisch in einem Schritt (Betriebssystem-Rename,
// nicht Kopieren). Ablehnen mit Fehlermeldung, wenn new_parent_id ein
// Nachfahre von folder_id ist (Zyklus-Schutz, rekursive Prüfung wie im
// HTML-Mockup). Danach: path-Update dieses Ordners + aller Nachfahren
// (rekursiv) + files.path aller betroffenen Dateien.

#[tauri::command]
pub fn move_file_to_folder(state: State<AppState>, file_id: String, folder_id: Option<String>) -> CmdResult<()>
// Physisches Verschieben ueber die BESTEHENDE Helper-Funktion move_file()
// (commands.rs:628, hat bereits CrossesDevices-Fallback fuer
// Kopieren+Loeschen bei Ziel auf anderem Dateisystem — identisches Muster
// wie delete_file()/Papierkorb). Danach files.path + files.folder_id
// aktualisieren. folder_id: None verschiebt die Datei zurueck in die
// Katalog-Importwurzel.
```

Pfad-Präfix-Update (für `rename_folder`/`move_folder`) als
`db::update_paths_under_folder(conn, folder_id, old_prefix, new_prefix)`:
ein `UPDATE files SET path = new_prefix || substr(path, length(old_prefix)+1) WHERE path LIKE old_prefix || '/%'`
plus dasselbe Muster für die `folders.path`-Spalte selbst (rekursiv über
alle Nachfahren-Ordner). Läuft in einer Transaktion mit dem
`std::fs::rename`-Aufruf: erst der physische Rename, dann bei Erfolg die
DB-Updates — schlägt der physische Rename fehl (z. B.
`PermissionDenied`), bleibt die DB unverändert und der Fehler geht als
`CmdResult<Err>` ans Frontend (Toast-Meldung, siehe unten), kein
Teil-Update.

## Katalog-Basisverzeichnis

Für "Neuer Ordner direkt unter der Wurzel" und "Datei zurück in die Wurzel
verschieben" braucht es einen konkreten Basispfad. Vorschlag: erster
importierter Ordner-Pfad wird als `catalog_base_dir` in
`app_settings`-Tabelle (bereits vorhanden für andere Einstellungen, siehe
`slicers`/`display_preference`-Persistenz) gemerkt, vom User im
Einstellungen-Panel wählbar/änderbar (neuer Abschnitt "Basisordner",
Datei-Dialog wie beim Slicer-Pfad). Ist noch kein Basisordner gesetzt
(z. B. bislang nur einzelne Dateien importiert), wird "Neuer Ordner" mit
einem nativen Verzeichnis-Auswahldialog statt eines automatischen Pfads
umgesetzt (`app.dialog().file().blocking_pick_folder()`, existierendes
Muster aus `import_folder`).

## Frontend: Sidebar-Ordner-Baum

Ersetzt die aktuelle flache `folders.map(...)`-Liste in `Sidebar.tsx`
(Zeilen 104-119). Neue Komponente `src/components/FolderTree.tsx`:

- Baut den Baum aus der flachen `FolderDto[]`-Liste (`parentId`-Zeiger)
  client-seitig.
- Zeilen mit Einrückung pro Tiefe (`paddingLeft: 6 + depth*16`), Caret zum
  Auf-/Zuklappen (State: `Set<string>` offener Ordner-IDs, lokal in
  `FolderTree`, kein Server-Roundtrip).
- Aktiver Ordner hervorgehoben wie bisherige `.f-item.active`.
- **Drag&Drop folgt dem bestehenden Projekt-Muster** (mausbasiert, NICHT
  natives HTML5-`draggable`): siehe `Sidebar.tsx:58-84`
  (Warteschlangen-Reorder) und `ModelGrid.tsx:33-73`
  (Karten-Reorder) — beide benutzen `onMouseDown`/`onMouseEnter`/globales
  `mouseup`-Listener statt `draggable`/`dragstart`, weil natives HTML5-DnD
  unter Tauri/WebKitGTK durch `dragDropEnabled` (für OS-Datei-Drop
  benötigt) auf Fensterebene abgefangen wird. Der HTML-Mockup nutzt
  native `draggable` (funktioniert im Browser-Artefakt), das MUSS beim
  echten Umbau auf das Maus-Event-Muster portiert werden — sonst
  funktioniert Drag&Drop in der gebauten App nicht.
- Model-Karten (`ModelGrid.tsx`) erhalten denselben
  Mousedown-Start-Mechanismus wie beim Karten-Reorder, meldet aber beim
  Loslassen über einem Ordner-Baum-Element (Ziel-Erkennung via
  `document.elementFromPoint` + `closest('[data-folder-id]')`, analog zum
  bestehenden `overIndex`-Pattern) `onMoveFileToFolder(fileId, folderId)`
  statt eines Reorders.
- Bei erfolgreichem Verschieben: Toast/Kurzmeldung mit altem und neuem
  Pfad (neue kleine Komponente oder Wiederverwendung eines bestehenden
  Hinweis-Musters — Implementierungsdetail für den Plan).

## Fehlerfälle (müssen im Plan mit expliziten Teststeps abgedeckt sein)

- Zielname existiert bereits im Zielverzeichnis (`fs::rename` liefert
  `AlreadyExists`/Betriebssystem-spezifisch) → Fehlermeldung, keine
  DB-Änderung.
- Datei/Ordner-Pfad nicht mehr erreichbar (analog zu `delete_file`s
  Behandlung von `NotFound`, `commands.rs:597-613`) → Fehlermeldung statt
  Absturz, DB bleibt unverändert.
- Ordner in eigenen Nachfahren verschieben → clientseitig UND
  serverseitig abgelehnt (Server ist die verbindliche Prüfung, Client nur
  UX-Vorabprüfung).
- Cross-Filesystem-Verschiebung (`CrossesDevices`) → nutzt für Dateien die
  bestehende `move_file()`-Fallback-Logik; für Ordner (`move_folder`)
  Kopieren+Rekursiv-Löschen als Fallback, da `std::fs::rename` für
  Verzeichnisse über Dateisystemgrenzen hinweg grundsätzlich fehlschlägt —
  neue Hilfsfunktion `copy_dir_recursive`, nur für diesen Fallback-Pfad.

## Testing

- Rust: Unit-Tests für `ensure_folder_path` (neue Ebenen, idempotent bei
  wiederholtem Import, Datei direkt in Importwurzel → `None`),
  `update_paths_under_folder` (Präfix-Ersetzung korrekt, betrifft nur
  passende Zeilen), `move_folder`-Zyklus-Ablehnung.
- Rust: Integrationstest mit `tempfile`-Verzeichnis — echter
  `move_file_to_folder`-Aufruf, prüft Datei liegt danach physisch am
  neuen Pfad UND DB-Zeile stimmt überein.
- Frontend: `npx tsc --noEmit` sauber; manuelle Prüfung Drag&Drop
  (Maus-Pattern) in der gebauten App (AppImage), nicht nur im Browser —
  das ist genau der Punkt, an dem das native-DnD-Mockup abweicht und wo
  ein rein browserbasierter Test das Problem nicht aufdecken würde.
