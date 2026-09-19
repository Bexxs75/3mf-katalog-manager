import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as catalogMetaApi from './catalogMeta';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => vi.mocked(invoke).mockReset());

describe('catalogMeta api', () => {
  it('listTagCounts', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await catalogMetaApi.listTagCounts();
    expect(invoke).toHaveBeenCalledWith('list_tag_counts');
  });
  it('listCreators', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await catalogMetaApi.listCreators();
    expect(invoke).toHaveBeenCalledWith('list_creators');
  });
  it('listSavedFilters', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await catalogMetaApi.listSavedFilters();
    expect(invoke).toHaveBeenCalledWith('list_saved_filters');
  });
  it('scanInstalledSlicers', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await catalogMetaApi.scanInstalledSlicers();
    expect(invoke).toHaveBeenCalledWith('scan_installed_slicers');
  });
});
