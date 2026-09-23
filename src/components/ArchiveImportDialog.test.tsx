import { describe, expect, it, vi, beforeEach } from 'vitest';
import { act, render, screen, fireEvent, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ArchiveImportDialog } from './ArchiveImportDialog';
import type { ArchiveInfo } from '../types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

function info(overrides: Partial<ArchiveInfo>): ArchiveInfo {
  return {
    path: '/dl/Benchy.zip',
    suggestedFolderName: 'Benchy',
    modelCount: 2,
    entryCount: 3,
    unpackedSize: 1024,
    fileSize: 500,
    modifiedUnixMs: 42,
    status: 'ok',
    ...overrides,
  };
}

const ARCHIVES = [
  info({}),
  info({ path: '/dl/Drache.7z', suggestedFolderName: 'Drache' }),
  info({ path: '/dl/Lies.rar', suggestedFolderName: 'Lies', modelCount: 0, status: 'noModels' }),
];

function mockBackend(conflicts: Record<string, boolean[]>) {
  vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === 'archive_target_conflicts') {
      const { targetDir } = args as { targetDir: string };
      return Promise.resolve(conflicts[targetDir] ?? [false, false]);
    }
    if (cmd === 'pick_folder_path') return Promise.resolve('/anders');
    if (cmd === 'extract_archives') return Promise.resolve({ imported: [], duplicateCount: 0, archives: [] });
    return Promise.resolve(undefined);
  });
}

function renderDialog(defaultTargetDir: string | null, onDone = vi.fn(), onCancel = vi.fn()) {
  localStorage.setItem('3mf-katalog-language', 'de');
  render(
    <LanguageProvider>
      <ArchiveImportDialog archives={ARCHIVES} defaultTargetDir={defaultTargetDir} onCancel={onCancel} onDone={onDone} />
    </LanguageProvider>,
  );
  return { onDone, onCancel };
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe('ArchiveImportDialog', () => {
  it('prefills the target, counts only extractable archives and explains skipped ones', async () => {
    mockBackend({});
    renderDialog('/katalog');
    expect(screen.getByText('/katalog')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Entpacken (2)' })).toBeEnabled();
    expect(screen.getByText('keine Modelle gefunden – wird übersprungen')).toBeInTheDocument();
  });

  it('disables extraction until a target folder is chosen', async () => {
    mockBackend({});
    renderDialog(null);
    expect(screen.getByRole('button', { name: 'Entpacken (2)' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Ändern…' }));
    await waitFor(() => expect(screen.getByText('/anders')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Entpacken (2)' })).toBeEnabled();
  });

  it('shows the conflict choice only for conflicting archives and re-checks after a target change', async () => {
    mockBackend({ '/katalog': [false, true], '/anders': [false, false] });
    renderDialog('/katalog');
    await waitFor(() => expect(screen.getByText('Ordner „Drache" existiert bereits:')).toBeInTheDocument());
    expect(screen.queryByText('Ordner „Benchy" existiert bereits:')).not.toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Neuen Ordner mit Nummer anlegen' })).toHaveAttribute('aria-checked', 'true');

    fireEvent.click(screen.getByRole('button', { name: 'Ändern…' }));
    await waitFor(() => expect(screen.queryByText('Ordner „Drache" existiert bereits:')).not.toBeInTheDocument());
    expect(invoke).toHaveBeenCalledWith('archive_target_conflicts', { targetDir: '/anders', folderNames: ['Benchy', 'Drache'] });
  });

  it('sends defaults (new folder, keep archives) unless the user changes them', async () => {
    mockBackend({ '/katalog': [false, true] });
    const { onDone } = renderDialog('/katalog');
    await waitFor(() => screen.getByRole('radio', { name: 'Zusammenführen (vorhandene Dateien bleiben unverändert)' }));
    expect(screen.getByRole('checkbox')).not.toBeChecked();

    fireEvent.click(screen.getByRole('radio', { name: 'Zusammenführen (vorhandene Dateien bleiben unverändert)' }));
    fireEvent.click(screen.getByRole('checkbox'));
    fireEvent.click(screen.getByRole('button', { name: 'Entpacken (2)' }));

    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith('extract_archives', {
      targetDir: '/katalog',
      deleteArchives: true,
      requests: [
        { path: '/dl/Benchy.zip', folderName: 'Benchy', onConflict: 'new', expectedSize: 500, expectedModifiedUnixMs: 42 },
        { path: '/dl/Drache.7z', folderName: 'Drache', onConflict: 'merge', expectedSize: 500, expectedModifiedUnixMs: 42 },
      ],
    });
  });

  it('clears stale progress labels when extraction is started again after an error', async () => {
    let emit: ((event: { payload: { path: string; state: string } }) => void) | undefined;
    vi.mocked(listen).mockImplementation(((_name: string, handler: typeof emit) => {
      emit = handler;
      return Promise.resolve(() => {});
    }) as never);
    mockBackend({});
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'archive_target_conflicts') return Promise.resolve([false, false]);
      if (cmd === 'extract_archives') return Promise.reject('Zielordner wurde nicht ueber die App ausgewaehlt');
      return Promise.resolve(undefined);
    });
    renderDialog('/katalog');
    await waitFor(() => expect(emit).toBeDefined());
    act(() => emit!({ payload: { path: '/dl/Benchy.zip', state: 'failed' } }));
    expect(screen.getByText('fehlgeschlagen')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Entpacken (2)' }));
    await waitFor(() => expect(screen.getByText('Zielordner wurde nicht ueber die App ausgewaehlt')).toBeInTheDocument());
    expect(screen.queryByText('fehlgeschlagen')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Entpacken (2)' })).toBeEnabled();
  });

  it('cancel closes without calling the backend', () => {
    mockBackend({});
    const { onCancel } = renderDialog('/katalog');
    fireEvent.click(screen.getByRole('button', { name: 'Abbrechen' }));
    expect(onCancel).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith('extract_archives', expect.anything());
  });
});
