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
});
