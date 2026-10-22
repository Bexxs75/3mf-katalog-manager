import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { WebImport } from './WebImport';
import { Header } from './Header';
import { WebAuthGate } from './WebAuthGate';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import * as session from '../lib/webSession';
class Events {
  static latest: Events;
  handlers = new Map<string, (event: MessageEvent) => void>();
  constructor() { Events.latest = this; }
  close() {} addEventListener(name: string, fn: (event: MessageEvent) => void) { this.handlers.set(name, fn); }
  emit(value: unknown) { this.handlers.get('import-progress')?.({ data: JSON.stringify(value) } as MessageEvent); }
}
afterEach(() => { cleanup(); session.authenticationFailed(401); vi.unstubAllGlobals(); });
async function start(uploadable: boolean) {
  vi.stubGlobal('EventSource', Events);
  vi.stubGlobal('fetch', vi.fn(async (url: string) => Response.json(url === '/api/session'
    ? { authenticated: true, writable: true, uploadable, csrfToken: 'csrf', mustChangePassword: false } : [])));
  await session.getSession();
  render(<LanguageProvider><WebImport open onClose={() => {}} onImported={() => {}} /></LanguageProvider>);
}
it('hides the entire upload surface without uploadable even when writable', async () => {
  await start(false); expect(screen.queryByRole('dialog')).toBeNull(); expect(document.querySelector('input[type=file]')).toBeNull();
});
it('uses the browser file chooser and rejects archive selections and dropped directories visibly', async () => {
  await start(true);
  const input = await screen.findByLabelText(de.webUploadChoose);
  expect(input).toHaveAttribute('multiple'); expect(input).toHaveAttribute('accept', '.3mf,.stl,.obj,.stp,.step');
  fireEvent.change(input, { target: { files: [new File(['a'], 'archive.zip')] } });
  expect(screen.getByText(de.webUploadArchives)).toBeVisible();
  fireEvent.drop(screen.getByTestId('web-upload-drop'), { dataTransfer: { items: [{ kind: 'file', webkitGetAsEntry: () => ({ isDirectory: true }) }], files: [] } });
  expect(screen.getByRole('alert')).toHaveTextContent(de.webUploadFolders);
});
it('offers import only for uploaded entries and discard for duplicates and invalid files', async () => {
  await start(true);
  await act(async () => Events.latest.emit({ snapshot: true, jobs: [], uploads: [
    { uploadId: 'a', name: 'ready.stl', state: 'uploaded', size: 12, time: 1, result: null },
    { uploadId: 'b', name: 'duplicate.stl', state: 'duplicate', size: 12, time: 1, result: null },
    { uploadId: 'c', name: 'invalid.stl', state: 'invalid', size: 12, time: 1, result: null },
  ] }));
  expect(screen.getAllByRole('button', { name: de.webUploadDiscard })).toHaveLength(3);
  expect(screen.getAllByRole('button', { name: de.webUploadImport })).toHaveLength(1);
});
it('shows partial results after cancellation and reloads the catalog once', async () => {
  await start(true);
  await act(async () => Events.latest.emit({ jobId: 'job', state: 'cancelled', total: 2, done: 1, current: null, kind: null, files: [
    { uploadId: 'a', name: 'done.stl', state: 'imported', fileId: '1', existingFileId: null, kind: null },
    { uploadId: 'b', name: 'waiting.stl', state: 'uploaded', fileId: null, existingFileId: null, kind: 'not_started' },
  ] }));
  await waitFor(() => expect(screen.getByRole('dialog', { name: de.webImportResultTitle })).toBeVisible());
  expect(screen.getByText('done.stl')).toBeVisible(); expect(screen.getByText('waiting.stl')).toBeVisible();
  expect(screen.getByText(de.webUploadPartial)).toBeVisible();
});
it('does not interrupt navigation with historical results from a reconnect snapshot', async () => {
  await start(true);
  await act(async () => Events.latest.emit({ snapshot: true, jobs: [{ jobId: 'old', state: 'completed', current: null, total: 1, done: 1, files: [], kind: null }], uploads: [] }));
  expect(screen.queryByRole('dialog', { name: de.webImportResultTitle })).toBeNull();
  expect(screen.getByRole('button', { name: de.webUploadResults })).toBeVisible();
});

it('shows the browser import entry only with permission and never offers native folder import', async () => {
  await start(false); cleanup();
  const open = vi.fn();
  render(<LanguageProvider><Header mainView="catalog" view="grid" sort="name" sortDirection="asc" count={0}
    onSortDirectionChange={() => {}} onViewChange={() => {}} onSortChange={() => {}} allFoldersCollapsed={false} onToggleAllFolders={() => {}}
    onCheckSourceFolders={() => {}} onImportFiles={open} onImportFolder={() => { throw new Error('native folder'); }} onImportFolderContents={() => { throw new Error('native contents'); }} /></LanguageProvider>);
  expect(screen.queryByRole('button', { name: de.webUploadTitle })).toBeNull();
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ authenticated: true, writable: true, uploadable: true, csrfToken: 'new', mustChangePassword: false })));
  await act(async () => { await session.getSession(); });
  fireEvent.click(screen.getByRole('button', { name: de.webUploadTitle })); expect(open).toHaveBeenCalledOnce();
  expect(screen.queryByText(de.importFolderOption)).toBeNull(); expect(screen.queryByText(de.importFolderContentsOption)).toBeNull();
});
it('explains interrupted uploads on the login screen after session expiry', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ authenticated: true, writable: true, uploadable: true, csrfToken: 'new', mustChangePassword: false })));
  render(<LanguageProvider><WebAuthGate><div>Catalog content</div></WebAuthGate></LanguageProvider>);
  await screen.findByText('Catalog content');
  act(() => session.authenticationFailed(401));
  expect(screen.getByRole('alert')).toHaveTextContent(de.webUploadSessionEnded);
  expect(screen.getByRole('button', { name: de.webLogin })).toBeVisible();
});
