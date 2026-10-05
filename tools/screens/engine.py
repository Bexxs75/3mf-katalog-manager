"""What a recipe works with: a Session (opens the app with a fixture) and its Capture."""
from __future__ import annotations

import asyncio
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Awaitable, Callable

import fixtures
from recipes.common import HELPERS
from cdp import Browser, Page

HERE = Path(__file__).parent
BASE_DIR = {"de": "/srv/3D-Katalog", "en": "/srv/3D-Catalog"}
DEFAULT_FIXTURE = {"de": "base-de", "en": "base-en"}

# UI texts the recipes click on, per language. The recipes look elements up by their
# visible text, so a changed label shows up as a failed recipe, not as a wrong picture.
RECIPES: dict[str, Callable[["Session"], Awaitable["Capture"]]] = {}


def recipe(name: str):
    def deco(fn):
        if name in RECIPES:
            raise ValueError(f"duplicate recipe {name}")
        RECIPES[name] = fn
        return fn
    return deco


@dataclass
class Capture:
    """Screenshot (device pixels) plus named rectangles in CSS pixels, for cropping."""
    png: bytes
    scale: float
    rects: dict[str, list[float]] = field(default_factory=dict)
    unknown_commands: list[str] = field(default_factory=list)


@dataclass
class Scene:
    recipe: str
    lang: str
    theme: str
    viewport: tuple[int, int]
    dpr: float
    args: dict

    def key(self) -> str:
        return json.dumps([self.recipe, self.lang, self.theme, list(self.viewport), self.dpr, self.args], sort_keys=True)


class Session:
    def __init__(self, browser: Browser, url: str, scene: Scene, snapshots: dict[str, str]):
        self.browser, self.url, self.scene, self.snapshots = browser, url, scene, snapshots
        self.lang, self.theme, self.args = scene.lang, scene.theme, scene.args
        self.pages: list[Page] = []

    def fixture(self, name: str | None = None) -> dict:
        return fixtures.load(name or DEFAULT_FIXTURE[self.lang])

    def prefs(self, extra: dict | None = None) -> dict:
        return {
            "3mf-katalog-language": self.lang, "3mf-katalog-theme": self.theme, "3mf-katalog-density": "compact",
            "3mf-katalog-display-preference": "thumbnail", "3mf-katalog-base-dir": BASE_DIR[self.lang],
            "3mf-katalog-setup-seen": "1", "3mf-katalog-dnd-tip-dismissed": "true", "3mf-katalog-filament-kind": "filament",
            **(extra or {}),
        }

    async def open(self, fixture: str | dict | None = None, prefs: dict | None = None, viewport: tuple[int, int] | None = None) -> Page:
        """New tab with the mock in place, the app loaded and the catalog shown."""
        fx = fixture if isinstance(fixture, dict) else self.fixture(fixture)
        mock = (HERE / "mock.js").read_text(encoding="utf-8")
        mock = mock.replace("__FIXTURE__", json.dumps(fx, ensure_ascii=False)).replace(
            "__PREFS__", json.dumps(self.prefs(prefs))).replace("__SNAPSHOTS__", "{}")
        p = await self.browser.new_page()
        self.pages.append(p)
        w, h = viewport or self.scene.viewport
        await p.viewport(w, h, self.scene.dpr)
        # the same browser language and time zone on every machine (dates, number formats, first-start language)
        locale = {"de": "de-DE", "en": "en-US"}[self.lang]
        await p.send("Emulation.setLocaleOverride", locale=locale)
        await p.send("Emulation.setTimezoneOverride", timezoneId="Europe/Berlin")
        await p.send("Network.enable")
        await p.send("Network.setUserAgentOverride", userAgent=(await p.js("navigator.userAgent")), acceptLanguage=locale)
        await p.send("Page.addScriptToEvaluateOnNewDocument", source=mock)
        await p.send("Page.navigate", url=self.url)
        await p.wait_for("document.readyState === 'complete' && document.body.innerText.length > 50")
        await asyncio.sleep(1.5)
        await p.js(HELPERS)
        return p

    async def capture(self, p: Page, rects: dict[str, str] | None = None, settle: float = 0.0) -> Capture:
        """Screenshot of the current state; `rects` maps names to JS expressions that return an element
        (or an object with getBoundingClientRect, or a list of those, which is united)."""
        if settle:
            await asyncio.sleep(settle)
        out: dict[str, list[float]] = {}
        for name, expr in (rects or {}).items():
            r = await p.js(f"""(() => {{ const v = ({expr}); const es = (Array.isArray(v) ? v : [v]).filter(Boolean);
              if (!es.length) return null; const rs = es.map(e => e.getBoundingClientRect());
              const x = Math.min(...rs.map(r => r.x)), y = Math.min(...rs.map(r => r.y));
              return [x, y, Math.max(...rs.map(r => r.right)) - x, Math.max(...rs.map(r => r.bottom)) - y]; }})()""")
            if r is None:
                raise RuntimeError(f"rectangle '{name}' not found")
            out[name] = r
        png = await p.shot()
        scale = await p.js("window.devicePixelRatio")
        unknown = await p.js("[...new Set(window.__demoUnknown || [])]")
        return Capture(png, scale, out, unknown)

    async def close(self):
        for p in self.pages:
            await p.close()
