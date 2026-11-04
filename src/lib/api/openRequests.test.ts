import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { addExternalToCatalog, releaseExternalSource } from './openRequests';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => vi.mocked(invoke).mockReset().mockResolvedValue(undefined));

it('names sources by id only, never by path', async () => {
  await addExternalToCatalog('ext-1', '12');
  expect(invoke).toHaveBeenCalledWith('add_external_to_catalog', { sourceId: 'ext-1', folderId: '12' });
  await releaseExternalSource('ext-1');
  expect(invoke).toHaveBeenCalledWith('release_external_source', { sourceId: 'ext-1' });
});
