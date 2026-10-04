#!/usr/bin/env python3
"""Post a short preview announcement with up to four images to Bluesky.

Used by .github/workflows/bluesky-preview.yml. The app password only exists as a
GitHub secret, so the post is made from the workflow; `--dry-run` composes
everything (text, link facet, resized images) without any network access.

Limits that matter (AT Protocol / Bluesky): 300 graphemes of text, at most four
images per post, about 1 MB per image blob. The text length is checked in code
points, which is never lower than the grapheme count, so a post that passes
here is accepted there.
"""
import argparse, datetime, io, json, os, pathlib, sys, urllib.parse, urllib.request

API = "https://bsky.social/xrpc"
PUBLIC_API = "https://public.api.bsky.app/xrpc"
MAX_TEXT = 300
MAX_IMAGES = 4
MAX_BLOB = 950_000          # Bluesky rejects blobs above ~1,000,000 bytes; keep headroom
MAX_WIDTH = 2000
IMAGE_TYPES = {".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".webp": "image/webp"}


def fail(msg):
    print(f"bluesky_preview: {msg}", file=sys.stderr)
    raise SystemExit(1)


def compose(title, body="", link=""):
    """Return (text, facets). The title is the first line; body and link follow."""
    parts = [title.strip()]
    if body.strip():
        parts.append(body.strip())
    if link.strip():
        parts.append(link.strip())
    text = "\n\n".join(parts)
    if not title.strip():
        fail("a title is required")
    if len(text) > MAX_TEXT:
        fail(f"post text is {len(text)} characters, the limit is {MAX_TEXT}; shorten the title or body")
    facets = []
    if link.strip():
        # Facets address UTF-8 byte offsets, not characters (umlauts and emoji matter).
        start = text.rindex(link.strip())
        byte_start = len(text[:start].encode("utf-8"))
        byte_end = byte_start + len(link.strip().encode("utf-8"))
        facets.append({
            "index": {"byteStart": byte_start, "byteEnd": byte_end},
            "features": [{"$type": "app.bsky.richtext.facet#link", "uri": link.strip()}],
        })
    return text, facets


def prepare_image(path):
    """Load an image and shrink it until it fits the blob limit. Returns (bytes, mime, width, height)."""
    p = pathlib.Path(path)
    if p.suffix.lower() not in IMAGE_TYPES:
        fail(f"{path}: unsupported image type (use png, jpg or webp)")
    if not p.is_file():
        fail(f"{path}: file not found")
    from PIL import Image
    im = Image.open(p)
    im.load()
    width, height = im.size
    raw = p.read_bytes()
    if len(raw) <= MAX_BLOB and width <= MAX_WIDTH:
        return raw, IMAGE_TYPES[p.suffix.lower()], width, height
    if width > MAX_WIDTH:
        height = round(height * MAX_WIDTH / width)
        width = MAX_WIDTH
        im = im.resize((width, height), Image.LANCZOS)
    rgb = im.convert("RGB")
    for quality in (90, 85, 80, 72, 64, 55):
        buf = io.BytesIO()
        rgb.save(buf, "JPEG", quality=quality, optimize=True)
        if buf.tell() <= MAX_BLOB:
            return buf.getvalue(), "image/jpeg", width, height
    fail(f"{path}: cannot shrink the image below {MAX_BLOB} bytes")


def split_list(value, sep):
    return [x.strip() for x in value.split(sep) if x.strip()] if value else []


def build_record(text, facets, images, alts, langs, now=None, blobs=None):
    now = now or datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")
    record = {"$type": "app.bsky.feed.post", "text": text, "langs": langs, "createdAt": now}
    if facets:
        record["facets"] = facets
    if images:
        record["embed"] = {
            "$type": "app.bsky.embed.images",
            "images": [
                {"alt": alt, "image": (blobs[i] if blobs else {"$type": "blob", "note": "uploaded when posting"}),
                 "aspectRatio": {"width": w, "height": h}}
                for i, (alt, (_, _, w, h)) in enumerate(zip(alts, images))
            ],
        }
    return record


def http_json(url, data=None, headers=None, method=None):
    req = urllib.request.Request(url, data=data, headers=headers or {}, method=method)
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.load(r)


def already_posted(handle, first_line):
    feed = http_json(f"{PUBLIC_API}/app.bsky.feed.getAuthorFeed?" + urllib.parse.urlencode(
        {"actor": handle, "limit": 30, "filter": "posts_no_replies"}))
    return any((e.get("post", {}).get("record", {}).get("text") or "").split("\n", 1)[0] == first_line
               for e in feed.get("feed", []))


def post(record_without_blobs, text, facets, images, alts, langs, handle, password):
    session = http_json(f"{API}/com.atproto.server.createSession",
                        json.dumps({"identifier": handle, "password": password}).encode(),
                        {"Content-Type": "application/json"})
    auth = {"Authorization": "Bearer " + session["accessJwt"]}
    blobs = []
    for data, mime, _, _ in images:
        out = http_json(f"{API}/com.atproto.repo.uploadBlob", data, {**auth, "Content-Type": mime})
        blobs.append(out["blob"])
    record = build_record(text, facets, images, alts, langs, blobs=blobs)
    body = json.dumps({"repo": session["did"], "collection": "app.bsky.feed.post", "record": record}).encode()
    return http_json(f"{API}/com.atproto.repo.createRecord", body, {**auth, "Content-Type": "application/json"})


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--title", required=True)
    ap.add_argument("--body", default="")
    ap.add_argument("--link", default="")
    ap.add_argument("--images", default="", help="comma-separated image paths (max 4)")
    ap.add_argument("--alts", default="", help="alt texts separated by |, one per image")
    ap.add_argument("--langs", default="de,en")
    ap.add_argument("--dry-run", action="store_true", help="compose and check only, no network access")
    ap.add_argument("--force", action="store_true", help="post even if a post with the same first line exists")
    a = ap.parse_args(argv)

    text, facets = compose(a.title, a.body, a.link)
    paths = split_list(a.images, ",")
    alts = split_list(a.alts, "|")
    if len(paths) > MAX_IMAGES:
        fail(f"{len(paths)} images given, Bluesky allows {MAX_IMAGES}")
    if len(alts) != len(paths):
        fail(f"{len(paths)} images but {len(alts)} alt texts; every image needs an alt text")
    images = [prepare_image(p) for p in paths]
    langs = split_list(a.langs, ",")

    print(f"text ({len(text)}/{MAX_TEXT} characters):\n{text}\n")
    for p, alt, (data, mime, w, h) in zip(paths, alts, images):
        print(f"image {p}: {w}x{h}, {len(data):,} bytes, {mime}, alt: {alt}")
    if a.dry_run:
        print("\ndry run: nothing was sent")
        return 0

    handle, password = os.environ.get("BLUESKY_HANDLE", ""), os.environ.get("BLUESKY_APP_PASSWORD", "")
    if not handle or not password:
        fail("BLUESKY_HANDLE and BLUESKY_APP_PASSWORD must be set")
    first = text.split("\n", 1)[0]
    if not a.force and already_posted(handle, first):
        print("an identical first line was already posted; skipping (use --force to post anyway)")
        return 0
    out = post(None, text, facets, images, alts, langs, handle, password)
    print("posted:", out.get("uri"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
