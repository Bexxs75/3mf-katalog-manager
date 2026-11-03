import { describe, expect, it } from 'vitest';
import {
  EMPTY_FILTER, colorBucket, evaluate, isActive, layerKey, nozzleKey, printDataFacets, printerKey,
  splitByPrintData, timeBucketOf, type PrintDataFilter,
} from './printDataFilter';
import { printFactsOf } from './printData';
import goldenFile from '../../crates/katalog-core/tests/fixtures/print-facts-golden.json';
import { makeModelFile, makePrintFacts, makePrintInfo } from '../test/factories';
import type { ModelFile, PrintFacts } from '../types';

const filter = (over: Partial<PrintDataFilter>): PrintDataFilter => ({ ...EMPTY_FILTER, ...over });
const withFacts = (facts: Partial<PrintFacts> | null, over: Partial<ModelFile> = {}) =>
  makeModelFile({ printFacts: facts ? makePrintFacts(facts) : undefined, printDataState: facts ? 'sliced' : 'unread', ...over });

describe('printFactsOf against the shared golden file', () => {
  // The same file the Rust side checks, so the two derivations of the sum rules cannot drift apart.
  const golden = goldenFile as unknown as {
    cases: { name: string; expected: { printInfo: ModelFile['printInfo']; printFacts: PrintFacts | null } }[];
  };

  it('has the cases', () => expect(golden.cases.length).toBeGreaterThanOrEqual(8));

  it.each(golden.cases.map(entry => [entry.name, entry] as const))('derives the facts of: %s', (_name, entry) => {
    expect(printFactsOf({ printInfo: entry.expected.printInfo, printFacts: undefined })).toEqual(entry.expected.printFacts);
  });

  it('leaves a left-out flag out instead of sending false', () => {
    const facts = printFactsOf({ printInfo: makePrintInfo({ plates: [] }), printFacts: undefined });
    expect(facts).toEqual({ plateCount: 0 });
    expect(Object.keys(facts ?? {})).toEqual(['plateCount']);
  });
});

describe('printFactsOf', () => {
  it('uses the list block when the print info is not loaded', () => {
    const facts = makePrintFacts({ printerModel: 'A' });
    expect(printFactsOf({ printInfo: null, printFacts: facts })).toBe(facts);
    expect(printFactsOf({ printInfo: null, printFacts: undefined })).toBeNull();
  });

  it('prefers the loaded print info over a stale list block', () => {
    const info = makePrintInfo({ printerModel: 'Fresh' });
    expect(printFactsOf({ printInfo: info, printFacts: makePrintFacts({ printerModel: 'Stale' }) })?.printerModel).toBe('Fresh');
  });

  it('does not break on an answer without a color list (older server)', () => {
    const old = { ...makePrintInfo({ printerModel: 'A' }), filamentColors: undefined } as unknown as ModelFile['printInfo'];
    expect(printFactsOf({ printInfo: old, printFacts: undefined })).toEqual({ plateCount: 0, printerModel: 'A' });
  });

  it('knows nothing for a row of another reader version', () => {
    expect(printFactsOf({ printInfo: makePrintInfo({ readerCurrent: false, printerModel: 'A' }), printFacts: undefined })).toBeNull();
  });
});

describe('keys', () => {
  it('rounds float noise away', () => {
    expect(layerKey(0.2)).toBe(layerKey(0.20000000298));
    expect(nozzleKey(0.4000000059604645)).toBe(0.4);
    expect(layerKey(0.125)).toBe(0.125);
    expect(nozzleKey(0.25)).toBe(0.25);
  });

  it('puts the printer name in one spelling-insensitive key', () => {
    expect(printerKey('  Bambu Lab X1 Carbon ')).toBe(printerKey('bambu lab x1 carbon'));
    expect(printerKey('   ')).toBeNull();
    expect(printerKey(undefined)).toBeNull();
    expect(printerKey('MK4IS')).not.toBe(printerKey('Prusa MK4S'));
  });

  it.each([
    [0, 'lt1'], [3599, 'lt1'], [3600, '1to4'], [14399, '1to4'], [14400, '4to12'], [43199, '4to12'], [43200, 'ge12'], [999999, 'ge12'],
  ] as const)('puts %i s in %s', (seconds, bucket) => expect(timeBucketOf(seconds)).toBe(bucket));
});

describe('colorBucket', () => {
  it.each([
    ['#000000', 'black'], ['#000000FF', 'black'], ['#1c1c1c', 'black'], ['#2b2b2b', 'black'], ['#161616', 'black'],
    ['#ffffff', 'white'], ['#FFFFFFFF', 'white'], ['#f2f2f0', 'white'], ['#fafafa', 'white'],
    ['#808080', 'gray'], ['#8a8d91', 'gray'], ['#c0c0c0', 'gray'], ['#4a4a4a', 'gray'],
    ['#ff0000', 'red'], ['#FF0000FF', 'red'], ['#c8322d', 'red'], ['#8b0000', 'red'], ['#e0245e', 'red'],
    ['#ff8000', 'orange'], ['#e8832a', 'orange'], ['#e8590c', 'orange'], ['#ffa500', 'orange'],
    ['#ffff00', 'yellow'], ['#e9c93a', 'yellow'], ['#f5e642', 'yellow'],
    ['#00ff00', 'green'], ['#3f9a5a', 'green'], ['#2e7d32', 'green'], ['#008000', 'green'],
    ['#0000ff', 'blue'], ['#2f6fc4', 'blue'], ['#00ffff', 'blue'], ['#1e3a8a', 'blue'],
    ['#7a4a2b', 'brown'], ['#8b4513', 'brown'], ['#5c3317', 'brown'], ['#6b3a2e', 'brown'],
    ['#800080', 'violet'], ['#8a4fb5', 'violet'], ['#ff69b4', 'violet'], ['#ffc0cb', 'violet'], ['#ff00ff', 'violet'],
  ] as const)('puts %s in %s', (hex, expected) => expect(colorBucket(hex)).toBe(expected));

  it.each(['red', '#12', '#gggggg', '', '#ff00', '#ff0000f'])('refuses %j', value => expect(colorBucket(value)).toBeNull());
});

describe('evaluate', () => {
  const x1 = withFacts({ printerModel: 'Bambu Lab X1 Carbon', nozzleMm: 0.4, layerHeightMm: 0.2, printSeconds: 7200, printSecondsComplete: true, colors: ['#ff0000', '#ffffff'] });

  it('passes everything without an active filter', () => {
    expect(evaluate(withFacts(null), EMPTY_FILTER)).toEqual({ fail: false, missing: [] });
    expect(isActive(filter({ includeUnknown: true }))).toBe(false);
  });

  it('passes a matching value and fails a different one', () => {
    expect(evaluate(x1, filter({ printers: ['bambu lab x1 carbon'] }))).toEqual({ fail: false, missing: [] });
    expect(evaluate(x1, filter({ printers: ['prusa mk4s'] }))).toEqual({ fail: true, missing: [] });
  });

  it('treats the choices inside one filter as OR', () => {
    expect(evaluate(x1, filter({ nozzles: [0.2, 0.4] })).fail).toBe(false);
    expect(evaluate(x1, filter({ nozzles: [0.2, 0.6] })).fail).toBe(true);
  });

  it('treats several filters as AND', () => {
    expect(evaluate(x1, filter({ nozzles: [0.4], layers: [0.2] })).fail).toBe(false);
    expect(evaluate(x1, filter({ nozzles: [0.4], layers: [0.16] })).fail).toBe(true);
  });

  it('names a missing value instead of failing it', () => {
    const none = withFacts(null);
    expect(evaluate(none, filter({ printers: ['x'] }))).toEqual({ fail: false, missing: ['printer'] });
    expect(evaluate(none, filter({ printers: ['x'], nozzles: [0.4], layers: [0.2], times: ['lt1'], colors: ['red'] })).missing)
      .toEqual(['printer', 'nozzle', 'layer', 'time', 'color']);
  });

  it('lists only the missing filters, and a value that differs still fails', () => {
    const half = withFacts({ printerModel: 'A' });
    expect(evaluate(half, filter({ printers: ['a'], layers: [0.2] }))).toEqual({ fail: false, missing: ['layer'] });
    expect(evaluate(half, filter({ printers: ['b'], layers: [0.2] }))).toEqual({ fail: true, missing: ['layer'] });
  });

  it('counts an unread file (no facts) as missing', () => {
    expect(evaluate(withFacts(null), filter({ layers: [0.2] }))).toEqual({ fail: false, missing: ['layer'] });
  });

  it('counts a mixed nozzle as missing, never as a value', () => {
    const mixed = withFacts({ nozzleMixed: true });
    expect(evaluate(mixed, filter({ nozzles: [0.4] }))).toEqual({ fail: false, missing: ['nozzle'] });
    // Even a stray single value next to the flag must not be taken for the nozzle.
    expect(evaluate(withFacts({ nozzleMixed: true, nozzleMm: 0.4 }), filter({ nozzles: [0.4] }))).toEqual({ fail: false, missing: ['nozzle'] });
  });

  it('matches 0.2 against 0.20000000298 and a nozzle against its float noise', () => {
    const noisy = withFacts({ layerHeightMm: 0.20000000298, nozzleMm: 0.4000000059604645 });
    expect(evaluate(noisy, filter({ layers: [0.2], nozzles: [0.4] }))).toEqual({ fail: false, missing: [] });
  });

  it('matches the printer without regard to case', () => {
    expect(evaluate(withFacts({ printerModel: ' PRUSA MK4S ' }), filter({ printers: ['prusa mk4s'] })).fail).toBe(false);
  });

  it.each([
    [3599, true, ['lt1'], false], [3600, true, ['lt1'], true], [3600, true, ['1to4'], false],
    [14399, true, ['1to4'], false], [14400, true, ['1to4'], true], [14400, true, ['4to12'], false],
    [43199, true, ['4to12'], false], [43200, true, ['4to12'], true], [43200, true, ['ge12'], false],
  ] as const)('puts %i s (complete %s) against %j: fail %s', (seconds, complete, times, fail) => {
    const model = withFacts({ printSeconds: seconds, ...(complete ? { printSecondsComplete: true as const } : {}) });
    expect(evaluate(model, filter({ times: [...times] }))).toEqual({ fail, missing: [] });
  });

  it('uses no print time when a plate has none (only a partial sum)', () => {
    const partial = withFacts({ printSeconds: 5000 });
    expect(evaluate(partial, filter({ times: ['1to4'] }))).toEqual({ fail: false, missing: ['time'] });
  });

  it('matches colors by OR and by AND', () => {
    expect(evaluate(x1, filter({ colors: ['red', 'blue'], colorMode: 'or' })).fail).toBe(false);
    expect(evaluate(x1, filter({ colors: ['red', 'blue'], colorMode: 'and' })).fail).toBe(true);
    expect(evaluate(x1, filter({ colors: ['red', 'white'], colorMode: 'and' })).fail).toBe(false);
    expect(evaluate(x1, filter({ colors: ['blue'], colorMode: 'or' })).fail).toBe(true);
  });

  it('counts a file without colors as missing for the color filter', () => {
    expect(evaluate(withFacts({ printerModel: 'A' }), filter({ colors: ['red'] }))).toEqual({ fail: false, missing: ['color'] });
  });

  it('works from a loaded print info as well', () => {
    const loaded = makeModelFile({ printInfo: makePrintInfo({ layerHeightMm: 0.16, filamentColors: ['#2e7d32'] }) });
    expect(evaluate(loaded, filter({ layers: [0.16], colors: ['green'] }))).toEqual({ fail: false, missing: [] });
  });
});

describe('splitByPrintData', () => {
  const match = withFacts({ layerHeightMm: 0.2 }, { id: 'match' });
  const other = withFacts({ layerHeightMm: 0.28 }, { id: 'other' });
  const unreadOne = withFacts(null, { id: 'unread' });
  const noRow = withFacts({ printerModel: 'A' }, { id: 'norow', printDataState: 'no_slice_data' });
  const stl = makeModelFile({ id: 'stl', path: '/c/a.stl', printDataState: 'none' });
  const models = [match, other, unreadOne, noRow, stl];

  it('returns the list untouched without an active filter', () => {
    const result = splitByPrintData(models, EMPTY_FILTER);
    expect(result.visible).toBe(models);
    expect([result.hidden, result.shownUnknown, result.unread]).toEqual([0, 0, 0]);
  });

  it('shows matches, drops differing files and holds back the ones without a value', () => {
    const result = splitByPrintData(models, filter({ layers: [0.2] }));
    expect(result.visible.map(m => m.id)).toEqual(['match']);
    expect(result.hidden).toBe(3);
    expect(result.shownUnknown).toBe(0);
  });

  it('counts how many of the hidden ones are unread (a non-3MF is not)', () => {
    expect(splitByPrintData(models, filter({ layers: [0.2] })).unread).toBe(1);
  });

  it('shows the missing ones too when asked and keeps the differing ones out', () => {
    const result = splitByPrintData(models, filter({ layers: [0.2], includeUnknown: true }));
    expect(result.visible.map(m => m.id)).toEqual(['match', 'unread', 'norow', 'stl']);
    expect(result.shownUnknown).toBe(3);
    expect(result.hidden).toBe(0);
    expect(result.unread).toBe(0);
  });

  it('measures 5000 models with five active filters', () => {
    const many = Array.from({ length: 5000 }, (_, i) => withFacts(i % 3 === 0 ? null : {
      printerModel: i % 2 ? 'A' : 'B', nozzleMm: 0.4, layerHeightMm: 0.2, printSeconds: i, printSecondsComplete: true, colors: ['#ff0000'],
    }, { id: String(i) }));
    const everything = filter({ printers: ['a'], nozzles: [0.4], layers: [0.2], times: ['lt1'], colors: ['red'] });
    const start = performance.now();
    const result = splitByPrintData(many, everything);
    const facets = printDataFacets(many);
    // eslint-disable-next-line no-console
    console.info(`splitByPrintData + facets over 5000 models: ${(performance.now() - start).toFixed(1)} ms`);
    expect(result.visible.length + result.hidden).toBeGreaterThan(0);
    expect(facets.missing.printer).toBe(1667);
  });
});

describe('printDataFacets', () => {
  const models = [
    withFacts({ printerModel: 'Bambu Lab X1 Carbon', nozzleMm: 0.4, layerHeightMm: 0.2, printSeconds: 100, printSecondsComplete: true, colors: ['#ff0000', '#ee1111', '#ffffff'] }),
    withFacts({ printerModel: 'bambu lab x1 carbon', nozzleMm: 0.4, layerHeightMm: 0.20000000298 }),
    withFacts({ printerModel: 'Bambu Lab X1 Carbon', nozzleMm: 0.6, layerHeightMm: 0.28 }),
    withFacts({ nozzleMixed: true }),
    withFacts(null),
  ];
  const facets = printDataFacets(models);

  it('merges spellings and shows the most frequent one', () => {
    expect(facets.printers).toEqual([{ key: 'bambu lab x1 carbon', label: 'Bambu Lab X1 Carbon', count: 3 }]);
  });

  it('counts nozzles and layers by rounded value, sorted', () => {
    expect(facets.nozzles).toEqual([{ key: 0.4, count: 2 }, { key: 0.6, count: 1 }]);
    expect(facets.layers).toEqual([{ key: 0.2, count: 2 }, { key: 0.28, count: 1 }]);
  });

  it('counts a file once per color group and lists only groups that occur', () => {
    expect(facets.colors).toEqual([{ key: 'white', count: 1 }, { key: 'red', count: 1 }]);
    expect(facets.times).toEqual([{ key: 'lt1', count: 1 }]);
  });

  it('counts the files without a value per filter (a mixed nozzle is one of them)', () => {
    expect(facets.missing).toEqual({ printer: 2, nozzle: 2, layer: 2, time: 4, color: 4 });
  });
});
