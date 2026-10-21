import { describe, expect, it } from 'vitest';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';
import { formatCount } from './types';

const languages = { de, en, es, fr } as const;
const keys = Object.keys(de).filter(key => key.startsWith('sf'));
const textOf = (value: unknown) => typeof value === 'string' ? [value] : Object.values(value as Record<string, string>);

describe('source folder texts', () => {
  it('exist in all four languages and are not empty', () => {
    expect(keys.length).toBeGreaterThan(30);
    for (const [name, dict] of Object.entries(languages)) {
      for (const key of keys) {
        const value = (dict as unknown as Record<string, unknown>)[key];
        expect(value, `${name}.${key}`).toBeDefined();
        for (const text of textOf(value)) expect(text.trim().length, `${name}.${key}`).toBeGreaterThan(0);
      }
    }
  });

  it('keep the same placeholders in every language', () => {
    const placeholders = (value: unknown) => textOf(value).flatMap(text => text.match(/\{\w+\}/g) ?? []).filter((p, i, all) => all.indexOf(p) === i).sort();
    for (const key of keys) {
      const expected = placeholders((de as unknown as Record<string, unknown>)[key]);
      for (const [name, dict] of Object.entries(languages)) expect(placeholders((dict as unknown as Record<string, unknown>)[key]), `${name}.${key}`).toEqual(expected);
    }
  });

  it('pick the singular and plural form of the file count', () => {
    expect(formatCount(en.sfFoundOne, 1, 'en')).toContain('1 new file in');
    expect(formatCount(en.sfFoundOne, 3, 'en')).toContain('3 new files in');
    expect(formatCount(fr.sfRejectedDone, 2, 'fr')).toBe('2 fichiers refusés');
    expect(formatCount(es.sfRejectedDone, 1, 'es')).toBe('1 archivo rechazado');
  });
});
