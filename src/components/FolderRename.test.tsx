import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProviderWithDiagnostics as Provider } from '../test/renderWithDiagnostics';
import { FolderTree as FolderTreeComponent } from './FolderTree';
import { useFolderExpansion } from '../hooks/useFolderExpansion';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { RenameToast } from './RenameToast';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';

function FolderTree(props: Omit<Parameters<typeof FolderTreeComponent>[0], 'expansion'>) {
  return <FolderTreeComponent {...props} expansion={useFolderExpansion()} />;
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
beforeEach(() => vi.mocked(invoke).mockReset());

const folders = [
  { id: '1', name: 'Parts', path: '/parts', parentId: null, count: 37 },
  { id: '2', name: 'Deko', path: '/deko', parentId: null, count: 28 },
];

function setup(props: Partial<Parameters<typeof FolderTree>[0]> = {}, locked = false) {
  const onRenamed = vi.fn();
  const onSelect = vi.fn();
  render(<Provider><ImportLockContext.Provider value={locked}>
    <FolderTree folders={folders} totalModelCount={65} activeFolderId="1" onSelect={onSelect} onRenamed={onRenamed} {...props} />
  </ImportLockContext.Provider></Provider>);
  const row = screen.getByText('Parts').closest('[tabindex]') as HTMLElement;
  return { row, onRenamed, onSelect };
}
const field = () => screen.getByRole('textbox', { name: 'Neuer Ordnername' }) as HTMLInputElement;

describe('folder rename', () => {
  it('offers Umbenennen with F2 as the first menu item and opens the field', () => {
    const { row } = setup();
    fireEvent.contextMenu(row);
    const items = screen.getAllByRole('menuitem');
    expect(items[0].textContent).toContain('Umbenennen');
    expect(items[0].textContent).toContain('F2');
    expect(screen.getByRole('menuitem', { name: 'Umbenennen' })).toBeTruthy();
    fireEvent.click(items[0]);
    expect(screen.queryByRole('menu')).toBeNull();
    expect(field().value).toBe('Parts');
    expect(document.activeElement).toBe(field());
  });

  it('F2 opens the field with the name selected and shows the model count of the row', () => {
    const { row } = setup();
    row.focus();
    fireEvent.keyDown(row, { key: 'F2' });
    expect(field().value).toBe('Parts');
    expect(field().selectionStart).toBe(0);
    expect(field().selectionEnd).toBe(5);
    expect(screen.getByText(/^37 Modelle in diesem Ordner und seinen Unterordnern werden mitgeführt/)).toBeTruthy();
    expect(field().getAttribute('aria-describedby')).toBeTruthy();
  });

  it('keeps Enter and Space typed in the field away from the row selection', () => {
    const { onSelect } = setup();
    fireEvent.keyDown(screen.getByText('Parts').closest('[tabindex]')!, { key: 'F2' });
    fireEvent.keyDown(field(), { key: ' ' });
    fireEvent.click(field());
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('Enter calls rename_folder with the trimmed new name, then reports and returns focus', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { row, onRenamed } = setup();
    fireEvent.keyDown(row, { key: 'F2' });
    fireEvent.change(field(), { target: { value: ' Druckerteile ' } });
    fireEvent.keyDown(field(), { key: 'Enter' });
    await waitFor(() => expect(onRenamed).toHaveBeenCalledWith({ id: '1', from: 'Parts', to: 'Druckerteile', count: 37 }));
    expect(invoke).toHaveBeenCalledWith('rename_folder', { folderId: '1', name: 'Druckerteile' });
    expect(screen.queryByRole('textbox')).toBeNull();
    expect(document.activeElement).toBe(screen.getByText('Parts').closest('[tabindex]'));
  });

  it('Escape cancels without a call and focuses the row', () => {
    const { row, onRenamed } = setup();
    fireEvent.keyDown(row, { key: 'F2' });
    fireEvent.change(field(), { target: { value: 'Other' } });
    fireEvent.keyDown(field(), { key: 'Escape' });
    expect(screen.queryByRole('textbox')).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
    expect(onRenamed).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(screen.getByText('Parts').closest('[tabindex]'));
  });

  it('a click outside cancels', () => {
    const { row } = setup();
    fireEvent.keyDown(row, { key: 'F2' });
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole('textbox')).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });

  it('shows the backend error unchanged, keeps the field open and marks it invalid', async () => {
    vi.mocked(invoke).mockRejectedValueOnce({ expected: true, message: 'Hier gibt es schon einen Ordner „Deko“.' });
    const { row, onRenamed } = setup();
    fireEvent.keyDown(row, { key: 'F2' });
    fireEvent.change(field(), { target: { value: 'Deko' } });
    fireEvent.keyDown(field(), { key: 'Enter' });
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toBe('Hier gibt es schon einen Ordner „Deko“.');
    expect(field().value).toBe('Deko');
    expect(field().getAttribute('aria-invalid')).toBe('true');
    expect(field().getAttribute('aria-describedby')).toContain(alert.id);
    expect(document.activeElement).toBe(field());
    expect(onRenamed).not.toHaveBeenCalled();
  });

  it.each([['', 'Bitte einen Namen eingeben.'], ['   ', 'Bitte einen Namen eingeben.'], ['a/b', 'Der Name darf weder „/“ noch „\\“ enthalten.'], ['a\\b', 'Der Name darf weder „/“ noch „\\“ enthalten.']])('rejects %j on the client without a call', (value, message) => {
    const { row } = setup();
    fireEvent.keyDown(row, { key: 'F2' });
    fireEvent.change(field(), { target: { value } });
    fireEvent.keyDown(field(), { key: 'Enter' });
    expect(screen.getByRole('alert').textContent).toBe(message);
    expect(invoke).not.toHaveBeenCalled();
    expect(field().getAttribute('aria-invalid')).toBe('true');
  });

  it('an unchanged name ends the mode without a call', () => {
    const { row, onRenamed } = setup();
    fireEvent.keyDown(row, { key: 'F2' });
    fireEvent.keyDown(field(), { key: 'Enter' });
    expect(screen.queryByRole('textbox')).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
    expect(onRenamed).not.toHaveBeenCalled();
  });

  it('is locked during an import run', () => {
    const { row } = setup({}, true);
    fireEvent.keyDown(row, { key: 'F2' });
    expect(screen.queryByRole('textbox')).toBeNull();
    fireEvent.contextMenu(row);
    expect((screen.getByRole('menuitem', { name: 'Umbenennen' }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe('rename toast and texts', () => {
  it('reports old and new name and the carried model count', () => {
    render(<Provider><RenameToast from="Drucker-Teile" to="Druckerteile" count={37} onDone={() => {}} /></Provider>);
    expect(screen.getByRole('status').textContent).toBe('„Drucker-Teile“ → „Druckerteile“ umbenannt, 37 Modelle mitgeführt.');
  });
  it.each([['de', de], ['en', en], ['es', es], ['fr', fr]] as const)('has the rename texts in %s', (_, dict) => {
    for (const key of ['renameFolder', 'renameFolderLabel', 'renameFolderEmpty', 'renameFolderSeparator'] as const) expect(dict[key].length).toBeGreaterThan(0);
    for (const key of ['renameFolderHint', 'folderRenamed'] as const) {
      expect(dict[key].one).toContain('{count}');
      expect(dict[key].other).toContain('{count}');
    }
    expect(dict.folderRenamed.one).toContain('{from}');
    expect(dict.folderRenamed.other).toContain('{to}');
  });
});
