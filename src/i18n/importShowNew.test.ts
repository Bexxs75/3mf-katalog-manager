import { expect, it } from 'vitest';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';

it('has a non-empty "view recently added" label in every language', () => {
  for (const dict of [de, en, es, fr]) expect(dict.importShowNew.trim().length).toBeGreaterThan(0);
  expect(new Set([de, en, es, fr].map((d) => d.importShowNew)).size).toBe(4);
});
