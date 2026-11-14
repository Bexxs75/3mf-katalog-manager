import { describe, expect, it } from 'vitest';
import { de } from '../i18n/de';
import { formatPrintDuration } from '../i18n/format';
import { makePrintInfo } from '../test/factories';
import type { SliceInfo } from '../types';
import { makeModelFile } from '../test/factories';
import { isWeightUnknown, mergeFilaments, printDataStateOf, printFactsOf, rereadTargets, showsPrintData, summarizePrintData, sumPrintTime, sumWeightG } from './printData';

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

  it.each(['not_recognized', 'unreadable'] as const)('derives %s from the stored state without shared values', state => {
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

describe('project settings without print data', () => {
  const noData = (extra: Partial<Parameters<typeof makePrintInfo>[0]> = {}) =>
    summarizePrintData({ sliceInfo: null, printInfo: makePrintInfo({ sliceState: 'no_slice_data', ...complete, plates: [plate(1, 600)], ...extra }) });

  it('keeps the state and gives the four settings without temperatures or time', () => {
    const summary = noData();
    expect(summary.state).toBe('no_slice_data');
    expect(summary.sharedKind).toBe('project');
    expect(summary.shared.printerModel).toEqual({ kind: 'value', value: 'Bambu Lab X1 Carbon' });
    expect(summary.shared.nozzleMm).toEqual({ kind: 'value', value: 0.4 });
    expect(summary.shared.bedTempC).toEqual({ kind: 'missing' });
    expect(summary.shared.nozzleTempC).toEqual({ kind: 'missing' });
    expect(summary.sum.seconds).toBeNull();
  });

  it.each(['missing', 'unreadable', 'not_attempted'] as const)('gives nothing when the settings state is %s', settingsState => {
    const summary = noData({ settingsState });
    expect(summary.sharedKind).toBe('none');
    expect(Object.values(summary.shared).every(value => value.kind === 'missing')).toBe(true);
  });

  it('marks a sliced file as full', () => {
    expect(summarizePrintData({ sliceInfo: sliceInfo(1), printInfo: makePrintInfo({ ...complete, plates: [plate(1, 600)] }) }).sharedKind).toBe('full');
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

describe('G-code files', () => {
  const gcodeInfo = makePrintInfo({ sliceState: 'sliced', printerModel: 'Prusa MK4S', plates: [
    { plateIndex: 1, name: null, printSeconds: 4020, firstLayerSeconds: null, plateBedType: null }] });

  it('shows print data for a fully loaded G-code file, text or binary, but not for a model without slicer data', () => {
    const base = { summaryOnly: undefined, deletedAt: null };
    expect(showsPrintData({ ...base, path: '/c/a.gcode' })).toBe(true);
    expect(showsPrintData({ ...base, path: '/c/a.BGCODE' })).toBe(true);
    expect(showsPrintData({ ...base, path: '/c/a.obj' })).toBe(false);
    expect(showsPrintData({ ...base, path: '/c/a.gcode', summaryOnly: true })).toBe(false);
  });

  it('derives the print data state of a G-code file from its row like for a 3MF', () => {
    expect(printDataStateOf({ path: '/c/a.gcode', printDataState: undefined, printInfo: gcodeInfo })).toBe('sliced');
    expect(printDataStateOf({ path: '/c/a.gcode', printDataState: undefined, printInfo: null })).toBe('unread');
    expect(printDataStateOf({ path: '/c/a.gcode', printDataState: undefined, printInfo: { ...gcodeInfo, readerCurrent: false } })).toBe('unread');
    expect(printDataStateOf({ path: '/c/a.stl', printDataState: undefined, printInfo: null })).toBe('none');
  });

  it('turns the print time and weight of the single plate into one row and one total', () => {
    const summary = summarizePrintData({ sliceInfo: sliceInfo(1), printInfo: gcodeInfo });
    expect(summary.state).toBe('partial');
    expect(summary.plates).toHaveLength(1);
    expect(summary.sum.seconds).toBe(4020);
    expect(printFactsOf({ printInfo: gcodeInfo, printFacts: undefined })?.printSecondsComplete).toBe(true);
  });

  it('includes unread G-code in "read all", next to unread 3MF files, and leaves read ones and models alone', () => {
    const unread3mf = makeModelFile({ id: 'a', path: '/c/a.3mf' });
    const unreadGcode = makeModelFile({ id: 'b', path: '/c/b.gcode' });
    const unreadBgcode = makeModelFile({ id: 'c', path: '/c/c.bgcode', printDataState: 'unread' });
    const readGcode = makeModelFile({ id: 'd', path: '/c/d.gcode', printInfo: gcodeInfo });
    const stl = makeModelFile({ id: 'e', path: '/c/e.stl' });
    const trashed = makeModelFile({ id: 'f', path: '/c/f.gcode', deletedAt: '2026-01-01T00:00:00Z' });
    const all = [unread3mf, unreadGcode, unreadBgcode, readGcode, stl, trashed];
    expect(rereadTargets(all, 'all')).toEqual(['a', 'b', 'c']);
    expect(rereadTargets(all, 'selected', new Set(['d', 'e']))).toEqual(['d']);
  });
});

describe('length-only usage', () => {
  const only = (usedG: number, usedM: number, total = 0): SliceInfo => ({ totalWeightG: total, plates: [{ plateIndex: 1, weightG: total, filaments: [{ type: 'PLA', color: null, usedG, usedM }] }] });

  it('is recognised like the backend does: lengths next to zero grams, nothing else', () => {
    expect(isWeightUnknown(only(0, 1.5))).toBe(true);
    expect(isWeightUnknown(only(4, 1.5, 4))).toBe(false);
    expect(isWeightUnknown(only(0, 0))).toBe(false);
    expect(isWeightUnknown({ totalWeightG: 0, plates: [] })).toBe(false);
    expect(isWeightUnknown(null)).toBe(false);
  });

  it('gives a plate without a weight instead of 0 g, and a total that is unknown too', () => {
    const rows = summarizePrintData({ sliceInfo: only(0, 1.5), printInfo: null }).plates;
    expect(rows[0].weightG).toBeNull();
    expect(rows[0].weightUnknown).toBe(true);
    expect(sumWeightG(rows)).toBeNull();
  });

  it('leaves a normal usage alone', () => {
    const rows = summarizePrintData({ sliceInfo: sliceInfo(1), printInfo: null }).plates;
    expect(rows[0].weightG).toBe(10);
    expect(rows[0].weightUnknown).toBe(false);
  });
});
