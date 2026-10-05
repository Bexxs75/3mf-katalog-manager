"""Demo catalogs for the screenshots: two full base catalogs (de, en) plus small overlays.

A fixture is a JSON object {command name: answer of that Tauri command}. The mock
(mock.js) hands these answers to the UI instead of the Rust backend, so the pictures
do not depend on a real catalog. Everything that differs between two scenes
(printers, trash, test catalog) lives in a small overlay file next to the base.

Overlay format (fixtures/<name>.json):
    {"base": "base-de",
     "ops": {"<command>": {"set": <value>}                       # replace the answer
                        | {"by_id": {"patch": {"id": {field: value}}, "upsert": [...], "drop": ["id", ...]}}  # list of {"id": ...}
                        | {"merge": {...}}}}                     # dict: replace the given keys
A file without "base" is a complete catalog. "__geometry" (model meshes, base64) is
kept in geometry.json and merged in by load(), so the base files stay small.
"""
from __future__ import annotations

import copy
import json
from pathlib import Path

DIR = Path(__file__).parent / "fixtures"


def _read(name: str) -> dict:
    return json.loads((DIR / f"{name}.json").read_text(encoding="utf-8"))


def apply_ops(base: dict, ops: dict) -> dict:
    out = copy.deepcopy(base)
    for key, op in ops.items():
        if "set" in op:
            out[key] = copy.deepcopy(op["set"])
        elif "merge" in op:
            out[key] = {**out.get(key, {}), **copy.deepcopy(op["merge"])}
        elif "by_id" in op:
            items = {str(i["id"]): i for i in out.get(key, [])}
            order = list(items)
            for i, fields in op["by_id"].get("patch", {}).items():
                items[str(i)] = {**items[str(i)], **copy.deepcopy(fields)}
            for i in op["by_id"].get("upsert", []):
                if str(i["id"]) not in items:
                    order.append(str(i["id"]))
                items[str(i["id"])] = copy.deepcopy(i)
            for i in op["by_id"].get("drop", []):
                items.pop(str(i), None)
            out[key] = [items[i] for i in order if i in items]
        else:
            raise ValueError(f"unknown overlay operation for {key}: {list(op)}")
    return out


def load(name: str) -> dict:
    """Complete fixture dict for a name (base or overlay), including the meshes."""
    spec = _read(name)
    if "base" in spec:
        data = apply_ops(load(spec["base"]), spec["ops"])
    else:
        data = spec
        data["__geometry"] = _read("geometry")
    return data


def diff_ops(a: dict, b: dict) -> dict:
    """Overlay operations that turn catalog a into catalog b (used to build the overlays)."""
    ops: dict = {}
    for key in b:
        if a.get(key) == b[key]:
            continue
        av, bv = a.get(key), b[key]
        if key == "__geometry":
            ops[key] = {"merge": {k: v for k, v in bv.items() if av.get(k) != v}}
        elif isinstance(av, list) and isinstance(bv, list) and av and bv and all(isinstance(i, dict) and "id" in i for i in av + bv):
            aid = {str(i["id"]): i for i in av}
            patch, upsert = {}, []
            for i in bv:
                old = aid.get(str(i["id"]))
                if old is None:
                    upsert.append(i)
                elif old != i:
                    patch[str(i["id"])] = {k: v for k, v in i.items() if old.get(k) != v}
            drop = [i for i in aid if i not in {str(x["id"]) for x in bv}]
            op = {"by_id": {k: v for k, v in (("patch", patch), ("upsert", upsert), ("drop", drop)) if v}}
            # list order and removed fields cannot be expressed as a patch: then take the whole list
            ops[key] = op if apply_ops({key: av}, {key: op})[key] == bv else {"set": bv}
        else:
            ops[key] = {"set": bv}
    for key in a:
        if key not in b:
            raise ValueError(f"key {key} missing in target")
    return ops
