import { describe, expect, it } from 'vitest';
import { canonicalTag, sortTagsForDisplay, tagLabel, tagMatches } from './autoTags';

describe('tagLabel', () => {
  it('translates every auto tag into every language', () => {
    expect(tagLabel('mehrteilig', 'en')).toBe('multipart');
    expect(tagLabel('miniatur', 'es')).toBe('miniatura');
    expect(tagLabel('grossformat', 'fr')).toBe('grand format');
    expect(tagLabel('mehrfarbig', 'de')).toBe('mehrfarbig');
    expect(tagLabel('mehrfarbig', 'fr')).toBe('multicolore');
  });

  it('leaves other tags untouched', () => {
    expect(tagLabel('Vase', 'en')).toBe('Vase');
    expect(tagLabel('mini', 'en')).toBe('mini');
  });
});

describe('canonicalTag', () => {
  it('maps aliases of all languages to the canonical tag, ignoring case and whitespace', () => {
    expect(canonicalTag('  Multipart ')).toBe('mehrteilig');
    expect(canonicalTag('MULTIPIÈCE')).toBe('mehrteilig');
    expect(canonicalTag('mini')).toBe('miniatur');
    expect(canonicalTag('Grand Format')).toBe('grossformat');
    expect(canonicalTag('multicolor')).toBe('mehrfarbig');
    expect(canonicalTag('mehrteilig')).toBe('mehrteilig');
  });

  it('returns other input unchanged', () => {
    expect(canonicalTag('Vase')).toBe('Vase');
    expect(canonicalTag('minis')).toBe('minis');
  });
});

describe('tagMatches', () => {
  it('matches the translated label', () => {
    expect(tagMatches('mehrteilig', 'multi', 'en')).toBe(true);
    expect(tagMatches('mehrfarbig', 'MULTI', 'en')).toBe(true);
  });

  it('still matches the canonical tag', () => {
    expect(tagMatches('mehrteilig', 'mehrt', 'en')).toBe(true);
  });

  it('does not match unrelated queries', () => {
    expect(tagMatches('mehrteilig', 'vase', 'en')).toBe(false);
    expect(tagMatches('Vase', 'multi', 'en')).toBe(false);
  });
});

describe('sortTagsForDisplay', () => {
  it('sorts by the displayed label in the current language', () => {
    const tags = [
      { label: 'mehrteilig', count: 2, colorHue: 1 },
      { label: 'deko', count: 2, colorHue: 2 },
      { label: 'grossformat', count: 2, colorHue: 3 },
    ];
    expect(sortTagsForDisplay(tags, 'en').map((t) => t.label)).toEqual(['deko', 'grossformat', 'mehrteilig']);
    // en: deko, large, multipart
    expect(sortTagsForDisplay(tags, 'fr').map((t) => t.label)).toEqual(['deko', 'grossformat', 'mehrteilig']);
    // fr: deko, grand format, multipièce
    const withMini = [...tags, { label: 'miniatur', count: 2, colorHue: 4 }];
    expect(sortTagsForDisplay(withMini, 'en').map((t) => t.label)).toEqual([
      'deko', 'grossformat', 'miniatur', 'mehrteilig',
    ]);
    // en: deko, large, mini, multipart
  });

  it('does not mutate its input', () => {
    const tags = [
      { label: 'b', count: 2, colorHue: 1 },
      { label: 'a', count: 2, colorHue: 2 },
    ];
    sortTagsForDisplay(tags, 'de');
    expect(tags.map((t) => t.label)).toEqual(['b', 'a']);
  });
});
