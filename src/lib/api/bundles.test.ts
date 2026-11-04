import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as bundlesApi from './bundles';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('bundles api', () => {
  it('listBundles', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await bundlesApi.listBundles();
    expect(invoke).toHaveBeenCalledWith('list_bundles');
  });
  it('createBundle', async () => {
    vi.mocked(invoke).mockResolvedValue({ id: 'b1' });
    await bundlesApi.createBundle('Vase', 'parts', ['1', '2']);
    expect(invoke).toHaveBeenCalledWith('create_bundle', { name: 'Vase', kind: 'parts', fileIds: ['1', '2'] });
  });
  it('updateBundle sends only the fields it is given', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await bundlesApi.updateBundle('b1', { name: 'Neu' });
    expect(invoke).toHaveBeenCalledWith('update_bundle', { bundleId: 'b1', name: 'Neu' });
    await bundlesApi.updateBundle('b1', { kind: 'variants' });
    expect(invoke).toHaveBeenLastCalledWith('update_bundle', { bundleId: 'b1', kind: 'variants' });
  });
  it('addFilesToBundle', async () => {
    vi.mocked(invoke).mockResolvedValue({ id: 'b1' });
    await bundlesApi.addFilesToBundle('b1', ['3']);
    expect(invoke).toHaveBeenCalledWith('add_files_to_bundle', { bundleId: 'b1', fileIds: ['3'] });
  });
  it('removeFilesFromBundle', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await bundlesApi.removeFilesFromBundle(['1']);
    expect(invoke).toHaveBeenCalledWith('remove_files_from_bundle', { fileIds: ['1'] });
  });
  it('dissolveBundle', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await bundlesApi.dissolveBundle('b1');
    expect(invoke).toHaveBeenCalledWith('dissolve_bundle', { bundleId: 'b1' });
  });
  it('updateBundleMember', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await bundlesApi.updateBundleMember('1', { label: '0,6 mm', makeTitle: true });
    expect(invoke).toHaveBeenCalledWith('update_bundle_member', { fileId: '1', label: '0,6 mm', makeTitle: true });
  });
});
