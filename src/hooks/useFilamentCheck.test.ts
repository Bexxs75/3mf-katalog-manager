import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useFilamentCheck } from './useFilamentCheck';
import type { FilamentCheck } from '../types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const check = (fileId: string): FilamentCheck => ({ fileId, status: 'ok', needs: [] });

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe('useFilamentCheck', () => {
  it('loads checks keyed by file id', async () => {
    vi.mocked(invoke).mockResolvedValue([check('1'), check('2')]);
    const { result } = renderHook(() => useFilamentCheck(['1', '2']));
    expect(result.current.checks).toBeNull();
    await waitFor(() => expect(result.current.checks?.get('2')?.status).toBe('ok'));
    expect(result.current.error).toBe(false);
  });

  it('returns an empty map without calling the backend for no ids', async () => {
    const { result } = renderHook(() => useFilamentCheck([]));
    await waitFor(() => expect(result.current.checks?.size).toBe(0));
    expect(invoke).not.toHaveBeenCalled();
  });

  it('reports an error when the backend fails', async () => {
    vi.mocked(invoke).mockRejectedValue('kaputt');
    const { result } = renderHook(() => useFilamentCheck(['1']));
    await waitFor(() => expect(result.current.error).toBe(true));
    expect(result.current.checks).toBeNull();
  });

  it('discards a stale answer when the ids change before it arrives', async () => {
    let resolveFirst: (v: FilamentCheck[]) => void = () => {};
    vi.mocked(invoke)
      .mockImplementationOnce(() => new Promise((r) => { resolveFirst = r as (v: FilamentCheck[]) => void; }))
      .mockResolvedValueOnce([check('2')]);
    const { result, rerender } = renderHook(({ ids }) => useFilamentCheck(ids), { initialProps: { ids: ['1'] } });
    rerender({ ids: ['2'] });
    await waitFor(() => expect(result.current.checks?.has('2')).toBe(true));
    resolveFirst([check('1')]);
    await new Promise((r) => setTimeout(r, 0));
    expect(result.current.checks?.has('1')).toBe(false);
  });

  it('keeps the previous checks visible while a reload is in flight', async () => {
    let resolveSecond: (v: FilamentCheck[]) => void = () => {};
    vi.mocked(invoke)
      .mockResolvedValueOnce([check('1')])
      .mockImplementationOnce(() => new Promise((r) => { resolveSecond = r as (v: FilamentCheck[]) => void; }));
    const { result, rerender } = renderHook(({ ids }) => useFilamentCheck(ids), { initialProps: { ids: ['1'] } });
    await waitFor(() => expect(result.current.checks?.has('1')).toBe(true));

    rerender({ ids: ['1', '2'] });
    // Kein Flackern: waehrend des Nachladens bleibt der alte Stand sichtbar.
    expect(result.current.checks?.has('1')).toBe(true);
    expect(result.current.error).toBe(false);

    resolveSecond([check('1'), check('2')]);
    await waitFor(() => expect(result.current.checks?.has('2')).toBe(true));
  });

  it('refetches when refreshKey changes for unchanged ids', async () => {
    vi.mocked(invoke).mockResolvedValue([check('1')]);
    const { rerender } = renderHook(({ refreshKey }) => useFilamentCheck(['1'], refreshKey), {
      initialProps: { refreshKey: 'a' },
    });
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));

    rerender({ refreshKey: 'b' });
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
  });
});
