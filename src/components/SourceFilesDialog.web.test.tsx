import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { SourceFilesDialog } from './SourceFilesDialog';
import * as api from '../lib/api/sourceFolders';
import * as sourceImport from '../lib/api/sourceImport';
import { de } from '../i18n/de';
import { getSession } from '../lib/webSession';
import type { SourceScan } from '../types';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), warn: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined), debug: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/sourceFolders');
vi.mock('../lib/api/sourceImport', async importOriginal => ({
  ...await importOriginal<typeof import('../lib/api/sourceImport')>(),
  startSourceImport: vi.fn(), getSourceImportJob: vi.fn(), cancelSourceImport: vi.fn(),
}));

const scan: SourceScan = {
  folderId: 'f1', folderPath: 'NAS/Technik', skipped: 0, truncated: false, unreachable: false,
  files: [{ path: '1/Technik/a.3mf', relativePath: 'a.3mf', sizeBytes: 2048, duplicateOfId: null }],
};
const advance = (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });

async function setup(writable: boolean, onImported = vi.fn()) {
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ authenticated: true, writable, csrfToken: 'token', mustChangePassword: false })));
  await getSession();
  const onImport = vi.fn(), onClose = vi.fn();
  render(<LanguageProvider><SourceFilesDialog scans={[scan]} onClose={onClose} onImport={onImport} onImported={onImported}
    onReject={vi.fn()} onRescan={vi.fn()} /></LanguageProvider>);
  return { onImport, onClose, onImported };
}

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.mocked(api.listSourceFolders).mockReset().mockResolvedValue([{ id: 'f1', path: 'NAS/Technik', addedAt: '', reachable: true, rootIdx: 1, rel: 'Technik' }]);
  vi.mocked(sourceImport.startSourceImport).mockReset().mockResolvedValue({ jobId: 'j1' });
  vi.mocked(sourceImport.getSourceImportJob).mockReset();
});
afterEach(() => { cleanup(); vi.useRealTimers(); vi.unstubAllGlobals(); });

describe('SourceFilesDialog in the web build', () => {
  it('offers a copy per folder instead of the desktop move and explains it', async () => {
    await setup(true);
    expect(await screen.findByRole('button', { name: `Importieren (Kopie): NAS/Technik` })).toHaveTextContent(de.sfiImportCopy);
    expect(screen.queryByRole('button', { name: /verschieben/ })).toBeNull();
    expect(screen.getByText(de.sfiWebHint)).toBeInTheDocument();
  });

  it('copies the whole source folder by root index and path, then reloads the catalog', async () => {
    const { onImport, onClose, onImported } = await setup(true);
    vi.mocked(sourceImport.getSourceImportJob).mockResolvedValue({ jobId: 'j1', state: 'completed', current: null, total: 1, done: 1, kind: null,
      files: [{ uploadId: 'source-0', name: 'a.3mf', state: 'imported', fileId: '5', existingFileId: null, kind: null }] });
    fireEvent.click(await screen.findByRole('button', { name: /Importieren \(Kopie\)/ }));
    await waitFor(() => expect(sourceImport.startSourceImport).toHaveBeenCalledWith(1, 'Technik'));
    await advance(700);
    expect(await screen.findByText(de.sfiDoneTitle)).toBeInTheDocument();
    expect(onImported).toHaveBeenCalledTimes(1);
    expect(onImport).not.toHaveBeenCalled();
    const close = screen.getByRole('button', { name: de.sfpClose });
    await waitFor(() => expect(close).toHaveFocus());
    fireEvent.click(close);
    expect(onClose).toHaveBeenCalled();
  });

  it('shows a start error and keeps the list', async () => {
    await setup(true);
    vi.mocked(sourceImport.startSourceImport).mockRejectedValue({ message: 'sourceImportSpace', expected: true });
    fireEvent.click(await screen.findByRole('button', { name: /Importieren \(Kopie\)/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent(de.errSourceImportSpace);
    expect(screen.getByRole('checkbox', { name: 'a.3mf' })).toBeInTheDocument();
  });

  it('keeps a read-only session from starting the copy and says why', async () => {
    await setup(false);
    const button = await screen.findByRole('button', { name: /Importieren \(Kopie\)/ });
    expect(button).toHaveAttribute('aria-disabled', 'true');
    expect(button).toHaveAccessibleDescription(de.sfiReadOnly);
    fireEvent.click(button);
    expect(sourceImport.startSourceImport).not.toHaveBeenCalled();
  });
});
