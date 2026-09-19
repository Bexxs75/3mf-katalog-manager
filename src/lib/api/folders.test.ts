import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as foldersApi from './folders';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => vi.mocked(invoke).mockReset());

describe('folders api', () => {
  it('listFolders', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await foldersApi.listFolders();
    expect(invoke).toHaveBeenCalledWith('list_folders');
  });
  it('createFolder', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await foldersApi.createFolder('p1', 'New');
    expect(invoke).toHaveBeenCalledWith('create_folder', { parentId: 'p1', name: 'New' });
  });
  it('moveFileToFolder', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await foldersApi.moveFileToFolder('f1', 'folder1');
    expect(invoke).toHaveBeenCalledWith('move_file_to_folder', { fileId: 'f1', folderId: 'folder1' });
  });
  it('moveFolder', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await foldersApi.moveFolder('folder1', 'folder2');
    expect(invoke).toHaveBeenCalledWith('move_folder', { folderId: 'folder1', newParentId: 'folder2' });
  });
});
