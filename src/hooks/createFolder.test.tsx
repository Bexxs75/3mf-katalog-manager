import { act, renderHook } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { useFolderDragAndDrop } from './useFolderDragAndDrop';
import { formatAppError } from '../lib/errorMessage';
import { de } from '../i18n/de';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });
function setup() {
  const refresh = vi.fn();
  return { ...renderHook(() => useFolderDragAndDrop([], [], { refreshFolders: refresh, refreshFiles: refresh })), refresh };
}
it('ends a cancelled native folder dialog without a toast or follow-up backend calls', async () => {
  vi.mocked(invoke).mockResolvedValue(null);
  const { result, refresh } = setup();
  await act(async () => { await result.current.onCreateFolder(null, 'New'); });
  expect(result.current.moveToast).toBeNull();
  expect(refresh).not.toHaveBeenCalled();
  expect(invoke).toHaveBeenCalledExactlyOnceWith('create_folder', { parentId: null, name: 'New' });
});
it.each([
  [new Error('Permission denied'), de.folderCreateFailed],
  [{ message: 'folderNameEmpty', expected: true }, de.errFolderNameEmpty],
])('shows a translated creation failure', async (error, message) => {
  vi.mocked(invoke).mockRejectedValue(error);
  const { result, refresh } = setup();
  await act(async () => { await result.current.onCreateFolder(null, 'New'); });
  expect(formatAppError(key => de[key], result.current.moveToast!.failure!)).toBe(message);
  expect(refresh).not.toHaveBeenCalled();
});
