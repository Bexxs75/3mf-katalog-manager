import hashlib
import sqlite3
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path
from tag_review import analyze

class TagReviewTests(unittest.TestCase):
    def test_orphaned_links_are_counted_without_losing_valid_links(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'copy.db'
            with sqlite3.connect(path) as db:
                db.executescript("CREATE TABLE tags(id INTEGER, name TEXT); CREATE TABLE file_tags(file_id INTEGER, tag_id INTEGER); INSERT INTO tags VALUES(1,'lid'); INSERT INTO file_tags VALUES(1,1),(2,99),(3,99);")
            result = analyze(path, {})
            self.assertEqual(result['orphaned_file_tags'], 2)
            self.assertEqual(result['merge_preview']['links_before'], 3)
            self.assertEqual(result['merge_preview']['distinct_links_after'], 1)
            self.assertEqual(result['top40'][0]['assignments'], 1)

    def test_alias_collisions_are_reported_and_not_arbitrarily_merged(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'copy.db'
            with sqlite3.connect(path) as db:
                db.executescript("CREATE TABLE tags(id INTEGER, name TEXT); CREATE TABLE file_tags(file_id INTEGER, tag_id INTEGER); INSERT INTO tags VALUES(1,'shared'); INSERT INTO file_tags VALUES(1,1);")
            dictionary = {'a': {'en': 'Shared', 'fr': 'shared', 'es': 'mini'},
                          'b': {'de': ' shared ', 'en': 'mini'}}
            result = analyze(path, dictionary)
            self.assertEqual(result['alias_collisions'], {'mini': ['a', 'b'], 'shared': ['a', 'b']})
            self.assertIsNone(result['top40'][0]['canonical'])
            self.assertEqual(result['merge_preview']['alias_rows_removed'], 0)
            self.assertEqual(analyze(path, dict(reversed(list(dictionary.items())))), result)

    def test_read_only_counts_and_merge_preview(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'copy.db'
            with sqlite3.connect(path) as db:
                db.executescript("CREATE TABLE tags(id INTEGER, name TEXT); CREATE TABLE file_tags(file_id INTEGER, tag_id INTEGER); INSERT INTO tags VALUES(1,'Lid'),(2,'lid'),(3,'deckel'),(4,'PLA'); INSERT INTO file_tags VALUES(1,1),(2,1),(1,2),(1,3),(3,4);")
            before = hashlib.sha256(path.read_bytes()).digest()
            result = analyze(path, {'deckel': {'de':'deckel','en':'lid','es':'tapa','fr':'couvercle'}})
            self.assertEqual(result['top40'][0], {'name':'lid','assignments':3,'files':2,'canonical':'deckel'})
            self.assertEqual(result['merge_preview']['alias_rows_removed'], 2)
            self.assertEqual(result['merge_preview']['distinct_links_after'], 3)
            self.assertEqual(hashlib.sha256(path.read_bytes()).digest(), before)
            self.assertEqual(sorted(p.name for p in path.parent.iterdir()), ['copy.db'])
    def test_ambiguous_aliases_are_not_merged_and_wal_copies_are_refused(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'copy.db'
            with sqlite3.connect(path) as db:
                db.executescript("CREATE TABLE tags(id INTEGER, name TEXT); CREATE TABLE file_tags(file_id INTEGER, tag_id INTEGER); INSERT INTO tags VALUES(1,'mini'); INSERT INTO file_tags VALUES(1,1);")
            result = analyze(path, {'miniatur': {'de':'miniatur','en':'mini','es':'miniatura','fr':'miniature'}})
            self.assertEqual(result['merge_preview']['alias_rows_removed'], 0)
            Path(str(path) + '-wal').touch()
            with self.assertRaises(ValueError): analyze(path, {})

    def test_connection_is_read_only_and_closed(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'copy.db'
            db = sqlite3.connect(path)
            db.executescript("CREATE TABLE tags(id INTEGER, name TEXT); CREATE TABLE file_tags(file_id INTEGER, tag_id INTEGER);")
            db.close()
            connect = sqlite3.connect
            connections = []

            def inspect_connection(*args, **kwargs):
                self.assertTrue(kwargs.get("uri"))
                connection = connect(*args, **kwargs)
                connections.append(connection)
                # Probe before query_only is enabled: the URI itself must deny writes.
                with self.assertRaises(sqlite3.OperationalError):
                    connection.execute('CREATE TABLE forbidden(value INTEGER)')
                return connection

            with patch('tag_review.sqlite3.connect', side_effect=inspect_connection):
                analyze(path, {})
            with self.assertRaises(sqlite3.ProgrammingError):
                connections[0].execute('SELECT 1')

    def test_missing_copy_is_not_created(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'missing.db'
            with self.assertRaises(FileNotFoundError): analyze(path, {})
            self.assertFalse(path.exists())

if __name__ == '__main__': unittest.main()
