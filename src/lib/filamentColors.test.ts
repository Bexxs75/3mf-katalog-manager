import { describe, expect, it } from 'vitest';
import { FILAMENT_PALETTE, UNIT_TEMPLATES, isValidColorHex } from './filamentColors';

describe('filamentColors', () => {
  it('validates #rrggbb values', () => {
    expect(isValidColorHex('#1a1A1a')).toBe(true);
    expect(isValidColorHex('1a1a1a')).toBe(false);
    expect(isValidColorHex('#1a1a1')).toBe(false);
    expect(isValidColorHex('#1a1a1g')).toBe(false);
  });

  it('has 16 valid palette colors and a template for every unit kind', () => {
    expect(FILAMENT_PALETTE).toHaveLength(16);
    expect(FILAMENT_PALETTE.every(isValidColorHex)).toBe(true);
    expect(UNIT_TEMPLATES.map((t) => t.kind)).toEqual([
      'bambu_ams', 'bambu_ams_lite', 'bambu_ams_ht', 'creality_cfs',
      'prusa_mmu3', 'anycubic_ace', 'external', 'custom',
    ]);
  });
});
