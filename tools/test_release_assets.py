import json, tempfile, unittest, pathlib, datetime
import release_assets as ra   # run with -s tools, so tools/ is on sys.path

class Names(unittest.TestCase):
    def test_asset_names(self):
        self.assertEqual(ra.asset_name("0.15.0", "Windows-x64", False, ".msi"), "3MF-Katalog-Manager-0.15.0-Windows-x64.msi")
        self.assertEqual(ra.asset_name("0.15.0", "macOS-universal", True, ".app.tar.gz"), "3MF-Katalog-Manager-0.15.0-macOS-universal-STEP.app.tar.gz")

class Rename(unittest.TestCase):
    def test_copies_each_artifact_under_its_new_name(self):
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as o:
            for art, ext, _, _ in ra.ARTIFACTS:
                d = pathlib.Path(a, art); d.mkdir()
                (d / f"whatever{ext}").write_bytes(b"x")
            ra.rename("0.15.0", a, o)
            names = sorted(p.name for p in pathlib.Path(o).iterdir())
            self.assertIn("3MF-Katalog-Manager-0.15.0-Linux-x86_64-STEP.AppImage", names)
            self.assertEqual(len(names), len(ra.ARTIFACTS))
    def test_missing_artifact_fails(self):
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as o:
            with self.assertRaises(SystemExit):
                ra.rename("0.15.0", a, o)

class LatestJson(unittest.TestCase):
    def test_both_variants_with_signatures(self):
        with tempfile.TemporaryDirectory() as o:
            for step in (False, True):
                for plat, ext in (("Windows-x64", ".msi"), ("macOS-universal", ".app.tar.gz"), ("Linux-x86_64", ".AppImage")):
                    n = ra.asset_name("0.15.0", plat, step, ext)
                    pathlib.Path(o, n).write_bytes(b"x")
                    pathlib.Path(o, n + ".sig").write_text(f"SIG-{n}")
            ra.latest_json("0.15.0", o, now=datetime.datetime(2026, 10, 3, 12, 0, tzinfo=datetime.timezone.utc))
            plain = json.loads(pathlib.Path(o, "latest.json").read_text())
            step = json.loads(pathlib.Path(o, "latest-step.json").read_text())
            self.assertEqual(plain["version"], "0.15.0")
            self.assertEqual(plain["pub_date"], "2026-10-03T12:00:00Z")
            self.assertEqual(plain["notes"], "https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.15.0")
            self.assertEqual(set(plain["platforms"]), {"windows-x86_64", "darwin-x86_64", "darwin-aarch64", "linux-x86_64"})
            w = plain["platforms"]["windows-x86_64"]
            self.assertEqual(w["url"], "https://github.com/Bexxs75/3mf-katalog-manager/releases/download/v0.15.0/3MF-Katalog-Manager-0.15.0-Windows-x64.msi")
            self.assertEqual(w["signature"], "SIG-3MF-Katalog-Manager-0.15.0-Windows-x64.msi")
            self.assertEqual(plain["platforms"]["darwin-aarch64"], plain["platforms"]["darwin-x86_64"])
            self.assertTrue(step["platforms"]["linux-x86_64"]["url"].endswith("-Linux-x86_64-STEP.AppImage"))
    def test_missing_signature_fails(self):
        with tempfile.TemporaryDirectory() as o:
            with self.assertRaises(SystemExit):
                ra.latest_json("0.15.0", o)

class Version(unittest.TestCase):
    def test_reads_versions(self):
        with tempfile.TemporaryDirectory() as r:
            p = pathlib.Path(r, "src-tauri"); p.mkdir()
            (p / "Cargo.toml").write_text('[package]\nname = "x"\nversion = "0.15.0"\n\n[dependencies]\nfoo = { version = "1" }\n')
            (p / "tauri.conf.json").write_text('{"version": "0.15.0"}')
            ra.check_version("0.15.0", r)
            with self.assertRaises(SystemExit):
                ra.check_version("0.15.1", r)

if __name__ == "__main__":
    unittest.main()
