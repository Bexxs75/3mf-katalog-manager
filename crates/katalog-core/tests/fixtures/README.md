# STEP-Prüfdateien

Einmalig erzeugt mit Open CASCADE 7.9.3 und dessen Draw-Harness `DRAWEXE`
aus dem Arch-Paket `opencascade`. Die Dateien sind im textbasierten STEP-Format
ISO 10303-21 gespeichert. Tests lesen sie nur; sie werden nicht zur Laufzeit
erzeugt und hängen daher nicht von einer STEP-Schreib-API ab.

Reproduzierbare Batch-Kommandos:

```bash
# Ein 10-mm-Würfel
DRAWEXE -b -c 'pload ALL; box b 10 10 10; stepwrite 0 b wuerfel-10mm.step'

# Zwei 10-mm-Würfel, der zweite um 30 mm in X versetzt, als ein Compound
DRAWEXE -b -c 'pload ALL; box a 10 10 10; box c 10 10 10; ttranslate c 30 0 0; compound a c d; stepwrite 0 d zwei-koerper.step'
```

Hinweis zur Syntax: Das Ergebnis steht bei `compound` **hinten**
(`compound a c d`). `stepwrite` schreibt mit `stepwrite 0 shape datei`.
Die im ursprünglichen Plan verwendeten Formen `compound d a c` und
`writeStep` sind für DRAWEXE 7.9 falsch.

## Erwartete Eigenschaften

- `wuerfel-10mm.step`: Maße `[10, 10, 10]` mm, Volumen `1.0 cm³`,
  12 Dreiecke nach der Tessellierung, 1 Körper.
- `zwei-koerper.step`: 2 Körper; Gesamtausdehnung ungefähr
  `[40, 10, 10]` mm. Das prüft insbesondere die Lageinformation des zweiten
  Körpers — ohne korrekte Transformation lägen beide Würfel im Ursprung.

## Creality Print: multiple plates / mehrere Druckplatten

`creality-3plates.3mf` is a read-only sample authorized in the work order. It contains five meshes across three plates (1, 2, 2, 2, 3), with external Production Extension components. Root object IDs are 2 on plate 1, 4/6/8 on plate 2 and 10 on plate 3. Synthetic in-memory packages additionally cover inline geometry and missing assignments.

`creality-3plates.3mf` ist die im Arbeitsauftrag freigegebene, nur gelesene Beispieldatei. Sie enthält fünf Meshes auf drei Platten (1, 2, 2, 2, 3), mit externen Komponenten der Produktions-Erweiterung. Die obersten Objekt-IDs sind 2 auf Platte 1, 4/6/8 auf Platte 2 und 10 auf Platte 3. Synthetische Pakete im Speicher prüfen zusätzlich eingebettete Geometrie und fehlende Zuordnungen.

## Synthetic Bambu-style project / synthetisches Bambu-Projekt

`synthetic-bambu-project.3mf` is **synthetic**: built by `make_synthetic_bambu_project.py` (next to it, reproducible byte for byte, run `python3 make_synthetic_bambu_project.py synthetic-bambu-project.3mf`) from invented, neutral values. It imitates only the layout and key names of a Bambu Studio project and is **not** copied or derived from any real or downloaded file (no designer, title, description, URL or image fields, one 10 mm cube). Contents: `slice_info.config` with a header only (no `<plate>`, so "no print data"), `project_settings.config` as JSON (`printer_model` "Bambu Lab X1 Carbon", `curr_bed_type` "Supertack Plate", `layer_height` "0.2", `nozzle_diameter` ["0.4"], nine-value bed temperature lists with a trailing "0", a 17-value `nozzle_temperature` list against a 9-value `filament_type`), `model_settings.config` with four plates, and `plate_1.json`, `plate_2.json`, `plate_4.json` (plate 3 has none on purpose). Expected `first_layer_time`: plate 1 = 123.4567 (stored 123.5), plate 2 = 88.04 (88.0), plate 4 = 250.96 (251.0); `bed_type` `supertack_plate`, `textured_plate`, `supertack_plate`. The ZIP comment repeats that the file is synthetic.

`synthetic-bambu-project.3mf` ist **synthetisch**: erzeugt von `make_synthetic_bambu_project.py` (daneben, Byte für Byte reproduzierbar) aus erfundenen, neutralen Werten. Es ahmt nur Aufbau und Schlüsselnamen eines Bambu-Studio-Projekts nach und ist **keine** Kopie oder Ableitung einer echten oder heruntergeladenen Datei (keine Designer-, Titel-, Beschreibungs-, URL- oder Bildfelder, ein 10-mm-Würfel). Die Erwartungswerte stehen oben.
