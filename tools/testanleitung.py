#!/usr/bin/env python3
"""Test guide generator for preview builds.

Builds a test guide in German or English (HTML, optionally a PDF via headless
Chromium) and a formatted result sheet (.xlsx) per platform, from fixed
building blocks, the "Unreleased" part of CHANGELOG.md in that language and a
per-version scenario file. Run with `-s tools` so `import release_assets`
resolves. Needs openpyxl for the result sheet.
"""
import copy, dataclasses, html, json, pathlib, re, shutil, subprocess, sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import release_assets

PRODUCT = "3MF Katalog Manager Preview"
IDENTIFIER = "com.thebexxs.mfkatalogmanager.preview"
REPO = "Bexxs75/3mf-katalog-manager"
PLATFORM_NAMES = {"windows": "Windows", "macos": "macOS", "linux": "Linux"}
# (release_assets platform key, package extension, data-folder path template,
# log-folder path template). Data-folder paths mirror
# docs/benutzerhandbuch/BENUTZERHANDBUCH.md, with the preview identifier. The
# log folder is a separate location: it comes from Tauri's app_log_dir (see
# the tauri_plugin_log::TargetKind::LogDir target in src-tauri/src/lib.rs),
# which on Windows and macOS is not simply "<data-folder>/logs".
PLATFORM_INFO = {
    "windows": ("Windows-x64", ".msi", r"%APPDATA%\{id}", r"%LOCALAPPDATA%\{id}\logs"),
    "macos": ("macOS-universal", ".dmg", "~/Library/Application Support/{id}", "~/Library/Logs/{id}"),
    "linux": ("Linux-x86_64", ".AppImage", "~/.local/share/{id}", "~/.local/share/{id}/logs"),
}


# Everything that differs between the German and the English guide and sheet.
# Scenario files are `docs/tests/<version>.md` (German) and
# `docs/tests/<version>.en.md`; building blocks live in `bausteine/` and
# `bausteine/en/`.
LANGS = {
    "de": {
        "scenario_suffix": ".md",
        "bausteine": "bausteine",
        "guide_file": "Testanleitung-{version}-{platform}",
        "sheet_file": "Ergebnisbogen-{version}-{platform}.xlsx",
        "guide_title": "{produkt} {version} – Testanleitung ({platform})",
        "form": "https://3mfkatalog.de/fehler-melden.html",
        "headings": ("Was ist neu", "Installieren", "Datenorte", "Fehler melden", "Deinstallieren"),
        "steps": "Schritte:",
        "expected": "Erwartet:",
        "note": "Hinweis:",
        "sheet_intro": ("Trag deine Ergebnisse im Ergebnisbogen ein ({sheet}). Er öffnet sich mit Excel, "
                        "Numbers oder LibreOffice. Diese Anleitung ist nur zum Lesen."),
        "result_hint": "Ergebnis im Ergebnisbogen eintragen, Zeile {id}",
        "sheet_title": "{produkt} {version} · Ergebnisbogen {platform}",
        "sheet_tab": "Ergebnisbogen",
        "sheet_help": ("Wähle bei jedem Test in der Spalte „Ergebnis“ OK, Fehler oder Übersprungen. Bei „Fehler“ "
                       "bitte kurz notieren, was passiert ist. Die Nummern entsprechen der Testanleitung (PDF)."),
        "columns": ("Nr.", "Test", "Ergebnis", "Notiz"),
        "results": ("OK", "Fehler", "Übersprungen"),
        "invalid_result": "Bitte OK, Fehler oder Übersprungen wählen.",
        "summary": "Zusammenfassung",
        "open": "Noch offen",
        "fields": {
            "windows": [("Gerät", "z. B. Lenovo ThinkPad T14", None),
                        ("Windows-Version", "z. B. Windows 11 24H2", None),
                        ("Virenschutz", "z. B. Microsoft Defender, Avast", None)],
            "macos": [("Gerät", "z. B. MacBook Air M4", None),
                      ("Chip", None, ("Apple Silicon", "Intel")),
                      ("macOS-Version", "z. B. macOS 15.5", None)],
            "linux": [("Gerät", "z. B. Tuxedo InfinityBook", None),
                      ("Distribution und Version", "z. B. Fedora 44, Linux Mint 22", None),
                      ("Desktop", "z. B. KDE Plasma, GNOME, Cinnamon", None),
                      ("Sitzung", None, ("Wayland", "X11"))],
            "common": [("App-Variante", None, ("Standard", "mit STEP-Vorschau")),
                       ("Datum", None, None),
                       ("Name oder Discord (freiwillig)", None, None)],
        },
    },
    "en": {
        "scenario_suffix": ".en.md",
        "bausteine": "bausteine/en",
        "guide_file": "Test-Guide-{version}-{platform}",
        "sheet_file": "Result-Sheet-{version}-{platform}.xlsx",
        "guide_title": "{produkt} {version} – Test guide ({platform})",
        "form": "https://3mfkatalog.de/en/report-a-bug.html",
        "headings": ("What's new", "Installing", "Where the data is", "Reporting a bug", "Uninstalling"),
        "steps": "Steps:",
        "expected": "Expected:",
        "note": "Note:",
        "sheet_intro": ("Enter your results in the result sheet ({sheet}). It opens in Excel, Numbers or "
                        "LibreOffice. This guide is for reading only."),
        "result_hint": "Enter the result in the result sheet, row {id}",
        "sheet_title": "{produkt} {version} · Result sheet {platform}",
        "sheet_tab": "Result sheet",
        "sheet_help": ("For every test, choose OK, Failed or Skipped in the column \"Result\". For \"Failed\", "
                       "please note briefly what happened. The numbers match the test guide (PDF)."),
        "columns": ("No.", "Test", "Result", "Note"),
        "results": ("OK", "Failed", "Skipped"),
        "invalid_result": "Please choose OK, Failed or Skipped.",
        "summary": "Summary",
        "open": "Still open",
        "fields": {
            "windows": [("Device", "e.g. Lenovo ThinkPad T14", None),
                        ("Windows version", "e.g. Windows 11 24H2", None),
                        ("Antivirus", "e.g. Microsoft Defender, Avast", None)],
            "macos": [("Device", "e.g. MacBook Air M4", None),
                      ("Chip", None, ("Apple Silicon", "Intel")),
                      ("macOS version", "e.g. macOS 15.5", None)],
            "linux": [("Device", "e.g. Tuxedo InfinityBook", None),
                      ("Distribution and version", "e.g. Fedora 44, Linux Mint 22", None),
                      ("Desktop", "e.g. KDE Plasma, GNOME, Cinnamon", None),
                      ("Session", None, ("Wayland", "X11"))],
            "common": [("App variant", None, ("Standard", "with STEP preview")),
                       ("Date", None, None),
                       ("Name or Discord (optional)", None, None)],
        },
    },
}


# Texts of the interactive test assistant (one test per screen, results saved
# as a text file). Kept apart from LANGS because only the assistant uses them.
ASSISTANT_TEXT = {
    "de": {
        "file": "Testassistent-{version}-{platform}.html",
        "result_file": "Testergebnis-{version}-{platform}.txt",
        "title": "{produkt} {version} – Testassistent ({platform})",
        "eyebrow": "Testassistent",
        "hello": "Hallo! Wir testen zusammen die Test-Version {version}.",
        "intro": ["Du bekommst immer nur einen Test auf einmal. Mach die Schritte der Reihe nach und sag danach, ob es geklappt hat.",
                  "Du kannst jederzeit aufhören. Wenn du diese Datei wieder öffnest, geht es an derselben Stelle weiter.",
                  "Wenn etwas nicht klappt, schreib kurz dazu, was stattdessen passiert ist.",
                  "Am Ende speicherst du das Ergebnis als Datei und schickst sie per E-Mail oder im Discord-Testkanal."],
        "start": "Los geht’s", "resume": "Weitermachen",
        "prep": "Vorbereitung", "install": "App installieren", "prep_done": "Erledigt, weiter",
        "test_of": "Test {n} von {total}", "all_done": "Alle Tests erledigt",
        "tip": "Tipp: Klick einen Schritt an, wenn du ihn erledigt hast.",
        "expected": "Das sollte passieren", "note": "Hinweis", "question": "Hat es geklappt?",
        "ok": "Klappt", "bad": "Klappt nicht", "skip": "Überspringen",
        "why": "Was ist stattdessen passiert?", "why_ph": "z. B. „Es kam die Meldung …“ oder „Der Ordner erschien nicht links“",
        "save_next": "Speichern und weiter", "skip_q": "Warum überspringst du diesen Test?", "choose": "Bitte wählen …",
        "skip_reasons": ["Das kann ich nicht testen (z. B. fehlt mir das Nötige)", "Ich weiß nicht, wie das geht", "Anderer Grund"],
        "next": "Weiter", "back": "← Zurück",
        "done_eyebrow": "Geschafft", "done": "Danke! Alle Tests sind durch.",
        "labels": {"ok": "klappt", "bad": "klappt nicht", "skip": "übersprungen", "open": "offen"},
        "device": "Zum Schluss noch kurz zu deinem Gerät (freiwillig):",
        "final": "Letzter Schritt: Speichere das Ergebnis und schick die Datei per E-Mail oder im Discord-Testkanal.",
        "save": "Ergebnis speichern", "saved": "Gespeichert als „{file}“ in deinem Download-Ordner.",
        "back_last": "← Letzten Test ändern", "reset": "Von vorn beginnen",
        "reset_confirm": "Wirklich alle Antworten löschen?", "reset_yes": "Ja, alles löschen", "reset_no": "Abbrechen",
        "result_head": "Testergebnis", "status": {"ok": "OK", "bad": "FEHLER", "skip": "ÜBERSPRUNGEN", "open": "OFFEN"},
    },
    "en": {
        "file": "Test-Assistant-{version}-{platform}.html",
        "result_file": "Test-Result-{version}-{platform}.txt",
        "title": "{produkt} {version} – Test assistant ({platform})",
        "eyebrow": "Test assistant",
        "hello": "Hi! Let's test version {version} together.",
        "intro": ["You get one test at a time. Do the steps in order, then tell us whether it worked.",
                  "You can stop at any time. When you open this file again, it continues where you left off.",
                  "If something doesn't work, write briefly what happened instead.",
                  "At the end you save the result as a file and send it by e-mail or in the Discord test channel."],
        "start": "Let's go", "resume": "Continue",
        "prep": "Preparation", "install": "Install the app", "prep_done": "Done, next",
        "test_of": "Test {n} of {total}", "all_done": "All tests done",
        "tip": "Tip: click a step once you have done it.",
        "expected": "This should happen", "note": "Note", "question": "Did it work?",
        "ok": "Works", "bad": "Doesn't work", "skip": "Skip",
        "why": "What happened instead?", "why_ph": "e.g. \"The message … appeared\" or \"The folder didn't show up on the left\"",
        "save_next": "Save and continue", "skip_q": "Why are you skipping this test?", "choose": "Please choose …",
        "skip_reasons": ["I can't test this (e.g. I don't have what it needs)", "I don't know how to do this", "Other reason"],
        "next": "Next", "back": "← Back",
        "done_eyebrow": "Done", "done": "Thank you! All tests are done.",
        "labels": {"ok": "works", "bad": "doesn't work", "skip": "skipped", "open": "open"},
        "device": "Finally, a few details about your device (optional):",
        "final": "Last step: save the result and send the file by e-mail or in the Discord test channel.",
        "save": "Save result", "saved": "Saved as \"{file}\" in your Downloads folder.",
        "back_last": "← Change the last test", "reset": "Start over",
        "reset_confirm": "Really delete all answers?", "reset_yes": "Yes, delete everything", "reset_no": "Cancel",
        "result_head": "Test result", "status": {"ok": "OK", "bad": "FAILED", "skip": "SKIPPED", "open": "OPEN"},
    },
}


@dataclasses.dataclass
class Scenario:
    id: str
    title: str
    steps: list
    expected: str
    note: str
    only: set


@dataclasses.dataclass
class Chapter:
    title: str
    scenarios: list


# German and English scenario files use their own keywords.
FIELD_KEYWORDS = {
    "Nur": "only", "Only": "only",
    "Schritte": "steps", "Steps": "steps",
    "Erwartet": "expected", "Expected": "expected",
    "Hinweis": "note", "Note": "note",
}
CHAPTER_RE = re.compile(r"^#\s+(.+?)\s*$")
SCENARIO_RE = re.compile(r"^##\s+(\S+)\s*·\s*(.+?)\s*$")


def parse_scenarios(text):
    """Parse the scenario Markdown format documented in docs/tests/README.md."""
    chapters = []
    chapter = None
    scenario = None
    section = None  # which multi-line field ("Schritte") is currently being collected
    for raw_line in text.splitlines():
        line = raw_line.rstrip()
        m = CHAPTER_RE.match(line)
        if m:
            if scenario is not None:
                _finish_scenario(scenario)
            chapter = Chapter(title=m.group(1), scenarios=[])
            chapters.append(chapter)
            scenario = None
            section = None
            continue
        m = SCENARIO_RE.match(line)
        if m:
            if scenario is not None:
                _finish_scenario(scenario)
            if chapter is None:
                raise ValueError(f"Szenario {m.group(1)}: kein Kapitel (# …) davor")
            scenario = Scenario(id=m.group(1), title=m.group(2), steps=[], expected="", note="", only=set())
            chapter.scenarios.append(scenario)
            section = None
            continue
        if scenario is None:
            # Free text between a chapter heading and its first scenario would
            # never reach the guide, so it is an error rather than lost silently.
            if line.strip():
                raise ValueError(f"Text ohne Szenario wird nicht übernommen: {line.strip()[:60]}")
            continue
        keyword, _, rest = line.partition(":")
        field = FIELD_KEYWORDS.get(keyword) if _ else None
        if field == "only":
            scenario.only = {p.strip() for p in rest.split(",") if p.strip()}
            section = None
        elif field == "steps":
            section = "steps"
        elif field == "expected":
            scenario.expected = rest.strip()
            section = None
        elif field == "note":
            scenario.note = rest.strip()
            section = None
        elif section == "steps":
            step_m = re.match(r"^\d+\.\s*(.+)$", line)
            if step_m:
                scenario.steps.append(step_m.group(1).strip())
    if scenario is not None:
        _finish_scenario(scenario)
    return chapters


def _finish_scenario(scenario):
    if not scenario.expected or not scenario.steps:
        raise ValueError(f"Szenario {scenario.id}: Erwartet/Expected oder Schritte/Steps fehlen")


def fill_scenario_text(chapters, values):
    """Fill {{name}} placeholders in scenario text (title, steps, expected,
    note) with the platform's values, in place. Lets a scenario that applies
    to all platforms name a path that actually differs per platform (e.g.
    {{datenordner}}) instead of forcing a platform-specific copy."""
    for chapter in chapters:
        for s in chapter.scenarios:
            s.title = fill(s.title, values)
            s.steps = [fill(step, values) for step in s.steps]
            s.expected = fill(s.expected, values)
            if s.note:
                s.note = fill(s.note, values)


PLACEHOLDER_RE = re.compile(r"\{\{(\w+)\}\}")


def fill(template, values):
    """Substitute {{name}} placeholders; an unknown name is a hard error so a
    typo in a building block never silently ships an empty guide."""
    def repl(m):
        name = m.group(1)
        if name not in values:
            raise ValueError(f"Platzhalter unbekannt: {name}")
        return values[name]
    return PLACEHOLDER_RE.sub(repl, template)


def platform_values(version, platform, lang="de"):
    asset_platform, ext, path_template, log_template = PLATFORM_INFO[platform]
    datenordner = path_template.format(id=IDENTIFIER)
    logordner = log_template.format(id=IDENTIFIER)
    paket = release_assets.asset_name(version, asset_platform, False, ext, preview=True)
    return {
        "version": version,
        "produkt": PRODUCT,
        "datenordner": datenordner,
        "logordner": logordner,
        "paket": paket,
        "release": f"https://github.com/{REPO}/releases/tag/preview",
        "formular": f"{LANGS[lang]['form']}?version={version}&os={platform}",
    }


UNRELEASED_RE = re.compile(r"## \[Unreleased\]\n(.*?)(?=\n## \[|\Z)", re.S)


def changelog_unreleased(text, lang="de"):
    """Return the '[Unreleased]' section in one language. CHANGELOG.md has the
    English text first and the German one after the '# Changelog (Deutsch)'
    heading; without that heading the first block is used for both."""
    marker = "# Changelog (Deutsch)"
    idx = text.find(marker)
    if idx == -1:
        search_text = text
    else:
        search_text = text[idx:] if lang == "de" else text[:idx]
    m = UNRELEASED_RE.search(search_text)
    return m.group(1).strip() if m else ""


def _markdown_inline(text):
    text = html.escape(text)
    text = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r'<a href="\2">\1</a>', text)
    text = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", text)
    text = re.sub(r"`([^`]+)`", r"<code>\1</code>", text)
    return text


def markdown_to_html(text):
    """Minimal Markdown->HTML for building blocks and the changelog excerpt:
    paragraphs, '-' lists, **bold**, `code`, [text](url) links."""
    lines = text.strip().splitlines()
    out = []
    para = []
    list_items = []

    def flush_para():
        if para:
            out.append(f"<p>{_markdown_inline(' '.join(para))}</p>")
            para.clear()

    def flush_list():
        if list_items:
            out.append("<ul>" + "".join(f"<li>{_markdown_inline(i)}</li>" for i in list_items) + "</ul>")
            list_items.clear()

    for line in lines:
        stripped = line.strip()
        if not stripped:
            flush_para()
            flush_list()
        elif stripped.startswith("#"):
            flush_para()
            flush_list()
            heading = stripped.lstrip("#").strip()
            out.append(f"<h3>{_markdown_inline(heading)}</h3>")
        elif stripped.startswith("- "):
            flush_para()
            list_items.append(stripped[2:].strip())
        else:
            flush_list()
            para.append(stripped)
    flush_para()
    flush_list()
    return "\n".join(out)


PAGE_CSS = """
@page { size: A4; margin: 18mm 16mm; }
body { font-family: "Segoe UI", system-ui, Arial, sans-serif; font-size: 10.5pt; line-height: 1.45; color: #222; }
h1 { font-size: 20pt; margin: 0 0 4pt; color: #c4502f; }
h2 { font-size: 14pt; margin: 20pt 0 6pt; color: #c4502f; border-bottom: 1.5px solid #f0c3b5; padding-bottom: 3pt; page-break-before: auto; page-break-after: avoid; }
h3 { font-size: 11.5pt; margin: 0 0 4pt; page-break-after: avoid; }
.sub { color: #666; margin-bottom: 12pt; }
code { font-family: Consolas, monospace; font-size: 9.5pt; background: #f4f1ee; padding: 1px 4px; border-radius: 3px; }
.szenario { page-break-inside: avoid; border: 1px solid #e5ddd6; border-radius: 6px; padding: 8px 11px; margin: 9pt 0; }
.szenario h3 { margin-top: 0; }
.lbl { font-weight: 600; color: #555; }
.ergebnis { color: #8a5a47; font-size: 9.5pt; margin-top: 6pt; }
.bogen { border-left: 3px solid #c4502f; background: #fbf1ec; padding: 6px 10px; margin: 8pt 0 12pt; }
ol, ul { margin: 3pt 0 3pt 18pt; padding: 0; }
li { margin: 2pt 0; }
"""


def _scenario_html(s, lang="de"):
    # Steps/expected/note may contain a `command` (e.g. the PowerShell one-liner in
    # P1), so they go through the same inline Markdown renderer as building blocks
    # and the changelog, rather than a plain html.escape that would leave the
    # backticks in place and print literally instead of rendering as <code>.
    t = LANGS[lang]
    steps_html = "<ol>" + "".join(f"<li>{_markdown_inline(step)}</li>" for step in s.steps) + "</ol>"
    note_html = f'<p><span class="lbl">{t["note"]}</span> {_markdown_inline(s.note)}</p>' if s.note else ""
    # No checkbox to tick here: a PDF can't be filled in, results go into the sheet.
    return (
        f'<div class="szenario"><h3>{html.escape(s.id)} · {_markdown_inline(s.title)}</h3>'
        f'<p><span class="lbl">{t["steps"]}</span></p>{steps_html}'
        f'<p><span class="lbl">{t["expected"]}</span> {_markdown_inline(s.expected)}</p>'
        f"{note_html}"
        f'<div class="ergebnis">→ {html.escape(t["result_hint"].format(id=s.id))}</div>'
        "</div>"
    )


def build(version, platform, out_dir, root=".", pdf=False, lang="de"):
    root = pathlib.Path(root)
    out_dir = pathlib.Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    t = LANGS[lang]
    platform_name = PLATFORM_NAMES[platform]
    values = platform_values(version, platform, lang)

    chapters = load_scenarios(root, version, lang)
    fill_scenario_text(chapters, values)

    bausteine_dir = root / "docs/tests" / t["bausteine"]

    def baustein(name):
        path = bausteine_dir / f"{name}.md"
        if not path.is_file():
            raise ValueError(f"Baustein fehlt: {path}")
        return markdown_to_html(fill(path.read_text(encoding="utf-8"), values))

    changelog_path = root / "CHANGELOG.md"
    changelog_html = (
        markdown_to_html(changelog_unreleased(changelog_path.read_text(encoding="utf-8"), lang))
        if changelog_path.is_file() else ""
    )

    chapters = platform_chapters(chapters, platform)
    chapters_html = [
        f"<h2>{html.escape(c.title)}</h2>" + "\n".join(_scenario_html(s, lang) for s in c.scenarios)
        for c in chapters
    ]

    sheet_name = t["sheet_file"].format(version=version, platform=platform_name)
    title = t["guide_title"].format(produkt=PRODUCT, version=version, platform=platform_name)
    new_h, install_h, data_h, report_h, uninstall_h = t["headings"]
    page = f"""<!doctype html><html lang="{lang}"><head><meta charset="utf-8"><title>{html.escape(title)}</title>
<style>{PAGE_CSS}</style></head><body>
<h1>{html.escape(title)}</h1>
<p class="bogen">{html.escape(t["sheet_intro"].format(sheet=sheet_name))}</p>
<h2>{new_h}</h2>
{changelog_html}
<h2>{install_h}</h2>
{baustein(f"installieren-{platform}")}
<h2>{data_h}</h2>
{baustein("daten")}
{''.join(chapters_html)}
<h2>{report_h}</h2>
{baustein("fehler-melden")}
<h2>{uninstall_h}</h2>
{baustein(f"deinstallieren-{platform}")}
</body></html>
"""
    html_path = out_dir / (t["guide_file"].format(version=version, platform=platform_name) + ".html")
    html_path.write_text(page, encoding="utf-8")

    sheet_path = out_dir / sheet_name
    write_result_sheet(sheet_path, chapters, version, platform, lang)
    build_assistant(out_dir, chapters, version, platform, lang, baustein(f"installieren-{platform}"))

    if pdf:
        render_pdf(html_path, html_path.with_suffix(".pdf"))
    return html_path, sheet_path


def build_assistant(out_dir, chapters, version, platform, lang, install_html):
    """Writes the interactive test assistant: a standalone HTML file that shows
    one test at a time, keeps its progress in the browser and saves the answers
    as a text file. `chapters` are already filtered for the platform and filled."""
    a = ASSISTANT_TEXT[lang]
    platform_name = PLATFORM_NAMES[platform]
    tests = [
        {"id": s.id, "chapter": c.title, "title": _markdown_inline(s.title),
         "steps": [_markdown_inline(step) for step in s.steps],
         "expected": _markdown_inline(s.expected), "note": _markdown_inline(s.note) if s.note else ""}
        for c in chapters for s in c.scenarios
    ]
    fields = [{"label": label, "hint": hint or "", "choices": list(choices or [])}
              for label, hint, choices in LANGS[lang]["fields"][platform] + LANGS[lang]["fields"]["common"]]
    title = a["title"].format(produkt=PRODUCT, version=version, platform=platform_name)
    data = {
        "text": a, "tests": tests, "fields": fields, "install": install_html, "title": title,
        "version": version, "platform": platform_name,
        "resultFile": a["result_file"].format(version=version, platform=platform_name),
        "storageKey": f"3mf-testassistent-{version}-{platform}-{lang}",
    }
    # "</" inside the JSON would end the <script> element early.
    payload = json.dumps(data, ensure_ascii=False).replace("</", "<\\/")
    page = (ASSISTANT_TEMPLATE.replace("__LANG__", lang).replace("__TITLE__", html.escape(title))
            .replace("__PRODUCT__", html.escape(PRODUCT)).replace("__VERSION__", html.escape(version))
            .replace("__PLATFORM__", html.escape(platform_name)).replace("__DATA__", payload))
    path = out_dir / a["file"].format(version=version, platform=platform_name)
    path.write_text(page, encoding="utf-8")
    return path


def load_scenarios(root, version, lang):
    """Scenarios of one language. The English file must list the same IDs in the
    same order as the German one, so both sheets and guides stay in step."""
    def read(suffix):
        path = root / "docs/tests" / f"{version}{suffix}"
        if not path.is_file():
            raise ValueError(f"Szenario-Datei fehlt: {path}")
        return parse_scenarios(path.read_text(encoding="utf-8"))

    chapters = read(LANGS[lang]["scenario_suffix"])
    if lang != "de":
        ids = [(s.id, sorted(s.only)) for c in chapters for s in c.scenarios]
        german = [(s.id, sorted(s.only)) for c in read(LANGS["de"]["scenario_suffix"]) for s in c.scenarios]
        if ids != german:
            raise ValueError(f"Szenarien ({lang}) passen nicht zur deutschen Datei: {ids} statt {german}")
    return chapters


def platform_chapters(chapters, platform):
    """Only the scenarios for this platform; a chapter left empty disappears."""
    result = []
    for c in chapters:
        scenarios = [s for s in c.scenarios if not s.only or platform in s.only]
        if scenarios:
            result.append(Chapter(title=c.title, scenarios=scenarios))
    return result


SHEET_COLORS = {"accent": "FF7A5C", "ink": "1F1B18", "muted": "6D665F", "hint": "A39B92",
                "chapter": "EFEBE7", "input": "FFF8E6", "line": "D9D4CE"}
RESULT_FILLS = ("D8F3E3", "FDDCDC", "ECEAE7")  # OK, failed, skipped


def write_result_sheet(path, chapters, version, platform, lang):
    """Result sheet for one platform: device fields, one row per scenario with a
    result list (OK/failed/skipped, coloured) and a note, and a summary."""
    from openpyxl import Workbook
    from openpyxl.formatting.rule import CellIsRule
    from openpyxl.styles import Alignment, Border, Font, PatternFill, Side
    from openpyxl.worksheet.datavalidation import DataValidation

    t = LANGS[lang]
    c = SHEET_COLORS
    fill = lambda color: PatternFill("solid", start_color=color, end_color=color)
    side = Side(style="thin", color=c["line"])
    box = Border(left=side, right=side, top=side, bottom=side)
    results = t["results"]

    wb = Workbook()
    ws = wb.active
    ws.title = t["sheet_tab"]
    ws.sheet_view.showGridLines = False
    for col, width in zip("ABCD", (7, 44, 18, 52)):
        ws.column_dimensions[col].width = width

    ws.merge_cells("A1:D1")
    ws["A1"] = t["sheet_title"].format(produkt=PRODUCT, version=version, platform=PLATFORM_NAMES[platform])
    ws["A1"].font = Font(size=16, bold=True, color=c["ink"])
    ws.row_dimensions[1].height = 28
    ws.merge_cells("A2:D2")
    ws["A2"] = t["sheet_help"]
    ws["A2"].font = Font(size=10, color=c["muted"])
    ws["A2"].alignment = Alignment(wrap_text=True, vertical="top")
    ws.row_dimensions[2].height = 30

    row = 4
    for label, hint, choices in t["fields"][platform] + t["fields"]["common"]:
        ws.cell(row, 1, label).font = Font(bold=True, color=c["ink"])
        ws.merge_cells(start_row=row, start_column=1, end_row=row, end_column=2)
        ws.merge_cells(start_row=row, start_column=3, end_row=row, end_column=4)
        for col in (3, 4):
            ws.cell(row, col).fill = fill(c["input"])
            ws.cell(row, col).border = box
        ws.row_dimensions[row].height = 20
        value = ws.cell(row, 3)
        value.alignment = Alignment(vertical="center")
        if hint:
            value.value = hint
            value.font = Font(italic=True, color=c["hint"])
        if choices:
            dv = DataValidation(type="list", formula1='"' + ",".join(choices) + '"', allow_blank=True)
            ws.add_data_validation(dv)
            dv.add(value)
        row += 1

    row += 1
    header_row = row
    for col, text in enumerate(t["columns"], start=1):
        cell = ws.cell(row, col, text)
        cell.font = Font(bold=True, color="FFFFFF")
        cell.fill = fill(c["accent"])
        cell.alignment = Alignment(vertical="center")
        cell.border = box
    ws.row_dimensions[row].height = 22
    row += 1

    result_list = DataValidation(type="list", formula1='"' + ",".join(results) + '"', allow_blank=True,
                                 showErrorMessage=True, errorTitle=t["columns"][2], error=t["invalid_result"])
    ws.add_data_validation(result_list)
    first = row
    for chapter in chapters:
        ws.merge_cells(start_row=row, start_column=1, end_row=row, end_column=4)
        cell = ws.cell(row, 1, chapter.title)
        cell.font = Font(bold=True, color=c["ink"])
        cell.fill = fill(c["chapter"])
        cell.alignment = Alignment(vertical="center")
        ws.row_dimensions[row].height = 20
        row += 1
        for s in chapter.scenarios:
            ws.cell(row, 1, s.id).font = Font(bold=True, color=c["muted"])
            # Titles may carry inline Markdown (`code`); the sheet shows plain text.
            ws.cell(row, 2, s.title.replace("`", ""))
            result_list.add(ws.cell(row, 3))
            for col in range(1, 5):
                ws.cell(row, col).border = box
                ws.cell(row, col).alignment = Alignment(wrap_text=True, vertical="center")
            ws.row_dimensions[row].height = 30
            row += 1
    last = row - 1

    results_range = f"C{first}:C{last}"
    for value, color in zip(results, RESULT_FILLS):
        ws.conditional_formatting.add(
            results_range, CellIsRule(operator="equal", formula=[f'"{value}"'], fill=fill(color)))

    row += 1
    ws.cell(row, 2, t["summary"]).font = Font(bold=True, color=c["ink"])
    row += 1
    # Chapter rows only fill column A, so counting column B counts scenarios.
    counts = [(value, f'=COUNTIF({results_range},"{value}")') for value in results]
    counts.append((t["open"], f"=COUNTA(B{first}:B{last})-COUNTA({results_range})"))
    for label, formula in counts:
        ws.cell(row, 2, label)
        ws.cell(row, 3, formula).alignment = Alignment(horizontal="left")
        row += 1

    # Calibri (openpyxl's default) is missing in Numbers and LibreOffice on Linux.
    for sheet_row in ws.iter_rows():
        for cell in sheet_row:
            font = copy.copy(cell.font)
            font.name = "Arial"
            cell.font = font

    ws.freeze_panes = ws.cell(header_row + 1, 1)
    ws.print_title_rows = f"{header_row}:{header_row}"
    ws.page_setup.orientation = "portrait"
    ws.page_setup.fitToWidth, ws.page_setup.fitToHeight = 1, 0
    ws.sheet_properties.pageSetUpPr.fitToPage = True
    wb.save(path)


# Standalone page of the test assistant. Offline by design: no external fonts
# or scripts, so it works from a double-click in the Downloads folder.
ASSISTANT_TEMPLATE = """<!doctype html>
<html lang="__LANG__"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>__TITLE__</title>
<style>
:root{--bg:#f6f2ef;--surface:#fff;--ink:#2b2320;--muted:#7a6a62;--line:#e5ddd6;--accent:#c4502f;--accent-ink:#fff;--accent-soft:#fbf1ec;
--ok:#2f7d4f;--ok-soft:#e6f3eb;--bad:#b3261e;--bad-soft:#fbe9e7;--skip:#8a7a3c;--skip-soft:#f6f1dc;--code:#f4f1ee;color-scheme:light}
@media (prefers-color-scheme:dark){:root{--bg:#1d1917;--surface:#282220;--ink:#f1e9e4;--muted:#b3a39a;--line:#3d3431;--accent:#e0714f;--accent-ink:#1d1917;
--accent-soft:#3a2721;--ok:#6cc08b;--ok-soft:#1f3326;--bad:#f08a80;--bad-soft:#3d2220;--skip:#d6c27a;--skip-soft:#353019;--code:#332b28;color-scheme:dark}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--ink);font-family:"Segoe UI",system-ui,-apple-system,Arial,sans-serif;font-size:17px;line-height:1.5;padding:20px 16px 48px}
.wrap{max-width:720px;margin:0 auto;display:flex;flex-direction:column;gap:16px}
header{display:flex;align-items:center;justify-content:space-between;gap:12px;flex-wrap:wrap}
.brand{font-weight:700}.brand span{color:var(--muted);font-weight:400}
.platform{font-size:13px;padding:3px 10px;border-radius:99px;background:var(--accent-soft);color:var(--accent);font-weight:600;letter-spacing:.03em;text-transform:uppercase}
.progress{display:flex;flex-direction:column;gap:6px}.progress .row{display:flex;justify-content:space-between;font-size:14px;color:var(--muted);font-variant-numeric:tabular-nums}
.bar{height:8px;border-radius:99px;background:var(--line);overflow:hidden}.bar i{display:block;height:100%;background:var(--accent);border-radius:99px;transition:width .3s}
.card{background:var(--surface);border:1px solid var(--line);border-radius:14px;padding:24px;display:flex;flex-direction:column;gap:18px}
.eyebrow{font-size:13px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);font-weight:600}
h1{font-size:26px;line-height:1.25;margin:0}h1 .id{color:var(--accent);margin-right:6px}
.card p{margin:0}.card ul{margin:0;padding-left:20px;display:flex;flex-direction:column;gap:6px}
ol.steps{margin:0;padding:0;list-style:none;display:flex;flex-direction:column;gap:10px;counter-reset:s}
ol.steps li{counter-increment:s;display:grid;grid-template-columns:34px 1fr;gap:12px;align-items:start;cursor:pointer;padding:8px;border-radius:10px}
ol.steps li:hover{background:var(--bg)}
ol.steps li::before{content:counter(s);width:30px;height:30px;border-radius:50%;display:grid;place-items:center;font-weight:700;font-size:15px;background:var(--accent-soft);color:var(--accent)}
ol.steps li.done{color:var(--muted)}ol.steps li.done::before{content:"\\2713";background:var(--ok-soft);color:var(--ok)}
.tip{font-size:13px;color:var(--muted);margin-top:-8px}
.expect{background:var(--accent-soft);border-left:4px solid var(--accent);border-radius:8px;padding:14px 16px}
.lbl{font-size:13px;font-weight:700;text-transform:uppercase;letter-spacing:.05em;color:var(--accent);display:block;margin-bottom:4px}
.note{border:1px solid var(--line);border-radius:8px;padding:12px 16px;font-size:15px}.note .lbl{color:var(--muted)}
.install h3{font-size:18px;margin:0}
code{font-family:Consolas,ui-monospace,monospace;background:var(--code);padding:1px 5px;border-radius:4px;font-size:.92em;overflow-wrap:anywhere}
a{color:var(--accent)}
.q{font-weight:600}
.actions{display:grid;grid-template-columns:repeat(3,1fr);gap:10px}@media (max-width:520px){.actions{grid-template-columns:1fr}}
button{font:inherit;cursor:pointer;border-radius:10px;border:2px solid var(--line);background:var(--surface);color:var(--ink);padding:14px 12px;font-weight:600}
button:focus-visible,textarea:focus-visible,select:focus-visible,input:focus-visible,li:focus-visible{outline:3px solid var(--accent);outline-offset:2px}
.b-ok{border-color:var(--ok);color:var(--ok)}.b-ok:hover,.b-ok.sel{background:var(--ok-soft)}
.b-bad{border-color:var(--bad);color:var(--bad)}.b-bad:hover,.b-bad.sel{background:var(--bad-soft)}
.b-skip{border-color:var(--skip);color:var(--skip)}.b-skip:hover,.b-skip.sel{background:var(--skip-soft)}
.primary{background:var(--accent);border-color:var(--accent);color:var(--accent-ink)}.primary:disabled{opacity:.45;cursor:not-allowed}
.followup{display:flex;flex-direction:column;gap:10px;border-top:1px solid var(--line);padding-top:16px}
textarea,select,input{font:inherit;color:var(--ink);background:var(--bg);border:1px solid var(--line);border-radius:8px;padding:10px 12px;width:100%}
textarea{min-height:96px;resize:vertical}
.nav{display:flex;justify-content:space-between;align-items:center;gap:10px;flex-wrap:wrap}
.linkbtn{background:none;border:none;padding:6px 0;color:var(--muted);font-weight:500;text-decoration:underline;text-underline-offset:3px}
.sum{display:grid;grid-template-columns:repeat(3,1fr);gap:10px}.sum div{border-radius:10px;padding:12px;text-align:center}.sum b{display:block;font-size:28px;line-height:1.1}
.s-ok{background:var(--ok-soft);color:var(--ok)}.s-bad{background:var(--bad-soft);color:var(--bad)}.s-skip{background:var(--skip-soft);color:var(--skip)}
table{width:100%;border-collapse:collapse;font-size:15px}td{padding:8px 6px;border-top:1px solid var(--line);vertical-align:top}td:first-child{white-space:nowrap;font-weight:600}
.tag{font-size:12px;font-weight:700;padding:2px 8px;border-radius:99px;white-space:nowrap}
.t-ok{background:var(--ok-soft);color:var(--ok)}.t-bad{background:var(--bad-soft);color:var(--bad)}.t-skip{background:var(--skip-soft);color:var(--skip)}.t-open{background:var(--bg);color:var(--muted)}
.fields{display:grid;gap:10px}.fields label{font-size:14px;color:var(--muted);display:flex;flex-direction:column;gap:4px}
.saved{color:var(--ok);font-weight:600}
@media (prefers-reduced-motion:reduce){.bar i{transition:none}}
</style></head><body>
<div class="wrap">
<header><div class="brand">__PRODUCT__ <span>· __VERSION__</span></div><div class="platform">__PLATFORM__</div></header>
<div class="progress" id="progress" hidden><div class="row"><span id="progText"></span><span id="progCounts"></span></div><div class="bar"><i id="barFill"></i></div></div>
<main id="view"></main>
</div>
<script id="data" type="application/json">__DATA__</script>
<script>
const D = JSON.parse(document.getElementById("data").textContent), T = D.text, TESTS = D.tests;
const fresh = () => ({ started:false, prepDone:false, index:0, results:{}, done:{}, device:{} });
let state = load() || fresh();
function load(){ try { return JSON.parse(localStorage.getItem(D.storageKey)); } catch(e) { return null; } }
function save(){ try { localStorage.setItem(D.storageKey, JSON.stringify(state)); } catch(e) {} }
const view = document.getElementById("view");
const esc = s => String(s).replace(/[&<>"]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c]));
const counts = () => { const r = Object.values(state.results); return { ok:r.filter(x=>x.s==="ok").length, bad:r.filter(x=>x.s==="bad").length, skip:r.filter(x=>x.s==="skip").length }; };
function progress(){
  const p = document.getElementById("progress"); p.hidden = !state.started || !state.prepDone; if (p.hidden) return;
  const i = Math.min(state.index, TESTS.length), c = counts();
  document.getElementById("progText").textContent = i >= TESTS.length ? T.all_done : T.test_of.replace("{n}", i+1).replace("{total}", TESTS.length);
  document.getElementById("progCounts").textContent = "✅ " + c.ok + " · ❌ " + c.bad + " · ⏭ " + c.skip;
  document.getElementById("barFill").style.width = (Object.keys(state.results).length / TESTS.length * 100) + "%";
}
function render(){
  progress();
  if (!state.started) return renderStart();
  if (!state.prepDone) return renderPrep();
  if (state.index >= TESTS.length) return renderSummary();
  renderTest(TESTS[state.index]);
}
function renderStart(){
  const resume = Object.keys(state.results).length > 0 || state.prepDone;
  view.innerHTML = '<section class="card"><div class="eyebrow">' + esc(T.eyebrow) + '</div><h1>' + esc(T.hello.replace("{version}", D.version)) + '</h1><ul>'
    + T.intro.map(x => '<li>' + esc(x) + '</li>').join("") + '</ul><button class="primary" id="go">' + esc(resume ? T.resume : T.start) + '</button></section>';
  document.getElementById("go").onclick = () => { state.started = true; save(); render(); };
}
function renderPrep(){
  view.innerHTML = '<section class="card install"><div class="eyebrow">' + esc(T.prep) + '</div><h1>' + esc(T.install) + '</h1>' + D.install
    + '<button class="primary" id="prepOk">' + esc(T.prep_done) + '</button></section>';
  view.querySelectorAll("a").forEach(a => { a.target = "_blank"; a.rel = "noopener"; });
  document.getElementById("prepOk").onclick = () => { state.prepDone = true; save(); render(); };
}
function renderTest(t){
  const r = state.results[t.id], doneSteps = state.done[t.id] || [];
  view.innerHTML = '<section class="card"><div class="eyebrow">' + esc(t.chapter) + '</div><h1><span class="id">' + esc(t.id) + '</span>' + t.title + '</h1>'
    + '<ol class="steps">' + t.steps.map((s, i) => '<li tabindex="0" data-i="' + i + '" class="' + (doneSteps.includes(i) ? "done" : "") + '">' + s + '</li>').join("") + '</ol>'
    + '<div class="tip">' + esc(T.tip) + '</div>'
    + '<div class="expect"><span class="lbl">' + esc(T.expected) + '</span>' + t.expected + '</div>'
    + (t.note ? '<div class="note"><span class="lbl">' + esc(T.note) + '</span>' + t.note + '</div>' : '')
    + '<p class="q">' + esc(T.question) + '</p><div class="actions">'
    + '<button class="b-ok" data-s="ok">✅ ' + esc(T.ok) + '</button><button class="b-bad" data-s="bad">❌ ' + esc(T.bad) + '</button><button class="b-skip" data-s="skip">⏭ ' + esc(T.skip) + '</button></div>'
    + '<div id="follow"></div><div class="nav"><button class="linkbtn" id="back"' + (state.index === 0 ? ' hidden' : '') + '>' + esc(T.back) + '</button></div></section>';
  view.querySelectorAll("a").forEach(a => { a.target = "_blank"; a.rel = "noopener"; });
  view.querySelectorAll("ol.steps li").forEach(li => {
    const toggle = () => { const i = +li.dataset.i, d = state.done[t.id] || (state.done[t.id] = []), k = d.indexOf(i); k < 0 ? d.push(i) : d.splice(k, 1); li.classList.toggle("done"); save(); };
    li.onclick = e => { if (e.target.closest("a")) return; toggle(); };
    li.onkeydown = e => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggle(); } };
  });
  view.querySelectorAll(".actions button").forEach(b => b.onclick = () => choose(t, b.dataset.s));
  document.getElementById("back").onclick = () => { state.index--; save(); render(); };
  if (r) { view.querySelector('.actions [data-s="' + r.s + '"]').classList.add("sel"); if (r.s !== "ok") choose(t, r.s, true); }
}
function choose(t, s, restoring){
  view.querySelectorAll(".actions button").forEach(b => b.classList.toggle("sel", b.dataset.s === s));
  const f = document.getElementById("follow"), prev = state.results[t.id] && state.results[t.id].s === s ? state.results[t.id] : null;
  if (s === "ok") { state.results[t.id] = { s:"ok" }; next(); return; }
  if (s === "bad") {
    f.innerHTML = '<div class="followup"><label class="q" for="why">' + esc(T.why) + '</label><textarea id="why" placeholder="' + esc(T.why_ph) + '">' + esc(prev ? prev.text : "") + '</textarea><button class="primary" id="cont" disabled>' + esc(T.save_next) + '</button></div>';
    const ta = document.getElementById("why"), c = document.getElementById("cont"), upd = () => c.disabled = ta.value.trim().length < 5;
    ta.oninput = upd; upd(); c.onclick = () => { state.results[t.id] = { s:"bad", text:ta.value.trim() }; next(); };
    if (!restoring) ta.focus();
  } else {
    f.innerHTML = '<div class="followup"><label class="q" for="reason">' + esc(T.skip_q) + '</label><select id="reason"><option value="">' + esc(T.choose) + '</option>'
      + T.skip_reasons.map(x => '<option' + (prev && prev.text === x ? ' selected' : '') + '>' + esc(x) + '</option>').join("") + '</select><button class="primary" id="cont" disabled>' + esc(T.next) + '</button></div>';
    const sel = document.getElementById("reason"), c = document.getElementById("cont"), upd = () => c.disabled = !sel.value;
    sel.onchange = upd; upd(); c.onclick = () => { state.results[t.id] = { s:"skip", text:sel.value }; next(); };
    if (!restoring) sel.focus();
  }
}
function next(){ state.index++; save(); render(); window.scrollTo(0, 0); }
function plain(h){ const d = document.createElement("div"); d.innerHTML = h; return d.textContent; }
function resultText(){
  const lines = [D.title, T.result_head + " · " + new Date().toLocaleString(), ""];
  D.fields.forEach((f, i) => { const v = (state.device[i] || "").trim(); if (v) lines.push(f.label + ": " + v); });
  lines.push("");
  TESTS.forEach(t => { const r = state.results[t.id]; lines.push(t.id + " " + plain(t.title) + ": " + T.status[r ? r.s : "open"] + (r && r.text ? " – " + r.text : "")); });
  return lines.join("\\r\\n") + "\\r\\n";
}
function renderSummary(){
  const c = counts();
  view.innerHTML = '<section class="card"><div class="eyebrow">' + esc(T.done_eyebrow) + '</div><h1>' + esc(T.done) + '</h1>'
    + '<div class="sum"><div class="s-ok"><b>' + c.ok + '</b>' + esc(T.labels.ok) + '</div><div class="s-bad"><b>' + c.bad + '</b>' + esc(T.labels.bad) + '</div><div class="s-skip"><b>' + c.skip + '</b>' + esc(T.labels.skip) + '</div></div>'
    + '<div style="overflow-x:auto"><table><tbody>' + TESTS.map(t => { const r = state.results[t.id], k = r ? r.s : "open";
        return '<tr><td>' + esc(t.id) + '</td><td>' + t.title + (r && r.text ? '<br><span style="color:var(--muted)">' + esc(r.text) + '</span>' : '') + '</td><td><span class="tag t-' + k + '">' + esc(T.labels[k]) + '</span></td></tr>'; }).join("") + '</tbody></table></div>'
    + '<p class="q">' + esc(T.device) + '</p><div class="fields">' + D.fields.map((f, i) => '<label>' + esc(f.label)
        + (f.choices.length ? '<select data-f="' + i + '"><option value=""></option>' + f.choices.map(x => '<option' + (state.device[i] === x ? ' selected' : '') + '>' + esc(x) + '</option>').join("") + '</select>'
                            : '<input data-f="' + i + '" placeholder="' + esc(f.hint) + '" value="' + esc(state.device[i] || "") + '">') + '</label>').join("") + '</div>'
    + '<p>' + esc(T.final) + '</p><button class="primary" id="saveRes">' + esc(T.save) + '</button><p class="saved" id="saved" hidden></p>'
    + '<div class="nav"><button class="linkbtn" id="back">' + esc(T.back_last) + '</button><span id="resetBox"><button class="linkbtn" id="reset">' + esc(T.reset) + '</button></span></div></section>';
  view.querySelectorAll("[data-f]").forEach(el => el.oninput = el.onchange = () => { state.device[el.dataset.f] = el.value; save(); });
  document.getElementById("saveRes").onclick = () => {
    const blob = new Blob(["﻿" + resultText()], { type:"text/plain;charset=utf-8" }), a = document.createElement("a");
    a.href = URL.createObjectURL(blob); a.download = D.resultFile; document.body.appendChild(a); a.click(); a.remove();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
    const s = document.getElementById("saved"); s.textContent = T.saved.replace("{file}", D.resultFile); s.hidden = false;
  };
  document.getElementById("back").onclick = () => { state.index = TESTS.length - 1; save(); render(); };
  document.getElementById("reset").onclick = () => {
    const box = document.getElementById("resetBox");
    box.innerHTML = esc(T.reset_confirm) + ' <button class="linkbtn" id="resetYes">' + esc(T.reset_yes) + '</button> <button class="linkbtn" id="resetNo">' + esc(T.reset_no) + '</button>';
    document.getElementById("resetYes").onclick = () => { state = fresh(); save(); render(); };
    document.getElementById("resetNo").onclick = () => render();
  };
}
document.addEventListener("keydown", e => {
  if (!state.started || !state.prepDone || state.index >= TESTS.length) return;
  if (["TEXTAREA","SELECT","INPUT"].includes(document.activeElement && document.activeElement.tagName)) return;
  const m = { "1":"ok", "2":"bad", "3":"skip" }[e.key]; if (m) choose(TESTS[state.index], m);
});
render();
</script></body></html>
"""


CHROMIUM_CANDIDATES = ["chromium", "google-chrome", "google-chrome-stable", "/usr/bin/chromium"]


def render_pdf(html_path, pdf_path):
    chromium = next((c for c in CHROMIUM_CANDIDATES if shutil.which(c) or pathlib.Path(c).is_file()), None)
    if chromium is None:
        raise ValueError("Kein Chromium/Chrome für das PDF gefunden")
    subprocess.run(
        [chromium, "--headless", "--no-sandbox", "--no-pdf-header-footer",
         f"--print-to-pdf={pdf_path}", html_path.resolve().as_uri()],
        check=True, capture_output=True,
    )


def main(argv):
    args = [a for a in argv if not a.startswith("--")]
    flags = [a for a in argv if a.startswith("--")]
    lang = next((f.split("=", 1)[1] for f in flags if f.startswith("--lang=")), "de")
    if len(args) < 3:
        print("usage: testanleitung.py <version> <windows|macos|linux> <out_dir> [--pdf] [--lang=de|en]",
              file=sys.stderr)
        return 1
    version, platform, out_dir = args[0], args[1], args[2]
    try:
        if platform not in PLATFORM_NAMES:
            raise ValueError(f"Unbekannte Plattform: {platform}")
        if lang not in LANGS:
            raise ValueError(f"Unbekannte Sprache: {lang}")
        build(version, platform, out_dir, pdf="--pdf" in flags, lang=lang)
    except ValueError as e:
        print(f"testanleitung: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
