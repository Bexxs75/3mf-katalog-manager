import { describe, expect, it } from 'vitest';
import { de } from '../i18n/de';
import { formatPrintDuration } from '../i18n/format';
import { makePrintInfo } from '../test/factories';
import type { SliceInfo } from '../types';
import { mergeFilaments, showsPrintData, summarizePrintData, sumPrintTime, sumWeightG } from './printData';

const filament = (type: string, usedG: number) => ({ type, color: '#2b2b2b', usedG, usedM: usedG / 3 });
const sliceInfo = (count: number): SliceInfo => ({
  totalWeightG: count * 10,
  plates: Array.from({ length: count }, (_, i) => ({ plateIndex: i + 1, weightG: 10, filaments: [filament('PLA', 10)] })),
});
const plate = (plateIndex: number, printSeconds: number | null) => ({
  plateIndex, name: null, printSeconds, firstLayerSeconds: null, plateBedType: null,
});
const complete = {
  printerModel: 'Bambu Lab X1 Carbon', bedType: 'Textured PEI Plate', layerHeightMm: 0.2,
  nozzleMm: { value: 0.4, mixed: false }, bedTempC: { value: 60, mixed: false }, nozzleTempC: { value: 220, mixed: false },
};

describe('summarizePrintData', () => {
  it('sums the seconds of the known plates, not the rounded minutes', () => {
    const summary = summarizePrintData({
      sliceInfo: sliceInfo(3), printInfo: makePrintInfo({ ...complete, plates: [plate(1, 8040), plate(2, 3120), plate(3, 13260)] }),
    });
    expect(summary.sum).toEqual({ seconds: 24420, known: 3, total: 3 });
    expect(formatPrintDuration(summary.sum.seconds ?? 0, 'de', key => de[key])).toBe('6 h 47 min');
    expect(summary.state).toBe('sliced');
  });

  it('rounds once at the end: two 30 s plates make 1 min, not 2', () => {
    const rows = summarizePrintData({
      sliceInfo: sliceInfo(2), printInfo: makePrintInfo({ ...complete, plates: [plate(1, 30), plate(2, 30)] }),
    }).plates;
    const sum = sumPrintTime(rows);
    expect(sum.seconds).toBe(60);
    expect(formatPrintDuration(sum.seconds ?? 0, 'de', key => de[key])).toBe('1 min');
    const roundedSeparately = rows.reduce((total, row) => total + Math.round((row.printSeconds ?? 0) / 60), 0);
    expect(roundedSeparately).toBe(2);
  });

  it('counts only the plates with a time as known and turns partial', () => {
    const summary = summarizePrintData({
      sliceInfo: sliceInfo(3), printInfo: makePrintInfo({ ...complete, plates: [plate(1, 8040), plate(2, null), plate(3, null)] }),
    });
    expect(summary.sum).toEqual({ seconds: 8040, known: 1, total: 3 });
    expect(summary.state).toBe('partial');
  });

  it('has no sum (null, never 0) when no plate has a time', () => {
    const summary = summarizePrintData({ sliceInfo: sliceInfo(2), printInfo: makePrintInfo({ ...complete, plates: [plate(1, null), plate(2, null)] }) });
    expect(summary.sum).toEqual({ seconds: null, known: 0, total: 2 });
  });

  it('never uses the first-layer time as a print time', () => {
    const summary = summarizePrintData({
      sliceInfo: sliceInfo(1),
      printInfo: makePrintInfo({ ...complete, plates: [{ ...plate(1, null), firstLayerSeconds: 194.1 }] }),
    });
    expect(summary.plates[0].printSeconds).toBeNull();
    expect(summary.sum.seconds).toBeNull();
  });

  it('is partial when a shared value is missing and sliced when all six are known', () => {
    const plates = [plate(1, 600)];
    expect(summarizePrintData({ sliceInfo: sliceInfo(1), printInfo: makePrintInfo({ ...complete, plates }) }).state).toBe('sliced');
    const partial = summarizePrintData({ sliceInfo: sliceInfo(1), printInfo: makePrintInfo({ ...complete, bedType: null, plates }) });
    expect(partial.state).toBe('partial');
    expect(partial.shared.bedType).toEqual({ kind: 'missing' });
    expect(partial.shared.layerHeightMm).toEqual({ kind: 'value', value: 0.2 });
  });

  it('shows a mixed value as mixed and does not call it missing', () => {
    const summary = summarizePrintData({
      sliceInfo: sliceInfo(1),
      printInfo: makePrintInfo({ ...complete, nozzleTempC: { value: null, mixed: true }, plates: [plate(1, 600)] }),
    });
    expect(summary.shared.nozzleTempC).toEqual({ kind: 'mixed' });
    expect(summary.state).toBe('sliced');
  });

  it('never turns a missing value into a 0', () => {
    const summary = summarizePrintData({
      sliceInfo: sliceInfo(1), printInfo: makePrintInfo({ ...complete, bedTempC: { value: null, mixed: false }, plates: [plate(1, 600)] }),
    });
    expect(summary.shared.bedTempC).toEqual({ kind: 'missing' });
  });

  it.each(['no_slice_data', 'not_recognized', 'unreadable'] as const)('derives %s from the stored state without shared values', state => {
    const summary = summarizePrintData({ sliceInfo: null, printInfo: makePrintInfo({ sliceState: state, ...complete, plates: [plate(1, 600)] }) });
    expect(summary.state).toBe(state);
    expect(Object.values(summary.shared).every(value => value.kind === 'missing')).toBe(true);
    expect(summary.sum.seconds).toBeNull();
  });

  it('treats no row as unread', () => {
    expect(summarizePrintData({ sliceInfo: sliceInfo(2), printInfo: null }).state).toBe('unread');
  });

  it('treats a row of another reader version as unread and shows none of its values', () => {
    const summary = summarizePrintData({
      sliceInfo: sliceInfo(1), printInfo: makePrintInfo({ ...complete, readerCurrent: false, plates: [plate(1, 600)] }),
    });
    expect(summary.state).toBe('unread');
    expect(summary.sum.seconds).toBeNull();
    expect(summary.plates[0].printSeconds).toBeNull();
    expect(Object.values(summary.shared).every(value => value.kind === 'missing')).toBe(true);
  });

  it('remembers unreadable project settings', () => {
    const summary = summarizePrintData({
      sliceInfo: sliceInfo(1), printInfo: makePrintInfo({ ...complete, settingsState: 'unreadable', bedType: null, plates: [plate(1, 600)] }),
    });
    expect(summary.settingsUnreadable).toBe(true);
  });

  it('builds the rows from the print data when the slicer usage is absent', () => {
    const summary = summarizePrintData({ sliceInfo: null, printInfo: makePrintInfo({ ...complete, plates: [plate(1, 600), plate(2, null)] }) });
    expect(summary.plates.map(row => row.plateIndex)).toEqual([1, 2]);
    expect(summary.sum).toEqual({ seconds: 600, known: 1, total: 2 });
  });
});

describe('plate helpers', () => {
  it('merges filaments of the same type and color and sums the weight', () => {
    const rows = summarizePrintData({ sliceInfo: sliceInfo(2), printInfo: null }).plates;
    expect(mergeFilaments(rows)).toEqual([{ type: 'PLA', color: '#2b2b2b', usedG: 20, usedM: 20 / 3 }]);
    expect(sumWeightG(rows)).toBe(20);
    expect(sumWeightG([])).toBeNull();
  });

  it('shows print data only for a fully loaded 3MF that is not in the trash', () => {
    const base = { path: '/c/a.3mf', summaryOnly: undefined, deletedAt: null };
    expect(showsPrintData(base)).toBe(true);
    expect(showsPrintData({ ...base, path: '/c/a.STL' })).toBe(false);
    expect(showsPrintData({ ...base, summaryOnly: true })).toBe(false);
    expect(showsPrintData({ ...base, deletedAt: '2026-01-01T00:00:00Z' })).toBe(false);
  });
});
