"""From a Capture to the finished picture (crop, scale, format) and the structure comparison."""
from __future__ import annotations

import io
from pathlib import Path

import numpy as np
from PIL import Image

from engine import Capture

ASPECT_TOLERANCE = 0.03  # a crop whose proportions changed by more is kept at its natural size


def render(entry: dict, lang: str, cap: Capture) -> tuple[Image.Image, list[str]]:
    """Finished image for a manifest entry and a list of notes (for example a changed size)."""
    notes: list[str] = []
    im = Image.open(io.BytesIO(cap.png)).convert("RGB")
    crop = entry.get("crop")
    if crop:
        x, y, w, h = cap.rects[crop["rect"]]
        pad = crop.get("pad", 0)
        s = cap.scale
        want = (entry.get("sizes") or {}).get(lang)
        if entry.get("fit") == "exact" and want:
            # the page gives the picture a fixed size (width/height attributes): start at the element's top left
            # (so headings stay) and cut or extend on the right and at the bottom
            left = min(max(0, int(round((x - pad) * s))), max(0, im.width - want[0]))
            top = min(max(0, int(round((y - pad) * s))), max(0, im.height - want[1]))
            box = (left, top, min(im.width, left + want[0]), min(im.height, top + want[1]))
            if (box[2] - box[0], box[3] - box[1]) != tuple(want):
                notes.append("window too small for the fixed size")
            if int(round((x + w + pad) * s)) - left > want[0] or int(round((y + h + pad) * s)) - top > want[1]:
                notes.append("element larger than the fixed size, cut off on the right/bottom")
        else:
            box = (max(0, int(round((x - pad) * s))), max(0, int(round((y - pad) * s))),
                   min(im.width, int(round((x + w + pad) * s))), min(im.height, int(round((y + h + pad) * s))))
        im = im.crop(box)
    if entry.get("max_edge"):
        im.thumbnail((entry["max_edge"], entry["max_edge"]), Image.LANCZOS)
    want = (entry.get("sizes") or {}).get(lang)
    if want and tuple(want) != im.size:
        want = tuple(want)
        if not crop or abs((im.width / im.height) / (want[0] / want[1]) - 1) <= ASPECT_TOLERANCE:
            im = im.resize(want, Image.LANCZOS)
        else:
            notes.append(f"size {im.width}x{im.height} instead of {want[0]}x{want[1]}")
    return im, notes


def encode(im: Image.Image, fmt: str, quality: int = 80) -> bytes:
    buf = io.BytesIO()
    if fmt == "png":
        im.save(buf, "PNG", optimize=True)
    elif fmt == "webp":
        im.save(buf, "WEBP", quality=quality, method=6)
    elif fmt in ("jpg", "jpeg"):
        im.save(buf, "JPEG", quality=quality)
    else:
        raise ValueError(fmt)
    return buf.getvalue()


def _box_mean(a: np.ndarray, k: int) -> np.ndarray:
    pad = k // 2
    p = np.pad(a, pad, mode="reflect")
    c = np.cumsum(np.cumsum(p, 0), 1)
    c = np.pad(c, ((1, 0), (1, 0)))
    n = a.shape[0], a.shape[1]
    s = c[k:k + n[0], k:k + n[1]] - c[:n[0], k:k + n[1]] - c[k:k + n[0], :n[1]] + c[:n[0], :n[1]]
    return s / (k * k)


def ssim(a: Image.Image, b: Image.Image, window: int = 7) -> float:
    """Structural similarity of two images (grey, mean over windows); b is scaled to a's size if needed."""
    if a.size != b.size:
        b = b.resize(a.size, Image.LANCZOS)
    # the large pictures are compared at reduced size: faster and not sensitive to single pixels
    scale = min(1.0, 800 / max(a.size))
    if scale < 1:
        size = (max(8, int(a.width * scale)), max(8, int(a.height * scale)))
        a, b = a.resize(size, Image.LANCZOS), b.resize(size, Image.LANCZOS)
    x = np.asarray(a.convert("L"), dtype=np.float64)
    y = np.asarray(b.convert("L"), dtype=np.float64)
    if min(x.shape) < window:
        window = max(3, min(x.shape) // 2 * 2 - 1)
    c1, c2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
    mx, my = _box_mean(x, window), _box_mean(y, window)
    vx, vy = _box_mean(x * x, window) - mx * mx, _box_mean(y * y, window) - my * my
    cov = _box_mean(x * y, window) - mx * my
    m = ((2 * mx * my + c1) * (2 * cov + c2)) / ((mx * mx + my * my + c1) * (vx + vy + c2))
    return float(m.mean())
