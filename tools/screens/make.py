#!/usr/bin/env python3
"""Screenshots for the documentation: render, compare with the checked-in pictures, update.

    python tools/screens/make.py --list                      # all manifest entries and their state
    python tools/screens/make.py --check                     # render to a temp folder, list outdated pictures, HTML report
    python tools/screens/make.py --check --only 'hb-*' --lang de
    python tools/screens/make.py --update --only 'wiki-*'    # overwrite the pictures at their target paths
    python tools/screens/make.py --coverage                  # pictures without a manifest entry, entries without a recipe

Roots of the manifest: app = this repository, website = the website repository (--website-root or
SCREENS_WEBSITE), wiki = <website>/wiki. Entries whose root is missing are skipped and reported.
Needs: Python 3.10+, Pillow, numpy, websockets (tools/screens/requirements.txt), Node with the repo's
node_modules (npm ci) and a Chromium (system package, SCREENS_CHROMIUM or `npx playwright install chromium`).
"""
from __future__ import annotations

import argparse
import asyncio
import fnmatch
import html
import json
import os
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import images  # noqa: E402
import render  # noqa: E402
from cdp import Browser  # noqa: E402
from server import REPO, Preview  # noqa: E402

MANIFEST = HERE / "manifest.json"
LANGS = ("de", "en")


def load_manifest() -> list[dict]:
    return json.loads(MANIFEST.read_text(encoding="utf-8"))["images"]


def roots(website: str | None) -> dict[str, Path | None]:
    web = Path(website or os.environ.get("SCREENS_WEBSITE") or REPO.parent / "3mf-webseite-wt-0.16")
    web = web if web.is_dir() else None
    return {"app": REPO, "website": web, "wiki": web / "wiki" if web else None}


def target(entry: dict, lang: str, rts: dict) -> Path | None:
    root = rts[entry["root"]]
    return root / entry["paths"][lang] if root else None


def select(manifest: list[dict], only: list[str], langs: list[str]) -> list[tuple[dict, str]]:
    out = []
    for e in manifest:
        if only and not any(fnmatch.fnmatch(e["id"], pat) or e["id"] == pat for pat in only):
            continue
        for lang in LANGS:
            if lang in e["paths"] and lang in langs:
                out.append((e, lang))
    return out


def list_cmd(manifest, rts) -> None:
    for e in manifest:
        for lang in LANGS:
            if lang not in e["paths"]:
                continue
            t = target(e, lang, rts)
            state = "root missing" if t is None else ("ok" if t.exists() else "FILE MISSING")
            has = "recipe" if e["recipe"] in _recipes() else "NO RECIPE"
            print(f"{e['id']:44} {lang}  {e['root']:8} {e['paths'][lang]:70} {state:12} {has}")
    print(f"\n{sum(len([l for l in LANGS if l in e['paths']]) for e in manifest)} pictures, {len(manifest)} entries")


def _recipes() -> dict:
    import engine
    render.load_recipes()
    return engine.RECIPES


def coverage(manifest, rts) -> int:
    """Pictures on disk without a manifest entry and entries without recipe or file."""
    rec = _recipes()
    known = {(e["root"], e["paths"][l]) for e in manifest for l in LANGS if l in e["paths"]}
    dirs = {"app": ["docs/benutzerhandbuch/bilder", "docs/tests/bilder", "docs/assets"],
            "website": ["bilder"], "wiki": ["src/assets"]}
    problems = 0
    for root, subs in dirs.items():
        base = rts[root]
        if not base:
            print(f"[{root}] root not available, skipped")
            continue
        for sub in subs:
            for f in sorted((base / sub).rglob("*")):
                if f.is_file() and f.suffix.lower() in (".png", ".webp", ".gif", ".jpg", ".jpeg"):
                    rel = f.relative_to(base).as_posix()
                    if (root, rel) not in known and (root, rel) not in {(r, p) for r, p in _excluded()}:
                        print(f"no manifest entry: [{root}] {rel}")
                        problems += 1
    for e in manifest:
        if e["recipe"] not in rec:
            print(f"no recipe: {e['id']} -> {e['recipe']}")
            problems += 1
    return problems


def _excluded() -> list[tuple[str, str]]:
    """Pictures that are deliberately not generated (with the reason in the manifest's "excluded" list)."""
    data = json.loads(MANIFEST.read_text(encoding="utf-8"))
    out = []
    for item in data.get("excluded", []):
        out.append((item["root"], item["path"]))
    return out


def run(manifest, rts, sel, out_dir: Path, jobs: int, dist: str | None, log=lambda m: print(m, flush=True)) -> dict:
    """Render the selected entries; returns {(id, lang): (image, notes) or error text}."""
    render.load_recipes()
    scenes: dict[str, render.Scene] = {}
    for e, lang in sel:
        sc = render.scene_of(e, lang)
        scenes[sc.key()] = sc
    log(f"{len(sel)} pictures from {len(scenes)} scenes")
    with Preview(Path(dist) if dist else None, rebuild=not dist) as pv, Browser() as browser:
        caps = asyncio.run(render.run_all(browser, pv.url, list(scenes.values()), jobs, log))
    results = {}
    for e, lang in sel:
        cap = caps[render.scene_of(e, lang).key()]
        if isinstance(cap, Exception):
            results[(e["id"], lang)] = f"scene failed: {cap!r}"[:300]
            continue
        try:
            im, notes = images.render(e, lang, cap)
        except Exception as ex:
            results[(e["id"], lang)] = f"post-processing failed: {ex!r}"[:300]
            continue
        results[(e["id"], lang)] = (im, notes, cap.unknown_commands)
    return results


def write_report(rows: list[dict], out_dir: Path, threshold: float) -> Path:
    cards = []
    for r in sorted(rows, key=lambda r: (r["score"] is None, r["score"] if r["score"] is not None else 2)):
        sc = "-" if r["score"] is None else f"{r['score']:.3f}"
        cls = "bad" if r["status"] != "ok" else "ok"
        cards.append(f"""<section class="{cls}"><h3>{html.escape(r['id'])} <small>{r['lang']}</small> &middot; {sc} &middot; {html.escape(r['status'])}</h3>
<p>{html.escape(r['path'])} {html.escape(r.get('note', ''))}</p><div class="pair">
{'<figure><figcaption>checked in</figcaption><img loading="lazy" src="old/%s"></figure>' % r['old'] if r.get('old') else ''}
{'<figure><figcaption>new</figcaption><img loading="lazy" src="new/%s"></figure>' % r['new'] if r.get('new') else ''}</div></section>""")
    n_bad = sum(1 for r in rows if r["status"] != "ok")
    doc = f"""<!doctype html><meta charset="utf-8"><title>Screenshot report</title>
<style>body{{font:14px system-ui;background:#161616;color:#ddd;margin:20px}}section{{border:1px solid #333;margin:0 0 14px;padding:8px 12px;border-radius:8px}}
section.bad{{border-color:#c0392b}}h3{{margin:.2em 0}}small{{color:#999}}.pair{{display:flex;gap:12px;flex-wrap:wrap;align-items:flex-start}}
figure{{margin:0;max-width:48%}}img{{max-width:100%;border:1px solid #444;background:#fff}}p{{color:#999;margin:.2em 0}}</style>
<h1>Screenshot report</h1><p>{len(rows)} pictures, {n_bad} outdated or faulty (threshold {threshold}). Worst first.</p>{''.join(cards)}"""
    path = out_dir / "report.html"
    path.write_text(doc, encoding="utf-8")
    return path


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--list", action="store_true")
    mode.add_argument("--check", action="store_true", help="render, compare, write the HTML report (exit code 0 even when pictures are outdated; 2 when scenes failed)")
    mode.add_argument("--update", action="store_true", help="render and overwrite the pictures at their target paths")
    mode.add_argument("--coverage", action="store_true", help="pictures without entry / entries without recipe")
    ap.add_argument("--only", action="append", default=[], help="entry id or pattern (fnmatch), repeatable")
    ap.add_argument("--lang", default="de,en")
    ap.add_argument("--threshold", type=float, default=0.97, help="SSIM below this counts as outdated (default 0.97)")
    ap.add_argument("--out", help="folder for rendered pictures and the report (default: a temp folder)")
    ap.add_argument("--website-root", help="checkout of the website repository")
    ap.add_argument("--jobs", type=int, default=3, help="scenes in parallel")
    ap.add_argument("--dist", help="use an existing frontend build instead of running `vite build` (quick iterations)")
    ap.add_argument("--only-outdated", action="store_true", help="with --update/--export: only pictures that --check found outdated")
    ap.add_argument("--export", help="with --check: also write the new pictures to <folder>/<root>/<target path> (for a CI artifact; nothing in the repo changes)")
    a = ap.parse_args()
    manifest, rts = load_manifest(), roots(a.website_root)
    langs = [x for x in a.lang.split(",") if x]
    if a.list or not (a.check or a.update or a.coverage):
        list_cmd(manifest, rts)
        return 0
    if a.coverage:
        n = coverage(manifest, rts)
        print(f"{n} problems")
        return 1 if n else 0
    sel = select(manifest, a.only, langs)
    skipped = [(e["id"], l) for e, l in sel if target(e, l, rts) is None]
    sel = [(e, l) for e, l in sel if target(e, l, rts) is not None]
    if skipped:
        print(f"skipped (root not available): {len(skipped)}")
    out = Path(a.out) if a.out else Path(tempfile.mkdtemp(prefix="screens-out-"))
    for sub in ("new", "old"):
        (out / sub).mkdir(parents=True, exist_ok=True)
    t0 = time.time()
    results = run(manifest, rts, sel, out, a.jobs, a.dist)
    rows, failed = [], 0
    for e, lang in sel:
        res = results[(e["id"], lang)]
        tgt = target(e, lang, rts)
        row = {"id": e["id"], "lang": lang, "path": f"[{e['root']}] {e['paths'][lang]}", "score": None}
        if isinstance(res, str):
            row.update(status="FAILED", note=res)
            failed += 1
            rows.append(row)
            continue
        im, notes, unknown = res
        fmt = e.get("format") or tgt.suffix.lstrip(".").lower()
        data = images.encode(im, fmt, e.get("quality", 80))
        name = f"{e['id']}.{lang}.png"
        im.save(out / "new" / name)
        newfile = out / f"{e['id']}.{lang}.{fmt}"
        newfile.write_bytes(data)
        row["new"] = name
        row["note"] = "; ".join(notes + ([f"unmocked commands: {unknown}"] if unknown and False else []))
        if tgt.exists():
            from PIL import Image
            old = Image.open(tgt)
            old.convert("RGB").save(out / "old" / name)
            row["old"] = name
            row["score"] = images.ssim(old.convert("RGB"), im)
            row["status"] = "ok" if row["score"] >= a.threshold and old.size == im.size else "OUTDATED"
            if old.size != im.size:
                row["note"] += f" size {old.size[0]}x{old.size[1]} -> {im.size[0]}x{im.size[1]}"
        else:
            row["status"] = "NEW (no file yet)"
        row["bytes"] = data
        row["fmt"] = fmt
        rows.append(row)
    report = write_report([{k: v for k, v in r.items() if k not in ("bytes",)} for r in rows], out, a.threshold)
    stale = [r for r in rows if r["status"] != "ok"]
    print(f"\n{len(rows)} pictures in {time.time() - t0:.0f} s, {len(stale)} not ok, {failed} failed")
    for r in sorted(stale, key=lambda r: (r["score"] is None, r["score"] or 0)):
        sc = "  -  " if r["score"] is None else f"{r['score']:.3f}"
        print(f"  {sc}  {r['status']:10} {r['id']} {r['lang']}  {r.get('note', '')}")
    print(f"report: {report}")
    (out / "stale.json").write_text(json.dumps([{k: r[k] for k in ("id", "lang", "score", "status")} for r in stale], indent=1))
    if a.export:
        n = 0
        for r in rows:
            if "bytes" in r and not (a.only_outdated and r["status"] == "ok"):
                e = next(x for x in manifest if x["id"] == r["id"])
                dst = Path(a.export) / e["root"] / e["paths"][r["lang"]]
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_bytes(r["bytes"])
                n += 1
        print(f"{n} new pictures exported to {a.export}")
    if a.update:
        n = 0
        for r in rows:
            if "bytes" not in r or (a.only_outdated and r["status"] == "ok"):
                continue
            e = next(x for x in manifest if x["id"] == r["id"])
            t = target(e, r["lang"], rts)
            t.parent.mkdir(parents=True, exist_ok=True)
            t.write_bytes(r["bytes"])
            n += 1
        print(f"{n} pictures written")
    return 2 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
