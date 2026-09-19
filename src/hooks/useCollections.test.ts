import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useCollections } from './useCollections';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('useCollections', () => {
  it('loads collections on mount', async () => {
    vi.mocked(invoke).mockResolvedValue([{ id: 'c1', name: 'A', modelCount: 2 }]);
    const { result } = renderHook(() => useCollections());
    await waitFor(() => expect(result.current.collections).toHaveLength(1));
    expect(invoke).toHaveBeenCalledWith('list_collections');
  });

  it('setActiveCollection loads that collection\'s files', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_collections') return Promise.resolve([]);
      if (cmd === 'list_collection_files') return Promise.resolve([{ id: 'm1' }]);
      return Promise.resolve(undefined);
    });
    const { result } = renderHook(() => useCollections());
    act(() => result.current.setActiveCollection('c1'));
    await waitFor(() => expect(result.current.collectionModels).toEqual([{ id: 'm1' }]));
    expect(invoke).toHaveBeenCalledWith('list_collection_files', { collectionId: 'c1' });
  });

  it('setActiveCollection(null) clears collectionModels', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const { result } = renderHook(() => useCollections());
    act(() => result.current.setActiveCollection(null));
    expect(result.current.collectionModels).toEqual([]);
  });

  it('createCollection calls create_collection and refreshes the list', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const { result } = renderHook(() => useCollections());
    await act(async () => result.current.createCollection('New'));
    expect(invoke).toHaveBeenCalledWith('create_collection', { name: 'New' });
  });

  it('bulkAddToCollection adds files and refreshes the active collection', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_collections') return Promise.resolve([]);
      if (cmd === 'list_collection_files') return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    const { result } = renderHook(() => useCollections());
    act(() => result.current.setActiveCollection('c1'));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('list_collection_files', { collectionId: 'c1' }));
    vi.mocked(invoke).mockClear();
    await act(async () => result.current.bulkAddToCollection(['m1', 'm2'], 'c1'));
    expect(invoke).toHaveBeenCalledWith('add_files_to_collection', { collectionId: 'c1', fileIds: ['m1', 'm2'] });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('list_collection_files', { collectionId: 'c1' }));
  });
});
