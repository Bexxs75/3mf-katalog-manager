"""The frontend may only call window commands the main window's capability grants.

A missing permission is invisible to unit tests and only fails in the packaged app:
the close handler (onCloseRequested) ends the window with a destroy call, and without
core:window:allow-destroy the window silently refuses to close. "core:default" does
not include these permissions, so each one must be listed explicitly.
"""
import json, pathlib, re, tempfile, unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent

# frontend call (regex) -> permission that call needs
NEEDS = [
    (re.compile(r"\bonCloseRequested\b"), "core:window:allow-destroy"),
    (re.compile(r"getCurrentWindow\(\)\s*\.\s*destroy\s*\("), "core:window:allow-destroy"),
    (re.compile(r"getCurrentWindow\(\)\s*\.\s*close\s*\("), "core:window:allow-close"),
]

def granted(root):
    perms = set()
    for cap in pathlib.Path(root, "src-tauri", "capabilities").glob("*.json"):
        for p in json.loads(cap.read_text(encoding="utf-8")).get("permissions", []):
            perms.add(p if isinstance(p, str) else p.get("identifier"))
    return perms

def missing(root):
    """Permissions the frontend code needs but no capability file grants."""
    have = granted(root)
    need = {}
    for f in sorted(pathlib.Path(root, "src").rglob("*.ts*")):
        if re.search(r"\.(test|spec)\.tsx?$", f.name):
            continue
        text = f.read_text(encoding="utf-8")
        for pattern, perm in NEEDS:
            if pattern.search(text):
                need.setdefault(perm, f.relative_to(root).as_posix())
    return {perm: src for perm, src in need.items() if perm not in have}

class Capabilities(unittest.TestCase):
    def test_frontend_window_calls_are_granted(self):
        self.assertEqual(missing(ROOT), {}, "add the permission to src-tauri/capabilities/default.json")

    def _tree(self, t, permissions):
        r = pathlib.Path(t)
        (r / "src").mkdir(); (r / "src-tauri/capabilities").mkdir(parents=True)
        (r / "src/a.ts").write_text("win.onCloseRequested(async () => {});")
        (r / "src-tauri/capabilities/default.json").write_text(json.dumps({"permissions": permissions}))
        return r

    def test_close_handler_without_destroy_permission_is_reported(self):
        with tempfile.TemporaryDirectory() as t:
            r = self._tree(t, ["core:default"])
            self.assertEqual(missing(r), {"core:window:allow-destroy": "src/a.ts"})

    def test_granted_permission_passes(self):
        with tempfile.TemporaryDirectory() as t:
            r = self._tree(t, ["core:default", {"identifier": "core:window:allow-destroy"}])
            self.assertEqual(missing(r), {})

    def test_test_files_are_ignored(self):
        with tempfile.TemporaryDirectory() as t:
            r = self._tree(t, ["core:default"])
            (r / "src/a.ts").rename(r / "src/a.test.ts")
            self.assertEqual(missing(r), {})

if __name__ == "__main__":
    unittest.main()
