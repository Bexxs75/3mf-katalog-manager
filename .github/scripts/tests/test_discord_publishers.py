import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch
import urllib.error

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import roadmap_discord as roadmap

class RoadmapTargets(unittest.TestCase):
    def test_legacy_order(self):
        self.assertEqual(roadmap.targets({'DISCORD_ROADMAP_WEBHOOK': 'hook', 'DISCORD_ROADMAP_MSGS': '1,2,3,4'}), [('hook', str(n)) for n in range(1,5)])

    def test_split_order(self):
        env = {'DISCORD_ROADMAP_WEBHOOK_DE':'de', 'DISCORD_ROADMAP_WEBHOOK_EN':'en', 'DISCORD_ROADMAP_MSGS_DE':'1,2', 'DISCORD_ROADMAP_MSGS_EN':'3,4'}
        self.assertEqual(roadmap.targets(env), [('de','1'),('de','2'),('en','3'),('en','4')])

    def test_partial_split_never_falls_back(self):
        env = {'DISCORD_ROADMAP_WEBHOOK':'old', 'DISCORD_ROADMAP_MSGS':'1,2,3,4', 'DISCORD_ROADMAP_MSGS_EN':'3,4'}
        with self.assertRaises(ValueError): roadmap.targets(env)

    def test_identical_split_hooks_rejected_with_trailing_slash(self):
        for en in ('same', 'same/'):
            env = {'DISCORD_ROADMAP_WEBHOOK_DE':'same', 'DISCORD_ROADMAP_WEBHOOK_EN':en,
                   'DISCORD_ROADMAP_MSGS_DE':'1,2', 'DISCORD_ROADMAP_MSGS_EN':'3,4'}
            with self.subTest(en=en), self.assertRaises(ValueError):
                roadmap.targets(env)

    def test_invalid_and_duplicate_ids(self):
        for ids in ('1,2,3,x', '1,2,3,3', '1,2,3'):
            with self.subTest(ids=ids), self.assertRaises(ValueError):
                roadmap.targets({'DISCORD_ROADMAP_WEBHOOK':'hook','DISCORD_ROADMAP_MSGS':ids})


class Releases(unittest.TestCase):
    def setUp(self):
        import release_discord
        self.mod = release_discord
        self.data = {'tag_name':'v1.2.3','name':'Mixed title','html_url':'https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v1.2.3','body':'Release notes'}

    def test_localized_split_and_no_mentions(self):
        posts = self.mod.posts(self.data, {'DISCORD_RELEASE_WEBHOOK_DE':'de','DISCORD_RELEASE_WEBHOOK_EN':'en'})
        self.assertEqual([p[0] for p in posts], ['de','en'])
        self.assertIn('veröffentlicht', posts[0][1]['embeds'][0]['description'])
        self.assertIn('released', posts[1][1]['embeds'][0]['description'])
        for _, payload in posts:
            self.assertEqual(payload['allowed_mentions'], {'parse':[]})
            self.assertNotIn('Mixed title', payload['embeds'][0]['title'])

    def test_preview_never_posts_even_manual(self):
        self.data['tag_name']='preview'
        self.assertEqual(self.mod.posts(self.data, {}), [])

    def test_legacy_preserves_notes_with_unicode_bounds(self):
        self.data['body']='😀'*5000
        self.data['name']='a'*300
        payload=self.mod.posts(self.data, {'DISCORD_RELEASE_WEBHOOK':'old'})[0][1]
        self.assertLessEqual(len(payload['embeds'][0]['description']),4096)
        self.assertLessEqual(len(payload['embeds'][0]['title']),256)
        self.assertIn('GitHub',payload['embeds'][0]['description'])

    def test_partial_split_is_error(self):
        with self.assertRaises(ValueError):
            self.mod.posts(self.data, {'DISCORD_RELEASE_WEBHOOK':'old','DISCORD_RELEASE_WEBHOOK_DE':'de'})

    def test_duplicate_release_hooks_rejected(self):
        with self.assertRaises(ValueError):
            self.mod.posts(self.data, {'DISCORD_RELEASE_WEBHOOK_DE':'same','DISCORD_RELEASE_WEBHOOK_EN':'same'})

    def test_http_error_surfaces(self):
        with patch('urllib.request.urlopen', side_effect=urllib.error.HTTPError('secret',429,'rate limit',{},None)):
            with self.assertRaises(urllib.error.HTTPError): self.mod.send('https://discord.com/api/webhooks/secret', {})


class DeliverySafety(unittest.TestCase):
    def test_roadmap_invalid_config_has_no_network_or_writes(self):
        with patch.dict('os.environ', {'DISCORD_ROADMAP_WEBHOOK_DE':'de'}, clear=True), patch.object(roadmap, 'fetch_items') as fetch, patch.object(roadmap, 'edit') as edit:
            with self.assertRaises(ValueError): roadmap.main()
            fetch.assert_not_called()
            edit.assert_not_called()

    def test_roadmap_oversized_english_part_never_partially_updates_german(self):
        env = {'GH_TOKEN':'test','DISCORD_ROADMAP_WEBHOOK':'hook','DISCORD_ROADMAP_MSGS':'1,2,3,4'}
        items = [{'title':'Kurz / ' + 'x'*5000, 'Version':'v1', 'Status':'Todo'}]
        with patch.dict('os.environ', env, clear=True), patch.object(roadmap, 'fetch_items', return_value=(['v1'],items)), patch.object(roadmap,'edit') as edit:
            with self.assertRaises(SystemExit): roadmap.main()
            edit.assert_not_called()

    def test_roadmap_patch_clears_old_content_and_suppresses_mentions(self):
        with patch('urllib.request.urlopen') as open_url:
            roadmap.edit('https://discord.com/api/webhooks/example','123','@everyone roadmap')
        req = open_url.call_args.args[0]
        payload = json.loads(req.data)
        self.assertEqual(req.method, 'PATCH')
        self.assertEqual(req.full_url, 'https://discord.com/api/webhooks/example/messages/123')
        self.assertEqual(payload['content'], '')
        self.assertEqual(payload['allowed_mentions'], {'parse':[]})

    def test_manual_tag_is_escaped_in_api_path(self):
        import release_discord
        with patch('urllib.request.urlopen') as open_url:
            open_url.return_value.__enter__.return_value.read.return_value = b'{"tag_name":"feature/a"}'
            release = release_discord.load_release({'TAG_INPUT':'feature/a','GITHUB_REPOSITORY':'owner/repo','GH_TOKEN':'test'})
        self.assertEqual(release['tag_name'], 'feature/a')
        self.assertTrue(open_url.call_args.args[0].full_url.endswith('/feature%2Fa'))

if __name__ == '__main__': unittest.main()

