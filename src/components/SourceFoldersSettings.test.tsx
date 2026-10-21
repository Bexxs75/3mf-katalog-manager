import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { SourceFoldersSettings } from './SourceFoldersSettings';
import * as api from '../lib/api/sourceFolders';
import { pickFolderPath } from '../lib/api/importExport';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), warn: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined), debug: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/sourceFolders');
vi.mock('../lib/api/importExport', () => ({ pickFolderPath: vi.fn() }));

const downloads = { id: 'f1', path: '/home/a/Downloads', addedAt: '2026-10-01', reachable: true };
const gone = { id: 'f2', path: '/home/a/Weg', addedAt: '2026-10-01', reachable: false };

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(api.addSourceFolder).mockReset(); vi.mocked(api.removeSourceFolder).mockReset(); vi.mocked(api.setSourceCheckOnStart).mockReset();
  vi.mocked(api.resetRejectedSourceFiles).mockReset(); vi.mocked(pickFolderPath).mockReset();
  vi.mocked(api.listSourceFolders).mockResolvedValue([downloads, gone]);
  vi.mocked(api.getSourceCheckOnStart).mockResolvedValue(false);
  vi.mocked(api.listRejectedCount).mockResolvedValue(12);
});
const setup = (locked = false) => render(<LanguageProvider><ImportLockContext.Provider value={locked}><SourceFoldersSettings /></ImportLockContext.Provider></LanguageProvider>);

describe('SourceFoldersSettings', () => {
  it('lists the folders, marks an unreachable one and shows the declined count', async () => {
    setup();
    expect(await screen.findByText(/\/home\/a\/Downloads/)).toBeInTheDocument();
    expect(screen.getByText(/\/home\/a\/Weg/)).toHaveTextContent('nicht erreichbar');
    expect(screen.getByText(/\/home\/a\/Downloads/)).not.toHaveTextContent('nicht erreichbar');
    expect(screen.getByText('Abgelehnte Dateien: 12')).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: /Beim Start der App prüfen/ })).not.toBeChecked();
  });

  it('adds the folder chosen in the dialog and reloads the list', async () => {
    vi.mocked(api.listSourceFolders).mockResolvedValueOnce([]).mockResolvedValue([downloads]);
    vi.mocked(pickFolderPath).mockResolvedValue('/home/a/Downloads');
    vi.mocked(api.addSourceFolder).mockResolvedValue(downloads);
    setup();
    fireEvent.click(await screen.findByRole('button', { name: 'Ordner hinzufügen …' }));
    expect(await screen.findByText(/\/home\/a\/Downloads/)).toBeInTheDocument();
    expect(api.addSourceFolder).toHaveBeenCalledWith('/home/a/Downloads');
  });

  it('does nothing when the folder dialog is cancelled', async () => {
    vi.mocked(pickFolderPath).mockResolvedValue(null);
    setup();
    fireEvent.click(await screen.findByRole('button', { name: 'Ordner hinzufügen …' }));
    await waitFor(() => expect(pickFolderPath).toHaveBeenCalled());
    expect(api.addSourceFolder).not.toHaveBeenCalled();
  });

  it('shows the backend text when adding is refused', async () => {
    vi.mocked(pickFolderPath).mockResolvedValue('/home/a/Katalog');
    vi.mocked(api.addSourceFolder).mockRejectedValue({ message: 'Der Katalogordner kann kein Quellordner sein.', expected: true });
    setup();
    fireEvent.click(await screen.findByRole('button', { name: 'Ordner hinzufügen …' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Der Katalogordner kann kein Quellordner sein.');
  });

  it('removes the chosen folder by id and keeps focus on a control', async () => {
    vi.mocked(api.removeSourceFolder).mockResolvedValue();
    vi.mocked(api.listSourceFolders).mockResolvedValueOnce([downloads, gone]).mockResolvedValue([gone]);
    setup();
    fireEvent.click(await screen.findByRole('button', { name: 'Quellordner /home/a/Downloads entfernen' }));
    await waitFor(() => expect(screen.queryByText(/\/home\/a\/Downloads/)).not.toBeInTheDocument());
    expect(api.removeSourceFolder).toHaveBeenCalledWith('f1');
    expect(screen.getByRole('button', { name: 'Ordner hinzufügen …' })).toHaveFocus();
  });

  it('saves the start check setting in both directions', async () => {
    vi.mocked(api.setSourceCheckOnStart).mockResolvedValue();
    setup();
    const box = await screen.findByRole('checkbox', { name: /Beim Start der App prüfen/ });
    fireEvent.click(box);
    await waitFor(() => expect(box).toBeChecked());
    expect(api.setSourceCheckOnStart).toHaveBeenLastCalledWith(true);
    fireEvent.click(box);
    await waitFor(() => expect(box).not.toBeChecked());
    expect(api.setSourceCheckOnStart).toHaveBeenLastCalledWith(false);
  });

  it('keeps the setting unchanged and says why when saving fails', async () => {
    vi.mocked(api.setSourceCheckOnStart).mockRejectedValue({ message: 'Datenbank gesperrt', expected: true });
    setup();
    const box = await screen.findByRole('checkbox', { name: /Beim Start der App prüfen/ });
    fireEvent.click(box);
    expect(await screen.findByRole('alert')).toHaveTextContent('Datenbank gesperrt');
    expect(box).not.toBeChecked();
  });

  it('resets the declined list and disables the button at zero', async () => {
    vi.mocked(api.resetRejectedSourceFiles).mockResolvedValue(12);
    vi.mocked(api.listRejectedCount).mockResolvedValueOnce(12).mockResolvedValue(0);
    setup();
    fireEvent.click(await screen.findByRole('button', { name: 'Liste zurücksetzen' }));
    await waitFor(() => expect(screen.getByText('Abgelehnte Dateien: 0')).toBeInTheDocument());
    expect(api.resetRejectedSourceFiles).toHaveBeenCalledOnce();
    expect(screen.getByRole('button', { name: 'Liste zurücksetzen' })).toBeDisabled();
  });

  it('locks add, remove and reset during an import and explains it', async () => {
    setup(true);
    await screen.findByText(/\/home\/a\/Downloads/);
    for (const name of ['Ordner hinzufügen …', 'Liste zurücksetzen', 'Quellordner /home/a/Downloads entfernen']) {
      const button = screen.getByRole('button', { name });
      expect(button).toBeDisabled();
      expect(button).toHaveAttribute('title', 'Während eines Imports gesperrt');
    }
    expect(screen.getByText(/Während eines Imports lässt sich die Liste/)).toBeInTheDocument();
  });
});
