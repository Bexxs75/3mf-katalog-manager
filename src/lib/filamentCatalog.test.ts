import { describe, expect, it } from 'vitest';
import { manufacturerSuggestions, FILAMENT_MANUFACTURERS, RESIN_MANUFACTURERS } from './filamentCatalog';

describe('manufacturerSuggestions', () => {
  it('keeps curated spelling and appends sorted, trimmed custom names without duplicates', () => {
    expect(manufacturerSuggestions('filament', [' zeta ', 'esun', 'Alpha', 'ALPHA', '', '   '])).toEqual([
      ...FILAMENT_MANUFACTURERS, 'Alpha', 'zeta',
    ]);
  });
  it('uses the resin catalog for bottles', () => {
    expect(manufacturerSuggestions('resin', ['elegoo', 'Custom Resin'])).toEqual([
      ...RESIN_MANUFACTURERS, 'Custom Resin',
    ]);
  });
});
