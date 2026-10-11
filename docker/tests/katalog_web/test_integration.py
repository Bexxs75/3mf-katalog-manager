import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ProxyTests(unittest.TestCase):
    def test_proxy_port_configuration(self):
        helper = ROOT / 'root/usr/local/libexec/3mf-katalog-proxy'
        self.assertTrue(helper.exists(), 'Proxy port configuration helper must exist')
        with tempfile.TemporaryDirectory() as tmp:
            config = Path(tmp) / 'proxy.conf'
            original = (ROOT / 'root/etc/nginx/3mf-katalog.conf').read_text()
            config.write_text(original)
            for port in ['8100', '8099', '65535']:
                subprocess.run([str(helper), str(config)], env={**os.environ, 'KATALOG_WEB_PORT': port}, check=True, capture_output=True)
                self.assertEqual(config.read_text().count(f'proxy_pass http://127.0.0.1:{port};'), 2)
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
