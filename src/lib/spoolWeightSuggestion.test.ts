import { describe, expect, it } from 'vitest';
import type { FilamentSpool } from '../types';
import { suggestSpoolWeight } from './spoolWeightSuggestion';

function spool(id: string, manufacturer: string | null, material: string, originalWeightG: number, kind: 'filament' | 'resin' = 'filament'): FilamentSpool {
  return {
    id, material, manufacturer, color: null, location: null, diameterMm: 1.75, originalWeightG, remainingWeightG: originalWeightG,
    price: null, imagePng: null, colorHex: null, colorHexes: [], homeLocation: null, unitId: null, slotIndex: null, kind,
  };
}

const stock = [
  spool('1', 'Polymaker', 'PLA', 1000),
  spool('2', 'Polymaker', 'PLA', 750),
  spool('3', 'Polymaker', 'PETG', 500),
  spool('4', 'Sunlu', 'PLA', 250),
];

describe('suggestSpoolWeight', () => {
  it('uses the newest spool of the same manufacturer and material', () => {
    expect(suggestSpoolWeight(stock, 'filament', 'Polymaker', 'PLA')).toEqual({
      weight: 750, source: 'manufacturerMaterial', manufacturer: 'Polymaker', material: 'PLA',
    });
  });

  it('compares without case and surrounding spaces', () => {
    expect(suggestSpoolWeight(stock, 'filament', ' polymaker ', 'pla ').weight).toBe(750);
  });

  it('falls back to the newest spool of the manufacturer when the material is new', () => {
    expect(suggestSpoolWeight(stock, 'filament', 'Polymaker', 'ASA')).toEqual({
      weight: 500, source: 'manufacturer', manufacturer: 'Polymaker',
    });
    expect(suggestSpoolWeight(stock, 'filament', 'Polymaker', '').weight).toBe(500);
  });

  it('falls back to 1000 g for an unknown or empty manufacturer', () => {
    expect(suggestSpoolWeight(stock, 'filament', 'Unknown', 'PLA')).toEqual({ weight: 1000, source: 'default' });
    expect(suggestSpoolWeight(stock, 'filament', '', 'PLA')).toEqual({ weight: 1000, source: 'default' });
    expect(suggestSpoolWeight([], 'filament', 'Polymaker', 'PLA')).toEqual({ weight: 1000, source: 'default' });
  });

  it('orders by id as a number, not as text', () => {
    const list = [spool('9', 'A', 'PLA', 100), spool('10', 'A', 'PLA', 200)];
    expect(suggestSpoolWeight(list, 'filament', 'A', 'PLA').weight).toBe(200);
  });

  it('ignores spools of the other kind', () => {
    const list = [spool('1', 'Elegoo', 'Standard', 500, 'resin')];
    expect(suggestSpoolWeight(list, 'filament', 'Elegoo', 'Standard')).toEqual({ weight: 1000, source: 'default' });
    expect(suggestSpoolWeight(list, 'resin', 'Elegoo', 'Standard').weight).toBe(500);
  });
});
