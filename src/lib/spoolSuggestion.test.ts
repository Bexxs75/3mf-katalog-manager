import { describe, expect, it } from 'vitest';
import type { FilamentSpool } from '../types';
import { suggestSpool } from './spoolSuggestion';

function spool(id: string, remainingWeightG: number): FilamentSpool {
  return {
    id, material: 'PLA', manufacturer: 'Polymaker', color: 'Black', location: 'Shelf A', diameterMm: 1.75,
    originalWeightG: 1000, remainingWeightG, price: null, imagePng: null, colorHex: '#1a1a1a', colorHexes: [],
    homeLocation: null, unitId: null, slotIndex: null, kind: 'filament',
  };
}

describe('suggestSpool', () => {
  it('prefers the smallest rest that is not empty', () => {
    const picked = suggestSpool([spool('1', 920), spool('2', 120), spool('3', 640)]);
    expect(picked?.id).toBe('2');
  });

  it('skips empty spools', () => {
    expect(suggestSpool([spool('1', 0), spool('2', 640), spool('3', 920)])?.id).toBe('2');
  });

  it('takes the smallest rest that still covers the needed amount', () => {
    const list = [spool('1', 920), spool('2', 120), spool('3', 640)];
    expect(suggestSpool(list, 100)?.id).toBe('2');
    expect(suggestSpool(list, 300)?.id).toBe('3');
    expect(suggestSpool(list, 640)?.id).toBe('3');
    expect(suggestSpool(list, 800)?.id).toBe('1');
  });

  it('falls back to the fullest spool when none covers the amount', () => {
    expect(suggestSpool([spool('1', 120), spool('2', 640)], 900)?.id).toBe('2');
  });

  it('gives the older spool the tie', () => {
    expect(suggestSpool([spool('12', 500), spool('3', 500)])?.id).toBe('3');
  });

  it('returns null when nothing is usable', () => {
    expect(suggestSpool([])).toBeNull();
    expect(suggestSpool([spool('1', 0), spool('2', 0)])).toBeNull();
  });

  it('does not change the order of the input', () => {
    const list = [spool('1', 920), spool('2', 120)];
    suggestSpool(list);
    expect(list.map((s) => s.id)).toEqual(['1', '2']);
  });
});
