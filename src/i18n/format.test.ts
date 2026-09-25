import { describe, expect, it } from 'vitest';
import { formatDecimalInput, formatSpoolAmount, formatStockG, formatVolumeMl } from './format';

describe('formatStockG', () => {
  it('shows one decimal in German', () => {
    expect(formatStockG(612.4, 'de')).toBe('612,4 g');
    expect(formatStockG(1000, 'de')).toBe('1.000,0 g');
  });
  it('shows one decimal in English', () => {
    expect(formatStockG(612.4, 'en')).toBe('612.4 g');
  });
});

describe('formatDecimalInput', () => {
  it('uses the decimal separator of the language without grouping', () => {
    expect(formatDecimalInput(29.95, 'de')).toBe('29,95');
    expect(formatDecimalInput(29.95, 'en')).toBe('29.95');
    expect(formatDecimalInput(1234.5, 'de')).toBe('1234,5');
    expect(formatDecimalInput(30, 'fr')).toBe('30');
  });
});

describe('formatVolumeMl / formatSpoolAmount', () => {
  it('shows ml with at most one decimal', () => {
    expect(formatVolumeMl(640, 'de')).toBe('640 ml');
    expect(formatVolumeMl(12.5, 'de')).toBe('12,5 ml');
    expect(formatVolumeMl(1000, 'en')).toBe('1,000 ml');
  });

  it('picks g or ml by kind', () => {
    expect(formatSpoolAmount(612.4, 'filament', 'de')).toBe('612,4 g');
    expect(formatSpoolAmount(640, 'resin', 'de')).toBe('640 ml');
  });
});
