import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import React from 'react';

const api = vi.hoisted(() => ({
  getPrinterLinkEnabled: vi.fn(),
  setPrinterLinkEnabled: vi.fn(),
  listPrinterConnections: vi.fn(),
  testPrinterConnection: vi.fn(),
  removePrinterConnection: vi.fn(),
  syncPrintersNow: vi.fn(),
  listOpenPrinterJobs: vi.fn(),
  previewPrinterJob: vi.fn(),
  ignorePrinterJob: vi.fn(),
  confirmPrinterJobs: vi.fn(),
  PRINTER_JOBS_CHANGED_EVENT: 'printer-jobs-changed',
}));
vi.mock('../lib/api/printerLink', () => api);
const listeners: Record<string, () => void> = {};
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, cb: () => void) => {
    listeners[name] = cb;
    return Promise.resolve(() => undefined);
  }),
}));

import { usePrinterLink } from './usePrinterLink';

beforeEach(() => {
  Object.values(api).forEach((f) => typeof f === 'function' && (f as ReturnType<typeof vi.fn>).mockReset());
  api.getPrinterLinkEnabled.mockResolvedValue(true);
  api.listPrinterConnections.mockResolvedValue([]);
  api.listOpenPrinterJobs.mockResolvedValue([]);
});

describe('usePrinterLink', () => {
  it('loads switch, connections and jobs', async () => {
    const { result } = renderHook(() => usePrinterLink());
    await waitFor(() => expect(result.current.enabled).toBe(true));
    expect(api.listOpenPrinterJobs).toHaveBeenCalled();
  });

  it('reloads when the backend reports a sync', async () => {
    renderHook(() => usePrinterLink());
    await waitFor(() => expect(listeners['printer-jobs-changed']).toBeDefined());
    api.listOpenPrinterJobs.mockClear();
    await act(async () => {
      listeners['printer-jobs-changed']();
      await Promise.resolve();
    });
    expect(api.listOpenPrinterJobs).toHaveBeenCalled();
  });

  it('switching on refreshes the state', async () => {
    api.setPrinterLinkEnabled.mockResolvedValue(undefined);
    const { result } = renderHook(() => usePrinterLink());
    await waitFor(() => expect(result.current.enabled).toBe(true));
    await act(() => result.current.setEnabled(false));
    expect(api.setPrinterLinkEnabled).toHaveBeenCalledWith(false);
  });

  it('confirms jobs and refreshes', async () => {
    api.confirmPrinterJobs.mockResolvedValue({ confirmed: 1, failed: 0 });
    const { result } = renderHook(() => usePrinterLink());
    await waitFor(() => expect(result.current.enabled).toBe(true));
    api.listOpenPrinterJobs.mockClear();
    await act(() => result.current.confirmJobs([{ jobId: '5', spoolId: '100', fileId: null }]));
    expect(api.confirmPrinterJobs).toHaveBeenCalledWith([{ jobId: '5', spoolId: '100', fileId: null }]);
    expect(api.listOpenPrinterJobs).toHaveBeenCalled();
  });

  it('previews a job without refreshing', async () => {
    api.previewPrinterJob.mockResolvedValue({ grams: 4, materialMismatch: false });
    const { result } = renderHook(() => usePrinterLink());
    await waitFor(() => expect(result.current.enabled).toBe(true));
    api.listOpenPrinterJobs.mockClear();
    const preview = await act(() => result.current.previewJob('5', '100'));
    expect(preview).toEqual({ grams: 4, materialMismatch: false });
    expect(api.listOpenPrinterJobs).not.toHaveBeenCalled();
  });

  it('reports an error when loading fails', async () => {
    api.listOpenPrinterJobs.mockRejectedValue(new Error('boom'));
    const { result } = renderHook(() => usePrinterLink());
    await waitFor(() => expect(result.current.error).not.toBeNull());
  });

  it('loads state correctly under React StrictMode', async () => {
    const testConnections = [{ id: 'printer1', name: 'Test Printer', address: '192.168.1.100' }];
    api.listPrinterConnections.mockResolvedValue(testConnections);

    const { result } = renderHook(() => usePrinterLink(), {
      wrapper: React.StrictMode as React.ComponentType<{ children: React.ReactNode }>,
    });

    await waitFor(() => expect(result.current.enabled).toBe(true));
    expect(result.current.connections).toEqual(testConnections);
    expect(api.listPrinterConnections).toHaveBeenCalled();
  });
});
