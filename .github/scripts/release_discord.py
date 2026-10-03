#!/usr/bin/env python3
"""Publish release events or an explicitly requested tag without shell interpolation."""
import json
import os
from pathlib import Path
import sys
import urllib.error
import urllib.parse
import urllib.request


def posts(release, env):
    tag = release.get('tag_name') or ''
    if tag == 'preview':
        return []
    url = release.get('html_url') or ''
    if not tag or not url.startswith('https://github.com/') or len(url) > 1000:
        raise ValueError('Release-Tag oder GitHub-URL fehlt/ist ungültig')
    de = env.get('DISCORD_RELEASE_WEBHOOK_DE', '').strip()
    en = env.get('DISCORD_RELEASE_WEBHOOK_EN', '').strip()
    if bool(de) != bool(en):
        raise ValueError('Beide DISCORD_RELEASE_WEBHOOK_DE/EN werden benötigt')
    if de and en:
        if de == en:
            raise ValueError('Sprach-Webhooks müssen verschieden sein')
        entries = [
            (de, f'3MF Katalog Manager · {tag}', 'Eine neue Version wurde veröffentlicht.\n\n'
             f'[Änderungen und Downloads auf GitHub]({url})'),
            (en, f'3MF Katalog Manager · {tag}', 'A new version has been released.\n\n'
             f'[Release notes and downloads on GitHub]({url})'),
        ]
    else:
        hook = env.get('DISCORD_RELEASE_WEBHOOK', '').strip()
        if not hook:
            raise ValueError('DISCORD_RELEASE_WEBHOOK fehlt')
        body = release.get('body') or ''
        if len(body) > 4096:
            suffix = f'…\n\n[Vollständige Notes auf GitHub]({url})'
            body = body[:4096-len(suffix)] + suffix
        entries = [(hook, release.get('name') or tag, body)]
    return [(hook, {'embeds':[{'title':title[:256], 'url':url,
                              'description':body, 'color':3447003}],
                    'allowed_mentions':{'parse':[]}}) for hook, title, body in entries]


def send(hook, payload):
    request = urllib.request.Request(hook, json.dumps(payload).encode(),
        {'Content-Type':'application/json', 'User-Agent':'3mf-release-notify'}, method='POST')
    with urllib.request.urlopen(request, timeout=30) as response:
        response.read()


def load_release(env):
    tag = env.get('TAG_INPUT', '')
    if tag:
        repo = env['GITHUB_REPOSITORY']
        url = f'https://api.github.com/repos/{repo}/releases/tags/{urllib.parse.quote(tag, safe="")}'
        request = urllib.request.Request(url, headers={
            'Authorization':f'Bearer {env["GH_TOKEN"]}', 'Accept':'application/vnd.github+json',
            'User-Agent':'3mf-release-notify'})
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    return json.loads(Path(env['GITHUB_EVENT_PATH']).read_text())['release']


def main():
    pending = posts(load_release(os.environ), os.environ)
    for index, (hook, payload) in enumerate(pending, 1):
        send(hook, payload)
        print(f'Release-Ziel {index}: veröffentlicht')
    if not pending:
        print('Preview wird nicht angekündigt')


if __name__ == '__main__':
    try:
        main()
    except urllib.error.HTTPError as exc:
        sys.exit(f'HTTP-Fehler {exc.code}; Veröffentlichung abgebrochen')
    except (urllib.error.URLError, TimeoutError):
        sys.exit('Netzwerkfehler; Veröffentlichung abgebrochen')
    except (ValueError, KeyError):
        sys.exit('Ungültige Release-Daten oder unvollständige Webhook-Konfiguration')
