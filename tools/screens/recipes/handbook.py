"""Pictures for the user manual, the wiki, the README and the website (the demo catalog without markers)."""
from __future__ import annotations

import asyncio
import json

from engine import Session, recipe
from recipes.common import C, js, union_expr

TX = {
    "de": dict(haushalt="Haushalt", collection="Weihnachtsmarkt-Projekt", rocket="Rakete.3mf", select=["Rakete.3mf", "Kreisel.3mf", "Spiralvase.3mf"],
               view3d="3D-Ansicht", cost="Geschätzte Materialkosten", printers="DRUCKER", add_spool="Spule anlegen", resin="Resin",
               nav_filament="Material Manager", nav_settings="Einstellungen", nav_trash="Papierkorb", general="Allgemein", slicer="Slicer",
               catalog="Katalog", info="Info", settings="EINSTELLUNGEN", import_="Importieren", files="Dateien...", archive="Archive entpacken", extract="Entpacken",
               list_btn="Liste", favorites="Favoriten", queue="Warteschlange", queue_last="Wabenregal.stl", review="Prüfen", new_prints="Neue Drucke", report="Fehler melden",
               log_yes="Ja, Logdatei anhängen", update_now="Jetzt aktualisieren", reset="Katalog zurücksetzen …", tags="TAGS",
               remove_folder_item="entfernen"),
    "en": dict(haushalt="Household", collection="Christmas Market Project", rocket="Rocket.3mf", select=["Rocket.3mf", "Spinning Top.3mf", "Spiral Vase.3mf"],
               view3d="3D view", cost="Estimated material cost", printers="PRINTERS", add_spool="Add spool", resin="Resin",
               nav_filament="Material Manager", nav_settings="Settings", nav_trash="Trash", general="General", slicer="Slicer",
               catalog="Catalog", info="Info", settings="SETTINGS", import_="Import", files="Files...", archive="Extract archives", extract="Extract", new_prints="New prints",
               list_btn="List", favorites="Favorites", queue="Print Queue", queue_last="Honeycomb Shelf.stl", review="Review", report="Report a bug",
               log_yes="Yes, attach the log file", update_now="Update now", reset="Reset catalog …", tags="TAGS",
               remove_folder_item="remove"),
}
VERSION, NEXT = "0.15.0", "0.15.1"
DOCS = {"de": "/home/tester/Dokumente", "en": "/home/tester/Documents"}


def tx(s: Session) -> dict:
    return TX[s.lang]


async def expand_tree(p):
    await p.js("__expand()")
    await asyncio.sleep(0.5)


async def to_catalog(s: Session, p, tags: str = "closed"):
    await expand_tree(p)
    if tags == "open":
        await js(p, "[...document.querySelectorAll('*')].find(e => (e.innerText || '').trim().toUpperCase() === 'TAGS' && e.getBoundingClientRect().x < 320).click()", 0.8)


async def view_button(p, index: int):
    """The grid / folder / list switch in the header (language independent: by position)."""
    await p.wait_for("[...document.querySelectorAll('button')].filter(b => b.getBoundingClientRect().y < 40 && b.getBoundingClientRect().x > 380 && b.getBoundingClientRect().x < 720 && !b.querySelector('svg')).length >= 3", 10)
    ok = await p.js(f"""(() => {{ const bs = [...document.querySelectorAll('button')].filter(b => b.getBoundingClientRect().y < 40 && b.getBoundingClientRect().x > 380 && b.getBoundingClientRect().x < 720 && !b.querySelector('svg'))
      .sort((a, b) => a.getBoundingClientRect().x - b.getBoundingClientRect().x); if (bs.length < 3) return false; bs[{index}].click(); return true; }})()""")
    assert ok, "view switch not found"
    await asyncio.sleep(2.5)


async def click_card(p, name: str, dbl: bool = False):
    ok = await p.js(f"""(() => {{ const el = __card({C(name)}); if (!el) return false; el.scrollIntoView({{block: 'center'}});
      el.click(); {"el.dispatchEvent(new MouseEvent('dblclick', {bubbles: true, cancelable: true}));" if dbl else ""} return true; }})()""")
    assert ok, f"card {name} not found"


async def sidebar(p, text: str):
    ok = await p.js(f"""(() => {{ const el = __leaf({C(text)}, e => e.getBoundingClientRect().x < 320); if (!el) return false; el.click(); return true; }})()""")
    assert ok, f"sidebar entry {text} not found"
    await asyncio.sleep(1.5)


@recipe("hb-first-start")
async def first_start(s):
    p = await s.open(prefs={"3mf-katalog-setup-seen": "", "3mf-katalog-base-dir": ""})
    await asyncio.sleep(2)
    return await s.capture(p)


@recipe("hb-catalog")
async def catalog(s):
    """Overview of the catalog. args: tags open|closed, view grid|folder|list, folder, select, density."""
    a, t = s.args, tx(s)
    p = await s.open(prefs={"3mf-katalog-density": a.get("density", "compact")} if a.get("density") else None)
    await to_catalog(s, p, a.get("tags", "closed"))
    if a.get("folder"):
        await sidebar(p, t[a["folder"]])
    if a.get("view") in ("folder", "list"):
        await view_button(p, {"grid": 0, "folder": 1, "list": 2}[a["view"]])
    if a.get("select"):
        await click_card(p, t["rocket"])
        await asyncio.sleep(4)
    # the queue block of the sidebar: from its heading to the last entry, full sidebar width
    rects = {"queue": f"""(() => {{ const h = __leaf({C(t['queue'])}, e => e.getBoundingClientRect().x < 320), l = __leaf({C(t['queue_last'])}, e => e.getBoundingClientRect().x < 320);
      if (!h || !l) return null; const a = h.getBoundingClientRect(), b = l.getBoundingClientRect(); const y = a.y - 6, bt = b.bottom + 6;
      return {{ getBoundingClientRect: () => ({{ x: 68, y, width: 238, height: bt - y, right: 306, bottom: bt }}) }}; }})()"""}
    return await s.capture(p, rects)


@recipe("hb-context-menu")
async def context_menu(s):
    t = tx(s)
    p = await s.open()
    await to_catalog(s, p)
    await js(p, f"""(() => {{ const el = __card({C(t['rocket'])}); const r = el.getBoundingClientRect(); const x = r.x + r.width / 2, y = r.y + 70;
      document.elementFromPoint(x, y).dispatchEvent(new MouseEvent('contextmenu', {{bubbles: true, cancelable: true, clientX: x, clientY: y}})); }})()""", 1.2)
    return await s.capture(p)


@recipe("hb-multiselect")
async def multiselect(s):
    t = tx(s)
    p = await s.open()
    await to_catalog(s, p)
    for name in t["select"]:
        ok = await p.js(f"""(() => {{ const el = __card({C(name)}); const card = el && el.closest('[data-model-id], [class*=rounded]');
          const box = card && (card.querySelector('input[type=checkbox]') || card.querySelector('[role=checkbox]')); if (!box) return false; box.click(); return true; }})()""")
        assert ok, name
        await asyncio.sleep(0.4)
    await asyncio.sleep(1)
    return await s.capture(p)


@recipe("hb-collection")
async def collection(s):
    t = tx(s)
    p = await s.open()
    await to_catalog(s, p, s.args.get("tags", "closed"))
    await sidebar(p, t["collection"])
    return await s.capture(p)


@recipe("hb-detail")
async def detail(s):
    t = tx(s)
    p = await s.open()
    await to_catalog(s, p)
    await click_card(p, t["rocket"], dbl=True)
    await asyncio.sleep(3)
    assert await p.click_text(t["view3d"])
    await asyncio.sleep(4)
    await p.js(f"""(() => {{ const el = [...document.querySelectorAll('*')].find(e => e.children.length === 0 && (e.innerText || '').trim().startsWith({C(t['cost'])}));
      if (!el) return; let sc = el.parentElement; while (sc && !(sc.scrollHeight > sc.clientHeight + 5 && getComputedStyle(sc).overflowY !== 'visible')) sc = sc.parentElement;
      if (!sc) return; const card = el.closest('section, [class*=rounded]') || el; const r = card.getBoundingClientRect(), q = sc.getBoundingClientRect();
      sc.scrollTop += (r.bottom - q.bottom) + 24; }})()""")
    await asyncio.sleep(1.5)
    return await s.capture(p)


@recipe("hb-tools")
async def tools(s):
    t = tx(s)
    p = await s.open()
    await to_catalog(s, p)
    ok = await p.js(f"""(() => {{ const b = [...document.querySelectorAll('button')].find(e => (e.innerText || '').includes({C(t['favorites'])}) && e.getBoundingClientRect().x < 320); if (!b) return false; b.click(); return true; }})()""")
    assert ok, "favorites"
    await asyncio.sleep(2.5)
    return await s.capture(p)


def printers_panel(t) -> str:
    """Right column "Printers" of the material manager, as far as the printer list reaches."""
    return f"""(() => {{ let e = __leaf({C(t['printers'])}); if (!e) return null; while (e.parentElement && e.parentElement.getBoundingClientRect().width < 500) e = e.parentElement; return e; }})()"""


@recipe("hb-material")
async def material(s):
    """args: view dashboard|list|form, kind filament|resin, link (fixture with printer connection)."""
    a, t = s.args, tx(s)
    p = await s.open(f"link-{s.lang}" if a.get("link") else None, {"3mf-katalog-filament-kind": a.get("kind", "filament")})
    await js(p, f"__lbl({C(t['nav_filament'])}).click()", 2.0)
    if a.get("view") in ("list", "form"):
        assert await p.click_text(t["list_btn"])
        await asyncio.sleep(1.5)
    rects = {"printers": printers_panel(t)}
    if a.get("view") == "form":
        assert await p.click_text(t["add_spool"], "button")
        await asyncio.sleep(1.5)
    return await s.capture(p, rects)


@recipe("hb-trash")
async def trash(s):
    t = tx(s)
    p = await s.open(f"trash-{s.lang}")
    await js(p, f"__lbl({C(t['nav_trash'])}).click()", 2.0)
    return await s.capture(p)


@recipe("hb-archive")
async def archive(s):
    """The dialog "Extract archives" after a drop of two archives (no native file dialog in the browser)."""
    t = tx(s)
    fx = s.fixture()
    paths = [i["path"] for i in fx.get("inspect_archives", [])][:2]
    p = await s.open(fx)
    await to_catalog(s, p)
    await js(p, f"__demoEmitDrop({json.dumps(paths)})", 1.0)
    counts = {"imported": 0, "importedNotPlaced": 0, "duplicate": 0, "skipped": 0, "archive": len(paths), "known": len(paths)}
    res = {"jobId": "demo-job", "source": "dropped", "state": "finished", "parentJobId": None, "jobError": None, "scanComplete": True, "placementRequired": False,
           "groups": {"imported": [], "importedNotPlaced": [], "duplicate": [], "skipped": [], "archive": [{"path": q, "state": "pending"} for q in paths]}, "counts": counts}
    prog = {"jobId": "demo-job", "state": "finished", "scanComplete": True, "total": len(paths), "found": len(paths), "done": len(paths), "inFlight": 0,
            "counts": counts, "current": None, "elapsedMs": 500}
    await p.js(f"window.__demoImportResult = {json.dumps(res)}; window.__demoImportProgress = {json.dumps(prog)}; window.__demoEmit('import://progress', {json.dumps(prog)}); window.__demoEmit('import://finished', {json.dumps(res)});")
    await asyncio.sleep(2.0)
    return await s.capture(p, {"dialog": "__panel(__starts(" + C(t["extract"]) + ", 'button'), 500)"})


@recipe("hb-settings")
async def settings(s):
    """args: tab general|slicer|catalog|info."""
    t = tx(s)
    p = await s.open(v15_fixture(s, "info") if s.args.get("v15") else None)
    await js(p, f"__lbl({C(t['nav_settings'])}).click()", 1.0)
    await p.js(f"window.__GENERAL = {C(t['general'])}")
    await js(p, f"__tab({C(t[s.args['tab']])}).click()", 1.5)
    await p.js("document.activeElement && document.activeElement.blur(); __unclamp()")
    await asyncio.sleep(0.5)
    return await s.capture(p, {"panel": f"__box(__tab({C(t['general'])}))"})


@recipe("hb-link-material")
async def link_material(s):
    """Material manager with the printer connection and the "new prints" hint; args: dialog."""
    t = tx(s)
    p = await s.open(f"link-{s.lang}")
    await js(p, f"__lbl({C(t['nav_filament'])}).click()", 2.5)
    if s.args.get("dialog"):
        assert await p.click_text(t["review"], "button")
        await asyncio.sleep(2)
    rects = {"dialog": f"__panel(__leaf({C(t['new_prints'])}), 700)"} if s.args.get("dialog") else None
    return await s.capture(p, rects)


def v15_fixture(s: Session, name: str, update=None) -> dict:
    """Catalog of the 0.15.0 manual pictures: extra folders, log preview and version data."""
    de = s.lang == "de"
    fx = s.fixture()
    k, b = ("Küche", "Bad") if de else ("Kitchen", "Bathroom")
    parent = next(f for f in fx["list_folders"] if f["id"] == "2")
    fx["list_folders"] += [{"count": 2, "id": "7", "name": k, "parentId": "2", "path": parent["path"] + "/" + k},
                           {"count": 2, "id": "8", "name": b, "parentId": "2", "path": parent["path"] + "/" + b}]
    lines = [
        (f"2026-10-03 10:00:00 INFO  [start] 3MF Katalog Manager {VERSION} (ohne STEP) · Windows 11 · Schema 39 · 15 Modelle\n", None),
        ("2026-10-03 10:00:00 INFO  [start] Katalog: ", None), ("<katalog>", True), ("\n", None),
        ("2026-10-03 10:02:11 INFO  [import] 3 importiert, 1 Duplikate, 0 übersprungen, 0 Archive offen, 1.3 s\n", None),
        ("2026-10-03 10:05:42 INFO  [drucker] 1 Drucker abgefragt, 2 neue Drucke\n", None),
        ("2026-10-03 10:06:03 INFO  [update] Update-Check nicht möglich: Verbindung zu ", None), ("<ip-1>", True), (" abgelehnt\n", None),
        ("2026-10-03 10:07:30 INFO  [backup] Sicherung erstellt\n", None),
    ]
    fx.update({
        "get_app_version": VERSION, "is_preview_build": False, "has_step_preview": False,
        "check_app_update": {"currentVersion": VERSION, "availableVersion": update, "releaseUrl": None, "canInstall": True, "lastUpdate": None},
        "get_verbose_logging": {"enabled": False, "untilMs": None}, "get_bug_report_info": {"version": VERSION, "os": "windows"},
        "default_catalog_parent": DOCS[s.lang],
        "folder_removal_summary": {"name": parent["name"], "subfolderCount": 2, "modelCount": 4},
        "preview_log_export": {"id": 1, "segments": [{"text": x, "replaced": bool(r)} for x, r in lines],
                               "containsDebug": False, "replaceFileNames": False, "empty": False},
    })
    return fx


@recipe("hb-update-notice")
async def update_notice(s):
    t = tx(s)
    p = await s.open(v15_fixture(s, "update", NEXT))
    await asyncio.sleep(2)
    return await s.capture(p, {"notice": f"__fixed(__txt({C(t['update_now'])}, 'button'))"})


@recipe("hb-bug-report")
async def bug_report(s):
    t = tx(s)
    p = await s.open(v15_fixture(s, "fehler"))
    await js(p, f"__lbl({C(t['nav_settings'])}).click()", 1.0)
    await p.js(f"window.__GENERAL = {C(t['general'])}")
    await js(p, f"__tab({C(t['info'])}).click()", 1.5)
    await js(p, f"__txt({C(t['report'])}, 'button').click()", 1.2)
    await js(p, f"__txt({C(t['log_yes'])}).click()", 1.5)
    # long log lines wrap (for the picture only) so that nothing is cut off
    await p.js("""document.querySelectorAll('[role=dialog] *').forEach(e => { if (getComputedStyle(e).whiteSpace.startsWith('pre')) { e.style.whiteSpace = 'pre-wrap'; e.style.wordBreak = 'break-word'; } })""")
    await asyncio.sleep(0.5)
    return await s.capture(p, {"dialog": "document.querySelector('[role=dialog]')"})


@recipe("hb-remove-folder")
async def remove_folder(s):
    t = tx(s)
    p = await s.open(v15_fixture(s, "entfernen"))
    await to_catalog(s, p)
    ok = await p.js(f"""(() => {{ const el = __leaf({C(t['haushalt'])}, e => e.getBoundingClientRect().x < 320); if (!el) return false; const r = el.getBoundingClientRect();
      el.dispatchEvent(new MouseEvent('contextmenu', {{bubbles: true, cancelable: true, clientX: r.x + 20, clientY: r.y + 8}})); return true; }})()""")
    assert ok
    await asyncio.sleep(0.8)
    await p.js("[...document.querySelectorAll('[role=menuitem]')][0].click()")
    await asyncio.sleep(1.5)
    return await s.capture(p, {"dialog": "document.querySelector('[role=dialog]')"})


@recipe("hb-reset-catalog")
async def reset_catalog(s):
    t = tx(s)
    p = await s.open(v15_fixture(s, "reset"))
    await js(p, f"__lbl({C(t['nav_settings'])}).click()", 1.0)
    await p.js(f"window.__GENERAL = {C(t['general'])}")
    await js(p, f"__tab({C(t['catalog'])}).click()", 1.5)
    ok = await p.js(f"""(() => {{ const b = [...document.querySelectorAll('button')].find(b => (b.innerText || '').trim() === {C(t['reset'])}); if (!b) return false; b.scrollIntoView({{block: 'center'}}); b.click(); return true; }})()""")
    assert ok
    await asyncio.sleep(1.5)
    return await s.capture(p, {"dialog": "document.querySelector('[role=dialog]')"})
