import { describe, expect, it } from 'vitest';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';
import type { Translations } from './types';

const keys = Object.keys(de).filter(key => key.startsWith('sr') && /^sr[A-Z]/.test(key) || key.startsWith('errSlicerReport'));
const dictionaries = { de, en, es, fr } as const;
const textOf = (value: Translations[keyof Translations]): string => typeof value === 'string' ? value : `${value.one}|${value.other}`;
const placeholdersOf = (value: Translations[keyof Translations]) => [...new Set(textOf(value).match(/\{\w+\}/g) ?? [])].sort();

describe('slicer report texts', () => {
  it('finds the keys of the feature', () => {
    expect(keys.length).toBeGreaterThan(40);
    expect(keys).toContain('srTitle');
    expect(keys).toContain('errSlicerReportNotFound');
  });

  it.each(keys)('%s is filled in all four languages with the same placeholders', key => {
    const reference = placeholdersOf(de[key as keyof Translations]);
    for (const [language, dict] of Object.entries(dictionaries)) {
      const value = dict[key as keyof Translations];
      expect(value, `${language}.${key}`).toBeDefined();
      expect(textOf(value).trim(), `${language}.${key}`).not.toBe('');
      expect(placeholdersOf(value), `${language}.${key}`).toEqual(reference);
    }
  });

  it('says in every language that the tick moves the file', () => {
    expect(de.srIncludeMoves).toContain('verschiebt');
    expect(en.srIncludeMoves).toContain('moves');
    expect(es.srIncludeMoves).toContain('mueve');
    expect(fr.srIncludeMoves).toContain('déplace');
  });
});
