import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { renderHook, waitFor } from '@testing-library/react';
import type { StartSyncResult } from '../lib/api/diskSync';

const api = vi.hoisted(() => ({ run: vi.fn() }));
vi.mock('../lib/api/diskSync', async importOriginal => ({ ...await importOriginal<typeof import('../lib/api/diskSync')>(), runStartSync: api.run }));
import { resetStartSyncSession, startSyncPoll, useStartSync } from './useStartSync';

const result = (over: Partial<StartSyncResult>): StartSyncResult => ({ ran: true, busy: false, newFiles: 0, renamed: 0, missingFiles: 0, missingFolders: 0, duplicates: 0, unreachableRoots: 0, applied: null, state: 'done', ...over });
beforeEach(() => { api.run.mockReset(); resetStartSyncSession(); startSyncPoll.intervalMs = 1; });
afterEach(() => { startSyncPoll.intervalMs = 5000; startSyncPoll.maxAttempts = 24; });

describe('web start check', () => {
  it('asks again while the server is still checking, then shows the notice', async () => {
    api.run.mockResolvedValueOnce(result({ busy: true, ran: false, state: 'running' })).mockResolvedValueOnce(result({ busy: true, ran: false, state: 'running' })).mockResolvedValueOnce(result({ newFiles: 3 }));
    const { result: hook } = renderHook(() => useStartSync(true, async () => {}));
    await waitFor(() => expect(hook.current.notice).toMatchObject({ newFiles: 3 }));
    expect(api.run).toHaveBeenCalledTimes(3);
  });
  it('gives up after the maximum number of attempts without a notice', async () => {
    startSyncPoll.maxAttempts = 3;
    api.run.mockResolvedValue(result({ busy: true, ran: false, state: 'running' }));
    const { result: hook } = renderHook(() => useStartSync(true, async () => {}));
    await waitFor(() => expect(api.run).toHaveBeenCalledTimes(4));
    await new Promise(resolve => setTimeout(resolve, 20));
    expect(api.run).toHaveBeenCalledTimes(4);
    expect(hook.current.notice).toBeNull();
  });
  it('says nothing when the check is off', async () => {
    api.run.mockResolvedValue(result({ ran: false, state: 'off' }));
    const { result: hook } = renderHook(() => useStartSync(true, async () => {}));
    await waitFor(() => expect(api.run).toHaveBeenCalledTimes(1));
    expect(hook.current.notice).toBeNull();
  });
  it('waits until the catalog is ready', () => {
    renderHook(() => useStartSync(false, async () => {}));
    expect(api.run).not.toHaveBeenCalled();
  });
});
