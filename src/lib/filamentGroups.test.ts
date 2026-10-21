import { describe, expect, it } from 'vitest';
import type { FilamentSpool } from '../types';
import { groupFilaments, sameSpoolColor, singleSpoolGroups, spoolColorList } from './filamentGroups';

let nextId = 1;
function spool(over: Partial<FilamentSpool> = {}): FilamentSpool {
  return {
    id: String(nextId++), material: 'PLA', manufacturer: 'Polymaker', color: 'Black', location: 'Shelf A', diameterMm: 1.75,
    originalWeightG: 1000, remainingWeightG: 1000, price: null, imagePng: null, colorHex: '#1a1a1a', colorHexes: [],
    homeLocation: null, unitId: null, slotIndex: null, kind: 'filament', ...over,
  };
}

describe('groupFilaments: what counts as equal', () => {
  it('groups spools with the same material, manufacturer, location, diameter and color', () => {
    const groups = groupFilaments([spool(), spool(), spool()]);
    expect(groups).toHaveLength(1);
    expect(groups[0].spools).toHaveLength(3);
  });

  it.each([
    ['material text', { material: 'PLA+' }],
    ['material case', { material: 'pla' }],
    ['manufacturer', { manufacturer: 'Sunlu' }],
    ['location', { location: 'Shelf B' }],
    ['diameter', { diameterMm: 2.85 }],
    ['color value', { colorHex: '#ff0000' }],
  ])('keeps spools apart that differ in %s', (_label, over) => {
    expect(groupFilaments([spool(), spool(over)])).toHaveLength(2);
  });

  it('treats a missing manufacturer and an empty one alike', () => {
    expect(groupFilaments([spool({ manufacturer: null }), spool({ manufacturer: '' })])).toHaveLength(1);
  });

  it('compares the color value when both spools have one, even if the names agree', () => {
    const groups = groupFilaments([spool({ color: 'Red', colorHex: '#ff0000' }), spool({ color: 'Red', colorHex: '#00ff00' })]);
    expect(groups).toHaveLength(2);
  });

  it('ignores the color name when both spools have a color value', () => {
    const groups = groupFilaments([spool({ color: 'Red', colorHex: '#FF0000' }), spool({ color: 'Rot', colorHex: '#ff0000' })]);
    expect(groups).toHaveLength(1);
  });

  it('falls back to the color name, ignoring case and surrounding spaces, when a value is missing', () => {
    const same = groupFilaments([spool({ color: 'Sage', colorHex: null }), spool({ color: '  sage ', colorHex: '#9caf88' })]);
    expect(same).toHaveLength(1);
    const different = groupFilaments([spool({ color: 'Sage', colorHex: null }), spool({ color: 'Mint', colorHex: '#9caf88' })]);
    expect(different).toHaveLength(2);
  });

  it('treats two spools without any color information as equal, but not one with a name and one without', () => {
    expect(groupFilaments([spool({ color: null, colorHex: null }), spool({ color: ' ', colorHex: null })])).toHaveLength(1);
    expect(groupFilaments([spool({ color: 'Red', colorHex: null }), spool({ color: null, colorHex: null })])).toHaveLength(2);
  });

  it('does not chain colors that only match through a spool without a value', () => {
    const a = spool({ color: 'Red', colorHex: '#ff0000' });
    const b = spool({ color: 'red', colorHex: null });
    const c = spool({ color: 'Red', colorHex: '#aa0000' });
    const groups = groupFilaments([a, b, c]);
    expect(groups.map((g) => g.spools.map((s) => s.id))).toEqual([[a.id, b.id], [c.id]]);
  });

  it('keeps filament and resin apart', () => {
    expect(groupFilaments([spool(), spool({ kind: 'resin' })])).toHaveLength(2);
  });

  it('leaves a spool without a twin as its own group', () => {
    const groups = groupFilaments([spool(), spool({ material: 'PETG' })]);
    expect(groups.map((g) => g.spools.length)).toEqual([1, 1]);
  });
});

describe('multi-color spools', () => {
  const rainbow = ['#dd3333', '#33aa66', '#3366cc'];

  it('reads the list first and falls back to the single value', () => {
    expect(spoolColorList(spool({ colorHex: '#DD3333', colorHexes: ['#DD3333', '#33aa66'] }))).toEqual(['#dd3333', '#33aa66']);
    expect(spoolColorList(spool({ colorHex: '#1A1A1A' }))).toEqual(['#1a1a1a']);
    expect(spoolColorList(spool({ colorHex: null }))).toEqual([]);
  });

  it('groups the same list in the same order', () => {
    const groups = groupFilaments([
      spool({ colorHex: rainbow[0], colorHexes: rainbow }),
      spool({ colorHex: rainbow[0], colorHexes: rainbow.map((c) => c.toUpperCase()) }),
    ]);
    expect(groups).toHaveLength(1);
  });

  it('keeps a different order, a different list and a single color apart', () => {
    const base = spool({ colorHex: rainbow[0], colorHexes: rainbow });
    const reordered = spool({ colorHex: rainbow[2], colorHexes: [...rainbow].reverse() });
    const shorter = spool({ colorHex: rainbow[0], colorHexes: rainbow.slice(0, 2) });
    const single = spool({ colorHex: rainbow[0], colorHexes: [] });
    expect(groupFilaments([base, reordered, shorter, single])).toHaveLength(4);
  });

  it('compares a multi-color spool by name when the other has no color value', () => {
    const multi = spool({ color: 'Rainbow', colorHex: rainbow[0], colorHexes: rainbow });
    const plain = spool({ color: 'rainbow', colorHex: null });
    expect(sameSpoolColor(multi, plain)).toBe(true);
  });
});

describe('loaded spools', () => {
  const label = (unitId: string) => (unitId === 'u1' ? 'X1C · AMS 1' : 'X1C · AMS 2');

  it('forms one group per unit and never mixes with the shelf', () => {
    const groups = groupFilaments(
      [
        spool({ unitId: 'u1', slotIndex: 2, location: null, homeLocation: 'Shelf A' }),
        spool({ unitId: 'u1', slotIndex: 0, location: null, homeLocation: 'Shelf A' }),
        spool({ unitId: 'u2', slotIndex: 0, location: null }),
        spool(),
      ],
      label,
    );
    expect(groups).toHaveLength(3);
    const ams1 = groups.find((g) => g.place.kind === 'unit' && g.place.unitId === 'u1');
    expect(ams1?.spools).toHaveLength(2);
    expect(ams1?.place).toEqual({ kind: 'unit', unitId: 'u1', label: 'X1C · AMS 1', slots: [1, 3] });
    expect(groups.filter((g) => g.place.kind === 'storage')).toHaveLength(1);
  });

  it('ignores the home location of loaded spools', () => {
    const groups = groupFilaments([
      spool({ unitId: 'u1', slotIndex: 0, location: null, homeLocation: 'Shelf A' }),
      spool({ unitId: 'u1', slotIndex: 1, location: null, homeLocation: 'Shelf B' }),
    ]);
    expect(groups).toHaveLength(1);
  });
});

describe('totals, status and order', () => {
  it('sums the remaining and original amount and counts empty spools', () => {
    const [group] = groupFilaments([
      spool({ remainingWeightG: 920.4 }),
      spool({ remainingWeightG: 0 }),
      spool({ remainingWeightG: 640.3 }),
    ]);
    expect(group.remainingG).toBe(1560.7);
    expect(group.originalG).toBe(3000);
    expect(group.emptyCount).toBe(1);
  });

  it('judges the status by the total of the group, not by single spools', () => {
    // One nearly empty spool next to a full one: the group as a whole is fine.
    const [fine] = groupFilaments([spool({ remainingWeightG: 50 }), spool({ remainingWeightG: 1000 })]);
    expect(fine.status).toBe('ok');
    // Two spools at 15 % each: the total is low.
    const [low] = groupFilaments([spool({ remainingWeightG: 150 }), spool({ remainingWeightG: 150 })]);
    expect(low.status).toBe('low');
    const [empty] = groupFilaments([spool({ remainingWeightG: 0 }), spool({ remainingWeightG: 0 })]);
    expect(empty.status).toBe('empty');
  });

  it('puts groups in ascending order of their remaining share', () => {
    const full = spool({ material: 'PETG' });
    const half = spool({ material: 'ABS', remainingWeightG: 500 });
    const empty = spool({ material: 'TPU', remainingWeightG: 0 });
    const low = [spool({ remainingWeightG: 100 }), spool({ remainingWeightG: 300 })];
    const groups = groupFilaments([full, half, ...low, empty]);
    expect(groups.map((g) => g.first.material)).toEqual(['TPU', 'PLA', 'ABS', 'PETG']);
  });

  it('keeps the input order for equal shares', () => {
    const a = spool({ material: 'A' });
    const b = spool({ material: 'B' });
    const c = spool({ material: 'C' });
    expect(groupFilaments([b, c, a]).map((g) => g.first.material)).toEqual(['B', 'C', 'A']);
  });

  it('has no groups for no spools and a ratio of 0 without an original amount', () => {
    expect(groupFilaments([])).toEqual([]);
    expect(groupFilaments([spool({ originalWeightG: 0, remainingWeightG: 0 })])[0].ratio).toBe(0);
  });

  it('does not modify the input', () => {
    const input = [spool({ remainingWeightG: 900 }), spool({ material: 'PETG', remainingWeightG: 10 })];
    const copy = [...input];
    groupFilaments(input);
    expect(input).toEqual(copy);
  });
});

describe('singleSpoolGroups', () => {
  it('makes every spool its own group in the given order', () => {
    const list = [spool({ remainingWeightG: 900 }), spool({ remainingWeightG: 10 })];
    const groups = singleSpoolGroups(list);
    expect(groups.map((g) => g.first.id)).toEqual(list.map((s) => s.id));
    expect(groups.every((g) => g.spools.length === 1)).toBe(true);
  });
});
