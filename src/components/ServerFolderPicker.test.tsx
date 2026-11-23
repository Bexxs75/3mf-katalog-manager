import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ServerFolderPicker, type ServerFolderPickerProps } from './ServerFolderPicker';
import * as api from '../lib/api/sourceFolders';
import * as sourceImport from '../lib/api/sourceImport';
import type { SourceImportJob } from '../lib/api/sourceImport';
import { de } from '../i18n/de';
import type { SourceDirListing } from '../types';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), warn: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined), debug: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/sourceFolders');
vi.mock('../lib/api/sourceImport', async importOriginal => ({
  ...await importOriginal<typeof import('../lib/api/sourceImport')>(),
  startSourceImport: vi.fn(), getSourceImportJob: vi.fn(), cancelSourceImport: vi.fn(),
}));

const dir = (rel: string, fileCount: number, truncated = false) => ({ rel, name: rel.split('/').pop()!, directory: true, fileCount, truncated });
const roots = [{ rootIdx: 0, name: 'Downloads' }, { rootIdx: 1, name: 'NAS' }];
const listing = (rootIdx: number, rel: string, entries: SourceDirListing['entries'], truncated = false): SourceDirListing => ({ roots, rootIdx, rel, entries, truncated });

function mockTree() {
  vi.mocked(api.listSourceDirs).mockImplementation(async (rootIdx?: number, rel = '') => {
    if (rootIdx === undefined) return { roots, entries: [], truncated: false };
    if (rootIdx === 0 && rel === '') return listing(0, '', [dir('Dragons', 18), dir('Mounts', 24)]);
    if (rootIdx === 0 && rel === 'Dragons') return listing(0, 'Dragons', []);
    if (rootIdx === 1 && rel === '') return listing(1, '', [dir('Tech', 9, true)], true);
    return listing(rootIdx, rel, []);
  });
}
function setup(props: Partial<ServerFolderPickerProps> = {}) {
  const onClose = vi.fn();
  const onRemember = vi.fn().mockResolvedValue(undefined);
  const opener = document.createElement('button'); document.body.appendChild(opener); opener.focus();
  const result = render(<LanguageProvider><ServerFolderPicker onClose={onClose} onRemember={onRemember} {...props} /></LanguageProvider>);
  return { onClose, onRemember, opener, ...result };
}
const tree = () => screen.findByRole('tree', { name: de.sfpTreeLabel });

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(api.listSourceDirs).mockReset();
  document.body.innerHTML = '';
});

describe('ServerFolderPicker', () => {
  it('lists the roots as a tree, focuses it and describes the active item', async () => {
    mockTree();
    setup();
    const t = await tree();
    await waitFor(() => expect(t).toHaveFocus());
    const items = within(t).getAllByRole('treeitem');
    expect(items[0]).toHaveAttribute('aria-level', '1');
    expect(items[0]).toHaveAttribute('aria-posinset', '1');
    expect(items[0]).toHaveAttribute('aria-setsize', '2');
    expect(t).toHaveAttribute('aria-activedescendant', items[0].id);
    expect(items[0]).toHaveAttribute('aria-selected', 'false');
  });

  it('opens and closes with the arrow keys and moves with up and down', async () => {
    mockTree();
    setup();
    const t = await tree();
    const first = () => within(t).getAllByRole('treeitem')[0];
    fireEvent.keyDown(t, { key: 'ArrowRight' });
    await waitFor(() => expect(within(t).getAllByRole('treeitem')).toHaveLength(4));
    expect(first()).toHaveAttribute('aria-expanded', 'true');
    expect(api.listSourceDirs).toHaveBeenCalledWith(0, '');
    fireEvent.keyDown(t, { key: 'ArrowRight' }); // already open: first child
    expect(t).toHaveAttribute('aria-activedescendant', within(t).getAllByRole('treeitem')[1].id);
    expect(within(t).getAllByRole('treeitem')[1]).toHaveAttribute('aria-level', '2');
    fireEvent.keyDown(t, { key: 'ArrowDown' });
    expect(t).toHaveAttribute('aria-activedescendant', within(t).getAllByRole('treeitem')[2].id);
    fireEvent.keyDown(t, { key: 'ArrowLeft' }); // leaf-ish: to parent
    expect(t).toHaveAttribute('aria-activedescendant', first().id);
    fireEvent.keyDown(t, { key: 'ArrowLeft' }); // close
    expect(first()).toHaveAttribute('aria-expanded', 'false');
    expect(within(t).getAllByRole('treeitem')).toHaveLength(2);
  });

  it('selects with Enter, names the count and offers the folder to the caller', async () => {
    mockTree();
    const { onRemember } = setup();
    const t = await tree();
    fireEvent.keyDown(t, { key: 'ArrowRight' });
    await waitFor(() => expect(within(t).getAllByRole('treeitem')).toHaveLength(4));
    fireEvent.keyDown(t, { key: 'ArrowDown' });
    fireEvent.keyDown(t, { key: 'Enter' });
    const dragons = within(t).getByRole('treeitem', { name: /Dragons/ });
    expect(dragons).toHaveAttribute('aria-selected', 'true');
    expect(dragons).toHaveAccessibleName(/18 unterstützte Dateien/);
    expect(screen.getByText(/Enthält 18 unterstützte Dateien/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: de.sfpRemember }));
    await waitFor(() => expect(onRemember).toHaveBeenCalledWith(0, 'Dragons'));
    expect(await screen.findByText(de.sfpRememberedNote.replace('{name}', 'Dragons'))).toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.sfpRemembered })).toBeDisabled();
    await waitFor(() => expect(t).toHaveFocus());
  });

  it('marks truncated counts as minimums and warns about the shortened list', async () => {
    mockTree();
    setup();
    const t = await tree();
    fireEvent.click(within(t).getByRole('treeitem', { name: /NAS/ }));
    fireEvent.keyDown(t, { key: 'ArrowDown' });
    fireEvent.keyDown(t, { key: 'ArrowRight' });
    expect(await screen.findByText(de.sfpTruncatedTitle)).toBeInTheDocument();
    expect(within(t).getByRole('treeitem', { name: /Tech/ })).toHaveAccessibleName(/mindestens 9 unterstützte Dateien/);
  });

  it('closes with Escape and gives focus back to the opener', async () => {
    mockTree();
    const { onClose } = setup();
    await tree();
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('shows the translated backend error, blocks the actions and reloads', async () => {
    mockTree();
    setup();
    const t = await tree();
    fireEvent.keyDown(t, { key: 'Enter' });
    vi.mocked(api.listSourceDirs).mockRejectedValueOnce({ message: 'sourceRootNotAllowed', expected: true });
    fireEvent.keyDown(t, { key: 'ArrowRight' });
    expect(await screen.findByRole('alert')).toHaveTextContent('Dieser Quellordner ist nicht freigegeben.');
    expect(screen.getByRole('button', { name: de.sfpRemember })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: de.sfpReload }));
    await waitFor(() => expect(within(t).getAllByRole('treeitem').length).toBeGreaterThan(2));
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('explains that no root is shared and offers only closing', async () => {
    vi.mocked(api.listSourceDirs).mockResolvedValue({ roots: [], entries: [], truncated: false });
    const { onClose } = setup();
    expect(await screen.findByText(de.sfpNoRoots)).toBeInTheDocument();
    expect(screen.queryByRole('tree')).toBeNull();
    const close = screen.getAllByRole('button', { name: de.sfpClose }).find(b => b.textContent === de.sfpClose)!;
    await waitFor(() => expect(close).toHaveFocus());
    fireEvent.click(close);
    expect(onClose).toHaveBeenCalled();
  });

  it('shows a failure to list the roots with a reload button', async () => {
    mockTree();
    vi.mocked(api.listSourceDirs).mockRejectedValueOnce({ message: 'sourceRootNotAllowed', expected: true });
    setup();
    expect(await screen.findByRole('alert')).toHaveTextContent('nicht freigegeben');
    fireEvent.click(screen.getByRole('button', { name: de.sfpReload }));
    expect(await tree()).toBeInTheDocument();
  });

  it('hides the copy import unless the caller provides it', async () => {
    mockTree();
    const { unmount } = setup();
    await tree();
    expect(screen.queryByRole('button', { name: de.sfpImport })).toBeNull();
    unmount();
    setup({ copyImport: { allowed: true } });
    await tree();
    expect(screen.getByRole('button', { name: de.sfpImport })).toBeInTheDocument();
  });

  describe('copy import', () => {
    const job = (patch: Partial<SourceImportJob>): SourceImportJob => ({ jobId: 'j1', state: 'importing', current: null, total: 0, done: 0, files: [], kind: null, ...patch });
    const file = (name: string, state: string) => ({ uploadId: name, name, state, fileId: null, existingFileId: null, kind: null });
    // The title bar's × carries the same name; the footer button comes last.
    const footerClose = () => { const all = screen.getAllByRole('button', { name: de.sfpClose }); return all[all.length - 1]; };
    const advance = (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });
    async function startCopy(props: Partial<ServerFolderPickerProps> = {}) {
      mockTree();
      const view = setup({ copyImport: { allowed: true, onFinished: vi.fn() }, ...props });
      const t = await tree();
      fireEvent.keyDown(t, { key: 'ArrowRight' });
      await waitFor(() => expect(within(t).getAllByRole('treeitem')).toHaveLength(4));
      fireEvent.keyDown(t, { key: 'ArrowDown' });
      fireEvent.keyDown(t, { key: 'Enter' });
      return view;
    }
    beforeEach(() => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      vi.mocked(sourceImport.startSourceImport).mockReset().mockResolvedValue({ jobId: 'j1' });
      vi.mocked(sourceImport.getSourceImportJob).mockReset();
      vi.mocked(sourceImport.cancelSourceImport).mockReset();
    });
    afterEach(() => { vi.useRealTimers(); });

    it('starts the copy of the chosen folder with root index and path and shows the progress', async () => {
      const onFinished = vi.fn();
      await startCopy({ copyImport: { allowed: true, onFinished } });
      vi.mocked(sourceImport.getSourceImportJob).mockResolvedValue(job({ total: 18, done: 5, current: 'Fafnir.3mf' }));
      fireEvent.click(screen.getByRole('button', { name: de.sfpImport }));
      await waitFor(() => expect(sourceImport.startSourceImport).toHaveBeenCalledWith(0, 'Dragons'));
      const cancel = await screen.findByRole('button', { name: de.sfiStop });
      await waitFor(() => expect(cancel).toHaveFocus());
      expect(screen.getByText(de.sfpCopyNote)).toBeInTheDocument();
      await advance(700);
      expect(await screen.findByText('5 von 18 Dateien kopiert')).toBeInTheDocument();
      expect(screen.getByText('Aktuell: Fafnir.3mf')).toBeInTheDocument();
      expect(screen.getByRole('progressbar', { name: de.sfiBarLabel })).toHaveAttribute('aria-valuenow', '28');
      expect(screen.queryByRole('tree')).toBeNull();
      expect(onFinished).not.toHaveBeenCalled();
    });

    it('does not close while running; at the end it summarises, reloads and hands focus to Close', async () => {
      const onFinished = vi.fn();
      const { onClose } = await startCopy({ copyImport: { allowed: true, onFinished } });
      vi.mocked(sourceImport.getSourceImportJob).mockResolvedValueOnce(job({ total: 3, done: 1 })).mockResolvedValue(job({
        state: 'completed', total: 3, done: 3, files: [file('a.3mf', 'imported'), file('b.3mf', 'duplicate'), file('c.stl', 'invalid')],
      }));
      fireEvent.click(screen.getByRole('button', { name: de.sfpImport }));
      await screen.findByRole('button', { name: de.sfiStop });
      fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
      expect(onClose).not.toHaveBeenCalled();
      await advance(700); await advance(700);
      const done = await screen.findByText(de.sfiDoneTitle);
      expect(done.parentElement).toHaveTextContent('1 Datei kopiert.');
      expect(done.parentElement).toHaveTextContent('1 war schon im Katalog und wurde übersprungen.');
      expect(done.parentElement).toHaveTextContent('Die Originale in „Dragons“ sind unverändert.');
      expect(screen.getByText(/1 Datei ließ sich nicht lesen/)).toBeInTheDocument();
      expect(screen.getByText('c.stl')).toBeInTheDocument();
      expect(onFinished).toHaveBeenCalledTimes(1);
      const close = footerClose();
      await waitFor(() => expect(close).toHaveFocus());
      fireEvent.click(close);
      expect(onClose).toHaveBeenCalled();
    });

    it('cancels through the job id and reports what is already in the catalog', async () => {
      await startCopy();
      vi.mocked(sourceImport.getSourceImportJob).mockResolvedValueOnce(job({ total: 10, done: 4 })).mockResolvedValue(job({ state: 'cancelled', total: 10, done: 4 }));
      vi.mocked(sourceImport.cancelSourceImport).mockResolvedValue(job({ state: 'cancelling', total: 10, done: 4 }));
      fireEvent.click(screen.getByRole('button', { name: de.sfpImport }));
      fireEvent.click(await screen.findByRole('button', { name: de.sfiStop }));
      await waitFor(() => expect(sourceImport.cancelSourceImport).toHaveBeenCalledWith('j1'));
      const stopping = await screen.findByRole('button', { name: de.sfiStopping });
      expect(stopping).toHaveAttribute('aria-disabled', 'true');
      await advance(700); await advance(700);
      const note = await screen.findByText(de.sfiCancelledTitle);
      expect(note.parentElement).toHaveTextContent('4 von 10 Dateien wurden verarbeitet');
    });

    it('shows a coded job error through the error text and an unknown one as a plain sentence', async () => {
      await startCopy();
      vi.mocked(sourceImport.getSourceImportJob).mockResolvedValue(job({ state: 'failed', total: 3, done: 1, kind: 'sourceFolderMoved' }));
      fireEvent.click(screen.getByRole('button', { name: de.sfpImport }));
      await advance(700);
      const alert = await screen.findByRole('alert');
      expect(alert).toHaveTextContent(de.sfiFailedTitle);
      expect(alert).toHaveTextContent(de.errSourceFolderMoved);
      expect(alert).toHaveTextContent(de.sfiFailedKept);
    });

    it('falls back to a general sentence for an unregistered failure code', async () => {
      await startCopy();
      vi.mocked(sourceImport.getSourceImportJob).mockResolvedValue(job({ state: 'failed', kind: 'import_failed' }));
      fireEvent.click(screen.getByRole('button', { name: de.sfpImport }));
      await advance(700);
      expect(await screen.findByRole('alert')).toHaveTextContent(de.webUploadFailed);
    });

    it('keeps the tree and shows the error when the job does not start', async () => {
      await startCopy();
      vi.mocked(sourceImport.startSourceImport).mockRejectedValue({ message: 'sourceImportLimit', expected: true });
      fireEvent.click(screen.getByRole('button', { name: de.sfpImport }));
      expect(await screen.findByRole('alert')).toHaveTextContent(de.errSourceImportLimit);
      expect(screen.getByRole('tree')).toBeInTheDocument();
      expect(sourceImport.getSourceImportJob).not.toHaveBeenCalled();
    });

    it('stops following after repeated lost answers and offers Close', async () => {
      const onFinished = vi.fn();
      await startCopy({ copyImport: { allowed: true, onFinished } });
      vi.mocked(sourceImport.getSourceImportJob).mockRejectedValue({ message: 'offline', expected: true });
      fireEvent.click(screen.getByRole('button', { name: de.sfpImport }));
      await advance(700); await advance(700); await advance(700);
      expect(await screen.findByRole('alert')).toHaveTextContent('offline');
      expect(footerClose()).toHaveTextContent(de.sfpClose);
      expect(onFinished).toHaveBeenCalledTimes(1);
    });

    it('explains a read-only access instead of starting the copy', async () => {
      await startCopy({ copyImport: { allowed: false } });
      const button = screen.getByRole('button', { name: de.sfpImport });
      expect(button).toHaveAttribute('aria-disabled', 'true');
      expect(button).toHaveAccessibleDescription(de.sfiReadOnly);
      fireEvent.click(button);
      expect(sourceImport.startSourceImport).not.toHaveBeenCalled();
    });
  });
});
