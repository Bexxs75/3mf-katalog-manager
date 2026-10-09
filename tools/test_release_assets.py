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

class RcManifest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        self.source = self.root / 'latest.json'
        self.release = self.root / 'release.json'
        self.output = self.root / 'latest-rc.json'
        self.current = self.root / 'current.json'
        self.manifest, self.metadata = self.fixture('0.16.0-2')

    def fixture(self, version):
        platforms = {}
        for key, (platform, ext) in ra.UPDATE_TARGETS.items():
            url = f'https://github.com/{ra.REPO}/releases/download/v{version}/{ra.asset_name(version, platform, True, ext)}'
            platforms[key] = {'url': url, 'signature': 'signed'}
        manifest = {'version': version, 'platforms': platforms, 'notes': 'unchanged'}
        metadata = {'tag_name': 'v' + version, 'draft': False,
                    'assets': [{'browser_download_url': p['url'], 'state': 'uploaded'} for p in platforms.values()]}
        return manifest, metadata

    def run_copy(self, current=None, tag='v0.16.0-2'):
        self.source.write_text(json.dumps(self.manifest))
        self.release.write_text(json.dumps(self.metadata))
        args = ['rc-manifest', tag, str(self.source), str(self.release), str(self.output)]
        if current is not None:
            self.current.write_text(json.dumps({'version': current}))
            args.append(str(self.current))
        ra.main(args)

    def test_first_run_copies_bytes(self):
        self.run_copy()
        self.assertEqual(self.source.read_bytes(), self.output.read_bytes())

    def test_numeric_order_and_stable(self):
        versions = ['0.15.4', '0.16.0-2', '0.16.0-10', '0.16.0', '0.16.1-1']
        self.assertEqual(sorted(reversed(versions), key=ra.rc_version_key), versions)

    def test_equal_and_newer_rc_allowed(self):
        for current in ['0.16.0-1', '0.16.0-2', '0.15.4']:
            self.run_copy(current)

    def test_downgrade_preserves_output(self):
        for current in ['0.16.0-10', '0.16.0', '0.17.0-1']:
            self.output.write_text('keep')
            with self.assertRaises(SystemExit):
                self.run_copy(current)
            self.assertEqual(self.output.read_text(), 'keep')

    def test_stable_after_rc(self):
        self.manifest, self.metadata = self.fixture('0.16.0')
        self.run_copy('0.16.0-10', 'v0.16.0')

    def test_lower_stable_hotfix_rejected(self):
        self.manifest, self.metadata = self.fixture('0.15.4')
        with self.assertRaises(SystemExit):
            self.run_copy('0.16.0-2', 'v0.15.4')
        self.assertFalse(self.output.exists())

    def test_invalid_signature(self):
        for sig in [None, '', '  ', 42]:
            self.manifest['platforms']['linux-x86_64']['signature'] = sig
            with self.assertRaises(SystemExit):
                self.run_copy()
        del self.manifest['platforms']['linux-x86_64']['signature']
        with self.assertRaises(SystemExit):
            self.run_copy()

    def test_wrong_url_or_nonexistent_asset(self):
        for url in ['https://example.org/pkg',
                    f'https://github.com/{ra.REPO}/releases/download/v0.16.0-1/pkg',
                    f'https://github.com/{ra.REPO}/releases/download/v0.16.0-2/missing']:
            self.manifest['platforms']['linux-x86_64']['url'] = url
            with self.assertRaises(SystemExit):
                self.run_copy()

    def test_missing_platform(self):
        del self.manifest['platforms']['linux-x86_64']
        with self.assertRaises(SystemExit):
            self.run_copy()

    def test_invalid_release(self):
        for field, value in [('draft', True), ('tag_name', 'preview'), ('assets', [])]:
            self.manifest, self.metadata = self.fixture('0.16.0-2')
            self.metadata[field] = value
            with self.assertRaises(SystemExit):
                self.run_copy()

    def test_version_must_match_tag(self):
        self.manifest['version'] = '0.16.0-3'
        with self.assertRaises(SystemExit):
            self.run_copy()

    def test_invalid_tags_and_versions(self):
        for tag in ['preview', 'rc', 'v0.16.0-2\n', 'v0.16.0;echo x', 'v0.16.0-rc.1', 'v00.16.0', 'v0.16.0-02']:
            with self.assertRaises(SystemExit):
                self.run_copy(tag=tag)

    def test_unreadable_current_is_not_first_run(self):
        self.run_copy()
        for content in ['', '{}', 'null', '{"version": "invalid"}']:
            self.current.write_text(content)
            with self.assertRaises(SystemExit):
                ra.rc_manifest('v0.16.0-2', self.source, self.release, self.output, self.current)
        self.current.unlink()
        with self.assertRaises(SystemExit):
            ra.rc_manifest('v0.16.0-2', self.source, self.release, self.output, self.current)


if __name__ == "__main__":
    unittest.main()
