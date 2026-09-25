import { describe, expect, it } from 'vitest';
import { formatStockG } from './format';

describe('formatStockG', () => {
  it('shows one decimal in German', () => {
    expect(formatStockG(612.4, 'de')).toBe('612,4 g');
    expect(formatStockG(1000, 'de')).toBe('1.000,0 g');
  });
  it('shows one decimal in English', () => {
    expect(formatStockG(612.4, 'en')).toBe('612.4 g');
  });
});
