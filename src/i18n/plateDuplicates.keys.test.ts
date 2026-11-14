import { describe, expect, it } from 'vitest';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';
import { formatCount } from './types';

const dictionaries = { de, en, es, fr } as const;
const prefixes = { po: /^po[A-Z]/, dv: /^dv[A-Z]/ };
const textsOf = (value: unknown) => typeof value === 'string' ? [value] : Object.values(value as Record<string, string>);
const placeholders = (value: unknown) => textsOf(value).flatMap(text => text.match(/\{\w+\}/g) ?? []).filter((p, i, all) => all.indexOf(p) === i).sort();
const valueOf = (dict: object, key: string) => (dict as Record<string, unknown>)[key];

describe.each(Object.entries(prefixes))('texts with the prefix %s', (_name, pattern) => {
  const keys = Object.keys(de).filter(key => pattern.test(key));

  it('exist, so the feature is not silently untranslated', () => {
    expect(keys.length).toBeGreaterThan(5);
  });

  it('exist in all four languages and are not empty', () => {
    for (const [name, dict] of Object.entries(dictionaries)) {
      for (const key of keys) {
        const value = valueOf(dict, key);
        expect(value, `${name}.${key}`).toBeDefined();
        for (const text of textsOf(value)) expect(text.trim().length, `${name}.${key}`).toBeGreaterThan(0);
      }
    }
  });

  it('keep the same placeholders and the same plural shape in every language', () => {
    for (const key of keys) {
      const expected = placeholders(valueOf(de, key));
      for (const [name, dict] of Object.entries(dictionaries)) {
        expect(placeholders(valueOf(dict, key)), `${name}.${key}`).toEqual(expected);
        expect(typeof valueOf(dict, key), `${name}.${key}`).toBe(typeof valueOf(de, key));
      }
    }
  });
});

describe('plate overview texts', () => {
  it('pick singular and plural of the plate count', () => {
    expect(formatCount(de.poOpenCount, 1, 'de')).toBe('Plattenübersicht (1 Platte)');
    expect(formatCount(de.poOpenCount, 4, 'de')).toBe('Plattenübersicht (4 Platten)');
    expect(formatCount(en.poOpenCount, 2, 'en')).toBe('Plate overview (2 plates)');
    expect(formatCount(fr.poOpenCount, 1, 'fr')).toBe('Aperçu des plateaux (1 plateau)');
    expect(formatCount(es.poOpenCount, 3, 'es')).toBe('Resumen de placas (3 placas)');
  });
});

describe('duplicates view texts', () => {
  it('count entries, groups and deselected entries in the right number form', () => {
    expect(formatCount(de.dvCount, 1, 'de')).toBe('1 Eintrag');
    expect(formatCount(de.dvSummaryDropped, 3, 'de')).toBe('3 Einträge abgewählt');
    expect(formatCount(en.dvSummaryGroups, 1, 'en')).toBe('1 group');
    expect(formatCount(fr.dvPartial, 2, 'fr')).toContain('2 modèles');
  });
});
