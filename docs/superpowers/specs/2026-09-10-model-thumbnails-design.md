# Modell-Thumbnails, eigenes Bild-Upload + Quelle-Link — Design

## Ausgangslage

Die Raster-Ansicht (`ModelGrid.tsx`) zeigt für jedes Modell bisher nur einen
generischen Platzhalter (gestricheltes Quadrat + "3D-Vorschau"-Text) - nie
ein echtes Bild, obwohl `files.thumbnail_png` (aus dem 3MF-Container
eingebettetes Thumbnail, beim Import extrahiert, falls vorhanden) bereits
in der Datenbank liegt. Der Grund: `ModelFileDto` stellt dieses Feld
bisher gar nicht ans Frontend aus. Die eigentliche 3D-Ansicht wird nur
live im `DetailPanel` gerendert (three.js, Geometrie via nativer Rust-
Extraktion, `get_model_geometry`-Command) - es gibt keinen
serverseitigen Renderer.

Angeregt durch den Wunsch, in der Raster-Ansicht echte Vorschaubilder zu
sehen (ähnlich der 3D-Vorschau im Detailbereich), plus zwei verwandte
Wünsche: eigenes Bild pro Modell hochladen können (z. B. Foto vom
fertigen Druck) und eine Quelle als Link pro Modell hinterlegen können.

## Ziel

Jede Modell-Karte im Grid zeigt, wenn möglich, ein echtes Bild statt des
Platzhalters. Drei Bildquellen sind möglich, mit fester Priorität:

1. **Eigenes hochgeladenes Bild** (höchste Priorität) - der Nutzer lädt
   im Detailbereich ein Bild hoch (z. B. Foto vom Druck, Bild von der
   Quelle der Datei).
2. **Eingebettetes 3MF-Thumbnail** - vom Ersteller der Datei mitgeliefert,
   höhere Qualität/Absicht als ein automatischer Snapshot.
3. **Automatisch erzeugter 3D-Snapshot** (niedrigste Priorität, Fallback) -
   sobald ein Modell einmal im Detailbereich betrachtet wurde (die 3D-
   Ansicht also einmal live gerendert wurde), wird automatisch ein
   Snapshot dieser Ansicht erzeugt und gespeichert, damit auch Modelle
   ohne eingebettetes Thumbnail (praktisch alle STL-Dateien, viele
   3MF-Dateien) irgendwann ein echtes Bild im Grid zeigen, ohne dass der
   Nutzer manuell etwas hochladen muss.

Fehlt aller drei, bleibt der bisherige Platzhalter bestehen.

Zusätzlich: pro Modell lässt sich eine Quelle als URL hinterlegen (z. B.
Link zur Fundstelle der Datei), im Detailbereich anzeigbar/bearbeitbar.

## Architektur

### 1. Datenmodell (`src-tauri/src/db/schema.sql`)

Drei neue Spalten auf `files`, alle nullable:

```sql
ALTER TABLE files ADD COLUMN render_snapshot_png BLOB;
ALTER TABLE files ADD COLUMN custom_image_png BLOB;
ALTER TABLE files ADD COLUMN source_url TEXT;
```

(Als Teil von `CREATE TABLE IF NOT EXISTS files` für Neuinstallationen,
zusätzlich per fehlertoleranter `ALTER TABLE` in `repository.rs`s
`init()` für die reale, bereits befüllte Produktions-DB - gleiches
Muster wie bei allen bisherigen Schema-Erweiterungen dieses Projekts.)

`thumbnail_png` (bereits vorhanden) bleibt unverändert - schreibgeschützt,
wird ausschließlich beim Import aus der 3MF-Datei extrahiert.

### 2. Backend (`src-tauri/src/db/repository.rs`, `commands.rs`)

Neue Repository-Funktionen, Standard-Update analog zu `set_print_status`:

```rust
pub fn set_render_snapshot_png(conn: &Connection, file_id: i64, png: &[u8]) -> Result<(), DbError>;
pub fn set_custom_image_png(conn: &Connection, file_id: i64, png: &[u8]) -> Result<(), DbError>;
pub fn set_source_url(conn: &Connection, file_id: i64, url: Option<&str>) -> Result<(), DbError>;
```

Neue Tauri-Commands, Base64-Encoding an der IPC-Grenze analog zum
bestehenden Filament-Bild-Upload-Muster (`pick_and_read_image`):

```rust
#[tauri::command]
pub async fn upload_custom_image(app: AppHandle, state: State<AppState>, file_id: String) -> CmdResult<Option<String>>;
// Öffnet nativen Dateidialog (wie pick_and_read_image), liest die gewählte
// Datei, schreibt sie direkt in custom_image_png, gibt das neue
// displayImage zurück (None, wenn der Dialog abgebrochen wurde).

#[tauri::command]
pub fn set_render_snapshot(state: State<AppState>, file_id: String, image_base64: String) -> CmdResult<()>;
// Vom Frontend automatisch aufgerufen (nicht nutzergetriggert), siehe
// Abschnitt 3. Kein Dateidialog - image_base64 kommt direkt vom
// three.js-Canvas-Snapshot.

#[tauri::command]
pub fn set_source_url(state: State<AppState>, file_id: String, url: Option<String>) -> CmdResult<()>;
```

`ModelFileDto`/`to_dto` lösen die Bild-Priorität serverseitig zu einem
einzigen Feld auf, damit das Frontend nicht selbst zwischen drei
BLOB-Quellen wählen muss:

```rust
pub display_image: Option<String>,  // data:image/png;base64,... oder None
pub source_url: Option<String>,
```

`to_dto` wählt `custom_image_png` → `thumbnail_png` →
`render_snapshot_png` → `None`, in dieser Reihenfolge, und kodiert das
gewählte Feld als `data:image/png;base64,...`-String (gleiches Präfix-
Muster wie beim Filament-Bild).

### 3. Frontend

**`ModelViewer.tsx`** (bestehende 3D-Live-Ansicht im Detailbereich):

- `WebGLRenderer` bekommt `preserveDrawingBuffer: true` ergänzt (nötig,
  damit `canvas.toDataURL()` zuverlässig den zuletzt gerenderten Frame
  liefert - ohne dieses Flag kann der Puffer vor dem Auslesen bereits
  gelöscht sein).
- Neue Prop `needsSnapshot: boolean` - `App.tsx`/`DetailPanel.tsx`
  übergeben `true`, wenn `model.displayImage === null` ist.
- Sobald der Ladezustand auf `'ready'` wechselt **und** `needsSnapshot`
  gesetzt ist, wird einmalig (nicht bei jedem Render-Frame)
  `canvas.toDataURL('image/png')` aufgerufen, der Base64-Teil
  extrahiert und über eine neue Prop `onSnapshotCaptured: (base64: string) => void`
  an den Aufrufer gemeldet.
- `App.tsx` reicht `onSnapshotCaptured` durch: ruft `set_render_snapshot`
  auf und aktualisiert danach das passende Modell im lokalen `models`-
  State (`displayImage` optimistisch setzen), damit das Grid sofort das
  neue Bild zeigt, ohne Neuladen der ganzen Liste.

**`ModelGrid.tsx`**: zeigt `m.displayImage` als `<img>` (füllt den
`aspect-square`-Vorschaubereich), wenn gesetzt - der bisherige
gestrichelte Platzhalter bleibt der Fallback bei `null`. Der
`previewLabel3d`-Text ("3D-Vorschau") wird nur beim Platzhalter gezeigt,
nicht über einem echten Bild.

**`DetailPanel.tsx`**:

- Neuer "Bild hochladen"-Button neben/unter der 3D-Live-Ansicht, ruft
  `upload_custom_image` auf (analog zum bestehenden
  `pick_and_read_image`-Aufruf in `FilamentView.tsx`), aktualisiert bei
  Erfolg lokal `displayImage`.
- Neue Metadaten-Zeile "Quelle" in `buildMetaRows`-Nachbarschaft (eigene
  Zeile, kein Teil der bestehenden Funktion, da sie interaktiv ist statt
  reiner Text): zeigt die URL als klickbaren Link (öffnet im
  System-Browser, `target="_blank" rel="noreferrer"` reicht für ein
  Tauri-Webview mit Standard-Link-Handling) mit einem Stift-Icon (✎)
  daneben (gleiches ✎-Glyph wie beim bestehenden Bearbeiten-Icon in
  `FilamentView.tsx`, dort öffnet es allerdings das ganze Formular -
  hier ist die Interaktion neu: Klick blendet nur ein einzelnes
  Eingabefeld an Ort und Stelle ein statt eines separaten Formulars).
  Bestätigen (Enter oder Blur) ruft `set_source_url` auf.
  Ohne gesetzte URL: Hinweistext statt Link, Stift-Icon bleibt klickbar
  zum erstmaligen Setzen.

## Fehlerbehandlung

- `upload_custom_image`/`set_render_snapshot`/`set_source_url` folgen
  dem bestehenden `CmdResult<T>`-Muster (`Result<T, String>`),
  Frontend-Fehler werden wie an anderen Stellen dieser App als kurzer
  Text angezeigt.
- Schlägt die clientseitige Snapshot-Erzeugung fehl (z. B.
  `toDataURL()` wirft, was bei einem verlorenen WebGL-Kontext passieren
  kann), wird der Fehler geloggt (`console.error`) und nichts
  gespeichert - kein Blockieren der 3D-Ansicht selbst, nächster Besuch
  versucht es erneut (da `displayImage` weiterhin `null` bleibt).
- Ungültige/leere Quelle-URL: keine clientseitige Validierung über das
  native `type="url"`-Verhalten hinaus - bewusst einfach gehalten,
  gleiche Linie wie die fehlende Zahlenfeld-Validierung im
  Filament-Feature.

## Testing

- Rust: Roundtrip-Tests in `db/mod.rs` für `set_render_snapshot_png`,
  `set_custom_image_png`, `set_source_url`, sowie ein Test für die
  Prioritäts-Auflösung in `to_dto` (kombiniert unterschiedliche
  Teilmengen der drei BLOB-Felder, prüft die gewählte Quelle).
- Frontend: `tsc --noEmit` sauber. Live-Verifikation im laufenden
  `npm run tauri dev` vor Abschluss: ein Modell ohne eingebettetes
  Thumbnail einmal ansehen, prüfen dass danach ein Snapshot im Grid
  erscheint; ein eigenes Bild hochladen, prüfen dass es den Snapshot
  überschreibt; eine Quelle-URL setzen und den Link-Klick prüfen.

## Out of Scope

- Kein "Zurücksetzen auf automatisches Bild" nach einem Upload (wie
  beim Filament-Bild-Upload: erneutes Hochladen überschreibt, es gibt
  keinen expliziten Lösch-/Zurücksetzen-Button für dieses erste
  Ausbaustufe).
- Keine Größenbeschränkung/Kompression für hochgeladene Bilder über das
  native Dateidialog-Verhalten hinaus - ein großes Foto wird 1:1 als
  BLOB gespeichert und bei jedem `list_files`-Aufruf als Base64
  mitgeschickt (gleiches, bereits akzeptiertes Muster wie beim
  Filament-Bild; bei sehr großen Katalogen mit vielen hochauflösenden
  Fotos ein späteres Performance-Thema, nicht Teil dieser Ausbaustufe).
- Keine erneute Snapshot-Erzeugung, wenn sich die 3D-Ansicht ändert
  (z. B. nach einem erneuten Import derselben Datei mit geänderter
  Geometrie) - der Snapshot wird nur einmalig erzeugt, solange kein
  höher priorisiertes Bild existiert.
- Keine Validierung, ob die Quelle-URL tatsächlich erreichbar ist.
- Kein serverseitiger 3D-Renderer - der automatische Snapshot ist
  bewusst clientseitig (nutzt die ohnehin vorhandene Live-Vorschau),
  kein neuer Rust-Rendering-Stack.
