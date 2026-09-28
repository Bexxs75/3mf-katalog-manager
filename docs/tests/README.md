# Test guide generator

`tools/testanleitung.py` builds a test guide (HTML, optionally PDF) and a
result sheet (.xlsx) per platform and language (German, English) for a preview
version, from:

- the fixed building blocks in `docs/tests/bausteine/` (German) and
  `docs/tests/bausteine/en/` (English): installing, data folders, reporting a
  bug, uninstalling — one file per platform where the text differs,
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
answers (plus the variant, the active test time overall and per scenario, capped at
30 minutes per scenario so breaks don't count, and the optional device fields of the
result sheet) as
`Testergebnis-…txt` / `Test-Result-…txt`. It uses the same scenario files, so
there is nothing extra to write.

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
- `Variante:` / `Variant:` (optional) `standard` or `step`: the scenario only
  applies to that download variant. The assistant asks for the variant at the
  start, leaves the other variant's scenarios out and lists them as not
  applicable; the guide shows a note.

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
