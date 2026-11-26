import { describe, expect, it } from 'vitest';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';
import type { Translations } from './types';

const keys = Object.keys(de).filter(key => /^(diskApply|diskSkip_|diskUnclear|diskFolder|webSync|cleanupFolder|cleanupTruncated|cleanupWebSettingsHint)/.test(key));
const dictionaries = { en, es, fr } as const;
const textOf = (value: Translations[keyof Translations]) => typeof value === 'string' ? value : `${value.one}|${value.other}`;
const placeholders = (value: Translations[keyof Translations]) => [...new Set(textOf(value).match(/\{\w+\}/g) ?? [])].sort();

describe('web disk sync texts', () => {
  it('finds the keys of the feature', () => {
    expect(keys.length).toBeGreaterThan(70);
    expect(keys).toContain('diskUnclearSummary');
    expect(keys).toContain('diskSkip_exists_again');
  });
  it.each(Object.entries(dictionaries))('%s has every key, keeps the placeholders and the plural shape of the German text', (_, dictionary) => {
    for (const key of keys) {
      const value = (dictionary as unknown as Record<string, Translations[keyof Translations]>)[key];
      const reference = (de as unknown as Record<string, Translations[keyof Translations]>)[key];
      expect(value, key).toBeDefined();
      expect(typeof value, key).toBe(typeof reference);
      expect(textOf(value).trim(), key).not.toBe('');
      expect(placeholders(value), key).toEqual(placeholders(reference));
    }
  });
  it('has the same key set in all four dictionaries', () => {
    for (const dictionary of Object.values(dictionaries)) expect(Object.keys(dictionary).sort()).toEqual(Object.keys(de).sort());
  });
  it('states the trash decision: removing goes to the trash, never an endless delete', () => {
    expect(de.diskApplyMissingAction).toContain('Papierkorb');
    expect(en.diskApplyMissingAction).toContain('trash');
    expect(de.diskApplyCancelHint).toContain('laufenden Block');
  });
});
