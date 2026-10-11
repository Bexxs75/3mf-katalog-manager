import contextlib
import hashlib
import hmac
import importlib.machinery
import importlib.util
import io
import os
from pathlib import Path
import sqlite3
import tempfile
import time
import types
import unittest
from unittest.mock import patch
from urllib.parse import quote

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
        self.app = self.web.Catalog(str(self.db), [str(self.root)], b's' * 32)

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

    def request(self, path='/katalog/', method='GET', headers=None):
        values = {'Host': 'catalog.example:3000', **(headers or {})}
        raw = f'{method} {path} HTTP/1.1\r\n' + ''.join(f'{k}: {v}\r\n' for k, v in values.items()) + '\r\n'
        sock = Socket(raw.encode())
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

    def signed(self, fid='1', token=None, name='different.stl'):
        return f'/katalog/signed/{fid}/{token or self.app.token(fid)}/{name}'

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
                         "script-src 'self' 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'")

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
        self.app = self.web.Catalog(str(self.db), [str(alias)], b's' * 32)
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


if __name__ == '__main__':
    unittest.main()
