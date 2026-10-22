#!/usr/bin/env python3
"""Builds docs/assets/demo.gif (README animation) from single screenshots of the demo catalog.

    python tools/screens/make_demo_gif.py [--dist DIR] [--out docs/assets/demo.gif] [--keep-frames DIR]

The English interface in the dark theme, taken with the same mock and fixtures as the manual pictures
(pm-en with the disk-sync answer). Story: search the catalog, open a model with its slicer data, Material
Manager with AMS slots, Printer Manager, sync with the disk. Every step adds screenshots with a display time;
the frames share one optimized palette. Needs the same things as make.py (see there).
"""
from __future__ import annotations

import argparse
import asyncio
import io
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import engine  # noqa: E402
import fixtures  # noqa: E402
import render  # noqa: E402
from cdp import Browser  # noqa: E402
from server import REPO, Preview  # noqa: E402
from PIL import Image  # noqa: E402

SIZE = (1100, 619)
VIEWPORT = (1280, 720)


class Recorder:
    def __init__(self, page):
        self.page, self.frames = page, []

    async def snap(self, ms: int = 120, n: int = 1):
        png = await self.page.shot()
        im = Image.open(io.BytesIO(png)).convert("RGB").resize(SIZE, Image.LANCZOS)
        for _ in range(n):
            self.frames.append((im, ms))

    async def wait(self, seconds: float):
        await asyncio.sleep(seconds)

    async def burst(self, seconds: float, ms: int = 100, gap: float = 0.12):
        """Frames while something changes (a page opens, the 3D view loads)."""
        end = asyncio.get_event_loop().time() + seconds
        while asyncio.get_event_loop().time() < end:
            await self.snap(ms)
            await asyncio.sleep(gap)


async def script(rec: Recorder, p) -> None:
    js = p.js
    await rec.snap(900)                                  # catalog grid
    await rec.snap(120, 3)
    # search "rocket", one letter at a time
    await js("document.querySelector('input[placeholder^=\"Search\"]').focus()")
    for ch in "rocket":
        await p.send("Input.insertText", text=ch)
        await rec.wait(0.25)
        await rec.snap(150)
    await rec.wait(0.8)
    await rec.snap(700)
    # open the model, show the 3D view and the slicer data further down
    await js("""(() => { const el = [...document.querySelectorAll('*')].find(e => e.children.length === 0 && (e.innerText || '').trim() === 'Rocket.3mf' && e.getBoundingClientRect().x > 300);
      el.dispatchEvent(new MouseEvent('dblclick', {bubbles: true, cancelable: true})); })()""")
    await rec.burst(1.2)
    await rec.snap(800)
    await p.click_text("3D view")
    await rec.burst(1.8)
    await rec.snap(120, 2)
    await rec.snap(700)
    # slow scroll down to the filament usage of the slicer
    await js("""(() => { const el = [...document.querySelectorAll('*')].find(e => e.children.length === 0 && (e.innerText || '').trim().startsWith('Estimated material cost'));
      let sc = el.parentElement; while (sc && !(sc.scrollHeight > sc.clientHeight + 5 && getComputedStyle(sc).overflowY !== 'visible')) sc = sc.parentElement;
      window.__sc = sc; window.__target = sc.scrollTop + (el.getBoundingClientRect().bottom - sc.getBoundingClientRect().bottom) + 30; window.__start = sc.scrollTop; })()""")
    steps = 7
    for i in range(1, steps + 1):
        await js(f"window.__sc.scrollTop = window.__start + (window.__target - window.__start) * {i / steps}")
        await rec.wait(0.15)
        await rec.snap(110)
    await rec.snap(900)
    # Material Manager with the AMS slots
    await js("[...document.querySelectorAll('[aria-label],[title]')].find(e => (e.getAttribute('aria-label') || e.getAttribute('title')) === 'Material Manager').click()")
    await rec.burst(0.8)
    await rec.snap(1200)
    await p.click_text("List")
    await rec.wait(1.0)
    await rec.snap(1000)
    # Printer Manager
    await js("[...document.querySelectorAll('[aria-label],[title]')].find(e => (e.getAttribute('aria-label') || e.getAttribute('title')) === 'Printer Manager').click()")
    await rec.burst(0.7)
    await rec.snap(900)
    await js("[...document.querySelectorAll('button')].find(b => (b.innerText || '').includes('Sovol SV08')).click()")
    await rec.burst(0.4)
    await rec.snap(1500)
    # back to the catalog (leave the model page, clear the search), sync with the disk
    await js("document.querySelector('nav button').click()")
    await rec.wait(1.2)
    await js('''(() => { const b = document.querySelector('button[title="Back to catalog"]'); if (b) b.click(); })()''')
    await rec.wait(0.8)
    await js("[...document.querySelectorAll('button')].find(b => (b.textContent || '').trim() === 'Clear all').click()")
    await rec.wait(1.0)
    await js("document.activeElement && document.activeElement.blur()")
    await p.send("Input.dispatchMouseEvent", type="mouseMoved", x=900, y=22)  # no hover/focus highlight on a card
    await rec.burst(0.5)
    await rec.snap(700)
    await js("[...document.querySelectorAll('button')].find(b => (b.textContent || '').trim() === 'Refresh').click()")
    await rec.wait(0.4)
    await rec.burst(0.5)
    await rec.snap(2200)


def encode(frames, out: Path) -> None:
    """One shared palette from a sample of all frames, no dithering (flat UI colors), delta frames."""
    sample = Image.new("RGB", (SIZE[0], SIZE[1] * 4))
    for k, idx in enumerate(range(0, len(frames), max(1, len(frames) // 4))[:4]):
        sample.paste(frames[idx][0], (0, k * SIZE[1]))
    pal = sample.quantize(colors=160, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
    q = [f.quantize(palette=pal, dither=Image.Dither.NONE) for f, _ in frames]
    q[0].save(out, save_all=True, append_images=q[1:], duration=[d for _, d in frames], loop=0, optimize=True, disposal=1)


async def main_async(a) -> None:
    render.load_recipes()
    fx = fixtures.load("pm-en")
    fx["scan_catalog_disk"] = fixtures.load("sync-en")["scan_catalog_disk"]
    with Preview(Path(a.dist) if a.dist else None, rebuild=not a.dist) as pv, Browser() as browser:
        scene = engine.Scene("demo-gif", "en", "dark", VIEWPORT, 1.5, {})
        s = engine.Session(browser, pv.url, scene, {})
        try:
            p = await s.open(fx)
            await p.js("__expand()")
            await asyncio.sleep(0.5)
            await p.send("Input.dispatchMouseEvent", type="mouseMoved", x=1270, y=710)
            rec = Recorder(p)
            await script(rec, p)
        finally:
            await s.close()
    if a.keep_frames:
        d = Path(a.keep_frames); d.mkdir(parents=True, exist_ok=True)
        for i, (im, ms) in enumerate(rec.frames):
            im.save(d / f"{i:03d}_{ms}.png")
    out = Path(a.out)
    encode(rec.frames, out)
    total = sum(ms for _, ms in rec.frames)
    print(f"{len(rec.frames)} frames, {total / 1000:.1f} s, {out.stat().st_size / 1e6:.2f} MB -> {out}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dist", help="existing frontend build (default: vite build)")
    ap.add_argument("--out", default=str(REPO / "docs" / "assets" / "demo.gif"))
    ap.add_argument("--keep-frames", help="also write the single frames here (for checking)")
    asyncio.run(main_async(ap.parse_args()))


if __name__ == "__main__":
    main()
