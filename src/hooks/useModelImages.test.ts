import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { listFileImages } from '../lib/api/files';
import { clearModelImages, getModelImages, invalidateModelImages, useModelImages } from './useModelImages';
vi.mock('../lib/api/files', () => ({ listFileImages: vi.fn() }));
beforeEach(() => {
  clearModelImages();
  vi.mocked(listFileImages).mockReset().mockImplementation(async ids => ids.map(id => ({ id, thumbnailImage: id, renderSnapshotImage: null, customImage: null })));
});
it('loads only requested IDs in batches of 50', async () => {
  const ids = Array.from({ length: 121 }, (_, i) => String(i));
  const { result } = renderHook(() => useModelImages(ids));
  await waitFor(() => expect(result.current.get('120')?.thumbnailImage).toBe('120'));
  expect(vi.mocked(listFileImages).mock.calls.map(([batch]) => batch.length)).toEqual([50, 50, 21]);
});
it('deduplicates consumers and reloads invalidated images', async () => {
  const first = renderHook(() => useModelImages(['a']));
  renderHook(() => useModelImages(['a']));
  await waitFor(() => expect(first.result.current.has('a')).toBe(true));
  expect(listFileImages).toHaveBeenCalledTimes(1);
  vi.mocked(listFileImages).mockResolvedValue([{ id: 'a', thumbnailImage: 'new', renderSnapshotImage: null, customImage: null }]);
  act(() => invalidateModelImages('a'));
  await waitFor(() => expect(first.result.current.get('a')?.thumbnailImage).toBe('new'));
});
it('keeps at most 600 entries, evicting the oldest', async () => {
  const ids = Array.from({ length: 650 }, (_, i) => String(i));
  renderHook(() => useModelImages(ids));
  await waitFor(() => expect(getModelImages('649')).toBeDefined());
  expect(getModelImages('0')).toBeUndefined();
  expect(getModelImages('49')).toBeUndefined();
  expect(getModelImages('50')).toBeDefined();
});
it('discards an invalidated in-flight response and fetches the replacement', async () => {
  let complete!: (value: Awaited<ReturnType<typeof listFileImages>>) => void;
  vi.mocked(listFileImages).mockImplementationOnce(() => new Promise(resolve => { complete = resolve; }));
  const { result } = renderHook(() => useModelImages(['racing']));
  await waitFor(() => expect(complete).toBeDefined());
  act(() => invalidateModelImages('racing'));
  act(() => complete([{ id: 'racing', thumbnailImage: 'old', renderSnapshotImage: null, customImage: null }]));
  await waitFor(() => expect(result.current.get('racing')?.thumbnailImage).toBe('racing'));
  expect(listFileImages).toHaveBeenCalledTimes(2);
});
it('cancels an idle request when its consumer unmounts', async () => {
  let idle!: IdleRequestCallback;
  const cancel = vi.fn();
  vi.stubGlobal('requestIdleCallback', (callback: IdleRequestCallback) => { idle = callback; return 7; });
  vi.stubGlobal('cancelIdleCallback', cancel);
  const { unmount } = renderHook(() => useModelImages(['cancelled']));
  unmount();
  expect(cancel).toHaveBeenCalledWith(7);
  act(() => idle({ didTimeout: false, timeRemaining: () => 20 }));
  expect(listFileImages).not.toHaveBeenCalled();
  vi.unstubAllGlobals();
});
