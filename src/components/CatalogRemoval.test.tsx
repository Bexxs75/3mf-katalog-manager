import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProviderWithDiagnostics as Provider } from '../test/renderWithDiagnostics';
import { FolderTree } from './FolderTree';
import { CatalogResetSection } from './CatalogResetSection';
import { BulkActionToolbar } from './BulkActionToolbar';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
beforeEach(() => vi.mocked(invoke).mockReset());
const folders = [{ id: '1', name: 'Parts', path: '/parts', parentId: null, count: 4 }];

describe('folder catalog removal', () => {
  it('opens with Shift+F10, loads counts, removes and selects All models', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ name: 'Parts', subfolderCount: 2, modelCount: 4 }).mockResolvedValueOnce({ modelCount: 4, folderCount: 3 });
    const onRemoved = vi.fn(); const onSelect = vi.fn();
    render(<Provider><FolderTree folders={folders} totalModelCount={4} activeFolderId="1" onSelect={onSelect} onRemoved={onRemoved} /></Provider>);
    const row = screen.getByText('Parts').closest('[tabindex]')!;
    (row as HTMLElement).focus();
    fireEvent.keyDown(row, { key: 'F10', shiftKey: true });
    fireEvent.click(screen.getByRole('menuitem', { name: 'Aus dem Katalog entfernen' }));
    expect(await screen.findByText('der Ordner „Parts“ mit 2 Unterordnern')).toBeTruthy();
    expect(screen.getByText('4 Modelle samt Tags und Einträgen im Druckprotokoll')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Aus dem Katalog entfernen' }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('remove_folder_from_catalog', { folderId: '1' }));
    await waitFor(() => expect(onRemoved).toHaveBeenCalledOnce());
    expect(onSelect).toHaveBeenCalledWith('all');
    expect(document.activeElement).toBe(screen.getByRole('button', { name: /^Alle Modelle/ }));
  });
  it('shows the translated expected storage-location error without Report problem', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ name: 'Parts', subfolderCount: 0, modelCount: 4 }).mockRejectedValueOnce({ expected: true, message: 'Das ist der Speicherort des Katalogs. Zum Leeren „Katalog zurücksetzen“ in den Einstellungen verwenden.' });
    render(<Provider><FolderTree folders={folders} totalModelCount={4} activeFolderId="all" onSelect={() => {}} onRemoved={() => {}} /></Provider>);
    fireEvent.contextMenu(screen.getByText('Parts'));
    fireEvent.click(screen.getByRole('menuitem'));
    await screen.findByRole('dialog');
    await screen.findByText('4 Modelle samt Tags und Einträgen im Druckprotokoll');
    fireEvent.click(screen.getByRole('button', { name: 'Aus dem Katalog entfernen' }));
    expect(await screen.findByText(/Das ist der Speicherort des Katalogs/)).toBeTruthy();
    expect(screen.queryByText('Problem melden')).toBeNull();
  });
  it('Escape closes the dialog and restores focus to the folder', async () => {
    vi.mocked(invoke).mockResolvedValue({ name: 'Parts', subfolderCount: 0, modelCount: 4 });
    render(<Provider><FolderTree folders={folders} totalModelCount={4} activeFolderId="all" onSelect={() => {}} /></Provider>);
    const row = screen.getByText('Parts').closest('[tabindex]')!;
    fireEvent.contextMenu(row);
    fireEvent.click(screen.getByRole('menuitem'));
    await screen.findByText('4 Modelle samt Tags und Einträgen im Druckprotokoll');
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(row);
  });
});

describe('reset catalog', () => {
  it('focuses Cancel, exports, and resets only on confirmation', async () => {
    vi.mocked(invoke).mockResolvedValue({ modelCount: 4, folderCount: 2 });
    const onExport = vi.fn().mockResolvedValue(true); const onReset = vi.fn();
    render(<Provider><CatalogResetSection modelCount={4} folderCount={2} onExport={onExport} onReset={onReset} /></Provider>);
    fireEvent.click(screen.getByText('Katalog zurücksetzen …'));
    expect(document.activeElement).toBe(screen.getByText('Abbrechen'));
    expect(screen.getByText(/alle 4 Modelle, 2 Ordner/)).toBeTruthy();
    expect(invoke).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText('Erst sichern …'));
    expect(await screen.findByText('✓ Gesichert')).toBeDisabled();
    fireEvent.click(screen.getByText('Zurücksetzen'));
    await waitFor(() => expect(onReset).toHaveBeenCalledOnce());
    expect(invoke).toHaveBeenCalledWith('reset_catalog');
  });
  it('does not claim a cancelled backup succeeded', async () => {
    const onExport = vi.fn().mockResolvedValue(false);
    render(<Provider><CatalogResetSection modelCount={0} folderCount={0} onExport={onExport} onReset={() => {}} /></Provider>);
    fireEvent.click(screen.getByText('Katalog zurücksetzen …'));
    fireEvent.click(screen.getByText('Erst sichern …'));
    await waitFor(() => expect(screen.getByText('Erst sichern …')).not.toBeDisabled());
    expect(screen.queryByText('✓ Gesichert')).toBeNull();
  });
});

it('bulk toolbar confirms removal and calls its removal action', async () => {
  const noop = () => {}; const onRemove = vi.fn().mockResolvedValue(undefined);
  render(<Provider><BulkActionToolbar selectedCount={3} confirmBulkDelete={false} onConfirmBulkDeleteChange={noop}
    onSelectAllVisible={noop} onClearSelection={noop} onBulkAddToQueue={noop} addToCollectionMenuOpen={false}
    onAddToCollectionMenuOpenChange={noop} collections={[]} onBulkAddToCollection={noop} activeCollection={null}
    onBulkRemoveFromCollection={noop} onBulkSetPrintStatus={noop} onBulkDelete={noop} onBulkRemove={onRemove}
    addTagMenuOpen={false} onAddTagMenuOpenChange={noop} tagDraft="" onTagDraftChange={noop} onSubmitBulkAddTag={noop}
    removeTagMenuOpen={false} onRemoveTagMenuOpenChange={noop} tagsInSelection={[]} onBulkRemoveTag={noop} /></Provider>);
  fireEvent.click(screen.getByText('Aus Katalog entfernen'));
  expect(screen.getByText('3 Modelle aus dem Katalog entfernen?')).toBeTruthy();
  fireEvent.click(screen.getByText('Entfernen'));
  expect(onRemove).toHaveBeenCalledOnce();
});


it('folder activation does not reach catalog-wide selection shortcuts', () => {
  const globalKey = vi.fn();
  document.addEventListener('keydown', globalKey);
  try {
    render(<Provider><FolderTree folders={folders} totalModelCount={4} activeFolderId="all" onSelect={() => {}} /></Provider>);
    fireEvent.keyDown(screen.getByText('Parts').closest('[tabindex]')!, { key: ' ' });
    expect(globalKey).not.toHaveBeenCalled();
  } finally { document.removeEventListener('keydown', globalKey); }
});
