import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as catalogMetaApi from './catalogMeta';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('catalogMeta api', () => {
  it('listTagCounts', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await catalogMetaApi.listTagCounts();
    expect(invoke).toHaveBeenCalledWith('list_tag_counts');
  });
  it('scanInstalledSlicers', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await catalogMetaApi.scanInstalledSlicers();
    expect(invoke).toHaveBeenCalledWith('scan_installed_slicers');
  });
});
