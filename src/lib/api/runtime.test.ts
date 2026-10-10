import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.resetModules(); vi.mocked(invoke).mockReset(); });

it('shares one runtime query for concurrent and later consumers', async () => {
  vi.mocked(invoke).mockResolvedValue({ container: true });
  const { getRuntimeEnvironment } = await import('./runtime');
  const first = getRuntimeEnvironment();
  expect(getRuntimeEnvironment()).toBe(first);
  expect(await first).toEqual({ container: true });
  expect(await getRuntimeEnvironment()).toEqual({ container: true });
  expect(invoke).toHaveBeenCalledExactlyOnceWith('get_runtime_environment');
});

it('caches the desktop fallback after an IPC error', async () => {
  vi.mocked(invoke).mockRejectedValue(new Error('offline IPC'));
  const { getRuntimeEnvironment } = await import('./runtime');
  expect(await getRuntimeEnvironment()).toEqual({ container: false });
  expect(await getRuntimeEnvironment()).toEqual({ container: false });
  expect(invoke).toHaveBeenCalledTimes(1);
});
