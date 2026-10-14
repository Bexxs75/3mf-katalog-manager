import contextlib
import hashlib
import hmac
import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import time
import types
import unittest
from unittest.mock import patch
from urllib.parse import quote, urlencode

SCRIPT = Path(__file__).resolve().parents[2] / 'root/usr/local/libexec/3mf-katalog-web'


def load_web():
    loader = importlib.machinery.SourceFileLoader('katalog_web', str(SCRIPT))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


class Socket:
    def __init__(self, request):
        self.input = io.BytesIO(request)
        self.output = io.BytesIO()
        self.writes = []

    def makefile(self, *args):
        return self.input

    def sendall(self, data):
        self.writes.append(len(data))
        self.output.write(data)

    def settimeout(self, value):
        self.timeout = value


class WebTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not SCRIPT.exists():
            raise AssertionError('The catalog companion service must exist')
        cls.web = load_web()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        origin_patch = patch.object(self.web, 'PUBLIC_ORIGIN', self.base / 'public-origin')
        origin_patch.start()
        self.addCleanup(origin_patch.stop)
        self.root = self.base / 'models'
        self.root.mkdir()
        self.db = self.base / 'catalog.db'
        with contextlib.closing(sqlite3.connect(self.db)) as con, con:
            con.executescript('''
                CREATE TABLE files (id INTEGER PRIMARY KEY, name TEXT, path TEXT,
                    file_type TEXT, folder_id INTEGER, file_size_bytes INTEGER,
                    thumbnail_png BLOB, imported_at TEXT, deleted_at TEXT);
                CREATE TABLE folders (id INTEGER PRIMARY KEY, name TEXT);
                CREATE TABLE tags (id INTEGER PRIMARY KEY, name TEXT);
                CREATE TABLE file_tags (file_id INTEGER, tag_id INTEGER);
                INSERT INTO folders VALUES (1, 'Folder <one>'), (2, 'Two');
                INSERT INTO tags VALUES (1, 'Tag "one"'), (2, 'Second');
            ''')
        self.add_file(1, 'Ä " file.3mf', '3mf', b'first model')
        self.hidden = self.base / 'hidden.json'
        self.app = self.web.Catalog(str(self.db), [str(self.root)], b's' * 32, str(self.hidden))

    def add_file(self, fid, name, kind='stl', data=b'stl', path=None, deleted=None):
        path = Path(path) if path else self.root / str(fid)
        if not path.exists():
            path.write_bytes(data)
        with contextlib.closing(sqlite3.connect(self.db)) as con, con:
            con.execute('INSERT INTO files VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)',
                        (fid, name, str(path), kind, 1 if fid % 2 else 2,
                         len(data), b'PNG' if fid == 1 else None,
                         f'2026-10-{fid % 28 + 1:02}', deleted))
            con.execute('INSERT INTO file_tags VALUES (?, ?)', (fid, 1 if fid % 2 else 2))

    def request(self, path='/katalog/', method='GET', headers=None, body=b''):
        values = {'Host': 'catalog.example:3000', **(headers or {})}
        raw = f'{method} {path} HTTP/1.1\r\n' + ''.join(f'{k}: {v}\r\n' for k, v in values.items()) + '\r\n'
        sock = Socket(raw.encode() + body)
        log = io.StringIO()
        with contextlib.redirect_stdout(log), contextlib.redirect_stderr(log):
            self.web.Handler(sock, ('127.0.0.1', 1234), types.SimpleNamespace(catalog=self.app))
        head, body = sock.output.getvalue().split(b'\r\n\r\n', 1)
        lines = head.decode().split('\r\n')
        status = int(lines[0].split()[1])
        response_headers = dict(line.split(': ', 1) for line in lines[1:])
        self.assertIn('Content-Security-Policy', response_headers)
        self.assertEqual(response_headers['X-Content-Type-Options'], 'nosniff')
        self.assertEqual(response_headers['Referrer-Policy'], 'no-referrer')
        self.assertNotIn('Set-Cookie', response_headers)
        self.assertEqual(sock.timeout, 10)
        return status, response_headers, body, sock.writes, log.getvalue()

    def post(self, route, fields=None, headers=None, body=None, csrf=True):
        fields = dict(fields or {})
        if csrf and 'csrf' not in fields and 'id' in fields and str(fields['id']).isdigit():
            fields['csrf'] = self.app.csrf(str(fields['id']))
        data = urlencode(fields).encode() if body is None else body
        values = {'Content-Type': 'application/x-www-form-urlencoded', 'Content-Length': str(len(data)), **(headers or {})}
        values = {k: v for k, v in values.items() if v is not None}
        return self.request('/katalog/' + route, 'POST', values, data)

    def hide(self, fid=1, **kwargs):
        response = self.post('remove', {'id': fid}, **kwargs)
        self.assertEqual(response[0], 303, response)
        return response

    def signed(self, fid='1', token=None, name='different.stl'):
        return f'/katalog/signed/{fid}/{token or self.app.token(fid)}/{name}'

    def upload(self, name='model.stl', data=b'solid model', headers=None):
        self.app.upload_dir = str(self.root / 'Eingang')
        values = {'Content-Type': 'application/octet-stream', 'Content-Length': str(len(data)),
                  'X-Filename': quote(name, safe=''),
                  'X-CSRF-Token': hmac.new(self.app.secret, b'upload|csrf', hashlib.sha256).hexdigest(),
                  **(headers or {})}
        return self.request('/katalog/upload', 'POST', {k: v for k, v in values.items() if v is not None}, data)

    def test_upload_list_display_structure(self):
        for key in ('upload_clear', 'upload_summary'):
            self.assertEqual(len(self.web.TEXT[key]), 4)
            self.assertEqual(len(set(self.web.TEXT[key])), 4)
        for text in self.web.TEXT['upload_summary']:
            self.assertIn('{ok}', text)
            self.assertIn('{bad}', text)
        self.assertEqual(self.web.TEXT['upload_clear'][1], 'Clear list')
        # --good must exist in light, media-dark and forced-dark blocks.
        self.assertEqual(self.web.STYLE.count('--good: oklch(0.515 0.13 150)'), 1)
        self.assertEqual(self.web.STYLE.count('--good: oklch(0.72 0.14 150)'), 2)
        for rule in ('li[data-state=done] [role=status] {border-left-color:var(--good)}',
                     'li[data-state=error] [role=status] {border-left-color:var(--crit)}',
                     'li[data-state=done] progress {accent-color:var(--good)}',
                     'li[data-state=error] progress'):
            self.assertIn(rule, self.web.STYLE)
        script = self.web.UPLOAD_SCRIPT
        # The cancel button is created visible, and hidden on every terminal state via finish().
        self.assertIn("row.dataset.state = 'waiting'", script)
        self.assertIn("item.row.dataset.state = 'loading'", script)
        self.assertIn('item.cancel.hidden = true', script)
        self.assertNotIn('cancel.disabled', script)
        for state in ("'done'", "'error'", "'cancelled'"):
            self.assertIn('finish(', script)
            self.assertIn(state, script)
        self.assertIn('t.upload_summary', script)
        self.assertIn("clear.onclick", script)
        self.assertIn('choose.focus()', script)

    def test_upload_atomic_collision_and_permissions(self):
        for expected in ('model.stl', 'model (2).stl'):
            status, headers, body, _, log = self.upload()
            self.assertEqual(status, 200, body)
            self.assertEqual(json.loads(body), {'ok': True, 'name': expected, 'size': 11})
            target = self.root / 'Eingang' / expected
            self.assertEqual(target.read_bytes(), b'solid model')
            self.assertEqual(target.stat().st_mode & 0o777, 0o644)
            self.assertEqual(headers['Content-Type'], 'application/json')
            self.assertEqual(headers['Cache-Control'], 'no-store')
            self.assertEqual(log.strip(), 'POST /katalog/upload 200')
        self.assertFalse(list((self.root / 'Eingang').glob('.upload-*.part')))

    def test_upload_names(self):
        for name, expected in [('../x.stl', 400), ('a/b.stl', 400), ('a\\b.stl', 400),
                               ('/x.stl', 400), ('..', 400), ('', 400), ('.x.stl', 400),
                               ('a\x00.stl', 400), ('a\x1f.stl', 400), (' x.stl', 400),
                               ('x.stl ', 400), ('x.stl.', 400), ('a' * 177 + '.stl', 400),
                               ('界' * 80 + '.stl', 400), ('x.3mf.exe', 415), ('x.txt', 415)]:
            with self.subTest(name=name):
                self.assertEqual(self.upload(name)[0], expected)
        self.assertEqual(json.loads(self.upload('e\u0301.stl')[2])['name'], 'é.stl')

    def test_upload_signatures(self):
        good = {'3mf': b'PK\x03\x04', 'zip': b'PK\x03\x04', '7z': b"7z\xbc\xaf\x27\x1c",
                'rar': b'Rar!\x1a\x07', 'stp': b' \nISO-10303-21;', 'step': b'ISO-10303-21;',
                'stl': b'solid x', 'obj': b'v 1 2 3'}
        for ext, data in good.items():
            with self.subTest(ext=ext):
                self.assertEqual(self.upload('x.' + ext, data)[0], 200)
                for invalid in (b'MZbad', b'\x7fELFbad', b'#!bad'):
                    self.assertEqual(self.upload('bad.' + ext, invalid)[0], 415)
        self.assertEqual(self.upload(data=b'')[0], 400)
        self.assertFalse(list((self.root / 'Eingang').glob('.upload-*.part')))

    def test_upload_length_auth_origin(self):
        for headers, status in [({'Content-Length': None}, 411), ({'Content-Length': 'x'}, 411),
                                 ({'Content-Length': '-1'}, 411), ({'Content-Length': '9999999999'}, 413),
                                 ({'Content-Type': 'text/plain'}, 415), ({'X-CSRF-Token': None}, 403),
                                 ({'X-CSRF-Token': 'wrong'}, 403), ({'X-CSRF-Token': self.app.csrf('1')}, 403),
                                 ({'Origin': 'https://evil.test'}, 403), ({'Referer': 'https://evil.test/x'}, 403),
                                 ({'Transfer-Encoding': 'chunked'}, 400), ({'Content-Length': '20'}, 400)]:
            with self.subTest(headers=headers):
                self.assertEqual(self.upload(headers=headers)[0], status)
        self.assertEqual(self.upload(headers={'Origin': 'null'})[0], 200)
        self.assertEqual(self.upload(headers={'Origin': 'http://catalog.example:3000'})[0], 200)

    def test_upload_streaming_and_early_signature_rejection(self):
        self.app.upload_dir = str(self.root / 'Eingang')
        class Generated:
            def __init__(self, invalid=False):
                self.calls = []
                self.invalid = invalid
            def read(self, size):
                self.calls.append(size)
                return (b'MZ' if self.invalid else b'xx') + b'x' * (size - 2)
        stream = Generated()
        result = self.app.receive_upload(stream, 5 * 1024 * 1024, 'large.stl')
        self.assertEqual(result['size'], 5 * 1024 * 1024)
        self.assertEqual(stream.calls, [1024 * 1024] * 5)
        bad = Generated(True)
        with self.assertRaises(self.web.UploadError):
            self.app.receive_upload(bad, 5 * 1024 * 1024, 'bad.stl')
        self.assertEqual(len(bad.calls), 1)
        self.assertFalse(list((self.root / 'Eingang').glob('.upload-*.part')))

    def test_upload_surplus_and_short_body_cleanup(self):
        self.assertEqual(self.upload(headers={'Content-Length': '3'})[0], 400)
        self.assertEqual(self.upload(headers={'Content-Length': '100'})[0], 400)
        self.assertFalse(list((self.root / 'Eingang').iterdir()))
        class OversizedRead:
            def read(self, size):
                return b'x' * (size + 1)
        with self.assertRaises(self.web.UploadError) as raised:
            self.app.receive_upload(OversizedRead(), 10, 'bad.stl')
        self.assertEqual(raised.exception.code, 'length_mismatch')

    def test_upload_space_write_failure_and_conflict(self):
        self.app.upload_dir = str(self.root / 'Eingang')
        space = types.SimpleNamespace(f_bavail=0, f_frsize=4096, f_flag=0)
        with patch.object(self.web.os, 'statvfs', return_value=space):
            response = self.upload()
            self.assertEqual(response[0], 409)
            self.assertEqual(json.loads(response[2])['error'], 'no_space')
        with patch.object(self.web.os, 'fsync', side_effect=OSError):
            self.assertEqual(self.upload()[0], 503)
        with patch.object(self.web.os, 'link', side_effect=FileExistsError):
            self.assertEqual(self.upload()[0], 409)
        (self.root / 'Eingang').rmdir()
        with patch.object(self.web.os, 'makedirs', side_effect=PermissionError):
            self.assertEqual(self.upload()[0], 503)
        self.assertFalse((self.root / 'Eingang').exists())

    def test_upload_slots_and_timeout_cleanup(self):
        import threading
        self.app.upload_dir = str(self.root / 'Eingang')
        entered = threading.Barrier(3)
        release = threading.Event()
        original = self.app.receive_upload
        def hold(*args, **kwargs):
            entered.wait(timeout=5)
            release.wait(timeout=5)
            return original(*args, **kwargs)
        # Separate handlers avoid redirecting process-wide stdout in worker threads.
        def request():
            data = b'solid x'
            raw = ('POST /katalog/upload HTTP/1.1\r\nHost: catalog.example:3000\r\n'
                   'Content-Type: application/octet-stream\r\nContent-Length: 7\r\n'
                   'X-Filename: same.stl\r\nX-CSRF-Token: ' + self.app.upload_csrf() + '\r\n\r\n').encode() + data
            sock = Socket(raw)
            self.web.Handler(sock, ('127.0.0.1', 1234), types.SimpleNamespace(catalog=self.app))
            return sock.output.getvalue()
        results = []
        with patch.object(self.app, 'receive_upload', side_effect=hold), patch.object(self.web.Handler, 'log_request'):
            workers = [threading.Thread(target=lambda: results.append(request())) for _ in range(2)]
            for worker in workers:
                worker.start()
            try:
                entered.wait(timeout=5)
                self.assertEqual(self.upload()[0], 429)
            finally:
                release.set()
                for worker in workers:
                    worker.join(timeout=5)
        self.assertEqual(len(results), 2)
        self.assertTrue(all(b'200 OK' in result for result in results))
        self.assertTrue((self.root / 'Eingang/same (2).stl').exists())
        class Interrupted:
            def read(self, size):
                raise TimeoutError
        with self.assertRaises(TimeoutError):
            self.app.receive_upload(Interrupted(), 10, 'timeout.stl')
        self.assertFalse(list((self.root / 'Eingang').glob('.upload-*.part')))
        self.assertEqual(self.upload()[0], 200)

    def test_upload_lazy_directory_cleanup_and_configuration(self):
        target = self.root / 'Eingang'
        with patch.dict(os.environ, {'KATALOG_UPLOAD_DIR': str(target), 'KATALOG_UPLOAD_MAX_MB': ''}):
            app = self.web.Catalog(str(self.db), [str(self.root)], b's' * 32, str(self.hidden))
            self.assertEqual(app.upload_max, 2048 * 1024 * 1024)
            self.assertFalse(target.exists())
            target.mkdir()
            old = target / '.upload-old.part'
            old.write_bytes(b'old')
            os.utime(old, (time.time() - 7200,) * 2)
            fresh = target / '.upload-fresh.part'
            fresh.write_bytes(b'new')
            model = target / 'keep.stl'
            model.write_bytes(b'old')
            os.utime(model, (time.time() - 7200,) * 2)
            self.web.Catalog(str(self.db), [str(self.root)], b's' * 32, str(self.hidden))
            self.assertFalse(old.exists())
            self.assertTrue(fresh.exists())
            self.assertTrue(model.exists())
        with patch.dict(os.environ, {'KATALOG_UPLOAD_DIR': '', 'KATALOG_UPLOAD_MAX_MB': ''}):
            app = self.web.Catalog(str(self.db), [], b's' * 32, str(self.hidden))
            self.assertEqual(app.upload_dir, '/models/Eingang')

    def test_upload_disabled_symlink_permissions_readonly(self):
        (self.root / 'Eingang').symlink_to(self.base, target_is_directory=True)
        self.assertEqual(self.upload()[0], 404)
        self.assertNotIn(b'id="upload-zone"', self.request()[2])
        (self.root / 'Eingang').unlink()
        with patch.object(self.web.os, 'access', return_value=False):
            self.assertEqual(self.upload()[0], 404)
            self.assertNotIn(b'id="upload-zone"', self.request()[2])
        space = types.SimpleNamespace(f_flag=os.ST_RDONLY)
        with patch.object(self.web.os, 'statvfs', return_value=space):
            self.assertEqual(self.upload()[0], 404)

    def test_upload_directory_creation_cannot_follow_swapped_parent(self):
        parent = self.root / 'parent'
        parent.mkdir()
        outside = self.base / 'outside'
        outside.mkdir()
        self.app.upload_dir = str(parent / 'child')
        original = self.web.os.makedirs
        swapped = False
        def swap_then_create(path, *args, **kwargs):
            nonlocal swapped
            if not swapped:
                swapped = True
                parent.rename(self.root / 'parked')
                parent.symlink_to(outside, target_is_directory=True)
            return original(path, *args, **kwargs)
        with patch.object(self.web.os, 'makedirs', side_effect=swap_then_create):
            try:
                self.app.receive_upload(io.BytesIO(b'solid x'), 7, 'safe.stl')
            except (self.web.UploadError, OSError):
                pass
        self.assertTrue(swapped)
        self.assertFalse((outside / 'child').exists())

    def test_upload_html_and_languages(self):
        self.app.upload_dir = str(self.root / 'Eingang')
        for index, lang in enumerate(self.web.LANGS):
            body = self.request('/katalog/?lang=' + lang)[2].decode()
            self.assertIn('id="upload-zone"', body)
            self.assertIn(self.app.upload_csrf(), body)
            self.assertIn(self.web.TEXT['upload_title'][index], body)
            self.assertIn('type="file" multiple', body)
            self.assertIn('<noscript>', body)
        self.assertFalse((self.root / 'Eingang').exists())
        for key, values in self.web.TEXT.items():
            self.assertEqual(len(values), 4, key)
            self.assertTrue(all(values), key)

    def test_static_assets_allowlist(self):
        for name, kind, cache in [('fonts/barlow-400.woff2', 'font/woff2', 'public, max-age=31536000, immutable'),
                                  ('fonts/barlow-700.woff2', 'font/woff2', 'public, max-age=31536000, immutable'),
                                  ('icon-128.png', 'image/png', 'public, max-age=86400'),
                                  ('icon-32.png', 'image/png', 'public, max-age=86400')]:
            status, headers, body, _, _ = self.request('/katalog/static/' + name)
            self.assertEqual(status, 200, name)
            self.assertEqual(headers['Content-Type'], kind)
            self.assertEqual(headers['Cache-Control'], cache)
            self.assertTrue(body)
        for name in ['fonts/barlow-300.woff2', 'fonts/', '../libexec/3mf-katalog-web', 'fonts/../icon-32.png',
                     '%2e%2e/x', 'icon-32.png/x', 'barlow-400.woff2', 'fonts/OFL.txt', '']:
            self.assertEqual(self.request('/katalog/static/' + name)[0], 404, name)

    def test_csp_allows_only_own_fonts(self):
        _, headers, _, _, _ = self.request()
        self.assertEqual(headers['Content-Security-Policy'],
                         "default-src 'none'; img-src 'self'; font-src 'self'; style-src 'self' 'unsafe-inline'; "
                         "script-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'")

    def test_theme_and_branding(self):
        _, _, body, _, _ = self.request()
        html = body.decode()
        self.assertIn('--accent: oklch(0.545 0.165 30)', html)
        self.assertIn('@media (prefers-color-scheme: dark)', html)
        self.assertIn("font-family:'Barlow',Helvetica,sans-serif", html)
        self.assertIn('font-display:swap', html)
        self.assertIn('<link rel="icon" type="image/png" sizes="32x32" href="/katalog/static/icon-32.png">', html)
        self.assertIn('<img src="/katalog/static/icon-128.png" width="30" height="30" alt="">', html)
        self.assertIn('<a class="brand" href="/katalog/">', html)

    def test_thumbnail_placeholder(self):
        self.add_file(2, 'plain.stl')
        _, _, body, _, _ = self.request()
        html = body.decode()
        self.assertEqual(html.count('class="ph" aria-hidden="true"'), 1)
        self.assertIn('<span>STL</span>', html)
        self.assertIn('<img src="/katalog/thumb/1"', html)
        _, _, body, _, _ = self.request('/katalog/?file=1')
        self.assertNotIn(b'class="ph"', body)
        _, _, body, _, _ = self.request('/katalog/?file=2')
        self.assertIn(b'class="ph" aria-hidden="true"', body)

    def test_header_links_per_language(self):
        for lang, expected in [('de', ['https://wiki.3mfkatalog.de/"', 'https://3mfkatalog.de/"', '>Homepage<']),
                               ('en', ['https://wiki.3mfkatalog.de/en/"', 'https://3mfkatalog.de/en/"', '>Website<'])]:
            _, _, body, _, _ = self.request(headers={'Accept-Language': lang})
            html = body.decode()
            header = html[html.index('<header'):html.index('</header>')]
            for item in expected + ['https://discord.gg/abfVNfFqu3"', '>Wiki<', '>Discord<']:
                self.assertIn(item, header, (lang, item))
            self.assertEqual(header.count('target="_blank" rel="noopener noreferrer"'), 3)

    def test_style_block_has_balanced_braces_so_the_dark_media_query_survives(self):
        # A stray "}" makes browsers drop the following @media block (dark mode stays light).
        _, _, body, _, _ = self.request()
        css = body.decode().split('<style>', 1)[1].split('</style>', 1)[0]
        self.assertEqual(css.count('{'), css.count('}'))
        depth = 0
        for char in css:
            depth += (char == '{') - (char == '}')
            self.assertGreaterEqual(depth, 0)

    def test_notice_box_sits_above_the_list_and_keeps_the_hints(self):
        _, _, body, _, _ = self.request(headers={'Accept-Language': 'en'})
        html = body.decode()
        box = html[html.index('<aside class="notice"'):html.index('</aside>')]
        for text in ('Notice', 'Links expire after 1 hour', 'PrusaSlicer: download the file', 'Slicer help'):
            self.assertIn(text, box)
        self.assertLess(html.index('<aside class="notice"'), html.index('<ul class="files">'))
        self.assertEqual(html.count('Links expire after 1 hour'), 1)

    def test_theme_toggle_script_and_sticky_bar(self):
        _, _, body, _, _ = self.request(headers={'Accept-Language': 'en'})
        html = body.decode()
        head = html[:html.index('</head>')]
        self.assertIn("localStorage.getItem('3mf-theme')", head)
        self.assertIn("setAttribute('data-theme'", head)
        self.assertIn('id="theme-toggle"', html)
        self.assertIn('data-label="Color scheme"', html)
        self.assertIn('position:sticky;top:0', html)
        self.assertIn(':root:not([data-theme="light"])', html)
        self.assertIn(':root[data-theme="dark"]', html)
        self.assertIn('scroll-padding-top', html)
        # Single view keeps the back link in the content, not in the bar.
        _, _, body, _, _ = self.request('/katalog/?file=1')
        html = body.decode()
        self.assertNotIn('Alle Dateien', html[html.index('<header'):html.index('</header>')])
        self.assertIn('Alle Dateien anzeigen', html)

    def test_single_file_page(self):
        self.add_file(2, 'other.stl')
        status, _, body, _, _ = self.request('/katalog/?file=1')
        self.assertEqual(status, 200)
        self.assertIn(b'<details open>', body)
        self.assertIn(b'/katalog/download/1', body)
        self.assertNotIn(b'other.stl', body)
        self.assertIn(b'Alle Dateien anzeigen', body)
        self.add_file(3, 'deleted', deleted='yesterday')
        self.add_file(4, 'outside', path=self.base / 'outside')
        (self.root / '2').unlink()
        for fid in ['2', '3', '4', '999', 'abc', '', '-1', '1%20', '%D9%A1', '1&file=2']:
            with self.subTest(fid=fid):
                status, _, body, _, _ = self.request('/katalog/?file=' + fid)
                self.assertEqual(status, 404)
                self.assertIn(b'Alle Dateien anzeigen', body)
                self.assertNotIn(b'/katalog/download/', body)

    def test_public_origin_is_atomic_and_only_written_on_change(self):
        target = self.base / 'public-origin'
        with patch.object(self.web, 'PUBLIC_ORIGIN', target):
            with patch.object(self.web.os, 'replace', wraps=os.replace) as replace:
                self.assertEqual(self.request()[0], 200)
                self.assertEqual(target.read_text(), 'http://catalog.example:3000')
                self.request()
                self.assertEqual(replace.call_count, 1)
                self.request(self.signed(), headers={'Host': 'other.example'})
                self.request('/katalog/download/1', headers={'Host': 'other.example'})
                self.assertEqual(replace.call_count, 1)
                self.request(headers={'Host': 'bad/host'})
                self.assertEqual(replace.call_count, 1)
                self.request(headers={'X-Forwarded-Proto': 'https'})
                self.assertEqual(target.read_text(), 'https://catalog.example:3000')
                self.assertEqual(replace.call_count, 2)
                self.assertEqual(Path(replace.call_args.args[0]).parent, target.parent)
            with patch.object(self.web.os, 'replace', side_effect=OSError('read only')):
                self.assertEqual(self.request()[0], 200)
            self.assertEqual(list(self.base.glob('.public-origin-*')), [])

    def test_signature_and_filename_ignored(self):
        token = self.app.token('1')
        expiry, digest = token.split('.')
        self.assertAlmostEqual(int(expiry), int(time.time()) + 3600, delta=2)
        self.assertEqual(digest, hmac.new(b's' * 32, f'1|{expiry}|download'.encode(), hashlib.sha256).hexdigest())
        status, headers, body, _, log = self.request(self.signed(token=token))
        self.assertEqual((status, body), (200, b'first model'))
        self.assertTrue(headers['Content-Disposition'].startswith('inline;'))
        self.assertNotIn(token, log)
        for bad in [token + 'a', 'invalid', '0.' + digest, expiry + '.nope', str(int(expiry) + 1) + '.' + digest, '.']:
            with self.subTest(token=bad):
                self.assertEqual(self.request(self.signed(token=bad))[0], 403)
        self.assertEqual(self.request(self.signed(fid='2', token=token))[0], 403)
        with patch.object(self.web.time, 'time', return_value=int(expiry)):
            self.assertEqual(self.request(self.signed(token=token))[0], 403)
        self.app.secret = b'x' * 32
        self.assertEqual(self.request(self.signed(token=token))[0], 403)
        self.assertEqual(self.request('/katalog/signed/1/')[0], 403)

    def test_path_safety_and_deleted_files(self):
        outside = self.base / 'outside'
        outside.write_bytes(b'secret')
        link = self.root / 'link'
        link.symlink_to(outside)
        self.add_file(2, 'outside', path=outside)
        self.add_file(3, 'symlink', path=link)
        self.add_file(4, 'dotdot', path=str(self.root) + '/../outside')
        self.add_file(5, 'deleted', deleted='yesterday')
        for fid in ['2', '3', '4', '5', '999']:
            for path in [f'/katalog/download/{fid}', self.signed(fid=fid), f'/katalog/thumb/{fid}']:
                with self.subTest(path=path):
                    self.assertEqual(self.request(path)[0], 404)
        for fid in ['x', '-1', '1%20', '1' * 21, '../1']:
            self.assertEqual(self.request('/katalog/download/' + fid)[0], 404)
        html = self.request()[2].decode()
        self.assertNotIn('deleted', html)
        self.assertIn('outside', html)
        self.assertNotIn('/katalog/download/2', html)

    def test_download_stream_head_and_mime(self):
        data = b'X' * (self.web.BLOCK_SIZE * 3 + 17)
        self.add_file(2, 'large.stl', data=data)
        status, headers, body, writes, _ = self.request('/katalog/download/2')
        self.assertEqual((status, body), (200, data))
        self.assertEqual(int(headers['Content-Length']), len(data))
        self.assertLessEqual(max(writes), self.web.BLOCK_SIZE)
        self.assertGreaterEqual(len(writes), 5)
        self.assertEqual(headers['Content-Type'], 'model/stl')
        status, head, body, _, _ = self.request('/katalog/download/2', 'HEAD')
        self.assertEqual((status, body, head['Content-Length']), (200, b'', str(len(data))))
        head = self.request('/katalog/download/1')[1]
        self.assertEqual(head['Content-Disposition'], "attachment; filename*=UTF-8''%C3%84%20%22%20file.3mf")
        for fid, kind, mime in [(3, 'obj', 'model/obj'), (4, 'stp', 'model/step'), (5, 'other', 'application/octet-stream')]:
            self.add_file(fid, 'file', kind)
            self.assertEqual(self.request(f'/katalog/download/{fid}')[1]['Content-Type'], mime)

    def test_html_escaping_slicers_and_origin(self):
        self.add_file(2, '<script>alert("x")</script>.stl')
        html = self.request(headers={'X-Forwarded-Proto': 'https'})[2].decode()
        self.assertIn('&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;', html)
        self.assertIn('Folder &lt;one&gt;', html)
        self.assertIn('Tag &quot;one&quot;', html)
        self.assertIn('https%3A%2F%2Fcatalog.example%3A3000%2Fkatalog%2Fsigned%2F', html)
        # Bambu Studio is offered for the 3MF file only: an STL does not open through its link.
        self.assertEqual(html.count('>Bambu Studio</a>'), 1)
        self.assertEqual(html.count('>SuperSlicer</a>'), 2)
        self.assertNotIn('>PrusaSlicer</a>', html)
        self.assertIn('bambustudioopen://', html)
        self.assertIn('lycheeslicer://open/', html)
        self.add_file(3, 'step', 'stp')
        step = self.request('/katalog/?type=stp')[2].decode()
        self.assertNotIn('>Cura</a>', step)
        self.assertNotIn('>Lychee</a>', step)
        self.assertNotIn('>Bambu Studio</a>', step)
        self.assertIn('>SuperSlicer</a>', step)
        for host in ['bad"host', 'user@host', 'host/path', 'host:99999', '']:
            self.assertEqual(self.request(headers={'Host': host})[0], 400)
        self.assertEqual(self.request(headers={'X-Forwarded-Proto': 'javascript'})[0], 400)

    def test_filters_sort_pagination(self):
        self.add_file(2, 'Zebra.stl', data=b'zzzz')
        self.add_file(3, 'Alpha.stl')
        for query, included, excluded in [('q=Zebra', 'Zebra.stl', 'Alpha.stl'), ('type=3mf', 'file.3mf', 'Alpha.stl'), ('folder=2', 'Zebra.stl', 'Alpha.stl'), ('tag=2', 'Zebra.stl', 'Alpha.stl')]:
            html = self.request('/katalog/?' + query)[2].decode()
            self.assertIn(included, html)
            self.assertNotIn(excluded, html)
        html = self.request('/katalog/?sort=name')[2].decode()
        self.assertLess(html.index('Alpha.stl'), html.index('Zebra.stl'))
        html = self.request('/katalog/?sort=newest')[2].decode()
        self.assertLess(html.index('Alpha.stl'), html.index('Zebra.stl'))
        html = self.request('/katalog/?sort=size')[2].decode()
        self.assertLess(html.index('Zebra.stl'), html.index('Alpha.stl'))
        for fid in range(4, 55):
            self.add_file(fid, f'item{fid:03}.stl')
        html = self.request('/katalog/?type=stl&page=2')[2].decode()
        self.assertEqual(html.count('/katalog/download/'), 3)
        self.assertIn('type=stl', html)
        self.assertEqual(self.request('/katalog/?page=nonsense')[0], 200)
        self.assertEqual(self.request('/katalog/?q=%27%20OR%201%3D1--')[2].decode().count('/katalog/download/'), 0)

    def test_missing_old_database_language_health_methods(self):
        self.app.db_path = str(self.base / 'missing.db')
        self.assertIn('Katalog noch nicht angelegt', self.request()[2].decode())
        self.assertIn('Catalog not created', self.request(headers={'Accept-Language': 'en-US,en;q=0.9,de;q=0.5'})[2].decode())
        self.assertFalse(Path(self.app.db_path).exists())
        self.assertEqual(self.request('/katalog/health')[2], b'ok\n')
        self.assertEqual(self.request('/katalog/download/1')[0], 404)
        self.app.db_path = str(self.db)
        with contextlib.closing(sqlite3.connect(self.db)) as con, con:
            con.execute('ALTER TABLE files DROP COLUMN deleted_at')
        self.assertEqual(self.request()[0], 200)
        self.assertEqual(self.request('/katalog/download/1')[0], 404)
        for method in ['POST', 'PUT', 'DELETE', 'OPTIONS', 'PATCH', 'WHATEVER']:
            response = self.request(method=method)
            self.assertEqual(response[0], 405)
            self.assertEqual(response[1]['Allow'], 'GET, HEAD')
        self.assertEqual(self.request('/katalog/?q=' + 'x' * 9000)[0], 414)

    def test_thumbnail_private_and_secret_file(self):
        status, headers, body, _, _ = self.request('/katalog/thumb/1')
        self.assertEqual((status, body), (200, b'PNG'))
        self.assertEqual(headers['Cache-Control'], 'private')
        self.assertEqual(headers['Content-Type'], 'image/png')
        secret_path = self.base / 'key'
        key = self.web.load_secret(secret_path)
        self.assertEqual(len(key), 32)
        self.assertEqual(secret_path.stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.web.load_secret(secret_path), key)
        self.assertEqual(secret_path.stat().st_uid, os.getuid())

    def test_allowed_root_symlink_cannot_expand_scope(self):
        outside = self.base / 'private'
        outside.mkdir()
        secret_file = outside / 'model.stl'
        secret_file.write_bytes(b'private')
        alias = self.base / 'models2'
        alias.symlink_to(outside, target_is_directory=True)
        self.add_file(2, 'private.stl', path=alias / 'model.stl')
        self.app = self.web.Catalog(str(self.db), [str(alias)], b's' * 32, str(self.hidden))
        self.assertEqual(self.request('/katalog/download/2')[0], 404)

    def test_download_filename_strips_controls(self):
        with contextlib.closing(sqlite3.connect(self.db)) as con, con:
            con.execute('UPDATE files SET name=? WHERE id=1', ('a\r\n\t\x85.3mf',))
        header = self.request('/katalog/download/1')[1]['Content-Disposition']
        self.assertEqual(header, "attachment; filename*=UTF-8''a.3mf")

    def test_schema_from_checkout_and_missing_table(self):
        schema = Path(__file__).resolve().parents[3] / 'src-tauri/src/db/schema.sql'
        full_db = self.base / 'full.db'
        with contextlib.closing(sqlite3.connect(full_db)) as con, con:
            con.executescript(schema.read_text())
            con.execute('INSERT INTO files (id, name, path, file_type, file_size_bytes, imported_at) VALUES (1, ?, ?, ?, 11, ?)',
                        ('actual.3mf', str(self.root / '1'), '3mf', '2026-10-11'))
        self.app.db_path = str(full_db)
        self.assertIn('actual.3mf', self.request()[2].decode())
        self.assertEqual(self.request('/katalog/download/1')[2], b'first model')
        with contextlib.closing(sqlite3.connect(full_db)) as con, con:
            con.execute('DROP TABLE tags')
        self.assertIn('Katalog noch nicht angelegt', self.request()[2].decode())

    def test_database_read_only_and_default_discovery(self):
        with self.app.connect() as con:
            with self.assertRaises(sqlite3.OperationalError):
                con.execute('DELETE FROM files')
        share = self.base / 'share'
        for name in ['stable', 'preview']:
            (share / name).mkdir(parents=True)
            (share / name / 'catalog.db').touch()
        os.utime(share / 'stable/catalog.db', (1, 1))
        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(self.web.discover_db(share), str(share / 'preview/catalog.db'))
        with patch.dict(os.environ, {'CATALOG_DB': str(self.db)}):
            self.assertEqual(self.web.discover_db(share), str(self.db))

    def test_hide_removes_from_every_route_and_keeps_data(self):
        self.add_file(2, 'other.stl')
        signed = self.signed()
        self.assertEqual(self.request(signed)[0], 200)
        before = self.db.read_bytes()
        status, headers, _, _, log = self.hide(1, headers={'Origin': 'http://catalog.example:3000'})
        self.assertEqual(headers['Location'], '/katalog/?removed=1')
        self.assertEqual(headers['Cache-Control'], 'no-store')
        self.assertEqual(log, 'POST /katalog/remove 303\n')
        html = self.request()[2].decode()
        self.assertNotIn('/katalog/download/1"', html)
        self.assertIn('/katalog/download/2"', html)
        for path in ['/katalog/download/1', '/katalog/thumb/1', '/katalog/?file=1', signed]:
            with self.subTest(path=path):
                self.assertEqual(self.request(path)[0], 404)
        self.assertEqual(self.request('/katalog/?q=file.3mf')[2].decode().count('/katalog/download/'), 0)
        self.assertEqual(self.request('/katalog/?type=3mf')[2].decode().count('/katalog/download/'), 0)
        self.assertEqual((self.root / '1').read_bytes(), b'first model')
        self.assertEqual(self.db.read_bytes(), before)
        with contextlib.closing(sqlite3.connect(self.db)) as con:
            self.assertEqual(con.execute('SELECT count(*) FROM files WHERE id=1 AND deleted_at IS NULL').fetchone()[0], 1)
        status, headers, _, _, _ = self.post('restore', {'id': 1})
        self.assertEqual((status, headers['Location']), (303, '/katalog/'))
        self.assertEqual(self.request(signed)[0], 200)
        self.assertEqual(self.request('/katalog/download/1')[0], 200)
        self.assertIn('/katalog/download/1"', self.request()[2].decode())

    def test_pagination_counts_ignore_hidden_files(self):
        for fid in range(2, 54):
            self.add_file(fid, f'item{fid:03}.stl')
        self.assertIn('page=2', self.request()[2].decode())
        for fid in range(2, 6):
            self.hide(fid)
        html = self.request('/katalog/?page=2')[2].decode()
        self.assertEqual(html.count('/katalog/download/'), 0)
        self.assertNotIn('page=2', self.request()[2].decode())

    def test_hidden_view_count_and_restore_forms(self):
        self.add_file(2, 'Two.stl')
        html = self.request(headers={'Accept-Language': 'en'})[2].decode()
        self.assertNotIn('Show hidden files', html)
        self.hide(2)
        html = self.request(headers={'Accept-Language': 'en'})[2].decode()
        self.assertIn('Show hidden files (1)', html)
        self.assertIn('Ausgeblendete anzeigen (1)', self.request()[2].decode())
        view = self.request('/katalog/?hidden=1', headers={'Accept-Language': 'en'})[2].decode()
        self.assertIn('Two.stl', view)
        self.assertNotIn('file.3mf', view)
        self.assertIn('Show again', view)
        self.assertIn('action="/katalog/restore"', view)
        self.assertIn('Back to the download list', view)
        for text in ('/katalog/download/', 'Open in slicer', 'Remove from list', 'orcaslicer://'):
            self.assertNotIn(text, view)
        # A stale id and a catalog-deleted file do not count.
        self.add_file(3, 'gone.stl', deleted='yesterday')
        self.hidden.write_text('[2, 3, 999]')
        self.assertIn('Show hidden files (1)', self.request(headers={'Accept-Language': 'en'})[2].decode())
        self.post('restore', {'id': 2})
        self.assertNotIn('Show hidden', self.request(headers={'Accept-Language': 'en'})[2].decode())
        self.assertIn('No hidden files.', self.request('/katalog/?hidden=1', headers={'Accept-Language': 'en'})[2].decode())

    def test_restore_from_hidden_view_returns_to_list_when_empty(self):
        self.hide(1)
        status, headers, *_ = self.post('restore', {'id': 1, 'next': '/katalog/?hidden=1'})
        self.assertEqual(headers['Location'], '/katalog/')
        self.hide(1)
        self.add_file(2, 'b.stl')
        self.hide(2)
        status, headers, *_ = self.post('restore', {'id': 1, 'next': '/katalog/?hidden=1'})
        self.assertEqual(headers['Location'], '/katalog/?hidden=1')

    def test_csrf_and_origin_checks(self):
        for fields in [{'id': 1, 'csrf': 'x' * 64}, {'id': 1, 'csrf': ''}, {'id': 1, 'csrf': self.app.csrf('2')}]:
            self.assertEqual(self.post('remove', fields)[0], 403, fields)
        self.assertEqual(self.post('remove', {'id': 1}, csrf=False)[0], 403)
        # A download token must not work as CSRF token.
        self.assertEqual(self.post('remove', {'id': 1, 'csrf': self.app.token('1')})[0], 403)
        self.assertNotEqual(self.app.csrf('1'), hmac.new(b's' * 32, b'1', hashlib.sha256).hexdigest())
        self.assertEqual(self.app.csrf('1'), hmac.new(b's' * 32, b'csrf|1', hashlib.sha256).hexdigest())
        for headers in [{'Origin': 'http://evil.example'}, {'Origin': 'http://catalog.example'},
                        {'Origin': 'http://catalog.example:3000@evil.example'},
                        {'Referer': 'http://evil.example/katalog/'}, {'Origin': 'http://evil.example', 'Referer': 'http://catalog.example:3000/'}]:
            with self.subTest(headers=headers):
                self.assertEqual(self.post('remove', {'id': 1}, headers=headers)[0], 403)
        self.assertEqual(self.post('remove', {'id': 1}, headers={'Referer': 'http://catalog.example:3000/katalog/'})[0], 303)
        self.assertEqual(self.post('restore', {'id': 1}, headers={'Origin': 'http://CATALOG.example:3000'})[0], 303)
        # Browsers send "Origin: null" under Referrer-Policy: no-referrer; the token still decides.
        self.assertEqual(self.post('remove', {'id': 1}, headers={'Origin': 'null'})[0], 303)
        self.assertEqual(self.post('restore', {'id': 1}, headers={'Origin': 'null'})[0], 303)
        self.assertEqual(self.post('remove', {'id': 1, 'csrf': 'x' * 64}, headers={'Origin': 'null'})[0], 403)
        self.assertEqual(self.post('remove', {'id': 1}, csrf=False, headers={'Origin': 'null'})[0], 403)
        self.assertEqual(self.post('remove', {'id': 1}, headers={'Origin': 'null', 'Referer': 'http://evil.example/katalog/'})[0], 403)
        self.assertEqual(self.hidden.read_text(), '[]')
        self.hide(1)
        self.assertEqual(json.loads(self.hidden.read_text()), [1])
        self.assertEqual(self.post('remove', {'id': 1, 'csrf': 'bad'}, headers={'Origin': 'http://evil.example'})[0], 403)

    def test_post_input_validation_never_500(self):
        good = {'id': 1, 'csrf': self.app.csrf('1')}
        encoded = urlencode(good).encode()
        cases = [
            ('missing length', dict(headers={'Content-Length': None}, body=encoded), 411),
            ('bad length', dict(headers={'Content-Length': 'abc'}, body=encoded), 411),
            ('negative length', dict(headers={'Content-Length': '-1'}, body=encoded), 411),
            ('too large', dict(body=b'id=1&csrf=' + b'a' * 3000), 413),
            ('chunked', dict(headers={'Transfer-Encoding': 'chunked'}, body=encoded), 400),
            ('short body', dict(headers={'Content-Length': str(len(encoded) + 5)}, body=encoded), 400),
            ('json type', dict(headers={'Content-Type': 'application/json'}, body=encoded), 415),
            ('no type', dict(headers={'Content-Type': None}, body=encoded), 415),
            ('multipart', dict(headers={'Content-Type': 'multipart/form-data; boundary=x'}, body=encoded), 415),
            ('non ascii', dict(body='id=1&csrf=ä'.encode()), 400),
            ('empty', dict(body=b''), 400),
            ('garbage', dict(body=b'&&&=&'), 400),
            ('duplicate id', dict(body=b'id=1&id=2&csrf=' + good['csrf'].encode()), 400),
            ('non numeric id', dict(body=b'id=abc&csrf=x'), 400),
            ('negative id', dict(body=b'id=-1&csrf=x'), 400),
            ('huge id', dict(body=b'id=' + b'9' * 25 + b'&csrf=x'), 400),
            ('overflow id', dict(body=b'id=9223372036854775808&csrf=x'), 400),
        ]
        for label, kwargs, expected in cases:
            with self.subTest(label):
                kwargs.setdefault('fields', None)
                self.assertEqual(self.post('remove', csrf=False, **kwargs)[0], expected)
        self.assertEqual(self.post('remove', {'id': 999})[0], 404)
        self.assertEqual(self.post('restore', {'id': 999})[0], 404)
        self.add_file(5, 'del', deleted='yesterday')
        self.assertEqual(self.post('remove', {'id': 5})[0], 404)
        self.assertFalse(self.hidden.exists())
        # Other paths and methods stay read-only.
        self.assertEqual(self.post('', {'id': 1})[0], 405)
        self.assertEqual(self.post('download/1', {'id': 1})[0], 405)
        self.assertEqual(self.request('/katalog/remove', 'PUT')[0], 405)
        self.assertEqual(self.request('/katalog/restore')[0], 404)

    def test_next_is_never_an_open_redirect(self):
        for value in ['http://evil.example/', '//evil.example/katalog/', '/other', '/katalog/../x', 'javascript:alert(1)',
                      '/katalog/\\evil', '/katalog/%0d%0aSet-Cookie:x', '/katalogx/', 'https://catalog.example:3000/katalog/']:
            with self.subTest(value=value):
                status, headers, *_ = self.post('remove', {'id': 1, 'next': value})
                self.assertEqual(headers['Location'], '/katalog/?removed=1')
        status, headers, *_ = self.post('remove', {'id': 1, 'next': '/katalog/?q=a+b&type=stl&removed=7&file=3&evil=1&page=2'})
        self.assertEqual(headers['Location'], '/katalog/?q=a+b&type=stl&page=2&removed=1')

    def test_removed_status_line_undo_and_escaping(self):
        self.add_file(2, '<b>"x"</b>.stl')
        self.hide(2)
        html = self.request('/katalog/?removed=2', headers={'Accept-Language': 'en'})[2].decode()
        self.assertIn('<p role="status">‘&lt;b&gt;&quot;x&quot;&lt;/b&gt;.stl’ was removed from the list.', html)
        self.assertIn('>Undo</button>', html)
        self.assertIn('action="/katalog/restore"', html)
        self.assertNotIn('<b>"x"', html)
        self.assertIn('wurde aus der Liste entfernt', self.request('/katalog/?removed=2')[2].decode())
        # Only currently hidden files produce the line; junk values are ignored.
        for value in ['1', '999', 'abc', '2%20']:
            self.assertNotIn('wurde aus der Liste entfernt', self.request('/katalog/?removed=' + value)[2].decode())

    def test_dialog_markup_and_confirmation_page(self):
        self.add_file(2, '<i>quote".stl')
        html = self.request(headers={'Accept-Language': 'en'})[2].decode()
        self.assertIn('<dialog id="rm-dialog"', html)
        for text in ('Remove from the download list?', 'The file stays in the catalog and in storage; it is only no longer offered in this list.',
                     'data-rm-cancel autofocus', '>Cancel<', '>Remove</button>', 'Remove from list', 'aria-hidden="true"'):
            self.assertIn(text, html)
        self.assertIn(f'data-csrf="{self.app.csrf("2")}"', html)
        self.assertIn('data-name="&lt;i&gt;quote&quot;.stl"', html)
        self.assertNotIn('<i>quote', html)
        self.assertIn('href="/katalog/remove?id=2&amp;next=%2Fkatalog%2F"', html)
        self.assertIn('showModal', html)
        self.assertNotIn('<dialog', self.request('/katalog/?hidden=1')[2].decode())
        status, headers, body, _, _ = self.request('/katalog/remove?id=2&next=http%3A%2F%2Fevil.example%2F', headers={'Accept-Language': 'en'})
        page = body.decode()
        self.assertEqual(status, 200)
        self.assertIn('Remove from the download list?', page)
        self.assertIn('&lt;i&gt;quote&quot;.stl', page)
        self.assertIn('The file stays in the catalog and in storage', page)
        self.assertIn('<form method="post" action="/katalog/remove">', page)
        self.assertIn(f'name="csrf" value="{self.app.csrf("2")}"', page)
        self.assertIn('<a href="/katalog/">Cancel</a>', page)
        self.assertNotIn('evil.example', page)
        for query in ['', '?id=999', '?id=abc', '?id=1%20']:
            self.assertEqual(self.request('/katalog/remove' + query)[0], 404)
        self.hide(2)
        self.assertEqual(self.request('/katalog/remove?id=2')[0], 404)

    def test_hidden_store_robustness_atomic_write_and_mode(self):
        for content in [None, b'', b'not json', b'{"a": 1}', b'[1, "2"]', b'[true]', b'[-1]', b'[1.5]', b'\xff\xfe']:
            with self.subTest(content=content):
                if content is None:
                    self.hidden.unlink(missing_ok=True)
                else:
                    self.hidden.write_bytes(content)
                self.assertEqual(self.request()[0], 200)
                self.assertIn('/katalog/download/1"', self.request()[2].decode())
        self.hidden.write_bytes(b'garbage')
        with patch.object(self.web.os, 'replace', wraps=os.replace) as replace:
            self.hide(1)
            self.assertEqual(Path(replace.call_args.args[0]).parent, self.base)
            self.assertEqual(Path(replace.call_args.args[1]), self.hidden)
        self.assertEqual(json.loads(self.hidden.read_text()), [1])
        self.assertEqual(self.hidden.stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.hidden.stat().st_uid, os.getuid())
        self.assertEqual(list(self.base.glob('.katalog-web-hidden-*')), [])
        with patch.object(self.web.os, 'replace', side_effect=OSError('read only')):
            self.assertEqual(self.post('restore', {'id': 1})[0], 503)
        self.assertEqual(list(self.base.glob('.katalog-web-hidden-*')), [])
        self.assertEqual(json.loads(self.hidden.read_text()), [1])

    def test_hidden_store_limit_and_concurrency(self):
        store = self.web.HiddenStore(self.base / 'limit.json')
        with patch.object(self.web, 'HIDDEN_LIMIT', 3):
            self.assertTrue(all(store.change(i, True) for i in (1, 2, 3)))
            self.assertFalse(store.change(4, True))
            self.assertTrue(store.change(3, True))
            self.assertEqual(store.ids(), {1, 2, 3})
            self.assertEqual(self.post('remove', {'id': 1})[0], 303)
        import threading
        store = self.web.HiddenStore(self.base / 'parallel.json')
        threads = [threading.Thread(target=store.change, args=(i, True)) for i in range(1, 41)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        self.assertEqual(store.ids(), set(range(1, 41)))
        with patch.dict(os.environ, {'KATALOG_WEB_HIDDEN': str(self.base / 'env.json')}):
            self.assertEqual(self.web.HiddenStore().path, self.base / 'env.json')

    def test_large_hidden_list_does_not_hit_sql_limits(self):
        self.hidden.write_text(json.dumps(list(range(1000, 101000 - 1))))
        self.assertEqual(self.request()[0], 200)
        self.assertIn('/katalog/download/1"', self.request()[2].decode())

    def test_access_log_has_no_secrets(self):
        _, _, _, _, log = self.hide(1)
        self.assertEqual(log, 'POST /katalog/remove 303\n')
        _, _, _, _, log = self.post('remove', {'id': 1, 'csrf': 'secret-token'})
        self.assertNotIn('secret-token', log)


    # Marker texts that occur only in one language, for the elements checked below.
    MARKERS = {
        'de': {'remove_title': 'Aus der Download-Liste entfernen?', 'removed': 'wurde aus der Liste entfernt', 'error': 'Anfrage nicht verfügbar.',
               'missing': 'Katalog noch nicht angelegt', 'info': 'Links gelten 1 Stunde', 'slicer': 'In Slicer öffnen', 'homepage': '>Homepage<'},
        'en': {'remove_title': 'Remove from the download list?', 'removed': 'was removed from the list', 'error': 'Request unavailable.',
               'missing': 'Catalog not created', 'info': 'Links expire after 1 hour', 'slicer': 'Open in slicer', 'homepage': '>Website<'},
        'es': {'remove_title': '¿Quitar de la lista de descargas?', 'removed': 'se ha quitado de la lista', 'error': 'Solicitud no disponible.',
               'missing': 'Catálogo aún no creado', 'info': 'Los enlaces caducan al cabo de 1 hora', 'slicer': 'Abrir en el slicer', 'homepage': '>Sitio web<'},
        'fr': {'remove_title': 'Retirer de la liste de téléchargement ?', 'removed': 'a été retiré de la liste', 'error': 'Requête indisponible.',
               'missing': 'Catalogue pas encore créé', 'info': 'Les liens expirent au bout d’1 heure', 'slicer': 'Ouvrir dans le slicer', 'homepage': '>Site web<'},
    }

    def test_all_languages_define_every_text_key(self):
        self.assertEqual(self.web.LANGS, ('de', 'en', 'es', 'fr'))
        for key, values in self.web.TEXT.items():
            self.assertEqual(len(values), 4, key)
            self.assertTrue(all(v.strip() for v in values), key)
        for key in ('removed', 'show_hidden'):
            placeholder = '{name}' if key == 'removed' else '{n}'
            for lang, value in zip(self.web.LANGS, self.web.TEXT[key]):
                self.assertIn(placeholder, value, (key, lang))
        # Translated labels must differ from German, except words identical by nature.
        same_ok = {'tag', 'name', 'page', 'homepage'}
        for key, values in self.web.TEXT.items():
            if key not in same_ok:
                for index in (1, 2, 3):
                    self.assertNotEqual(values[index], values[0], (key, self.web.LANGS[index]))

    def test_language_detection(self):
        language = self.web.language
        for header, expected in [('es', 'es'), ('fr', 'fr'), ('en-US', 'en'), ('de-AT', 'de'), ('es-MX,es;q=0.9', 'es'),
                                 ('pt', 'de'), ('pt-BR,pt;q=0.9', 'de'), ('', 'de'), ('xx;q=abc', 'de'),
                                 ('fr;q=0.4,es;q=0.8,en;q=0.2', 'es'), ('pt,fr;q=0.5', 'fr'), ('es;q=0,fr;q=0.1', 'fr'),
                                 ('es,fr', 'es'), ('en;q=0.9,de;q=0.9', 'en')]:
            self.assertEqual(language(header), expected, header)
        # An explicit choice beats the header; invalid values fall back to detection.
        self.assertEqual(language('de', 'fr'), 'fr')
        for bad in ('xx', '', '<script>', None, 'ES', 'es ', 'pt'):
            self.assertEqual(language('en', bad), 'en', bad)

    def test_lang_parameter_beats_header_and_is_never_echoed_unchecked(self):
        html = self.request('/katalog/?lang=es', headers={'Accept-Language': 'de'})[2].decode()
        self.assertIn('<html lang="es">', html)
        for value in ('xx', '', '%3Cscript%3E', '"onmouseover="x', 'ES', 'es%00'):
            html = self.request('/katalog/?lang=' + value, headers={'Accept-Language': 'fr'})[2].decode()
            self.assertIn('<html lang="fr">', html, value)
            self.assertNotIn('<script>alert', html)
            self.assertNotIn('onmouseover', html)
            self.assertNotIn('name=lang', html)
        for path in ('/katalog/?file=1&lang=%3Cscript%3E', '/katalog/remove?id=1&lang=%3Cscript%3E&next=%2Fkatalog%2F%3Flang%3Dzz'):
            body = self.request(path)[2].decode()
            self.assertIn('<html lang="de">', body)
            self.assertNotIn('%3Cscript', body)
            self.assertNotIn('zz', body.split('</style>', 1)[1])

    def test_html_lang_and_header_links_per_language(self):
        for lang in ('de', 'en', 'es', 'fr'):
            for via in ({'Accept-Language': lang}, {}):
                path = '/katalog/' if via else f'/katalog/?lang={lang}'
                html = self.request(path, headers=via)[2].decode()
                self.assertIn(f'<html lang="{lang}">', html)
                header = html[html.index('<header'):html.index('</header>')]
                english = lang != 'de'
                self.assertIn('https://wiki.3mfkatalog.de/en/"' if english else 'https://wiki.3mfkatalog.de/"', header)
                self.assertIn('https://3mfkatalog.de/en/"' if english else 'https://3mfkatalog.de/"', header)
                self.assertNotIn('/es/', header)
                self.assertNotIn('/fr/', header)
                self.assertIn('https://discord.gg/abfVNfFqu3"', header)
                self.assertIn(self.MARKERS[lang]['homepage'], header)

    def test_language_menu(self):
        html = self.request('/katalog/?q=a&page=2&lang=fr')[2].decode()
        menu = html[html.index('<nav class="langs"'):html.index('</nav>', html.index('<nav class="langs"'))]
        self.assertIn('aria-label="Langue"', menu)
        for code, name in [('de', 'Deutsch'), ('en', 'English'), ('es', 'Español'), ('fr', 'Français')]:
            self.assertIn(f'lang="{code}" hreflang="{code}" title="{name}" aria-label="{name}"', menu)
        self.assertEqual(menu.count('aria-current="true"'), 1)
        self.assertIn('href="/katalog/?q=a&amp;page=2&amp;lang=es" lang="es"', menu)
        # The current language is marked on the French link, and every link shows a decorative flag.
        self.assertRegex(menu, r'lang="fr" hreflang="fr" title="Français" aria-label="Français" aria-current="true"><svg class="flag"')
        self.assertEqual(menu.count('<svg class="flag"'), 4)
        self.assertEqual(menu.count('aria-hidden="true"'), 4)
        self.assertNotRegex(menu, r'>(DE|EN|ES|FR)<')
        for lang, label in [('de', 'Sprache'), ('en', 'Language'), ('es', 'Idioma')]:
            self.assertIn(f'<nav class="langs" aria-label="{label}">', self.request(headers={'Accept-Language': lang})[2].decode())
        # Without an explicit choice the menu links still set one; the single view keeps its file.
        self.assertIn('href="/katalog/?lang=en" lang="en"', self.request()[2].decode())
        self.assertIn('href="/katalog/?file=1&amp;lang=es" lang="es"', self.request('/katalog/?file=1')[2].decode())

    def test_every_language_renders_all_views(self):
        self.add_file(2, 'Two.stl')
        self.hide(2)
        for lang in ('de', 'en', 'es', 'fr'):
            mark = self.MARKERS[lang]
            others = [m for other, ms in self.MARKERS.items() if other != lang for m in ms.values()]
            q = f'lang={lang}'
            pages = {
                'list': self.request(f'/katalog/?{q}')[2].decode(),
                'single': self.request(f'/katalog/?file=1&{q}')[2].decode(),
                'status': self.request(f'/katalog/?removed=2&{q}')[2].decode(),
                'hidden': self.request(f'/katalog/?hidden=1&{q}')[2].decode(),
                'confirm': self.request(f'/katalog/remove?id=1&{q}')[2].decode(),
                'error': self.request(f'/katalog/nothing?{q}')[2].decode(),
            }
            for name, page in pages.items():
                if name != 'error':
                    self.assertIn(f'<html lang="{lang}">', page, (lang, name))
                for foreign in others:
                    self.assertNotIn(foreign, page.split('</style>', 1)[-1], (lang, name, foreign))
            self.assertIn(mark['info'], pages['list'])
            self.assertIn(mark['slicer'], pages['list'])
            self.assertIn(mark['slicer'], pages['single'])
            self.assertIn(mark['remove_title'], pages['list'])
            self.assertIn(mark['remove_title'], pages['confirm'])
            self.assertIn(mark['removed'], pages['status'])
            self.assertIn(self.web.TEXT['restore'][self.web.LANGS.index(lang)], pages['hidden'])
            self.assertEqual(pages['error'], mark['error'])
            self.assertIn(self.web.TEXT['show_hidden'][self.web.LANGS.index(lang)].format(n=1), pages['list'])
        # Missing catalog page.
        self.app.db_path = str(self.base / 'absent.db')
        for lang in ('de', 'en', 'es', 'fr'):
            self.assertIn(self.MARKERS[lang]['missing'], self.request(f'/katalog/?lang={lang}')[2].decode())

    def test_spanish_and_french_samples(self):
        es = self.request(headers={'Accept-Language': 'es-ES,es;q=0.9'})[2].decode()
        for text in ('Descargas y slicer', 'Buscar nombre', 'Carpeta', 'Etiqueta', 'Descargar', 'Quitar de la lista', 'Aplicar'):
            self.assertIn(text, es)
        fr = self.request(headers={'Accept-Language': 'fr-FR,fr;q=0.9'})[2].decode()
        for text in ('Téléchargement et slicer', 'Rechercher un nom', 'Dossier', 'Étiquette', 'Télécharger', 'Retirer de la liste', 'Appliquer'):
            self.assertIn(text, fr)
        self.assertIn('<html lang="de">', self.request(headers={'Accept-Language': 'pt-BR'})[2].decode())

    def test_language_survives_forms_links_and_redirects(self):
        self.add_file(2, 'Two.stl')
        for fid in range(3, 56):
            self.add_file(fid, f'item{fid:03}.stl')
        html = self.request('/katalog/?q=item&lang=es')[2].decode()
        self.assertIn('<input type=hidden name=lang value=es>', html)
        self.assertRegex(html, r'href="/katalog/\?q=item&amp;lang=es&amp;page=2"')
        self.assertIn('href="/katalog/?lang=es"', html)
        self.assertIn('data-next="/katalog/?q=item&amp;lang=es"', html)
        self.assertIn('lang=es', html[html.index('href="/katalog/remove?'):html.index('data-rm')])
        # Confirmation page keeps the language in its menu, cancel link and hidden next field.
        page = self.request('/katalog/remove?id=2&next=%2Fkatalog%2F%3Flang%3Dfr')[2].decode()
        self.assertIn('<html lang="fr">', page)
        self.assertIn('name="next" value="/katalog/?lang=fr"', page)
        self.assertIn('<a href="/katalog/?lang=fr">Annuler</a>', page)
        self.assertIn('href="/katalog/remove?id=2&amp;next=%2Fkatalog%2F%3Flang%3Des&amp;lang=es"', page)
        # POST redirects keep the language through next; undo form and hidden view do too.
        _, headers, *_ = self.post('remove', {'id': 2, 'next': '/katalog/?q=item&lang=es'})
        self.assertEqual(headers['Location'], '/katalog/?q=item&lang=es&removed=2')
        status = self.request(headers['Location'])[2].decode()
        self.assertIn('<html lang="es">', status)
        self.assertIn('se ha quitado de la lista', status)
        self.assertIn('name="next" value="/katalog/?q=item&amp;lang=es"', status)
        self.assertIn('href="/katalog/?hidden=1&amp;lang=es"', status)
        _, headers, *_ = self.post('restore', {'id': 2, 'next': '/katalog/?hidden=1&lang=es'})
        self.assertEqual(headers['Location'], '/katalog/?lang=es')
        # Without a language the redirect target is unchanged.
        _, headers, *_ = self.post('remove', {'id': 2, 'next': '/katalog/?lang=xx'})
        self.assertEqual(headers['Location'], '/katalog/?removed=2')
        # Single view: back link and forms keep the choice.
        single = self.request('/katalog/?file=1&lang=fr')[2].decode()
        self.assertIn('<a href="/katalog/?lang=fr">Afficher tous les fichiers</a>', single)
        # Error pages honour ?lang= too.
        self.assertEqual(self.request('/katalog/download/999?lang=es')[2], b'Solicitud no disponible.')


if __name__ == '__main__':
    unittest.main()
