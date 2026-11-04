import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useBundles } from './useBundles';
import type { Bundle } from '../types';

const gate = vi.hoisted(() => ({ canEdit: true }));
vi.mock('../lib/webSession', () => ({ canEditMetadata: () => gate.canEdit }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const bundle: Bundle = {
  id: 'b1', name: 'Vase', kind: 'parts', createdAt: '2026-11-01T00:00:00Z',
  members: [{ fileId: '1', position: 0, label: null, inTrash: false }, { fileId: '2', position: 1, label: 'Lid', inTrash: false }],
};

beforeEach(() => { vi.mocked(invoke).mockReset(); gate.canEdit = true; });

describe('useBundles', () => {
  it('loads the bundles on mount and indexes the files', async () => {
    vi.mocked(invoke).mockResolvedValue([bundle]);
    const { result } = renderHook(() => useBundles());
    await waitFor(() => expect(result.current.bundles).toEqual([bundle]));
    expect(invoke).toHaveBeenCalledWith('list_bundles');
    expect(result.current.bundleOfFile.get('2')).toBe(result.current.bundles[0]);
  });

  it('creates a bundle and reads the list again', async () => {
    vi.mocked(invoke).mockImplementation(command => Promise.resolve(command === 'list_bundles' ? [] : bundle));
    const { result } = renderHook(() => useBundles());
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('list_bundles'));
    vi.mocked(invoke).mockClear();
    let created: Bundle | undefined;
    await act(async () => { created = await result.current.createBundle('Vase', 'parts', ['1', '2']); });
    expect(created).toEqual(bundle);
    expect(vi.mocked(invoke).mock.calls.map(call => call[0])).toEqual(['create_bundle', 'list_bundles']);
    expect(invoke).toHaveBeenCalledWith('create_bundle', { name: 'Vase', kind: 'parts', fileIds: ['1', '2'] });
  });

  it('passes every other change on and refreshes after each', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const { result } = renderHook(() => useBundles());
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('list_bundles'));
    const calls: [() => Promise<unknown> | undefined, string, unknown][] = [
      [() => result.current.updateBundle('b1', { name: 'Neu' }), 'update_bundle', { bundleId: 'b1', name: 'Neu' }],
      [() => result.current.addFilesToBundle('b1', ['3']), 'add_files_to_bundle', { bundleId: 'b1', fileIds: ['3'] }],
      [() => result.current.removeFilesFromBundle(['3']), 'remove_files_from_bundle', { fileIds: ['3'] }],
      [() => result.current.dissolveBundle('b1'), 'dissolve_bundle', { bundleId: 'b1' }],
      [() => result.current.updateBundleMember('1', { makeTitle: true }), 'update_bundle_member', { fileId: '1', makeTitle: true }],
    ];
    for (const [run, command, args] of calls) {
      vi.mocked(invoke).mockClear();
      await act(async () => { await run(); });
      expect(vi.mocked(invoke).mock.calls.map(call => call[0])).toEqual([command, 'list_bundles']);
      expect(invoke).toHaveBeenCalledWith(command, args);
    }
  });

  it('changes nothing without the right to edit metadata', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const { result } = renderHook(() => useBundles());
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('list_bundles'));
    gate.canEdit = false;
    vi.mocked(invoke).mockClear();
    await act(async () => {
      expect(await result.current.createBundle('Vase', 'parts', ['1', '2'])).toBeUndefined();
      await result.current.dissolveBundle('b1');
    });
    expect(invoke).not.toHaveBeenCalled();
  });

  it('passes a failure on after reading the real state again', async () => {
    vi.mocked(invoke).mockImplementation(command => command === 'create_bundle' ? Promise.reject(new Error('bundleTooFewFiles')) : Promise.resolve([]));
    const { result } = renderHook(() => useBundles());
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('list_bundles'));
    vi.mocked(invoke).mockClear();
    await act(async () => {
      await expect(result.current.createBundle('Vase', 'parts', ['1'])).rejects.toThrow('bundleTooFewFiles');
    });
    expect(vi.mocked(invoke).mock.calls.map(call => call[0])).toEqual(['create_bundle', 'list_bundles']);
  });

  it('keeps the newest list when answers arrive out of order', async () => {
    const answers: ((value: Bundle[]) => void)[] = [];
    vi.mocked(invoke).mockImplementation(() => new Promise(resolve => { answers.push(resolve as (value: Bundle[]) => void); }));
    const { result } = renderHook(() => useBundles());
    await waitFor(() => expect(answers).toHaveLength(1));
    act(() => { void result.current.refreshBundles(); });
    await waitFor(() => expect(answers).toHaveLength(2));
    await act(async () => { answers[1]([bundle]); });
    await act(async () => { answers[0]([]); });
    expect(result.current.bundles).toEqual([bundle]);
  });
});
