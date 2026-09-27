import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as updateApi from './update';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('update api', () => {
  it('getAppVersion', async () => {
    vi.mocked(invoke).mockResolvedValue('0.15.0');
    const actual = await updateApi.getAppVersion();
    expect(invoke).toHaveBeenCalledWith('get_app_version');
    expect(actual).toBe('0.15.0');
  });

  it('openReleaseUrl', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await updateApi.openReleaseUrl('https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.7.9');
    expect(invoke).toHaveBeenCalledWith('open_release_url', {
      url: 'https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.7.9',
    });
  });
});
