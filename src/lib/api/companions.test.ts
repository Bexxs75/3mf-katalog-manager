import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as companionsApi from './companions';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('companions api', () => {
  it('listCompanions', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await companionsApi.listCompanions('7');
    expect(invoke).toHaveBeenCalledWith('list_companions', { fileId: '7' });
  });
  it('pickCompanionFiles', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await companionsApi.pickCompanionFiles();
    expect(invoke).toHaveBeenCalledWith('pick_companion_files');
  });
  it('addCompanion', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    await companionsApi.addCompanion('7', '/tmp/a.png');
    expect(invoke).toHaveBeenCalledWith('add_companion', { fileId: '7', sourcePath: '/tmp/a.png' });
  });
  it('removeCompanion and restoreCompanion', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await companionsApi.removeCompanion('3');
    await companionsApi.restoreCompanion('3');
    expect(invoke).toHaveBeenCalledWith('remove_companion', { companionId: '3' });
    expect(invoke).toHaveBeenCalledWith('restore_companion', { companionId: '3' });
  });
  it('open info, open and image data go by id only', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await companionsApi.companionOpenInfo('3');
    await companionsApi.openCompanion('3');
    await companionsApi.companionImageData('3');
    expect(invoke).toHaveBeenCalledWith('companion_open_info', { companionId: '3' });
    expect(invoke).toHaveBeenCalledWith('open_companion', { companionId: '3' });
    expect(invoke).toHaveBeenCalledWith('companion_image_data', { companionId: '3' });
  });
});
