"""Runs the scenes of the selected manifest entries in headless Chromium."""
from __future__ import annotations

import asyncio
import importlib
import sys

import engine
from cdp import Browser
from engine import Capture, Scene, Session

RECIPE_MODULES = ["recipes.testkatalog", "recipes.handbook"]


def load_recipes() -> None:
    for m in RECIPE_MODULES:
        importlib.import_module(m)


def scene_of(entry: dict, lang: str) -> Scene:
    return Scene(entry["recipe"], lang, entry.get("theme", "dark"), tuple(entry.get("viewport", [1920, 1080])),
                 entry.get("dpr", 1.5), entry.get("args", {}))


async def run_scene(browser: Browser, url: str, scene: Scene, retries: int = 2) -> Capture | Exception:
    fn = engine.RECIPES.get(scene.recipe)
    if fn is None:
        return LookupError(f"recipe '{scene.recipe}' does not exist")
    err: Exception | None = None
    for attempt in range(retries + 1):
        s = Session(browser, url, scene, {})
        try:
            return await asyncio.wait_for(fn(s), 120)
        except Exception as e:  # a failed recipe must not stop the others
            err = e
            await asyncio.sleep(1)
        finally:
            await s.close()
    return err  # type: ignore[return-value]


async def run_all(browser: Browser, url: str, scenes: list[Scene], jobs: int = 3, log=print) -> dict[str, Capture | Exception]:
    sem = asyncio.Semaphore(jobs)
    out: dict[str, Capture | Exception] = {}
    done = 0

    async def one(sc: Scene):
        nonlocal done
        async with sem:
            r = await run_scene(browser, url, sc)
        out[sc.key()] = r
        done += 1
        state = "FAILED: " + repr(r)[:200] if isinstance(r, Exception) else "ok"
        log(f"[{done}/{len(scenes)}] {sc.recipe} {sc.lang} {sc.theme} {state}")

    await asyncio.gather(*(one(sc) for sc in scenes))
    return out
