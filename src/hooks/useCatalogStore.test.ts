import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useCatalogStore } from './useCatalogStore';
import { makeModelFile, makeModelFileSummary } from '../test/factories';
import * as filesApi from '../lib/api/files';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

// M-03: die Resync-Tests unten spionieren einzelne filesApi-Funktionen
// (setFavorite/setPrintStatus/listFilesByIds/...) direkt an, um sie
// gezielt fehlschlagen zu lassen bzw. um ihre Aufrufreihenfolge zu
// pruefen - der Rest der Datei nutzt weiterhin ausschliesslich das
// bestehende invoke-Mocking (mockInitialLoad), da die gewrappten
// vi.fn()-Implementierungen standardmaessig einfach an die echte
// invoke()-basierte Implementierung durchreichen.
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

// Seit Finding M-01 laedt die Katalog-Uebersicht list_file_summaries statt
// list_files - die vollen ModelFile-Fixtures aus makeModelFile() werden hier
// auf die schlanke Summary-Form projiziert, damit bestehende Test-Setups
// (die volle ModelFile-Objekte als Ausgangsdaten formulieren) unveraendert
// bleiben koennen. list_files_by_ids wird ebenfalls bedient, falls ein Test
// das Nachladen der vollen Daten (ensureFullModel/selectModel) ausloest.
function mockInitialLoad(models = [makeModelFile({ id: 'm1' })]) {
  vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === 'list_file_summaries') return Promise.resolve(models.map((m) => makeModelFileSummary(m)));
    // Nachtrag zu Finding M-01: list_all_file_tags liefert alle Datei->Tag-
    // Zuordnungen in einer Abfrage - hier aus den vollen Fixtures abgeleitet,
    // damit bestehende Tests, die `tags` auf makeModelFile() setzen,
    // weiterhin den erwarteten gemergten Zustand nach dem initialen Laden sehen.
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
    if (cmd === 'list_creators') return Promise.resolve([]);
    if (cmd === 'list_saved_filters') return Promise.resolve([]);
    if (cmd === 'list_trash') return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
}

function callCount(cmd: string) {
  return vi.mocked(invoke).mock.calls.filter(([c]) => c === cmd).length;
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  // Nur mockClear (nicht mockReset) fuer filesApi: der Standard-Passthrough
  // zur echten, invoke()-basierten Implementierung (siehe vi.mock-Factory
  // oben) soll erhalten bleiben - nur die Aufrufhistorie/etwaige
  // Once-Ueberschreibungen aus dem vorherigen Test sollen verschwinden.
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
    // Regressionstest fuer den Sidebar-Tag-Filter-Fund (Nachtrag zu Finding
    // M-01): list_file_summaries liefert selbst keine Tags, ohne den Merge
    // ueber list_all_file_tags waeren m1.tags/m2.tags hier beide [] und
    // filterAndSortModels(activeTag: 'vase') haette faelschlich nichts
    // gefunden, obwohl m1 nie einzeln ausgewaehlt/geoeffnet wurde.
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
      activeFolderId: 'all', activeTag: 'vase', activeCreator: null, query: '', sort: 'name',
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
    // Nicht optimistisch: der Name darf sich noch nicht geaendert haben,
    // solange der Backend-Aufruf noch laeuft.
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
    // delete_files kann einzelne IDs stillschweigend uebersprungen haben - das
    // Backend meldet hier weiterhin beide Modelle, lokales Filtern haette m1
    // faelschlich entfernt.
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
    // list_file_summaries traegt seit dem Bugfix vom 2026-09-20 den echten
    // renderSnapshotImage-Blob mit - m2 gilt deshalb von Anfang an korrekt
    // als "hat bereits einen Snapshot", ganz ohne dass zuerst selectModel()
    // die vollen Daten nachladen muesste.
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
    // Doppelte Regression, beide bereits behoben: (1, Finding 1) vor jenem
    // Fix hardcodete summaryToModelFile() renderSnapshotImage IMMER auf
    // null, wodurch pendingSnapshotIds jedes frisch geladene Modell als
    // "braucht Snapshot" wertete, unabhaengig vom tatsaechlichen DB-Stand.
    // (2, 2026-09-20) list_file_summaries selbst lieferte den Blob dann zwar
    // korrekt NICHT als "braucht Snapshot", aber auch nie den Blob selbst,
    // wodurch das Grid einen laengst gerenderten Snapshot nicht anzeigte,
    // bevor ensureFullModel() lief. m1 hat hier bereits einen Snapshot und
    // muss deshalb sofort (schon aus der Summary, vor jedem ensureFullModel)
    // sowohl sein Bild zeigen als auch aus pendingSnapshotIds fernbleiben.
    mockInitialLoad([makeModelFile({ id: 'm1', renderSnapshotImage: 'data:image/png;base64,yy' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));

    expect(result.current.models[0].renderSnapshotImage).toBe('data:image/png;base64,yy');
    expect(result.current.pendingSnapshotIds).toEqual([]);
  });

  it('Bugfix 2026-09-20: creator is populated straight from the summary, not hardcoded to null', async () => {
    // Derselbe Fehlerklasse wie oben: summaryToModelFile() hardcodete creator
    // bislang auf null, wodurch der Sidebar-/Suchfilter nach Creator fuer
    // jedes nur per Summary geladene Modell (also den gesamten Katalog vor
    // dem ersten ensureFullModel()) ins Leere lief.
    mockInitialLoad([makeModelFile({ id: 'm1', creator: 'CarlFromUp' })]);
    const { result } = renderHook(() => useCatalogStore());
    await waitFor(() => expect(result.current.models).toHaveLength(1));

    expect(result.current.models[0].creator).toBe('CarlFromUp');
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

  // M-03: Resync betroffener optimistischer Mutationen nach Abschluss ALLER
  // ueberlappenden Backend-Aufrufe fuer dasselbe Feld+ID (siehe Task-8-Brief
  // fuer die Begruendung, warum weder ein simpler "previous"-Rollback noch
  // ein reiner Generation-Zaehler ausreicht).
  describe('M-03: mutation resync on settle', () => {
    it('resyncs the favorite flag from the backend if the call fails', async () => {
      vi.mocked(filesApi.setFavorite).mockRejectedValueOnce(new Error('db locked'));
      mockInitialLoad([makeModelFile({ id: 'm1', favorite: false })]);
      const { result } = renderHook(() => useCatalogStore());
      await waitFor(() => expect(result.current.models).toHaveLength(1));
      const modelId = result.current.models[0].id;
      const initialFavorite = result.current.models[0].favorite;
      // Backend behaelt den urspruenglichen Wert - listFilesByIds() liefert
      // ihn beim Resync unveraendert zurueck.
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
        // Nach dem fehlgeschlagenen reorder_queue wird die gesamte Liste
        // (inkl. Reihenfolge) per refreshFiles() aus dem Backend
        // nachgeladen - list_file_summaries wurde dafuer ein zweites Mal
        // aufgerufen (einmal beim initialen Mount, einmal fuer den Resync).
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
      // Korrektur nach siebter Review-Runde: listFilesByIds() SEQUENZIELL mit
      // dem jeweils nach diesem Klick erwarteten Backend-Wert mocken (statt
      // dauerhaft mit favorite:true) - da setFavorite() hier sofort aufloest,
      // koennte ein Resync bereits INNERHALB desselben act()-Blocks
      // abschliessen und den `models`-Snapshot fuer den naechsten Klick
      // veraendern. Ein konstanter Resync-Wert (true) wuerde deshalb nach
      // Klick B (der Backend-Wert waere dann false) einen falschen Zustand
      // einspielen, wodurch Klick C wieder mit next=false statt next=true
      // aufgerufen wuerde und die erwartete Abfolge [true, false, true] nicht
      // mehr stimmt. Die sequenzielle Mockierung macht den Test unabhaengig
      // von der exakten Mikrotask-Interleaving-Reihenfolge zwischen den
      // act()-Bloecken.
      vi.mocked(filesApi.listFilesByIds)
        .mockResolvedValueOnce([{ ...initial, favorite: true }]) // nach A
        .mockResolvedValueOnce([{ ...initial, favorite: false }]) // nach B
        .mockResolvedValueOnce([{ ...initial, favorite: true }]); // nach C

      // Jeder Klick in einem eigenen act() - echter Re-Render dazwischen,
      // genau wie bei drei zeitlich getrennten Nutzerklicks.
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
      const backendValue = result.current.models[0].favorite; // Backend aendert sich in diesem Test NIE erfolgreich.
      vi.mocked(filesApi.listFilesByIds).mockResolvedValue([{ ...result.current.models[0], favorite: backendValue }]);

      await act(async () => {
        result.current.toggleFavorite(modelId); // A: pending
      });
      await act(async () => {
        result.current.toggleFavorite(modelId); // B: pending, neuere Generation
      });

      // B (neuester Aufruf) schlaegt zuerst fehl.
      await act(async () => {
        deferredB.reject(new Error('db locked'));
        await Promise.resolve();
        await Promise.resolve();
      });
      // A (aelterer Aufruf) schlaegt DANACH ebenfalls fehl - ein reiner
      // Generation-Zaehler wuerde diesen Fehler ignorieren und die UI faelschlich
      // auf einem erfolgreichen Zustand belassen, den das Backend nie hatte.
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
        .mockReturnValueOnce(deferredA.promise as Promise<void>) // A: bleibt pending, schlaegt spaeter fehl
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
        result.current.toggleFavorite(modelId); // C: erfolgreich (true -> false -> ... siehe Zuweisung oben, Endwert durch Resync bestimmt)
      });
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });

      // A schlaegt jetzt verspaetet fehl - C ist bereits erfolgreich durchgelaufen.
      await act(async () => {
        deferredA.reject(new Error('db locked'));
        await Promise.resolve();
        await Promise.resolve();
      });

      // Der Resync liefert den vom Mock vorgegebenen kanonischen Wert (true) -
      // das ist der Punkt: unabhaengig vom Erfolg/Fehler-Muster einzelner
      // Aufrufe bestimmt am Ende IMMER der Backend-Stand die UI, nicht ein
      // lokal nachgebildeter "letzter gueltiger Wert".
      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(true);
    });

    it('does not let a stale, slow resync overwrite a newer resync that already completed', async () => {
      // Deckt die Resync-Race ab (siebte Review-Runde): A endet -> Resync R1
      // startet (bleibt hier absichtlich haengen); WAEHREND R1 noch laeuft,
      // startet und beendet B vollstaendig eine eigene Mutation, deren Resync
      // R2 sofort abschliesst und den neuen, korrekten Zustand uebernimmt;
      // ERST DANACH liefert das verzoegerte R1 seinen (jetzt veralteten)
      // Zustand. Ein reiner "pendingMutationCounts[key] === undefined"-Check
      // wuerde das nicht erkennen, da nach B's Abschluss der Zaehler erneut
      // undefined ist - der Mutation-Epoch-Zaehler verhindert die
      // Ueberschreibung durch R1.
      vi.mocked(filesApi.setFavorite).mockResolvedValue(undefined);
      const deferredResyncR1 = createDeferred<import('../types').ModelFile[]>();
      mockInitialLoad([makeModelFile({ id: 'm1', favorite: false })]);
      const { result } = renderHook(() => useCatalogStore());
      await waitFor(() => expect(result.current.models).toHaveLength(1));
      const modelId = result.current.models[0].id;
      const initial = result.current.models[0];

      // R1 (Resync nach A) bleibt zunaechst haengen.
      vi.mocked(filesApi.listFilesByIds).mockReturnValueOnce(deferredResyncR1.promise);

      await act(async () => {
        result.current.toggleFavorite(modelId); // A: loest setFavorite + danach R1 aus (R1 haengt)
        await Promise.resolve();
        await Promise.resolve();
      });

      // R2 (Resync nach B) liefert sofort den neuen, korrekten Zustand.
      vi.mocked(filesApi.listFilesByIds).mockResolvedValueOnce([{ ...initial, favorite: false }]);
      await act(async () => {
        result.current.toggleFavorite(modelId); // B: eigene, vollstaendig abgeschlossene Mutation inkl. R2
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(false);

      // R1 liefert jetzt endlich seinen LAENGST VERALTETEN Zustand - darf den
      // bereits von R2 gesetzten korrekten Zustand NICHT ueberschreiben.
      await act(async () => {
        deferredResyncR1.resolve([{ ...initial, favorite: true }]);
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(false);
    });

    it('Finding 3: a stale, slow ensureFullModel fetch does not overwrite a newer mutation resync', async () => {
      // Szenario aus dem Abschluss-Review: ensureFullModel() (Fetch A, bleibt
      // hier haengen) wird VOR einer Mutation gestartet; die Mutation
      // (toggleFavorite) schliesst inklusive ihres eigenen M-03-Resyncs
      // vollstaendig ab, WAEHREND Fetch A noch laeuft; erst DANACH liefert
      // Fetch A sein (jetzt veraltetes) Ergebnis mit dem ALTEN favorite-Wert.
      // Vor dem Fix ersetzte ensureFullModel() den kompletten Datensatz
      // unbedingt, wodurch die laengst korrekt resyncte Mutation wieder
      // stillschweigend rueckgaengig gemacht wurde.
      vi.mocked(filesApi.setFavorite).mockResolvedValue(undefined);
      mockInitialLoad([makeModelFile({ id: 'm1', favorite: false })]);
      const { result } = renderHook(() => useCatalogStore());
      await waitFor(() => expect(result.current.models).toHaveLength(1));
      const modelId = result.current.models[0].id;
      const initial = result.current.models[0];

      const deferredFetchA = createDeferred<import('../types').ModelFile[]>();
      // 1. Aufruf von listFilesByIds: ensureFullModel's Fetch A - bleibt haengen.
      vi.mocked(filesApi.listFilesByIds).mockReturnValueOnce(deferredFetchA.promise);

      await act(async () => {
        result.current.ensureFullModel(modelId); // startet Fetch A (haengt)
        await Promise.resolve();
      });

      // 2. Aufruf von listFilesByIds: der M-03-Resync von toggleFavorite -
      // liefert sofort den neuen, korrekten Zustand (favorite: true).
      vi.mocked(filesApi.listFilesByIds).mockResolvedValueOnce([{ ...initial, favorite: true }]);
      await act(async () => {
        result.current.toggleFavorite(modelId); // schliesst inkl. eigenem Resync vollstaendig ab
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(true);

      // Fetch A liefert jetzt endlich seinen LAENGST VERALTETEN Zustand
      // (favorite: false, wie vor der Mutation) - darf den bereits korrekt
      // resyncten Zustand NICHT ueberschreiben.
      await act(async () => {
        deferredFetchA.resolve([{ ...initial, favorite: false }]);
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(result.current.models.find((m) => m.id === modelId)?.favorite).toBe(true);
    });
  });
});
