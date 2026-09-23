import { describe, expect, it } from 'vitest';
import { isInStorage, slotKey, spoolLabel, spoolsBySlot } from './filamentSlots';
import type { FilamentSpool } from '../types';

function spool(overrides: Partial<FilamentSpool>): FilamentSpool {
  return {
    id: 's1', material: 'PLA', manufacturer: null, color: 'Schwarz', location: null,
    diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 500, price: null, imagePng: null,
    colorHex: null, homeLocation: null, unitId: null, slotIndex: null, ...overrides,
  };
}

describe('filamentSlots', () => {
  it('maps loaded spools by slot and leaves storage spools out', () => {
    const loaded = spool({ id: 'a', unitId: 'u1', slotIndex: 2 });
    const stored = spool({ id: 'b', location: 'Regal 1' });
    const map = spoolsBySlot([loaded, stored]);
    expect(map.get(slotKey('u1', 2))).toBe(loaded);
    expect(map.size).toBe(1);
    expect(isInStorage(stored)).toBe(true);
    expect(isInStorage(loaded)).toBe(false);
  });

  it('builds a short label', () => {
    expect(spoolLabel(spool({}))).toBe('PLA Schwarz');
    expect(spoolLabel(spool({ color: null }))).toBe('PLA');
  });
});
