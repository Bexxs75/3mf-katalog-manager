import hashlib
import sqlite3
import tempfile
import unittest
from pathlib import Path
from tag_review import analyze

class TagReviewTests(unittest.TestCase):
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

    def test_missing_copy_is_not_created(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'missing.db'
            with self.assertRaises(FileNotFoundError): analyze(path, {})
            self.assertFalse(path.exists())

if __name__ == '__main__': unittest.main()
