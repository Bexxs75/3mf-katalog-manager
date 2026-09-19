import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as updateApi from './update';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('update api', () => {
  it('checkForUpdate', async () => {
    const result = {
      currentVersion: '0.7.8',
      latestVersion: '0.7.9',
      updateAvailable: true,
      releaseUrl: 'https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.7.9',
    };
    vi.mocked(invoke).mockResolvedValue(result);
    const actual = await updateApi.checkForUpdate();
    expect(invoke).toHaveBeenCalledWith('check_for_update');
    expect(actual).toEqual(result);
  });

  it('openReleaseUrl', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await updateApi.openReleaseUrl('https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.7.9');
    expect(invoke).toHaveBeenCalledWith('open_release_url', {
      url: 'https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.7.9',
    });
  });
});
