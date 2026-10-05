"""Frontend build and a preview server on a free port."""
from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path

from cdp import free_port

REPO = Path(__file__).resolve().parents[2]


def npx() -> str:
    return "npx.cmd" if os.name == "nt" else "npx"


def build(out_dir: Path) -> None:
    """Production build of the app into out_dir (the repo's own dist/ stays untouched)."""
    r = subprocess.run([npx(), "vite", "build", "--outDir", str(out_dir), "--emptyOutDir"], cwd=REPO,
                       capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError("vite build failed:\n" + r.stdout[-2000:] + r.stderr[-2000:])


class Preview:
    """`vite preview` for a finished build; use as a context manager."""

    def __init__(self, dist: Path | None = None, rebuild: bool = True):
        self.tmp: str | None = None
        if dist is None:
            self.tmp = tempfile.mkdtemp(prefix="screens-dist-")
            dist = Path(self.tmp)
        self.dist = dist
        self.rebuild = rebuild
        self.port = free_port()  # never a fixed port: a dev server of the developer may already use 1420
        self.proc: subprocess.Popen | None = None

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.port}/"

    def __enter__(self):
        if self.rebuild:
            print("vite build ...", flush=True)
            build(self.dist)
        self.proc = subprocess.Popen([npx(), "vite", "preview", "--outDir", str(self.dist), "--port", str(self.port), "--strictPort",
                                      "--host", "127.0.0.1"], cwd=REPO, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                     start_new_session=True)
        for _ in range(150):
            try:
                urllib.request.urlopen(self.url, timeout=1).read()
                return self
            except Exception:
                if self.proc.poll() is not None:
                    raise RuntimeError("vite preview exited")
                time.sleep(0.2)
        raise RuntimeError("vite preview did not start")

    def __exit__(self, *exc):
        if self.proc and self.proc.poll() is None:
            import signal
            try:
                os.killpg(self.proc.pid, signal.SIGTERM)  # npx starts node as a child process
            except (ProcessLookupError, AttributeError, PermissionError):
                self.proc.terminate()
            try:
                self.proc.wait(10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        if self.tmp:
            shutil.rmtree(self.tmp, ignore_errors=True)

