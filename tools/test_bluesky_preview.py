import io, pathlib, tempfile, unittest
import bluesky_preview as bp   # run with -s tools, so tools/ is on sys.path
try:
    from PIL import Image
except ImportError:  # pragma: no cover
    Image = None


class Compose(unittest.TestCase):
    def test_joins_title_body_and_link(self):
        text, facets = bp.compose("Preview v0.16.0", "Neuer Import", "https://example.org/x")
        self.assertEqual(text, "Preview v0.16.0\n\nNeuer Import\n\nhttps://example.org/x")
        self.assertEqual(len(facets), 1)

    def test_link_facet_uses_utf8_byte_offsets(self):
        title = "🖼️ Preview – Übersicht"
        text, facets = bp.compose(title, "", "https://e.org")
        idx = facets[0]["index"]
        self.assertEqual(text.encode("utf-8")[idx["byteStart"]:idx["byteEnd"]], b"https://e.org")
        self.assertGreater(idx["byteStart"], len(text.split("https")[0]))  # bytes > characters here

    def test_text_over_300_characters_fails(self):
        with self.assertRaises(SystemExit):
            bp.compose("x" * 301)

    def test_exactly_300_is_accepted(self):
        text, _ = bp.compose("x" * 300)
        self.assertEqual(len(text), 300)

    def test_title_required(self):
        with self.assertRaises(SystemExit):
            bp.compose("  ")


@unittest.skipIf(Image is None, "Pillow missing")
class Images(unittest.TestCase):
    def _png(self, d, name, size, noisy=False):
        import os
        im = Image.new("RGB", size, (200, 120, 90))
        if noisy:
            im = Image.frombytes("RGB", size, os.urandom(size[0] * size[1] * 3))
        p = pathlib.Path(d, name); im.save(p); return str(p)

    def test_small_image_is_passed_through_unchanged(self):
        with tempfile.TemporaryDirectory() as d:
            p = self._png(d, "a.png", (800, 450))
            data, mime, w, h = bp.prepare_image(p)
            self.assertEqual((mime, w, h), ("image/png", 800, 450))
            self.assertEqual(data, pathlib.Path(p).read_bytes())

    def test_large_image_is_shrunk_below_the_limit(self):
        with tempfile.TemporaryDirectory() as d:
            p = self._png(d, "big.png", (1800, 1200), noisy=True)
            self.assertGreater(pathlib.Path(p).stat().st_size, bp.MAX_BLOB)
            data, mime, w, h = bp.prepare_image(p)
            self.assertLessEqual(len(data), bp.MAX_BLOB)
            self.assertEqual(mime, "image/jpeg")
            self.assertEqual(Image.open(io.BytesIO(data)).size, (w, h))

    def test_wide_image_is_scaled_to_max_width_keeping_the_ratio(self):
        with tempfile.TemporaryDirectory() as d:
            p = self._png(d, "wide.png", (4000, 1000))
            _, _, w, h = bp.prepare_image(p)
            self.assertEqual((w, h), (bp.MAX_WIDTH, 500))

    def test_unknown_type_and_missing_file_fail(self):
        with self.assertRaises(SystemExit):
            bp.prepare_image("x.gif")
        with self.assertRaises(SystemExit):
            bp.prepare_image("/nonexistent/a.png")


class Record(unittest.TestCase):
    def test_images_embed_with_alt_and_aspect_ratio(self):
        imgs = [(b"x", "image/png", 1600, 900)]
        r = bp.build_record("t", [], imgs, ["Alt"], ["de", "en"], now="2026-10-04T00:00:00.000Z",
                            blobs=[{"$type": "blob", "ref": {"$link": "c"}, "mimeType": "image/png", "size": 1}])
        self.assertEqual(r["embed"]["$type"], "app.bsky.embed.images")
        self.assertEqual(r["embed"]["images"][0]["alt"], "Alt")
        self.assertEqual(r["embed"]["images"][0]["aspectRatio"], {"width": 1600, "height": 900})
        self.assertEqual(r["langs"], ["de", "en"])


class Cli(unittest.TestCase):
    def test_alt_text_count_must_match_images(self):
        with self.assertRaises(SystemExit):
            bp.main(["--title", "t", "--images", "a.png,b.png", "--alts", "one", "--dry-run"])

    def test_more_than_four_images_fail(self):
        with self.assertRaises(SystemExit):
            bp.main(["--title", "t", "--images", ",".join(f"{i}.png" for i in range(5)),
                     "--alts", "|".join("a" for _ in range(5)), "--dry-run"])

    @unittest.skipIf(Image is None, "Pillow missing")
    def test_dry_run_makes_no_network_call(self):
        import unittest.mock as m
        with tempfile.TemporaryDirectory() as d:
            p = pathlib.Path(d, "a.png"); Image.new("RGB", (640, 360)).save(p)
            with m.patch.object(bp, "http_json", side_effect=AssertionError("network")):
                self.assertEqual(bp.main(["--title", "T", "--images", str(p), "--alts", "Alt", "--dry-run"]), 0)


if __name__ == "__main__":
    unittest.main()
