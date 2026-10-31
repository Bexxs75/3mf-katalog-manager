import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { authenticationFailed, login } from '../lib/webSession';
import { useCatalogStore } from './useCatalogStore';
import { makeModelFile, makeModelFileSummary } from '../test/factories';
import { clearMetadataError, getMetadataError } from '../lib/metadataErrors';

const live = makeModelFile({ id: '1' });
const trashed = makeModelFile({ id: '2', deletedAt: '2026-10-01T00:00:00Z' });
let commands: string[]; let viewedAnswer: () => Response;
beforeEach(async () => {
  commands = []; clearMetadataError();
  viewedAnswer = () => Response.json({ message: 'Eintrag nicht gefunden', expected: true }, { status: 404 });
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    if (!url.startsWith('/api/invoke/')) return Response.json({ authenticated: true, writable: true, csrfToken: 't', mustChangePassword: false });
    const command = url.split('/').pop()!; commands.push(command);
    if (command === 'mark_file_viewed') return viewedAnswer();
    return Response.json(({ list_file_summaries: [makeModelFileSummary(live)], list_trash: [trashed], list_all_file_tags: {}, list_files_by_ids: [live, trashed] } as Record<string, unknown>)[command] ?? []);
  }));
  await login('pw');
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); });

it('does not stamp a trashed entry as viewed', async () => {
  const { result } = renderHook(() => useCatalogStore());
  await waitFor(() => expect(result.current.trashModels).toHaveLength(1));
  commands.length = 0;
  await act(async () => result.current.selectModel('2'));
  expect(commands).not.toContain('mark_file_viewed');
  expect(result.current.selectedId).toBe('2');
});
it('keeps a refused last-viewed update out of the error banner', async () => {
  vi.spyOn(console, 'error').mockImplementation(() => {});
  const { result } = renderHook(() => useCatalogStore());
  await waitFor(() => expect(result.current.models).toHaveLength(1));
  await act(async () => result.current.selectModel('1'));
  await waitFor(() => expect(commands).toContain('mark_file_viewed'));
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
  expect(getMetadataError()).toBe('');
});
