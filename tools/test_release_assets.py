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

class Preview(unittest.TestCase):
    def test_preview_names(self):
        self.assertEqual(ra.asset_name("0.15.0-2", "Windows-x64", False, ".msi", preview=True),
                         "3MF-Katalog-Manager-Preview-0.15.0-2-Windows-x64.msi")

    def _files(self, d, version, preview, steps):
        for step in steps:
            for plat, ext in (("Windows-x64", ".msi"), ("macOS-universal", ".app.tar.gz"), ("Linux-x86_64", ".AppImage")):
                n = ra.asset_name(version, plat, step, ext, preview=preview)
                (d / n).write_text("x"); (d / (n + ".sig")).write_text("SIG-" + n)

    def test_preview_manifest_urls_and_names(self):
        with tempfile.TemporaryDirectory() as t:
            d = pathlib.Path(t); self._files(d, "0.15.0-2", True, (False,))
            ra.latest_json("0.15.0-2", d, now=datetime.datetime(2026, 10, 3, tzinfo=datetime.timezone.utc), preview=True, step=False)
            m = json.loads((d / "latest-preview.json").read_text())
            self.assertFalse((d / "latest-preview-step.json").exists())
            self.assertEqual(m["notes"], "https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/preview")
            self.assertEqual(m["platforms"]["linux-x86_64"]["url"],
                "https://github.com/Bexxs75/3mf-katalog-manager/releases/download/preview/3MF-Katalog-Manager-Preview-0.15.0-2-Linux-x86_64.AppImage")
            self.assertFalse((d / "latest.json").exists())

    def test_rename_without_step_ignores_step_artifacts(self):
        with tempfile.TemporaryDirectory() as t:
            src = pathlib.Path(t, "a"); out = pathlib.Path(t, "o")
            for art, ext, _p, step in ra.ARTIFACTS:
                if not step:
                    (src / art).mkdir(parents=True); (src / art / f"x{ext}").write_text("x")
            ra.rename("0.15.0-2", src, out, preview=True, step=False)
            self.assertEqual(len(list(out.iterdir())), 4)
            self.assertTrue((out / "3MF-Katalog-Manager-Preview-0.15.0-2-macOS-universal.dmg").exists())

    def test_check_preview(self):
        ra.check_preview("0.15.0-2")
        for bad in ("0.15.0", "0.15.0-rc.1", "v0.15.0-2"):
            with self.assertRaises(SystemExit): ra.check_preview(bad)
        with tempfile.TemporaryDirectory() as t:
            p = pathlib.Path(t, "latest-preview.json"); p.write_text(json.dumps({"version": "0.15.0-3"}))
            with self.assertRaises(SystemExit): ra.check_preview("0.15.0-2", p)
            ra.check_preview("0.15.0-3", p)   # rebuilding the same version is allowed
            ra.check_preview("0.15.1-1", p)
            self.assertLess(ra.version_key("0.15.0-10"), ra.version_key("0.15.1-1"))
            self.assertLess(ra.version_key("0.15.0-2"), ra.version_key("0.15.0-10"))

    def test_set_version(self):
        with tempfile.TemporaryDirectory() as t:
            r = pathlib.Path(t); (r / "src-tauri").mkdir()
            (r / "src-tauri/Cargo.toml").write_text('[package]\nname = "mf-katalog-manager"\nversion = "0.14.0"\n\n[dependencies]\nserde = { version = "1" }\n')
            (r / "src-tauri/tauri.conf.json").write_text(json.dumps({"productName": "x", "version": "0.14.0"}, indent=2))
            (r / "package.json").write_text(json.dumps({"name": "mf-katalog-manager", "version": "0.14.0"}, indent=2))
            (r / "src-tauri/Cargo.lock").write_text('[[package]]\nname = "serde"\nversion = "1.0.0"\n\n[[package]]\nname = "mf-katalog-manager"\nversion = "0.14.0"\n')
            ra.set_version("0.15.0-2", r)
            ra.check_version("0.15.0-2", r)
            self.assertEqual(json.loads((r / "package.json").read_text())["version"], "0.15.0-2")
            lock = (r / "src-tauri/Cargo.lock").read_text()
            self.assertIn('name = "mf-katalog-manager"\nversion = "0.15.0-2"', lock)
            self.assertIn('name = "serde"\nversion = "1.0.0"', lock)
            self.assertIn('serde = { version = "1" }', (r / "src-tauri/Cargo.toml").read_text())

if __name__ == "__main__":
    unittest.main()
