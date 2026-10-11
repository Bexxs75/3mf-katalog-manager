#!/usr/bin/env bash
# Exercise nginx authentication and the supervised service in an already-built image.
set -euo pipefail
image=${1:?usage: katalog-web.sh IMAGE}
name=katalog-web-test-$$
volume=katalog-web-config-$$
cleanup() {
    docker rm -f "$name" >/dev/null 2>&1 || :
    docker volume rm "$volume" >/dev/null 2>&1 || :
}
trap cleanup EXIT
PASSWORD=$(python3 -c 'import secrets; print(secrets.token_hex(24))')
export PASSWORD
# Use a private environment value rather than embedding credentials in this script.
docker volume create "$volume" >/dev/null
docker run --pull=never -d --name "$name" \
    -p 127.0.0.1::3000 -p 127.0.0.1::3001 -v "$volume":/config \
    -e PASSWORD -e CUSTOM_USER=abc -e PUID="$(id -u)" -e PGID="$(id -g)" \
    -e CATALOG_DB=/config/companion-test/catalog.db \
    -e KATALOG_ALLOWED_ROOTS=/config/companion-test/models \
    --shm-size=1g "$image" >/dev/null
for _ in $(seq 1 90); do
    if [ "$(docker inspect -f '{{.State.Health.Status}}' "$name")" = healthy ]; then break; fi
    sleep 2
done
[ "$(docker inspect -f '{{.State.Health.Status}}' "$name")" = healthy ] || {
    printf 'FAIL: container did not become healthy\n' >&2
    exit 1
}
docker exec -u abc -i "$name" /usr/bin/python3 - <<'PY'
from pathlib import Path
import sqlite3
base = Path('/config/companion-test')
(base / 'models').mkdir(parents=True)
with sqlite3.connect(base / 'catalog.db') as con:
    con.executescript('''
        CREATE TABLE files (id INTEGER PRIMARY KEY, name TEXT, path TEXT,
            file_type TEXT, folder_id INTEGER, file_size_bytes INTEGER,
            thumbnail_png BLOB, imported_at TEXT, deleted_at TEXT);
        CREATE TABLE folders (id INTEGER PRIMARY KEY, name TEXT);
        CREATE TABLE tags (id INTEGER PRIMARY KEY, name TEXT);
        CREATE TABLE file_tags (file_id INTEGER, tag_id INTEGER);
    ''')
    for fid, kind in [(1, '3mf'), (2, 'stl')]:
        path = base / 'models' / f'model.{kind}'
        data = f'catalog fixture {fid}\n'.encode()
        path.write_bytes(data)
        con.execute('INSERT INTO files VALUES (?, ?, ?, ?, NULL, ?, NULL, ?, NULL)',
                    (fid, path.name, str(path), kind, len(data), '2026-10-11'))
PY
http_port=$(docker port "$name" 3000/tcp | sed 's/.*://')
https_port=$(docker port "$name" 3001/tcp | sed 's/.*://')
python3 - "$http_port" "$https_port" <<'PY'
import base64
from html.parser import HTMLParser
import os
import ssl
import sys
import urllib.error
import urllib.request
from urllib.parse import unquote, urlsplit

class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.urls = []
    def handle_starttag(self, tag, attrs):
        href = dict(attrs).get('href', '')
        if href.startswith('orcaslicer://open?file='):
            self.urls.append(unquote(href.split('file=', 1)[1]))

# Ignore host proxy settings so these checks target only the local test container.
context = ssl._create_unverified_context()
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPSHandler(context=context))
auth = 'Basic ' + base64.b64encode(('abc:' + os.environ['PASSWORD']).encode()).decode()
def fetch(url, expected, login=False, extra=None):
    headers = dict(extra or {})
    if login:
        headers['Authorization'] = auth
    try:
        response = opener.open(urllib.request.Request(url, headers=headers), timeout=10)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        assert response.status == expected, (response.status, expected)
        assert not response.headers.get('Set-Cookie')
        return response.read(), response.headers

for scheme, port in [('http', sys.argv[1]), ('https', sys.argv[2])]:
    base = f'{scheme}://127.0.0.1:{port}'
    fetch(base + '/', 401)
    fetch(base + '/katalog/', 401)
    fetch(base + '/katalog/download/1', 401)
    page, _ = fetch(base + '/katalog/', 200, True)
    parser = Links()
    parser.feed(page.decode())
    assert len(parser.urls) == 2
    for url in parser.urls:
        path = urlsplit(url).path
        fid = path.split('/')[3]
        expected = f'catalog fixture {fid}\n'.encode()
        data, headers = fetch(url, 200)
        assert data == expected
        assert int(headers['Content-Length']) == len(expected)
        assert headers['Content-Disposition'].startswith('inline;')
        parts = path.split('/')
        parts[4] += 'x'
        fetch(base + '/'.join(parts), 403)
        fetch(base + f'/katalog/signed/{fid}/', 403)
        download, _ = fetch(base + f'/katalog/download/{fid}', 200, True)
        assert download == expected
    proxied, _ = fetch(base + '/katalog/', 200, True,
                       {'Host': 'catalog.example:443', 'X-Forwarded-Proto': 'https'})
    assert b'https%3A%2F%2Fcatalog.example%3A443%2Fkatalog%2Fsigned%2F' in proxied
    fetch(base + '/', 401)
    print(f'OK: {scheme}: desktop/list/download require login; signed files bypass login with exact bytes; bad/missing tokens return 403')
PY
python3 - "$http_port" "$https_port" <<'PY'
import base64
import os
import re
import ssl
import sys
import urllib.error
import urllib.request
from urllib.parse import urlencode

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None

context = ssl._create_unverified_context()
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPSHandler(context=context), NoRedirect)
auth = 'Basic ' + base64.b64encode(('abc:' + os.environ['PASSWORD']).encode()).decode()

def call(url, data=None, login=True, extra=None):
    headers = dict(extra or {})
    if login:
        headers['Authorization'] = auth
    if data is not None:
        headers['Content-Type'] = 'application/x-www-form-urlencoded'
        data = urlencode(data).encode()
    try:
        response = opener.open(urllib.request.Request(url, data=data, headers=headers), timeout=10)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        return response.status, response.headers, response.read().decode()

for scheme, port in [('http', sys.argv[1]), ('https', sys.argv[2])]:
    base = f'{scheme}://127.0.0.1:{port}'
    same = {'Origin': base}
    status, _, page = call(base + '/katalog/')
    assert status == 200 and 'href="/katalog/download/1"' in page
    token = re.search(r'data-id="1" data-csrf="([0-9a-f]{64})"', page).group(1)
    form = {'id': '1', 'csrf': token, 'next': '/katalog/'}
    assert call(base + '/katalog/remove', form, login=False, extra=same)[0] == 401
    assert call(base + '/katalog/restore', form, login=False, extra=same)[0] == 401
    assert call(base + '/katalog/remove', {'id': '1'}, extra=same)[0] == 403
    assert call(base + '/katalog/remove', {**form, 'csrf': '0' * 64}, extra=same)[0] == 403
    assert call(base + '/katalog/remove', form, extra={'Origin': 'http://evil.example'})[0] == 403
    status, headers, _ = call(base + '/katalog/remove', form, extra=same)
    assert status == 303 and headers['Location'] == '/katalog/?removed=1', (status, headers['Location'])
    status, _, page = call(base + '/katalog/?removed=1')
    assert status == 200 and 'href="/katalog/download/1"' not in page and 'href="/katalog/download/2"' in page
    assert call(base + '/katalog/download/1')[0] == 404
    assert call(base + '/katalog/?hidden=1')[0] == 200
    status, headers, _ = call(base + '/katalog/restore', form, extra=same)
    assert status == 303 and headers['Location'] == '/katalog/'
    assert call(base + '/katalog/download/1')[0] == 200
    print(f'OK: {scheme}: remove/restore need login, CSRF token and same-origin; hidden files return 404 and come back')
PY
docker exec "$name" sh -c 'test "$(stat -c %a /config/.katalog-web-hidden.json)" = 600 && test "$(stat -c %U /config/.katalog-web-hidden.json)" = abc'
docker exec "$name" sh -c 'test "$(stat -c %a /config/.katalog-web-secret)" = 600 && test "$(stat -c %U /config/.katalog-web-secret)" = abc'
# s6 must restart the companion after an unexpected exit.
docker exec "$name" /command/s6-svc -k /run/service/svc-katalog-web
for _ in $(seq 1 20); do
    if docker exec "$name" /usr/bin/python3 -c 'import os, urllib.request; assert urllib.request.urlopen("http://127.0.0.1:" + os.environ.get("KATALOG_WEB_PORT", "8099") + "/katalog/health", timeout=2).read() == b"ok\n"' >/dev/null 2>&1; then
        printf 'OK: catalog web restarted after crash; secret mode/owner verified\n'
        exit 0
    fi
    sleep 1
done
printf 'FAIL: catalog web did not restart\n' >&2
exit 1
