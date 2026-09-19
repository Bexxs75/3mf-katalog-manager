import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as slicerApi from './slicer';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('slicer api', () => {
  it('openInSlicer', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await slicerApi.openInSlicer('/usr/bin/slicer', '/model.3mf');
    expect(invoke).toHaveBeenCalledWith('open_in_slicer', { slicerPath: '/usr/bin/slicer', filePath: '/model.3mf' });
  });
});
