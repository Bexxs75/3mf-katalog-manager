# Lizenzen von Drittkomponenten

Diese Anwendung steht unter der MIT-Lizenz (siehe [`LICENSE`](LICENSE)). Sie
bindet zusätzlich die folgenden Fremdkomponenten ein.

## Open CASCADE Technology (OCCT)

Verwendet für die 3D-Vorschau und die Metadaten von STEP-Dateien
(`.stp`/`.step`), angebunden über das Rust-Crate `opencascade-sys` 0.3.0.
Die eingebundene Crate-Fassung ist ein kleiner, im Quellcode mitgelieferter
Kompatibilitäts-Fork; Änderungen und Herkunft stehen in
`src-tauri/vendor/opencascade-sys/VENDORING.md`.

- **Lizenz:** GNU Lesser General Public License, Version 2.1, **mit der
  Open-CASCADE-Ausnahme** (`OCCT_LGPL_EXCEPTION.txt`).
- **Einbindung:** dynamisch als unveränderte Shared Libraries
  (`libTK*.so` bzw. auf anderen Plattformen `TK*.dll`). Die Bibliotheken
  bleiben als eigene Dateien von der Anwendung getrennt und können durch eine
  kompatible, auch geänderte Fassung ersetzt werden. Damit bleibt der in
  LGPL-2.1 §6 verlangte Austauschmechanismus erhalten.
- **Quellcode:** Der OCCT-Quellcode ist unter
  <https://github.com/Open-Cascade-SAS/OCCT> verfügbar. Für ein konkretes
  Programmpaket gilt die dort dokumentierte OCCT-Version; der Linux-Build ist
  mit OCCT 7.9.3 verifiziert.

Die vollständigen Texte werden im Programmpaket mitgeliefert:

- `LICENSES/LGPL-2.1.txt`
- `LICENSES/OCCT_LGPL_EXCEPTION.txt`

Die hier verwendeten OCCT-Kernbibliotheken für STEP-Lesen, Eigenschaften und
Tessellierung haben im verifizierten Arch-Linux-Build neben den üblichen
System-/C++-Laufzeitbibliotheken keine weiteren direkten Fremdabhängigkeiten.
Grafik-, Schrift- und optionale Importmodule (z. B. FreeType/OpenGL) werden von
diesem Funktionspfad nicht benutzt und nicht mitgeliefert.

## `opencascade-sys`

Copyright © Brian Schwind und Mitwirkende. Lizenz: LGPL-2.1. Das Crate besteht
aus Rust-/C++-Bindings; es wird im Quellcode unter
`src-tauri/vendor/opencascade-sys/` mitgeliefert. Sein `builtin`-Feature ist
absichtlich **nicht** aktiv: Es würde OCCT aus dem Quelltext bauen und statisch
linken. Dieser Projekt-Build verwendet ausschließlich die dynamische
System-/Paketfassung von OCCT.

## Fassungen ohne STEP-Vorschau

Wird die Anwendung ohne das Feature `step-preview` gebaut
(`cargo tauri build --no-default-features`), sind weder `opencascade-sys` noch
OCCT Teil des Abhängigkeitsgraphen oder Programmpakets. Eine zukünftige
kommerzielle Fassung kann diese Variante verwenden; die Anwendung verhält sich
für STEP-Dateien dann wie Version 0.11.0 (Katalogisierung ohne Vorschau und ohne
automatisch ausgelesene STEP-Metadaten).
