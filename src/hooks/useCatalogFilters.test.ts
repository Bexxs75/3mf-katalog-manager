import { describe, expect, it } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useCatalogFilters } from './useCatalogFilters';
import { makeModelFile } from '../test/factories';

describe('useCatalogFilters', () => {
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
    expect(result.current).toHaveProperty('activeCreator');
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
