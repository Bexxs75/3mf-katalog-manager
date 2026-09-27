import csv, pathlib, tempfile, unittest
import testanleitung as ta

SAMPLE = """# 3 · Einrichten

## E2 · Erster Start
Nur: windows
Schritte:
1. Die App starten.
2. Den Ordner wählen.
Erwartet: Leerer Katalog.
Hinweis: Nur beim ersten Mal.

## E3 · Backup
Schritte:
1. Backup einspielen.
Erwartet: Filament ist da.
"""

class Parse(unittest.TestCase):
    def test_chapters_and_scenarios(self):
        ch = ta.parse_scenarios(SAMPLE)
        self.assertEqual(ch[0].title, "3 · Einrichten")
        e2 = ch[0].scenarios[0]
        self.assertEqual((e2.id, e2.title, e2.steps, e2.expected, e2.note, e2.only),
                         ("E2", "Erster Start", ["Die App starten.", "Den Ordner wählen."], "Leerer Katalog.", "Nur beim ersten Mal.", {"windows"}))
        self.assertEqual(ch[0].scenarios[1].only, set())

    def test_missing_expectation_names_the_id(self):
        with self.assertRaisesRegex(ValueError, "E9"):
            ta.parse_scenarios("# K\n\n## E9 · X\nSchritte:\n1. a\n")

class ParseErrors(unittest.TestCase):
    def test_scenario_without_chapter(self):
        with self.assertRaisesRegex(ValueError, "E1"):
            ta.parse_scenarios("## E1 · X\nSchritte:\n1. a\nErwartet: b\n")

    def test_text_outside_a_scenario_is_an_error(self):
        with self.assertRaisesRegex(ValueError, "Einleitung"):
            ta.parse_scenarios("# K\n\nEinleitung zum Kapitel.\n\n## E1 · X\nSchritte:\n1. a\nErwartet: b\n")

    def test_last_scenario_of_a_chapter_is_checked(self):
        with self.assertRaisesRegex(ValueError, "E1"):
            ta.parse_scenarios("# K\n\n## E1 · X\nSchritte:\n1. a\n\n# L\n\n## E2 · Y\nSchritte:\n1. a\nErwartet: b\n")


class Fill(unittest.TestCase):
    def test_unknown_placeholder_fails(self):
        with self.assertRaisesRegex(ValueError, "unbekannt"):
            ta.fill("{{version}} {{unbekannt}}", {"version": "1"})

    def test_platform_values(self):
        v = ta.platform_values("0.15.0-2", "windows")
        self.assertEqual(v["paket"], "3MF-Katalog-Manager-Preview-0.15.0-2-Windows-x64.msi")
        self.assertIn("com.thebexxs.mfkatalogmanager.preview", v["datenordner"])
        self.assertIn("version=0.15.0-2", v["formular"])
        # Log paths match Tauri's app_log_dir (see tauri-plugin-log's
        # TargetKind::LogDir), which is NOT "<data folder>/logs" on Windows/macOS.
        self.assertEqual(v["logordner"], r"%LOCALAPPDATA%\com.thebexxs.mfkatalogmanager.preview\logs")
        macos = ta.platform_values("0.15.0-2", "macos")
        self.assertEqual(macos["logordner"], "~/Library/Logs/com.thebexxs.mfkatalogmanager.preview")
        linux = ta.platform_values("0.15.0-2", "linux")
        self.assertEqual(linux["logordner"], "~/.local/share/com.thebexxs.mfkatalogmanager.preview/logs")

class FillScenarioText(unittest.TestCase):
    def test_substitutes_placeholders_in_place(self):
        ch = ta.parse_scenarios("# K\n\n## U5 · Sicherung\nSchritte:\n1. Öffne {{datenordner}}.\nErwartet: Datei {{version}}.db liegt dort.\n")
        ta.fill_scenario_text(ch, {"datenordner": "/x/y", "version": "0.15.0-2"})
        s = ch[0].scenarios[0]
        self.assertEqual(s.steps, ["Öffne /x/y."])
        self.assertEqual(s.expected, "Datei 0.15.0-2.db liegt dort.")

    def test_unknown_placeholder_in_scenario_text_fails(self):
        ch = ta.parse_scenarios("# K\n\n## U5 · X\nSchritte:\n1. a\nErwartet: {{unbekannt}}\n")
        with self.assertRaisesRegex(ValueError, "unbekannt"):
            ta.fill_scenario_text(ch, {"version": "1"})

class Changelog(unittest.TestCase):
    def test_german_part_of_unreleased(self):
        text = "# Changelog\n\n## [Unreleased]\n\n### Added\n- **Update:** EN text\n\n## [0.14.0]\n- old\n\n# Changelog (Deutsch)\n\n## [Unreleased]\n\n### Hinzugefügt\n- **Update:** DE-Text\n\n## [0.14.0]\n- alt\n"
        self.assertIn("DE-Text", ta.changelog_unreleased_de(text))
        self.assertNotIn("alt", ta.changelog_unreleased_de(text))
        self.assertNotIn("EN text", ta.changelog_unreleased_de(text))

class ScenarioHtml(unittest.TestCase):
    def test_backticks_become_code_and_no_raw_html_passes(self):
        s = ta.Scenario(
            id="P1", title="Mit `code`",
            steps=["Nutze `$f = [System.IO.File]::Open(\"<Katalog>\\x.3mf\")`."],
            expected="Zeigt `<script>alert(1)</script>` nicht aus.",
            note="Auch `hier`.", only=set(),
        )
        out = ta._scenario_html(s)
        self.assertIn("<code>code</code>", out)
        self.assertIn("<code>$f = [System.IO.File]::Open(&quot;&lt;Katalog&gt;\\x.3mf&quot;)</code>", out)
        self.assertIn("<code>hier</code>", out)
        # The angle brackets inside a backtick span must stay escaped text, not
        # turn into real tags - that was the whole bug (raw HTML from user text).
        self.assertIn("&lt;script&gt;alert(1)&lt;/script&gt;", out)
        self.assertNotIn("<script>", out)
        self.assertNotIn("<Katalog>", out)

class Build(unittest.TestCase):
    def test_html_and_csv(self):
        with tempfile.TemporaryDirectory() as t:
            root = pathlib.Path(t)
            (root / "docs/tests/bausteine").mkdir(parents=True)
            for name in ("installieren-windows", "daten", "fehler-melden", "deinstallieren-windows"):
                (root / f"docs/tests/bausteine/{name}.md").write_text(f"Baustein {name} {{{{version}}}}\n")
            (root / "docs/tests/0.15.0-2.md").write_text(SAMPLE)
            (root / "CHANGELOG.md").write_text("## [Unreleased]\n### Hinzugefügt\n- Neu\n")
            out = root / "out"
            ta.build("0.15.0-2", "windows", out, root=root, pdf=False)
            html = (out / "Testanleitung-0.15.0-2-Windows.html").read_text()
            self.assertIn("Erster Start", html); self.assertIn("Baustein daten 0.15.0-2", html)
            rows = list(csv.reader((out / "Ergebnisbogen-0.15.0-2.csv").open(encoding="utf-8-sig")))
            self.assertEqual(rows[0][:2], ["ID", "Titel"]); self.assertEqual([r[0] for r in rows[1:]], ["E2", "E3"])

if __name__ == "__main__":
    unittest.main()
