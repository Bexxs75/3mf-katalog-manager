import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { CatalogSetupDialog } from './CatalogSetupDialog';
import { de } from '../i18n/de';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
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
});
