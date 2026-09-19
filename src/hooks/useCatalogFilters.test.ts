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
      result.current.setView('list');
    });
    expect(result.current.view).toBe('list');

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
});
