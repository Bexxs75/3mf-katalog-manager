import { describe, expect, it } from 'vitest';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';

const dictionaries = { de, en, es, fr };
const keys = [
  'fitPrinterFilter', 'fitPrinterAll', 'plateFitHeading', 'plateFitModelSize', 'plateFitFits', 'plateFitTooLarge',
  'plateFitUnknown', 'plateFitTooWide', 'plateFitTooDeep', 'plateFitTooHigh', 'plateFitHint',
  'filamentUsageMissingHeading', 'filamentUsageMissingTitle', 'filamentUsageMissingBody', 'filamentUsageMissingLink',
] as const;
const placeholders = { plateFitModelSize: '{size}', plateFitTooWide: '{mm}', plateFitTooDeep: '{mm}', plateFitTooHigh: '{mm}' } as const;

describe.each(Object.entries(dictionaries))('print bed and filament usage texts in %s', (_language, dictionary) => {
  it.each(keys)('has a text for %s', key => {
    expect(dictionary[key].trim().length).toBeGreaterThan(0);
  });

  it.each(Object.entries(placeholders))('keeps the placeholder in %s', (key, placeholder) => {
    expect(dictionary[key as keyof typeof placeholders]).toContain(placeholder);
  });
});
