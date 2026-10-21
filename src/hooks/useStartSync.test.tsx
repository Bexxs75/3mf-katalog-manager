import { beforeEach, expect, it, vi } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import { resetStartSyncSession, useStartSync } from './useStartSync';
import * as api from '../lib/api/diskSync';

vi.mock('../lib/api/diskSync', () => ({ runStartSync: vi.fn() }));
const result = (over: Partial<api.StartSyncResult> = {}): api.StartSyncResult => ({
  ran: true, busy: false, newFiles: 0, renamed: 0, missingFiles: 0, missingFolders: 0, duplicates: 0, unreachableRoots: 0, applied: null, ...over,
});
beforeEach(() => { resetStartSyncSession(); vi.mocked(api.runStartSync).mockReset(); vi.mocked(api.runStartSync).mockResolvedValue(result({ newFiles: 2 })); });
const setup = (ready: boolean, onApplied = vi.fn().mockResolvedValue(undefined)) =>
  ({ onApplied, ...renderHook(({ ready }) => useStartSync(ready, onApplied), { initialProps: { ready } }) });

it('does not call before the catalog is ready and calls once when it is', async () => {
  const hook = setup(false);
  await act(async () => {});
  expect(api.runStartSync).not.toHaveBeenCalled();
  hook.rerender({ ready: true });
  await waitFor(() => expect(hook.result.current.notice).not.toBeNull());
  expect(api.runStartSync).toHaveBeenCalledTimes(1);
});
it('calls only once per session, also after a remount, a flip of readiness or a dismissed notice', async () => {
  const first = setup(true);
  await waitFor(() => expect(first.result.current.notice).not.toBeNull());
  act(() => first.result.current.dismiss());
  first.rerender({ ready: false });
  first.rerender({ ready: true });
  first.unmount();
  const second = setup(true);
  await act(async () => {});
  expect(api.runStartSync).toHaveBeenCalledTimes(1);
  expect(second.result.current.notice).toBeNull();
});
it('shows no notice when the setting is off or nothing changed', async () => {
  vi.mocked(api.runStartSync).mockResolvedValue(result({ ran: false }));
  const hook = setup(true);
  await waitFor(() => expect(api.runStartSync).toHaveBeenCalled());
  await act(async () => {});
  expect(hook.result.current.notice).toBeNull();
  resetStartSyncSession();
  vi.mocked(api.runStartSync).mockResolvedValue(result());
  const none = setup(true);
  await act(async () => {});
  expect(none.result.current.notice).toBeNull();
});
it('reloads the lists only when something was taken over, before showing the notice', async () => {
  vi.mocked(api.runStartSync).mockResolvedValue(result({ applied: { added: 1, renamed: 0, failed: 0 } }));
  const hook = setup(true);
  await waitFor(() => expect(hook.result.current.notice).not.toBeNull());
  expect(hook.onApplied).toHaveBeenCalledTimes(1);
  resetStartSyncSession();
  vi.mocked(api.runStartSync).mockResolvedValue(result({ newFiles: 1 }));
  const plain = setup(true);
  await waitFor(() => expect(plain.result.current.notice).not.toBeNull());
  expect(plain.onApplied).not.toHaveBeenCalled();
});
it('survives a failing call without a notice', async () => {
  const log = vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.mocked(api.runStartSync).mockRejectedValue(new Error('boom'));
  const hook = setup(true);
  await waitFor(() => expect(log).toHaveBeenCalled());
  expect(hook.result.current.notice).toBeNull();
  log.mockRestore();
});
