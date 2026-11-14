import { beforeEach, describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useDuplicateRemoval } from './useDuplicateRemoval';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); vi.mocked(invoke).mockResolvedValue(undefined); });

const run = (ids: string[], refetch: (ids: string[]) => Promise<readonly { id: string }[]> | void) =>
  renderHook(() => useDuplicateRemoval(refetch)).result.current(ids);

describe('useDuplicateRemoval', () => {
  it('deletes the ids, reloads the list and reports nothing left when they are gone', async () => {
    const refetch = vi.fn().mockResolvedValue([{ id: 'keep' }]);
    await expect(run(['a', 'b'], refetch)).resolves.toEqual([]);
    expect(invoke).toHaveBeenCalledWith('delete_files', { fileIds: ['a', 'b'] });
    expect(refetch).toHaveBeenCalledWith(['a', 'b']);
  });

  it('reports the ids the backend skipped (locked or read-only files)', async () => {
    await expect(run(['a', 'b', 'c'], vi.fn().mockResolvedValue([{ id: 'b' }, { id: 'keep' }]))).resolves.toEqual(['b']);
  });

  it('reloads the list after a failed call and then shows that failure', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('disk gone'));
    const refetch = vi.fn().mockRejectedValue(new Error('reload failed too'));
    await expect(run(['a'], refetch)).rejects.toThrow('disk gone');
    expect(refetch).toHaveBeenCalled();
  });

  it('does nothing for an empty list', async () => {
    const refetch = vi.fn();
    await expect(run([], refetch)).resolves.toEqual([]);
    expect(invoke).not.toHaveBeenCalled();
    expect(refetch).not.toHaveBeenCalled();
  });
});
