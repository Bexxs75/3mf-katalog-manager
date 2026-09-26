import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useCatalogStore } from './useCatalogStore';
import { makeModelFile, makeModelFileSummary } from '../test/factories';
import * as filesApi from '../lib/api/files';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

// Individual filesApi functions are spies that pass through to the real
// implementation by default; tests make them fail on purpose.
vi.mock('../lib/api/files', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../lib/api/files')>();
  return Object.fromEntries(
    Object.entries(actual).map(([key, value]) => [key, typeof value === 'function' ? vi.fn(value) : value]),
  );
});

function createDeferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

// Projects full ModelFile fixtures onto summaries and also serves
// list_files_by_ids for lazy loading.
function mockInitialLoad(models = [makeModelFile({ id: 'm1' })]) {
  vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === 'list_file_summaries') return Promise.resolve(models.map((m) => makeModelFileSummary(m)));
    // Tags from the full fixtures so the merged state is correct.
    if (cmd === 'list_all_file_tags') {
      const byFile: Record<string, string[]> = {};
      for (const m of models) if (m.tags.length > 0) byFile[m.id] = m.tags;
      return Promise.resolve(byFile);
    }
    if (cmd === 'list_files_by_ids') {
      const ids = ((args as { ids?: string[] } | undefined)?.ids) ?? [];
      return Promise.resolve(models.filter((m) => ids.includes(m.id)));
    }
    if (cmd === 'list_folders') return Promise.resolve([]);
    if (cmd === 'list_tag_counts') return Promise.resolve([]);
    if (cmd === 'list_trash') return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
}

function callCount(cmd: string) {
  return vi.mocked(invoke).mock.calls.filter(([c]) => c === cmd).length;
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  // Only clear, not reset: the passthrough to the real implementation stays.
  vi.clearAllMocks();
});

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

  it('merges the bulk tags aggregate into summary-derived models so tag filtering works for never-opened models', async () => {
    // Summaries have no tags; without the merge via list_all_file_tags the
    // tag filter would find nothing.
    mockInitialLoad([
      makeModelFile({ id: 'm1', tags: ['vase'] }),
      makeModelFile({ id: 'm2', tags: ['red'] }),
    ]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(2));

    const m1 = result.current.models.find((m) => m.id === 'm1');
    const m2 = result.current.models.find((m) => m.id === 'm2');
    expect(m1?.tags).toEqual(['vase']);
    expect(m2?.tags).toEqual(['red']);

    const { filterAndSortModels } = await import('../lib/catalogFilters');
    const filtered = filterAndSortModels(result.current.models, [], {
      activeFolderId: 'all', activeTag: 'vase', query: '', sort: 'name',
    });
    expect(filtered.map((m) => m.id)).toEqual(['m1']);
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

  it('addTag stores a translated auto tag name as the canonical tag', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1', tags: [] })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));
    act(() => result.current.addTag('m1', 'Multipart'));
    expect(result.current.models[0].tags).toEqual(['mehrteilig']);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('add_tag', { fileId: 'm1', tag: 'mehrteilig' }));
  });

  it('addTag does nothing if the model already has the canonical tag', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1', tags: ['mehrteilig'] })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));
    act(() => result.current.addTag('m1', 'multipart'));
    expect(result.current.models[0].tags).toEqual(['mehrteilig']);
    expect(invoke).not.toHaveBeenCalledWith('add_tag', expect.anything());
  });

  it('renameFile updates the local name only after the backend call resolves', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1', name: 'alt.3mf' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));

    let resolveRename: () => void = () => {};
    vi.mocked(invoke).mockImplementationOnce(
      () => new Promise<void>((resolve) => { resolveRename = resolve; }),
    );

    let pending: Promise<void>;
    act(() => {
      pending = result.current.renameFile('m1', 'neu.3mf');
    });
    // Not optimistic: the name must not have changed yet
    // while the backend call is still running.
    expect(result.current.models[0].name).toBe('alt.3mf');

    await act(async () => {
      resolveRename();
      await pending;
    });
    expect(result.current.models[0].name).toBe('neu.3mf');
    expect(invoke).toHaveBeenCalledWith('rename_file', { fileId: 'm1', name: 'neu.3mf' });
  });

  it('renameFile rejects and leaves the local name unchanged when the backend call fails', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1', name: 'alt.3mf' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));

    vi.mocked(invoke).mockRejectedValueOnce(new Error('Zieldatei existiert bereits'));

    await expect(result.current.renameFile('m1', 'neu.3mf')).rejects.toThrow('Zieldatei existiert bereits');
    expect(result.current.models[0].name).toBe('alt.3mf');
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
    // delete_files may have silently skipped individual IDs - the backend
    // still reports both models here, local filtering would have wrongly
    // removed m1.
    act(() => result.current.refetchAfterPartialDelete(['m1']));
    await waitFor(() => expect(callCount('list_file_summaries')).toBe(2));
    expect(result.current.models.map((m) => m.id)).toEqual(['m1', 'm2']);
    expect(result.current.selectedId).toBeNull();
  });

  it('refetchAfterPartialDelete keeps the selection when the selected model was not affected', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1' }), makeModelFile({ id: 'm2' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(2));
    act(() => result.current.setSelectedId('m2'));
    act(() => result.current.refetchAfterPartialDelete(['m1']));
    await waitFor(() => expect(callCount('list_file_summaries')).toBe(2));
    expect(result.current.selectedId).toBe('m2');
  });

  it('mergeImported appends the imported models, selects the last one and refreshes side lists', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));
    const before = {
      folders: callCount('list_folders'),
      tags: callCount('list_tag_counts'),
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
    });
  });

  it('pendingSnapshotIds lists models without a render snapshot and skipSnapshot removes them', async () => {
    // The summary already carries the snapshot, m2 doesn't need a new one.
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

  it('Finding 1 + Bugfix 2026-09-20: a model with a saved snapshot loaded only via the summary path is never pending and shows its snapshot', async () => {
    // m1 already has a snapshot: the image must come from the summary right away,
    // and m1 must not be in pendingSnapshotIds.
    mockInitialLoad([makeModelFile({ id: 'm1', renderSnapshotImage: 'data:image/png;base64,yy' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));

    expect(result.current.models[0].renderSnapshotImage).toBe('data:image/png;base64,yy');
    expect(result.current.pendingSnapshotIds).toEqual([]);
  });

  it('Bugfix 2026-09-20: creator is populated straight from the summary, not hardcoded to null', async () => {
    // creator must already come from the summary, otherwise searching by creator finds nothing.
    mockInitialLoad([makeModelFile({ id: 'm1', creator: 'CarlFromUp' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));

    expect(result.current.models[0].creator).toBe('CarlFromUp');
  });

  it('maps lastViewedAt and contentHash from the catalog summaries', async () => {
    mockInitialLoad([
      makeModelFile({ id: 'm1', lastViewedAt: '2026-09-24T08:00:00+00:00', contentHash: 'abc' }),
    ]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));
    expect(result.current.models[0].lastViewedAt).toBe('2026-09-24T08:00:00+00:00');
    expect(result.current.models[0].contentHash).toBe('abc');
  });

  it('addToQueue stores the returned position', async () => {
    mockInitialLoad([makeModelFile({ id: 'm1', queuePosition: null })]);
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'add_to_queue') return Promise.resolve(1);
      return mockInitialLoadResolver(cmd);
    });
    function mockInitialLoadResolver(cmd: string) {
      if (cmd === 'list_file_summaries') {
        return Promise.resolve([makeModelFileSummary({ id: 'm1', queuePosition: null })]);
      }
      return Promise.resolve([]);
    }
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));
    await act(async () => result.current.addToQueue('m1'));
    expect(result.current.models[0].queuePosition).toBe(1);
  });

  // Resync after ALL overlapping calls for the same field+ID have settled
  // (see comment in useCatalogStore).
  describe('M-03: mutation resync on settle', () => {
    it('resyncs the favorite flag from the backend if the call fails', async () => {
      vi.mocked(filesApi.setFavorite).mockRejectedValueOnce(new Error('db locked'));
      mockInitialLoad([makeModelFile({ id: 'm1', favorite: false })]);
      const { result } = renderHook(() => useCatalogStore());
      await waitFor(() => expect(result.current.models).toHaveLength(1));
      const modelId = result.current.models[0].id;
      const initialFavorite = result.current.models[0].favorite;
      // The backend keeps the original value - listFilesByIds() returns
      // it unchanged on resync.
      vi.mocked(filesApi.listFilesByIds).mockResolvedValueOnce([
        { ...result.current.models[0], favorite: initialFavorite },
      ]);

      act(() => {
        result.current.toggleFavorite(modelId);
      });
      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(!initialFavorite);

      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(initialFavorite);
      expect(filesApi.listFilesByIds).toHaveBeenCalledWith([modelId]);
    });

    it('resyncs the print status from the backend if the call fails', async () => {
      vi.mocked(filesApi.setPrintStatus).mockRejectedValueOnce(new Error('db locked'));
      mockInitialLoad([makeModelFile({ id: 'm1', printStatus: 'not_printed' })]);
      const { result } = renderHook(() => useCatalogStore());
      await waitFor(() => expect(result.current.models).toHaveLength(1));
      const modelId = result.current.models[0].id;
      const initial = result.current.models[0];
      vi.mocked(filesApi.listFilesByIds).mockResolvedValueOnce([{ ...initial, printStatus: 'not_printed' }]);

      act(() => {
        result.current.togglePrintStatus(modelId);
      });
      expect(result.current.models.find((m) => m.id === modelId)?.printStatus).toBe('printed');

      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(result.current.models.find((m) => m.id === modelId)?.printStatus).toBe('not_printed');
      expect(filesApi.listFilesByIds).toHaveBeenCalledWith([modelId]);
    });

    it('resyncs tags from the backend if addTag fails', async () => {
      vi.mocked(filesApi.addTag).mockRejectedValueOnce(new Error('db locked'));
      mockInitialLoad([makeModelFile({ id: 'm1', tags: [] })]);
      const { result } = renderHook(() => useCatalogStore());
      await waitFor(() => expect(result.current.models).toHaveLength(1));
      const modelId = result.current.models[0].id;
      const initial = result.current.models[0];
      vi.mocked(filesApi.listFilesByIds).mockResolvedValueOnce([{ ...initial, tags: [] }]);

      act(() => {
        result.current.addTag(modelId, 'red');
      });
      expect(result.current.models.find((m) => m.id === modelId)?.tags).toEqual(['red']);

      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(result.current.models.find((m) => m.id === modelId)?.tags).toEqual([]);
      expect(filesApi.listFilesByIds).toHaveBeenCalledWith([modelId]);
    });

    it('resyncs queue order from the backend when reorderQueue fails', async () => {
      vi.mocked(filesApi.reorderQueue).mockRejectedValueOnce(new Error('db locked'));
      mockInitialLoad([
        makeModelFile({ id: 'm1', queuePosition: 1 }),
        makeModelFile({ id: 'm2', queuePosition: 2 }),
      ]);
      const { result } = renderHook(() => useCatalogStore());
      await waitFor(() => expect(result.current.models).toHaveLength(2));

      act(() => {
        result.current.reorderQueue(['m2', 'm1']);
      });
      expect(result.current.models.map((m) => m.queuePosition)).toEqual([2, 1]);

      await waitFor(() => {
        // Once on load, once for the resync.
        expect(callCount('list_file_summaries')).toBe(2);
      });
    });

    it('calls the backend in user-triggered order for sequential successful toggles', async () => {
      vi.mocked(filesApi.setFavorite).mockResolvedValue(undefined);
      mockInitialLoad([makeModelFile({ id: 'm1', favorite: false })]);
      const { result } = renderHook(() => useCatalogStore());
      await waitFor(() => expect(result.current.models).toHaveLength(1));
      const modelId = result.current.models[0].id;
      const initial = result.current.models[0];
      // Mock resync values one after another per click: a resync can already
      // settle in the same act(), a constant value would be wrong then.
      vi.mocked(filesApi.listFilesByIds)
        .mockResolvedValueOnce([{ ...initial, favorite: true }]) // after A
        .mockResolvedValueOnce([{ ...initial, favorite: false }]) // after B
        .mockResolvedValueOnce([{ ...initial, favorite: true }]); // after C

      // Each click in its own act() - a real re-render in between,
      // just like three user clicks separated in time.
      await act(async () => {
        result.current.toggleFavorite(modelId); // A
      });
      await act(async () => {
        result.current.toggleFavorite(modelId); // B
      });
      await act(async () => {
        result.current.toggleFavorite(modelId); // C
      });
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });

      const calls = vi.mocked(filesApi.setFavorite).mock.calls;
      expect(calls.map((c) => c[0])).toEqual([modelId, modelId, modelId]);
      expect(calls.map((c) => c[1])).toEqual([true, false, true]); // A, B, C alternieren korrekt
    });

    it('ends up matching the backend when two overlapping calls both fail', async () => {
      const deferredA = createDeferred<void>();
      const deferredB = createDeferred<void>();
      vi.mocked(filesApi.setFavorite)
        .mockReturnValueOnce(deferredA.promise as Promise<void>)
        .mockReturnValueOnce(deferredB.promise as Promise<void>);

      mockInitialLoad([makeModelFile({ id: 'm1', favorite: false })]);
      const { result } = renderHook(() => useCatalogStore());
      await waitFor(() => expect(result.current.models).toHaveLength(1));
      const modelId = result.current.models[0].id;
      const backendValue = result.current.models[0].favorite; // the backend NEVER changes successfully in this test.
      vi.mocked(filesApi.listFilesByIds).mockResolvedValue([{ ...result.current.models[0], favorite: backendValue }]);

      await act(async () => {
        result.current.toggleFavorite(modelId); // A: pending
      });
      await act(async () => {
        result.current.toggleFavorite(modelId); // B: pending, newer generation
      });

      // B (newest call) fails first.
      await act(async () => {
        deferredB.reject(new Error('db locked'));
        await Promise.resolve();
        await Promise.resolve();
      });
      // A (older call) ALSO fails AFTERWARDS - a plain generation counter
      // would ignore this error and wrongly leave the UI in a successful
      // state the backend never had.
      await act(async () => {
        deferredA.reject(new Error('db locked'));
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(backendValue);
    });

    it('keeps the latest successful value when an older overlapping call fails later', async () => {
      const deferredA = createDeferred<void>();
      vi.mocked(filesApi.setFavorite)
        .mockReturnValueOnce(deferredA.promise as Promise<void>) // A: stays pending, fails later
        .mockResolvedValueOnce(undefined); // C: erfolgreich

      mockInitialLoad([makeModelFile({ id: 'm1', favorite: false })]);
      const { result } = renderHook(() => useCatalogStore());
      await waitFor(() => expect(result.current.models).toHaveLength(1));
      const modelId = result.current.models[0].id;
      vi.mocked(filesApi.listFilesByIds).mockResolvedValue([{ ...result.current.models[0], favorite: true }]);

      await act(async () => {
        result.current.toggleFavorite(modelId); // A: pending (false -> true)
      });
      await act(async () => {
        result.current.toggleFavorite(modelId); // C: succeeds (true -> false -> ... see assignment above, final value determined by resync)
      });
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });

      // A now fails late - C has already completed successfully.
      await act(async () => {
        deferredA.reject(new Error('db locked'));
        await Promise.resolve();
        await Promise.resolve();
      });

      // In the end the backend state always determines the UI.
      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(true);
    });

    it('does not let a stale, slow resync overwrite a newer resync that already completed', async () => {
      // Race: resync R1 after A hangs, meanwhile B including resync R2
      // runs to completion. The late R1 must not overwrite R2 (epoch).
      vi.mocked(filesApi.setFavorite).mockResolvedValue(undefined);
      const deferredResyncR1 = createDeferred<import('../types').ModelFile[]>();
      mockInitialLoad([makeModelFile({ id: 'm1', favorite: false })]);
      const { result } = renderHook(() => useCatalogStore());
      await waitFor(() => expect(result.current.models).toHaveLength(1));
      const modelId = result.current.models[0].id;
      const initial = result.current.models[0];

      // R1 (resync after A) hangs for now.
      vi.mocked(filesApi.listFilesByIds).mockReturnValueOnce(deferredResyncR1.promise);

      await act(async () => {
        result.current.toggleFavorite(modelId); // A: triggers setFavorite and then R1 (R1 hangs)
        await Promise.resolve();
        await Promise.resolve();
      });

      // R2 (resync after B) delivers the new, correct state right away.
      vi.mocked(filesApi.listFilesByIds).mockResolvedValueOnce([{ ...initial, favorite: false }]);
      await act(async () => {
        result.current.toggleFavorite(modelId); // B: its own, fully completed mutation incl. R2
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(false);

      // R1 finally delivers its LONG OUTDATED state - must NOT overwrite
      // the correct state already set by R2.
      await act(async () => {
        deferredResyncR1.resolve([{ ...initial, favorite: true }]);
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(false);
    });

    it('Finding 3: a stale, slow ensureFullModel fetch does not overwrite a newer mutation resync', async () => {
      // ensureFullModel() hangs, meanwhile a mutation including resync runs
      // through. The late fetch result must not undo it.
      vi.mocked(filesApi.setFavorite).mockResolvedValue(undefined);
      mockInitialLoad([makeModelFile({ id: 'm1', favorite: false })]);
      const { result } = renderHook(() => useCatalogStore());
      await waitFor(() => expect(result.current.models).toHaveLength(1));
      const modelId = result.current.models[0].id;
      const initial = result.current.models[0];

      const deferredFetchA = createDeferred<import('../types').ModelFile[]>();
      // 1st call of listFilesByIds: ensureFullModel's fetch A - hangs.
      vi.mocked(filesApi.listFilesByIds).mockReturnValueOnce(deferredFetchA.promise);

      await act(async () => {
        result.current.ensureFullModel(modelId); // startet Fetch A (haengt)
        await Promise.resolve();
      });

      // 2nd call of listFilesByIds: the M-03 resync of toggleFavorite -
      // delivers the new, correct state right away (favorite: true).
      vi.mocked(filesApi.listFilesByIds).mockResolvedValueOnce([{ ...initial, favorite: true }]);
      await act(async () => {
        result.current.toggleFavorite(modelId); // completes fully including its own resync
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(true);

      // Fetch A finally delivers its LONG OUTDATED state
      // (favorite: false, as before the mutation) - must NOT overwrite the
      // already correctly resynced state.
      await act(async () => {
        deferredFetchA.resolve([{ ...initial, favorite: false }]);
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(true);
    });
  });
});
