import { describe, expect, it } from 'vitest';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';

const dictionaries = { de, en, es, fr } as unknown as Record<string, Record<string, unknown>>;
const keys = Object.keys(de).filter(key => key.startsWith('tv') || key.startsWith('errThingiverse'));
const placeholders = (text: string) => [...new Set([...text.matchAll(/\{(\d+)\}/g)].map(m => m[1]))].sort().join(',');

describe('Thingiverse texts', () => {
  it('has a good number of tv keys', () => {
    expect(keys.length).toBeGreaterThan(100);
  });

  it('has every key in German, English, Spanish and French with the same placeholders', () => {
    const problems: string[] = [];
    for (const key of keys) {
      for (const [language, dictionary] of Object.entries(dictionaries)) {
        const text = dictionary[key];
        if (typeof text !== 'string' || text.trim() === '') problems.push(`${key}: no text in ${language}`);
        else if (placeholders(text) !== placeholders(String(de[key as keyof typeof de]))) problems.push(`${key}: placeholders differ in ${language}`);
      }
    }
    expect(problems).toEqual([]);
  });

  it('keeps the "tv" prefix clear of other prefixes of the app', () => {
    const others = Object.keys(de).filter(key => /^tv[A-Z]/.test(key) === false && key.startsWith('tv'));
    expect(others).toEqual([]);
  });
});
