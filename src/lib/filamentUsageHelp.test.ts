import { describe, expect, it } from 'vitest';
import { canHaveFilamentUsage, filamentUsageHelpUrl, isFilamentUsageMissing } from './filamentUsageHelp';

const sliceInfo = { totalWeightG: 12, plates: [] };

describe('isFilamentUsageMissing', () => {
  it('is true for a loaded 3MF without slicer data', () => {
    expect(isFilamentUsageMissing({ path: 'a/Benchy.3mf', sliceInfo: null })).toBe(true);
    expect(isFilamentUsageMissing({ path: 'a/BENCHY.3MF', sliceInfo: null, summaryOnly: false })).toBe(true);
  });

  it('is false when the 3MF has slicer data', () => {
    expect(isFilamentUsageMissing({ path: 'a.3mf', sliceInfo })).toBe(false);
  });

  it.each(['a.stl', 'a.obj', 'a.step', 'a.stp', 'a'])('is false for %s, which cannot have usage data', path => {
    expect(isFilamentUsageMissing({ path, sliceInfo: null })).toBe(false);
    expect(canHaveFilamentUsage(path)).toBe(false);
  });

  it('is false while only the list summary is loaded', () => {
    expect(isFilamentUsageMissing({ path: 'a.3mf', sliceInfo: null, summaryOnly: true })).toBe(false);
  });
});

describe('filamentUsageHelpUrl', () => {
  it('points German to the German wiki page and every other language to the English one', () => {
    expect(filamentUsageHelpUrl('de')).toBe('https://wiki.3mfkatalog.de/help/filament-usage/');
    for (const language of ['en', 'es', 'fr']) {
      expect(filamentUsageHelpUrl(language)).toBe('https://wiki.3mfkatalog.de/en/help/filament-usage/');
    }
  });
});
