# Test guide generator

**Testers only get the interactive test assistant** (`Testassistent-…html` /
`Test-Assistant-…html`, built with `--assistant-only`). preview.yml publishes
it without the version and with the language in the name
(`Testassistent-DE-Windows.html`, `Test-Assistant-EN-Windows.html`, …), so links
on the website, in Discord and in forum posts stay valid; 3mfkatalog.de reads
the version from the installer names. It guides them from downloading the right installer
to sending the result file to testing@3mfkatalog.de. The PDF guide and the
result sheet described below can still be built locally but are no longer
published.

**Silent test versions:** for a small fix, run preview.yml by hand with
`publish` switched off. It builds every installer as usual but leaves the
`preview` release alone: testers keep the current test version and get no
update. The installers stay as run artifacts (`gh run download <run id>`) for
testing in our own VMs; the fix reaches testers with the next published test
version. The scenario file for the silent version is still required.

`tools/testanleitung.py` builds a test guide (HTML, optionally PDF) and a
result sheet (.xlsx) per platform and language (German, English) for a preview
version, from:

- the fixed building blocks in `docs/tests/bausteine/` (German) and
  `docs/tests/bausteine/en/` (English): installing, data folders, reporting a
  bug, uninstalling — one file per platform where the text differs,
- `testdaten-<platform>.md` in the same folders: how to get and extract
  `3MF-Testdaten.zip` (sample catalog, archives and broken files). The ZIP is
  uploaded by hand to the `preview` release (`{{testdaten}}` links to it);
  preview.yml keeps it and its checksum across runs,
- the `[Unreleased]` part of `CHANGELOG.md` in that language ("what's new"),
- a scenario file per language: `docs/tests/<version>.md` (German) and
  `docs/tests/<version>.en.md` (English). Both must list the same scenario IDs
  in the same order, with the same `Nur:`/`Only:` restrictions; the English
  build fails otherwise. English scenarios quote the English UI texts; messages
  the backend only knows in German are quoted in German with a note.

Each build also writes an interactive **test assistant** per platform and
language (`Testassistent-<version>-<platform>.html`,
`Test-Assistant-<version>-<platform>.html`): a standalone offline page that
shows the install block and then one scenario at a time with "works /
doesn't work / skip" buttons, keeps its progress in the browser and saves the
answers (plus the active test time overall and per scenario, capped at
30 minutes per scenario so breaks don't count, and the optional device fields of the
result sheet) as
`Testergebnis-…txt` / `Test-Result-…txt`. It uses the same scenario files, so
there is nothing extra to write.

The assistant starts by asking for the scope: **Just what's new** (the newest
"Neu in …" chapter plus the update tests, suggested automatically to testers who
already have progress stored for an earlier test version), **Short test**
(setup, basic functions, F1/F2, what's new, update) or **Full test**
(everything). Tests that need more than clicking around (`EXPERT_TESTS` in
`tools/testanleitung.py`: broken files, the PowerShell lock test, log details)
move to an optional chapter "Für Profis / For experts" just before the update
tests and are only part of the full test.


The result sheet needs `openpyxl` (`python3-openpyxl` on Debian/Ubuntu).

## CLI

```
python3 tools/testanleitung.py <version> <windows|macos|linux> <out_dir> [--pdf] [--lang=de|en]
```

German (default) writes `Testanleitung-<version>-<Windows|macOS|Linux>.html`
(and `.pdf` with `--pdf`, rendered with headless Chromium/Chrome) and
`Ergebnisbogen-<version>-<platform>.xlsx`; English writes
`Test-Guide-<version>-<platform>.html/.pdf` and
`Result-Sheet-<version>-<platform>.xlsx`.

The sheet lists only the scenarios for that platform, grouped by chapter, with
a result list (OK / failed / skipped, coloured), a note column, device fields
that fit the platform (e.g. chip on macOS, session on Linux) and a summary.
The guide has no checkboxes: it points to the sheet row of every scenario.

## Scenario file format

German keywords are `Nur:`, `Schritte:`, `Erwartet:`, `Hinweis:`; English
files use `Only:`, `Steps:`, `Expected:`, `Note:`.

```markdown
# 3 · Einrichten

## E2 · Erster Start
Nur: windows, macos
Schritte:
1. Die App starten.
2. Den Ordner wählen.
Erwartet: Die App zeigt einen leeren Katalog.
Hinweis: Optional, Text in einer Zeile.
```

- `# <text>` starts a chapter. Only `## <ID> · <title>` blocks are rendered —
  text between a chapter heading and its first scenario is rejected with an
  error, so a chapter-level note belongs in a scenario's own `Hinweis:`.
- `## <ID> · <title>` starts a scenario inside the current chapter.
- `Nur:` (optional) restricts the scenario to a comma-separated list of
  platforms (`windows`, `macos`, `linux`); omitted means all platforms.
- `Schritte:` is followed by numbered lines (`1. …`, `2. …`, …).
- `Erwartet:` (required, one line) is the expected result.
- `Hinweis:` (optional, one line) is an extra note shown with the scenario.
- From v0.16.0 there is one installer per system, always with STEP preview.
  The assistant and result sheet no longer ask for a download variant.
  Archived `Variante:` / `Variant:` markers are still parsed: `step` scenarios
  now apply to everyone; obsolete `standard` scenarios are omitted.

Ab v0.16.0 gibt es einen Installer je System, immer mit STEP-Vorschau. Assistent
und Ergebnisbogen fragen nicht mehr nach der Variante. Alte `step`-Szenarien
gelten für alle; frühere `standard`-Szenarien entfallen. Archivierte
Szenariodateien bleiben unverändert.

### Screenshots

A step can carry a screenshot: an indented line right below the step,
`   Bild: <name> · <caption>` (English files: `   Image: …`). The expected
result can have one too: `Bild-Ergebnis: <name> · <caption>` (`Result image:`),
shown as "So sieht es richtig aus" / "This is what it should look like".
Pictures are `docs/tests/bilder/<name>.webp` (German UI) and
`docs/tests/bilder/en/<name>.webp` (English UI); `<name>.<platform>.webp`
wins over `<name>.webp` for pictures that show a path. A missing picture fails
the build. They are embedded in both the PDF guide and the test assistant.

The pictures are taken from the real frontend (production build) with demo
data and red numbered frames that count the clicks, by
`take_testassistent.py` in the screenshot pipeline next to the handbook
pictures (`~/Projekte/3mf-demo-katalog/shots/`, see its docstring). Retake
them when the UI of a shown screen changes.

A missing `Erwartet:` or an empty step list raises an error naming the
scenario's ID, so a scenario file with a typo fails the build instead of
shipping a broken guide.

## Placeholders

Building blocks, the guide title and scenario text (title, steps, `Erwartet:`,
`Hinweis:`) may use `{{version}}`, `{{produkt}}`, `{{datenordner}}`,
`{{logordner}}`, `{{paket}}`, `{{release}}` and `{{formular}}`; an unknown
placeholder fails the build. Prefer writing a scenario's expectation with a
concrete value; reach for a placeholder only when the same scenario applies
to every platform but the correct value genuinely differs per platform (e.g.
a data-folder path).

## Releasing a test version (checklist)

1. Add `docs/tests/<version>.md` and `<version>.en.md` (copy the previous version, add new scenarios or notes) and build the assistants once locally with `--assistant-only`.
2. Retake the screenshots **before** tagging: raise `VERSION`/`NEXT` in `3mf-demo-katalog/shots/take_testassistent.py`, run `vite build` + `vite preview --port 1420` and headless Chromium on port 9333, shoot German and English.
3. Commit, push, then push the tag `preview-<version>` and watch the "Preview (test version)" workflow.
4. Check the `preview` release: all packages and signatures, both manifests on the new version, spot checks against `SHA256SUMS.txt`, the assistants contain the new notes.
5. Update the Discord category "🧪 BETA-TEST": in #🇩🇪-beta-ankündigungen and #🇬🇧-beta-announcements, edit the test call at the top to the new version with a short "New since …" paragraph, and post a short notice below it. #🧪-beta-testing is for questions only; no announcements there (scripts in `discord-setup/`, e.g. `post-beta-bereich-0150-10-20260929.js`; dry run first, 2000-character limit). Afterwards run `node prune-beta-notices.js --apply` (after a dry run without `--apply`): each channel keeps only the test call and the newest notice.
6. The website picks up the new version by itself (`preview.php`, 15-minute cache).
