import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useCatalogStore } from './useCatalogStore';
import { makeModelFile } from '../test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

function mockInitialLoad(models = [makeModelFile({ id: 'm1' })]) {
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === 'list_files') return Promise.resolve(models);
    if (cmd === 'list_folders') return Promise.resolve([]);
    if (cmd === 'list_tag_counts') return Promise.resolve([]);
    if (cmd === 'list_creators') return Promise.resolve([]);
    if (cmd === 'list_saved_filters') return Promise.resolve([]);
    if (cmd === 'list_trash') return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
}

function callCount(cmd: string) {
  return vi.mocked(invoke).mock.calls.filter(([c]) => c === cmd).length;
}

beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('useCatalogStore', () => {
  it('loads models on mount and selects the first one', async () => {
    mockInitialLoad();
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));
    expect(result.current.selectedId).toBe('m1');
  });

  it('togglePrintStatus flips status and clears queuePosition when marking printed', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1', printStatus: 'not_printed', queuePosition: 2 })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));
    act(() => result.current.togglePrintStatus('m1'));
    expect(result.current.models[0].printStatus).toBe('printed');
    expect(result.current.models[0].queuePosition).toBeNull();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_print_status', { fileId: 'm1', status: 'printed' }),
    );
  });

  it('toggleFavorite flips favorite and persists it', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1', favorite: false })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));
    act(() => result.current.toggleFavorite('m1'));
    expect(result.current.models[0].favorite).toBe(true);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('set_favorite', { fileId: 'm1', favorite: true }));
  });

  it('addTag appends locally then persists', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1', tags: [] })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));
    act(() => result.current.addTag('m1', 'red'));
    expect(result.current.models[0].tags).toEqual(['red']);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('add_tag', { fileId: 'm1', tag: 'red' }));
  });

  it('deleteModel removes it locally, clears selection, and refreshes side lists', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.selectedId).toBe('m1'));
    await act(async () => result.current.deleteModel('m1'));
    expect(result.current.models).toEqual([]);
    expect(result.current.selectedId).toBeNull();
    expect(invoke).toHaveBeenCalledWith('delete_file', { fileId: 'm1' });
  });

  it('applyLocalDeletion filters models and clears selection when affected', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1' }), makeModelFile({ id: 'm2' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(2));
    act(() => result.current.setSelectedId('m1'));
    act(() => result.current.applyLocalDeletion(['m1']));
    expect(result.current.models.map((m) => m.id)).toEqual(['m2']);
    expect(result.current.selectedId).toBeNull();
  });

  it('reorderQueue only sends updates for models whose position actually changed', async () => {
    mockInitialLoad([
      makeModelFile({ id: 'm1', queuePosition: 1 }),
      makeModelFile({ id: 'm2', queuePosition: 2 }),
      makeModelFile({ id: 'm3', queuePosition: 3 }),
    ]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(3));
    act(() => result.current.reorderQueue(['m1', 'm3', 'm2']));
    expect(result.current.models.map((m) => m.queuePosition)).toEqual([1, 3, 2]);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('reorder_queue', {
        updates: [
          { fileId: 'm3', position: 2 },
          { fileId: 'm2', position: 3 },
        ],
      }),
    );
  });

  it('reorderQueue sends nothing when the order is unchanged', async () => {
    mockInitialLoad([
      makeModelFile({ id: 'm1', queuePosition: 1 }),
      makeModelFile({ id: 'm2', queuePosition: 2 }),
    ]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(2));
    act(() => result.current.reorderQueue(['m1', 'm2']));
    expect(callCount('reorder_queue')).toBe(0);
  });

  it('refetchAfterPartialDelete reloads the list from the backend instead of filtering locally', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1' }), makeModelFile({ id: 'm2' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(2));
    act(() => result.current.setSelectedId('m1'));
    // delete_files kann einzelne IDs stillschweigend uebersprungen haben - das
    // Backend meldet hier weiterhin beide Modelle, lokales Filtern haette m1
    // faelschlich entfernt.
    act(() => result.current.refetchAfterPartialDelete(['m1']));
    await waitFor(() => expect(callCount('list_files')).toBe(2));
    expect(result.current.models.map((m) => m.id)).toEqual(['m1', 'm2']);
    expect(result.current.selectedId).toBeNull();
  });

  it('refetchAfterPartialDelete keeps the selection when the selected model was not affected', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1' }), makeModelFile({ id: 'm2' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(2));
    act(() => result.current.setSelectedId('m2'));
    act(() => result.current.refetchAfterPartialDelete(['m1']));
    await waitFor(() => expect(callCount('list_files')).toBe(2));
    expect(result.current.selectedId).toBe('m2');
  });

  it('mergeImported appends the imported models, selects the last one and refreshes side lists', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));
    const before = {
      folders: callCount('list_folders'),
      tags: callCount('list_tag_counts'),
      creators: callCount('list_creators'),
    };
    act(() =>
      result.current.mergeImported({
        imported: [makeModelFile({ id: 'm2' }), makeModelFile({ id: 'm3' })],
        duplicateCount: 0,
      }),
    );
    expect(result.current.models.map((m) => m.id)).toEqual(['m1', 'm2', 'm3']);
    expect(result.current.selectedId).toBe('m3');
    await waitFor(() => {
      expect(callCount('list_folders')).toBe(before.folders + 1);
      expect(callCount('list_tag_counts')).toBe(before.tags + 1);
      expect(callCount('list_creators')).toBe(before.creators + 1);
    });
  });

  it('pendingSnapshotIds lists models without a render snapshot and skipSnapshot removes them', async () => {
    mockInitialLoad([
      makeModelFile({ id: 'm1', renderSnapshotImage: null }),
      makeModelFile({ id: 'm2', renderSnapshotImage: 'data:image/png;base64,xx' }),
      makeModelFile({ id: 'm3', renderSnapshotImage: null }),
    ]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(3));
    expect(result.current.pendingSnapshotIds).toEqual(['m1', 'm3']);
    act(() => result.current.skipSnapshot('m1'));
    expect(result.current.pendingSnapshotIds).toEqual(['m3']);
  });

  it('addToQueue stores the returned position', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1', queuePosition: null })]);
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'add_to_queue') return Promise.resolve(1);
      return mockInitialLoadResolver(cmd);
    });
    function mockInitialLoadResolver(cmd: string) {
      if (cmd === 'list_files') return Promise.resolve([makeModelFile({ id: 'm1', queuePosition: null })]);
      return Promise.resolve([]);
    }
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));
    await act(async () => result.current.addToQueue('m1'));
    expect(result.current.models[0].queuePosition).toBe(1);
  });
});
