import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProviderWithDiagnostics as Provider } from '../test/renderWithDiagnostics';
import { FolderTree as FolderTreeComponent } from './FolderTree';
import { useFolderExpansion } from '../hooks/useFolderExpansion';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { FolderTrashToast } from './FolderTrashToast';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';
import type { FolderTrashResult, FolderTrashSummary } from '../types';

function FolderTree(props: Omit<Parameters<typeof FolderTreeComponent>[0], 'expansion'>) {
  return <FolderTreeComponent {...props} expansion={useFolderExpansion()} />;
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));

const folders = [
  { id: '1', name: 'Parts', path: '/parts', parentId: null, count: 37 },
  { id: '2', name: 'Inner', path: '/parts/inner', parentId: '1', count: 3 },
  { id: '3', name: 'Other', path: '/other', parentId: null, count: 1 },
];
const summary: FolderTrashSummary = { name: 'Parts', subfolderCount: 2, modelCount: 37, otherFileCount: 4, otherFileCountTruncated: false, isCatalogRootOrAncestor: false };
const done: FolderTrashResult = { trashedModelCount: 37, failed: [], removedFolderCount: 3, keptOnDisk: [] };

function mockBackend(opts: { summary?: Partial<FolderTrashSummary>; trash?: () => Promise<FolderTrashResult> } = {}) {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'folder_trash_summary') return { ...summary, ...opts.summary };
    if (cmd === 'trash_folder') return opts.trash ? opts.trash() : done;
    return undefined;
  });
}
function setup(props: Partial<Parameters<typeof FolderTree>[0]> = {}, locked = false) {
  const onTrashed = vi.fn(); const onSelect = vi.fn();
  render(<Provider><ImportLockContext.Provider value={locked}>
    <FolderTree folders={folders} totalModelCount={41} activeFolderId="3" onSelect={onSelect} onTrashed={onTrashed} {...props} />
  </ImportLockContext.Provider></Provider>);
  const row = screen.getByText('Parts').closest('[tabindex]') as HTMLElement;
  return { row, onTrashed, onSelect };
}
const openDialog = async (row: HTMLElement) => {
  fireEvent.contextMenu(row);
  fireEvent.click(screen.getByRole('menuitem', { name: 'In den Papierkorb legen …' }));
  return screen.findByRole('dialog');
};
beforeEach(() => vi.mocked(invoke).mockReset());

describe('folder trash menu entry', () => {
  it('is a third menu item next to rename and remove', () => {
    mockBackend();
    const { row } = setup();
    fireEvent.contextMenu(row);
    expect(screen.getAllByRole('menuitem').map((i) => i.textContent?.replace('F2', ''))).toEqual(['Umbenennen', 'Aus dem Katalog entfernen', 'In den Papierkorb legen …']);
  });
  it('is disabled during an import run', () => {
    mockBackend();
    const { row } = setup({}, true);
    fireEvent.contextMenu(row);
    expect(screen.getByRole('menuitem', { name: 'In den Papierkorb legen …' })).toBeDisabled();
  });
});

describe('folder trash dialog', () => {
  it('shows the counts, focuses Cancel and calls nothing but the summary until confirmed', async () => {
    mockBackend();
    const { row } = setup();
    const dialog = await openDialog(row);
    expect(await screen.findByText('2 Unterordner')).toBeTruthy();
    expect(screen.getByText('37 Modelle kommen in den App-Papierkorb (7 Tage, wiederherstellbar)')).toBeTruthy();
    expect(screen.getByText('4 andere Dateien (z. B. Bilder, Notizen) bleiben auf der Festplatte liegen und werden nicht angefasst')).toBeTruthy();
    expect(screen.getByText('Der Ordner selbst wird erst von der Festplatte entfernt, wenn er leer ist.')).toBeTruthy();
    expect(dialog.getAttribute('aria-labelledby')).toBeTruthy();
    expect(screen.getByRole('heading', { name: '„Parts“ in den Papierkorb legen?' })).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Abbrechen' }));
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('folder_trash_summary', { folderId: '1' });
  });
  it('says "at least" for a truncated count and hides the line without other files', async () => {
    mockBackend({ summary: { otherFileCount: 1000, otherFileCountTruncated: true } });
    const { row } = setup();
    await openDialog(row);
    expect(await screen.findByText(/^mindestens 1\.000 andere Dateien/)).toBeTruthy();
  });
  it('omits the other-files line when there are none', async () => {
    mockBackend({ summary: { otherFileCount: 0 } });
    const { row } = setup();
    await openDialog(row);
    await screen.findByText('2 Unterordner');
    expect(screen.queryByText(/andere Datei/)).toBeNull();
  });
  it('shows a notice instead of the button for the catalog root or an ancestor', async () => {
    mockBackend({ summary: { isCatalogRootOrAncestor: true } });
    const { row } = setup();
    await openDialog(row);
    expect(await screen.findByText(/Speicherort des Katalogs oder ein Ordner darüber/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'In den Papierkorb legen' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Abbrechen' })).toBeTruthy();
  });
  it('confirms with the folder id, closes, selects All models when the active folder was inside, and refocuses', async () => {
    mockBackend();
    const { row, onTrashed, onSelect } = setup({ activeFolderId: '2' });
    await openDialog(row);
    await screen.findByText('2 Unterordner');
    fireEvent.click(screen.getByRole('button', { name: 'In den Papierkorb legen' }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('trash_folder', { folderId: '1' }));
    await waitFor(() => expect(onTrashed).toHaveBeenCalledWith(done));
    expect(onSelect).toHaveBeenCalledWith('all');
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: /^Alle Modelle/ }));
  });
  it('keeps the selection when the active folder was elsewhere', async () => {
    mockBackend();
    const { row, onTrashed, onSelect } = setup({ activeFolderId: '3' });
    await openDialog(row);
    await screen.findByText('2 Unterordner');
    fireEvent.click(screen.getByRole('button', { name: 'In den Papierkorb legen' }));
    await waitFor(() => expect(onTrashed).toHaveBeenCalled());
    expect(onSelect).not.toHaveBeenCalled();
  });
  it('keeps the dialog open with an alert when trashing fails, and locks the buttons while working', async () => {
    let fail!: (e: unknown) => void;
    mockBackend({ trash: () => new Promise<FolderTrashResult>((_, reject) => { fail = reject; }) });
    const { row, onTrashed } = setup();
    await openDialog(row);
    await screen.findByText('2 Unterordner');
    fireEvent.click(screen.getByRole('button', { name: 'In den Papierkorb legen' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Abbrechen' })).toBeDisabled());
    expect(screen.getByRole('button', { name: 'In den Papierkorb legen' })).toBeDisabled();
    fail({ expected: true, message: 'Papierkorb nicht erreichbar' });
    expect((await screen.findByRole('alert')).textContent).toContain('Papierkorb nicht erreichbar');
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Abbrechen' })).not.toBeDisabled();
    expect(onTrashed).not.toHaveBeenCalled();
  });
  it('cancel closes without trashing and returns focus to the folder row', async () => {
    mockBackend();
    const { row, onTrashed } = setup();
    await openDialog(row);
    await screen.findByText('2 Unterordner');
    fireEvent.click(screen.getByRole('button', { name: 'Abbrechen' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith('trash_folder', expect.anything());
    expect(onTrashed).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(row);
  });
});

describe('folder trash toast', () => {
  const show = (result: FolderTrashResult) => render(<Provider><FolderTrashToast result={result} onDone={() => {}} /></Provider>);
  it('reports only the trashed models on a clean run', () => {
    show(done);
    expect(screen.getByRole('status').textContent).toBe('37 Modelle in den Papierkorb gelegt.');
  });
  it('adds kept folders and failed files, naming at most two of each', () => {
    show({ trashedModelCount: 1, removedFolderCount: 0,
      keptOnDisk: ['/a/Parts', '/a/Parts/Inner', '/a/Parts/Deep'],
      failed: [{ fileId: 'x', path: '/a/Parts/one.3mf', reason: 'denied' }] });
    expect(screen.getByRole('status').textContent).toBe(
      '1 Modell in den Papierkorb gelegt. 3 Ordner bleiben auf der Festplatte (enthalten andere Dateien) (Parts, Inner, …). 1 Datei konnte nicht verschoben werden (one.3mf).');
  });
});

describe('folder trash texts', () => {
  it.each([['de', de], ['en', en], ['es', es], ['fr', fr]] as const)('has all texts in %s', (_, dict) => {
    for (const key of ['trashFolder', 'trashFolderQuestion', 'trashFolderEmptyNote', 'trashFolderConfirm', 'trashFolderProtected'] as const) expect(dict[key].length).toBeGreaterThan(0);
    expect(dict.trashFolderQuestion).toContain('{name}');
    for (const key of ['trashFolderSubfolders', 'trashFolderModels', 'trashFolderOthers', 'trashFolderOthersAtLeast', 'trashFolderDone', 'trashFolderKept', 'trashFolderFailed'] as const) {
      expect(dict[key].one).toContain('{count}');
      expect(dict[key].other).toContain('{count}');
    }
  });
});
