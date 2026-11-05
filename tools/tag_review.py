"""Read-only tag inventory. Supply an owner-provided catalog COPY, never the live DB."""
import argparse
import json
import sqlite3
from pathlib import Path


def analyze(copy, dictionary):
    copy = Path(copy).resolve(strict=True)
    if any(Path(str(copy) + suffix).exists() for suffix in ('-wal', '-shm', '-journal')):
        raise ValueError('Supply a standalone, checkpointed catalog copy without WAL/journal sidecars')
    ambiguous = {'mini', 'large', 'grande'}
    aliases = {name.strip().lower(): key for key, names in dictionary.items() for name in names.values() if name.strip().lower() not in ambiguous}
    with sqlite3.connect(copy.as_uri() + '?mode=ro&immutable=1') as db:
        db.execute('PRAGMA query_only=ON')
        db.execute('BEGIN')
        tags = dict(db.execute('SELECT id, name FROM tags'))
        links = list(db.execute('SELECT file_id, tag_id FROM file_tags'))
    grouped = {}
    for tag_id, name in tags.items():
        normalized = name.strip().lower()
        grouped.setdefault(normalized, {'name': normalized, 'assignments': 0, 'files': set(), 'canonical': aliases.get(normalized)})
    canonical_links = set()
    for file_id, tag_id in links:
        name = tags[tag_id].strip().lower()
        grouped[name]['assignments'] += 1
        grouped[name]['files'].add(file_id)
        canonical_links.add((file_id, aliases.get(name, tags[tag_id])))
    rows = [{**row, 'files': len(row['files'])} for row in grouped.values()]
    return {
        'top40': sorted(rows, key=lambda row: (-row['assignments'], row['name']))[:40],
        'merge_preview': {
            'alias_rows_removed': sum(1 for name in tags.values() if (key := aliases.get(name.strip().lower())) is not None and name != key),
            'links_before': len(links), 'distinct_links_after': len(canonical_links),
        },
        'review_required': 'Exclude proper names, numbers, material abbreviations, filename noise and ambiguous words manually. Owner approves up to 20 new nouns and four translations. Alias colors are lost on desktop merge; back up first.',
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--catalog-copy', required=True, type=Path)
    args = parser.parse_args()
    dictionary = json.loads((Path(__file__).resolve().parents[1] / 'src/lib/autoTags.json').read_text())
    print(json.dumps(analyze(args.catalog_copy, dictionary), ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
