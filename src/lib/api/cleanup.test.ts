import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as cleanupApi from './cleanup';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('cleanup api', () => {
  it('scanCatalogIssues', async () => {
    vi.mocked(invoke).mockResolvedValue({ orphaned: [], duplicateGroups: [] });
    await cleanupApi.scanCatalogIssues();
    expect(invoke).toHaveBeenCalledWith('scan_catalog_issues');
  });
});
