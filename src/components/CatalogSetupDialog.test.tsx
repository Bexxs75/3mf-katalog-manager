import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { CatalogSetupDialog } from './CatalogSetupDialog';
import { de } from '../i18n/de';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  localStorage.setItem('3mf-katalog-language', 'de');
});

describe('CatalogSetupDialog', () => {
  it('registers an adopted folder as catalog base dir, even when it contains no models', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'pick_folder_path') return Promise.resolve('/modelle');
      if (cmd === 'list_folders') return Promise.resolve([]);
      if (cmd === 'import_dropped') return Promise.resolve({ imported: [], duplicateCount: 0, pendingArchives: [] });
      return Promise.resolve(undefined);
    });
    const onBaseDirSet = vi.fn();
    localStorage.setItem('3mf-katalog-language', 'de');
    render(
      <LanguageProvider>
        <CatalogSetupDialog onClose={vi.fn()} onLater={vi.fn()} onImported={vi.fn()} onBaseDirSet={onBaseDirSet} />
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByText(de.catalogSetupAdoptTitle));
    await waitFor(() => expect(onBaseDirSet).toHaveBeenCalledWith('/modelle'));
    expect(invoke).toHaveBeenCalledWith('register_catalog_base_dir', { path: '/modelle' });
  });

  it('offers "Report problem" for an unexpected setup error but not for an expected one', async () => {
    const mockCommands = (registerRejection: unknown) =>
      vi.mocked(invoke).mockImplementation((cmd: string) => {
        if (cmd === 'pick_folder_path') return Promise.resolve('/modelle');
        if (cmd === 'list_folders') return Promise.resolve([]);
        if (cmd === 'import_dropped') return Promise.resolve({ imported: [], duplicateCount: 0, pendingArchives: [] });
        if (cmd === 'register_catalog_base_dir') return Promise.reject(registerRejection);
        return Promise.resolve(undefined);
      });

    mockCommands({ message: 'x', expected: false });
    render(
      <LanguageProvider>
        <CatalogSetupDialog onClose={vi.fn()} onLater={vi.fn()} onImported={vi.fn()} onBaseDirSet={vi.fn()} />
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByText(de.catalogSetupAdoptTitle));
    await screen.findByText('Problem melden');

    mockCommands({ message: 'x', expected: true });
    fireEvent.click(screen.getByText(de.catalogSetupAdoptTitle));
    await waitFor(() => expect(screen.queryByText('Problem melden')).not.toBeInTheDocument());
    expect(screen.getByText('x')).toBeInTheDocument();
  });
});
