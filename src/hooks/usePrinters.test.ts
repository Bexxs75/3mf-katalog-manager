import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { usePrinters } from './usePrinters';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

const X1C = { id: 'p1', name: 'X1C', units: [] };

describe('usePrinters', () => {
  it('loads printers on mount', async () => {
    vi.mocked(invoke).mockResolvedValue([X1C]);
    const { result } = renderHook(() => usePrinters());
    await waitFor(() => expect(result.current.printers).toEqual([X1C]));
    expect(invoke).toHaveBeenCalledWith('list_printers');
  });

  it('runs a change and reloads the list afterwards', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === 'list_printers' ? [X1C] : { id: 'u1' }),
    );
    const { result } = renderHook(() => usePrinters());
    await waitFor(() => expect(result.current.printers).toHaveLength(1));
    vi.mocked(invoke).mockClear();
    await act(async () => {
      await result.current.addUnit('p1', 'bambu_ams', 'AMS A', null);
    });
    expect(invoke).toHaveBeenCalledWith('add_unit', { printerId: 'p1', kind: 'bambu_ams', name: 'AMS A', slotCount: null });
    expect(invoke).toHaveBeenCalledWith('list_printers');
  });

  it('keeps the error and rethrows it', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === 'list_printers' ? Promise.resolve([]) : Promise.reject('Ein Drucker kann hoechstens 4 AMS haben'),
    );
    const { result } = renderHook(() => usePrinters());
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('list_printers'));
    let thrown: unknown = null;
    await act(async () => {
      await result.current.addUnit('p1', 'bambu_ams', 'AMS E', null).catch((e) => { thrown = e; });
    });
    expect(thrown).toBe('Ein Drucker kann hoechstens 4 AMS haben');
    expect(result.current.error).toBe('Ein Drucker kann hoechstens 4 AMS haben');
  });
});
