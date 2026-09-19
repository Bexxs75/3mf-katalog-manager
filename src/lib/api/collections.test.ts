import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as collectionsApi from './collections';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => vi.mocked(invoke).mockReset());

describe('collections api', () => {
  it('listCollections', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await collectionsApi.listCollections();
    expect(invoke).toHaveBeenCalledWith('list_collections');
  });
  it('listCollectionFiles', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await collectionsApi.listCollectionFiles('c1');
    expect(invoke).toHaveBeenCalledWith('list_collection_files', { collectionId: 'c1' });
  });
  it('createCollection', async () => {
    vi.mocked(invoke).mockResolvedValue({ id: 'c1', name: 'X', modelCount: 0 });
    await collectionsApi.createCollection('X');
    expect(invoke).toHaveBeenCalledWith('create_collection', { name: 'X' });
  });
  it('renameCollection', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await collectionsApi.renameCollection('c1', 'Y');
    expect(invoke).toHaveBeenCalledWith('rename_collection', { collectionId: 'c1', name: 'Y' });
  });
  it('deleteCollection', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await collectionsApi.deleteCollection('c1');
    expect(invoke).toHaveBeenCalledWith('delete_collection', { collectionId: 'c1' });
  });
  it('addFilesToCollection', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await collectionsApi.addFilesToCollection('c1', ['m1']);
    expect(invoke).toHaveBeenCalledWith('add_files_to_collection', { collectionId: 'c1', fileIds: ['m1'] });
  });
  it('removeFileFromCollection', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await collectionsApi.removeFileFromCollection('c1', 'm1');
    expect(invoke).toHaveBeenCalledWith('remove_file_from_collection', { collectionId: 'c1', fileId: 'm1' });
  });
  it('reorderCollection', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await collectionsApi.reorderCollection('c1', [{ fileId: 'm1', position: 0 }]);
    expect(invoke).toHaveBeenCalledWith('reorder_collection', { collectionId: 'c1', updates: [{ fileId: 'm1', position: 0 }] });
  });
});
