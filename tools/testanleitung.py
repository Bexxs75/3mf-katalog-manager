#!/usr/bin/env python3
"""Test guide generator for preview builds.

Builds a German HTML test guide (optionally a PDF via headless Chromium) and
a CSV result sheet from fixed building blocks, the German "Unreleased" part
of CHANGELOG.md and a per-version scenario file. Run with `-s tools` so
`import release_assets` resolves.
"""
import csv, dataclasses, html, pathlib, re, shutil, subprocess, sys

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
        if line.startswith("Nur:"):
            scenario.only = {p.strip() for p in line[len("Nur:"):].split(",") if p.strip()}
            section = None
        elif line.startswith("Schritte:"):
            section = "steps"
        elif line.startswith("Erwartet:"):
            scenario.expected = line[len("Erwartet:"):].strip()
            section = None
        elif line.startswith("Hinweis:"):
            scenario.note = line[len("Hinweis:"):].strip()
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
        raise ValueError(f"Szenario {scenario.id}: Erwartet oder Schritte fehlen")


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


def platform_values(version, platform):
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
        "formular": f"https://3mfkatalog.de/fehler-melden.html?version={version}&os={platform}",
    }


UNRELEASED_RE = re.compile(r"## \[Unreleased\]\n(.*?)(?=\n## \[|\Z)", re.S)


def changelog_unreleased_de(text):
    """Return the German '[Unreleased]' section: the block after the
    '# Changelog (Deutsch)' heading if present, else the first block."""
    marker = "# Changelog (Deutsch)"
    idx = text.find(marker)
    search_text = text[idx:] if idx != -1 else text
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
.ergebnis { border: 1px dashed #bbb; border-radius: 4px; padding: 5px 8px; margin-top: 6pt; }
ol, ul { margin: 3pt 0 3pt 18pt; padding: 0; }
li { margin: 2pt 0; }
"""


def _scenario_html(s):
    # Steps/expected/note may contain a `command` (e.g. the PowerShell one-liner in
    # P1), so they go through the same inline Markdown renderer as building blocks
    # and the changelog, rather than a plain html.escape that would leave the
    # backticks in place and print literally instead of rendering as <code>.
    steps_html = "<ol>" + "".join(f"<li>{_markdown_inline(step)}</li>" for step in s.steps) + "</ol>"
    note_html = f'<p><span class="lbl">Hinweis:</span> {_markdown_inline(s.note)}</p>' if s.note else ""
    return (
        f'<div class="szenario"><h3>{html.escape(s.id)} · {_markdown_inline(s.title)}</h3>'
        f'<p><span class="lbl">Schritte:</span></p>{steps_html}'
        f'<p><span class="lbl">Erwartet:</span> {_markdown_inline(s.expected)}</p>'
        f"{note_html}"
        '<div class="ergebnis">Ergebnis: &#9744; OK &#9744; Fehler – Notiz: ______________________________</div>'
        "</div>"
    )


def build(version, platform, out_dir, root=".", pdf=False):
    root = pathlib.Path(root)
    out_dir = pathlib.Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    platform_name = PLATFORM_NAMES[platform]
    values = platform_values(version, platform)

    scenario_path = root / "docs/tests" / f"{version}.md"
    if not scenario_path.is_file():
        raise ValueError(f"Szenario-Datei fehlt: {scenario_path}")
    chapters = parse_scenarios(scenario_path.read_text(encoding="utf-8"))
    fill_scenario_text(chapters, values)

    bausteine_dir = root / "docs/tests/bausteine"

    def baustein(name):
        path = bausteine_dir / f"{name}.md"
        if not path.is_file():
            raise ValueError(f"Baustein fehlt: {path}")
        return markdown_to_html(fill(path.read_text(encoding="utf-8"), values))

    changelog_path = root / "CHANGELOG.md"
    changelog_html = markdown_to_html(changelog_unreleased_de(changelog_path.read_text(encoding="utf-8"))) if changelog_path.is_file() else ""

    chapters_html = []
    csv_rows = []
    for chapter in chapters:
        scenario_htmls = []
        for s in chapter.scenarios:
            csv_rows.append([s.id, s.title, ",".join(sorted(s.only)) if s.only else "alle", "", ""])
            if s.only and platform not in s.only:
                continue
            scenario_htmls.append(_scenario_html(s))
        if scenario_htmls:
            chapters_html.append(f"<h2>{html.escape(chapter.title)}</h2>" + "\n".join(scenario_htmls))

    title = fill("{{produkt}} {{version}} – Testanleitung", values) + f" ({platform_name})"
    page = f"""<!doctype html><html lang="de"><head><meta charset="utf-8"><title>{html.escape(title)}</title>
<style>{PAGE_CSS}</style></head><body>
<h1>{html.escape(title)}</h1>
<h2>Was ist neu</h2>
{changelog_html}
<h2>Installieren</h2>
{baustein(f"installieren-{platform}")}
<h2>Datenorte</h2>
{baustein("daten")}
{''.join(chapters_html)}
<h2>Fehler melden</h2>
{baustein("fehler-melden")}
<h2>Deinstallieren</h2>
{baustein(f"deinstallieren-{platform}")}
</body></html>
"""
    html_path = out_dir / f"Testanleitung-{version}-{platform_name}.html"
    html_path.write_text(page, encoding="utf-8")

    csv_path = out_dir / f"Ergebnisbogen-{version}.csv"
    with csv_path.open("w", encoding="utf-8-sig", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["ID", "Titel", "Plattform", "Ergebnis (OK/Fehler)", "Notiz"])
        writer.writerows(csv_rows)

    if pdf:
        pdf_path = out_dir / f"Testanleitung-{version}-{platform_name}.pdf"
        render_pdf(html_path, pdf_path)

    return html_path, csv_path


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
    if len(argv) < 3:
        print("usage: testanleitung.py <version> <windows|macos|linux> <out_dir> [--pdf]", file=sys.stderr)
        return 1
    version, platform, out_dir = argv[0], argv[1], argv[2]
    pdf = "--pdf" in argv[3:]
    try:
        if platform not in PLATFORM_NAMES:
            raise ValueError(f"Unbekannte Plattform: {platform}")
        build(version, platform, out_dir, pdf=pdf)
    except ValueError as e:
        print(f"testanleitung: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
