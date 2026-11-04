import { describe, expect, it } from 'vitest';
import {
  MAX_BUNDLE_MEMBERS, MIN_BUNDLE_MEMBERS, bundleAggregate, bundleIdOfKey, bundleKey, buildCards, expandSelection,
  fileIdsOfBundlesMatching, indexBundles, sortCards, sortValueOfCard, type BundleCard,
} from './bundles';
import { filterAndSortModels } from './catalogFilters';
import { makeModelFile } from '../test/factories';
import type { Bundle, BundleMember } from '../types';

function member(fileId: string, position: number, extra: Partial<BundleMember> = {}): BundleMember {
  return { fileId, position, label: null, inTrash: false, ...extra };
}
function bundle(id: string, members: BundleMember[], extra: Partial<Bundle> = {}): Bundle {
  return { id, name: `Bundle ${id}`, kind: 'parts', createdAt: '2026-11-01T00:00:00Z', members, ...extra };
}

describe('bundle limits', () => {
  it('match the backend', () => {
    expect([MIN_BUNDLE_MEMBERS, MAX_BUNDLE_MEMBERS]).toEqual([2, 200]);
  });
});

describe('bundle keys', () => {
  it('tell a bundle card from a file id', () => {
    expect(bundleKey('7')).toBe('bundle:7');
    expect(bundleIdOfKey('bundle:7')).toBe('7');
    expect(bundleIdOfKey('7')).toBeNull();
  });
});

describe('indexBundles', () => {
  it('maps every member file to its bundle', () => {
    const a = bundle('a', [member('1', 0), member('2', 1)]);
    const index = indexBundles([a]);
    expect(index.get('1')).toBe(a);
    expect(index.get('2')).toBe(a);
    expect(index.get('3')).toBeUndefined();
  });
});

describe('buildCards', () => {
  const models = ['1', '2', '3', '4'].map(id => makeModelFile({ id, name: `M${id}` }));

  it('shows files without a bundle as single cards, in list order', () => {
    const cards = buildCards(models, [], { collapse: true });
    expect(cards.map(c => [c.type, c.key])).toEqual([['model', '1'], ['model', '2'], ['model', '3'], ['model', '4']]);
  });

  it('turns a bundle with two visible members into one card at the place of its first visible member', () => {
    const cards = buildCards(models, [bundle('b', [member('3', 0), member('1', 1)])], { collapse: true });
    expect(cards.map(c => c.key)).toEqual(['bundle:b', '2', '4']);
    const card = cards[0] as BundleCard;
    expect(card.title.id).toBe('3');
    expect(card.visibleMembers.map(m => m.model.id)).toEqual(['3', '1']);
    expect(card.livingCount).toBe(2);
  });

  it('keeps the labels and positions of the members', () => {
    const [card] = buildCards(models, [bundle('b', [member('1', 0, { label: '0,4 mm' }), member('2', 1)])], { collapse: true }) as BundleCard[];
    expect(card.visibleMembers.map(m => [m.model.id, m.label, m.position])).toEqual([['1', '0,4 mm', 0], ['2', null, 1]]);
  });

  it('counts members the filters hid ("2 of 3") and lists only the visible ones', () => {
    const visible = models.slice(0, 2);
    const [card] = buildCards(visible, [bundle('b', [member('1', 0), member('2', 1), member('9', 2)])], { collapse: true }) as BundleCard[];
    expect(card.visibleMembers).toHaveLength(2);
    expect(card.livingCount).toBe(3);
  });

  it('takes the first visible member as the title when the title model itself was filtered out', () => {
    const [card] = buildCards(models.slice(1, 3), [bundle('b', [member('1', 0), member('2', 1), member('3', 2)])], { collapse: true }) as BundleCard[];
    expect(card.title.id).toBe('2');
  });

  it('leaves a single visible member a single card marked as part of the bundle', () => {
    const cards = buildCards(models.slice(0, 2), [bundle('b', [member('1', 0), member('9', 1)], { kind: 'variants' })], { collapse: true });
    expect(cards[0]).toMatchObject({ type: 'model', key: '1', partOf: { bundleId: 'b', name: 'Bundle b', kind: 'variants' } });
    expect(cards[1]).toMatchObject({ type: 'model', key: '2', partOf: null });
  });

  it('does not count a member in the trash as visible or living', () => {
    const cards = buildCards(models.slice(0, 3), [bundle('b', [member('1', 0), member('2', 1, { inTrash: true }), member('3', 2)])], { collapse: true });
    const card = cards.find(c => c.type === 'bundle') as BundleCard;
    expect(card.visibleMembers.map(m => m.model.id)).toEqual(['1', '3']);
    expect(card.livingCount).toBe(2);
    expect(cards.map(c => c.key)).toEqual(['bundle:b', '2']);
  });

  it('shows every file on its own, marked, when the bundles are not collapsed', () => {
    const cards = buildCards(models, [bundle('b', [member('1', 0), member('2', 1)])], { collapse: false });
    expect(cards.map(c => c.type)).toEqual(['model', 'model', 'model', 'model']);
    expect(cards.map(c => (c.type === 'model' ? c.partOf?.bundleId ?? null : 'x'))).toEqual(['b', 'b', null, null]);
  });

  it('builds a card per bundle and never shows a file twice', () => {
    const cards = buildCards(models, [bundle('a', [member('1', 0), member('2', 1)]), bundle('b', [member('3', 0), member('4', 1)])], { collapse: true });
    expect(cards.map(c => c.key)).toEqual(['bundle:a', 'bundle:b']);
  });

  it('works on the result of the filters: a filter that keeps one member leaves a marked single card', () => {
    const all = [makeModelFile({ id: '1', name: 'Vase body' }), makeModelFile({ id: '2', name: 'Vase lid' }), makeModelFile({ id: '3', name: 'Other' })];
    const b = bundle('b', [member('1', 0), member('2', 1)]);
    const both = filterAndSortModels(all, [], { activeFolderId: 'all', activeTag: null, query: 'vase', sort: 'name' });
    expect(buildCards(both, [b], { collapse: true }).map(c => c.key)).toEqual(['bundle:b']);
    const one = filterAndSortModels(all, [], { activeFolderId: 'all', activeTag: null, query: 'lid', sort: 'name' });
    expect(buildCards(one, [b], { collapse: true })).toMatchObject([{ type: 'model', key: '2', partOf: { bundleId: 'b' } }]);
  });
});

describe('expandSelection', () => {
  const bundles = [bundle('b', [member('1', 0), member('2', 1, { inTrash: true }), member('3', 2)])];

  it('replaces a bundle card by its living members, also the ones a filter hides', () => {
    expect(expandSelection(['bundle:b'], bundles)).toEqual(['1', '3']);
  });
  it('keeps plain ids and lists every file once', () => {
    expect(expandSelection(['9', 'bundle:b', '1', '9'], bundles)).toEqual(['9', '1', '3']);
  });
  it('ignores a bundle that is gone', () => {
    expect(expandSelection(['bundle:zzz', '5'], bundles)).toEqual(['5']);
  });
  it('accepts any iterable such as a Set', () => {
    expect(expandSelection(new Set(['bundle:b']), bundles)).toEqual(['1', '3']);
  });
});

describe('fileIdsOfBundlesMatching', () => {
  const bundles = [
    bundle('a', [member('1', 0), member('2', 1, { inTrash: true })], { name: 'Wall Vase' }),
    bundle('b', [member('3', 0), member('4', 1)], { name: 'Hinge' }),
  ];
  it('finds the living members of a bundle by its name, case ignored', () => {
    expect([...fileIdsOfBundlesMatching(bundles, 'vASe')]).toEqual(['1']);
  });
  it('finds nothing for an empty query or no match', () => {
    expect(fileIdsOfBundlesMatching(bundles, '  ').size).toBe(0);
    expect(fileIdsOfBundlesMatching(bundles, 'zebra').size).toBe(0);
  });
});

describe('bundleAggregate', () => {
  it('sums size, volume, weight, print time and printed models', () => {
    const a = makeModelFile({ id: '1', fileSizeBytes: 100, volumeCm3: 2, estimatedWeightG: 10, printStatus: 'printed', printFacts: { plateCount: 1, printSeconds: 60, printSecondsComplete: true } });
    const b = makeModelFile({ id: '2', fileSizeBytes: 50, volumeCm3: null, estimatedWeightG: 5, printStatus: 'not_printed', printFacts: { plateCount: 1, printSeconds: 30 } });
    expect(bundleAggregate([a, b])).toMatchObject({
      count: 2, sizeBytes: 150, volumeCm3: 2, weightG: 15, printSeconds: 90, printSecondsComplete: false, printedCount: 1,
    });
  });
  it('reports an unknown value as null, never as 0', () => {
    const a = makeModelFile({ id: '1', volumeCm3: null, estimatedWeightG: null });
    expect(bundleAggregate([a])).toMatchObject({ volumeCm3: null, weightG: null, printSeconds: null, printSecondsComplete: false });
  });
  it('calls the print time complete when every member has a complete one', () => {
    const facts = { plateCount: 1, printSeconds: 10, printSecondsComplete: true as const };
    expect(bundleAggregate([makeModelFile({ id: '1', printFacts: facts }), makeModelFile({ id: '2', printFacts: facts })]).printSecondsComplete).toBe(true);
  });
  it('joins the tags of all members, each once, sorted', () => {
    const a = makeModelFile({ id: '1', tags: ['vase', 'deko'] });
    const b = makeModelFile({ id: '2', tags: ['deko', 'abc'] });
    expect(bundleAggregate([a, b]).tags).toEqual(['abc', 'deko', 'vase']);
  });
  it('tells one license from none and from mixed', () => {
    const lic = (id: string, license: string | null) => makeModelFile({ id, license });
    expect(bundleAggregate([lic('1', null), lic('2', null)]).license).toEqual({ state: 'none', value: null });
    expect(bundleAggregate([lic('1', 'CC-BY'), lic('2', 'CC-BY')]).license).toEqual({ state: 'single', value: 'CC-BY' });
    expect(bundleAggregate([lic('1', 'CC-BY'), lic('2', 'CC0')]).license).toEqual({ state: 'mixed', value: null });
    expect(bundleAggregate([lic('1', 'CC-BY'), lic('2', null)]).license).toEqual({ state: 'mixed', value: null });
  });
  it('handles an empty list', () => {
    expect(bundleAggregate([])).toMatchObject({ count: 0, sizeBytes: 0, volumeCm3: null, printSecondsComplete: false, license: { state: 'none' } });
  });
});

describe('sorting cards', () => {
  const a = makeModelFile({ id: '1', name: 'A', importedAt: '2026-03-01', fileModifiedAt: '2026-03-05', fileSizeBytes: 10, volumeCm3: 1, lastViewedAt: null, printStatus: 'printed' });
  const b = makeModelFile({ id: '2', name: 'B', importedAt: '2026-01-01', fileModifiedAt: null, fileSizeBytes: 20, volumeCm3: null, lastViewedAt: '2026-04-01', printStatus: 'not_printed' });
  const c = makeModelFile({ id: '3', name: 'C', importedAt: '2026-02-01', fileModifiedAt: '2026-02-02', fileSizeBytes: 5, volumeCm3: 4, lastViewedAt: '2026-05-01', printStatus: 'printed' });
  const [card] = buildCards([a, b], [bundle('x', [member('1', 0), member('2', 1)], { name: 'Pair' })], { collapse: true }) as BundleCard[];
  const single = buildCards([c], [], { collapse: true })[0];

  it('uses the earliest import, the latest change, the sums and the share printed for a bundle card', () => {
    expect(sortValueOfCard(card, 'name')).toBe('Pair');
    expect(sortValueOfCard(card, 'imported')).toBe('2026-01-01');
    expect(sortValueOfCard(card, 'modified')).toBe('2026-03-05');
    expect(sortValueOfCard(card, 'size')).toBe(30);
    expect(sortValueOfCard(card, 'vol')).toBe(1);
    expect(sortValueOfCard(card, 'viewed')).toBe('2026-04-01');
    expect(sortValueOfCard(card, 'printStatus')).toBe(0.5);
  });
  it('uses the file for a single card', () => {
    expect(sortValueOfCard(single, 'name')).toBe('C');
    expect(sortValueOfCard(single, 'vol')).toBe(4);
  });
  it('gives null where no member knows the value', () => {
    const [none] = buildCards([makeModelFile({ id: '7', volumeCm3: null, lastViewedAt: null }), makeModelFile({ id: '8', volumeCm3: null, lastViewedAt: null })], [bundle('y', [member('7', 0), member('8', 1)])], { collapse: true });
    expect(sortValueOfCard(none, 'vol')).toBeNull();
    expect(sortValueOfCard(none, 'viewed')).toBeNull();
  });
  it('orders cards and bundles together in both directions, a card without a value last', () => {
    expect(sortCards([card, single], 'size', 'asc').map(x => x.key)).toEqual(['3', 'bundle:x']);
    expect(sortCards([card, single], 'size', 'desc').map(x => x.key)).toEqual(['bundle:x', '3']);
    expect(sortCards([card, single], 'imported').map(x => x.key)).toEqual(['3', 'bundle:x']);
    const b2 = makeModelFile({ id: '8', name: 'Z', fileModifiedAt: null });
    expect(sortCards([buildCards([b2], [], { collapse: true })[0], single], 'modified', 'desc').map(x => x.key)).toEqual(['3', '8']);
  });
  it('breaks ties by name and leaves its input alone', () => {
    const d = buildCards([makeModelFile({ id: '4', name: 'D', fileSizeBytes: 5 }), c], [], { collapse: true });
    const before = d.map(x => x.key);
    expect(sortCards(d, 'size', 'asc').map(x => x.key)).toEqual(['3', '4']);
    expect(d.map(x => x.key)).toEqual(before);
  });
});
