# Stage 1 advisory report / Beratender Abschlussbericht

## English

Work order SHA256: `0eb556894e37c78dc6bbe203489de6953fa031269abee55eff1dca5e182ba7ac`, verified against the supplied PLAN.md. Base: `b7c78322`, branch `codex/finish-0180`. All edits are in this checkout. No commit, push, tag, publication, network request, original-checkout edit or live-catalog access was performed. This report is advisory; independent review and release approval are outstanding.

### Scope and outcome

- **P9-3a/b/d:** SECURITY and manual operator guidance, local bilingual roadmap, owner checklist and this report. F4 proxy guidance already existed in SECURITY; the manual now covers it too. F6 is handled by documentation: change the generated first password and protect retained logs; no log deletion or behavior change. F8 hard-link limits documented. F3/F10 workflow changes remain outside this order. Phase D is not declared accepted; post-P2 delta audit and owner checks remain open. `4e06d73a` is an ancestor of HEAD (local ancestry check).
- **P9-1e:** literal-only dictionary reader generates CSV with DE/EN/ES/FR, plural forms, tag names and source references. CSV was generated only in `/tmp`, not added to delivery. Native-speaker review is pending.
- **P9-2a/b:** read-only inventory/dry-run for an owner-supplied standalone database copy; rejects WAL/journal sidecars and never opens the app database code. Three synthetic tests check counts, preservation, ambiguous aliases and missing/unsafe copies. All 19 explicitly named dictionary entries already exist; `autoTags.json` is unchanged. A four-language/alias-collision test was added. Actual catalog analysis/merge counts and further entries wait for the copy and owner review.
- **P3-10/P5-9:** existing bilingual manual/changelog already describe G-code/resin, print technology, slicer reports, open-with, STL export and STEP parts, as well as the other built packages. Added operator guidance and replaced obsolete drag-prototype changelog claims. No invented performance measurements; no promotion of native drag as an accepted manual feature.
- **P10 T6/P7-7:** existing offline frontend and release-tool tests exercised; added a 100-preview cache-budget Rust test (byte accounting, not process memory). Rust, mutation, live-key, GUI and VM checks not performed here are explicitly listed in ABNAHME-0180.
- **P5-8, last:** experimental opt-in drag handles in grid/list/detail, multi-selection, STEP alternative, translated accessible text, desktop/container checks, native-session guard in backend and import hook. Existing custom commands remain class C and need no new plugin capability. Per-app cache namespace separates preview/normal; each process owns a unique locked instance directory and each drag a unique session. Copies survive subsequent drags and process exit. Collection at startup/exit only considers own unlocked instance directories older than 24 h and ignores symlinks/unknown directories. Tests cover two sessions, parallel instances, preview separation, age/lock protection and private Unix permissions, but Rust execution is blocked.

### Proof

See the final results below and local logs under `.proof-0180/`. Rust compilation, tests and clippy have **not passed** in this environment; zero Rust tests ran. A filter with zero tests is not being reported as success.

### Deviations and blockers

1. Package commits omitted as explicitly instructed by the current request.
2. All-current-key CSV is a superset of additions since 0.17.0 and also covers edited translations; dynamic usage sites are labelled for manual inspection. The generated CSV is not shipped.
3. No new tag entries: all specified candidates already exist and no catalog copy was supplied. Proposed next step: owner provides a checkpointed copy, reviews the top-40/dry-run and approves candidates.
4. Process `.drawio` sources are absent from this checkout. Proposed next step: coordinator supplies the source diagrams for a separate documentation reconciliation; original/external sources were not edited.
5. Required `cargo` commands cannot start (`cargo: command not found`, exit 127). Run all three listed Rust commands in the approved local environment before accepting the code. Standard file locks require Rust ≥1.89; native lock behavior remains a platform acceptance item.
6. Exact `npx vitest run` fails with `EROFS` while bundling config through the read-only `node_modules` symlink. It did not change the linked original checkout. `--configLoader runner` is the documented additional proof command. Native loader was also tried and rejected this environment's `.ts` config (`ERR_UNKNOWN_FILE_EXTENSION`). No permission escalation or dependency installation was attempted.
7. No three-system prototype acceptance is claimed. The native end callback retains the import guard for a 250 ms delivery grace period; real platform event ordering and slicer acceptance must be verified in the mandatory probe. Before release without a passing probe, the owner must choose removal or hiding. Platform probes cannot be replaced with headless tests.

## Deutsch

Plan-Hash und Basis siehe oben. Ausschließlich dieser Checkout wurde geändert; keine Commits, Pushes, Tags, Veröffentlichung, Netzabfragen oder Live-Katalogzugriffe. Beratender Bericht zur unabhängigen Gegenprüfung, keine Release-Freigabe.

P9-Dokumente behandeln Proxy-Sperre, Erstpasswort im Log und Hardlink-Grenzen; Workflows, externe Eigentümerprüfungen und Delta-Audit bleiben offen. Übersetzungsprüfblatt-Skript und nur lesende Tag-Auswertung sind vorhanden. Alle 19 ausdrücklich genannten Tag-Einträge existieren bereits; ohne Katalogkopie bleiben Wörterbuch und reale Messzahlen unverändert. Das bestehende Handbuch deckt G-Code/Resin, Drucktechnik, Slicer-Meldung, Öffnen mit, STL und STEP-Teile bereits ab. Veraltete Drag-Aussagen wurden im Changelog ersetzt; im Handbuch wird Drag-out nicht als fertige Funktion beworben.

Drag-out wurde zuletzt experimentell ergänzt: Anfasser an Karte/Liste/Detail, Mehrfachauswahl, STEP-Alternative, vier Sprachen, Sperre gegen Eigenimport. Je App/Instanz/Sitzung getrennte Kopien, Prozess-Dateisperren, nur sichere verwaiste eigene Ordner älter als 24 h bei Start/Ende bereinigen. Frontend-Verhalten ist getestet; Rust-Code und Lebensdauer-Gegenproben sind mangels Cargo **nicht ausgeführt/übersetzt**. Der Cache-Test mit 100 Bildern misst nur die interne Bytebuchhaltung, nicht den Prozessspeicher.

Abweichungen/Blockaden: keine Paket-Commits gemäß aktueller Anweisung; CSV enthält alle aktuellen Schlüssel als Obermenge und bleibt privat; neue Tags warten auf Kopie/Freigabe; fehlende `.drawio`-Quellen sollen vom Koordinator für einen Folgeabgleich geliefert werden. Cargo fehlt (Exit 127, **0 Rust-Tests**). Der exakte Vitest-Befehl scheitert am schreibgeschützten Link (`EROFS`); zusätzliche Prüfung mit `--configLoader runner`, ohne Änderungen am anderen Checkout. Der native Loader scheitert an `.ts`; keine Rechteausweitung/Installation versucht. Dateisperren setzen Rust ≥1.89 voraus. Native Ereignisreihenfolge einschließlich 250-ms-Nachlauf der Importsperre, Drei-System-Probe, echte Schlüssel und VMs bleiben Pflichtabnahmen. Ohne bestandene Drag-Probe muss der Auftraggeber vor Release Entfernen oder Verbergen entscheiden.

Prüfergebnisse, Dateiliste und Logs folgen unten. Weitere Abnahmen: [ABNAHME-0180.md](ABNAHME-0180.md), [OWNER-CHECKS.md](OWNER-CHECKS.md), [Roadmap-Entwurf](ROADMAP-0180-DRAFT.md). Schema 63, Abhängigkeiten, Cargo-Features, `package.json` und `.github/` wurden nicht verändert.

## Proof results / Prüfergebnisse

| Command / Befehl | Result / Ergebnis |
|---|---|
| `cargo test --workspace --no-default-features` | Blocked / blockiert: `cargo: command not found`, exit 127; **0 tests executed** |
| `cargo clippy --workspace --no-default-features --all-targets -- -D warnings` | Blocked / blockiert: `cargo: command not found`, exit 127 |
| `cargo test -p katalog-core --no-default-features repairs_5000_files_under_one_second` | Blocked / blockiert: `cargo: command not found`, exit 127; **0 tests executed** |
| `npx tsc --noEmit` | PASS, exit 0, no diagnostics / keine Meldungen |
| `npx vitest run` | Blocked / blockiert: config bundling `EROFS`, exit 1; **0 tests executed** |
| `npx --no-install vitest run --configLoader runner` | PASS, exit 0: **361 files, 4,681 tests**, 153.95 s |
| `python3 -m unittest discover -s tools -p test_tag_review.py` | PASS: **3 tests** |
| `python3 -m unittest discover -s tools -p test_release_assets.py` | PASS: **37 tests** |
| Self-drop counterprobe / Eigenimport-Gegenprobe | Temporarily removed import guard: **1 failing test** detects `start_dropped_import`; source restored, **52/52 tests pass** |
| `git diff --check` | PASS |

Logs: `.proof-0180/cargo-test.log`, `clippy.log`, `flaky.log`, `tsc.log`, `vitest-agreed.log`, `vitest-final.log`, `tags.log`, `release-assets.log`, `self-drop-counterprobe.log`, `self-drop-restored.log` (all beneath `.proof-0180/`). No failed mutation remains in source. / Kein absichtlich eingebauter Gegenprobenfehler bleibt im Quelltext.

## Files changed / Geänderte Dateien

- `CHANGELOG.md`
- `SECURITY.md`
- `crates/katalog-core/src/thingiverse/cache.rs`
- `docs/benutzerhandbuch/BENUTZERHANDBUCH.md`
- `src-tauri/src/commands/file_drag.rs`
- `src-tauri/src/lib.rs`
- `src/components/DragOutSetting.tsx`
- `src/components/ModelDetailPage.tsx`
- `src/components/ModelGrid.tsx`
- `src/components/ModelList.tsx`
- `src/hooks/useFileImport.test.ts`
- `src/hooks/useFileImport.ts`
- `src/i18n/de.ts`
- `src/i18n/en.ts`
- `src/i18n/es.ts`
- `src/i18n/fr.ts`
- `src/i18n/types.ts`
- `src/lib/autoTags.test.ts`
- `docs/ABNAHME-0180.md`
- `docs/ABSCHLUSS-0180.md`
- `docs/OWNER-CHECKS.md`
- `docs/ROADMAP-0180-DRAFT.md`
- `scripts/translation-review.mjs`
- `scripts/translation-review.test.js`
- `src-tauri/src/drag_storage.rs`
- `src/components/NativeDragHandle.test.tsx`
- `src/components/NativeDragHandle.tsx`
- `src/lib/nativeDrag.test.ts`
- `src/lib/nativeDrag.ts`
- `tools/tag_review.py`
- `tools/test_tag_review.py`
