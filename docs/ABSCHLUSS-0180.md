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

## Nachbesserung 1

Frozen order: `FIX1.md`, SHA256
`bf15b7e578a905df5a9f65a169af8f3d0abf1254c38538d56c2a7898bc8e3d79`.

- F1: Desktop `rust-version = "1.89"`; all discovered Rust toolchain workflow
  entries use `dtolnay/rust-toolchain@stable`. Workflows were only read.
- F2: Independent 60-second frontend/backend watchdogs release hung sessions.
  Session identity prevents late completion/rejection from releasing a newer
  guard. Listener setup that completes after expiry cannot start a native drag.
  Staging that exceeds the deadline cannot start a native session afterwards.
- F3: Acquire the guard before staging. Failed staging, GTK acquisition and native
  starts remove the session folder. Before each new drag, while holding the guard
  with no active session, collect only this instance's session directories older
  than six hours. Recent copies and other instances remain untouched.
- F4: Mouse-only grip has `tabIndex={-1}` and `aria-hidden`; detail grip is outside
  the heading.
- F5: Grid/list provide model metadata to filter selected IDs to live 3MF/STL/OBJ
  files. Selections above 64 supported files show an inline alert in DE/EN/ES/FR
  and do not invoke the backend.
- F6: Corrected indentation of the second grid insertion.

**Deutsch:** F1–F6 sind umgesetzt: Rust-Mindestversion nach CI-Leseprüfung,
60-Sekunden-Wächter auf beiden Seiten, Sperre vor dem Kopieren, Aufräumen bei
Startfehlern und vor neuen Drags (nur eigene Sitzungen älter als sechs Stunden),
nicht fokussierbarer Anfasser außerhalb der Überschrift, Auswahlfilter plus
64-Dateien-Meldung in vier Sprachen und Raster-Einrückung.

**Scope / Grenzen:** No commits, push, publication, dependencies or changes to an
original checkout. The requested per-finding commits were omitted because the
current user instruction expressly forbids commits. No product requirement was
redesigned; the timeout alternative was chosen over mouse/focus release. Rust
proof is delegated to the requester as specified, not claimed as passed. The
referenced private `docs/intern/release-baseline-v0.16.0.md` is absent from this
checkout; the supplied instructions and frozen orders were followed.

Keine Commits, kein Push, keine Veröffentlichung, keine neuen Abhängigkeiten und
keine Änderungen an einem Ursprungscheckout. Einzelcommits entfallen wegen des
aktuellen ausdrücklichen Commit-Verbots. Keine Umgestaltung der Anforderungen;
die erlaubte Timeout-Variante wurde gewählt. Rust-Nachweise misst der Auftraggeber.
Das referenzierte private Baseline-Dokument fehlt in diesem Checkout.

### Changed files / Geänderte Dateien (Nachbesserung 1)

- `docs/ABNAHME-0180.md`
- `docs/ABSCHLUSS-0180.md`
- `src-tauri/Cargo.toml`
- `src-tauri/src/commands/file_drag.rs`
- `src-tauri/src/drag_storage.rs`
- `src/components/ModelDetailPage.tsx`
- `src/components/ModelGrid.tsx`
- `src/components/ModelList.tsx`
- `src/components/NativeDragHandle.test.tsx`
- `src/components/NativeDragHandle.tsx`
- `src/i18n/de.ts`
- `src/i18n/en.ts`
- `src/i18n/es.ts`
- `src/i18n/fr.ts`
- `src/i18n/types.ts`
- `src/lib/nativeDrag.test.ts`
- `src/lib/nativeDrag.ts`

### Proof / Nachweise (Nachbesserung 1)

| Command / Befehl | Result / Ergebnis |
|---|---|
| `npx tsc --noEmit` | PASS, exit 0, no diagnostics / keine Diagnosen |
| `npx --no-install vitest run --configLoader runner` | PASS, exit 0: 361 files, 4,685 tests, 155.33 s |
| Targeted native drag Vitest tests / gezielte Drag-Tests | PASS: 2 files, 12 tests |
| `git diff --check` | PASS, exit 0, no output / keine Ausgabe |
| Rust workspace tests and Clippy | Not executed as agreed; requester must measure / vereinbarungsgemäß nicht ausgeführt, Auftraggeber misst |

Regression tests first failed on the missing timeout, filtering, accessibility
attributes and limit handling (5 failed, 7 passed); they pass after the fixes.
The failed timeout test also produced an unawaited-assertion diagnostic when its
early assertion aborted the test; the green run has no such diagnostic.
Logs: `.proof-0180/fix1-tsc.log`, `fix1-vitest.log`, `fix1-target.log`,
`fix1-red.log` (all under `.proof-0180/`). No sandbox-denied mutation was attempted.

Die Gegenproben scheiterten zuerst an den fehlenden Korrekturen (5 fehlgeschlagen,
7 bestanden); nach der Umsetzung sind alle grün. Im roten Timeout-Test entstand
beim vorzeitigen Abbruch außerdem eine Diagnose zur nicht abgewarteten Assertion;
im grünen Lauf tritt sie nicht auf. Keine schreibende Aktion wurde von der
Sandbox abgewiesen. Dieser Bericht ist beratend; die unabhängige Prüfung durch
den anderen Anbieter steht aus.


## Nachbesserung 2 — G1–G5

Frozen order: `FIX2.md`, SHA256
`3567a044f094c9d4b4beb74293c3b3cee1822d755189bd30d5bf0154d620d227`.
Base: `378695d3`. All source paths were resolved in this checkout.

- G1: Removed `collect_sessions` and its pre-drag invocation. One 24-hour
  retention constant remains for startup/exit collection of orphaned, unlocked
  instance folders. Added a 12-hour orphan preservation test; the existing test
  retains locked instances even beyond 24 hours. This supersedes F3's six-hour
  collection described above.
- G2: Left-button movement beyond four pixels starts exactly one drag. A click,
  release, blur or unmount cancels the pending gesture. Selection filtering and
  the 64-file limit remain covered.
- G3: SQLite opens with `uri=True` inside `contextlib.closing`. The new test
  checks URI mode, rejects a write before `query_only`, and verifies closure.
  Byte identity and absence of sidecar files remain tested.
- G4: The shared webview adapter suppresses every drag/drop event while
  `NativeDragState.isActive()`. All three consumers use that adapter, including
  image drops and external folder highlighting. Regression coverage includes
  event delivery resuming after the native drag ends and listener disposal.
- G5: The Rust timeout test now uses `Instant::now().checked_sub(DRAG_TIMEOUT)`.
  The greater-than-60-second held drag is listed as an open native acceptance
  case: timeout recovery ends self-drop suppression while the mouse may remain
  held.

**Implementation ruling:** G1's “same constant” is satisfied by removing the
second cleanup policy altogether, rather than retaining per-drag collection at
24 hours. The latter would still violate “no deletion on the next drag” and the
live-instance lock rule. Cost: copies from long-running live instances stay
until that instance exits and a later startup/exit finds them old and unlocked.
No other functional deviation. No dependencies, commits, pushes, publication or
changes to an original checkout. No sandbox-denied action was attempted.
Rust execution is unavailable by agreement; owner workspace tests and Clippy
remain required. A read-only code review found no actionable issues; this report
is advisory and does not replace the other provider's final independent review.

**Deutsch:** G1–G5 umgesetzt. G1 entfernt die Bereinigung beim nächsten Drag;
alle Aufräumvorgänge für aufbewahrte Kopien folgen derselben 24-Stunden-Grenze
und prüfen verwaiste, entsperrte Instanzen beim Start/Beenden. Der neue Rust-Test
prüft zwölf Stunden alte Kopien. Dies ersetzt die oben dokumentierte
Sechs-Stunden-Regel aus F3. G2 startet erst nach mehr als vier Pixeln Bewegung
mit gedrückter linker Taste, einmalig; Klick, Loslassen, Fokusverlust und Unmount
lösen keinen Drag aus. G3 erzwingt URI-Modus und Verbindungsschließung mit
Schreibversuchs-Test. G4 schützt alle Webview-Verbraucher zentral. G5 verwendet
`checked_sub`; der Langdrag nach Wächterfreigabe bleibt offene Plattformabnahme.

**Auslegung/Abweichung:** Statt nur die zweite Konstante auf 24 Stunden zu setzen,
entfällt die widersprüchliche Bereinigung beim nächsten Drag vollständig.
Kopien lange laufender Instanzen belegen daher weiter Speicher, bis nach ihrem
Ende ein Start/Beenden verwaiste, ausreichend alte Ordner bereinigt. Sonst keine
funktionale Abweichung. Keine abgewiesenen Aktionen; Rust bleibt
vereinbarungsgemäß ungemessen. Kein Commit, Push oder Veröffentlichen, keine
neuen Abhängigkeiten, keine Änderungen an Original-Checkouts. Die unabhängige
Abschlussprüfung durch den anderen Anbieter bleibt erforderlich.

### Files changed / Geänderte Dateien (Nachbesserung 2)

- `CHANGELOG.md`
- `docs/ABNAHME-0180.md`
- `docs/ABSCHLUSS-0180.md`
- `src-tauri/src/commands/file_drag.rs`
- `src-tauri/src/drag_storage.rs`
- `src/components/NativeDragHandle.tsx`
- `src/components/NativeDragHandle.test.tsx`
- `src/lib/webview.ts`
- `src/lib/webview.test.ts`
- `tools/tag_review.py`
- `tools/test_tag_review.py`

### Proof / Nachweise (Nachbesserung 2)

| Command / Befehl | Result / Ergebnis |
|---|---|
| `npx tsc --noEmit` | PASS, exit 0, no diagnostics / keine Diagnosen |
| Targeted grip and webview tests / gezielte Anfasser- und Webview-Tests | PASS, 2 files, 9 tests; initially 3 failed / zunächst 3 fehlgeschlagen |
| `python3 -m unittest discover -s tools -p test_tag_review.py` | PASS, exit 0, 4 tests; connection closure test initially failed / Verbindungsschließung zunächst fehlgeschlagen |
| `npx --no-install vitest run --configLoader runner` (first full run / erster Gesamtlauf) | FAIL, exit 1: 361 files passed, 1 failed; 4,686 tests passed, 1 failed; 160.40 s |
| Isolated `src/components/AutoTagBlock.web.test.tsx` retry / isolierte Wiederholung | PASS, exit 0, 2 tests, 2.06 s; no code change / ohne Codeänderung |
| Rust workspace tests and Clippy | Not executed as agreed; owner must measure / vereinbarungsgemäß nicht ausgeführt, Auftraggeber misst |

The first full run timed out after 5,000 ms in
`AutoTagBlock.web.test.tsx > offers remove-and-block in the web build and sends the block through the web command`.
That run also emitted duplicate React key warnings for `model-1`. Both tests
passed on isolated retry without code changes; this suggests a timing-sensitive
failure, but does not establish its root cause. No unrelated fix was made.

Im ersten Gesamtlauf scheiterte der oben genannte Test am 5.000-ms-Limit;
zusätzlich erschienen React-Warnungen zum doppelten Schlüssel `model-1`.
Isoliert bestanden beide Tests ohne Codeänderung. Ein zeitabhängiger Fehler ist
damit plausibel, seine Ursache aber nicht bewiesen; keine sachfremde Korrektur.

Logs under / Protokolle unter `.proof-0180/`: `fix2-red.log`, `fix2-green.log`,
`fix2-tsc.log`, `fix2-python.log`, `fix2-vitest.log`, `fix2-retry.log`.

The unchanged full proof command passed on repeat: **362 files, 4,687 tests,
exit 0, 154.36 s**, without the earlier duplicate-key warnings. Log:
`.proof-0180/fix2-vitest-repeat.log`. Final `git diff --check`: PASS, exit 0,
no output (`.proof-0180/fix2-diff-check.log`).

Der unveränderte vollständige Prüfbefehl bestand bei Wiederholung:
**362 Dateien, 4.687 Tests, Exit 0, 154,36 s**, ohne die vorherigen Warnungen zu
doppelten Schlüsseln. Abschließend `git diff --check`: bestanden, Exit 0,
keine Ausgabe. Der erste Timeout bleibt oben ausdrücklich dokumentiert.


## Nachbesserung 3 — RV-1–RV-4

Frozen order: `FIX3.md`, SHA256
`d02828b3406ace7d6cdb1eddf7413ed86f4f9ee090c1d1136ab4302255e55a9c`.
Base: `35c6fcd5`. Sources resolved only in this checkout.

- RV-1: Hourly background sweep and exit cleanup remove this instance's completed
  `drag-*` directories older than 24 hours. No collection on the next drag.
  A shared session guard protects staging and the native callback/grace period,
  independently of the 60-second UI timeout. Creation and sweeping share a mutex;
  a weak timer reference does not keep storage alive. Missing callbacks keep
  copies protected while the native callback retains its guard. Added Rust test
  source for 12-hour retention, deletion after 24 hours, active/callback guard
  protection, release, other-instance isolation and unknown/symlink preservation.
  This supersedes the long-running-instance storage limitation in correction 2.
- RV-2: Both named state lookups use `try_state`. Missing native state means
  inactive in the event hook and `DragFailed` in the command. State registration
  is the first setup operation.
- RV-3: Orphaned `file_tags` rows are skipped and counted in `orphaned_file_tags`.
  `links_before` still includes them; `distinct_links_after` counts valid links.
  `alias_collisions` lists normalized aliases and their sorted canonical keys,
  including collisions in excluded ambiguous words. Colliding aliases are not
  assigned an arbitrary winner. Two regression tests initially failed with
  KeyErrors and now pass; dictionary order does not change the result.
- RV-4: Acceptance text distinguishes unexecuted Rust test source from verified
  coverage. It names the untested hourly timer scheduling, 250-ms grace thread
  and 60-s backend watchdog thread, Rust ≥1.89, the existing desktop MSRV, stable
  CI and the digest-pinned Docker server build that does not compile src-tauri.

**Deutsch:** RV-1–RV-4 sind im Quelltext umgesetzt. Stündlicher Hintergrundlauf
und Beenden bereinigen abgeschlossene eigene Drag-Sitzungen nach 24 Stunden,
niemals der nächste Drag. Aktive Sitzungen sind durch einen eigenen Guard auch
nach UI-Timeout geschützt. Beide Zustandszugriffe sind optional, die Registrierung
steht am Setup-Anfang. Verwaiste Tag-Zuordnungen werden gezählt statt abzustürzen;
Alias-Kollisionen werden vollständig gemeldet und nicht willkürlich aufgelöst.
Die Abnahme beschreibt Rust-Testquelltext, ausstehende Ausführung und fehlende
Thread-Zeitnachweise ausdrücklich.

### Files changed / Geänderte Dateien

- `src-tauri/src/drag_storage.rs`
- `src-tauri/src/commands/file_drag.rs`
- `src-tauri/src/lib.rs`
- `tools/tag_review.py`
- `tools/test_tag_review.py`
- `docs/ABNAHME-0180.md`
- `docs/ABSCHLUSS-0180.md`

### Scope, blockers and deviations / Grenzen, Blockaden und Abweichungen

No functional requirement was dropped or redesigned. Conservative collision
handling excludes conflicting aliases from automatic merge suggestions. No new
dependencies, commits, pushes, publication or edits to an original checkout.
No sandbox-denied action was attempted. Rust compilation, tests and Clippy were
not executed, as the order explicitly delegates that proof to the requester;
**zero Rust tests executed here**. The private baseline and desktop collaboration
prompt are absent at the supplied paths. The supplied rules and frozen orders
were followed. This report is advisory; the other provider's independent final
review remains required.

Keine funktionale Anforderung verworfen oder umgestaltet; kollidierende Aliase
werden vorsorglich von automatischen Zusammenführungsvorschlägen ausgeschlossen.
Keine neuen Abhängigkeiten, Commits, Pushes, Veröffentlichung oder Änderungen am
Ursprungscheckout. Keine von der Sandbox abgewiesene Aktion. Rust ist laut
Auftrag nicht ausführbar: **0 ausgeführte Rust-Tests**, kein Build-/Clippy-Nachweis.
Private Baseline und Desktop-Zusammenarbeitsprompt fehlen an den angegebenen
Pfaden; die gelieferten Regeln und eingefrorenen Aufträge wurden befolgt.
Beratender Bericht, unabhängige Abschlussprüfung durch den anderen Anbieter offen.


### Proof / Nachweise (Nachbesserung 3)

| Command / Befehl | Result / Ergebnis |
|---|---|
| `npx tsc --noEmit` | PASS, exit 0, no diagnostics / keine Diagnosen |
| `npx --no-install vitest run --configLoader runner` | FAIL, exit 1: 361 files passed, 1 failed; 4,686 tests passed, 1 failed; 156.00 s |
| `npx --no-install vitest run --configLoader runner src/components/AutoTagBlock.web.test.tsx` | PASS, exit 0: 1 file, 2 tests; 2.07 s; no code change / ohne Codeänderung |
| `python3 -m unittest discover -s tools -p test_tag_review.py` | PASS, exit 0: 6 tests; initially 2 errors and 4 passed / zunächst 2 Fehler und 4 bestanden |
| `git diff --check` | PASS, exit 0, no output / keine Ausgabe |
| Rust workspace tests and Clippy | Not executed as agreed; zero Rust tests / vereinbarungsgemäß nicht ausgeführt; 0 Rust-Tests |

Full-run failure: `AutoTagBlock.web.test.tsx > offers remove-and-block in the web
build and sends the block through the web command` exceeded 5,000 ms and emitted
duplicate React key warnings for `model-1`. The previous correction report records
the same failure. Both tests pass on isolated retry without changes; that does
not make this full run green or prove the timeout's root cause. No unrelated
frontend fix was made. Full output: `.proof-0180/fix3-vitest.log`; isolated retry:
`.proof-0180/fix3-vitest-retry.log`.

Der Gesamtlauf scheiterte im genannten Test am 5.000-ms-Limit; dazu erschienen
React-Warnungen zu `model-1`. Derselbe Fehler ist bereits in Nachbesserung 2
dokumentiert. Isoliert bestehen beide Tests ohne Änderungen. Der Gesamtlauf
bleibt trotzdem fehlgeschlagen; die Ursache des Timeouts ist nicht bewiesen.
Keine sachfremde Frontend-Korrektur. Vollständige Protokolle siehe oben.
