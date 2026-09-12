# Modell-Detailseite (printables.com-artig) + Druckplatten-Erkennung — Design

## Ausgangslage

Ein Klick auf eine Modell-Karte im Grid/in der Liste zeigt Metadaten
aktuell nur im schmalen rechten Seitenpanel (`DetailPanel.tsx`, 585
Zeilen) — Maße, Volumen, Gewicht, Objektanzahl, Material, Dateigröße,
Importdatum, Tags, Favorit/Druckstatus/Warteschlange-Buttons, "In Slicer
öffnen", Bild-Upload, Quelle-URL. Der Nutzer möchte stattdessen (in
Anlehnung an die Modellseite von printables.com, z. B.
`printables.com/model/1824097-...`) eine großzügige, vollflächige
Ansicht mit großer Vorschau und klar gegliederten Metadaten — zusätzlich
zum bestehenden Panel, nicht als dessen Ersatz.

Zusätzlicher Wunsch: falls eine `.3mf`-Datei mehrere Druckplatten
enthält (Bambu Studio/OrcaSlicer-Workflow, erkennbar an den
vorhandenen Dateinamen im Katalog des Nutzers), soll die Anzahl in der
Detailseite sichtbar sein. Das ist **kein** Teil des offiziellen
3MF-Standards (der Kern-`<build>`-Block kennt nur Objekt-Platzierung in
einer einzigen Bauszene) — Plattenanzahl steckt nur in
herstellerspezifischen Zusatzdateien im Zip
(`Metadata/model_settings.config` bei Bambu Studio/OrcaSlicer). Der
Parser liest aktuell nur `3D/3dmodel.model` und ein Thumbnail; diese
Datei wird gar nicht angefasst.

Die App hat keinen Router (`App.tsx` schaltet Ansichten rein über
lokalen `useState`, z. B. `mainView: 'catalog' | 'filament'`) — die neue
Detailseite folgt diesem bestehenden Muster statt eine Routing-Bibliothek
einzuführen.

## Ziel

1. Ein Doppelklick auf eine Modell-Karte (Grid oder Liste) öffnet eine
   neue vollflächige Detailseite, die das Grid im Hauptbereich ersetzt
   (Sidebar bleibt sichtbar). Einfacher Klick verhält sich unverändert
   (wählt aus, zeigt weiter im schmalen Seitenpanel — für schnelles
   Durchstöbern).
2. Die Detailseite zeigt links (~60% Breite) die vorhandene interaktive
   3D-Live-Ansicht (`ModelViewer`) deutlich größer als im Panel, mit
   Umschaltmöglichkeit auf das eigene hochgeladene Bild, falls
   vorhanden. Rechts (~40%) alle Metadaten großzügig gelayoutet, inkl.
   neu: Druckplatten-Anzahl (nur wenn erkannt).
3. Best-Effort-Erkennung der Druckplattenanzahl für Bambu
   Studio/OrcaSlicer-`.3mf`-Dateien; bei anderen Slicern/STL-Dateien
   bleibt das Feld leer, kein Fehler.

## Architektur

### 1. Backend: Druckplatten-Erkennung

Neues Modul `src-tauri/src/threemf/plates.rs`:

```rust
/// Liest `Metadata/model_settings.config` (Bambu Studio/OrcaSlicer-
/// spezifisch) aus dem bereits geöffneten Zip-Archiv und zählt die
/// enthaltenen `<plate>`-Elemente. Existiert die Datei nicht (jeder
/// andere Slicer, reine STL-Importe) oder lässt sie sich nicht als XML
/// parsen, wird `None` zurückgegeben - kein Fehlerfall, siehe
/// Fehlerbehandlung unten.
pub fn count_plates<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Option<u32>;
```

Implementierung: `archive.by_name("Metadata/model_settings.config").ok()`
early-return `None` bei `Err`; Inhalt mit dem bereits im Projekt
genutzten `quick_xml`-Reader durchlaufen, `Start`-Events mit lokalem
Namen `plate` zählen (Namespace-agnostisch, gleiche Toleranz wie der
bestehende Model-Parser gegenüber Namespace-Präfixen).

Aufruf aus `src-tauri/src/threemf/mod.rs`s bestehender
`parse_3mf_file`/`parse_3mf_bytes`-Pipeline, direkt nach dem Öffnen des
Zip-Archivs (gleiche Stelle wie die bestehende Thumbnail-Extraktion).
Neues Feld auf `ThreeMfDocument`:

```rust
pub plate_count: Option<u32>,
```

### 2. Backend: Datenfluss zur DB/DTO

Neue nullable Spalte auf `files` (gleiches Migrations-Muster wie bei
allen bisherigen Erweiterungen — Teil von `CREATE TABLE IF NOT EXISTS`
für Neuinstallationen, zusätzlich fehlertolerantes `ALTER TABLE` in
`repository.rs`s `init()` für Bestands-DBs):

```sql
ALTER TABLE files ADD COLUMN plate_count INTEGER;
```

`import_one` (`commands.rs`) übernimmt `doc.plate_count` in `NewFile`
(nur für `.3mf`; bei `.stl` bleibt es `None`, wie bei `object_count`
heute schon gehandhabt). `ModelFileDto`/`to_dto` bekommen ein neues
Feld:

```rust
pub plate_count: Option<i64>,
```

Kein Backfill für Bestandsdaten nötig (anders als bei `creator`/
`content_hash` im Easy-Wins-Feature) — `plate_count` ist rein
informativ, kein Filter-/Dedupe-Kriterium, das für alte Einträge
sofort korrekt sein muss. Bleibt bei bereits importierten Dateien bis
zu einem erneuten Import einfach leer.

### 3. Frontend: gemeinsame Metadaten-Logik

`buildMetaRows()` (aktuell privat in `DetailPanel.tsx`) wird nach
`src/lib/modelMetadata.ts` (neue Datei) verschoben und um eine
Druckplatten-Zeile ergänzt (nur enthalten, wenn `model.plateCount !==
null`). Beide Ansichten (`DetailPanel.tsx` und die neue
`ModelDetailPage.tsx`) importieren dieselbe Funktion — keine doppelte
Metadaten-Logik.

`ModelFile`-Typ (`src/types/index.ts`) bekommt `plateCount: number |
null`.

### 4. Frontend: neue Detailseite

Neue Komponente `src/components/ModelDetailPage.tsx`. Props: das
ausgewählte `ModelFile`, plus dieselben Callback-Props, die
`DetailPanel.tsx` heute schon bekommt (Tags, Löschen, Druckstatus,
Favorit, Warteschlange, Bild-Upload, Quelle-URL, In-Slicer-öffnen) —
keine neue Callback-Logik in `App.tsx`, nur Weiterreichung an die neue
statt (bzw. zusätzlich zur) bestehenden Komponente.

Layout:

- Kopfzeile: Zurück-Pfeil, Modellname als `<h1>`, Favorit-/Druckstatus-/
  Warteschlange-Buttons.
- Hauptbereich, zweispaltig (`flex`, wie das Projekt es bereits an
  anderen Stellen handhabt): links `ModelViewer` (größer skaliert über
  eine neue optionale Größen-Prop oder einfach einen größeren
  Container — `ModelViewer` selbst füllt bereits seinen Container
  responsiv), rechts die Metadaten-Liste aus `buildMetaRows` plus
  Ersteller, Quelle-URL (bestehende Editier-Interaktion aus
  `DetailPanel.tsx` übernommen), Tags.
- Fußbereich: "In Slicer öffnen", Dateipfad, Löschen — wie im
  bestehenden Panel-Footer.

`App.tsx`: neuer State `const [detailModelId, setDetailModelId] =
useState<string | null>(null)`. Ist er gesetzt, rendert der
Hauptbereich `ModelDetailPage` statt Grid/Liste (Sidebar bleibt
unverändert sichtbar). `ModelGrid.tsx`/`ModelList.tsx` bekommen eine
neue Prop `onOpenDetail: (id: string) => void`, ausgelöst per
`onDoubleClick` auf der Karte/Zeile (zusätzlich zum bestehenden
`onClick`/`onSelect`, der unverändert bleibt). `Escape`-Taste
(`useEffect` mit `keydown`-Listener, bereits an anderer Stelle im
Projekt genutztes Muster, z. B. Kontextmenü) sowie der Zurück-Pfeil
setzen `detailModelId` zurück auf `null`.

## Fehlerbehandlung

- `count_plates` gibt bei jedem Fehler (Datei fehlt, ungültiges XML)
  `None` zurück, geloggt wird nichts extra — analog zum bestehenden
  Thumbnail-Fallback-Verhalten, kein neuer Fehlerpfad im Import.
- Fehlt `displayImage` und schlägt der 3D-Viewer fehl (z. B. defekte
  Geometrie), zeigt die Detailseite denselben Fehlerzustand wie
  `ModelViewer` es im Panel heute schon tut (bestehende
  Fehlerbehandlung wird wiederverwendet, nichts Neues).

## Testing

- Rust: Unit-Tests für `count_plates` in `plates.rs` (Zip mit
  vorhandener/fehlender/kaputter `model_settings.config`,
  Namespace-Präfix-Toleranz). Roundtrip-Test für `plate_count` in
  `import_one`/`to_dto` in `db/mod.rs`, analog zu bestehenden
  Feld-Roundtrip-Tests.
- Frontend: `tsc --noEmit` sauber. Live-Verifikation im laufenden
  `npm run tauri dev`: Doppelklick öffnet die Seite, Escape/Zurück-Pfeil
  schließt sie, Metadaten stimmen mit dem Panel überein, eine
  Bambu-Studio-`.3mf`-Datei mit mehreren Platten aus dem eigenen
  Katalog des Nutzers zeigt die korrekte Plattenanzahl.

## Out of Scope

- Keine Vor-/Zurück-Navigation zwischen Modellen innerhalb der
  Detailseite (wie printables.com es nicht kennt) — Zurück führt immer
  zum Grid, kein "nächstes Modell"-Pfeil in dieser Ausbaustufe.
- Keine Erkennung von Druckplatten für andere Slicer als Bambu
  Studio/OrcaSlicer (z. B. PrusaSlicer-Multi-Plate-Format) — kann bei
  Bedarf später als eigenes kleines Feature nachgezogen werden.
- Kein Rückwirkendes Backfill von `plate_count` für bereits importierte
  Dateien.
- Keine URL-Route/Deep-Link zur Detailseite (kein Router in diesem
  Projekt, siehe Architektur-Begründung oben).
- Keine Änderung an der Kompakt-/Komfort-Ansicht-Umschaltung im Grid
  selbst — die Detailseite ist ein zusätzlicher dritter Anzeigemodus,
  kein Ersatz für Grid oder Liste.
