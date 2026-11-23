import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { SourceFoldersSettings } from './SourceFoldersSettings';
import * as api from '../lib/api/sourceFolders';
import { pickFolderPath } from '../lib/api/importExport';
import { de } from '../i18n/de';
import { getSession } from '../lib/webSession';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), warn: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined), debug: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/sourceFolders');
vi.mock('../lib/api/importExport', () => ({ pickFolderPath: vi.fn() }));

const onCheck = vi.fn();
const setup = () => render(<LanguageProvider><ImportLockContext.Provider value={false}><SourceFoldersSettings onCheck={onCheck} /></ImportLockContext.Provider></LanguageProvider>);

beforeEach(async () => {
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ authenticated: true, writable: true, csrfToken: 'token', mustChangePassword: false })));
  await getSession();
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(api.listSourceFolders).mockResolvedValue([{ id: '1', path: 'NAS/Technik', addedAt: '2026-10-01', reachable: true, rootIdx: 1, rel: 'Technik' }]);
  vi.mocked(api.getSourceCheckOnStart).mockResolvedValue(false);
  vi.mocked(api.listRejectedCount).mockReset();
  vi.mocked(api.listSourceDirs).mockResolvedValue({ roots: [{ rootIdx: 0, name: 'NAS' }], entries: [], truncated: false });
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('SourceFoldersSettings in the web build', () => {
  it('offers the server folder choice and the check, never the declined count or the desktop dialog', async () => {
    setup();
    expect(await screen.findByText('NAS/Technik')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Ordner hinzufügen …' })).toBeNull();
    expect(api.listRejectedCount).not.toHaveBeenCalled();
    expect(screen.queryByText(/Abgelehnte Dateien: /)).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: de.sfpCheckNow }));
    expect(onCheck).toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: de.sfpOpen }));
    expect(await screen.findByRole('dialog', { name: de.sfpTitle })).toBeInTheDocument();
    expect(pickFolderPath).not.toHaveBeenCalled();
  });

  it('adds a folder by root index and relative path and reloads the list', async () => {
    vi.mocked(api.addServerSourceFolder).mockResolvedValue({ id: '2', path: 'NAS', addedAt: '', reachable: true });
    setup();
    fireEvent.click(await screen.findByRole('button', { name: de.sfpOpen }));
    const tree = await screen.findByRole('tree');
    fireEvent.keyDown(tree, { key: 'Enter' });
    fireEvent.click(screen.getByRole('button', { name: de.sfpRemember }));
    expect(await screen.findByText(de.sfpRememberedNote.replace('{name}', 'NAS'))).toBeInTheDocument();
    expect(api.addServerSourceFolder).toHaveBeenCalledWith(0, '');
    expect(api.listSourceFolders).toHaveBeenCalledTimes(2);
  });

  it('offers the copy import in the folder dialog now that the server supports it', async () => {
    setup();
    fireEvent.click(await screen.findByRole('button', { name: de.sfpOpen }));
    await screen.findByRole('tree');
    expect(screen.getByRole('button', { name: de.sfpImport })).not.toHaveAttribute('aria-disabled');
    expect(screen.getByText(de.sfpCopyNote)).toBeInTheDocument();
  });

  it('shows how many declined files were offered again', async () => {
    vi.mocked(api.resetRejectedSourceFiles).mockResolvedValue(3);
    setup();
    fireEvent.click(await screen.findByRole('button', { name: de.sfResetRejected }));
    expect(await screen.findByText('3 abgelehnte Dateien werden wieder angeboten.')).toBeInTheDocument();
  });
});
