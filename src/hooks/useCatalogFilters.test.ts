import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useCatalogFilters } from './useCatalogFilters';
import { makeModelFile, makePrintFacts, makePrinter } from '../test/factories';
import { EMPTY_FILTER } from '../lib/printDataFilter';

beforeEach(() => localStorage.removeItem('3mf-katalog-view'));
afterEach(() => localStorage.removeItem('3mf-katalog-view'));

describe('useCatalogFilters', () => {
  it('starts with the saved view', () => {
    localStorage.setItem('3mf-katalog-view', 'groupedList');
    const { result } = renderHook(() => useCatalogFilters([], []));
    expect(result.current.view).toBe('groupedList');
  });

  it('persists an explicit view choice across remounts without persisting sorting', () => {
    const { result, unmount } = renderHook(() => useCatalogFilters([], []));
    act(() => {
      result.current.setView('groupedGrid');
      result.current.setSort('size');
    });
    expect(result.current.view).toBe('groupedGrid');
    expect(localStorage.getItem('3mf-katalog-view')).toBe('groupedGrid');
    unmount();
    const remounted = renderHook(() => useCatalogFilters([], []));
    expect(remounted.result.current.view).toBe('groupedGrid');
    expect(remounted.result.current.sort).toBe('name');
    expect(remounted.result.current.sortDirection).toBe('asc');
  });

  it('returns initial state with correct structure', () => {
    const { result } = renderHook(() => useCatalogFilters([], []));

    expect(result.current).toHaveProperty('view');
    expect(result.current).toHaveProperty('setView');
    expect(result.current).toHaveProperty('sort');
    expect(result.current).toHaveProperty('setSort');
    expect(result.current).toHaveProperty('query');
    expect(result.current).toHaveProperty('setQuery');
    expect(result.current).toHaveProperty('activeFolderId');
    expect(result.current).toHaveProperty('setActiveFolderId');
    expect(result.current).toHaveProperty('activeTag');
    expect(result.current).toHaveProperty('setActiveTag');
    expect(result.current).toHaveProperty('filtered');
    expect(result.current).toHaveProperty('queue');
  });

  it('applies filter and sort through the hook', () => {
    const models = [
      makeModelFile({ id: '1', name: 'Benchy', tags: ['red'] }),
      makeModelFile({ id: '2', name: 'Vase', tags: [] }),
    ];
    const { result } = renderHook(() => useCatalogFilters(models, []));

    expect(result.current.filtered.length).toBe(2);

    act(() => {
      result.current.setQuery('Benchy');
    });

    expect(result.current.filtered.length).toBe(1);
    expect(result.current.filtered[0].id).toBe('1');
  });

  it('queues models separately from filtered', () => {
    const models = [
      makeModelFile({ id: '1', queuePosition: 1 }),
      makeModelFile({ id: '2', queuePosition: null }),
    ];
    const { result } = renderHook(() => useCatalogFilters(models, []));

    expect(result.current.queue.length).toBe(1);
    expect(result.current.queue[0].id).toBe('1');
    expect(result.current.filtered.length).toBe(2);
  });

  it('updates state setters', () => {
    const { result } = renderHook(() => useCatalogFilters([], []));

    expect(result.current.view).toBe('grid');
    act(() => {
      result.current.setView('groupedList');
    });
    expect(result.current.view).toBe('groupedList');

    expect(result.current.sort).toBe('name');
    act(() => {
      result.current.setSort('size');
    });
    expect(result.current.sort).toBe('size');

    expect(result.current.query).toBe('');
    act(() => {
      result.current.setQuery('test');
    });
    expect(result.current.query).toBe('test');
  });

  it('exposes a tool view that filters the catalog and can be cleared', () => {
    const models = [
      makeModelFile({ id: 'dup1', name: 'A', contentHash: 'h', importedAt: '2026-01-01T00:00:00.000Z' }),
      makeModelFile({ id: 'dup2', name: 'B', contentHash: 'h', importedAt: '2026-02-01T00:00:00.000Z' }),
      makeModelFile({ id: 'solo', name: 'C', contentHash: 'x' }),
    ];
    const { result } = renderHook(() => useCatalogFilters(models, []));
    expect(result.current.toolView).toBeNull();
    act(() => result.current.setToolView('duplicates'));
    expect(result.current.filtered.map((m) => m.id)).toEqual(['dup1', 'dup2']);
    act(() => result.current.setToolView(null));
    expect(result.current.filtered).toHaveLength(3);
  });

  it('freezes the "recent" ranking while active and refreshes it on re-activation', () => {
    const initial = [
      makeModelFile({ id: 'a', lastViewedAt: '2026-01-01T00:00:00.000Z' }),
      makeModelFile({ id: 'b', lastViewedAt: '2026-01-02T00:00:00.000Z' }),
    ];
    const { result, rerender } = renderHook(({ models }) => useCatalogFilters(models, []), {
      initialProps: { models: initial },
    });

    act(() => result.current.setToolView('recent'));
    expect(result.current.filtered.map((m) => m.id)).toEqual(['b', 'a']);

    // A model's lastViewedAt is updated live (as selectModel does on click),
    // making 'a' the newest by live data - the active view must not reorder.
    const updated = [
      { ...initial[0], lastViewedAt: '2026-01-03T00:00:00.000Z' },
      initial[1],
    ];
    rerender({ models: updated });
    expect(result.current.filtered.map((m) => m.id)).toEqual(['b', 'a']);

    // Re-activating the view (off, then on again) takes a fresh snapshot.
    act(() => result.current.setToolView(null));
    act(() => result.current.setToolView('recent'));
    expect(result.current.filtered.map((m) => m.id)).toEqual(['a', 'b']);
  });
});

it('resets direction only when the sort key changes', () => {
  const { result } = renderHook(() => useCatalogFilters([], []));
  expect(result.current.sortDirection).toBe('asc');
  for (const key of ['imported', 'modified', 'size', 'vol', 'viewed', 'printStatus', 'name'] as const) {
    act(() => result.current.setSort(key));
    expect(result.current.sortDirection).toBe((key === 'name' || key === 'printStatus') ? 'asc' : 'desc');
    act(() => result.current.setSortDirection((key === 'name' || key === 'printStatus') ? 'desc' : 'asc'));
    act(() => result.current.setSort(key));
    expect(result.current.sortDirection).toBe((key === 'name' || key === 'printStatus') ? 'desc' : 'asc');
    expect(result.current.sort).toBe(key);
  }
});

it('updates and clears usage filtering', () => {
  const models = [makeModelFile({ id: '1', license: 'personal' }), makeModelFile({ id: '2', license: null }), makeModelFile({ id: '3', license: 'free text' })];
  const { result } = renderHook(() => useCatalogFilters(models, []));
  act(() => result.current.setUsage('private'));
  expect(result.current.filtered.map(m => m.id)).toEqual(['1']);
  act(() => result.current.setUsage('unknown'));
  expect(result.current.filtered.map(m => m.id)).toEqual(['2', '3']);
  act(() => result.current.setUsage(null));
  expect(result.current.filtered).toHaveLength(3);
});

it('updates and clears print status without losing usage', () => {
  const models = [makeModelFile({ id: 'p', printStatus: 'printed', license: 'personal' }), makeModelFile({ id: 'u', printStatus: 'not_printed', license: 'personal' }), makeModelFile({ id: 'other', license: null })];
  const { result } = renderHook(() => useCatalogFilters(models, []));
  act(() => result.current.setUsage('private'));
  act(() => result.current.setPrintStatus('printed'));
  expect(result.current.filtered.map(m => m.id)).toEqual(['p']);
  act(() => result.current.setPrintStatus('not_printed'));
  expect(result.current.filtered.map(m => m.id)).toEqual(['u']);
  act(() => result.current.setPrintStatus(null));
  expect(result.current.filtered.map(m => m.id)).toEqual(['p', 'u']);
});

describe('print bed filter', () => {
  const models = [
    makeModelFile({ id: 'small', dimensionsMm: [50, 50, 50] }),
    makeModelFile({ id: 'wide', dimensionsMm: [300, 50, 50] }),
    makeModelFile({ id: 'unknown', dimensionsMm: null }),
  ];
  const mini = makePrinter({ id: 'mini', name: 'Mini', bedXMm: 180, bedYMm: 180, bedZMm: 180 });
  const big = makePrinter({ id: 'big', name: 'Big', bedXMm: 400, bedYMm: 400, bedZMm: 400 });

  it('starts with all models and filters by the chosen printer', () => {
    const { result } = renderHook(() => useCatalogFilters(models, [], 'de', [mini, big]));
    expect(result.current.fitPrinterId).toBeNull();
    expect(result.current.filtered).toHaveLength(3);
    act(() => result.current.setFitPrinterId('mini'));
    expect(result.current.filtered.map(m => m.id).sort()).toEqual(['small', 'unknown']);
    act(() => result.current.setFitPrinterId('big'));
    expect(result.current.filtered).toHaveLength(3);
    act(() => result.current.setFitPrinterId(null));
    expect(result.current.filtered).toHaveLength(3);
  });

  it('ignores a printer that was deleted or has no build volume', () => {
    const noBed = makePrinter({ id: 'nobed', bedXMm: null });
    const { result, rerender } = renderHook(({ printers }) => useCatalogFilters(models, [], 'de', printers), {
      initialProps: { printers: [mini] },
    });
    act(() => result.current.setFitPrinterId('mini'));
    expect(result.current.filtered).toHaveLength(2);
    rerender({ printers: [] });
    expect(result.current.fitPrinterId).toBeNull();
    expect(result.current.filtered).toHaveLength(3);
    rerender({ printers: [noBed] });
    act(() => result.current.setFitPrinterId('nobed'));
    expect(result.current.fitPrinterId).toBeNull();
    expect(result.current.filtered).toHaveLength(3);
  });
  describe('print data filter', () => {
    const a = makeModelFile({ id: 'a', name: 'A', printFacts: makePrintFacts({ layerHeightMm: 0.2 }), printDataState: 'sliced' });
    const b = makeModelFile({ id: 'b', name: 'B', printFacts: makePrintFacts({ layerHeightMm: 0.28 }), printDataState: 'sliced' });
    const c = makeModelFile({ id: 'c', name: 'C' });
    const d = makeModelFile({ id: 'd', name: 'D', printFacts: makePrintFacts({}), printDataState: 'no_slice_data' });
    const models = [a, b, c, d];

    it('starts empty and leaves the list alone', () => {
      const { result } = renderHook(() => useCatalogFilters(models, []));
      expect(result.current.printFilter).toEqual(EMPTY_FILTER);
      expect(result.current.filtered).toHaveLength(4);
      expect(result.current.printNotice).toEqual({ hidden: 0, shownUnknown: 0, unread: 0 });
    });

    it('keeps the matches, holds back the files without the value and counts the unread ones', () => {
      const { result } = renderHook(() => useCatalogFilters(models, []));
      act(() => result.current.setPrintFilter({ ...EMPTY_FILTER, layers: [0.2] }));
      expect(result.current.filtered.map(m => m.id)).toEqual(['a']);
      expect(result.current.printNotice).toEqual({ hidden: 2, shownUnknown: 0, unread: 1 });
    });

    it('shows the files without the value on request and keeps the sort order', () => {
      const { result } = renderHook(() => useCatalogFilters(models, []));
      act(() => result.current.setPrintFilter({ ...EMPTY_FILTER, layers: [0.2], includeUnknown: true }));
      expect(result.current.filtered.map(m => m.id)).toEqual(['a', 'c', 'd']);
      expect(result.current.printNotice).toEqual({ hidden: 0, shownUnknown: 2, unread: 0 });
      act(() => result.current.setSortDirection('desc'));
      expect(result.current.filtered.map(m => m.id)).toEqual(['d', 'c', 'a']);
    });

    it('combines with the other filters', () => {
      const { result } = renderHook(() => useCatalogFilters(models, []));
      act(() => result.current.setPrintFilter({ ...EMPTY_FILTER, layers: [0.2, 0.28] }));
      expect(result.current.filtered.map(m => m.id)).toEqual(['a', 'b']);
      act(() => result.current.setQuery('b'));
      expect(result.current.filtered.map(m => m.id)).toEqual(['b']);
      expect(result.current.printNotice.hidden).toBe(0);
    });
  });

  it('finds the files of a bundle by the bundle name only when the bundles are handed over', () => {
    const models = [makeModelFile({ id: '1', name: 'Left.stl' }), makeModelFile({ id: '2', name: 'Right.stl' }), makeModelFile({ id: '3', name: 'Other.stl' })];
    const bundles = [{
      id: 'b', name: 'Vase set', kind: 'parts' as const, createdAt: '2026-11-01T00:00:00Z',
      members: [{ fileId: '1', position: 0, label: null, inTrash: false }, { fileId: '2', position: 1, label: null, inTrash: false }],
    }];
    const withBundles = renderHook(() => useCatalogFilters(models, [], 'de', [], bundles));
    act(() => withBundles.result.current.setQuery('vase'));
    expect(withBundles.result.current.filtered.map(m => m.id)).toEqual(['1', '2']);
    const without = renderHook(() => useCatalogFilters(models, []));
    act(() => without.result.current.setQuery('vase'));
    expect(without.result.current.filtered).toEqual([]);
  });
});
