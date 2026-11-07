import { describe, expect, it, vi, beforeEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import { createElement, useState, type ReactNode } from 'react';
import type { ModelFile } from '../types';

const { rescan, listByIds, invalidate } = vi.hoisted(() => ({ rescan: vi.fn(), listByIds: vi.fn(), invalidate: vi.fn() }));
vi.mock('../lib/api/files', () => ({ rescanFileMetadata: rescan, listFilesByIds: listByIds }));
vi.mock('./useModelImages', () => ({ invalidateModelImages: invalidate }));

import { PrintDataRereadProvider, applyRereadPatches, patchOf, runPrintDataReread, usePrintDataReread, type RereadDeps, type RereadPatch } from './usePrintDataReread';
import { printDataStateOf, rereadTargets } from '../lib/printData';

const IMAGE = 'data:image/png;base64,AAAA';

function model(id: string, over: Partial<ModelFile> = {}): ModelFile {
  return {
    id, name: id, path: `/m/${id}.3mf`, folderId: 'f', tags: [], origin: 'local', sync: 'local-only', dimensionsMm: null, volumeCm3: null,
    objectCount: null, plateCount: null, materials: [], fileSizeBytes: 1, importedAt: '', fileModifiedAt: null, printStatus: 'not_printed',
    estimatedWeightG: 12, weightSource: 'slicer', sliceInfo: null, printInfo: null, costEstimate: null, lastViewedAt: null, contentHash: null,
    creator: null, customImage: null, thumbnailImage: null, renderSnapshotImage: null, sourceUrl: null, sourcePlatform: null, sourceModelId: null,
    sourceProfileId: null, queuePosition: null, favorite: false, deletedAt: null, license: null, hasThumbnail: false, printDataState: 'unread', summaryOnly: true,
    ...over,
  } as ModelFile;
}

/** The answer of a read: current print data, and the three pictures a non-lean answer would carry. */
function answer(id: string, over: Partial<ModelFile> = {}): ModelFile {
  return model(id, {
    printDataState: undefined, summaryOnly: undefined, thumbnailImage: IMAGE, customImage: IMAGE, renderSnapshotImage: IMAGE,
    printInfo: { sliceState: 'sliced', settingsState: 'read', readerCurrent: true, readAt: 'x', printerModel: 'P', bedType: 'B', layerHeightMm: 0.2,
      nozzleMm: { value: 0.4, mixed: false }, bedTempC: { value: 55, mixed: false }, nozzleTempC: { value: 220, mixed: false }, plates: [], filamentColors: [] },
    ...over,
  });
}

function deps(over: Partial<RereadDeps> = {}): RereadDeps & { batches: ReadonlyMap<string, RereadPatch>[]; waits: number[] } {
  const batches: ReadonlyMap<string, RereadPatch>[] = [];
  const waits: number[] = [];
  return {
    rescan: async id => answer(id), applyPatches: p => { batches.push(p); }, invalidate: vi.fn(), wait: async ms => { waits.push(ms); },
    cancelled: () => false, now: () => 0, onProgress: () => {}, ...over, batches, waits,
  };
}

beforeEach(() => { rescan.mockReset(); listByIds.mockReset(); invalidate.mockReset(); });

describe('runPrintDataReread', () => {
  it('reads one file at a time, in order', async () => {
    let running = 0; let maxRunning = 0; const order: string[] = [];
    const d = deps({ rescan: async id => { running++; maxRunning = Math.max(maxRunning, running); order.push(id); await Promise.resolve(); await Promise.resolve(); running--; return answer(id); } });
    const outcome = await runPrintDataReread(['a', 'b', 'c', 'd'], d);
    expect(order).toEqual(['a', 'b', 'c', 'd']);
    expect(maxRunning).toBe(1);
    expect(outcome).toMatchObject({ done: 4, failed: 0, cancelled: false });
  });

  it('reports the progress after every file', async () => {
    const seen: number[] = [];
    await runPrintDataReread(['a', 'b', 'c'], deps({ onProgress: p => seen.push(p.done) }));
    expect(seen).toEqual([1, 2, 3]);
  });

  it('stops after the file in progress when cancelled: file 3 of 5 and later stay untouched', async () => {
    let cancelled = false;
    const calls: string[] = [];
    const d = deps({ cancelled: () => cancelled, rescan: async id => { calls.push(id); if (id === 'b') cancelled = true; return answer(id); } });
    const outcome = await runPrintDataReread(['a', 'b', 'c', 'd', 'e'], d);
    expect(calls).toEqual(['a', 'b']);
    expect(outcome).toMatchObject({ done: 2, cancelled: true });
    // What was read before the stop is kept: the pending batch is still written.
    expect([...d.batches.flatMap(b => [...b.keys()])]).toEqual(['a', 'b']);
  });

  it('counts failures per file, goes on and keeps the first error', async () => {
    const d = deps({ rescan: async id => { if (id === 'b' || id === 'd') throw { message: id === 'b' ? 'rescan_file_invalid' : 'rescan_file_changed', expected: true }; return answer(id); } });
    const outcome = await runPrintDataReread(['a', 'b', 'c', 'd'], d);
    expect(outcome).toMatchObject({ done: 4, failed: 2, cancelled: false });
    expect(outcome.firstError).toMatchObject({ message: 'rescan_file_invalid', unexpected: false });
    expect([...d.batches.flatMap(b => [...b.keys()])]).toEqual(['a', 'c']);
  });

  it('pauses while an import runs, asks again after 2 s and then continues', async () => {
    let attempts = 0; const paused: boolean[] = [];
    const d = deps({
      rescan: async id => { if (id === 'b' && attempts++ < 2) throw { message: 'importActive', expected: true }; return answer(id); },
      onProgress: p => paused.push(p.paused),
    });
    const outcome = await runPrintDataReread(['a', 'b', 'c'], d);
    expect(d.waits).toEqual([2000, 2000]);
    expect(paused).toContain(true);
    expect(outcome).toMatchObject({ done: 3, failed: 0, paused: false, cancelled: false });
    expect(paused[paused.length - 1]).toBe(false);
  });

  it('cancels out of an import pause without counting the waiting file', async () => {
    let cancelled = false;
    const d = deps({ cancelled: () => cancelled, wait: async () => { cancelled = true; }, rescan: async id => { if (id === 'b') throw { message: 'importActive', expected: true }; return answer(id); } });
    const outcome = await runPrintDataReread(['a', 'b', 'c'], d);
    expect(outcome).toMatchObject({ done: 1, cancelled: true, failed: 0 });
  });

  it('repeats a busy web answer with Retry-After up to 3 times, then counts a failure', async () => {
    let busy = 3;
    const ok = deps({ rescan: async id => { if (busy-- > 0) throw { message: 'busy', retryAfter: '2' }; return answer(id); } });
    expect(await runPrintDataReread(['a'], ok)).toMatchObject({ done: 1, failed: 0 });
    expect(ok.waits).toEqual([2000, 2000, 2000]);
    const tries = vi.fn(async () => { throw { message: 'busy', retryAfter: '1' }; });
    const bad = deps({ rescan: tries });
    expect(await runPrintDataReread(['a', 'b'], bad)).toMatchObject({ done: 2, failed: 2 });
    expect(tries).toHaveBeenCalledTimes(8); // 1 try + 3 repeats per file
  });

  it('does not repeat an error without Retry-After', async () => {
    const tries = vi.fn(async () => { throw { message: 'boom', expected: false }; });
    const outcome = await runPrintDataReread(['a'], deps({ rescan: tries }));
    expect(tries).toHaveBeenCalledTimes(1);
    expect(outcome.firstError).toMatchObject({ message: 'boom', unexpected: true });
  });

  it('counts a read with missing values as partial and drops the picture cache per file', async () => {
    const d = deps({ rescan: async id => id === 'a' ? answer(id, { printInfo: { ...answer(id).printInfo!, bedType: null } , sliceInfo: { plates: [{ plateIndex: 1, weightG: 1, filaments: [] }] } as never }) : answer(id) });
    const outcome = await runPrintDataReread(['a', 'b'], d);
    expect(outcome.partial).toBe(1);
    expect(d.invalidate).toHaveBeenCalledTimes(2);
    expect(d.invalidate).toHaveBeenCalledWith('a');
  });
});

describe('large catalogs (5000 mock files)', () => {
  const N = 5000;
  const ids = Array.from({ length: N }, (_, i) => `f${i}`);
  /** An array that counts how many elements each pass over it visits. */
  class CountingArray<T> extends Array<T> {
    static visited = 0;
    static get [Symbol.species]() { return Array; }
    map<U>(fn: (value: T, index: number, array: T[]) => U): U[] { CountingArray.visited += this.length; return Array.prototype.map.call(this, fn) as U[]; }
  }

  async function runAll(over: Partial<RereadDeps> = {}) {
    let models: ModelFile[] = ids.map(id => model(id));
    const counting = () => { const list = new CountingArray<ModelFile>(); list.push(...models); return list as unknown as ModelFile[]; };
    let setModelsCalls = 0; CountingArray.visited = 0;
    let clock = 0;
    await runPrintDataReread(ids, deps({
      now: () => (clock += 1),
      applyPatches: patches => { setModelsCalls++; models = applyRereadPatches(counting(), patches); },
      ...over,
    }));
    return { models, setModelsCalls, visited: CountingArray.visited };
  }

  it('criterion 1: no patched model carries picture content', async () => {
    const { models } = await runAll();
    expect(models.every(m => m.printInfo !== null)).toBe(true);
    expect(models.filter(m => m.thumbnailImage || m.customImage || m.renderSnapshotImage)).toHaveLength(0);
    expect(models.every(m => m.summaryOnly === true)).toBe(true);
  });

  it('criterion 2: at most ceil(5000 / 50) + 1 list updates and 1 million visited elements', async () => {
    const { setModelsCalls, visited } = await runAll();
    expect(setModelsCalls).toBeLessThanOrEqual(Math.ceil(N / 50) + 1);
    expect(visited).toBeLessThanOrEqual(1_000_000);
    expect(visited).toBeGreaterThan(0);
  });

  it('also writes a batch after 250 ms, even with fewer than 50 files', async () => {
    let clock = 0; const batches: number[] = [];
    await runPrintDataReread(['a', 'b', 'c', 'd'], deps({ now: () => clock, rescan: async id => { clock += 200; return answer(id); }, applyPatches: p => { batches.push(p.size); } }));
    expect(batches).toEqual([2, 2]);
  });
});

describe('patches', () => {
  it('carries only the listed fields and never a picture', () => {
    expect(Object.keys(patchOf(answer('a'))).sort()).toEqual(['contentHash', 'costEstimate', 'dimensionsMm', 'estimatedWeightG', 'fileModifiedAt', 'fileSizeBytes', 'hasThumbnail', 'materials', 'plateCount', 'plateDimensions', 'printDataState', 'printFacts', 'printInfo', 'sliceInfo', 'sourceModelId', 'sourcePlatform', 'sourceProfileId', 'weightSource']);
  });

  it('derives the new list state from the stored row', () => {
    expect(patchOf(answer('a')).printDataState).toBe('sliced');
    expect(patchOf(answer('a', { printInfo: { ...answer('a').printInfo!, sliceState: 'no_slice_data' } })).printDataState).toBe('no_slice_data');
    expect(patchOf(answer('a', { printInfo: { ...answer('a').printInfo!, readerCurrent: false } })).printDataState).toBe('unread');
  });

  it('gives a read G-code file the state of its stored row, not "none"', () => {
    expect(patchOf(answer('a', { path: '/m/a.gcode' })).printDataState).toBe('sliced');
    expect(patchOf(answer('a', { path: '/m/a.bgcode', printInfo: { ...answer('a').printInfo!, sliceState: 'not_recognized' } })).printDataState).toBe('not_recognized');
  });

  it('derives the print facts of the list from the stored row', () => {
    expect(patchOf(answer('a')).printFacts).toEqual({ printerModel: 'P', layerHeightMm: 0.2, nozzleMm: 0.4, plateCount: 0 });
  });

  it('clears facts a file no longer has, and replaces stale ones when applied', () => {
    const unusable = patchOf(answer('a', { printInfo: { ...answer('a').printInfo!, sliceState: 'not_recognized' } }));
    expect(unusable.printFacts).toBeUndefined();
    const stale = model('a', { printFacts: { printerModel: 'Old', plateCount: 3 } });
    expect(applyRereadPatches([stale], new Map([['a', unusable]]))[0].printFacts).toBeUndefined();
    expect(applyRereadPatches([stale], new Map([['a', patchOf(answer('a'))]]))[0].printFacts?.printerModel).toBe('P');
  });
});

describe('rereadTargets', () => {
  const models = [
    model('slicedUnread'),
    model('slicedRead', { printDataState: 'sliced' }),
    model('sourceModel', { weightSource: 'estimated' }),
    model('stl', { path: '/m/a.stl', printDataState: 'none' }),
    model('trashed', { deletedAt: 'x' }),
    model('fullRecord', { printDataState: undefined, summaryOnly: undefined, printInfo: null }),
  ];

  it('all: every 3MF whose print data are unread, whatever its weight source', () => {
    expect(rereadTargets(models, 'all')).toEqual(['slicedUnread', 'sourceModel', 'fullRecord']);
  });

  it('selected: only the chosen 3MF files, whatever their state', () => {
    expect(rereadTargets(models, 'selected', new Set(['slicedRead', 'sourceModel', 'stl', 'ghost']))).toEqual(['slicedRead', 'sourceModel']);
  });

  it('a PrusaSlicer project without slice data is read by "all" while unread, and by "selected" in any state', () => {
    // No slicer weight (nothing sliced), print data stored by an older reader version: shown as unread.
    const prusa = model('prusa', { weightSource: 'estimated', printDataState: 'unread' });
    const prusaDone = model('prusaDone', { weightSource: 'estimated', printDataState: 'no_slice_data' });
    const list = [prusa, prusaDone, model('slicedUnread')];
    expect(rereadTargets(list, 'all')).toEqual(['prusa', 'slicedUnread']);
    expect(rereadTargets(list, 'selected', new Set(['prusa', 'prusaDone']))).toEqual(['prusa', 'prusaDone']);
  });

  it('derives the state of a full record without a list state', () => {
    expect(printDataStateOf(model('x', { printDataState: undefined, printInfo: answer('x').printInfo }))).toBe('sliced');
    expect(printDataStateOf(model('x', { path: '/a.obj', printDataState: undefined }))).toBe('none');
  });
});

describe('PrintDataRereadProvider', () => {
  function setup(initial: ModelFile[]) {
    let latest: ModelFile[] = initial;
    const wrapper = ({ children }: { children: ReactNode }) => {
      const [models, setModels] = useState(initial);
      latest = models;
      return createElement(PrintDataRereadProvider, { models, setModels }, children);
    };
    const view = renderHook(() => usePrintDataReread(), { wrapper });
    return { view, models: () => latest };
  }

  it('reads sequentially with lean answers and patches the list without pictures', async () => {
    rescan.mockImplementation(async (id: string) => answer(id));
    const { view, models } = setup([model('a'), model('b'), model('x', { printDataState: 'sliced' })]);
    let result = '';
    act(() => { result = view.result.current.start('settings'); });
    expect(result).toBe('started');
    await waitFor(() => expect(view.result.current.state.status).toBe('done'));
    expect(rescan.mock.calls).toEqual([['a', true], ['b', true]]);
    expect(view.result.current.state).toMatchObject({ done: 2, total: 2, failed: 0 });
    expect(models().map(m => m.printDataState)).toEqual(['sliced', 'sliced', 'sliced']);
    expect(models().every(m => !m.thumbnailImage && !m.customImage && !m.renderSnapshotImage)).toBe(true);
  });

  it('refuses a second start while a run is going and shows that it is running', async () => {
    let release: (m: ModelFile) => void = () => {};
    rescan.mockImplementation((id: string) => new Promise<ModelFile>(resolve => { release = () => resolve(answer(id)); }));
    const { view } = setup([model('a'), model('b')]);
    act(() => { view.result.current.start('settings'); });
    let second = '';
    act(() => { second = view.result.current.start('selection', ['a']); });
    expect(second).toBe('busy');
    expect(view.result.current.state.busyNotice).toBe('selection');
    expect(rescan).toHaveBeenCalledTimes(1);
    act(() => { release(answer('a')); });
    await waitFor(() => expect(rescan).toHaveBeenCalledTimes(2));
    act(() => { release(answer('b')); });
    await waitFor(() => expect(view.result.current.state.status).toBe('done'));
    expect(view.result.current.state.busyNotice).toBeNull();
  });

  it('reads the record again when only the stored answer of the web server was lost', async () => {
    rescan.mockRejectedValue({ message: 'request_id_result_unavailable', expected: true });
    listByIds.mockResolvedValue([answer('a')]);
    const { view, models } = setup([model('a')]);
    act(() => { view.result.current.start('settings'); });
    await waitFor(() => expect(view.result.current.state.status).toBe('done'));
    expect(view.result.current.state.failed).toBe(0);
    expect(models()[0].printInfo).not.toBeNull();
    expect(models()[0].thumbnailImage).toBeNull();
  });

  it('ends with the text "nothing to read" when no file is due', () => {
    const { view } = setup([model('x', { printDataState: 'sliced' })]);
    let result = '';
    act(() => { result = view.result.current.start('settings'); });
    expect(result).toBe('nothing');
    expect(view.result.current.state).toMatchObject({ status: 'done', total: 0 });
    expect(rescan).not.toHaveBeenCalled();
  });

  it('cancels a run: the current file ends, the rest stays untouched, cancel is reported', async () => {
    let release: (m: ModelFile) => void = () => {};
    rescan.mockImplementation((id: string) => new Promise<ModelFile>(resolve => { release = () => resolve(answer(id)); }));
    const { view, models } = setup([model('a'), model('b'), model('c'), model('d'), model('e')]);
    act(() => { view.result.current.start('settings'); });
    act(() => { release(answer('a')); });
    await waitFor(() => expect(rescan).toHaveBeenCalledTimes(2));
    act(() => { view.result.current.cancel(); });
    expect(view.result.current.state.status).toBe('cancelling');
    act(() => { release(answer('b')); });
    await waitFor(() => expect(view.result.current.state.status).toBe('cancelled'));
    expect(rescan).toHaveBeenCalledTimes(2);
    expect(view.result.current.state).toMatchObject({ done: 2, total: 5 });
    expect(models().map(m => m.printDataState)).toEqual(['sliced', 'sliced', 'unread', 'unread', 'unread']);
  });
});
