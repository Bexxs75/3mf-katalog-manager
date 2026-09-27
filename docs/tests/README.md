# Test guide generator

`tools/testanleitung.py` builds a German test guide (HTML, optionally PDF) and a
CSV result sheet for a preview version, from:

- the fixed building blocks in `docs/tests/bausteine/` (installing, data
  folders, reporting a bug, uninstalling — one file per platform where the
  text differs),
- the German `[Unreleased]` part of `CHANGELOG.md` ("what's new"),
- a scenario file `docs/tests/<version>.md` (test steps, written per release).

## CLI

```
python3 tools/testanleitung.py <version> <windows|macos|linux> <out_dir> [--pdf]
```

Writes `<out_dir>/Testanleitung-<version>-<Windows|macOS|Linux>.html` (and
`.pdf` with `--pdf`, rendered with headless Chromium/Chrome) and
`<out_dir>/Ergebnisbogen-<version>.csv` (one row per scenario, across all
platforms).

## Scenario file format

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
  any text between a chapter heading and its first scenario is dropped, so a
  chapter-level note belongs in a scenario's own `Hinweis:` instead.
- `## <ID> · <title>` starts a scenario inside the current chapter.
- `Nur:` (optional) restricts the scenario to a comma-separated list of
  platforms (`windows`, `macos`, `linux`); omitted means all platforms.
- `Schritte:` is followed by numbered lines (`1. …`, `2. …`, …).
- `Erwartet:` (required, one line) is the expected result.
- `Hinweis:` (optional, one line) is an extra note shown with the scenario.

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
