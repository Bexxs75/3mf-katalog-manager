import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useSlicers } from './useSlicers';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  localStorage.clear();
});

describe('useSlicers', () => {
  it('loads the backend-side registered-slicers registry on mount via scan_installed_slicers', async () => {
    vi.mocked(invoke).mockResolvedValue([{ id: '1', name: 'Orca', executablePath: '/usr/bin/orca' }]);
    const { result } = renderHook(() => useSlicers());
    await waitFor(() => expect(result.current.slicers).toHaveLength(1));
    expect(result.current.slicers[0].name).toBe('Orca');
    expect(result.current.slicers[0].path).toBe('/usr/bin/orca');
    expect(invoke).toHaveBeenCalledWith('scan_installed_slicers');
  });

  it('does not crash the hook when the scan fails', async () => {
    vi.mocked(invoke).mockRejectedValue('no permission');
    const { result } = renderHook(() => useSlicers());
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    expect(result.current.slicers).toEqual([]);
  });

  it('addSlicer goes through the backend-driven pick_and_register_slicer dialog, never a self-constructed path', async () => {
    // M-06/P0 (Task 11): es gibt hier bewusst keinen Weg, addSlicer() einen
    // Pfad-String zu uebergeben - die einzige Quelle fuer einen neuen
    // Slicer-Eintrag ist der native, backend-gesteuerte Dialog.
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'scan_installed_slicers') return Promise.resolve([]);
      if (cmd === 'pick_and_register_slicer') {
        return Promise.resolve({ id: '1', name: 'Orca', executablePath: '/usr/bin/orca' });
      }
      if (cmd === 'list_registered_slicers') {
        return Promise.resolve([{ id: '1', name: 'Orca', executablePath: '/usr/bin/orca' }]);
      }
      return Promise.resolve(null);
    });
    const { result } = renderHook(() => useSlicers());
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('scan_installed_slicers'));

    await act(async () => {
      await result.current.addSlicer();
    });

    expect(invoke).toHaveBeenCalledWith('pick_and_register_slicer');
    await waitFor(() => expect(result.current.slicers).toHaveLength(1));
    expect(result.current.slicers[0].name).toBe('Orca');
  });

  it('removeSlicer hides a slicer from the visible list without a backend call (no delete endpoint exists)', async () => {
    vi.mocked(invoke).mockResolvedValue([{ id: '1', name: 'Orca', executablePath: '/usr/bin/orca' }]);
    const { result } = renderHook(() => useSlicers());
    await waitFor(() => expect(result.current.slicers).toHaveLength(1));

    act(() => result.current.removeSlicer('1'));

    expect(result.current.slicers).toHaveLength(0);
  });
});
