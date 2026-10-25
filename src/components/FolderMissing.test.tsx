import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProviderWithDiagnostics as Provider } from '../test/renderWithDiagnostics';
import { FolderTree as FolderTreeComponent } from './FolderTree';
import { CatalogDiskSync } from './CatalogDiskSync';
import { useFolderExpansion } from '../hooks/useFolderExpansion';
import type { DiskScan } from '../lib/api/diskSync';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
beforeEach(() => { vi.mocked(invoke).mockReset(); localStorage.clear(); });

function FolderTree(props: Omit<Parameters<typeof FolderTreeComponent>[0], 'expansion'>) {
  return <FolderTreeComponent {...props} expansion={useFolderExpansion()} />;
}
const folders = [
  { id: '1', name: 'Deko', path: '/m/Deko', parentId: null, count: 28 },
  { id: '2', name: 'Altes Zeug', path: '/m/Altes Zeug', parentId: null, count: 9, missing: true },
];

describe('folder tree: folders that are gone from disk', () => {
  it('marks only the missing folder, with an icon and a text label', () => {
    render(<Provider><FolderTree folders={folders} totalModelCount={37} activeFolderId="all" onSelect={vi.fn()} /></Provider>);
    const missing = screen.getByText('Altes Zeug').closest('[tabindex]') as HTMLElement;
    const fine = screen.getByText('Deko').closest('[tabindex]') as HTMLElement;
    expect(within(missing).getByText(de.folderMissingOnDisk)).toBeTruthy();
    expect(within(missing).getByTestId('folder-missing-icon')).toBeTruthy();
    expect(missing).toHaveAttribute('title', de.folderMissingOnDisk);
    // The label is part of the row's accessible name, so a screen reader announces it.
    expect(missing).toHaveAccessibleName(expect.stringContaining(de.folderMissingOnDisk));
    expect(within(fine).queryByText(de.folderMissingOnDisk)).toBeNull();
    expect(fine).not.toHaveAttribute('title');
    expect(fine).toHaveAccessibleName(expect.not.stringContaining(de.folderMissingOnDisk));
  });
  it('keeps click and keyboard selection working on a missing folder', () => {
    const onSelect = vi.fn();
    render(<Provider><FolderTree folders={folders} totalModelCount={37} activeFolderId="all" onSelect={onSelect} /></Provider>);
    const row = screen.getByText('Altes Zeug').closest('[tabindex]') as HTMLElement;
    fireEvent.click(row);
    fireEvent.keyDown(row, { key: 'Enter' });
    expect(onSelect).toHaveBeenCalledTimes(2);
    expect(onSelect).toHaveBeenCalledWith('2');
  });
});

const emptyScan: DiskScan = { newFiles: [], newFolders: [], renamedFiles: [], duplicateFiles: [], missingFiles: [], missingFolders: [], unreachableRoots: [], skippedPaths: [], trashBlockedCount: 0 };
const gone: DiskScan = {
  ...emptyScan,
  missingFiles: [{ path: '/m/loose.stl', relativePath: 'loose.stl', root: '/m', id: '5' }],
  missingFolders: [{ id: '2', path: '/m/Altes Zeug', relativePath: 'Altes Zeug', root: '/m', modelCount: 9 }],
};
async function openDialog(scan: DiskScan, onScanned = vi.fn()) {
  vi.mocked(invoke).mockResolvedValue(scan);
  render(<Provider><CatalogDiskSync onScanned={onScanned} /></Provider>);
  fireEvent.keyDown(document, { key: 'F5' });
  return { dialog: await screen.findByRole('dialog'), onScanned };
}

describe('disk sync dialog: missing folders', () => {
  it('lists the folder first in the missing group with its model count and counts it in the header', async () => {
    const { dialog } = await openDialog(gone);
    expect(within(dialog).getByText(de.diskSyncMissingHeading)).toBeTruthy();
    expect(within(dialog).getByText('1 Ordner fehlt')).toBeTruthy();
    const items = within(dialog).getAllByRole('listitem');
    expect(items[0]).toHaveTextContent('Ordner');
    expect(items[0]).toHaveTextContent('Altes Zeug (9 Modelle)');
    expect(items[1]).toHaveTextContent('loose.stl');
  });
  it('does not show the folder pill when no folder is missing', async () => {
    const { dialog } = await openDialog({ ...gone, missingFolders: [] });
    expect(within(dialog).queryByText(/Ordner fehl/)).toBeNull();
    expect(within(dialog).queryByText(/Altes Zeug/)).toBeNull();
  });
  it('opens for a missing folder alone and lets the user remove it with the existing option', async () => {
    const { dialog } = await openDialog({ ...emptyScan, missingFolders: gone.missingFolders });
    const box = within(dialog).getByRole('checkbox', { name: de.diskSyncRemove });
    fireEvent.click(box);
    vi.mocked(invoke).mockResolvedValueOnce({ fileIds: [], folderIds: [], renamedIds: [], renamedCount: 0, removedCount: 0, failedPaths: [] });
    fireEvent.click(within(dialog).getByRole('button', { name: de.diskSyncRemoveAction }));
    await screen.findByRole('status');
    expect(invoke).toHaveBeenLastCalledWith('apply_catalog_disk_sync', { selection: { newPaths: [], missingIds: [], removeMissing: true, renames: [] } });
  });
  it('tells the folder tree to reload after a scan', async () => {
    const { onScanned } = await openDialog(gone);
    expect(onScanned).toHaveBeenCalledTimes(1);
  });
});

describe('i18n: missing folder texts', () => {
  it.each([['de', de], ['en', en], ['es', es], ['fr', fr]] as const)('are complete in %s', (_, dictionary) => {
    for (const key of ['folderMissingOnDisk', 'diskSyncFolderLabel'] as const) expect(dictionary[key].length).toBeGreaterThan(1);
    for (const key of ['diskSyncMissingFolders', 'diskSyncFolderModels'] as const) {
      expect(dictionary[key].one).toContain('{count}');
      expect(dictionary[key].other).toContain('{count}');
      expect(dictionary[key].one).not.toBe(dictionary[key].other);
    }
  });
});
