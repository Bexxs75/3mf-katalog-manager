import { act, renderHook, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { useLoadedSetting } from './SettingState';

it('discards a late read after a newer write confirmation', async () => {
  let finish!: (value: boolean) => void;
  const load = vi.fn().mockResolvedValueOnce(false).mockImplementationOnce(() => new Promise<boolean>(resolve => { finish = resolve; }));
  const { result } = renderHook(() => useLoadedSetting<boolean>(load));
  await waitFor(() => expect(result.current.state).toBe('ready'));
  let pending!: Promise<boolean>;
  act(() => { pending = result.current.reload(); });
  expect(result.current.state).toBe('loading');
  act(() => result.current.confirm(true));
  await act(async () => { finish(false); await pending; });
  expect(result.current.value).toBe(true);
  expect(result.current.state).toBe('ready');
  expect(result.current.error).toBeNull();
});
