import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { useLastPrinter } from './useLastPrinter';
import { getLastPrinterForFile, type LastPrinter } from '../lib/api/lastPrinter';
vi.mock('../lib/api/lastPrinter', () => ({ getLastPrinterForFile: vi.fn() }));
beforeEach(() => vi.clearAllMocks());
it('discards stale responses and reloads on refresh', async () => {
  let resolveFirst!: (value: LastPrinter) => void;
  vi.mocked(getLastPrinterForFile).mockImplementationOnce(() => new Promise(resolve => { resolveFirst = resolve; }))
    .mockResolvedValueOnce({ printerName: 'B', endedAt: 2 }).mockResolvedValueOnce({ printerName: 'C', endedAt: 3 });
  const { result, rerender } = renderHook(({ id, key }) => useLastPrinter(id, key), { initialProps: { id: '1', key: '0' } });
  rerender({ id: '2', key: '0' });
  await waitFor(() => expect(result.current?.printerName).toBe('B'));
  await act(async () => resolveFirst({ printerName: 'A', endedAt: 1 }));
  expect(result.current?.printerName).toBe('B');
  rerender({ id: '2', key: '1' });
  await waitFor(() => expect(result.current?.printerName).toBe('C'));
});
it('hides errors and does not load without a selection', async () => {
  vi.mocked(getLastPrinterForFile).mockRejectedValueOnce(new Error('failed'));
  const { result, rerender } = renderHook(({ id }) => useLastPrinter(id), { initialProps: { id: null as string | null } });
  expect(getLastPrinterForFile).not.toHaveBeenCalled();
  rerender({ id: '1' });
  await act(async () => {});
  expect(result.current).toBeNull();
});
