import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ProxyTests(unittest.TestCase):
    def test_internal_listener_build_anchors_fail_closed(self):
        dockerfile = (ROOT / 'Dockerfile').read_text()
        command = dockerfile.split('# Only nginx authenticates requests;', 1)[1].split('\nRUN ', 1)[1].split('\n\n', 1)[0]
        bind = '                    "0.0.0.0",\n'
        good = 'async with ws_async.serve(\n                    self.ws_handler,\n' + bind + '                    self.port,\n'
        cases = {
            'patched once': (good, True),
            'upstream line changed': (good.replace('0.0.0.0', 'localhost'), False),
            'bind line appears twice': (good + good, False),
            'file missing': (None, False),
        }
        for name, (content, patched) in cases.items():
            with self.subTest(case=name), tempfile.TemporaryDirectory() as tmp:
                target = Path(tmp + '/lsiopy/lib/python3.12/site-packages/selkies/selkies.py')
                if content is not None:
                    target.parent.mkdir(parents=True)
                    target.write_text(content)
                result = subprocess.run(['sh', '-c', command.replace('/lsiopy/', tmp + '/lsiopy/')], capture_output=True)
                self.assertEqual(result.returncode == 0, patched, result.stderr)
                if patched:
                    text = target.read_text()
                    self.assertIn('"127.0.0.1",', text)
                    self.assertNotIn('0.0.0.0', text)

    def test_smoke_internal_listener_check(self):
        smoke = (ROOT / 'scripts/ci-smoke.sh').read_text()
        function = 'check_internal_listeners() {' + smoke.split('check_internal_listeners() {', 1)[1].split('\n}', 1)[0] + '\n}'
        # Supply ss output without starting Docker or opening any sockets.
        command = '''
fail() { exit 1; }
ok() { :; }
docker() { printf '%s\\n' "$LISTENERS"; return "${SS_STATUS:-0}"; }
''' + function + '\ncheck_internal_listeners test'
        for addresses, good in [
            (['127.0.0.1:8082', '127.0.0.1:8099'], True),
            (['[::1]:8082', '[::1]:8099'], True),
            (['::1:8082', '::1:8099'], True),
            (['0.0.0.0:8082', '127.0.0.1:8099'], False),
            (['127.0.0.1:8082', '[::]:8099'], False),
            (['*:8082', '127.0.0.1:8099'], False),
            (['127.0.0.1:8082', '172.18.0.2:8099'], False),
            (['127.0.0.1:8082', '127.0.0.1:8099', '0.0.0.0:8099'], False),
            (['127.0.0.1:8082'], False),
            ([], False),
        ]:
            with self.subTest(addresses=addresses):
                listeners = '\n'.join(f'LISTEN 0 128 {address} 0.0.0.0:*' for address in addresses)
                for status in ['0', '1']:
                    result = subprocess.run(['bash', '-c', command], capture_output=True,
                                            env={**os.environ, 'LISTENERS': listeners, 'SS_STATUS': status})
                    self.assertEqual(result.returncode == 0, good and status == '0', result.stderr)

    def test_proxy_port_configuration(self):
        helper = ROOT / 'root/usr/local/libexec/3mf-katalog-proxy'
        self.assertTrue(helper.exists(), 'Proxy port configuration helper must exist')
        with tempfile.TemporaryDirectory() as tmp:
            config = Path(tmp) / 'proxy.conf'
            original = (ROOT / 'root/etc/nginx/3mf-katalog.conf').read_text()
            config.write_text(original)
            for port in ['8100', '8099', '65535']:
                subprocess.run([str(helper), str(config)], env={**os.environ, 'KATALOG_WEB_PORT': port}, check=True, capture_output=True)
                self.assertEqual(config.read_text().count(f'proxy_pass http://127.0.0.1:{port};'), 3)
            for port in ['0', '65536', '-1', '80;bad', '999999999999999999', 'abc']:
                before = config.read_text()
                result = subprocess.run([str(helper), str(config)], env={**os.environ, 'KATALOG_WEB_PORT': port}, capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(config.read_text(), before)

    def test_dockerfile_anchor_fails_closed(self):
        dockerfile = (ROOT / 'Dockerfile').read_text()
        marker = '# Insert the companion routes'
        self.assertIn(marker, dockerfile)
        command = dockerfile.split(marker, 1)[1].split('\nRUN ', 1)[1].split('\n\n', 1)[0]
        with tempfile.TemporaryDirectory() as tmp:
            config = Path(tmp) / 'default.conf'
            command = command.replace('/defaults/default.conf', str(config))
            for fixture, good in [
                ('server {\n listen 3000;\n}\nserver {\n listen 3001 ssl;\n}\n', True),
                ('server {\n listen 3000;\n}\n', False),
                ('server {\n listen 3000;\n}\nserver {\n listen 4001 ssl;\n}\n', False),
                ('', False),
            ]:
                config.write_text(fixture)
                result = subprocess.run(['sh', '-c', command], capture_output=True)
                self.assertEqual(result.returncode == 0, good, result.stderr.decode())
                if good:
                    text = config.read_text()
                    self.assertEqual(text.count('include /etc/nginx/3mf-katalog.conf;'), 2)
                    self.assertNotIn('#', text)
