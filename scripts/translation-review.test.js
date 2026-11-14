import { describe, it, expect } from 'vitest';
import { readDictionary, reviewCsv } from './translation-review.mjs';

describe('translation review sheet', () => {
  it('reads escaped literals and plural forms without executing source', () => {
    expect(readDictionary(`export const en = { a: "A\\nB", p: {one: 'one', other: 'many'} };`)).toEqual({a:'A\nB', 'p.one':'one', 'p.other':'many'});
    expect(() => readDictionary('export const en = {a: dangerous()};')).toThrow();
  });
  it('quotes CSV fields and includes tags and all four languages', () => {
    const csv = reviewCsv({de:{a:'a,"b'},en:{a:'English'},es:{a:'Español'},fr:{a:'Français'}}, {deckel:{de:'deckel',en:'lid',es:'tapa',fr:'couvercle'}});
    expect(csv).toContain('"a,""b"');
    expect(csv).toContain('"autoTags.deckel","deckel","lid","tapa","couvercle"');
    expect(csv).toContain('"English","Español","Français"');
  });
  it('refuses missing translations instead of hiding them', () => {
    expect(() => reviewCsv({de:{a:'a'},en:{},es:{a:'a'},fr:{a:'a'}}, {})).toThrow();
  });
});
