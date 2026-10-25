import { describe, expect, it } from 'vitest';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';
import type { Translations } from '../i18n/types';
import errorCodes from './errorCodes.json';
import { formatAppError, legacyCodeKey, registeredCodeKey } from './errorMessage';

const DICTIONARIES: Record<string, Translations> = { de, en, es, fr };

const placeholders = (text: string) => [...new Set([...text.matchAll(/\{(\d+)\}/g)].map((m) => m[1]))].sort();

/**
 * Everything wrong with `codes`: a missing or empty entry in any language, or
 * placeholders that differ between the languages. The backend list
 * (`errorCodes.json`, checked against the Rust registry by a Rust test) goes in here.
 */
function problemsWith(codes: readonly string[], keyOf: (code: string) => keyof Translations | null): string[] {
  const problems: string[] = [];
  for (const code of codes) {
    const key = keyOf(code);
    const texts = Object.entries(DICTIONARIES).map(([language, dictionary]) => [language, key ? (dictionary as unknown as Record<string, unknown>)[key] : undefined] as const);
    for (const [language, text] of texts) {
      if (typeof text !== 'string' || text.trim() === '') problems.push(`${code}: no text in ${language}`);
    }
    const forms = new Set(texts.map(([, text]) => (typeof text === 'string' ? placeholders(text).join(',') : '')));
    if (forms.size > 1) problems.push(`${code}: placeholders differ between languages`);
  }
  return problems;
}

describe('backend error codes', () => {
  it('registers at least one code and no duplicates', () => {
    expect(errorCodes.registered.length).toBeGreaterThan(0);
    expect(new Set(errorCodes.registered).size).toBe(errorCodes.registered.length);
  });

  it('has a text in German, English, Spanish and French for every registered code', () => {
    expect(problemsWith(errorCodes.registered, registeredCodeKey)).toEqual([]);
  });

  it('has a text in all four languages for every legacy code', () => {
    expect(problemsWith(errorCodes.legacy.filter((code) => code !== 'cancelled'), legacyCodeKey)).toEqual([]);
  });

  it('keeps wording out of the parameters: texts only use numbered placeholders', () => {
    for (const code of errorCodes.registered) {
      const text = String(de[registeredCodeKey(code) as keyof Translations]);
      expect(text.replace(/\{\d+\}/g, '')).not.toMatch(/[{}]/);
    }
  });

  // The negative case: a code the backend can emit but nobody translated must fail the check.
  it('fails for a code that has no dictionary entry', () => {
    const problems = problemsWith(['codeWithoutTranslation'], registeredCodeKey);
    expect(problems).toHaveLength(Object.keys(DICTIONARIES).length);
    expect(problems[0]).toContain('codeWithoutTranslation');
  });

  it('fails when a language lacks the text or changes the placeholders', () => {
    const key = registeredCodeKey(errorCodes.registered.find((code) => placeholders(String(de[registeredCodeKey(code)])).length > 0) ?? '');
    const original = (fr as unknown as Record<string, string>)[key];
    try {
      (fr as unknown as Record<string, string>)[key] = original.replace(/\{0\}/g, '');
      expect(problemsWith(errorCodes.registered, registeredCodeKey).join()).toContain('placeholders differ');
      (fr as unknown as Record<string, string>)[key] = '';
      expect(problemsWith(errorCodes.registered, registeredCodeKey).join()).toContain('no text in fr');
    } finally {
      (fr as unknown as Record<string, string>)[key] = original;
    }
  });
});

describe('formatAppError', () => {
  const tIn = (dictionary: Translations) => (<K extends keyof Translations>(key: K) => dictionary[key]);

  it('translates a registered code in the chosen language', () => {
    expect(formatAppError(tIn(en), { message: 'folderNameEmpty' })).toBe(en.errFolderNameEmpty);
    expect(formatAppError(tIn(de), { message: 'folderNameEmpty' })).toBe(de.errFolderNameEmpty);
  });

  it('fills the placeholders with the params', () => {
    expect(formatAppError(tIn(en), { message: 'folderNameForbiddenChar', params: [':'] })).toBe('This character is not allowed in folder names: :');
    expect(formatAppError(tIn(fr), { message: 'fileNameForbiddenChar', params: ['?'] })).toContain('?');
  });

  it('keeps a placeholder visible when its param is missing', () => {
    expect(formatAppError(tIn(en), { message: 'folderNameForbiddenChar' })).toContain('{0}');
  });

  it('neutralizes direction controls in params', () => {
    expect(formatAppError(tIn(en), { message: 'folderNameForbiddenChar', params: ['‮'] })).toContain('[U+202E]');
  });

  it('still translates the legacy codes', () => {
    expect(formatAppError(tIn(en), { message: 'importActive' })).toBe(en.importActiveError);
    expect(formatAppError(tIn(en), { message: 'container' })).toBe(en.containerUpdates);
  });

  it('shows anything else, such as technical text of an unexpected error, as it is', () => {
    expect(formatAppError(tIn(en), { message: 'database is locked' })).toBe('database is locked');
  });
});
