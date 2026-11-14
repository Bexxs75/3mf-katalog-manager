import { describe, expect, it } from 'vitest';
import { groupDuplicates } from './duplicateGroups';
import { applyToolView, toolCounts } from './toolViews';
import { makeModelFile } from '../test/factories';

const model = (id: string, contentHash: string | null, importedAt: string, name = id) => makeModelFile({ id, name, contentHash, importedAt });
const day = (n: number) => `2026-10-${String(n).padStart(2, '0')}T12:00:00Z`;

describe('groupDuplicates', () => {
  it('keeps only hashes shared by at least two models and never groups models without a hash', () => {
    const groups = groupDuplicates([
      model('a', 'h1', day(1)), model('b', 'h1', day(2)),
      model('single', 'h2', day(1)),
      model('n1', null, day(1)), model('n2', null, day(2)), model('e1', '', day(1)), model('e2', '', day(2)),
    ]);
    expect(groups.map(g => [g.hash, g.members.map(m => m.id)])).toEqual([['h1', ['a', 'b']]]);
  });

  it('puts the oldest import first in a group and breaks a tie by id, whatever order the list has', () => {
    const list = [model('c', 'h', day(5)), model('b', 'h', day(5)), model('a', 'h', day(9)), model('z', 'h', day(1))];
    const expected = ['z', 'b', 'c', 'a'];
    expect(groupDuplicates(list)[0].members.map(m => m.id)).toEqual(expected);
    expect(groupDuplicates([...list].reverse())[0].members.map(m => m.id)).toEqual(expected);
  });

  it('never takes a record with a broken import time for the oldest', () => {
    const [group] = groupDuplicates([model('bad', 'h', 'not a date'), model('ok', 'h', day(3))]);
    expect(group.members.map(m => m.id)).toEqual(['ok', 'bad']);
  });

  it('orders the groups by the name of their oldest member', () => {
    const groups = groupDuplicates([
      model('z2', 'hz', day(3), 'Zahnrad Kopie'), model('z1', 'hz', day(1), 'Zahnrad'),
      model('a2', 'ha', day(3), 'Adapter neu'), model('a1', 'ha', day(1), 'Adapter'),
    ]);
    expect(groups.map(g => g.hash)).toEqual(['ha', 'hz']);
  });

  it('is the one function behind the sidebar count and the tool view', () => {
    const list = [model('a', 'h1', day(1)), model('b', 'h1', day(2)), model('c', 'h2', day(1)), model('d', 'h2', day(2)), model('e', 'h2', day(3)), model('f', 'h3', day(1))];
    expect(toolCounts(list, new Date(day(20))).duplicateGroups).toBe(groupDuplicates(list).length);
    expect(applyToolView(list, 'duplicates', new Date(day(20))).map(m => m.id)).toEqual(groupDuplicates(list).flatMap(g => g.members.map(m => m.id)));
  });
});
