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
