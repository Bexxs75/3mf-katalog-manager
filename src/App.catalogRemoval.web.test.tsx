import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { makeModelFile, makeModelFileSummary } from './test/factories';
import { login, authenticationFailed } from './lib/webSession';
import { clearMetadataError } from './lib/metadataErrors';
import { MetadataError } from './components/MetadataError';
import { de } from './i18n/de';
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));
vi.mock('./components/WebBackgroundSnapshots', () => ({ WebBackgroundSnapshots: () => null }));
class Events { close() {} addEventListener() {} }
const models = [makeModelFile({ id: '1', name: 'Cube.stl' }), makeModelFile({ id: '2', name: 'Sphere.stl' })];
let removed: string[];
let calls: { command: string; args: Record<string, unknown> }[];
let failures: { status: number; retry?: string }[];
let writable: boolean;
let failRefresh: boolean;
let uploadReads: number;
const fetchMock = vi.fn(async (url: string, options?: RequestInit) => {
  if (url === '/api/uploads') { uploadReads++; return Response.json([]); }
  if (!url.startsWith('/api/invoke/')) return Response.json({ authenticated: true, writable, uploadable: true, csrfToken: 'token', mustChangePassword: false });
  const command = url.split('/').pop()!;
  const args = JSON.parse(String(options?.body ?? '{}'));
  calls.push({ command, args });
  if (command === 'remove_files_from_catalog') {
    const failure = failures.shift();
    if (failure) return Response.json({ message: `failure ${failure.status}`, expected: true }, { status: failure.status, headers: failure.retry !== undefined ? { 'Retry-After': failure.retry } : {} });
    removed = args.fileIds;
    return Response.json(removed.length);
  }
  if (command === 'list_folders' && removed.length && failRefresh) throw new Error('refresh failed');
  const remaining = models.filter(m => !removed.includes(m.id));
  const results: Record<string, unknown> = {
    list_file_summaries: remaining.map(makeModelFileSummary), list_files_by_ids: remaining,
    list_all_file_tags: {}, get_file_images: {}, get_file_images_batch: [],
    get_last_printer_for_file: null, has_step_preview: false,
  };
  return Response.json(results[command] ?? []);
});
beforeEach(async () => {
  localStorage.clear(); localStorage.setItem('3mf-katalog-setup-seen', '1');
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ width: 800, height: 400, top: 0, left: 0, right: 800, bottom: 400, x: 0, y: 0, toJSON: () => ({}) });
  vi.stubGlobal('fetch', fetchMock); vi.stubGlobal('EventSource', Events);
  removed = []; calls = []; failures = []; writable = true; failRefresh = false; uploadReads = 0; clearMetadataError();
  await login('password');
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
async function start() {
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><MetadataError /><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  await screen.findAllByText('Cube.stl');
}
const removals = () => calls.filter(c => c.command === 'remove_files_from_catalog');
async function openContext() {
  fireEvent.contextMenu((await screen.findAllByText('Cube.stl'))[0]);
  fireEvent.click(within(document.querySelector('[data-navigation-menu]') as HTMLElement).getByRole('button', { name: de.removeCatalog }));
}
it.each(['grid', 'list'])('removes through %s context menu only after confirmation and refreshes all views', async view => {
  await start();
  if (view === 'list') fireEvent.click(screen.getByRole('button', { name: de.viewList }));
  calls = []; const previousUploads = uploadReads;
  await openContext();
  expect(removals()).toHaveLength(0);
  expect(screen.getByText(/Offene Uploads/)).toBeVisible();
  fireEvent.mouseDown(screen.getByRole('button', { name: de.removeEntry }));
  fireEvent.click(screen.getByRole('button', { name: de.removeEntry }));
  await waitFor(() => expect(screen.queryAllByText('Cube.stl')).toHaveLength(0));
  expect(removals()).toEqual([{ command: 'remove_files_from_catalog', args: { fileIds: ['1'] } }]);
  for (const command of ['list_file_summaries', 'list_folders', 'list_tag_counts', 'list_collections']) expect(calls.some(c => c.command === command)).toBe(true);
  await waitFor(() => expect(uploadReads).toBeGreaterThan(previousUploads));
  expect(screen.getAllByText('Sphere.stl').length).toBeGreaterThan(0);
});
it('removes a bulk selection with one request after confirmation', async () => {
  await start();
  fireEvent.keyDown(window, { key: 'a', ctrlKey: true });
  fireEvent.click(screen.getByRole('button', { name: new RegExp(de.bulkMoreLabel) }));
  fireEvent.click(screen.getByRole('menuitem', { name: de.removeCatalogShort }));
  expect(removals()).toHaveLength(0);
  fireEvent.mouseDown(screen.getByRole('button', { name: de.removeEntry }));
  fireEvent.click(screen.getByRole('button', { name: de.removeEntry }));
  await waitFor(() => expect(screen.queryAllByText('Cube.stl')).toHaveLength(0));
  expect(removals()).toEqual([{ command: 'remove_files_from_catalog', args: { fileIds: ['1', '2'] } }]);
  expect(screen.queryByRole('button', { name: de.removeCatalogShort })).toBeNull();
});
it.each([404, 503, 429, 401])('shows %s and retains models on failure', async status => {
  await start(); failures = [{ status }];
  await openContext(); fireEvent.mouseDown(screen.getByRole('button', { name: de.removeEntry }));
  fireEvent.click(screen.getByRole('button', { name: de.removeEntry }));
  await waitFor(() => expect(screen.getAllByRole('alert').some(el => el.textContent?.includes(`failure ${status}`))).toBe(true));
  expect(screen.getAllByText('Cube.stl').length).toBeGreaterThan(0);
  expect(removals()).toHaveLength(1);
});
it.each([503, 429])('retries %s with Retry-After once, then reports failure', async status => {
  await start(); failures = [{ status, retry: '0' }, { status, retry: '0' }];
  await openContext(); fireEvent.mouseDown(screen.getByRole('button', { name: de.removeEntry }));
  fireEvent.click(screen.getByRole('button', { name: de.removeEntry }));
  await waitFor(() => expect(removals()).toHaveLength(2));
  await waitFor(() => expect(screen.getAllByRole('alert').some(el => el.textContent?.includes(`failure ${status}`))).toBe(true));
  expect(screen.getAllByText('Cube.stl').length).toBeGreaterThan(0);
});
it('reports refresh failure after successful removal', async () => {
  await start(); failRefresh = true;
  await openContext(); fireEvent.mouseDown(screen.getByRole('button', { name: de.removeEntry }));
  fireEvent.click(screen.getByRole('button', { name: de.removeEntry }));
  await waitFor(() => expect(screen.getAllByRole('alert').some(el => el.textContent?.includes('refresh failed'))).toBe(true));
});
it('does not offer removal or issue a request without write mode', async () => {
  writable = false; await login('password'); await start();
  fireEvent.contextMenu(screen.getAllByText('Cube.stl')[0]);
  expect(screen.queryByRole('button', { name: de.removeCatalog })).toBeNull();
  fireEvent.keyDown(window, { key: 'Escape' });
  fireEvent.keyDown(window, { key: 'a', ctrlKey: true });
  expect(screen.queryByRole('button', { name: de.removeCatalogShort })).toBeNull();
  expect(removals()).toHaveLength(0);
});

it.each(['compact', 'comfort', 'page'])('removes from the %s detail entry only after confirmation', async surface => {
  localStorage.setItem('3mf-katalog-density', surface === 'comfort' ? 'comfort' : 'compact');
  await start();
  if (surface === 'page') fireEvent.doubleClick(screen.getAllByText('Cube.stl')[0]);
  else fireEvent.click(screen.getAllByText('Cube.stl')[0]);
  const button = await screen.findByRole('button', { name: de.removeCatalog });
  fireEvent.click(button);
  expect(removals()).toHaveLength(0);
  fireEvent.click(screen.getByRole('button', { name: de.cancel }));
  expect(removals()).toHaveLength(0);
  fireEvent.click(button);
  fireEvent.mouseDown(screen.getByRole('button', { name: de.removeEntry }));
  fireEvent.click(screen.getByRole('button', { name: de.removeEntry }));
  await waitFor(() => expect(screen.queryAllByText('Cube.stl')).toHaveLength(0));
  expect(removals()).toEqual([{ command: 'remove_files_from_catalog', args: { fileIds: ['1'] } }]);
});
it('hides detail removal when write permission is revoked with confirmation open', async () => {
  await start(); fireEvent.click(screen.getAllByText('Cube.stl')[0]);
  fireEvent.click(await screen.findByRole('button', { name: de.removeCatalog }));
  await act(async () => { writable = false; await login('password'); });
  expect(screen.queryByRole('button', { name: de.removeEntry })).toBeNull();
  expect(screen.queryByRole('button', { name: de.removeCatalog })).toBeNull();
  expect(removals()).toHaveLength(0);
});
it('Escape cancels context confirmation and leaves the menu usable without a request', async () => {
  await start(); await openContext();
  fireEvent.keyDown(screen.getByRole('button', { name: de.cancel }), { key: 'Escape' });
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(document.querySelector('[data-navigation-menu]')).not.toBeNull();
  expect(removals()).toHaveLength(0);
});
