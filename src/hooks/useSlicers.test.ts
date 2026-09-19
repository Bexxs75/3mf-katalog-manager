import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useSlicers } from './useSlicers';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  localStorage.clear();
});

describe('useSlicers', () => {
  it('auto-merges detected slicers on mount', async () => {
    vi.mocked(invoke).mockResolvedValue([{ name: 'Orca', path: '/usr/bin/orca' }]);
    const { result } = renderHook(() => useSlicers());
    await waitFor(() => expect(result.current.slicers).toHaveLength(1));
    expect(result.current.slicers[0].name).toBe('Orca');
    expect(invoke).toHaveBeenCalledWith('scan_installed_slicers');
  });

  it('does not crash the hook when the scan fails', async () => {
    vi.mocked(invoke).mockRejectedValue('no permission');
    const { result } = renderHook(() => useSlicers());
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    expect(result.current.slicers).toEqual([]);
  });
});
