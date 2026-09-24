import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useBulkSelection } from './useBulkSelection';
import { makeModelFile } from '../test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

function setup(models = [makeModelFile({ id: 'm1' }), makeModelFile({ id: 'm2' })]) {
  const setModels = vi.fn();
  const refreshFolders = vi.fn();
  const refreshTags = vi.fn();
  const refreshCreators = vi.fn();
  const refreshTrash = vi.fn();
  const bulkAddToCollection = vi.fn().mockResolvedValue(undefined);
  const bulkRemoveFromCollection = vi.fn().mockResolvedValue(undefined);
  const hook = renderHook(() =>
    useBulkSelection({
      models, setModels, refreshFolders, refreshTags, refreshCreators, refreshTrash,
      bulkAddToCollection, bulkRemoveFromCollection,
    }),
  );
  return { ...hook, setModels, refreshFolders, refreshTags, refreshCreators, refreshTrash, bulkAddToCollection, bulkRemoveFromCollection };
}

describe('useBulkSelection', () => {
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

  it('bulkDelete calls delete_files, filters models, and clears the selection', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { result, setModels, refreshFolders, refreshTrash } = setup();
    act(() => result.current.selectAllVisible(['m1']));
    await act(async () => result.current.bulkDelete());
    expect(invoke).toHaveBeenCalledWith('delete_files', { fileIds: ['m1'] });
    expect(setModels).toHaveBeenCalled();
    expect(refreshFolders).toHaveBeenCalled();
    expect(refreshTrash).toHaveBeenCalled();
    expect(result.current.selectedForBulk.size).toBe(0);
  });

  it('bulkAddToQueue only queues models not already queued', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'add_to_queue') return Promise.resolve(1);
      if (cmd === 'list_files') return Promise.resolve([]);
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
