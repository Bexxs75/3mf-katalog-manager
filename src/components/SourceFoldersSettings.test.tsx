import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { SourceFoldersSettings } from './SourceFoldersSettings';
import * as api from '../lib/api/sourceFolders';
import { pickFolderPath } from '../lib/api/importExport';
import { de } from '../i18n/de';

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
    expect(screen.getByRole('switch', { name: de.sfCheckOnStart })).toHaveAttribute('aria-checked', 'false');
  });

  it('explains the start check and the re-offer of declined files at the controls', async () => {
    setup();
    const box = await screen.findByRole('switch', { name: de.sfCheckOnStart });
    expect(box).toHaveAccessibleDescription(de.sfCheckOnStartHint);
    expect(screen.getByRole('button', { name: de.sfResetRejected })).toHaveAccessibleDescription(`Abgelehnte Dateien: 12 ${de.sfResetRejectedNote}`);
    expect(screen.getByText(de.sfResetRejectedNote)).toBeVisible();
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
    // The component parks focus on the add button in an effect that runs once the action is over and the button is enabled
    // again, so it lands after the row has disappeared; under load the immediate check used to run too early.
    await waitFor(() => expect(screen.getByRole('button', { name: 'Ordner hinzufügen …' })).toHaveFocus());
  });

  it('saves the start check setting in both directions', async () => {
    vi.mocked(api.setSourceCheckOnStart).mockResolvedValue();
    setup();
    const box = await screen.findByRole('switch', { name: de.sfCheckOnStart });
    fireEvent.click(box);
    await waitFor(() => expect(box).toHaveAttribute('aria-checked', 'true'));
    expect(api.setSourceCheckOnStart).toHaveBeenLastCalledWith(true);
    fireEvent.click(box);
    await waitFor(() => expect(box).toHaveAttribute('aria-checked', 'false'));
    expect(api.setSourceCheckOnStart).toHaveBeenLastCalledWith(false);
  });

  it('keeps the setting unchanged and says why when saving fails', async () => {
    vi.mocked(api.setSourceCheckOnStart).mockRejectedValue({ message: 'Datenbank gesperrt', expected: true });
    setup();
    const box = await screen.findByRole('switch', { name: de.sfCheckOnStart });
    fireEvent.click(box);
    expect(await screen.findByRole('alert')).toHaveTextContent('Datenbank gesperrt');
    expect(box).toHaveAttribute('aria-checked', 'false');
  });

  it('resets the declined list and disables the button at zero', async () => {
    vi.mocked(api.resetRejectedSourceFiles).mockResolvedValue(12);
    vi.mocked(api.listRejectedCount).mockResolvedValueOnce(12).mockResolvedValue(0);
    setup();
    fireEvent.click(await screen.findByRole('button', { name: de.sfResetRejected }));
    await waitFor(() => expect(screen.getByText('Abgelehnte Dateien: 0')).toBeInTheDocument());
    expect(api.resetRejectedSourceFiles).toHaveBeenCalledOnce();
    expect(screen.getByRole('button', { name: de.sfResetRejected })).toBeDisabled();
  });

  it('locks add, remove and reset during an import and explains it', async () => {
    setup(true);
    await screen.findByText(/\/home\/a\/Downloads/);
    for (const name of ['Ordner hinzufügen …', de.sfResetRejected, 'Quellordner /home/a/Downloads entfernen']) {
      const button = screen.getByRole('button', { name });
      expect(button).toBeDisabled();
      expect(button).toHaveAttribute('title', 'Während eines Imports gesperrt');
    }
    expect(screen.getByText(/Während eines Imports lässt sich die Liste/)).toBeInTheDocument();
  });
});


it.each([['de', de], ['en', en], ['es', es], ['fr', fr]] as const)('keeps the full declined-file action operable in %s', async (language, texts) => {
  localStorage.setItem('3mf-katalog-language', language);
  setup();
  const reset = await screen.findByRole('button', { name: texts.sfResetRejected });
  await waitFor(() => expect(reset).toBeEnabled());
  expect(reset.parentElement).toHaveClass('flex-wrap', 'min-w-0');
  expect(reset).not.toHaveClass('shrink-0');
  vi.mocked(api.listRejectedCount).mockResolvedValue(0);
  fireEvent.click(reset);
  await waitFor(() => expect(api.resetRejectedSourceFiles).toHaveBeenCalledOnce());
  await waitFor(() => expect(reset).toBeDisabled());
  expect(document.getElementById('source-rejected-count')).toHaveTextContent(texts.sfRejected.replace('{count}', '0'));
});

it('keeps the rejected-file count together when the actions wrap', async () => {
  setup();
  await screen.findByText(/12/);
  expect(document.getElementById('source-rejected-count')).toHaveClass('whitespace-nowrap', 'flex-auto');
});
