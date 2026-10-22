import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useState } from 'react';
import { useBulkSelection } from './useBulkSelection';
import { makeModelFile } from '../test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

function setup(models = [makeModelFile({ id: 'm1' }), makeModelFile({ id: 'm2' })]) {
  const setModels = vi.fn();
  const refreshFolders = vi.fn();
  const refreshTags = vi.fn();
  const refreshTrash = vi.fn();
  const bulkAddToCollection = vi.fn().mockResolvedValue(undefined);
  const bulkRemoveFromCollection = vi.fn().mockResolvedValue(undefined);
  const hook = renderHook(() =>
    useBulkSelection({
      models, setModels, refreshFolders, refreshTags, refreshTrash, refetchAfterPartialDelete: vi.fn(),
      bulkAddToCollection, bulkRemoveFromCollection,
    }),
  );
  return { ...hook, setModels, refreshFolders, refreshTags, refreshTrash, bulkAddToCollection, bulkRemoveFromCollection };
}

describe('useBulkSelection', () => {
  it.each([
    ['setAddToCollectionMenuOpen', 'setAddTagMenuOpen', 'addTagMenuOpen'],
    ['setRemoveTagMenuOpen', 'setAddTagMenuOpen', 'addTagMenuOpen'],
    ['setAddTagMenuOpen', 'setAddToCollectionMenuOpen', 'addToCollectionMenuOpen'],
    ['setRemoveTagMenuOpen', 'setAddToCollectionMenuOpen', 'addToCollectionMenuOpen'],
    ['setAddToCollectionMenuOpen', 'setRemoveTagMenuOpen', 'removeTagMenuOpen'],
    ['setAddTagMenuOpen', 'setRemoveTagMenuOpen', 'removeTagMenuOpen'],
  ] as const)('opening with %s then %s leaves only %s open', (openFirst, openNext, expected) => {
    const { result } = setup();
    act(() => result.current[openFirst](true));
    act(() => result.current[openNext](true));
    for (const menu of ['addToCollectionMenuOpen', 'addTagMenuOpen', 'removeTagMenuOpen'] as const) {
      expect(result.current[menu]).toBe(menu === expected);
    }
    act(() => result.current[openNext](false));
    expect(result.current.addToCollectionMenuOpen).toBe(false);
    expect(result.current.addTagMenuOpen).toBe(false);
    expect(result.current.removeTagMenuOpen).toBe(false);
  });

  it('clearBulkSelection closes every menu, including the collection menu', () => {
    const { result } = setup();
    act(() => result.current.setAddToCollectionMenuOpen(true));
    act(() => result.current.clearBulkSelection());
    expect(result.current.addToCollectionMenuOpen).toBe(false);
    expect(result.current.addTagMenuOpen).toBe(false);
    expect(result.current.removeTagMenuOpen).toBe(false);
  });

  it('toggleBulkSelect adds and removes ids', () => {
    const { result } = setup();
    act(() => result.current.toggleBulkSelect('m1'));
    expect(result.current.selectedForBulk.has('m1')).toBe(true);
    act(() => result.current.toggleBulkSelect('m1'));
    expect(result.current.selectedForBulk.has('m1')).toBe(false);
  });

  it('selectAllVisible selects exactly the given ids', () => {
    const { result } = setup();
    act(() => result.current.selectAllVisible(['m1', 'm2']));
    expect(result.current.selectedForBulk).toEqual(new Set(['m1', 'm2']));
  });

  it('clearBulkSelection empties the set and closes the confirm prompt', () => {
    const { result } = setup();
    act(() => {
      result.current.selectAllVisible(['m1']);
      result.current.setConfirmBulkDelete(true);
    });
    act(() => result.current.clearBulkSelection());
    expect(result.current.selectedForBulk.size).toBe(0);
    expect(result.current.confirmBulkDelete).toBe(false);
  });

  it('reloads active models after a successful batch with a skipped deletion', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const a = makeModelFile({ id: 'm1' });
    const b = makeModelFile({ id: 'm2' });
    const { result } = renderHook(() => {
      const [models, setModels] = useState([a, b]);
      const bulk = useBulkSelection({
        models, setModels, refreshFolders: vi.fn(), refreshTags: vi.fn(), refreshTrash: vi.fn(),
        refetchAfterPartialDelete: async () => { setModels([b]); },
        bulkAddToCollection: vi.fn(), bulkRemoveFromCollection: vi.fn(),
      });
      return { models, ...bulk };
    });
    act(() => result.current.selectAllVisible(['m1', 'm2']));
    await act(async () => result.current.bulkDelete());
    expect(invoke).toHaveBeenCalledWith('delete_files', { fileIds: ['m1', 'm2'] });
    expect(result.current.models.map(model => model.id)).toEqual(['m2']);
    expect(result.current.selectedForBulk).toEqual(new Set(['m2']));
  });

  it('bulkAddToQueue only queues models not already queued', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'add_to_queue') return Promise.resolve(1);
      return Promise.resolve(undefined);
    });
    const models = [makeModelFile({ id: 'm1', queuePosition: null }), makeModelFile({ id: 'm2', queuePosition: 1 })];
    const { result } = setup(models);
    act(() => result.current.selectAllVisible(['m1', 'm2']));
    await act(async () => result.current.bulkAddToQueue());
    expect(invoke).toHaveBeenCalledWith('add_to_queue', { fileId: 'm1' });
    expect(invoke).not.toHaveBeenCalledWith('add_to_queue', { fileId: 'm2' });
  });

  it('bulkAddToCollectionAction delegates to the injected callback and clears selection', async () => {
    const { result, bulkAddToCollection } = setup();
    act(() => result.current.selectAllVisible(['m1']));
    await act(async () => result.current.bulkAddToCollectionAction('c1'));
    expect(bulkAddToCollection).toHaveBeenCalledWith(['m1'], 'c1');
    expect(result.current.selectedForBulk.size).toBe(0);
  });

  it('bulkAddTagAction tags every selected model, patches local state once, and closes the menu', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const models = [
      makeModelFile({ id: 'm1', tags: [] }),
      makeModelFile({ id: 'm2', tags: ['bambu'] }), // already has the tag - must not be sent twice
    ];
    const { result, setModels, refreshTags } = setup(models);
    act(() => {
      result.current.selectAllVisible(['m1', 'm2']);
      result.current.setTagDraft('  bambu  ');
      result.current.setAddTagMenuOpen(true);
    });
    await act(async () => result.current.bulkAddTagAction());

    expect(invoke).toHaveBeenCalledWith('add_tag', { fileId: 'm1', tag: 'bambu' });
    expect(invoke).toHaveBeenCalledWith('add_tag', { fileId: 'm2', tag: 'bambu' });
    expect(setModels).toHaveBeenCalledTimes(1);
    const patched = setModels.mock.calls[0][0](models);
    expect(patched.find((m: { id: string }) => m.id === 'm1').tags).toEqual(['bambu']);
    expect(patched.find((m: { id: string }) => m.id === 'm2').tags).toEqual(['bambu']);
    expect(refreshTags).toHaveBeenCalled();
    expect(result.current.tagDraft).toBe('');
    expect(result.current.addTagMenuOpen).toBe(false);
  });

  it('bulkAddTagAction normalizes a translated auto tag name', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const models = [makeModelFile({ id: 'm1', tags: [] })];
    const { result, setModels } = setup(models);
    act(() => {
      result.current.selectAllVisible(['m1']);
      result.current.setTagDraft(' Multipart ');
    });
    await act(async () => result.current.bulkAddTagAction());

    expect(invoke).toHaveBeenCalledWith('add_tag', { fileId: 'm1', tag: 'mehrteilig' });
    const patched = setModels.mock.calls[0][0](models);
    expect(patched[0].tags).toEqual(['mehrteilig']);
  });

  it('bulkAddTagAction is a no-op for a blank draft', async () => {
    const { result } = setup();
    act(() => {
      result.current.selectAllVisible(['m1']);
      result.current.setTagDraft('   ');
    });
    await act(async () => result.current.bulkAddTagAction());
    expect(invoke).not.toHaveBeenCalled();
  });

  it('bulkRemoveTagAction only calls remove for models that actually carry the tag', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const models = [
      makeModelFile({ id: 'm1', tags: ['bambu', 'mount'] }),
      makeModelFile({ id: 'm2', tags: ['decor'] }),
    ];
    const { result, setModels, refreshTags } = setup(models);
    act(() => result.current.selectAllVisible(['m1', 'm2']));
    await act(async () => result.current.bulkRemoveTagAction('bambu'));

    expect(invoke).toHaveBeenCalledWith('remove_tag', { fileId: 'm1', tag: 'bambu' });
    expect(invoke).not.toHaveBeenCalledWith('remove_tag', { fileId: 'm2', tag: 'bambu' });
    const patched = setModels.mock.calls[0][0](models);
    expect(patched.find((m: { id: string }) => m.id === 'm1').tags).toEqual(['mount']);
    expect(refreshTags).toHaveBeenCalled();
    expect(result.current.removeTagMenuOpen).toBe(false);
  });

  it('tagsInSelection is the sorted union of tags across selected models only', () => {
    const models = [
      makeModelFile({ id: 'm1', tags: ['mount', 'bambu'] }),
      makeModelFile({ id: 'm2', tags: ['decor'] }),
      makeModelFile({ id: 'm3', tags: ['unrelated'] }),
    ];
    const { result } = setup(models);
    act(() => result.current.selectAllVisible(['m1', 'm2']));
    expect(result.current.tagsInSelection).toEqual(['bambu', 'decor', 'mount']);
  });
});

it('computes tags shared by all selected models using canonical names', () => {
  const { result } = setup([
    makeModelFile({ id: 'm1', tags: ['multipart', 'partial'] }),
    makeModelFile({ id: 'm2', tags: ['mehrteilig'] }),
    makeModelFile({ id: 'm3', tags: [] }),
  ]);
  act(() => result.current.selectAllVisible(['m1', 'm2']));
  expect(result.current.tagsInAllSelection).toEqual(['mehrteilig']);
  act(() => result.current.selectAllVisible(['m1', 'm2', 'm3']));
  expect(result.current.tagsInAllSelection).toEqual([]);
});

it('accepts a suggestion directly without relying on a stale draft', async () => {
  const { result } = setup();
  act(() => { result.current.selectAllVisible(['m1']); result.current.setTagDraft('old'); });
  await act(async () => result.current.bulkAddTagAction(' #Multipart '));
  expect(invoke).toHaveBeenCalledWith('add_tag', { fileId: 'm1', tag: 'mehrteilig' });
});

it.each(['#', 'x'.repeat(101)])('does not submit empty or overlong tags (%s)', async (tag) => {
  const { result } = setup();
  act(() => result.current.selectAllVisible(['m1']));
  await act(async () => result.current.bulkAddTagAction(tag));
  expect(invoke).not.toHaveBeenCalled();
});

describe('bulkDelete failure reporting', () => {
  function setupDelete(refetch: (ids: string[]) => Promise<readonly { id: string }[]>) {
    const models = [makeModelFile({ id: 'm1', name: 'one' }), makeModelFile({ id: 'm2', name: 'locked' }), makeModelFile({ id: 'm3', name: 'three' })];
    return renderHook(() => useBulkSelection({
      models, setModels: vi.fn(), refreshFolders: vi.fn(), refreshTags: vi.fn(), refreshTrash: vi.fn(),
      refetchAfterPartialDelete: refetch, bulkAddToCollection: vi.fn(), bulkRemoveFromCollection: vi.fn(),
    }));
  }

  it('reports the names of files that survive a successful batch', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { result } = setupDelete(async () => [{ id: 'm2' }]);
    act(() => result.current.selectAllVisible(['m1', 'm2', 'm3']));
    await act(async () => result.current.bulkDelete());
    expect(result.current.deleteFailures).toEqual(['locked']);
    act(() => result.current.dismissDeleteFailures());
    expect(result.current.deleteFailures).toEqual([]);
  });

  it('reports nothing when every file is gone', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { result } = setupDelete(async () => []);
    act(() => result.current.selectAllVisible(['m1', 'm2']));
    await act(async () => result.current.bulkDelete());
    expect(result.current.deleteFailures).toEqual([]);
  });

  it('closes the confirmation and rethrows the reload error when the reload fails', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { result } = setupDelete(async () => { throw new Error('reload failed'); });
    act(() => result.current.selectAllVisible(['m1']));
    await expect(act(async () => result.current.bulkDelete())).rejects.toThrow('reload failed');
    expect(result.current.confirmBulkDelete).toBe(false);
  });

  it('does not let a reload failure hide the delete error', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('delete failed'));
    const { result } = setupDelete(async () => { throw new Error('reload failed'); });
    act(() => result.current.selectAllVisible(['m1']));
    await expect(act(async () => result.current.bulkDelete())).rejects.toThrow('delete failed');
  });

  it('lists the files still present after a failed delete call even though the reload worked', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('delete failed'));
    const { result } = setupDelete(async () => [{ id: 'm1' }, { id: 'm2' }]);
    act(() => result.current.selectAllVisible(['m1', 'm2']));
    let thrown: unknown;
    await act(async () => { await result.current.bulkDelete().catch(error => { thrown = error; }); });
    expect(thrown).toEqual(new Error('delete failed'));
    expect(result.current.deleteFailures).toEqual(['one', 'locked']);
  });
});
