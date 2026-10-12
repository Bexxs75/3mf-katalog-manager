import { describe, expect, it } from 'vitest';
import { formatCount, type Language, type PluralForms } from './types';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';

const dictionaries = { de, en, es, fr };
describe('count texts', () => {
  it.each([
    ['de', '0 Modelle importiert', '1 Modell importiert', '2 Modelle importiert'],
    ['en', '0 models imported', '1 model imported', '2 models imported'],
    ['es', '0 modelos importados', '1 modelo importado', '2 modelos importados'],
    ['fr', '0 modèle importé', '1 modèle importé', '2 modèles importés'],
  ] as const)('formats model imports in %s', (language, ...expected) => {
    expected.forEach((text, count) => expect(formatCount(dictionaries[language].impModelsImported, count, language)).toBe(text));
  });
  it.each(Object.keys(dictionaries) as Language[])('uses language plural rules for every count text in %s', language => {
    for (const forms of Object.values(dictionaries[language])) {
      if (typeof forms === 'string') continue;
      for (const count of [0, 1, 2, 1000]) {
        const expected = (count === 1 || (language === 'fr' && count === 0) ? forms.one : forms.other)
          .replace('{count}', new Intl.NumberFormat(language).format(count));
        expect(formatCount(forms as PluralForms, count, language)).toBe(expected);
      }
    }
  });
});

describe('impOfWhichNotPlaced', () => {
  it.each(Object.keys(dictionaries) as Language[])('is defined in %s', language => {
    expect(dictionaries[language].impOfWhichNotPlaced.trim().length).toBeGreaterThan(0);
  });
});

it('keeps dictionary keys equal in all four languages, including print status', () => {
  const keys = Object.keys(de).sort();
  for (const dictionary of Object.values(dictionaries)) {
    expect(Object.keys(dictionary).sort()).toEqual(keys);
    for (const key of ['printStatusFilter', 'printStatusAll', 'printStatusPrinted', 'printStatusNotPrinted', 'sortPrintStatus', 'sortPrintStatusAsc', 'sortPrintStatusDesc'] as const) {
      expect(dictionary[key].trim().length).toBeGreaterThan(0);
    }
  }
});
