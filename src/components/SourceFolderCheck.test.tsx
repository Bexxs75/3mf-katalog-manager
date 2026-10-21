import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, renderHook, screen, waitFor, within } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { useSourceFolderCheck } from '../hooks/useSourceFolderCheck';
import { SourceFolderNotice } from './SourceFolderNotice';
import { SourceFilesDialog } from './SourceFilesDialog';
import * as api from '../lib/api/sourceFolders';
import type { SourceScan } from '../types';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), warn: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined), debug: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/sourceFolders');

const file = (relativePath: string, duplicateOfId: string | null = null, sizeBytes = 1_258_291) =>
  ({ path: `/home/a/Downloads/${relativePath}`, relativePath, sizeBytes, duplicateOfId });
const scan = (over: Partial<SourceScan> = {}): SourceScan => ({
  folderId: 'f1', folderPath: '/home/a/Downloads', files: [file('Halter.3mf'), file('Vase_v2.stl'), file('Kopie.3mf', 'm9')],
  skipped: 0, truncated: false, unreachable: false, ...over,
});
const folder = { id: 'f1', path: '/home/a/Downloads', addedAt: '2026-10-01', reachable: true };

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  for (const fn of [api.listSourceFolders, api.scanSourceFolders, api.getSourceCheckOnStart, api.importSourceFiles, api.rejectSourceFiles]) vi.mocked(fn).mockReset();
  vi.mocked(api.listSourceFolders).mockResolvedValue([folder]);
  vi.mocked(api.getSourceCheckOnStart).mockResolvedValue(false);
});

/** Wires hook, notice and dialog like the app does. */
function Harness({ ready = true, jobActive = false, onNoFolders = () => undefined, onImport = async () => undefined }: { ready?: boolean; jobActive?: boolean; onNoFolders?: () => void; onImport?: (paths: string[]) => Promise<unknown> }) {
  const check = useSourceFolderCheck({ ready, jobActive, onNoFolders });
  return <ImportLockContext.Provider value={jobActive}>
    <button onClick={() => void check.check()}>prüfen</button>
    {check.notice && <SourceFolderNotice notice={check.notice} onView={check.openDialog} onClose={check.dismissNotice} />}
    {check.dialogOpen && <SourceFilesDialog scans={check.scans} onClose={check.closeDialog} onImport={onImport} onReject={api.rejectSourceFiles} onRescan={check.rescan} />}
  </ImportLockContext.Provider>;
}
const setup = (props: Parameters<typeof Harness>[0] = {}) => render(<LanguageProvider><Harness {...props} /></LanguageProvider>);
const check = () => fireEvent.click(screen.getByRole('button', { name: 'prüfen' }));

describe('check notice', () => {
  it('names the single folder and counts the files', async () => {
    vi.mocked(api.scanSourceFolders).mockResolvedValue([scan()]);
    setup(); check();
    expect(await screen.findByRole('status')).toHaveTextContent('3 neue Dateien im Quellordner „Downloads“');
    expect(screen.getByRole('button', { name: 'Ansehen' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Später' })).toBeInTheDocument();
  });

  it('counts folders when several have new files', async () => {
    vi.mocked(api.scanSourceFolders).mockResolvedValue([scan(), scan({ folderId: 'f2', folderPath: '/home/a/Bambu', files: [file('x.3mf')] }), scan({ folderId: 'f3', folderPath: '/leer', files: [] })]);
    setup(); check();
    expect(await screen.findByRole('status')).toHaveTextContent('4 neue Dateien in 2 Quellordnern');
  });

  it('says so when nothing is new and mentions truncated, skipped and unreachable', async () => {
    vi.mocked(api.scanSourceFolders).mockResolvedValue([scan({ files: [], truncated: true, skipped: 2 }), scan({ folderId: 'f2', files: [], unreachable: true })]);
    setup(); check();
    const notice = await screen.findByRole('status');
    expect(notice).toHaveTextContent('Keine neuen Dateien in den Quellordnern');
    expect(notice).toHaveTextContent('Die Prüfung wurde bei sehr vielen Einträgen beendet');
    expect(notice).toHaveTextContent('2 Einträge konnten nicht gelesen werden.');
    expect(notice).toHaveTextContent('1 Quellordner ist nicht erreichbar.');
    expect(screen.queryByRole('button', { name: 'Ansehen' })).not.toBeInTheDocument();
  });

  it('points to the settings when no source folder exists', async () => {
    vi.mocked(api.listSourceFolders).mockResolvedValue([]);
    const onNoFolders = vi.fn();
    setup({ onNoFolders }); check();
    expect(await screen.findByRole('status')).toHaveTextContent('Es ist noch kein Quellordner eingerichtet');
    expect(onNoFolders).toHaveBeenCalledOnce();
    expect(api.scanSourceFolders).not.toHaveBeenCalled();
  });

  it('shows a failing check as an alert', async () => {
    vi.mocked(api.scanSourceFolders).mockRejectedValue({ message: 'Prüfung nicht möglich', expected: true });
    setup(); check();
    expect(await screen.findByRole('alert')).toHaveTextContent('Prüfung nicht möglich');
  });

  it('does not check while an import runs', async () => {
    setup({ jobActive: true }); check();
    await act(async () => { await Promise.resolve(); });
    expect(api.listSourceFolders).not.toHaveBeenCalled();
  });
});

describe('check at app start', () => {
  const start = (initial: { ready: boolean; jobActive: boolean }) => renderHook((p: { ready: boolean; jobActive: boolean }) => useSourceFolderCheck({ ...p, onNoFolders: vi.fn() }), { initialProps: initial });

  it('checks once when switched on, shows the notice and opens no window', async () => {
    vi.mocked(api.getSourceCheckOnStart).mockResolvedValue(true);
    vi.mocked(api.scanSourceFolders).mockResolvedValue([scan()]);
    const { result, rerender } = start({ ready: true, jobActive: false });
    await waitFor(() => expect(result.current.notice?.kind).toBe('found'));
    expect(result.current.dialogOpen).toBe(false);
    rerender({ ready: true, jobActive: true }); rerender({ ready: true, jobActive: false });
    await act(async () => { await Promise.resolve(); });
    expect(api.scanSourceFolders).toHaveBeenCalledTimes(1);
  });

  it('stays silent at start when nothing is new', async () => {
    vi.mocked(api.getSourceCheckOnStart).mockResolvedValue(true);
    vi.mocked(api.scanSourceFolders).mockResolvedValue([scan({ files: [] })]);
    const { result } = start({ ready: true, jobActive: false });
    await waitFor(() => expect(api.scanSourceFolders).toHaveBeenCalled());
    await act(async () => { await Promise.resolve(); });
    expect(result.current.notice).toBeNull();
  });

  it('does nothing when switched off', async () => {
    const { result } = start({ ready: true, jobActive: false });
    await waitFor(() => expect(api.getSourceCheckOnStart).toHaveBeenCalled());
    await act(async () => { await Promise.resolve(); });
    expect(api.scanSourceFolders).not.toHaveBeenCalled();
    expect(result.current.notice).toBeNull();
  });

  it('waits for the catalog and for a running import to end', async () => {
    vi.mocked(api.getSourceCheckOnStart).mockResolvedValue(true);
    vi.mocked(api.scanSourceFolders).mockResolvedValue([scan()]);
    const { result, rerender } = start({ ready: false, jobActive: false });
    await act(async () => { await Promise.resolve(); });
    expect(api.getSourceCheckOnStart).not.toHaveBeenCalled();
    rerender({ ready: true, jobActive: true });
    await act(async () => { await Promise.resolve(); });
    expect(api.getSourceCheckOnStart).not.toHaveBeenCalled();
    rerender({ ready: true, jobActive: false });
    await waitFor(() => expect(result.current.notice?.kind).toBe('found'));
  });

  it('never opens a window or nags when the start check fails', async () => {
    vi.mocked(api.getSourceCheckOnStart).mockResolvedValue(true);
    vi.mocked(api.scanSourceFolders).mockRejectedValue(new Error('boom'));
    const spy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const { result } = start({ ready: true, jobActive: false });
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(result.current.notice).toBeNull();
    spy.mockRestore();
  });
});

describe('new files dialog', () => {
  const open = async () => {
    vi.mocked(api.scanSourceFolders).mockResolvedValue([scan()]);
    const view = setup(); check();
    fireEvent.click(await screen.findByRole('button', { name: 'Ansehen' }));
    return { view, dialog: await screen.findByRole('dialog', { name: 'Neue Dateien im Quellordner' }) };
  };

  it('preselects everything except copies of existing models, which are marked', async () => {
    const { dialog } = await open();
    expect(within(dialog).getByRole('checkbox', { name: 'Halter.3mf' })).toBeChecked();
    expect(within(dialog).getByRole('checkbox', { name: 'Vase_v2.stl' })).toBeChecked();
    const copy = within(dialog).getByRole('checkbox', { name: 'Kopie.3mf' });
    expect(copy).not.toBeChecked();
    expect(copy).toHaveAccessibleDescription('Gleicher Inhalt wie ein vorhandenes Modell');
    expect(within(dialog).getByRole('button', { name: '2 in den Katalog verschieben' })).toBeEnabled();
    expect(within(dialog).getByText('/home/a/Downloads')).toBeInTheDocument();
    expect(within(dialog).getByText(/Die Dateien wandern in den Katalogordner/)).toBeInTheDocument();
    expect(screen.queryByRole('status', { name: /Ansehen/ })).not.toBeInTheDocument();
  });

  it('moves exactly the ticked files and closes', async () => {
    const onImport = vi.fn().mockResolvedValue(undefined);
    vi.mocked(api.scanSourceFolders).mockResolvedValue([scan()]);
    setup({ onImport }); check();
    fireEvent.click(await screen.findByRole('button', { name: 'Ansehen' }));
    const dialog = await screen.findByRole('dialog');
    fireEvent.click(within(dialog).getByRole('checkbox', { name: 'Vase_v2.stl' }));
    fireEvent.click(within(dialog).getByRole('checkbox', { name: 'Kopie.3mf' }));
    fireEvent.click(within(dialog).getByRole('button', { name: '2 in den Katalog verschieben' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(onImport).toHaveBeenCalledWith(['/home/a/Downloads/Halter.3mf', '/home/a/Downloads/Kopie.3mf']);
  });

  it('disables both actions when nothing is ticked', async () => {
    const { dialog } = await open();
    fireEvent.click(within(dialog).getByRole('checkbox', { name: 'Halter.3mf' }));
    fireEvent.click(within(dialog).getByRole('checkbox', { name: 'Vase_v2.stl' }));
    expect(within(dialog).getByRole('button', { name: 'Ausgewählte ablehnen' })).toBeDisabled();
    expect(within(dialog).getByRole('button', { name: '0 in den Katalog verschieben' })).toBeDisabled();
  });

  it('keeps the dialog open and shows the backend error when moving fails', async () => {
    const onImport = vi.fn().mockRejectedValue({ message: 'Es ist noch kein Katalogordner eingerichtet.', expected: true });
    vi.mocked(api.scanSourceFolders).mockResolvedValue([scan()]);
    setup({ onImport }); check();
    fireEvent.click(await screen.findByRole('button', { name: 'Ansehen' }));
    const dialog = await screen.findByRole('dialog');
    fireEvent.click(within(dialog).getByRole('button', { name: '2 in den Katalog verschieben' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('Es ist noch kein Katalogordner eingerichtet.');
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: '2 in den Katalog verschieben' })).toBeEnabled();
  });

  it('declines the ticked files, re-reads the folders and keeps the rest', async () => {
    vi.mocked(api.rejectSourceFiles).mockResolvedValue(1);
    const { dialog } = await open();
    vi.mocked(api.scanSourceFolders).mockResolvedValue([scan({ files: [file('Vase_v2.stl'), file('Kopie.3mf', 'm9')] })]);
    fireEvent.click(within(dialog).getByRole('checkbox', { name: 'Vase_v2.stl' }));
    fireEvent.click(within(dialog).getByRole('button', { name: 'Ausgewählte ablehnen' }));
    await waitFor(() => expect(within(dialog).queryByRole('checkbox', { name: 'Halter.3mf' })).not.toBeInTheDocument());
    expect(api.rejectSourceFiles).toHaveBeenCalledWith(['/home/a/Downloads/Halter.3mf']);
    // The user's choice survives the refresh: Vase_v2 was unticked, the copy was never ticked.
    expect(within(dialog).getByRole('checkbox', { name: 'Vase_v2.stl' })).not.toBeChecked();
    expect(within(dialog).getByText('1 Datei abgelehnt')).toBeInTheDocument();
    expect(within(dialog).getByText('1 Datei abgelehnt')).toHaveFocus();
  });

  it('closes when declining leaves nothing', async () => {
    vi.mocked(api.rejectSourceFiles).mockResolvedValue(3);
    const { dialog } = await open();
    vi.mocked(api.scanSourceFolders).mockResolvedValue([scan({ files: [] })]);
    fireEvent.click(within(dialog).getByRole('checkbox', { name: 'Kopie.3mf' }));
    fireEvent.click(within(dialog).getByRole('button', { name: 'Ausgewählte ablehnen' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('shows a decline error in the dialog', async () => {
    vi.mocked(api.rejectSourceFiles).mockRejectedValue({ message: 'Merken nicht möglich', expected: true });
    const { dialog } = await open();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Ausgewählte ablehnen' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('Merken nicht möglich');
  });

  it('closes with Später without calling the backend', async () => {
    const { dialog } = await open();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Später' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(api.rejectSourceFiles).not.toHaveBeenCalled();
    expect(api.importSourceFiles).not.toHaveBeenCalled();
  });

  it('locks moving while an import runs', () => {
    render(<LanguageProvider><ImportLockContext.Provider value>
      <SourceFilesDialog scans={[scan()]} onClose={vi.fn()} onImport={vi.fn()} onReject={vi.fn()} onRescan={vi.fn()} />
    </ImportLockContext.Provider></LanguageProvider>);
    const button = screen.getByRole('button', { name: '2 in den Katalog verschieben' });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('title', 'Während eines Imports gesperrt');
  });

  it('groups files by source folder and flags unreachable ones', () => {
    render(<LanguageProvider><SourceFilesDialog scans={[scan(), scan({ folderId: 'f2', folderPath: '/home/a/Bambu', files: [file('x.3mf')], truncated: true }), scan({ folderId: 'f3', folderPath: '/weg', files: [], unreachable: true })]}
      onClose={vi.fn()} onImport={vi.fn()} onReject={vi.fn()} onRescan={vi.fn()} /></LanguageProvider>);
    expect(screen.getByRole('region', { name: '/home/a/Downloads' })).toBeInTheDocument();
    expect(within(screen.getByRole('region', { name: '/home/a/Bambu' })).getByText(/sehr vielen Einträgen/)).toBeInTheDocument();
    expect(within(screen.getByRole('region', { name: '/weg' })).getByText('nicht erreichbar')).toBeInTheDocument();
  });
});
