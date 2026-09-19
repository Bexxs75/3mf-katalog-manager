import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as filesApi from './files';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe('files api', () => {
  it('listFiles', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await filesApi.listFiles();
    expect(invoke).toHaveBeenCalledWith('list_files');
  });
  it('listTrash', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await filesApi.listTrash();
    expect(invoke).toHaveBeenCalledWith('list_trash');
  });
  it('markFileViewed', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.markFileViewed('m1');
    expect(invoke).toHaveBeenCalledWith('mark_file_viewed', { fileId: 'm1' });
  });
  it('deleteFile', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.deleteFile('m1');
    expect(invoke).toHaveBeenCalledWith('delete_file', { fileId: 'm1' });
  });
  it('deleteFiles', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.deleteFiles(['m1', 'm2']);
    expect(invoke).toHaveBeenCalledWith('delete_files', { fileIds: ['m1', 'm2'] });
  });
  it('restoreFile', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.restoreFile('m1');
    expect(invoke).toHaveBeenCalledWith('restore_file', { fileId: 'm1' });
  });
  it('deleteFilePermanently', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.deleteFilePermanently('m1');
    expect(invoke).toHaveBeenCalledWith('delete_file_permanently', { fileId: 'm1' });
  });
  it('emptyTrash', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.emptyTrash();
    expect(invoke).toHaveBeenCalledWith('empty_trash');
  });
  it('addTag', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.addTag('m1', 'red');
    expect(invoke).toHaveBeenCalledWith('add_tag', { fileId: 'm1', tag: 'red' });
  });
  it('removeTag', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.removeTag('m1', 'red');
    expect(invoke).toHaveBeenCalledWith('remove_tag', { fileId: 'm1', tag: 'red' });
  });
  it('setPrintStatus', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.setPrintStatus('m1', 'printed');
    expect(invoke).toHaveBeenCalledWith('set_print_status', { fileId: 'm1', status: 'printed' });
  });
  it('setFavorite', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.setFavorite('m1', true);
    expect(invoke).toHaveBeenCalledWith('set_favorite', { fileId: 'm1', favorite: true });
  });
  it('addToQueue returns the position', async () => {
    vi.mocked(invoke).mockResolvedValue(3);
    const position = await filesApi.addToQueue('m1');
    expect(position).toBe(3);
    expect(invoke).toHaveBeenCalledWith('add_to_queue', { fileId: 'm1' });
  });
  it('removeFromQueue', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.removeFromQueue('m1');
    expect(invoke).toHaveBeenCalledWith('remove_from_queue', { fileId: 'm1' });
  });
  it('reorderQueue', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.reorderQueue([{ fileId: 'm1', position: 1 }]);
    expect(invoke).toHaveBeenCalledWith('reorder_queue', { updates: [{ fileId: 'm1', position: 1 }] });
  });
  it('uploadCustomImage', async () => {
    vi.mocked(invoke).mockResolvedValue('data:image/png;base64,abc');
    const result = await filesApi.uploadCustomImage('m1');
    expect(result).toBe('data:image/png;base64,abc');
    expect(invoke).toHaveBeenCalledWith('upload_custom_image', { fileId: 'm1' });
  });
  it('setRenderSnapshot', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.setRenderSnapshot('m1', 'abc');
    expect(invoke).toHaveBeenCalledWith('set_render_snapshot', { fileId: 'm1', imageBase64: 'abc' });
  });
  it('setSourceUrl', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await filesApi.setSourceUrl('m1', 'https://example.com');
    expect(invoke).toHaveBeenCalledWith('set_source_url', { fileId: 'm1', url: 'https://example.com' });
  });
  it('rescanFileMetadata', async () => {
    vi.mocked(invoke).mockResolvedValue({ id: 'm1' });
    await filesApi.rescanFileMetadata('m1');
    expect(invoke).toHaveBeenCalledWith('rescan_file_metadata', { fileId: 'm1' });
  });
});
