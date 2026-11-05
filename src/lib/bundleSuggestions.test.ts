import { beforeEach, describe, expect, it } from 'vitest';
import {
  loadSuggestionPreferences, nameStem, saveSuggestionPreferences, suggestBundles,
} from './bundleSuggestions';
import { makeModelFile } from '../test/factories';
import type { Bundle } from '../types';

const file = (id: string, name: string, extra: Parameters<typeof makeModelFile>[0] = {}) =>
  makeModelFile({ id, name, folderId: 'f1', ...extra });
const bundle = (ids: string[]): Bundle => ({
  id: 'b', name: 'B', kind: 'parts', createdAt: '2026-11-01T00:00:00Z',
  members: ids.map((fileId, position) => ({ fileId, position, label: null, inTrash: false })),
});
const mw = (id: string, name: string, profile: string | null, model = '123') =>
  file(id, name, { sourcePlatform: 'makerworld', sourceModelId: model, sourceProfileId: profile });

describe('nameStem', () => {
  // One row per ending rule; the left side is a file name, the right side the stem.
  it.each([
    ['Raumschiff Teil 1.3mf', 'raumschiff'],
    ['Raumschiff_Teil_12.3mf', 'raumschiff'],
    ['Raumschiff - Part 2.stl', 'raumschiff'],
    ['Regal_v2.3mf', 'regal'],
    ['Regal v1.2.3mf', 'regal'],
    ['Regal Version 3.3mf', 'regal'],
    ['Regal (2).3mf', 'regal'],
    ['Regal Kopie.3mf', 'regal'],
    ['Regal copy 2.3mf', 'regal'],
    ['Regal 3.3mf', 'regal'],
    ['Regal_v2 Teil 3.3mf', 'regal'],
    ['Lampenschirm.3mf', 'lampenschirm'],
  ])('%s -> %s', (name, stem) => {
    expect(nameStem(name)).toBe(stem);
  });

  it('keeps measures and words that merely look like an ending', () => {
    expect(nameStem('Wandhalter 0,4 mm.3mf')).toBe('wandhalter 0,4 mm');
    expect(nameStem('Revolver.3mf')).toBe('revolver');
    expect(nameStem('Apartment.3mf')).toBe('apartment');
  });

  it('returns nothing when too little is left', () => {
    expect(nameStem('1.3mf')).toBe('');
    expect(nameStem('Teil 2.3mf')).toBe('');
    expect(nameStem('ab 2.3mf')).toBe('');
  });
});

describe('suggestBundles: same platform model', () => {
  it('groups files with one model id and different profile ids as variants', () => {
    const [group, ...rest] = suggestBundles([mw('1', 'Lampe 0,2.3mf', 'p1'), mw('2', 'Lampe 0,4.3mf', 'p2'), mw('3', 'Anderes.3mf', 'p9', '999')], []);
    expect(rest).toEqual([]);
    expect(group).toMatchObject({ reason: 'sameModelId', kind: 'variants', fileIds: ['1', '2'], origin: { platform: 'makerworld', modelId: '123' } });
  });

  it('needs two different profile ids and ignores files without a profile id', () => {
    expect(suggestBundles([mw('1', 'Eins.3mf', 'p1'), mw('2', 'Zwei.3mf', 'p1')], [])).toEqual([]);
    expect(suggestBundles([mw('1', 'Eins.3mf', 'p1'), mw('2', 'Zwei.3mf', null)], [])).toEqual([]);
  });

  it('keeps platforms apart', () => {
    const other = file('2', 'Zwei.3mf', { sourcePlatform: 'printables', sourceModelId: '123', sourceProfileId: 'p2' });
    expect(suggestBundles([mw('1', 'Eins.3mf', 'p1'), other], [])).toEqual([]);
  });

  it('proposes the common name in the first file\'s spelling', () => {
    const [group] = suggestBundles([mw('1', 'Schöne Lampe v1.3mf', 'p1'), mw('2', 'Schöne Lampe v2.3mf', 'p2')], []);
    expect(group.name).toBe('Schöne Lampe');
  });

  it('falls back to the first file name when the names differ', () => {
    const [group] = suggestBundles([mw('1', 'Alpha.3mf', 'p1'), mw('2', 'Beta.3mf', 'p2')], []);
    expect(group.name).toBe('Alpha');
  });
});

describe('suggestBundles: similar name', () => {
  it('groups one stem in one folder and leaves the kind open', () => {
    const [group, ...rest] = suggestBundles([file('1', 'Raumschiff Teil 1.3mf'), file('2', 'Raumschiff Teil 2.3mf'), file('3', 'Vase.3mf')], []);
    expect(rest).toEqual([]);
    expect(group).toMatchObject({ reason: 'similarName', kind: null, fileIds: ['1', '2'], name: 'Raumschiff', origin: { folderId: 'f1' } });
  });

  it('orders the files naturally, so part 2 comes before part 10', () => {
    const [group] = suggestBundles([file('a', 'Schiff Teil 10.3mf'), file('b', 'Schiff Teil 2.3mf')], []);
    expect(group.fileIds).toEqual(['b', 'a']);
  });

  it('does not join folders', () => {
    expect(suggestBundles([file('1', 'Regal_v1.3mf'), file('2', 'Regal_v2.3mf', { folderId: 'f2' })], [])).toEqual([]);
  });

  it('does not suggest a stem that is too short or the same name twice', () => {
    expect(suggestBundles([file('1', 'Teil 1.3mf'), file('2', 'Teil 2.3mf')], [])).toEqual([]);
    expect(suggestBundles([file('1', 'Regal.3mf'), file('2', 'Regal.stl')], [])).toHaveLength(1);
    expect(suggestBundles([file('1', 'Regal.3mf'), file('2', 'regal.3mf')], [])).toEqual([]);
  });

  it('never suggests creator or folder alone', () => {
    expect(suggestBundles([file('1', 'Vase.3mf', { creator: 'Anna' }), file('2', 'Becher.3mf', { creator: 'Anna' })], [])).toEqual([]);
  });
});

describe('suggestBundles: rules for both signals', () => {
  it('skips files that are already in a bundle and groups left with fewer than two', () => {
    const models = [file('1', 'Regal_v1.3mf'), file('2', 'Regal_v2.3mf'), file('3', 'Regal_v3.3mf')];
    expect(suggestBundles(models, [bundle(['1', '2'])])).toEqual([]);
    expect(suggestBundles(models, [bundle(['1', '9'])])).toHaveLength(1);
  });

  it('lets the model id signal take a file before the name signal does', () => {
    const models = [mw('1', 'Lampe v1.3mf', 'p1'), mw('2', 'Lampe v2.3mf', 'p2'), file('3', 'Lampe v3.3mf')];
    const groups = suggestBundles(models, []);
    expect(groups.map(group => [group.reason, group.fileIds])).toEqual([['sameModelId', ['1', '2']]]);
  });

  it('skips a group above the bundle limit', () => {
    const models = Array.from({ length: 201 }, (_, i) => file(`m${i}`, `Serie ${i}.3mf`));
    expect(suggestBundles(models, [])).toEqual([]);
    expect(suggestBundles(models.slice(0, 200), [])).toHaveLength(1);
  });

  it('keeps its key when a file joins the group, so a dismissal still holds', () => {
    const before = suggestBundles([file('1', 'Regal_v1.3mf'), file('2', 'Regal_v2.3mf')], []);
    const after = suggestBundles([file('1', 'Regal_v1.3mf'), file('2', 'Regal_v2.3mf'), file('3', 'Regal_v3.3mf')], []);
    expect(after[0].key).toBe(before[0].key);
  });

  it('changes nothing it is given', () => {
    const models = [file('2', 'Regal_v2.3mf'), file('1', 'Regal_v1.3mf')];
    const copy = structuredClone(models);
    suggestBundles(models, []);
    expect(models).toEqual(copy);
  });
});

describe('suggestion preferences', () => {
  beforeEach(() => localStorage.clear());

  it('start with the hint on and nothing dismissed', () => {
    expect(loadSuggestionPreferences()).toEqual({ hint: true, dismissed: [] });
  });

  it('round-trip, and survive damaged data', () => {
    saveSuggestionPreferences({ hint: false, dismissed: ['a', 'b'] });
    expect(loadSuggestionPreferences()).toEqual({ hint: false, dismissed: ['a', 'b'] });
    localStorage.setItem('3mf-katalog-bundle-suggestions', '{"hint":"x","dismissed":[1,"k"]}');
    expect(loadSuggestionPreferences()).toEqual({ hint: true, dismissed: ['k'] });
    localStorage.setItem('3mf-katalog-bundle-suggestions', 'not json');
    expect(loadSuggestionPreferences()).toEqual({ hint: true, dismissed: [] });
  });

  it('keep only the newest 500 dismissals', () => {
    saveSuggestionPreferences({ hint: true, dismissed: Array.from({ length: 520 }, (_, i) => `k${i}`) });
    const { dismissed } = loadSuggestionPreferences();
    expect(dismissed).toHaveLength(500);
    expect(dismissed[0]).toBe('k20');
  });
});
