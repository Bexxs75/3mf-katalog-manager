import { describe, expect, it } from 'vitest';
import { canonicalTag, shortTagLabel, sortTagsForDisplay, tagLabel, tagMatches } from './autoTags';

describe('tagLabel', () => {
  it('translates every auto tag into every language', () => {
    expect(tagLabel('mehrteilig', 'en')).toBe('multipart');
    expect(tagLabel('miniatur', 'es')).toBe('miniatura');
    expect(tagLabel('grossformat', 'fr')).toBe('grand format');
    expect(tagLabel('mehrfarbig', 'de')).toBe('mehrfarbig');
    expect(tagLabel('mehrfarbig', 'fr')).toBe('multicolore');
  });

  it('leaves other tags untouched', () => {
    expect(tagLabel('Vasenhalter', 'en')).toBe('Vasenhalter');
    expect(tagLabel('mini', 'en')).toBe('mini');
  });

  it('does not resolve inherited Object.prototype properties as tags', () => {
    expect(tagLabel('constructor', 'en')).toBe('constructor');
    expect(tagLabel('toString', 'en')).toBe('toString');
  });
});

describe('dictionary entries for everyday terms', () => {
  it.each([
    ['deckel', 'lid', 'tapa', 'couvercle'],
    ['schublade', 'drawer', 'cajón', 'tiroir'],
    ['haken', 'hook', 'gancho', 'crochet'],
    ['vase', 'vase', 'jarrón', 'vase'],
    ['box', 'box', 'caja', 'boîte'],
    ['organizer', 'organizer', 'organizador', 'organisateur'],
    ['wandhalterung', 'wall mount', 'soporte de pared', 'support mural'],
    ['schlüsselanhänger', 'keychain', 'llavero', 'porte-clés'],
    ['spielzeug', 'toy', 'juguete', 'jouet'],
    ['zahnrad', 'gear', 'engranaje', 'engrenage'],
    ['werkzeug', 'tool', 'herramienta', 'outil'],
    ['adapter', 'adapter', 'adaptador', 'adaptateur'],
    ['ersatzteil', 'spare part', 'repuesto', 'pièce détachée'],
    ['lampe', 'lamp', 'lámpara', 'lampe'],
    ['klein', 'small', 'pequeño', 'petit'],
  ])('shows %s in every language and maps each name back to it', (key, en, es, fr) => {
    expect(tagLabel(key, 'de')).toBe(key);
    expect(tagLabel(key, 'en')).toBe(en);
    expect(tagLabel(key, 'es')).toBe(es);
    expect(tagLabel(key, 'fr')).toBe(fr);
    for (const name of [key, en, es, fr]) expect(canonicalTag(name)).toBe(key);
  });

  it('merges hand-typed names into the German key, ignoring case', () => {
    expect(canonicalTag('Lid')).toBe('deckel');
    expect(canonicalTag('  Couvercle')).toBe('deckel');
    expect(canonicalTag('Wall Mount')).toBe('wandhalterung');
    expect(canonicalTag('Vase')).toBe('vase');
  });

  it('does not touch holder/support/stand words that are deliberately not in the table', () => {
    for (const word of ['holder', 'support', 'soporte', 'stand', 'halter', 'ständer']) {
      expect(canonicalTag(word)).toBe(word);
    }
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
    expect(canonicalTag('Vasenhalter')).toBe('Vasenhalter');
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


describe('shortTagLabel', () => {
  it.each([
    ['', ''],
    ['#' + 'a'.repeat(28), '#' + 'a'.repeat(28)],
    ['#' + 'a'.repeat(29), '#' + 'a'.repeat(28) + '…'],
    ['#' + 'ä'.repeat(29), '#' + 'ä'.repeat(28) + '…'],
    ['#' + '😀'.repeat(28), '#' + '😀'.repeat(28)],
    ['#' + 'a'.repeat(27) + '😀z', '#' + 'a'.repeat(27) + '😀…'],
  ])('shortens the displayed label by codepoints: %s', (label, expected) => {
    expect(shortTagLabel(label)).toBe(expected);
  });
});
