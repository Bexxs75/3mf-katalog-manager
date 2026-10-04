import json, tempfile, unittest, pathlib, datetime
import release_assets as ra   # run with -s tools, so tools/ is on sys.path

class Names(unittest.TestCase):
    def test_asset_names(self):
        self.assertEqual(ra.asset_name("0.15.0", "Windows-x64", ".msi"), "3MF-Katalog-Manager-0.15.0-Windows-x64.msi")
        self.assertEqual(ra.asset_name("0.15.0", "macOS-universal", ".app.tar.gz"), "3MF-Katalog-Manager-0.15.0-macOS-universal.app.tar.gz")

class Rename(unittest.TestCase):
    def test_copies_each_artifact_under_its_new_name(self):
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as o:
            for art, ext, _ in ra.ARTIFACTS:
                d = pathlib.Path(a, art); d.mkdir()
                (d / f"whatever{ext}").write_bytes(b"x")
            ra.rename("0.15.0", a, o)
            names = sorted(p.name for p in pathlib.Path(o).iterdir())
            self.assertIn("3MF-Katalog-Manager-0.15.0-Linux-x86_64.AppImage", names)
            self.assertEqual(len(names), len(ra.ARTIFACTS))
            self.assertFalse(any("-STEP" in name for name in names))
    def test_missing_artifact_fails(self):
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as o:
            with self.assertRaises(SystemExit):
                ra.rename("0.15.0", a, o)

class LatestJson(unittest.TestCase):
    def test_identical_manifests_with_signatures(self):
        with tempfile.TemporaryDirectory() as o:
            for plat, ext in (("Windows-x64", ".msi"), ("macOS-universal", ".app.tar.gz"), ("Linux-x86_64", ".AppImage")):
                n = ra.asset_name("0.15.0", plat, ext)
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
            self.assertEqual(plain, step)
            self.assertEqual(pathlib.Path(o, "latest.json").read_bytes(), pathlib.Path(o, "latest-step.json").read_bytes())
            self.assertFalse(any("-STEP" in p.name for p in pathlib.Path(o).iterdir()))
    def test_missing_signature_fails(self):
        with tempfile.TemporaryDirectory() as o:
            for platform, ext in set(ra.UPDATE_TARGETS.values()):
                name = ra.asset_name("0.16.0", platform, ext)
                pathlib.Path(o, name).write_bytes(b"x")
                pathlib.Path(o, name + ".sig").write_text("SIG")
            pathlib.Path(o, "3MF-Katalog-Manager-0.16.0-Linux-x86_64.AppImage.sig").unlink()
            with self.assertRaises(SystemExit):
                ra.latest_json("0.16.0", o)
            self.assertFalse(pathlib.Path(o, "latest.json").exists())

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
        self.assertEqual(ra.asset_name("0.15.0-2", "Windows-x64", ".msi", preview=True),
                         "3MF-Katalog-Manager-Preview-0.15.0-2-Windows-x64.msi")

    def _files(self, d, version, preview):
        for plat, ext in (("Windows-x64", ".msi"), ("macOS-universal", ".app.tar.gz"), ("Linux-x86_64", ".AppImage")):
            n = ra.asset_name(version, plat, ext, preview=preview)
            (d / n).write_text("x"); (d / (n + ".sig")).write_text("SIG-" + n)

    def test_preview_manifest_urls_and_names(self):
        with tempfile.TemporaryDirectory() as t:
            d = pathlib.Path(t); self._files(d, "0.15.0-2", True)
            ra.latest_json("0.15.0-2", d, now=datetime.datetime(2026, 10, 3, tzinfo=datetime.timezone.utc), preview=True)
            m = json.loads((d / "latest-preview.json").read_text())
            self.assertEqual((d / "latest-preview.json").read_bytes(), (d / "latest-preview-step.json").read_bytes())
            self.assertEqual(m["notes"], "https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/preview")
            self.assertEqual(m["platforms"]["linux-x86_64"]["url"],
                "https://github.com/Bexxs75/3mf-katalog-manager/releases/download/preview/3MF-Katalog-Manager-Preview-0.15.0-2-Linux-x86_64.AppImage")
            self.assertFalse((d / "latest.json").exists())

    def test_missing_package_with_signature_fails(self):
        with tempfile.TemporaryDirectory() as t:
            d = pathlib.Path(t)
            self._files(d, "0.16.0-1", True)
            next(d.glob("*.msi")).unlink()
            with self.assertRaises(SystemExit):
                ra.latest_json("0.16.0-1", d, preview=True)

    def test_check_preview(self):
        ra.check_preview("0.15.0-2")
        for bad in ("0.15.0", "0.15.0-rc.1", "v0.15.0-2", "0.15.0-01", "0.15.0-1\n", "256.0.0-1", "0.15.0-70000"):
            with self.assertRaises(SystemExit): ra.check_preview(bad)
        with tempfile.TemporaryDirectory() as t:
            p = pathlib.Path(t, "latest-preview.json"); p.write_text(json.dumps({"version": "0.15.0-3"}))
            with self.assertRaises(SystemExit): ra.check_preview("0.15.0-2", p)
            ra.check_preview("0.15.0-3", p)   # rebuilding the same version is allowed
            ra.check_preview("0.15.1-1", p)
            self.assertLess(ra.version_key("0.15.0-10"), ra.version_key("0.15.1-1"))
            self.assertLess(ra.version_key("0.15.0-2"), ra.version_key("0.15.0-10"))

    def test_check_preview_manifest_edge_cases(self):
        with tempfile.TemporaryDirectory() as t:
            p = pathlib.Path(t, "latest-preview.json")
            p.write_text("")
            ra.check_preview("0.15.0-2", p)   # empty file = nothing published yet
            for broken in ("<html>404</html>", json.dumps({"pub_date": "x"}), json.dumps({"version": "abc"})):
                p.write_text(broken)
                with self.assertRaises(SystemExit): ra.check_preview("0.15.0-2", p)

    def test_each_old_preview_manifest_prevents_a_downgrade(self):
        with tempfile.TemporaryDirectory() as t:
            for name in ("latest-preview.json", "latest-preview-step.json"):
                path = pathlib.Path(t, name)
                path.write_text(json.dumps({"version": "0.16.0-3"}))
                with self.assertRaises(SystemExit):
                    ra.check_preview("0.16.0-2", path)
                ra.check_preview("0.16.0-3", path)

    def test_unknown_option_fails(self):
        with self.assertRaises(SystemExit): ra.main(["rename", "--foo", "0.15.0", "a", "b"])

    def test_set_version(self):
        with tempfile.TemporaryDirectory() as t:
            r = pathlib.Path(t); (r / "src-tauri").mkdir()
            (r / "src-tauri/Cargo.toml").write_text('[package]\nname = "mf-katalog-manager"\nversion = "0.14.0"\n\n[dependencies]\nserde = { version = "1" }\n', encoding="utf-8")
            (r / "src-tauri/tauri.conf.json").write_text(
                json.dumps({"productName": "3MF Katalog Manager – Prüfung", "version": "0.14.0"}, indent=2, ensure_ascii=False),
                encoding="utf-8",
            )
            (r / "package.json").write_text(json.dumps({"name": "mf-katalog-manager", "version": "0.14.0"}, indent=2), encoding="utf-8")
            (r / "src-tauri/Cargo.lock").write_text('[[package]]\nname = "serde"\nversion = "1.0.0"\n\n[[package]]\nname = "mf-katalog-manager"\nversion = "0.14.0"\n', encoding="utf-8")
            ra.set_version("0.15.0-2", r)
            ra.check_version("0.15.0-2", r)
            self.assertEqual(json.loads((r / "package.json").read_text(encoding="utf-8"))["version"], "0.15.0-2")
            lock = (r / "src-tauri/Cargo.lock").read_text(encoding="utf-8")
            self.assertIn('name = "mf-katalog-manager"\nversion = "0.15.0-2"', lock)
            self.assertIn('name = "serde"\nversion = "1.0.0"', lock)
            self.assertIn('serde = { version = "1" }', (r / "src-tauri/Cargo.toml").read_text(encoding="utf-8"))
            # ensure_ascii=False: the umlaut/en dash stay literal UTF-8, not \uXXXX escapes.
            conf_text = (r / "src-tauri/tauri.conf.json").read_text(encoding="utf-8")
            self.assertIn("3MF Katalog Manager – Prüfung", conf_text)
            self.assertNotIn("\\u", conf_text)

if __name__ == "__main__":
    unittest.main()
