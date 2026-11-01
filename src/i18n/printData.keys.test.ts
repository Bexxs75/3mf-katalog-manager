import { describe, expect, it } from 'vitest';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';
import type { Translations } from './types';

// Every key of the feature, found by its prefix so a new one is checked without touching this list.
const keys = Object.keys(de).filter(key => key.startsWith('printData') || key.startsWith('printDuration'));
const dictionaries = { de, en, es, fr } as const;
const PLACEHOLDERS = ['{n}', '{total}', '{k}', '{p}', '{f}', '{h}', '{min}', '{count}'];

const textOf = (value: Translations[keyof Translations]): string => typeof value === 'string' ? value : `${value.one}|${value.other}`;
const placeholdersOf = (value: Translations[keyof Translations]) => (textOf(value).match(/\{\w+\}/g) ?? []).sort();

describe('print data texts', () => {
  it('finds the keys of the feature', () => {
    expect(keys.length).toBeGreaterThan(20);
    expect(keys).toContain('printDataNotFound');
  });

  it.each(keys)('%s is filled in all four languages', key => {
    for (const [language, dict] of Object.entries(dictionaries)) {
      const value = dict[key as keyof Translations];
      expect(value, `${language}.${key}`).toBeDefined();
      expect(textOf(value).trim(), `${language}.${key}`).not.toBe('');
    }
  });

  it.each(keys)('%s uses the same placeholders in all four languages', key => {
    const reference = placeholdersOf(de[key as keyof Translations]);
    for (const [language, dict] of Object.entries(dictionaries)) {
      expect(placeholdersOf(dict[key as keyof Translations]), `${language}.${key}`).toEqual(reference);
    }
    for (const placeholder of reference) expect(PLACEHOLDERS, `${key} ${placeholder}`).toContain(placeholder);
  });

  it('keeps the neutral wording: no "not sliced" in the chip, tooltip or hints', () => {
    const forbidden = /nicht geslict|not sliced|no laminad|non découpé|Quellmodell|source model/i;
    for (const [language, dict] of Object.entries(dictionaries)) {
      for (const key of ['printDataNoDataChip', 'printDataNotFound', 'printDataNotRecognizedHint', 'printDataUnreadableHint'] as const) {
        expect(dict[key], `${language}.${key}`).not.toMatch(forbidden);
      }
    }
    expect(de.printDataNoDataChip).toBe('Keine Druckdaten');
    expect(de.printDataNotFound).toBe('In der Datei nicht gefunden');
  });
});
