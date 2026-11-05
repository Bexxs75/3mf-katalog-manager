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

  describe('composite changes', () => {
    async function ready() {
      vi.mocked(invoke).mockImplementation(command => Promise.resolve(command === 'create_bundle' ? bundle : []));
      const view = renderHook(() => useBundles());
      await waitFor(() => expect(invoke).toHaveBeenCalledWith('list_bundles'));
      vi.mocked(invoke).mockClear();
      return view;
    }
    const commands = () => vi.mocked(invoke).mock.calls.map(call => [call[0], call[1]]);

    it('creates a bundle, labels only the members that have a label, and reads the list once at the end', async () => {
      const { result } = await ready();
      await act(async () => { await result.current.createLabelledBundle('Vase', 'parts', [{ fileId: '1', label: '' }, { fileId: '2', label: 'Lid' }]); });
      expect(commands()).toEqual([
        ['create_bundle', { name: 'Vase', kind: 'parts', fileIds: ['1', '2'] }],
        ['update_bundle_member', { fileId: '2', label: 'Lid' }],
        ['list_bundles', undefined],
      ]);
    });

    it('restores a dissolved bundle in one backend call with every member, those in the trash included', async () => {
      const { result } = await ready();
      let restored: Awaited<ReturnType<typeof result.current.restoreBundle>>;
      await act(async () => {
        restored = await result.current.restoreBundle({
          name: 'Vase', kind: 'variants',
          members: [{ fileId: '2', label: 'b', inTrash: false }, { fileId: '1', label: '', inTrash: false }, { fileId: '7', label: 'x', inTrash: true }],
        });
      });
      expect(commands()).toEqual([
        ['create_bundle', { name: 'Vase', kind: 'variants', fileIds: ['2', '1', '7'], labels: ['b', '', 'x'], restore: true }],
        ['list_bundles', undefined],
      ]);
      expect(restored!).toEqual({ bundle, left: 1 });
    });

    it('saves an edit in the order removals, name and kind, title swaps, labels', async () => {
      const { result } = await ready();
      await act(async () => {
        await result.current.saveBundleEdit('b1', { dissolve: false, name: 'New', kind: 'variants', removeIds: ['3'], titleSteps: ['2'], labelChanges: [{ fileId: '1', label: 'x' }] });
      });
      expect(commands().map(call => call[0])).toEqual(['update_bundle', 'remove_files_from_bundle', 'update_bundle_member', 'update_bundle_member', 'list_bundles']);
      expect(commands()[0][1]).toMatchObject({ bundleId: 'b1', name: 'New', kind: 'variants' });
      expect(commands()[2][1]).toEqual({ fileId: '2', makeTitle: true });
      expect(commands()[3][1]).toEqual({ fileId: '1', label: 'x' });
    });

    it('dissolves instead of editing when the plan says so, and asks for nothing else', async () => {
      const { result } = await ready();
      await act(async () => { await result.current.saveBundleEdit('b1', { dissolve: true, removeIds: ['2'], titleSteps: [], labelChanges: [] }); });
      expect(commands().map(call => call[0])).toEqual(['dissolve_bundle', 'list_bundles']);
    });

    it('dissolves several bundles one after the other', async () => {
      const { result } = await ready();
      await act(async () => { await result.current.dissolveBundles(['b1', 'b2']); });
      expect(commands().map(call => [call[0], (call[1] as { bundleId?: string } | undefined)?.bundleId])).toEqual([
        ['dissolve_bundle', 'b1'], ['dissolve_bundle', 'b2'], ['list_bundles', undefined],
      ]);
    });

    it('does nothing without the right to edit', async () => {
      const { result } = await ready();
      gate.canEdit = false;
      await act(async () => {
        await result.current.saveBundleEdit('b1', { dissolve: true, removeIds: [], titleSteps: [], labelChanges: [] });
        await result.current.createLabelledBundle('V', 'parts', [{ fileId: '1', label: 'a' }, { fileId: '2', label: '' }]);
      });
      expect(invoke).not.toHaveBeenCalled();
    });
  });
});
