import { afterEach, beforeEach, expect, it, vi } from 'vitest';

beforeEach(() => { vi.resetModules(); vi.stubEnv('VITE_TARGET', 'web'); });
afterEach(() => { vi.unstubAllEnvs(); vi.restoreAllMocks(); });

// The web server rejects unknown arguments (deny_unknown_fields), so `lean` must never be sent.
it('rescanFileMetadata never sends lean in the web build', async () => {
  const send = vi.fn().mockResolvedValue({});
  vi.doMock('../transport', () => ({ invoke: send }));
  const { rescanFileMetadata } = await import('./files');
  await rescanFileMetadata('m1', true);
  expect(send).toHaveBeenCalledWith('rescan_file_metadata', { fileId: 'm1' });
});
