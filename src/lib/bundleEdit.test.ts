import { describe, expect, it } from 'vitest';
import { bundleRoom, duplicateIds, planBundleEdit, selectionBundleState, snapshotBundle, switchNotes, titleSwapSteps } from './bundleEdit';
import { makeModelFile } from '../test/factories';
import type { Bundle } from '../types';

function bundle(id: string, members: [string, number, (string | null)?, boolean?][], extra: Partial<Bundle> = {}): Bundle {
  return {
    id, name: `Bundle ${id}`, kind: 'parts', createdAt: '2026-11-01T00:00:00Z',
    members: members.map(([fileId, position, label = null, inTrash = false]) => ({ fileId, position, label, inTrash })),
    ...extra,
  };
}

/** What the backend does for "make title model": the file swaps places with the first one. */
function applySteps(order: string[], steps: string[]) {
  const list = [...order];
  for (const id of steps) {
    const at = list.indexOf(id);
    [list[0], list[at]] = [list[at], list[0]];
  }
  return list;
}
function permutations<T>(items: T[]): T[][] {
  if (items.length <= 1) return [items];
  return items.flatMap((item, i) => permutations([...items.slice(0, i), ...items.slice(i + 1)]).map(rest => [item, ...rest]));
}

describe('titleSwapSteps', () => {
  it('reaches every order of up to five files, each from the same start', () => {
    for (const size of [2, 3, 4, 5]) {
      const start = ['a', 'b', 'c', 'd', 'e'].slice(0, size);
      for (const wanted of permutations(start)) {
        const steps = titleSwapSteps(start, wanted);
        expect(applySteps(start, steps), `to ${wanted.join('')}`).toEqual(wanted);
        // At most two swaps for each place behind the first.
        expect(steps.length).toBeLessThanOrEqual(2 * (size - 1));
      }
    }
  });

  it('needs no step when the order does not change', () => {
    expect(titleSwapSteps(['a', 'b', 'c'], ['a', 'b', 'c'])).toEqual([]);
  });

  it('uses a single step to make the second file the title model of two', () => {
    expect(titleSwapSteps(['a', 'b'], ['b', 'a'])).toEqual(['b']);
  });
});

describe('planBundleEdit', () => {
  const b = bundle('b1', [['1', 0, 'big'], ['2', 1], ['3', 2, 'small']], { name: 'Holder', kind: 'variants' });
  const row = (fileId: string, label = '', keep = true) => ({ fileId, label, keep });

  it('changes nothing when nothing was touched', () => {
    const plan = planBundleEdit(b, { name: 'Holder', kind: 'variants', rows: [row('1', 'big'), row('2'), row('3', 'small')] });
    expect(plan).toEqual({ dissolve: false, removeIds: [], titleSteps: [], labelChanges: [] });
  });

  it('carries name, kind, new labels and an empty label that clears one', () => {
    const plan = planBundleEdit(b, { name: '  Holder v2 ', kind: 'parts', rows: [row('1', ''), row('2', ' mid '), row('3', 'small')] });
    expect(plan.name).toBe('Holder v2');
    expect(plan.kind).toBe('parts');
    expect(plan.labelChanges).toEqual([{ fileId: '1', label: '' }, { fileId: '2', label: 'mid' }]);
  });

  it('turns a new order into title swaps that produce it', () => {
    const plan = planBundleEdit(b, { name: 'Holder', kind: 'variants', rows: [row('3', 'small'), row('1', 'big'), row('2')] });
    expect(applySteps(['1', '2', '3'], plan.titleSteps)).toEqual(['3', '1', '2']);
  });

  it('removes the files that were taken out and orders the rest', () => {
    const plan = planBundleEdit(b, { name: 'Holder', kind: 'variants', rows: [row('3', 'small'), row('2'), row('1', 'big', false)] });
    expect(plan.removeIds).toEqual(['1']);
    expect(applySteps(['2', '3'], plan.titleSteps)).toEqual(['3', '2']);
    expect(plan.dissolve).toBe(false);
  });

  it('dissolves when fewer than two living members stay', () => {
    const plan = planBundleEdit(b, { name: 'Holder', kind: 'variants', rows: [row('1', 'big'), row('2', '', false), row('3', 'small', false)] });
    expect(plan.dissolve).toBe(true);
  });

  it('keeps a member in the trash in its place while the living ones are ordered', () => {
    const withTrash = bundle('b2', [['1', 0], ['9', 1, null, true], ['2', 2], ['3', 3]]);
    const plan = planBundleEdit(withTrash, { name: withTrash.name, kind: 'parts', rows: [row('3'), row('1'), row('2')] });
    expect(applySteps(['1', '9', '2', '3'], plan.titleSteps)).toEqual(['3', '9', '1', '2']);
  });
});

describe('snapshotBundle', () => {
  it('keeps name, kind, order and labels of the living members', () => {
    const b = bundle('b1', [['2', 1, 'b'], ['1', 0, 'a'], ['9', 2, 'gone', true]], { name: 'Pair', kind: 'variants' });
    expect(snapshotBundle(b)).toEqual({ name: 'Pair', kind: 'variants', members: [{ fileId: '1', label: 'a' }, { fileId: '2', label: 'b' }] });
  });
});

describe('selectionBundleState', () => {
  const a = bundle('a', [['1', 0], ['2', 1], ['3', 2]]);
  const c = bundle('c', [['4', 0], ['5', 1]]);

  it('knows which bundles a selection covers completely', () => {
    const state = selectionBundleState(['1', '2', '3', '4', '7'], [a, c]);
    expect(state.covered.map(x => x.id)).toEqual(['a']);
    expect(state.bundledIds).toEqual(['1', '2', '3', '4']);
    expect(state.loose).toBe(2);
    expect(state.exact).toBeNull();
  });

  it('names the exact bundle only when nothing else is selected', () => {
    expect(selectionBundleState(['4', '5'], [a, c]).exact?.id).toBe('c');
    expect(selectionBundleState(['4', '5', '7'], [a, c]).exact).toBeNull();
    expect(selectionBundleState(['4'], [a, c]).covered).toEqual([]);
  });

  it('ignores a member in the trash when it checks the cover', () => {
    const withTrash = bundle('t', [['1', 0], ['2', 1], ['9', 2, null, true]]);
    expect(selectionBundleState(['1', '2'], [withTrash]).exact?.id).toBe('t');
    // One living member is no card, so there is nothing to cover.
    const single = bundle('s', [['1', 0], ['9', 1, null, true]]);
    expect(selectionBundleState(['1'], [single]).covered).toEqual([]);
  });
});

describe('switchNotes', () => {
  const a = bundle('a', [['1', 0], ['2', 1], ['3', 2]]);

  it('lists files that leave another bundle and how many stay behind', () => {
    const notes = switchNotes(['3', '8'], [a], null);
    expect(notes.map(note => [note.fileId, note.from.id, note.left])).toEqual([['3', 'a', 2]]);
  });

  it('counts a bundle as dissolved when too few stay', () => {
    expect(switchNotes(['2', '3'], [a], null)[0].left).toBe(1);
  });

  it('says nothing about files that already are in the target', () => {
    expect(switchNotes(['1', '2'], [a], 'a')).toEqual([]);
  });
});

describe('duplicateIds', () => {
  it('marks later files with the content of an earlier one', () => {
    const files = [
      makeModelFile({ id: '1', contentHash: 'x' }), makeModelFile({ id: '2', contentHash: 'y' }),
      makeModelFile({ id: '3', contentHash: 'x' }), makeModelFile({ id: '4', contentHash: null }), makeModelFile({ id: '5', contentHash: null }),
    ];
    expect([...duplicateIds(files)]).toEqual(['3']);
  });
});

describe('bundleRoom', () => {
  it('counts only files that are not yet members and checks the limit', () => {
    const b = bundle('a', [['1', 0], ['2', 1]]);
    expect(bundleRoom(b, ['2', '3'])).toEqual({ total: 3, fits: true });
    const big = bundle('big', Array.from({ length: 200 }, (_, i) => [String(i), i] as [string, number]));
    expect(bundleRoom(big, ['x'])).toEqual({ total: 201, fits: false });
    expect(bundleRoom(big, ['5'])).toEqual({ total: 200, fits: true });
  });
});
