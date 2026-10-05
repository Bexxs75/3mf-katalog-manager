"""Manifest checks: structure, duplicates, target files and their sizes, recipes, fixtures."""
import json
import sys
from collections import Counter
from pathlib import Path

import pytest
from PIL import Image

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))

import fixtures  # noqa: E402
import make  # noqa: E402
import render  # noqa: E402

DATA = json.loads((HERE / "manifest.json").read_text(encoding="utf-8"))
ENTRIES = DATA["images"]
ROOTS = make.roots(None)
REQUIRED = {"id", "root", "paths", "recipe", "args", "theme", "viewport", "dpr", "format", "sizes"}


def targets():
    for e in ENTRIES:
        for lang, path in e["paths"].items():
            yield e, lang, path


def test_required_keys_and_values():
    for e in ENTRIES:
        assert REQUIRED <= set(e), e["id"]
        assert e["root"] in ("app", "website", "wiki"), e["id"]
        assert e["theme"] in ("dark", "light"), e["id"]
        assert set(e["paths"]) <= {"de", "en"} and e["paths"], e["id"]
        assert set(e["sizes"]) == set(e["paths"]), e["id"]
        assert len(e["viewport"]) == 2 and e["dpr"] > 0
        assert e["format"] in ("png", "webp", "jpg"), e["id"]
        for lang, p in e["paths"].items():
            assert p.endswith("." + e["format"]), f"{e['id']}: format and extension differ"
            assert not p.startswith("/") and ".." not in p, p
        if e.get("crop"):
            assert e["crop"].get("rect"), e["id"]


def test_no_duplicate_ids_or_targets():
    ids = Counter(e["id"] for e in ENTRIES)
    assert [i for i, n in ids.items() if n > 1] == []
    paths = Counter((e["root"], p) for e, _, p in targets())
    assert [p for p, n in paths.items() if n > 1] == []


def test_excluded_are_not_in_the_manifest():
    inc = {(e["root"], p) for e, _, p in targets()}
    for item in DATA["excluded"]:
        assert (item["root"], item["path"]) not in inc
        assert item.get("reason")


def test_every_entry_has_a_recipe():
    import engine
    render.load_recipes()
    missing = sorted({e["recipe"] for e in ENTRIES} - set(engine.RECIPES))
    assert missing == []


def test_target_files_exist_with_the_listed_size():
    for e, lang, path in targets():
        root = ROOTS[e["root"]]
        if root is None:
            continue  # website repository not checked out (CI): only the app pictures are checked
        f = root / path
        assert f.exists(), f"{e['id']}: {path} missing"
        assert list(Image.open(f).size) == e["sizes"][lang], f"{e['id']} {lang}: size differs from the manifest"


def test_fixtures_load_and_overlays_apply():
    for name in ("base-de", "base-en", "tk16", "link-de", "link-en", "trash-de", "trash-en"):
        fx = fixtures.load(name)
        assert fx["list_files"] and "__geometry" in fx
    assert len(fixtures.load("trash-de")["list_files"]) == 13
    assert len(fixtures.load("trash-de")["list_trash"]) == 2


def test_overlay_roundtrip():
    a = {"x": [{"id": "1", "v": 1}, {"id": "2", "v": 2}], "y": {"k": 1}, "z": 5}
    b = {"x": [{"id": "1", "v": 9}, {"id": "3", "v": 3}], "y": {"k": 2}, "z": 5, "w": [1]}
    assert fixtures.apply_ops(a, fixtures.diff_ops(a, b)) == b
