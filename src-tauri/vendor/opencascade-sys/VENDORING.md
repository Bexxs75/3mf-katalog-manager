# Vendored: opencascade-sys 0.3.0

Dieses Verzeichnis ist eine **1:1-Kopie von `opencascade-sys` 0.3.0** von
crates.io mit genau zwei Änderungen. Eingebunden wird sie über
`[patch.crates-io]` in `src-tauri/Cargo.toml`.

## Warum überhaupt ein Fork?

Die Kiste (OCCT 7.9.3, Arch Linux) ist neuer als das Crate. Upstream 0.3.0 ist
die letzte Veröffentlichung, und auch der git-Stand vom 2026-08-24 enthält
keinen Fix. Ohne Patch übersetzt `opencascade-sys` gegen OCCT 7.9 nicht.

## Änderung 1 — `src/topo_ds.rs`

**Symptom:** C++-Übersetzungsfehler
`'TopoDS' in namespace '::' does not name a type` in der von cxx erzeugten
`topo_ds.rs.cc`.

**Ursache:** OCCT 7.9 hat `TopoDS` von einer Klasse zu einem **Namensraum**
gemacht (`/usr/include/opencascade/TopoDS.hxx:64`). cxx übersetzt die
Crate-Deklaration `type TopoDS;` zusammen mit `#[Self = "TopoDS"]` in
`using TopoDS = ::TopoDS;` — ein Alias auf einen Namensraum ist in C++ ungültig.

**Fix:** Die sieben Cast-Funktionen werden nicht mehr über `#[Self = "TopoDS"]`,
sondern über `#[namespace = "TopoDS"]` + `#[cxx_name = "..."]` als
namensraumqualifizierte freie Funktionen gebunden (Rust-Namen `topods_face` usw.).
Damit die öffentliche Schnittstelle des Crates unverändert bleibt, definiert der
Fork zusätzlich einen Rust-seitigen `pub struct TopoDS` mit denselben sieben
assoziierten Funktionen, die an `inner::topods_*` weiterreichen. Aufrufstellen
wie `TopoDS::Face(&shape)` funktionieren also weiterhin unverändert — auch der
Originaltest des Crates läuft damit durch.

## Änderung 2 — `OCCT/CMakeLists.txt`

**Symptom:** `CMake Error at CMakeLists.txt:1: Compatibility with CMake < 3.5
has been removed from CMake.` — und danach, weil `build.rs` jeden
Configure-Fehler als "nicht gefunden" deutet, die **irreführende** Meldung
`Pre-installed OpenCASCADE library not found.`

**Ursache:** Zeile 1 verlangte `cmake_minimum_required (VERSION 3.1)`. CMake 4.x
konfiguriert das nicht mehr.

**Fix:** Anhebung auf `VERSION 3.10`. Die Datei ist ein reiner
`find_package (OpenCASCADE REQUIRED)`-Wrapper ohne eigene CMake-Logik; die
Anhebung ist deshalb gefahrlos. Ohne diesen Fix müsste jeder Entwickler und jede
CI `CMAKE_POLICY_VERSION_MINIMUM=3.5` exportieren.

## Was der Fork **nicht** ändert

- Kein `builtin`-Feature, kein `occt-sys`. OCCT wird **dynamisch** gelinkt
  (`BUILD_SHARED_LIBS=ON` aus der System-Installation). `occt-sys` darf nie in
  `cargo tree` auftauchen — statisches Linken würde die LGPL-Pflicht auslösen,
  relinkbare Objektdateien an jeden Empfänger zu liefern.
- Keine inhaltliche Änderung an Bindings, Headern oder `build.rs`.

## Pflege

Der Fork ist an **OCCT 7.8/7.9** gebunden. OCCT 7.10 wird weitere Arbeit
erfordern: Die `Handle_X`-Typedefs, die dieses Crate durchgehend benutzt, sind
seit 7.9 als veraltet markiert ("will be removed right after 7.9 release").
Beim Wechsel auf 7.10 also zuerst prüfen, ob es dann ein neues Upstream-Release
gibt — dann kann dieser Fork entfallen.

Aktualisieren lässt er sich mit:

```bash
CARGO_CPY=~/.cargo/registry/src/index.crates.io-*/opencascade-sys-0.3.0
rm -rf src-tauri/vendor/opencascade-sys
cp -r $CARGO_CPY src-tauri/vendor/opencascade-sys
```

…gefolgt von den beiden Patches oben.
