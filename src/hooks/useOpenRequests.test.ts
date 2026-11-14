import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useOpenRequests } from './useOpenRequests';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

const external = (sourceId: string) => ({ kind: 'external', sourceId, name: `${sourceId}.stl`, sizeBytes: 3, extension: 'stl' }) as const;

let fire: () => void;
const unlisten = vi.fn();

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  unlisten.mockReset();
  vi.mocked(listen).mockReset();
  vi.mocked(listen).mockImplementation(async (_event, handler) => {
    fire = () => (handler as () => void)();
    return unlisten;
  });
});

describe('useOpenRequests', () => {
  it('collects the files of a cold start once the hook mounts', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([{ kind: 'catalog', fileId: '4' }]);
    const { result } = renderHook(() => useOpenRequests());
    await waitFor(() => expect(result.current.requests).toEqual([{ kind: 'catalog', fileId: '4' }]));
    expect(invoke).toHaveBeenCalledWith('take_open_requests');
    expect(listen).toHaveBeenCalledWith('open-files-requested', expect.any(Function));
  });

  it('collects again on the event and keeps earlier requests until they are consumed', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]).mockResolvedValueOnce([external('a')]).mockResolvedValueOnce([external('b')]);
    const { result } = renderHook(() => useOpenRequests());
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
    await act(async () => fire());
    await act(async () => fire());
    await waitFor(() => expect(result.current.requests).toEqual([external('a'), external('b')]));

    let taken: unknown;
    act(() => {
      taken = result.current.consume();
    });
    expect(taken).toEqual([external('a'), external('b')]);
    expect(result.current.requests).toEqual([]);
  });

  it('survives a failing collect and stops listening on unmount', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    vi.mocked(invoke).mockRejectedValueOnce(new Error('boom')).mockResolvedValueOnce([external('c')]);
    const { result, unmount } = renderHook(() => useOpenRequests());
    await waitFor(() => expect(warn).toHaveBeenCalled());
    await act(async () => fire());
    await waitFor(() => expect(result.current.requests).toEqual([external('c')]));
    unmount();
    expect(unlisten).toHaveBeenCalled();
    warn.mockRestore();
  });
});
