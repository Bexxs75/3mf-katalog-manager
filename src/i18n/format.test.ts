import { describe, expect, it } from 'vitest';
import { formatDecimalInput, formatLayerHeightMm, formatNozzleMm, formatPrintDuration, formatSpoolAmount, formatStockG, formatTemperatureC, formatUsageG, formatUsageM, formatVolumeMl } from './format';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';
import type { Language, Translations } from './types';

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

const dictionaries: Record<Language, Translations> = { de, en, es, fr };

describe('formatPrintDuration', () => {
  const show = (seconds: number, language: Language) => formatPrintDuration(seconds, language, key => dictionaries[language][key]);

  it.each([
    [0, '< 1 min'], [29, '< 1 min'], [30, '1 min'], [59, '1 min'], [60, '1 min'],
    [3599, '1 h'], [3600, '1 h'], [8040, '2 h 14 min'], [7200, '2 h'], [2820, '47 min'],
  ])('shows %i s as "%s" in every language', (seconds, expected) => {
    for (const language of ['de', 'en', 'es', 'fr'] as const) expect(show(seconds, language)).toBe(expected);
  });

  it('takes the units from the translation of each language', () => {
    const custom: Translations = { ...de, printDurationHoursMinutes: '{h}:{min} Std', printDurationMinutes: '{min} Min', printDurationHours: '{h} Std', printDurationUnderMinute: 'unter 1 Min' };
    const t = (key: keyof typeof custom) => custom[key] as string;
    expect(formatPrintDuration(8040, 'de', t)).toBe('2:14 Std');
    expect(formatPrintDuration(2820, 'de', t)).toBe('47 Min');
    expect(formatPrintDuration(10, 'de', t)).toBe('unter 1 Min');
  });

  it('groups large hour counts like other numbers of the language', () => {
    expect(show(1_000 * 3600, 'de')).toBe('1.000 h');
    expect(show(1_000 * 3600, 'en')).toBe('1,000 h');
  });
});

describe('print data number formats', () => {
  it('shows the layer height with two or three decimals', () => {
    expect(formatLayerHeightMm(0.2, 'de')).toBe('0,20 mm');
    expect(formatLayerHeightMm(0.2, 'en')).toBe('0.20 mm');
    expect(formatLayerHeightMm(0.125, 'de')).toBe('0,125 mm');
    expect(formatLayerHeightMm(0.125, 'fr')).toBe('0,125 mm');
    expect(formatLayerHeightMm(0.16, 'es')).toBe('0,16 mm');
  });

  it('shows the nozzle with one or two decimals (0.4 is "0,4 mm", never a float tail)', () => {
    expect(formatNozzleMm(0.4, 'de')).toBe('0,4 mm');
    expect(formatNozzleMm(0.4, 'en')).toBe('0.4 mm');
    expect(formatNozzleMm(0.25, 'de')).toBe('0,25 mm');
    expect(formatNozzleMm(0.25, 'es')).toBe('0,25 mm');
    expect(formatNozzleMm(0.4000000059604645, 'de')).toBe('0,4 mm');
  });

  it('shows temperatures as whole degrees', () => {
    expect(formatTemperatureC(220, 'de')).toBe('220 °C');
    expect(formatTemperatureC(60.4, 'en')).toBe('60 °C');
    expect(formatTemperatureC(1000, 'de')).toBe('1.000 °C');
  });
});

describe('formatUsageG / formatUsageM', () => {
  it('always shows one decimal', () => {
    expect(formatUsageG(61, 'de')).toBe('61,0 g');
    expect(formatUsageG(38.24, 'en')).toBe('38.2 g');
    expect(formatUsageM(7, 'de')).toBe('7,0 m');
    expect(formatUsageM(12.76, 'fr')).toBe('12,8 m');
  });
});
