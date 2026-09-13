# Filamentverbrauch aus gesliceten 3mf-Dateien

## Kontext

OrcaSlicer und Bambu Studio (Orca ist ein Fork von Bambu Studio, gleiches
Projekt-3mf-Format) schreiben nach dem Slicen eine `Metadata/slice_info.config`
in die Projekt-3mf-Datei: pro Platte ein Gesamtgewicht sowie ein oder mehrere
`<filament>`-Elemente mit Typ, Farbe, verbrauchten Gramm und Metern.

Der Katalog-Manager berechnet aktuell nur eine grobe Gewichtsschätzung aus
Volumen × angenommener Materialdichte (`estimated_weight_g` in
`to_dto`/`ModelFileDto`). Diese Schätzung soll durch den echten, vom Slicer
berechneten Wert ersetzt werden, sobald slice_info.config vorhanden ist.

Bewusst außerhalb des Scopes (zu aufwendig für ein kostenloses Hobby-Projekt):

- Live-Anbindung an den Drucker (MQTT/Cloud) für tatsächlich verbrauchtes
  Material nach Druckende.
- Headless-Slicing durch die App selbst.
- Automatischer Abgleich/Abzug vom Filament-Lager (Spulen-Bestand).

## Architektur

Neues Rust-Modul `threemf::slice_info`, nach dem Vorbild des bestehenden
`threemf::plates` (liest `Metadata/model_settings.config` für die
Plattenzahl): liest `Metadata/slice_info.config` aus dem bereits geöffneten
Zip-Archiv, tolerant gegenüber fehlender Datei oder ungültigem XML — liefert
`Option<SliceInfo>`, nie einen Fehler.

```rust
pub struct SliceInfo {
    pub total_weight_g: f64,
    pub plates: Vec<PlateFilamentUsage>,
}

pub struct PlateFilamentUsage {
    pub plate_index: u32,
    pub weight_g: f64,
    pub filaments: Vec<FilamentUsage>,
}

pub struct FilamentUsage {
    pub filament_type: String, // z.B. "PLA"
    pub color: Option<String>, // Hex-Farbe, z.B. "#FFFFFFFF"
    pub used_g: f64,
    pub used_m: f64,
}
```

`ThreeMfDocument` (in `threemf/mod.rs`) bekommt ein zusätzliches Feld
`slice_info: Option<SliceInfo>`, befüllt von `parse_3mf_reader` analog zu
`plate_count`.

## Speicherung (DB)

Neue nullable Spalte auf `files`, per idempotenter Migration wie beim
bestehenden `plate_count`-Präzedenzfall:

```rust
let _ = conn.execute("ALTER TABLE files ADD COLUMN slice_info_json TEXT", []);
```

Die verschachtelte Struktur (mehrere Platten, mehrere Filamente pro Platte)
wird als JSON-Text serialisiert (`serde_json`) statt in einer eigenen
normalisierten Tabelle (wie `file_materials`) — sie dient nur der Anzeige,
wird nicht gefiltert oder sortiert, JSON hält den Aufwand klein.

`import_one` befüllt die Spalte beim (Re-)Import genau wie `plate_count`
heute schon (`NewFile.slice_info_json: Option<String>`,
`FileRecord.slice_info_json: Option<String>`).

## API / Commands

**Erweiterung von `to_dto`:** `slice_info_json` wird deserialisiert und als
strukturiertes Feld ins `ModelFileDto` übernommen. Ist `slice_info`
vorhanden, ersetzt dessen `total_weight_g` den bisherigen
`estimated_weight_g`-Wert; zusätzlich liefert `to_dto` ein Feld
`weight_source: "slicer" | "estimated"`, damit das Frontend die Quelle
korrekt beschriften kann ("aus Slicer" vs. "≈ geschätzt"). Ohne
`slice_info` bleibt das bisherige Verhalten (`estimate_weight_g` aus
Volumen/Materialdichte, `weight_source: "estimated"`) unverändert.

**Neuer Command `rescan_file_metadata(file_id: String) -> ModelFileDto`:**
liest die Datei am gespeicherten `path` erneut ein (3mf oder stl, gleicher
Parser-Aufruf wie in `import_one`), aktualisiert Maße, Volumen, Objektzahl,
Materialien, Metadaten, Thumbnail, `plate_count` und `slice_info_json` in der
DB, gibt den aktualisierten Datensatz zurück. Existiert die Datei am Pfad
nicht mehr (oder schlägt das Parsen fehl), liefert der Command einen Fehler
statt eines stillen Fehlschlags — der bestehende Katalog-Eintrag bleibt dabei
unverändert in der DB.

## Frontend

`ModelFile` (types/index.ts) bekommt:

```ts
weightSource: 'slicer' | 'estimated';
sliceInfo: {
  totalWeightG: number;
  plates: {
    plateIndex: number;
    weightG: number;
    filaments: { type: string; color: string | null; usedG: number; usedM: number }[];
  }[];
} | null;
```

Auf der Modell-Detailseite:

- Neuer Abschnitt "Filamentverbrauch (aus Slicer)" mit Aufschlüsselung pro
  Platte/Filament (Typ, Farb-Chip, Gramm, Meter) — nur sichtbar, wenn
  `sliceInfo` vorhanden ist.
- Die bestehende Gewichtsanzeige nutzt `weightSource`, um zwischen "aus
  Slicer" und "≈ geschätzt" zu unterscheiden. Grid/Liste bleiben unverändert.
- Neuer Button "Metadaten neu einlesen", ruft `rescan_file_metadata` auf,
  aktualisiert bei Erfolg den lokalen State, zeigt bei Fehler (Datei
  fehlt/verschoben) einen Toast — kein Datenverlust im Katalog.

## Fehlerbehandlung

- `slice_info.config` fehlt oder ist ungültiges XML → `None`, kein
  Fehlerfall (gleiches Verhalten wie der bestehende Thumbnail- und
  `plate_count`-Fallback).
- Rescan einer nicht mehr existierenden Datei → Command-Fehler, Frontend
  zeigt Toast, DB-Eintrag bleibt unverändert.
- Rescan einer STL-Datei funktioniert (aktualisiert Maße/Volumen), liefert
  aber nie `slice_info` (STL kennt kein Slicer-Metadatenformat).

## Tests

- Parser-Unit-Tests nach dem Muster von `threemf/plates.rs`
  (In-Memory-Zip-Fixtures): ein Filament/eine Platte, Multicolor (mehrere
  Filamente pro Platte), mehrere Platten, fehlende Datei, ungültiges XML,
  Namespace-Präfix-Toleranz.
- Command-Test für `rescan_file_metadata`: Erfolgsfall (Werte werden
  aktualisiert) und Fehlerfall (Datei am Pfad existiert nicht mehr).
