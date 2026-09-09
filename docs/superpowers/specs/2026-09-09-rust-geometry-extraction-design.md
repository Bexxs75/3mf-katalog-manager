# Rust-seitige Geometrie-Extraktion für die 3D-Vorschau

## Ausgangslage

Die 3D-Vorschau (`ModelViewer.tsx`) lädt die rohen Datei-Bytes vom Backend
(`get_model_geometry`, seit der vorherigen Optimierung als
`tauri::ipc::Response`-Rohbytes statt Base64/JSON übertragen) und parst sie
im Frontend mit three.js' `STLLoader`/`ThreeMFLoader`. Für kleine Dateien ist
das unproblematisch. Bei großen Multi-Part-3MF-Dateien (Bambu-Studio-
Projektdateien mit separaten Objekt-Dateien pro Bauteil) blockiert dieses
Parsen den kompletten UI-Thread für mehrere Sekunden bis Minuten:

- Beispiel-Datei aus dem Live-Test: 204 MB komprimiert, **1,41 GB**
  unkomprimiertes XML, verteilt über 35 separate `3D/Objects/object_*.model`-
  Dateien innerhalb der ZIP.
- `ThreeMFLoader` entpackt (via `fflate`, unproblematisch) und baut
  anschließend für jede Teildatei einen vollständigen Browser-DOM-Baum via
  `DOMParser` auf - für diese Datenmenge inhärent langsam und synchron.
- Ein Web-Worker-Zwischenschritt wurde bereits gebaut, live getestet und
  wieder verworfen: WebKitGTK (Tauri-Webview unter Linux) stellt in diesem
  Setup kein `DOMParser` im Worker-Kontext bereit. Jede 3MF-Datei fällt daher
  in den synchronen Haupt-Thread-Fallback zurück - kein Gewinn für den
  eigentlichen Problemfall.

## Ziel

ZIP-Entpacken und XML-/Mesh-Parsing komplett nativ in Rust erledigen
(bereits vorhandene Abhängigkeiten: `zip`, `quick-xml`) und dem Frontend nur
noch fertige, weltraum-transformierte Vertex-/Index-Rohdaten übergeben. Das
Frontend baut daraus direkt `THREE.BufferGeometry` - three.js' Datei-Loader
(`STLLoader`, `ThreeMFLoader`) und damit jede `DOMParser`-Abhängigkeit
entfallen vollständig.

**Scope:** sowohl 3MF als auch STL (einheitlicher Codepfad im Frontend, auch
wenn STL für sich genommen bereits schnell genug wäre). Als Nebeneffekt wird
ein bestehender Bug mitbehoben: Bei Multi-Part-3MF-Dateien zeigt das
Detail-Panel Größe/Volumen/Material aktuell als "–", weil der bestehende
Metadaten-Parser nur die Root-`3dmodel.model`-Datei liest und `p:path`-
Verweisen auf externe Objekt-Dateien nicht folgt. Die für die Geometrie-
Extraktion ohnehin nötige Multi-File-Auflösung behebt das automatisch mit.

**Out of scope:** Hochladen zu Cloud-Anbietern (weiterhin separat geplant),
jegliche Änderung an der OAuth-/Cloud-Anbindung.

## Architektur

### 1. Multi-File-Auflösung (`src-tauri/src/threemf/container.rs`)

3MF-Dateien im "Production Extension"-Format (u. a. von Bambu Studio /
OrcaSlicer erzeugt) lagern Objekt-Geometrie in separaten ZIP-Einträgen aus,
referenziert über ein `p:path`-Attribut an `<component>`- und `<item>`-
Elementen. Aktuell liest `container::read_package` ausschließlich die eine
Root-Modell-Datei (`3D/3dmodel.model` oder das per `_rels/.rels`
referenzierte Ziel).

Änderungen:

- `model_xml.rs`: `Component` und `BuildItem` bekommen ein zusätzliches
  `path: Option<String>`-Feld (aus dem `p:path`-Attribut, Namespace-Präfix
  wird wie bei den übrigen Attributen ignoriert).
- `container.rs`: iterative Auflösung bis zum Fixpunkt statt nur eines
  einzelnen Durchlaufs - nach dem Parsen der Root-Datei werden ihre
  referenzierten Pfade in eine Warteschlange gelegt; jede neu geparste Datei
  kann selbst wieder neue, noch unbekannte `p:path`-Referenzen enthalten
  (falls eine Slicer-Software mehrstufig referenziert), die ebenfalls
  eingereiht werden, bis keine neuen Pfade mehr auftauchen (Set bereits
  besuchter Pfade verhindert Endlosschleifen bei zirkulären Referenzen).
  Objekte aus allen Dateien landen in einer gemeinsamen
  `HashMap<(Option<String>, String), Object>` (Pfad + lokale Objekt-ID als
  Schlüssel, da Objekt-IDs nur innerhalb einer Datei eindeutig sind). Das
  Beispiel aus dem Live-Test referenziert nur eine Ebene (Root → 35
  Objekt-Dateien direkt), die Fixpunkt-Auflösung deckt aber auch tiefer
  verschachtelte Slicer-Ausgaben ab.
- Fehlt eine referenzierte Datei oder ist sie nicht parsbar, wird das
  betroffene Objekt übersprungen und eine Warnung geloggt (`eprintln!`,
  analog zum bestehenden Muster in `commands.rs`/`import_many`) - die
  übrigen Objekte werden trotzdem geladen.

### 2. Geometrie-Extraktion (`src-tauri/src/threemf/mod.rs`)

Neben der bestehenden `accumulate_object`-Funktion (läuft den Objektbaum ab
und akkumuliert eine Bounding-Box + Volumen) entsteht eine zweite,
strukturell identische Walk-Funktion, die statt dessen pro Blatt-Mesh die
weltraum-transformierten Dreiecke sammelt:

```rust
pub struct RenderMesh {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<[u32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
}

pub fn extract_render_meshes(model: &ResolvedModel) -> Vec<RenderMesh>
```

Die bereits vorhandene `Matrix3x4`-Transform-Komposition (verschachtelte
Components mit eigener Transformation) wird unverändert wiederverwendet -
das ist exakt dieselbe Logik wie bei der Bounding-Box-Berechnung, nur mit
anderem Akkumulator. Nur `object_type` `model`/`None` fließt ein (Support-
/Solidsupport-/Surface-Objekte werden wie bisher bei Bbox/Volumen
ausgeklammert). `normals` bleibt für 3MF `None` (three.js' `ThreeMFLoader`
setzte bisher ebenfalls keine Normalen - unverändertes Verhalten,
Indizes sind hier aber ein echter Effizienzgewinn: 3MF speichert Vertices
bereits eindeutig mit Dreiecken als Indexliste, im Gegensatz zur bisherigen
three.js-Aufbereitung, die das zu einer nicht-indizierten Geometrie
expandierte).

### 3. STL-Geometrie (`src-tauri/src/stl/mod.rs`)

`parser::parse(bytes)` liefert bereits `(vertices, triangles)` intern -
`parse_stl_bytes` verwirft sie aktuell nach der Bbox-/Volumen-Berechnung.
STL-Dateien enthalten pro Facette eigene, nicht geteilte Vertex-Einträge
(das Format selbst kennt kein Indexing) - `indices` ist dementsprechend nur
die triviale fortlaufende Zuordnung (`triangles_for`, bereits vorhanden).

Anders als bei 3MF ist die Normalen-Berechnung hier **neuer Code**: three.js'
`STLLoader`-Pfad rief bisher `geometry.computeVertexNormals()` im Frontend
auf (Mittelung der an einen Vertex angrenzenden Dreiecksnormalen). Damit
sich am Rendering-Ergebnis nichts ändert, bekommt `src-tauri/src/stl/mod.rs`
dieselbe Berechnung nachgebildet: pro Dreieck die Flächennormale bestimmen,
auf die drei beteiligten Vertices aufaddieren, am Ende normalisieren. Eine
neue Funktion `parse_stl_geometry(bytes) -> RenderMesh` liefert Positionen,
triviale Indizes und die berechneten Normalen (`Some(...)`).

### 4. Wire-Format & Tauri-Command (`src-tauri/src/commands.rs`)

`get_model_geometry` wird zu `async fn` und delegiert die eigentliche
Parse-Arbeit an `tauri::async_runtime::spawn_blocking`, damit das Parsen
einer 1,4-GB-Datei nicht den IPC-Dispatch-Thread blockiert (bewusste
Ausnahme gegenüber anderen, leichtgewichtigen Commands im Projekt, die
synchron bleiben).

Die Antwort bleibt ein einzelner `tauri::ipc::Response`-Rohbyte-Stream:

1. 4 Bytes: Länge des JSON-Headers, Little-Endian `u32`
2. JSON-Header (UTF-8, mit Leerzeichen auf ein Vielfaches von 4 Bytes
   aufgepolstert): Array von
   `{ vertexCount: number, hasNormal: boolean, indexCount: number,
   indexType: "u16" | "u32" }`, ein Eintrag pro Mesh, in der Reihenfolge,
   in der die Binärdaten folgen. Ein Indexbuffer ist immer vorhanden (bei
   STL die triviale fortlaufende Zuordnung, bei 3MF die echten, aus der
   Quelle übernommenen Dreiecks-Indizes) - das vereinfacht sowohl das
   Wire-Format (keine Sonderfall-Behandlung "kein Index") als auch den
   Frontend-Decoder.
3. Pro Mesh, in Header-Reihenfolge: `Float32`-Positionsdaten
   (`vertexCount * 3` Werte), optional `Float32`-Normalen (nur wenn
   `hasNormal`: STL berechnet sie nach, 3MF liefert wie bisher keine -
   siehe Abschnitt 2/3), dann `Uint16`- oder `Uint32`-Indexdaten je nach
   `indexType` (`indexCount` Werte, `indexCount = 3 * Dreieckszahl`)

Die Ausrichtung auf 4-Byte-Grenzen stellt sicher, dass das Frontend direkt
typed-array-"Views" auf den empfangenen `ArrayBuffer` legen kann, ohne die
Geometriedaten zu kopieren.

### 5. Frontend (`src/lib/parseModelGeometry.ts`, `src/components/ModelViewer.tsx`)

`parseModelGeometry.ts` wird ersetzt durch einen reinen Binär-Decoder ohne
three.js-Loader-Abhängigkeit:

```ts
export interface ParsedMesh {
  position: Float32Array;
  normal: Float32Array | null;
  index: Uint32Array | Uint16Array;
}

export function decodeModelGeometry(buffer: ArrayBuffer): ParsedMesh[]
```

`ModelViewer.tsx` entfällt die `extension`-Fallunterscheidung beim Parsen
komplett (Rust liefert für STL und 3MF dasselbe Format) - die `extension`-
Prop wird nicht mehr gebraucht und aus `Props`/`DetailPanel.tsx` entfernt.
Die bestehende `buildGroup`-Funktion (baut aus `ParsedMesh[]` eine
`THREE.Group`) bleibt strukturell erhalten, da pro Mesh keine Welt-Matrix
mehr separat mitgeschickt werden muss - Rust liefert die Positionen bereits
weltraum-transformiert, wodurch das `matrix`-Feld entfällt und `buildGroup`
sich vereinfacht (keine `mesh.matrix.fromArray(...)`/`matrixAutoUpdate`-
Handhabung mehr nötig; `geometry.setIndex(...)` wird jetzt immer statt nur
optional aufgerufen).

## Metadaten-Bugfix (Nebeneffekt)

`threemf::mod.rs`s bestehende `resolve_geometry`/`accumulate_object`
(Bounding-Box + Volumen für Größe/Volumen im Detail-Panel) läuft künftig
gegen dasselbe multi-file-aufgelöste Modell wie die neue Geometrie-
Extraktion, statt nur die Root-Datei zu sehen. Größe/Volumen und die
Material-Liste werden dadurch auch für Multi-Part-3MF-Dateien korrekt
befüllt, ohne dass dafür zusätzlicher Code nötig ist.

## Fehlerbehandlung

- Referenzierte externe Objekt-Datei fehlt/ist kaputt → Objekt überspringen,
  warnen, restliche Datei trotzdem laden (siehe oben).
- Datei liefert nach dem Parsen keine einzige Mesh → bestehender
  `previewUnavailable`-Zustand im Frontend (kein Verhaltensunterschied zu
  heute).
- Wire-Format-Encoding/-Decoding-Fehler (sollte durch Tests ausgeschlossen
  sein) → Command gibt `Err(String)` zurück, Frontend zeigt wie bisher
  `previewUnavailable`.

## Tests

- Rust: neue Unit-Tests für Multi-File-Auflösung (Test-ZIP mit mehreren
  `.model`-Dateien, verschachtelte Components über Dateigrenzen hinweg,
  fehlende Referenz wird übersprungen statt die ganze Datei abzulehnen).
- Rust: Geometrie-Extraktion gegen bekannte Test-Cubes (wie die bestehenden
  STL/3MF-Tests) - Vertex-/Dreieckszahlen und transformierte Koordinaten
  gegen Handrechnung geprüft.
- Rust: Wire-Format-Roundtrip (Bytes schreiben, wieder einlesen, mit
  Original vergleichen), inklusive Padding-Korrektheit.
- Frontend: `decodeModelGeometry` gegen von Hand konstruierte
  `ArrayBuffer`-Fixtures (kleine, bekannte Byte-Layouts).
- Manueller Live-Test mit der ursprünglichen 204-MB-Datei aus diesem
  Bug-Report als Abschlusskriterium.
