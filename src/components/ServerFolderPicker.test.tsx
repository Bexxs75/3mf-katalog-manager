import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ServerFolderPicker, type ServerFolderPickerProps } from './ServerFolderPicker';
import * as api from '../lib/api/sourceFolders';
import { de } from '../i18n/de';
import type { SourceDirListing } from '../types';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), warn: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined), debug: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/sourceFolders');

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
    const onImport = vi.fn().mockResolvedValue(undefined);
    setup({ onImport });
    const t = await tree();
    fireEvent.keyDown(t, { key: 'Enter' });
    fireEvent.click(screen.getByRole('button', { name: de.sfpImport }));
    await waitFor(() => expect(onImport).toHaveBeenCalledWith(0, ''));
  });
});
