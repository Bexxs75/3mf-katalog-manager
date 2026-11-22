"""The frontend may only call window commands the main window's capability grants.

A missing permission is invisible to unit tests and only fails in the packaged app:
the close handler (onCloseRequested) ends the window with a destroy call, and without
core:window:allow-destroy the window silently refuses to close. "core:default" does
not include these permissions, so each one must be listed explicitly.
"""
import json, pathlib, re, tempfile, unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent

# Window methods that need core:window:allow-<kebab-name> ("core:default" only grants getters).
METHODS = ["destroy", "close", "minimize", "unminimize", "maximize", "unmaximize", "toggleMaximize",
           "hide", "show", "setTitle", "setFullscreen", "setSize", "setPosition", "setFocus",
           "setAlwaysOnTop", "startDragging", "center", "requestUserAttention"]
# Ways to get a window handle. Calls are matched on the handle itself or on a variable
# assigned from it (const win = getCurrentWindow(); win.destroy()).
HANDLE = r"(?:getCurrentWindow|getCurrentWebviewWindow|Window\.getCurrent|WebviewWindow\.getCurrent|new\s+(?:Webview)?Window)\s*\([^)]*\)"
ALIAS = re.compile(r"(?:const|let|var)\s+(\w+)\s*=\s*(?:await\s+)?" + HANDLE)

def kebab(name):
    return "core:window:allow-" + re.sub(r"([A-Z])", lambda m: "-" + m.group(1).lower(), name)

# frontend call (regex) -> permission that call needs
# shortcut: only window methods are covered. Plugin commands (dialog, fs, ...), webview.*
# and event.emit/listen are not checked; those are caught by the packaged-app test.
NEEDS = [(re.compile(r"\bonCloseRequested\b"), "core:window:allow-destroy")]

def needs(text):
    """(pattern-found, permission) pairs for one source file."""
    found = [perm for pattern, perm in NEEDS if pattern.search(text)]
    receivers = [HANDLE] + [re.escape(a) for a in ALIAS.findall(text)]
    for m in METHODS:
        if re.search(r"(?:" + "|".join(receivers) + r")\s*\.\s*" + m + r"\s*\(", text):
            found.append(kebab(m))
    return found

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
        for perm in needs(text):
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

    def test_alias_and_other_window_getters_are_found(self):
        cases = {
            "const win = getCurrentWindow(); win.destroy();": "core:window:allow-destroy",
            "const w = await getCurrentWebviewWindow(); await w.setTitle('x');": "core:window:allow-set-title",
            "Window.getCurrent().minimize()": "core:window:allow-minimize",
            "const w = new Window('x'); w.hide()": "core:window:allow-hide",
            "getCurrentWindow().toggleMaximize()": "core:window:allow-toggle-maximize",
        }
        for src, perm in cases.items():
            with self.subTest(src=src):
                self.assertEqual(needs(src), [perm])

    def test_unrelated_close_is_not_a_window_call(self):
        self.assertEqual(needs("const events = new EventSource(u); events.close();"), [])

    def test_test_files_are_ignored(self):
        with tempfile.TemporaryDirectory() as t:
            r = self._tree(t, ["core:default"])
            (r / "src/a.ts").rename(r / "src/a.test.ts")
            self.assertEqual(missing(r), {})

if __name__ == "__main__":
    unittest.main()
