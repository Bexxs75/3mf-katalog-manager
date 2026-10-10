"""Pictures for the test catalog scenarios (docs/tests/bilder): small crops with red numbered frames.

The frames count the clicks in the order the tester makes them; the captions live in the
scenario files. One recipe per picture, named tk-<picture>.
"""
from __future__ import annotations

import asyncio
import json

from engine import Session, recipe
from recipes.common import C, HELPERS, js, union_expr

VERSION = "0.16.0-1"
NEXT = "0.16.0-2"

T_ALL = {
    "de": dict(settings="Einstellungen", general="Allgemein", catalog="Katalog", printers="Drucker", info="Info",
               imp_cat="Katalog importieren", exp_cat="Katalog exportieren", location="Katalog-Speicherort",
               setup="Einrichten", change="Ändern", log_folder="Log-Ordner öffnen", data_folder="Datenordner öffnen",
               report="Fehler melden", verbose="Ausführliches Protokoll", pref_view="Bevorzugte Ansicht",
               printer_link="Druckeranbindung", import_="Importieren", files="Dateien...", folder="Ordner...",
               filament="Material Manager", later_setup="Später einrichten",
               adopt="Bestehende Ordnerstruktur übernehmen", new_loc="Neuen Ort einrichten", yes_replace="Ja, ersetzen",
               rename="Umbenennen", trash="Papierkorb", manage_printers="Drucker verwalten",
               log_no="Nein, ohne Logdatei melden", log_yes="Ja, Logdatei anhängen", replace_names="Dateinamen ersetzen",
               open_form="Formular öffnen", update_now="Jetzt aktualisieren", pick_place="Ort wählen …",
               extract="Entpacken", model="Kreisel.3mf", new_name="3D-Katalog-Test", docs="Dokumente",
               cleanup="Aufräum-Vorschläge"),
    "en": dict(settings="Settings", general="General", catalog="Catalog", printers="Printers", info="Info",
               imp_cat="Import catalog", exp_cat="Export catalog", location="Catalog Location",
               setup="Set up", change="Change", log_folder="Open log folder", data_folder="Open data folder",
               report="Report a bug", verbose="Detailed log", pref_view="Preferred view",
               printer_link="Printer connection", import_="Import", files="Files...", folder="Folder...",
               filament="Material Manager", later_setup="Set up later",
               adopt="Adopt an existing folder structure", new_loc="Set up a new location", yes_replace="Yes, replace",
               rename="Rename", trash="Trash", manage_printers="Manage printers",
               log_no="No, report without log file", log_yes="Yes, attach the log file", replace_names="Replace file names",
               open_form="Open form", update_now="Update now", pick_place="Choose place …",
               extract="Extract", model="Kreisel.3mf", new_name="3D-Catalog-Test", docs="Documents",
               cleanup="Cleanup suggestions"),
}
L_ALL = {
    "de": dict(grid="Raster", folders="Ordner", list_="Liste", sort="Name", modified="Änderungsdatum", haushalt="Haushalt",
               funktional="Funktional", tags="TAGS", search="Name oder Tag suchen …", new_coll="+ Neue Sammlung",
               coll="Schreibtisch-Ordnung", rename="Umbenennen", prev="Vorheriges Modell", next_="Nächstes Modell", close="Details schließen",
               tips="Tastenkürzel und Tipps", gen="Allgemein", single="Einzeltasten-Kürzel", last="Zuletzt angesehen", kreisel="Kreisel.3mf",
               handy="Handyständer.3mf", pflanz="Pflanztopf mit Untersetzer.3mf",
               details="Details", filters="Filter", color_file="Dateifarben", color_one="Einfarbig", fit="Einpassen", legend="Farben der Datei",
               add_unit="Einheit hinzufügen", link="Anbindung", pm="Printer Manager", notfound="Die Datei wurde nicht gefunden",
               sovol="Sovol SV08", clear_all="Alle löschen", assign="Materialzuordnung", view3d="3D-Ansicht"),
    "en": dict(last="Last viewed", grid="Grid", folders="Folders", list_="List", sort="Name", modified="Modified date", haushalt="Haushalt",
               funktional="Funktional", tags="TAGS", search="Search name or tag …", new_coll="+ New collection",
               coll="Schreibtisch-Ordnung", rename="Rename", prev="Previous model", next_="Next model", close="Close details",
               tips="Keyboard shortcuts and tips", gen="General", single="Single-key shortcuts", kreisel="Kreisel.3mf",
               handy="Handyständer.3mf", pflanz="Pflanztopf mit Untersetzer.3mf",
               details="Details", filters="Filters", color_file="File colors", color_one="Single color", fit="Fit model", legend="Colors in the file",
               add_unit="Add unit", link="Connection", pm="Printer Manager", notfound="The file was not found",
               sovol="Sovol SV08", clear_all="Clear all", assign="Material assignment", view3d="3D view"),
}

DOCS = {"windows": "C:\\Users\\Tester\\Documents", "macos": "/Users/tester/Documents", "linux": None}  # linux: per language


class Ctx:
    """Texts, platform paths and the fixture for one scene."""

    def __init__(self, s: Session):
        self.s = s
        self.T, self.L = T_ALL[s.lang], L_ALL[s.lang]
        t = self.T
        self.docs = {**DOCS, "linux": "/home/tester/" + t["docs"]}
        self.base = {"windows": "C:\\Users\\Tester\\Documents\\" + t["new_name"], "macos": "/Users/tester/Documents/" + t["new_name"],
                     "linux": "/home/tester/" + t["docs"] + "/" + t["new_name"]}

    def fixture(self, platform="windows", update=None, last_update=None, empty=False) -> dict:
        fx = self.s.fixture("tk16")
        if empty:
            for k, v in list(fx.items()):
                if isinstance(v, list) and (k.startswith("list_") or k.startswith("get_")):
                    fx[k] = []
        current = NEXT if last_update else VERSION
        lines = [
            ("2026-10-03 10:00:00 INFO  [start] 3MF Katalog Manager " + VERSION + " (mit STEP) · Windows 11 · Schema 40 · 15 Modelle\n", None),
            ("2026-10-03 10:00:00 INFO  [start] Katalog: ", None), ("<katalog>", True), ("\n", None),
            ("2026-10-03 10:02:11 INFO  [import] ", None), ("<katalog>", True), ("/Spielzeug/", None), ("Rakete.3mf", None), (" importiert\n", None),
            ("2026-10-03 10:05:42 INFO  [printer] Verbindung zu ", None), ("<ip-1>", True), (" nicht möglich\n", None),
            ("2026-10-03 10:06:03 INFO  [backup] Sicherung erstellt: ", None), ("~", True), ("/Documents/3mf-katalog-backup_2026-10-03.zip\n", None),
        ]
        fx.update({
            "get_app_version": current, "is_preview_build": True, "has_step_preview": True,
            "check_app_update": {"currentVersion": current, "availableVersion": update, "releaseUrl": None, "canInstall": True, "lastUpdate": last_update},
            "get_verbose_logging": {"enabled": False, "untilMs": None},
            "get_bug_report_info": {"version": current, "os": platform},
            "default_catalog_parent": self.docs[platform],
            "preview_catalog_dir": {"path": self.base[platform], "state": "new"},
            "preview_log_export": {"id": 1, "segments": [{"text": t, "replaced": bool(r)} for t, r in lines],
                                   "containsDebug": False, "replaceFileNames": False, "empty": False},
        })
        return fx

    async def page(self, fx: dict, prefs: dict | None = None):
        p = await self.s.open(fx, prefs)
        await p.js(f"window.__GENERAL = {json.dumps(self.T['general'])};")
        return p

    async def helpers(self, p):
        await p.js(HELPERS + f"window.__GENERAL = {json.dumps(self.T['general'])};")

    async def shot(self, p, crop_elements: list[str] | None, pad: float = 36.0, marks: str | None = None, name: str = "") -> object:
        """Capture; the crop is the union of the elements, 46 px left/top and 10 px right/bottom more, plus pad (device px)."""
        scale = self.s.scene.dpr
        if marks:
            missing = await p.js(f"__mark([{marks}])")
            if missing:
                raise RuntimeError(f"{name}: mark {missing} not found")
        rects = {}
        if crop_elements:
            q = pad / scale
            rects["crop"] = union_expr(crop_elements, (46 + q, 46 + q, 10 + q, 10 + q))
        return await self.s.capture(p, rects)

    async def open_settings(self, p, tab: str):
        await js(p, f"__lbl({C(self.T['settings'])}).click()")
        await js(p, f"__tab({C(self.T[tab])}).click()")
        await p.js("document.activeElement && document.activeElement.blur(); __unclamp()")
        await asyncio.sleep(0.5)

    def gear(self) -> str:
        return f"[() => __lbl({C(self.T['settings'])}), 1]"

    def settings_crop(self) -> str:
        return "__box(__tab(" + C(self.T["general"]) + "))"


def tk(name: str):
    """Register recipe tk-<name>; the function receives a Ctx and returns a Capture."""
    def deco(fn):
        @recipe("tk-" + name)
        async def run(s: Session):
            x = Ctx(s)
            return await fn(x, s.args)
        return fn
    return deco


@tk("zahnrad")
async def zahnrad(x, a):
    p = await x.page(x.fixture())
    # A lower window (like a laptop) brings the gear close to the sidebar entries.
    await p.viewport(1920, 620, x.s.scene.dpr)
    await x.helpers(p)
    import asyncio
    await asyncio.sleep(0.8)
    t = x.T
    return await x.shot(p, [f"__lbl({C(t['settings'])})", f"__has({C(t['cleanup'])}, 'button, a, div, span')"], pad=24, marks=x.gear(), name="zahnrad")


async def settings_button(x, a, tab, target, starts=False, prefs=None, fixture_name="x"):
    t = x.T
    pf = a.get("platform", "windows")
    p = await x.page(x.fixture(pf), prefs)
    await x.open_settings(p, tab)
    finder = f"__starts({C(t[target])}, 'button, label, h2, h3, h4, div, span')" if starts else f"__txt({C(t[target])})"
    tabs = f"__tab({C(t['general'])}), __tab({C(t['info'])})"
    return await x.shot(p, [tabs.split(", ")[0] if False else f"__tab({C(t['general'])})", f"__tab({C(t['info'])})", finder], pad=50,
                        marks=f"[() => __tab({C(t[tab])}), 2], [() => {finder}, 3]", name=target)


def _base_pref(x, a):
    return {"3mf-katalog-base-dir": x.base[a.get("platform", "windows")]}


@tk("kat-importieren")
async def kat_importieren(x, a):
    return await settings_button(x, a, "catalog", "imp_cat", prefs=_base_pref(x, a))


@tk("kat-exportieren")
async def kat_exportieren(x, a):
    return await settings_button(x, a, "catalog", "exp_cat", prefs=_base_pref(x, a))


@tk("kat-einrichten")
async def kat_einrichten(x, a):
    return await settings_button(x, a, "catalog", "setup", prefs={"3mf-katalog-base-dir": ""})


@tk("kat-aendern")
async def kat_aendern(x, a):
    return await settings_button(x, a, "catalog", "change", prefs=_base_pref(x, a))


@tk("kat-speicherort")
async def kat_speicherort(x, a):
    return await settings_button(x, a, "catalog", "location", prefs=_base_pref(x, a))


@tk("info-fehler")
async def info_fehler(x, a):
    return await settings_button(x, a, "info", "report")


@tk("info-logordner")
async def info_logordner(x, a):
    return await settings_button(x, a, "info", "log_folder")


@tk("info-datenordner")
async def info_datenordner(x, a):
    return await settings_button(x, a, "info", "data_folder")


@tk("info-ausfuehrlich")
async def info_ausfuehrlich(x, a):
    return await settings_button(x, a, "info", "verbose")


@tk("allg-ansicht")
async def allg_ansicht(x, a):
    return await settings_button(x, a, "general", "pref_view")


@tk("ersetzen")
async def ersetzen(x, a):
    t = x.T
    pf = a["platform"]
    p = await x.page(x.fixture(pf), {"3mf-katalog-base-dir": x.base[pf]})
    await x.open_settings(p, "catalog")
    await js(p, f"__txt({C(t['imp_cat'])}, 'button').click()")
    yes = f"__txt({C(t['yes_replace'])}, 'button')"
    return await x.shot(p, [yes, f"__box({yes})"], pad=36, marks=f"[() => {yes}, 4]", name="ersetzen")


async def import_menu(x, name, option):
    t = x.T
    p = await x.page(x.fixture())
    imp = f"[...document.querySelectorAll('button')].find(b => b.querySelector('svg') && b.innerText.includes({C(t['import_'])}))"
    await js(p, f"{imp}.click()")
    item = lambda key: f"__txt({C(t[key])}, 'button, [role=menuitem], li, a, div, span')"
    return await x.shot(p, [imp, item("files"), item("folder")], pad=60, marks=f"[() => {imp}, 1], [() => {item(option)}, 2]", name=name)


@tk("import-dateien")
async def import_dateien(x, a):
    return await import_menu(x, "import-dateien", "files")


@tk("import-ordner")
async def import_ordner(x, a):
    return await import_menu(x, "import-ordner", "folder")


async def rail(x, name, key):
    t = x.T
    p = await x.page(x.fixture())
    return await x.shot(p, [f"__rail({C(t[key])})", f"__has({C(t['import_'])}, 'button')"], pad=20, marks=f"[() => __rail({C(t[key])}), 1]", name=name)


@tk("lager")
async def lager(x, a):
    return await rail(x, "lager", "filament")


@tk("papierkorb")
async def papierkorb(x, a):
    return await rail(x, "papierkorb", "trash")


@tk("umbenennen")
async def umbenennen(x, a):
    t = x.T
    p = await x.page(x.fixture())
    await js(p, f"""(() => {{ const el = __card({C(t['model'])}); const r = el.getBoundingClientRect();
      el.dispatchEvent(new MouseEvent('contextmenu', {{ bubbles: true, cancelable: true, clientX: r.x + 120, clientY: r.y + 50 }})); }})()""")
    ren = f"__has({C(t['rename'])}, 'button, [role=menuitem], li')"
    return await x.shot(p, [f"__card({C(t['model'])})", f"__fixed({ren})"], pad=36,
                        marks=f"[() => __card({C(t['model'])}), 1], [() => {ren}, 2]", name="umbenennen")


async def setup_shot(x, name, target, n, via_settings=False):
    t = x.T
    import asyncio
    if via_settings:
        p = await x.page(x.fixture(), {"3mf-katalog-base-dir": ""})
        await js(p, "localStorage.removeItem('3mf-katalog-base-dir'); location.reload()", 3)
        await x.helpers(p)
        await x.open_settings(p, "catalog")
        await js(p, f"__txt({C(t['setup'])}, 'button').click()", 1.2)
    else:
        p = await x.page(x.fixture(empty=True), {"3mf-katalog-setup-seen": "", "3mf-katalog-base-dir": ""})
        await js(p, "localStorage.removeItem('3mf-katalog-setup-seen'); localStorage.removeItem('3mf-katalog-base-dir'); location.reload()", 3)
        await x.helpers(p)
    finder = (f"(__has({C(t[target])}, 'button').closest('button') || __has({C(t[target])}, 'button'))" if target in ("adopt", "new_loc")
              else f"__txt({C(t[target])}, 'a, button, span')")
    return await x.shot_panel(p, f"__panel(__txt({C(t['later_setup'])}, 'a, button, span'), 500)", marks=f"[() => {finder}, {n}]", name=name)


async def _shot_panel(self, p, element: str, marks: str | None = None, name: str = "", pad: float = 36.0):
    """Capture cropped to one element (plus pad device px on every side)."""
    if marks:
        missing = await p.js(f"__mark([{marks}])")
        if missing:
            raise RuntimeError(f"{name}: mark {missing} not found")
    q = pad / self.s.scene.dpr
    return await self.s.capture(p, {"crop": union_expr([element], q)})


Ctx.shot_panel = _shot_panel


@tk("setup-spaeter")
async def setup_spaeter(x, a):
    return await setup_shot(x, "setup-spaeter", "later_setup", 1)


@tk("setup-uebernehmen")
async def setup_uebernehmen(x, a):
    return await setup_shot(x, "setup-uebernehmen", "adopt", 4, via_settings=True)


@tk("setup-neu")
async def setup_neu(x, a):
    return await setup_shot(x, "setup-neu", "new_loc", 4, via_settings=True)


@tk("setup-neu-form")
async def setup_neu_form(x, a):
    t = x.T
    pf = a["platform"]
    p = await x.page(x.fixture(pf), {"3mf-katalog-base-dir": x.base[pf]})
    await x.open_settings(p, "catalog")
    await js(p, f"__txt({C(t['change'])}, 'button').click()", 1.2)
    await js(p, f"(__has({C(t['new_loc'])}, 'button').closest('button') || __has({C(t['new_loc'])}, 'button')).click()", 1.2)
    await js(p, f"__has({C(t['pick_place'])}, 'button').click()", 1.2)
    inp = "[...document.querySelectorAll('input[type=text], input:not([type])')].filter(e => e.getBoundingClientRect().width > 0 && e.getBoundingClientRect().x > 400).pop()"
    await js(p, f"(() => {{ const i = {inp}; if (i) __setInput(i, {C(t['new_name'])}); }})()", 1.5)
    create = "[...document.querySelectorAll('button')].filter(b => /anlegen|Create folder/.test(b.innerText) && b.getBoundingClientRect().width > 0).pop()"
    pick = f"__has({C(t['pick_place'])}, 'button')"
    return await x.shot(p, [pick, inp, create], pad=70, marks=f"[() => {pick}, 5], [() => {inp}, 6], [() => {create}, 7]", name="setup-neu-form")


async def fehler(x, name, choice=None, marks=None):
    t = x.T
    p = await x.page(x.fixture())
    await x.open_settings(p, "info")
    await js(p, f"__txt({C(t['report'])}, 'button').click()", 1.2)
    if choice:
        await js(p, f"__txt({C(t[choice])}).click()", 1.2)
    return await x.shot_panel(p, f"__panel(__has({C(t['log_no'])}), 500)", marks=marks, name=name)


@tk("fehler-dialog")
async def fehler_dialog(x, a):
    return await fehler(x, "fehler-dialog")


@tk("fehler-nein")
async def fehler_nein(x, a):
    t = x.T
    return await fehler(x, "fehler-nein", marks=f"[() => __txt({C(t['log_no'])}), 1], [() => __txt({C(t['open_form'])}, 'button'), 2]")


@tk("fehler-ja")
async def fehler_ja(x, a):
    return await fehler(x, "fehler-ja", marks=f"[() => __txt({C(x.T['log_yes'])}), 1]")


@tk("fehler-vorschau")
async def fehler_vorschau(x, a):
    return await fehler(x, "fehler-vorschau", choice="log_yes")


@tk("fehler-dateinamen")
async def fehler_dateinamen(x, a):
    return await fehler(x, "fehler-dateinamen", choice="log_yes", marks=f"[() => __has({C(x.T['replace_names'])}, 'label, span, div'), 1]")


@tk("update-meldung")
async def update_meldung(x, a):
    import asyncio
    t = x.T
    p = await x.page(x.fixture(update=NEXT))
    await asyncio.sleep(1.5)
    btn = f"__txt({C(t['update_now'])}, 'button')"
    return await x.shot_panel(p, f"__fixed({btn})", marks=f"[() => {btn}, 1]", name="update-meldung")


@tk("info-update")
async def info_update(x, a):
    p = await x.page(x.fixture(update=NEXT))
    await x.open_settings(p, "info")
    return await x.shot_panel(p, x.settings_crop(), name="info-update")


@tk("info-nach-update")
async def info_nach_update(x, a):
    last = {"version": NEXT, "date": "2026-10-03", "backupFile": f"catalog-vor-{NEXT}.db"}
    p = await x.page(x.fixture(last_update=last))
    await x.open_settings(p, "info")
    return await x.shot_panel(p, x.settings_crop(), name="info-nach-update")


@tk("entpacken")
async def entpacken(x, a):
    import asyncio
    t = x.T
    fx = x.fixture()
    paths = [i["path"] for i in fx.get("inspect_archives", [])][:2]
    p = await x.page(fx)
    await js(p, f"__demoEmitDrop({json.dumps(paths)})", 1.0)
    res = {"jobId": "demo-job", "source": "dropped", "state": "finished", "parentJobId": None, "jobError": None, "scanComplete": True, "placementRequired": False,
           "groups": {"imported": [], "importedNotPlaced": [], "duplicate": [], "skipped": [], "archive": [{"path": q, "state": "pending"} for q in paths]},
           "counts": {"imported": 0, "importedNotPlaced": 0, "duplicate": 0, "skipped": 0, "archive": len(paths), "known": len(paths)}}
    prog = {"jobId": "demo-job", "state": "finished", "scanComplete": True, "total": len(paths), "found": len(paths), "done": len(paths), "inFlight": 0,
            "counts": res["counts"], "current": None, "elapsedMs": 500}
    await p.js(f"window.__demoImportResult = {json.dumps(res)}; window.__demoImportProgress = {json.dumps(prog)}; window.__demoEmit('import://progress', {json.dumps(prog)}); window.__demoEmit('import://finished', {json.dumps(res)});")
    await asyncio.sleep(2.0)
    btn = f"__starts({C(t['extract'])}, 'button')"
    return await x.shot_panel(p, f"__panel({btn}, 500)", marks=f"[() => {btn}, 1]", name="entpacken")


# ---------- new in 0.16.0 ----------

async def cat_page(x, prefs=None):
    p = await x.page(x.fixture(), prefs)
    await p.js("__expand()")  # folder tree open, as in the scenarios
    await asyncio.sleep(0.5)
    return p


@tk("ansichten")
async def ansichten(x, a):
    p = await cat_page(x)
    grp = f"__txt({C(x.L['grid'])}, 'button').parentElement"
    return await x.shot(p, [grp], pad=70, marks=f"[() => {grp}, 1]", name="ansichten")


@tk("sortieren")
async def sortieren(x, a):
    L = x.L
    p = await cat_page(x)
    btn = f"[...document.querySelectorAll('button')].find(b => b.querySelector('svg') && b.innerText.trim().startsWith({C(L['sort'])}))"
    await js(p, f"{btn}.click()")
    item = f"[...document.querySelectorAll('button, [role=menuitem], li')].find(e => e.innerText.trim().startsWith({C(L['modified'])}))"
    last = f"[...document.querySelectorAll('button, [role=menuitem], li')].find(e => e.innerText.trim().startsWith({C(L['last'])}))"
    return await x.shot(p, [btn, last, "__has('Z → A', 'button')"], pad=40, marks=f"[() => {btn}, 1], [() => {item}, 2]", name="sortieren")


@tk("filterleiste")
async def filterleiste(x, a):
    L = x.L
    p = await cat_page(x)
    await p.viewport(1250, 800, x.s.scene.dpr)
    import asyncio
    await asyncio.sleep(0.8)
    await x.helpers(p)
    await js(p, f"__txt({C(L['haushalt'])}, 'button, div, span, li').click()")
    await js(p, f"[...document.querySelectorAll('span')].find(e => e.innerText.trim() === '#' + {C(L['funktional'])} && e.getBoundingClientRect().width > 0).click()", 1.0)
    await js(p, f"(() => {{ const i = document.querySelector('input[placeholder={json.dumps(L['search'])}]') || [...document.querySelectorAll('input')].find(e => e.getBoundingClientRect().width > 0); __setInput(i, 'Pflanz'); }})()", 1.0)
    clr = f"__has({C(L['clear_all'])}, 'button')"
    lab = f"__has({C(L['filters'].upper())}, 'span, div')"
    return await x.shot(p, [lab, clr], pad=30, marks=f"[() => {union_expr([lab, clr])}, 1]", name="filterleiste")


@tk("sammlung-menue")
async def sammlung_menue(x, a):
    L = x.L
    p = await cat_page(x)
    coll = f"__txt({C(L['coll'])}, 'button, div, span, li')"
    await js(p, f"""(() => {{ const el = {coll}; const r = el.getBoundingClientRect();
      el.dispatchEvent(new MouseEvent('contextmenu', {{ bubbles: true, cancelable: true, clientX: r.right + 30, clientY: r.y + 8 }})); }})()""")
    it = f"__has({C(L['rename'])}, 'button, [role=menuitem], li')"
    return await x.shot(p, [coll, f"__fixed({it})"], pad=40, marks=f"[() => {coll}, 1], [() => {it}, 2]", name="sammlung-menue")


async def open_detail(p, name):
    await js(p, f"""(() => {{ const el = [...document.querySelectorAll('*')].filter(e => e.children.length === 0 && (e.innerText || '').trim() === {C(name)} && e.getBoundingClientRect().x > 300)[0]; el.scrollIntoView({{ block: 'center' }}); el.dispatchEvent(new MouseEvent('dblclick', {{ bubbles: true, cancelable: true }})); }})()""", 1.5)


@tk("detail-pfeile")
async def detail_pfeile(x, a):
    L = x.L
    p = await cat_page(x)
    await open_detail(p, L["handy"])
    pv, nx = f"__lbl({C(L['prev'])})", f"__lbl({C(L['next_'])})"
    return await x.shot(p, [pv, nx], pad=150, marks=f"[() => {union_expr([pv, nx])}, 1]", name="detail-pfeile")


@tk("detail-bereich")
async def detail_bereich(x, a):
    L = x.L
    p = await cat_page(x)
    await js(p, f"__card({C(L['kreisel'])}).click()", 1.5)
    st = "document.querySelector('[aria-label=\"Status\"]')"
    return await x.shot(p, [f"__lbl({C(L['close'])})", st], pad=60, marks=f"[() => {st}, 1]", name="detail-bereich")


@tk("tipps")
async def tipps(x, a):
    p = await cat_page(x)
    await js(p, f"__lbl({C(x.L['tips'])}).click()", 1.0)
    return await x.shot(p, ["__dialog()"], pad=20, name="tipps")


@tk("einzeltasten")
async def einzeltasten(x, a):
    L = x.L
    p = await cat_page(x)
    await x.open_settings(p, "general")
    tg = f"[...document.querySelectorAll('button, label, [role=switch]')].filter(e => (e.innerText || '').includes({C(L['single'])}) && e.getBoundingClientRect().width > 0 && e.getBoundingClientRect().height < 120)[0]"
    return await x.shot(p, [tg, f"__tab({C(L['gen'])})"], pad=40, marks=f"[() => {tg}, 1]", name="einzeltasten")


@tk("import-menue")
async def import_menue(x, a):
    t = x.T
    p = await cat_page(x)
    imp = f"[...document.querySelectorAll('button')].find(b => b.querySelector('svg') && b.innerText.includes({C(t['import_'])}))"
    await js(p, f"{imp}.click()")
    return await x.shot(p, [imp, f"__txt({C(t['folder'])}, 'button, [role=menuitem], li, a, div, span')"], pad=60, marks=f"[() => {imp}, 1]", name="import-menue")


PROGRESS = {"jobId": "demo-job", "state": "importing", "scanComplete": True, "total": 15, "found": 15, "done": 6, "inFlight": 2,
            "counts": {"imported": 0, "importedNotPlaced": 0, "duplicate": 6, "skipped": 0, "archive": 0, "known": 15},
            "current": "Kreisel.3mf", "elapsedMs": 2000}


@tk("import-zeile")
async def import_zeile(x, a):
    import asyncio
    t, L = x.T, x.L
    p = await cat_page(x)
    await p.viewport(1250, 800, x.s.scene.dpr)
    await asyncio.sleep(0.8)
    await x.helpers(p)
    imp = f"[...document.querySelectorAll('button')].find(b => b.querySelector('svg') && b.innerText.includes({C(t['import_'])}))"
    await js(p, f"{imp}.click()")
    await js(p, f"__txt({C(t['folder'])}, 'button, [role=menuitem], li, a, div, span').click()", 1.0)
    dup = [{"path": f"/home/tester/3MF-Testdaten/1 Katalog/{n}", "kind": "path", "existingFileId": str(i + 1)} for i, n in enumerate(["Deko/Geo-Schale.3mf", "Haushalt/Kabelclip.stl", "Spielzeug/Rakete.3mf"])]
    res = {"jobId": "demo-job", "source": "folder", "state": "finished", "parentJobId": None, "jobError": None, "scanComplete": True, "placementRequired": False,
           "groups": {"imported": [], "importedNotPlaced": [], "duplicate": dup, "skipped": [], "archive": []},
           "counts": {"imported": 0, "importedNotPlaced": 0, "duplicate": 15, "skipped": 0, "archive": 0, "known": 15}}
    done = {**PROGRESS, "state": "finished", "done": 15, "total": 15, "inFlight": 0, "current": None, "elapsedMs": 3000, "counts": res["counts"]}
    await p.js(f"window.__demoImportResult = {json.dumps(res)}; window.__demoImportProgress = {json.dumps(done)}; window.__demoEmit('import://progress', {json.dumps(done)}); window.__demoEmit('import://finished', {json.dumps(res)});")
    await asyncio.sleep(1.5)
    det = f"__txt({C(L['details'])}, 'button')"
    row = f"(() => {{ let e = {det}; while (e.parentElement && e.parentElement.getBoundingClientRect().height < 140) e = e.parentElement; return e; }})()"
    return await x.shot(p, [row], pad=30, marks=f"[() => {det}, 1]", name="import-zeile")


@tk("import-ablage")
async def import_ablage(x, a):
    t = x.T
    p = await cat_page(x)
    await js(p, f"__txt({C('Werkstatt')}, 'button, div, span, li').click()", 1.0)
    imp = f"[...document.querySelectorAll('button')].find(b => b.querySelector('svg') && b.innerText.includes({C(t['import_'])}))"
    await js(p, f"{imp}.click()")
    fld = f"__txt({C('Werkstatt')}, 'button, div, span, li')"
    files = f"__txt({C(t['files'])}, 'button, [role=menuitem], li, a, div, span')"
    return await x.shot(p, [fld, imp, files], pad=40, marks=f"[() => {fld}, 1], [() => {files}, 2]", name="import-ablage")


@tk("ziehen")
async def ziehen(x, a):
    p = await cat_page(x)
    await p.js("(() => { const st = document.createElement('style'); st.textContent = '[data-drag-grip]{opacity:1 !important}'; document.head.appendChild(st); })()")
    grip = "document.querySelector('[data-drag-grip]')"
    fld = f"__txt({C('Haushalt')}, 'button, div, span, li')"
    return await x.shot(p, [grip, fld], pad=60, marks=f"[() => {grip}, 1], [() => {fld}, 2]", name="ziehen")


async def pm_open(x, p):
    await js(p, f"__lbl({C(x.L['pm'])}).click()", 1.5)


@tk("pm-liste")
async def pm_liste(x, a):
    p = await cat_page(x)
    await pm_open(x, p)
    rect = "({getBoundingClientRect:()=>({x:0,y:0,width:380,height:420,right:380,bottom:420})})"
    return await x.shot(p, [rect], pad=10, marks=f"[() => __lbl({C(x.L['pm'])}), 1]", name="pm-liste")


@tk("pm-einheiten")
async def pm_einheiten(x, a):
    p = await cat_page(x)
    await pm_open(x, p)
    await js(p, f"[...document.querySelectorAll('button')].find(b => (b.innerText || '').includes({C(x.L['sovol'])})).click()", 1.0)
    add = f"__has({C(x.L['add_unit'])}, 'button')"
    return await x.shot(p, [add, f"__has({C(x.L['assign'])}, 'h2, h3, div')"], pad=60, marks=f"[() => {add}, 1]", name="pm-einheiten")


@tk("pm-anbindung")
async def pm_anbindung(x, a):
    L = x.L
    p = await cat_page(x)
    await pm_open(x, p)
    await js(p, f"[...document.querySelectorAll('button')].find(b => (b.innerText || '').includes({C(L['sovol'])})).click()", 1.0)
    sw = f"[...document.querySelectorAll('[role=switch], input[type=checkbox], button')].filter(e => (e.getAttribute('aria-label') || e.innerText || '').includes({C(L['link'])}) && e.getBoundingClientRect().y > 500)[0]"
    rect = "({getBoundingClientRect:()=>({x:0,y:960,width:380,height:120,right:380,bottom:1080})})"
    return await x.shot(p, [rect], pad=10, marks=f"[() => {sw}, 1]", name="pm-anbindung")


async def view3d(x, p, name):
    await open_detail(p, name)
    await js(p, "[...document.querySelectorAll('button')].find(b => /^(3D-Ansicht|3D view)$/.test((b.innerText || '').trim())).click()", 3.0)


@tk("farben")
async def farben(x, a):
    L = x.L
    p = await cat_page(x)
    await view3d(x, p, L["pflanz"])
    tog = f"__txt({C(L['color_file'])}, 'button')"
    leg = f"[...document.querySelectorAll('div, span')].filter(e => (e.innerText || '').toLowerCase().startsWith({C(L['legend'].lower())}) && e.getBoundingClientRect().height < 200 && e.getBoundingClientRect().width > 0)[0]"
    return await x.shot(p, [tog, leg], pad=60, marks=f"[() => {tog}, 1], [() => {leg}, 2]", name="farben")


@tk("farben-faelle")
async def farben_faelle(x, a):
    L = x.L
    p = await cat_page(x)
    await view3d(x, p, "Teelichthalter Stern.3mf")
    tog = f"__txt({C(L['color_file'])}, 'button')"
    return await x.shot(p, [f"{tog}.parentElement", f"__txt({C('3D-Ansicht')}, 'button')", f"__txt({C(L['fit'])}, 'button')"], pad=30,
                        marks=f"[() => {tog}, 1]", name="farben-faelle")


@tk("fehlerkarte")
async def fehlerkarte(x, a):
    L = x.L
    p = await cat_page(x)
    await js(p, "window.__demoFailGeometry = true", 0.1)
    await view3d(x, p, "Kabelclip.stl")
    return await x.shot(p, [f"__has({C(L['notfound'])}, 'h3').closest('[role=alert]')"], pad=30, name="fehlerkarte")


@tk("platten")
async def platten(x, a):
    p = await cat_page(x)
    await view3d(x, p, x.L["pflanz"])
    grp = "document.querySelector('[data-plate-selector]')"
    return await x.shot(p, [grp], pad=80, marks=f"[() => {grp}, 1]", name="platten")
