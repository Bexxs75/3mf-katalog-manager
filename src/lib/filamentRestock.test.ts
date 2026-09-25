import { describe, expect, it } from 'vitest';
import type { FilamentSpool } from '../types';
import {
  RESTOCK_MAX_COUNT,
  clampRestockCount,
  parseDecimalInput,
  restockDefaults,
  restockSpoolLabel,
  roundTenth,
  validateRestockInput,
} from './filamentRestock';

function spool(overrides: Partial<FilamentSpool> = {}): FilamentSpool {
  return {
    id: 's1', material: 'PETG', manufacturer: null, color: 'Rot', location: 'Regal 1',
    diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 200, price: 24.9, imagePng: null,
    colorHex: null, homeLocation: null, unitId: null, slotIndex: null, kind: 'filament', ...overrides,
  };
}

describe('restockDefaults', () => {
  it('uses original amount, price and storage location of the template', () => {
    expect(restockDefaults(spool())).toEqual({ weight: 1000, price: 24.9, location: 'Regal 1' });
    expect(restockDefaults(spool({ kind: 'resin', originalWeightG: 500 })).weight).toBe(500);
  });

  it('falls back to the home location when the template sits in a printer', () => {
    expect(restockDefaults(spool({ location: null, homeLocation: 'Technik', unitId: 'u1', slotIndex: 0 })).location).toBe('Technik');
    expect(restockDefaults(spool({ location: '  ', homeLocation: 'Technik' })).location).toBe('Technik');
    expect(restockDefaults(spool({ location: null, homeLocation: null })).location).toBe('');
  });
});

describe('restockSpoolLabel', () => {
  it('joins material and color with a middle dot', () => {
    expect(restockSpoolLabel(spool())).toBe('PETG · Rot');
    expect(restockSpoolLabel(spool({ color: null }))).toBe('PETG');
  });
});

describe('clampRestockCount / roundTenth', () => {
  it('keeps the count between 1 and 20', () => {
    expect(clampRestockCount(0)).toBe(1);
    expect(clampRestockCount(5)).toBe(5);
    expect(clampRestockCount(99)).toBe(RESTOCK_MAX_COUNT);
    expect(clampRestockCount(Number.NaN)).toBe(1);
  });

  it('rounds to one decimal', () => {
    expect(roundTenth(12.345)).toBe(12.3);
    expect(roundTenth(954.6000001)).toBe(954.6);
  });
});

describe('parseDecimalInput', () => {
  it('accepts comma and dot, treats empty as null and rejects garbage', () => {
    expect(parseDecimalInput('29,95')).toBe(29.95);
    expect(parseDecimalInput(' 29.95 ')).toBe(29.95);
    expect(parseDecimalInput('30')).toBe(30);
    expect(parseDecimalInput('')).toBeNull();
    expect(parseDecimalInput('1.234,50')).toBeUndefined();
    expect(parseDecimalInput('-3')).toBeUndefined();
    expect(parseDecimalInput('abc')).toBeUndefined();
  });
});

describe('validateRestockInput', () => {
  it('builds the backend request with the amount rounded to 0.1', () => {
    expect(validateRestockInput({ count: 3, weight: '750,04', price: '19,99', location: ' Regal 2 ' })).toEqual({
      ok: true,
      value: { count: 3, weight: 750, price: 19.99, location: 'Regal 2' },
    });
  });

  it('sends an empty price and location as null', () => {
    expect(validateRestockInput({ count: 1, weight: '1000', price: '', location: '' })).toEqual({
      ok: true,
      value: { count: 1, weight: 1000, price: null, location: null },
    });
  });

  it('rejects a missing, zero or unreadable amount', () => {
    for (const weight of ['', '0', '0,04', 'abc']) {
      expect(validateRestockInput({ count: 1, weight, price: '', location: '' })).toEqual({ ok: false, error: 'weight' });
    }
  });

  it('rejects an unreadable price', () => {
    expect(validateRestockInput({ count: 1, weight: '1000', price: 'x', location: '' })).toEqual({ ok: false, error: 'price' });
  });
});
