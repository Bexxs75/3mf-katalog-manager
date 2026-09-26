import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useSlicerLauncher } from './useSlicerLauncher';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// No expression body: vi.fn().mockReset() returns the mock function
// itself, and Vitest treats a function value returned from a hook as
// automatic teardown that is called again after the test - with a mock
// set up with mockRejectedValue this leads to a seemingly unhandled
// rejection that is blamed on the test.
beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe('useSlicerLauncher', () => {
  it('calls onNeedsSetup when there are no slicers configured', () => {
    const onNeedsSetup = vi.fn();
    const { result } = renderHook(() => useSlicerLauncher([], null, onNeedsSetup));
    act(() => result.current.openInSlicer('m1'));
    expect(onNeedsSetup).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it('opens the primary slicer for the given model id, resolving both server-side by id', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const slicers = [{ id: 's1', name: 'Orca', path: '/usr/bin/orca' }];
    const { result } = renderHook(() => useSlicerLauncher(slicers, 's1', vi.fn()));
    act(() => result.current.openInSlicer('m1'));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('open_in_slicer', { fileId: 'm1', slicerId: 's1' }),
    );
  });

  it('sets slicerError when opening fails', async () => {
    vi.mocked(invoke).mockRejectedValue('not found');
    const slicers = [{ id: 's1', name: 'Orca', path: '/usr/bin/orca' }];
    const { result } = renderHook(() => useSlicerLauncher(slicers, 's1', vi.fn()));
    act(() => result.current.openInSlicer('m1'));
    await waitFor(() => expect(result.current.slicerError).toBe('not found'));
  });
});
