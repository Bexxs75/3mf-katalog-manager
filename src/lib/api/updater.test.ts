import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke, Channel } from '@tauri-apps/api/core';
import * as api from './updater';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: unknown;
  },
}));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe('updater api', () => {
  it('checkAppUpdate calls check_app_update and returns the result', async () => {
    const info = {
      currentVersion: '0.15.0',
      availableVersion: '0.15.1',
      releaseUrl: 'https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.15.1',
      canInstall: true,
      lastUpdate: null,
    };
    vi.mocked(invoke).mockResolvedValue(info);
    const actual = await api.checkAppUpdate();
    expect(invoke).toHaveBeenCalledWith('check_app_update');
    expect(actual).toEqual(info);
  });

  it('downloadAppUpdate calls download_app_update with a channel wired to the callback', async () => {
    vi.mocked(invoke).mockResolvedValue('0.15.1');
    const onProgress = vi.fn();
    const actual = await api.downloadAppUpdate(onProgress);
    expect(actual).toBe('0.15.1');
    expect(invoke).toHaveBeenCalledWith('download_app_update', { onProgress: expect.any(Channel) });
    const [, args] = vi.mocked(invoke).mock.calls[0];
    const channel = (args as { onProgress: InstanceType<typeof Channel> }).onProgress;
    expect(channel.onmessage).toBe(onProgress);
  });

  it('discardAppUpdate calls discard_app_update', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.discardAppUpdate();
    expect(invoke).toHaveBeenCalledWith('discard_app_update');
  });

  it('installAppUpdate calls install_app_update', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.installAppUpdate();
    expect(invoke).toHaveBeenCalledWith('install_app_update');
  });
});
