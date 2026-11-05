import { describe, expect, it } from 'vitest';
import { commonTags, favoriteFlips, memberTitle, notQueued, panelMembers, panelSummary, printStatusFlips } from './bundlePanel';
import { makeModelFile } from '../test/factories';
import type { Bundle, BundleMember } from '../types';

function member(fileId: string, position: number, extra: Partial<BundleMember> = {}): BundleMember {
  return { fileId, position, label: null, inTrash: false, ...extra };
}
const bundle = (members: BundleMember[], kind: Bundle['kind'] = 'parts'): Bundle =>
  ({ id: 'b', name: 'B', kind, createdAt: '2026-11-01T00:00:00Z', members });

describe('panelMembers', () => {
  const models = [makeModelFile({ id: 'a', name: 'A' }), makeModelFile({ id: 'b', name: 'B' }), makeModelFile({ id: 'c', name: 'C' })];

  it('lists the living members by position, the first one as title, also when a filter hid them', () => {
    const list = panelMembers(bundle([member('c', 2), member('a', 0), member('b', 1, { label: 'Lid' })]), models);
    expect(list.map(entry => entry.model.id)).toEqual(['a', 'b', 'c']);
    expect(list.map(entry => entry.isTitle)).toEqual([true, false, false]);
    expect(list[1].label).toBe('Lid');
  });

  it('leaves out members in the trash and ids the catalog does not know', () => {
    const list = panelMembers(bundle([member('a', 0, { inTrash: true }), member('b', 1), member('gone', 2), member('c', 3)]), models);
    expect(list.map(entry => entry.model.id)).toEqual(['b', 'c']);
    expect(list[0].isTitle).toBe(true);
  });
});

describe('memberTitle', () => {
  it('prefers the label and ignores a blank one', () => {
    const model = makeModelFile({ name: 'file.stl' });
    expect(memberTitle({ model, label: '0.4 mm', isTitle: false })).toBe('0.4 mm');
    expect(memberTitle({ model, label: '  ', isTitle: false })).toBe('file.stl');
    expect(memberTitle({ model, label: null, isTitle: false })).toBe('file.stl');
  });
});

describe('panelSummary', () => {
  const a = makeModelFile({ id: 'a', estimatedWeightG: 10, materials: [{ name: 'PLA', displayColor: null }], printFacts: { plateCount: 1, printSeconds: 600, printSecondsComplete: true } });
  const b = makeModelFile({ id: 'b', estimatedWeightG: null, materials: [{ name: 'PLA', displayColor: null }, { name: 'PETG', displayColor: null }] });

  it('adds up parts and says which sums are incomplete', () => {
    const summary = panelSummary(bundle([]), [a, b]);
    expect(summary.summable).toBe(true);
    expect(summary.weightG).toBe(10);
    expect(summary.weightComplete).toBe(false);
    expect(summary.printSecondsComplete).toBe(false);
    expect(summary.materials).toEqual(['PLA']);
  });

  it('marks variants as not summable', () => {
    expect(panelSummary(bundle([], 'variants'), [a, b]).summable).toBe(false);
  });

  it('knows a complete weight', () => {
    expect(panelSummary(bundle([]), [a, makeModelFile({ id: 'c', estimatedWeightG: 5 })]).weightComplete).toBe(true);
  });
});

describe('plans for "apply to all"', () => {
  const a = makeModelFile({ id: 'a', tags: ['x', 'y'], printStatus: 'printed', favorite: true, queuePosition: 1 });
  const b = makeModelFile({ id: 'b', tags: ['x'], printStatus: 'not_printed', favorite: false, queuePosition: null });

  it('commonTags keeps only what every model has', () => {
    expect(commonTags([a, b])).toEqual(['x']);
    expect(commonTags([])).toEqual([]);
  });
  it('only the models that differ flip', () => {
    expect(printStatusFlips([a, b], true).map(m => m.id)).toEqual(['b']);
    expect(printStatusFlips([a, b], false).map(m => m.id)).toEqual(['a']);
    expect(favoriteFlips([a, b], true).map(m => m.id)).toEqual(['b']);
  });
  it('notQueued skips models already in the queue', () => {
    expect(notQueued([a, b]).map(m => m.id)).toEqual(['b']);
  });
});
