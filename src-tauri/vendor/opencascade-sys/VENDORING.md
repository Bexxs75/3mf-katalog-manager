# Vendored: opencascade-sys 0.3.0

Dieses Verzeichnis ist eine **1:1-Kopie von `opencascade-sys` 0.3.0** von
crates.io mit gezielten Patches für OCCT 7.9 (Linux/GCC) und MSVC (Windows).
Eingebunden wird sie über `[patch.crates-io]` in `src-tauri/Cargo.toml`.

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

## Änderung 3 — MSVC: `Handle_X` ist eine abgeleitete Klasse, kein Typalias (Windows)

**Symptom:** Mehrere unterschiedliche C++-Übersetzungsfehler nur unter MSVC,
u. a. `error C2440` beim Konstruieren eines `std::unique_ptr<Handle_X>` aus
einem `opencascade::handle<T>*`, `error C2440` bei Funktionszeiger-
Initialisierung für `handle_try_deref<T>`, sowie `error C2039`/`error C2440`
bei direkter Bindung an echte OCCT-Methoden/-Funktionen, die `Handle(T)&`
entgegennehmen (z. B. `BRepOffsetAPI_MakePipeShell::SetLaw`).

**Ursache:** `Standard_Handle.hxx` definiert `Handle_X`
(das rückwärtskompatible Alias für `opencascade::handle<X>`) je nach Compiler
unterschiedlich:

```cpp
#if (defined(_MSC_VER) && _MSC_VER >= 1800)
  // MSVC: Handle_X ist eine von opencascade::handle<X> ABGELEITETE Klasse
  // (fuer C++/CLI-Export-Kompatibilitaet)
#else
  // andere Compiler: Handle_X ist ein einfacher Typalias
  typedef Handle(X) Handle_X;
#endif
```

Auf GCC/Linux sind `Handle_X` und `opencascade::handle<X>` also identisch
(austauschbar), auf MSVC sind es zwei unterschiedliche Typen in einer
Basis-/Ableitungsbeziehung. Das bricht drei Muster, die implizite
Zeiger-/Referenzkonvertierung zwischen beiden voraussetzen:

1. **Zeiger-Konstruktion:** `new opencascade::handle<T>(...)` lässt sich nicht
   mehr in `std::unique_ptr<Handle_X>` konvertieren (Basis- vs. abgeleiteter
   Zeiger sind nicht implizit kompatibel).
   **Fix:** direkt über `new Handle_X(...)` konstruieren — der
   Zeiger-/Kopier-Konstruktor von `Handle_X` funktioniert auf beiden
   Compiler-Varianten identisch.
2. **`handle_try_deref<T>`-Template** (in `bindings_common.hxx`, parametrisiert
   auf `opencascade::handle<T>`): cxx braucht für die `Result<&T>`-Rückgabe
   exakte Funktionszeiger-Signaturgleichheit, die bei `Handle_X` als
   eigenständigem Typ nicht mehr gegeben ist.
   **Fix:** eigene, nicht-templatisierte Deref-Funktion pro betroffenem Typ
   (z. B. `top_tools_handle_try_deref`, `poly_triangulation_handle_try_deref`).
3. **Direkte Bindung an echte OCCT-Methoden/-Funktionen**, die `Handle(T)&`
   entgegennehmen: cxx bindet sowohl freie Funktionen als auch
   Instanzmethoden (`self:`-Parameter) über einen exakten
   Funktions-/Pointer-to-member-Signaturvergleich.
   **Fix:** dünne `inline`-Wrapper-Funktionen mit unserer eigenen, exakten
   `Handle_X`-Signatur, die intern die echte OCCT-Funktion aufrufen. Wichtig:
   Eine `self:`-Methode wird von cxx **immer** als Member-Funktions-Bindung
   behandelt, unabhängig von einem gesetzten `#[cxx_name]` — der Wrapper muss
   also als echte freie Funktion (ohne `self:`) gebunden werden.

Ein Nebenfund in `gc.hxx`: `GCE2d_MakeSegment` liefert ein `const`-
qualifiziertes Rückgabe-Handle, das die MSVC-Überladungsauflösung beim
direkten Durchreichen an den `Handle_X`-Konstruktor durcheinanderbrachte —
gelöst über eine explizite, nicht-konstante lokale Zwischenvariable.

**Betroffene Dateien:** `top_tools.{hxx,rs}`, `geom.hxx`, `gc.hxx`,
`geom2d.hxx`, `geom_api.hxx`, `b_rep.hxx`, `poly.{hxx,rs}`,
`b_rep_lib.{hxx,rs}`, `shape_analysis.{hxx,rs}`, `b_rep_offset_api.{hxx,rs}`.

Alle Fixes sind so geschrieben, dass sie auf **beiden** Compilern
funktionieren (verifiziert: 299 Linux-Tests weiterhin grün, vollständiger
Windows-Build inkl. MSI-Bundle mit gebündelten OCCT-DLLs erfolgreich getestet).

## Windows: STEP-fähiges Bundle lokal bauen

Windows hat kein OS-Paket für OCCT (anders als Linux-Distros oder Homebrew auf
macOS). Der Weg über `vcpkg` und ein manuelles DLL-Staging, lokal verifiziert
und als Workflow `.github/workflows/build-windows-step.yml` automatisiert:

1. OCCT über `vcpkg` mit dem committeten, gepinnten Manifest bauen
   (`src-tauri/occt-vcpkg-manifest/vcpkg.json`, Override auf 7.9.3):
   ```powershell
   cd src-tauri\occt-vcpkg-manifest
   vcpkg install --triplet x64-windows
   ```
   Ergebnis landet standardmäßig unter
   `src-tauri\occt-vcpkg-manifest\vcpkg_installed\x64-windows`.
2. DLLs in den Projektbaum stagen (nie committen, siehe `.gitignore`):
   ```powershell
   .\src-tauri\scripts\stage-occt-dlls.ps1 -VcpkgInstalledDir <pfad>\vcpkg_installed\x64-windows
   ```
3. Bauen mit `CMAKE_PREFIX_PATH` auf die vcpkg-Installation gesetzt **und**
   der zusätzlichen Config, die die DLLs als Bundle-Ressourcen einbindet:
   ```powershell
   $env:CMAKE_PREFIX_PATH = "<pfad>\vcpkg_installed\x64-windows"
   npm run tauri build -- --bundles msi --config src-tauri\tauri.windows-step.conf.json
   ```

**Wichtig:** `tauri.windows-step.conf.json` wird **nicht** automatisch von
Tauri eingemischt (anders als ein `tauri.windows.conf.json` hieße) — bewusst,
weil ein `resources`-Eintrag mit einem Glob, der keine Datei trifft (z. B. der
offizielle `--no-default-features`-Windows-CI-Build ohne OCCT), den gesamten
Build mit `glob pattern ... not found` hart abbrechen lässt (verifiziert).
Die Datei muss deshalb explizit per `--config` zugeschaltet werden, nur wenn
`occt-runtime/` tatsächlich befüllt ist.

## macOS: STEP-fähiges Bundle lokal bauen

Anders als Windows liefert Homebrew ein fertiges OCCT-Paket, kein
Selbstbau-Schritt nötig. Lokal verifiziert (Intel-Test-VM via `docker-osx`,
Sonoma 14.8.9) und als Workflow `.github/workflows/build-macos-step.yml`
automatisiert:

```bash
brew install opencascade
export CMAKE_PREFIX_PATH="$(brew --prefix opencascade)"
npm run tauri build -- --bundles dmg
```

**Homebrew liefert OCCT nur einzelarchitektur-rein** (kein universelles
arm64+x86_64-Fat-Binary) — anders als `build-macos.yml` (STEP-frei,
`--target universal-apple-darwin`) baut `build-macos-step.yml` deshalb nur für
die Host-Architektur des Runners (arm64 auf github-gehosteten
`macos-latest`-Runnern).

**Achtung, offizielles Homebrew-Installationsskript:** unterstützt inzwischen
nur noch Apple Silicon (`arm64`) — der Installer bricht auf x86_64-Macs mit
`"Homebrew on macOS is only supported on Apple Silicon processors!"` ab. Auf
Intel-Systemen (z. B. eine ältere Test-VM) hilft nur ein manueller Install am
Skript vorbei:

```bash
git clone --branch main https://github.com/Homebrew/brew ~/homebrew
eval "$(~/homebrew/bin/brew shellenv)"
brew update
```

Betrifft `build-macos-step.yml` selbst nicht (github-gehostete `macos-latest`-
Runner sind arm64 und haben Homebrew vorinstalliert), nur eine eigene
Intel-Test-VM/-Mac.

**Noch offen:** die OCCT-`.dylib`s werden aktuell **nicht** ins App-Bundle
kopiert (nur dynamisch gegen die lokale Homebrew-Installation gelinkt) — eine
so gebaute DMG läuft nur auf Systemen mit installiertem
`brew install opencascade`. Eine macOS-Entsprechung des Windows-DLL-Stagings
(`stage-occt-dlls.ps1` + `tauri.windows-step.conf.json`) steht noch aus.

## Was der Fork **nicht** ändert

- Kein `builtin`-Feature, kein `occt-sys`. OCCT wird **dynamisch** gelinkt
  (`BUILD_SHARED_LIBS=ON` aus der System-Installation). `occt-sys` darf nie in
  `cargo tree` auftauchen — statisches Linken würde die LGPL-Pflicht auslösen,
  relinkbare Objektdateien an jeden Empfänger zu liefern.
- Keine Änderung an der öffentlichen Rust-API des Crates, mit einer Ausnahme:
  `BRepOffsetAPI_MakePipeShell::SetLaw` ist jetzt eine freie Funktion
  (`BRepOffsetAPI_MakePipeShell_SetLaw`) statt einer Instanzmethode (siehe
  Änderung 3, Punkt 3). Im eigenen `step`-Modul dieses Projekts hat `SetLaw`
  keine Aufrufer, betrifft also aktuell nur zukünftige Crate-Nutzung.

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

…gefolgt von den drei Patches oben.
