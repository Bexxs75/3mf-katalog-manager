"""Minimal Chrome DevTools Protocol client plus an own headless Chromium instance.

The instance gets its own debug port and profile directory, so a browser the
developer already runs (for example on port 9333) is never touched.
"""
from __future__ import annotations

import asyncio
import glob
import json
import os
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path

import websockets


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def find_chromium() -> str:
    """SCREENS_CHROMIUM, then the system browser, then a Playwright download."""
    env = os.environ.get("SCREENS_CHROMIUM")
    if env:
        return env
    for name in ("chromium", "chromium-browser", "google-chrome", "google-chrome-stable", "chrome"):
        path = shutil.which(name)
        if path:
            return path
    hits = sorted(glob.glob(os.path.expanduser("~/.cache/ms-playwright/chromium-*/chrome-linux*/chrome")))
    if hits:
        return hits[-1]
    raise RuntimeError("No Chromium found. Install one (apt install chromium) or set SCREENS_CHROMIUM; "
                       "as a fallback: npx playwright install chromium")


class Browser:
    """Headless Chromium on a free debug port with a throw-away profile."""

    def __init__(self, binary: str | None = None):
        self.binary = binary or find_chromium()
        self.port = free_port()
        self.profile = tempfile.mkdtemp(prefix="screens-chromium-")
        self.proc: subprocess.Popen | None = None

    def start(self) -> None:
        args = [
            self.binary, "--headless=new", f"--remote-debugging-port={self.port}", f"--user-data-dir={self.profile}",
            "--no-first-run", "--noerrdialogs", "--disable-gpu-vsync", "--hide-scrollbars", "--force-color-profile=srgb",
            "--font-render-hinting=none", "--disable-dev-shm-usage", "--use-angle=swiftshader-webgl", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist",
            "--disable-background-timer-throttling", "--disable-renderer-backgrounding", "--disable-backgrounding-occluded-windows",
            "about:blank",
        ]
        if os.environ.get("SCREENS_NO_SANDBOX") or (hasattr(os, "geteuid") and os.geteuid() == 0):
            args.insert(1, "--no-sandbox")  # root in a container cannot use the sandbox
        self.proc = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{self.port}/json/version", timeout=1).read()
                return
            except Exception:
                if self.proc.poll() is not None:
                    raise RuntimeError(f"Chromium exited with code {self.proc.returncode}")
                time.sleep(0.2)
        raise RuntimeError("Chromium did not start")

    def stop(self) -> None:
        if self.proc and self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        shutil.rmtree(self.profile, ignore_errors=True)

    def __enter__(self):
        self.start()
        return self

    def __exit__(self, *exc):
        self.stop()

    async def _browser_call(self, method: str, **params):
        """One command on the browser-level connection (target management)."""
        loop = asyncio.get_running_loop()
        version = json.loads(await loop.run_in_executor(None, lambda: urllib.request.urlopen(
            f"http://127.0.0.1:{self.port}/json/version", timeout=10).read()))
        async with websockets.connect(version["webSocketDebuggerUrl"], max_size=2**24) as ws:
            await ws.send(json.dumps({"id": 1, "method": method, "params": params}))
            msg = json.loads(await asyncio.wait_for(ws.recv(), 30))
            if "error" in msg:
                raise RuntimeError(f"{method}: {msg['error']}")
            return msg["result"]

    async def new_page(self) -> "Page":
        """New tab in its own browser context: localStorage and cookies are not shared with the other tabs
        (several scenes run in parallel and set different preferences)."""
        ctx = (await self._browser_call("Target.createBrowserContext"))["browserContextId"]
        target_id = (await self._browser_call("Target.createTarget", url="about:blank", browserContextId=ctx))["targetId"]
        ws = await websockets.connect(f"ws://127.0.0.1:{self.port}/devtools/page/{target_id}", max_size=2**28)
        page = Page(ws, self, target_id)
        page.context_id = ctx
        await page.send("Page.enable")
        await page.send("Runtime.enable")
        return page


class Page:
    def __init__(self, ws, browser: Browser, target_id: str):
        self.ws = ws
        self.browser = browser
        self.target_id = target_id
        self.n = 0
        self.pending: dict[int, asyncio.Future] = {}
        self.console: list[str] = []
        self.reader = asyncio.create_task(self._read())

    async def _read(self):
        try:
            async for raw in self.ws:
                msg = json.loads(raw)
                if "id" in msg and msg["id"] in self.pending:
                    self.pending.pop(msg["id"]).set_result(msg)
                elif msg.get("method") == "Runtime.consoleAPICalled" and msg["params"]["type"] in ("error", "warning"):
                    args = msg["params"]["args"]
                    self.console.append(msg["params"]["type"] + ": " + " ".join(str(a.get("value", a.get("description", ""))) for a in args)[:600])
                elif msg.get("method") == "Runtime.exceptionThrown":
                    self.console.append("EXCEPTION: " + json.dumps(msg["params"]["exceptionDetails"])[:500])
        except websockets.ConnectionClosed:
            pass

    async def send(self, method: str, **params):
        self.n += 1
        fut = asyncio.get_running_loop().create_future()
        self.pending[self.n] = fut
        await self.ws.send(json.dumps({"id": self.n, "method": method, "params": params}))
        msg = await asyncio.wait_for(fut, 60)
        if "error" in msg:
            raise RuntimeError(f"{method}: {msg['error']}")
        return msg.get("result", {})

    async def js(self, expr: str):
        r = await self.send("Runtime.evaluate", expression=expr, awaitPromise=True, returnByValue=True)
        if "exceptionDetails" in r:
            raise RuntimeError(json.dumps(r["exceptionDetails"])[:800])
        return r["result"].get("value")

    async def wait_for(self, expr: str, timeout: float = 20):
        for _ in range(int(timeout * 10)):
            try:
                if await self.js(expr):
                    return True
            except RuntimeError:
                pass
            await asyncio.sleep(0.1)
        raise TimeoutError(expr)

    async def viewport(self, width: int, height: int, dpr: float):
        await self.send("Emulation.setDeviceMetricsOverride", width=width, height=height, deviceScaleFactor=dpr, mobile=False)

    async def shot(self) -> bytes:
        import base64
        r = await self.send("Page.captureScreenshot", format="png", captureBeyondViewport=False)
        return base64.b64decode(r["data"])

    async def click_text(self, text: str, selector: str = "button, a, [role=button], [role=tab], li, div, span", exact: bool = True) -> bool:
        """Click the innermost visible element with exactly this text."""
        return await self.js(f"""
        (() => {{
          const want = {json.dumps(text)};
          const els = [...document.querySelectorAll({json.dumps(selector)})].filter(e => {{
            const t = (e.innerText || '').trim();
            const r = e.getBoundingClientRect();
            return r.width > 0 && r.height > 0 && ({'t === want' if exact else 't.includes(want)'});
          }});
          if (!els.length) return false;
          const el = els.find(e => ![...e.querySelectorAll('*')].some(c => els.includes(c))) || els[0];
          el.scrollIntoView({{block: 'center'}});
          el.click();
          return true;
        }})()""")

    async def mouse(self, x: float, y: float, kind: str = "click"):
        for t in (["mousePressed", "mouseReleased"] if kind == "click" else ["mouseMoved"]):
            await self.send("Input.dispatchMouseEvent", type=t, x=x, y=y, button="left", clickCount=1)

    async def close(self):
        self.reader.cancel()
        try:
            await self.ws.close()
        except Exception:
            pass
        try:
            await self.browser._browser_call("Target.disposeBrowserContext", browserContextId=self.context_id)
        except Exception:
            pass
