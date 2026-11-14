import { describe, expect, it } from 'vitest';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';
import { formatCount } from './types';
import { COLOR_IDS, FILTER_KEYS, TIME_BUCKETS } from '../lib/printDataFilter';
import { COLOR_LABEL_KEY, FILTER_HINT_KEY, FILTER_LABEL_KEY, TIME_LABEL_KEY } from '../lib/printFilterLabels';

const dictionaries = { de, en, es, fr } as const;
const keys = Object.keys(de).filter(key => key.startsWith('pf'));
const textsOf = (value: unknown) => typeof value === 'string' ? [value] : Object.values(value as Record<string, string>);
const placeholders = (value: unknown) => textsOf(value).flatMap(text => text.match(/\{\w+\}/g) ?? []).filter((p, i, all) => all.indexOf(p) === i).sort();

describe('print data filter texts', () => {
  it('finds the keys of the feature', () => {
    expect(keys.length).toBeGreaterThan(40);
  });

  it('exist in all four languages and are not empty', () => {
    for (const [name, dict] of Object.entries(dictionaries)) {
      for (const key of keys) {
        const value = (dict as unknown as Record<string, unknown>)[key];
        expect(value, `${name}.${key}`).toBeDefined();
        for (const text of textsOf(value)) expect(text.trim().length, `${name}.${key}`).toBeGreaterThan(0);
      }
    }
  });

  it('keep the same placeholders in every language', () => {
    for (const key of keys) {
      const expected = placeholders((de as unknown as Record<string, unknown>)[key]);
      for (const [name, dict] of Object.entries(dictionaries)) expect(placeholders((dict as unknown as Record<string, unknown>)[key]), `${name}.${key}`).toEqual(expected);
    }
  });

  it('name every filter, time step and color group the code can produce', () => {
    const used = [
      ...FILTER_KEYS.flatMap(key => [FILTER_LABEL_KEY[key], FILTER_HINT_KEY[key]]),
      ...TIME_BUCKETS.map(key => TIME_LABEL_KEY[key]),
      ...COLOR_IDS.map(key => COLOR_LABEL_KEY[key]),
    ];
    for (const key of used) expect(keys, key).toContain(key);
    expect(new Set(COLOR_IDS.map(key => de[COLOR_LABEL_KEY[key] as 'pfColorRed'])).size).toBe(COLOR_IDS.length);
  });

  it('pick singular and plural of the file count', () => {
    expect(formatCount(en.pfHidden, 1, 'en')).toBe('1 file without print data hidden');
    expect(formatCount(de.pfHidden, 5, 'de')).toBe('5 Dateien ohne Druckdaten ausgeblendet');
    expect(formatCount(fr.pfNoData, 2, 'fr')).toBe('2 fichiers sans cette information');
    expect(formatCount(es.pfShown, 1, 'es')).toContain('1 archivo');
  });
});
