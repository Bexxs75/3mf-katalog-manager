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
      'bambu_ams', 'bambu_ams_lite', 'bambu_ams_2_pro', 'bambu_ams_ht', 'bambu_ams_3h', 'creality_cfs',
      'prusa_mmu3', 'anycubic_ace', 'external', 'custom',
    ]);
  });
});

it('offers AMS 3H after AMS HT with nine slots', () => {
  const index = UNIT_TEMPLATES.findIndex(t => t.kind === 'bambu_ams_ht');
  expect(UNIT_TEMPLATES[index + 1]).toEqual({ kind: 'bambu_ams_3h', slotCount: 9, defaultName: 'AMS 3H' });
});
