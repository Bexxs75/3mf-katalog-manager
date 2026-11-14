# Acceptance checklist / Abnahme — 0.18.0 stage 1

## English

This is not release approval. Automated checks cannot replace the native probes. No GUI, VM, real key, live catalog, server or network acceptance was performed in this order.

### Mandatory before release

- [ ] **P5-8 three-system prototype gate:** Windows, macOS, Linux X11 and Wayland, each with a real slicer and file manager. Drag one file, a selection, identical basenames, spaces/accents and large files. Originals must remain byte-identical even when the target moves/deletes its input. Check cancel and failure paths, repeat a drag, and drop into the catalog window: no import, no folder move. Verify keyboard “Open in slicer”, STEP → STL alternative, disabled switch and container/web exclusion. The function is experimental and off by default. **Without a passing probe, remove or hide before release; owner decides.**
- [ ] **Copy lifetime:** read the first drag source after a second drag and after drag end/app exit. Run normal/preview and parallel processes; neither may delete the other's files. Startup/exit must preserve recent copies and other locked live instances. An hourly background sweep and exit cleanup also remove completed drag sessions of the current instance older than 24 h; active sessions remain protected independently of the UI timeout. Never collect on the next drag. Unknown directories and symlinks must survive. Confirm file-lock behavior on Windows/macOS; the standard-library lock requires Rust 1.89 or later.
- [ ] **P7-7:** GNOME with/without AppIndicator, missing Linux library, Wayland focus, macOS Dock reopen (owner), tray show/hide/quit, second launch, logout, restored tray setting without an icon, container rejection. Measure idle CPU for five minutes with hidden window; no measurement is claimed here.
- [ ] Windows installer/upgrade/uninstall matrix (P7-5, outside this stage): MSI/NSIS, normal/preview separation, optional data deletion, updater signatures and file associations. Never use a live catalog.
- [ ] **P10 T6:** real user key, invalid/revoked key, offline, deleted Thing, rate limit/Cloudflare, key secrecy in DTO/errors/logs/backups, leaving the view/disable/remove clears cache. Measure process memory for 100 results/images; the cache byte-accounting unit test is not an RSS measurement. DE/EN owner read-through; ES/FR native-speaker review remains pending.
- [ ] Rust counterprobes on a disposable copy: remove backup secret scrubbing → corresponding backup byte-search tests must fail; bypass limiter → `the_sixty_first_request_in_five_minutes_is_refused` must fail; force `close_decision` to Hide → tray decision tests must fail. Restore each mutation before continuing. These mutations were **not run** here because Cargo is absent.
- [ ] Run post-P2 security delta audit and [OWNER-CHECKS.md](OWNER-CHECKS.md). No blanket phase-D sign-off.
- [ ] Owner-provided **standalone checkpointed catalog copy**, without WAL/journal sidecars: run tag inventory/dry-run, approve candidates and translations. Existing 19 entries already cover every explicitly named candidate; no new entries or catalog measurements were invented.

### Required proof commands (repository root)

Use a disposable `XDG_DATA_HOME`, not the real catalog. Run Rust in the approved local build environment as non-root, with its own target directory and no network. The current sandbox has no `cargo` executable.

```sh
cargo test --workspace --no-default-features
cargo clippy --workspace --no-default-features --all-targets -- -D warnings
cargo test -p katalog-core --no-default-features repairs_5000_files_under_one_second
npx tsc --noEmit
npx vitest run
python3 -m unittest discover -s tools -p 'test_tag_review.py'
python3 -m unittest discover -s tools -p 'test_release_assets.py'
```

The agreed Vitest command cannot write the config bundle through this checkout's read-only `node_modules` link. Additional proof uses `npx vitest run --configLoader runner` without modifying that linked checkout. Actual results and counts: [ABSCHLUSS-0180.md](ABSCHLUSS-0180.md).

## Deutsch

Keine Release-Freigabe. Automatisierte Prüfungen ersetzen keine nativen Proben. GUI, VM, echter Schlüssel, Live-Katalog, Server und Netz wurden nicht zur Abnahme genutzt.

- [ ] **P5-8 Pflichtprobe:** Windows, macOS, Linux X11 und Wayland, echter Slicer und Dateimanager. Einzel-/Mehrfachauswahl, gleiche Dateinamen, Leerzeichen/Umlaute, große Dateien, Abbruch und Fehler prüfen. Originale bleiben bytegleich, auch bei Verschieben/Löschen durch das Ziel. Wiederholtes Ziehen und Ablegen ins eigene Fenster: kein Import und kein Ordnerverschieben. Tastaturweg, STEP-Hinweis, Schalter aus sowie Container/Web prüfen. **Experimentell, standardmäßig aus; ohne bestandene Probe vor Release entfernen oder verbergen (Auftraggeberentscheidung).**
- [ ] **Lebensdauer:** Erste Drag-Quelle nach zweitem Drag, Drag-Ende und App-Ende lesen. Normal-/Preview- und parallele Instanzen dürfen einander nichts löschen. Start/Ende erhalten junge Kopien und andere laufende gesperrte Instanzen. Ein stündlicher Hintergrundlauf und die Bereinigung beim Beenden entfernen außerdem abgeschlossene Drag-Sitzungen der eigenen Instanz älter als 24 h; aktive Sitzungen bleiben unabhängig vom UI-Timeout geschützt. Nie beim nächsten Drag bereinigen. Fremde Ordner/Symlinks erhalten. Dateisperren auf Windows/macOS prüfen; Standardbibliothek benötigt Rust ab 1.89.
- [ ] **P7-7:** GNOME mit/ohne Erweiterung, fehlende Bibliothek, Wayland-Fokus, macOS-Dock (User), Tray öffnen/verstecken/beenden, Zweitstart, Abmelden, Backup mit Tray-Wunsch ohne Icon, Container-Sperre. CPU-Ruhelast fünf Minuten messen; hier keine Messung behauptet.
- [ ] Windows-Installer-/Update-/Deinstallationsmatrix P7-5 separat: MSI/NSIS, Preview-Trennung, Datenlöschoption, Signaturen und Zuordnungen; nur Wegwerfkatalog.
- [ ] **P10 T6:** Echter/falscher/widerrufener Schlüssel, offline, gelöschtes Thing, Drossel/Cloudflare, Geheimnisfreiheit von Antworten/Protokollen/Sicherungen, Cache-Leerung prüfen. Prozessspeicher bei 100 Treffern/Bildern messen; Cache-Bytezählertest ist keine RSS-Messung. DE/EN gegenlesen; ES/FR-Prüfung offen.
- [ ] Rust-Gegenproben auf Wegwerfkopie: Backup-Scrub entfernen, Drossel umgehen, Schließentscheidung immer Hide → passende Tests müssen rot werden; danach jeweils zurücksetzen. **Hier mangels Cargo nicht ausgeführt.**
- [ ] Delta-Sicherheitsprüfung nach P2 und Eigentümerliste abarbeiten; Phase D nicht pauschal abgenommen.
- [ ] **Eigenständige, konsistente Katalogkopie ohne WAL/Journal** liefern, Tag-Inventar/Trockenlauf prüfen, neue Begriffe und Übersetzungen freigeben. Die 19 ausdrücklich genannten Einträge existieren; keine zusätzlichen Begriffe/Messzahlen erfunden.

Pflichtbefehle siehe oben. Rust hier mangels `cargo` blockiert. Der vereinbarte Vitest-Aufruf scheitert am schreibgeschützten `node_modules`-Link; ergänzend läuft `--configLoader runner`, ohne den anderen Checkout zu ändern. Ergebnisse und Anzahlen stehen im [Abschlussbericht](ABSCHLUSS-0180.md).

## Nachbesserung 1 — Rust proof / Rust-Nachweis

The requester must rerun `cargo test --workspace --no-default-features` and
`cargo clippy --workspace --no-default-features --all-targets -- -D warnings`
against these uncommitted changes. Rust execution is unavailable in this work
order; the green Docker results for base `6b8c6b7a` do not validate this patch.
Rust test source exists for timeout-state recovery, stale callbacks, overlap
exclusion and preservation of orphaned sessions between six and 24 hours old.
Coverage is not verified until the requester runs these tests successfully.
Startup/exit collection of orphaned instances and the current-instance sweep
use the same 24-hour retention; neither runs on the next drag. The three-platform
native drag acceptance remains open.

Der Auftraggeber misst die beiden obigen Befehle erneut auf diesen uncommitteten
Änderungen. Rust ist in diesem Auftrag nicht ausführbar; die grünen Docker-Werte
für `6b8c6b7a` sind kein Nachweis für diese Nachbesserung. Rust-Testquelltext für
Timeout-Zustandsfreigabe, verspätete Callbacks, überlappende Starts und den Erhalt
verwaister Sitzungen zwischen sechs und 24 Stunden ist vorhanden. Die Abdeckung
ist erst nach erfolgreichem Lauf durch den Auftraggeber bestätigt. Verwaiste
Instanzen und abgeschlossene eigene Sitzungen haben dieselbe 24-Stunden-Grenze;
keine Bereinigung beim nächsten Drag. Die native Drei-System-Abnahme bleibt offen.

## Nachbesserung 2 — additional acceptance / zusätzliche Abnahme

- [ ] Hold a native drag for more than 60 seconds: the watchdog releases the guard
  while the user may still be holding the mouse button. Check a subsequent drop
  into the catalog/image zone for unwanted import or assignment. Self-drop
  suppression is time-limited; this case is **not accepted** by automated tests.
- [ ] Check click without movement, exactly four pixels, movement beyond four
  pixels, release, focus loss, and dragging outside the handle on each platform.
- [ ] Run the Rust tests, including the 12-hour orphan retention counterprobe and
  the `checked_sub` timeout test, plus workspace Clippy. Rust was not run here.

- [ ] Nativen Drag länger als 60 Sekunden halten: Der Wächter gibt die Sperre
  frei, obwohl die Maustaste noch gedrückt sein kann. Anschließende Ablage im
  Katalog/Bildbereich auf unerwünschten Import oder Bildzuweisung prüfen. Die
  Eigen-Drop-Sperre ist zeitlich begrenzt; dieser Fall ist **nicht abgenommen**.
- [ ] Klick ohne Bewegung, genau vier Pixel, Bewegung über vier Pixel, Loslassen,
  Fokusverlust und Ziehen außerhalb des Anfassers auf jeder Plattform prüfen.
- [ ] Rust-Tests einschließlich 12-Stunden-Aufbewahrungs-Gegenprobe und
  `checked_sub`-Timeout-Test sowie Workspace-Clippy ausführen. Hier nicht gemessen.


## Nachbesserung 3 — proof limits / Nachweisgrenzen

The new Rust sweep test exercises the function used by the hourly timer with
injected timestamps: copies younger than 24 hours survive, older completed
copies are removed, active copies survive and become eligible after release.
It has **not been run here**. There is no dedicated test of the hourly thread's
scheduling, the 250 ms grace thread or the 60 s backend watchdog thread. Existing
state-method tests do not establish those threads' runtime behavior. Owner Rust
workspace tests, Clippy and native platform acceptance remain required.

`lease.try_lock()` requires Rust **≥ 1.89**. `src-tauri/Cargo.toml` already declares
`rust-version = "1.89"`; CI Rust toolchain entries use `stable`. The Docker server
image in `docker-web/Dockerfile` uses the older digest-pinned `rust:1-bookworm`
base, but builds only `katalog-server` and its dependencies, not the `src-tauri`
desktop crate. Copying that workspace member into the image is not compiling it.
No toolchain, Dockerfile or workflow was changed in this correction.

Der neue Rust-Sweep-Test prüft die vom stündlichen Timer verwendete Funktion mit
vorgegebenen Zeitpunkten: Kopien unter 24 Stunden bleiben, ältere abgeschlossene
Kopien verschwinden, aktive Kopien bleiben bis zur Freigabe geschützt. Er wurde
**hier nicht ausgeführt**. Für die Zeitsteuerung des stündlichen Threads, den
250-ms-Grace-Thread und den 60-s-Backend-Wächter gibt es keinen eigenen Test.
Zustandsmethoden-Tests weisen deren tatsächliches Thread-Verhalten nicht nach.
Rust-Workspace-Tests, Clippy und native Plattformabnahme bleiben beim Auftraggeber.

`lease.try_lock()` benötigt Rust **≥ 1.89**. `src-tauri/Cargo.toml` deklariert
bereits `rust-version = "1.89"`; die CI-Rust-Toolchains verwenden `stable`. Das
Docker-Server-Image (`docker-web/Dockerfile`, älterer per Digest fixierter
`rust:1-bookworm`-Stand) baut nur `katalog-server` und dessen Abhängigkeiten,
nicht das Desktop-Crate `src-tauri`. Das Kopieren des Workspace-Mitglieds ist
kein Kompilieren. Keine Toolchain-, Dockerfile- oder Workflow-Änderungen.
