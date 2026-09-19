import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as slicerApi from './slicer';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('slicer api', () => {
  it('openInSlicer sends the model id and the registered slicer id, never a free path', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await slicerApi.openInSlicer('m1', 's1');
    expect(invoke).toHaveBeenCalledWith('open_in_slicer', { fileId: 'm1', slicerId: 's1' });
  });

  it('pickAndRegisterSlicer opens the backend-side native dialog with no path argument', async () => {
    vi.mocked(invoke).mockResolvedValue({ id: '1', name: 'Orca', executablePath: '/usr/bin/orca' });
    const result = await slicerApi.pickAndRegisterSlicer();
    expect(invoke).toHaveBeenCalledWith('pick_and_register_slicer');
    expect(result).toEqual({ id: '1', name: 'Orca', executablePath: '/usr/bin/orca' });
  });

  it('listRegisteredSlicers reads the backend registry', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await slicerApi.listRegisteredSlicers();
    expect(invoke).toHaveBeenCalledWith('list_registered_slicers');
  });
});
