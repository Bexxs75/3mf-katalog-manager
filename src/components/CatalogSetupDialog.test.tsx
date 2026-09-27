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
  describe('"Set up a new location"', () => {
    type Handler = (args: Record<string, unknown> | undefined) => unknown;
    const mockNewLocation = (overrides: Record<string, Handler> = {}) =>
      vi.mocked(invoke).mockImplementation((cmd: string, rawArgs?: unknown) => {
        const args = rawArgs as Record<string, unknown> | undefined;
        const handlers: Record<string, Handler> = {
          default_catalog_parent: () => '/home/anna/Dokumente',
          preview_catalog_dir: (a) => ({ path: `${a?.parent}/${a?.name}`, state: 'new' }),
          create_catalog_dir: (a) => `${a?.parent}/${a?.name}`,
          pick_folder_path: () => '/anders',
          ...overrides,
        };
        const handler = handlers[cmd];
        if (!handler) return Promise.resolve(undefined);
        try {
          return Promise.resolve(handler(args));
        } catch (e) {
          return Promise.reject(e);
        }
      });

    const renderDialog = (onBaseDirSet = vi.fn()) => {
      render(
        <LanguageProvider>
          <CatalogSetupDialog onClose={vi.fn()} onLater={vi.fn()} onImported={vi.fn()} onBaseDirSet={onBaseDirSet} />
        </LanguageProvider>,
      );
      fireEvent.click(screen.getByText(de.catalogSetupNewTitle));
    };

    it('suggests the Documents folder and "3D-Katalog" and shows the full path before creating', async () => {
      mockNewLocation();
      renderDialog();
      expect(invoke).not.toHaveBeenCalledWith('pick_folder_path');
      expect(await screen.findByText('/home/anna/Dokumente')).toBeInTheDocument();
      expect(screen.getByLabelText(de.catalogSetupNewNameLabel)).toHaveValue('3D-Katalog');
      expect(await screen.findByText('/home/anna/Dokumente/3D-Katalog')).toBeInTheDocument();
      expect(screen.getByText(/Wird angelegt:/)).toBeInTheDocument();
    });

    it('creates the folder and registers it as the storage location', async () => {
      mockNewLocation();
      const onBaseDirSet = vi.fn();
      renderDialog(onBaseDirSet);
      await screen.findByText('/home/anna/Dokumente/3D-Katalog');
      fireEvent.click(screen.getByText(de.catalogSetupNewCreateButton));
      await waitFor(() => expect(onBaseDirSet).toHaveBeenCalledWith('/home/anna/Dokumente/3D-Katalog'));
      expect(invoke).toHaveBeenCalledWith('create_catalog_dir', { parent: '/home/anna/Dokumente', name: '3D-Katalog' });
      expect(invoke).toHaveBeenCalledWith('register_catalog_base_dir', { path: '/home/anna/Dokumente/3D-Katalog' });
    });

    it('disables the button and names the character for an invalid name', async () => {
      mockNewLocation();
      renderDialog();
      await screen.findByText('/home/anna/Dokumente/3D-Katalog');
      fireEvent.change(screen.getByLabelText(de.catalogSetupNewNameLabel), { target: { value: '3D:Katalog' } });
      expect(screen.getByText('Dieses Zeichen ist in Ordnernamen nicht erlaubt: :')).toBeInTheDocument();
      expect(screen.getByText(de.catalogSetupNewCreateButton)).toBeDisabled();
      expect(screen.queryByText(/Wird angelegt:/)).not.toBeInTheDocument();

      fireEvent.change(screen.getByLabelText(de.catalogSetupNewNameLabel), { target: { value: '  ' } });
      expect(screen.getByText(de.catalogSetupNameEmpty)).toBeInTheDocument();
      expect(screen.getByText(de.catalogSetupNewCreateButton)).toBeDisabled();
    });

    it('says an existing folder will be reused instead of treating it as an error', async () => {
      mockNewLocation({ preview_catalog_dir: (a) => ({ path: `${a?.parent}/${a?.name}`, state: 'existingDir' }) });
      renderDialog();
      expect(await screen.findByText(de.catalogSetupNewExists, { exact: false })).toBeInTheDocument();
      expect(screen.getByText(de.catalogSetupNewCreateButton)).toBeEnabled();
    });

    it('keeps the button disabled until a place is chosen when there is no Documents folder', async () => {
      mockNewLocation({ default_catalog_parent: () => null });
      renderDialog();
      expect(await screen.findByText(de.catalogSetupNewParentNone)).toBeInTheDocument();
      expect(screen.getByText(de.catalogSetupNewCreateButton)).toBeDisabled();

      fireEvent.click(screen.getByText(de.catalogSetupNewPickParentButton));
      expect(await screen.findByText('/anders/3D-Katalog')).toBeInTheDocument();
      expect(screen.getByText(de.catalogSetupNewCreateButton)).toBeEnabled();
    });

    it('can still pick an existing folder directly, without creating anything', async () => {
      mockNewLocation();
      const onBaseDirSet = vi.fn();
      renderDialog(onBaseDirSet);
      fireEvent.click(screen.getByRole('button', { name: de.catalogSetupNewPickExistingLink }));
      await waitFor(() => expect(onBaseDirSet).toHaveBeenCalledWith('/anders'));
      expect(invoke).toHaveBeenCalledWith('register_catalog_base_dir', { path: '/anders' });
      expect(invoke).not.toHaveBeenCalledWith('create_catalog_dir', expect.anything());
    });

    it('explains an existing file with that name in its own words and blocks the button', async () => {
      mockNewLocation({ preview_catalog_dir: (a) => ({ path: `${a?.parent}/${a?.name}`, state: 'existingFile' }) });
      renderDialog();
      expect(await screen.findByText(de.catalogSetupNewExistingFile)).toBeInTheDocument();
      expect(screen.getByText(de.catalogSetupNewCreateButton)).toBeDisabled();
      expect(screen.queryByText(/Wird angelegt:/)).not.toBeInTheDocument();
    });

    it('blocks a name that is a link', async () => {
      mockNewLocation({ preview_catalog_dir: (a) => ({ path: `${a?.parent}/${a?.name}`, state: 'link' }) });
      renderDialog();
      expect(await screen.findByText(de.catalogSetupNewLink)).toBeInTheDocument();
      expect(screen.getByText(de.catalogSetupNewCreateButton)).toBeDisabled();
    });

    it('keeps the last result line while the next preview loads', async () => {
      let release: (() => void) | null = null;
      mockNewLocation();
      renderDialog();
      await screen.findByText('/home/anna/Dokumente/3D-Katalog');
      vi.mocked(invoke).mockImplementation((cmd: string) =>
        cmd === 'preview_catalog_dir'
          ? new Promise((resolve) => {
              release = () => resolve({ path: '/home/anna/Dokumente/Neu', state: 'new' });
            })
          : Promise.resolve(undefined),
      );
      fireEvent.change(screen.getByLabelText(de.catalogSetupNewNameLabel), { target: { value: 'Neu' } });
      expect(screen.getByText('/home/anna/Dokumente/3D-Katalog')).toBeInTheDocument();
      release!();
      expect(await screen.findByText('/home/anna/Dokumente/Neu')).toBeInTheDocument();
    });

    it('shows a failing preview to the user', async () => {
      mockNewLocation({
        preview_catalog_dir: () => {
          throw { message: 'Keine Berechtigung, dort einen Ordner anzulegen', expected: true };
        },
      });
      renderDialog();
      expect(await screen.findByText('Keine Berechtigung, dort einen Ordner anzulegen')).toBeInTheDocument();
    });

    it('shows a rejected create_catalog_dir with ErrorText', async () => {
      mockNewLocation({
        create_catalog_dir: () => {
          throw { message: 'Platte voll', expected: false };
        },
      });
      const onBaseDirSet = vi.fn();
      renderDialog(onBaseDirSet);
      await screen.findByText('/home/anna/Dokumente/3D-Katalog');
      fireEvent.click(screen.getByText(de.catalogSetupNewCreateButton));
      expect(await screen.findByText('Platte voll')).toBeInTheDocument();
      expect(screen.getByText('Problem melden')).toBeInTheDocument();
      expect(onBaseDirSet).not.toHaveBeenCalled();
    });

    it('shows its own hint instead of the backend text when a file appeared meanwhile', async () => {
      let createCalled = false;
      mockNewLocation({
        preview_catalog_dir: (a) => ({ path: `${a?.parent}/${a?.name}`, state: createCalled ? 'existingFile' : 'new' }),
        create_catalog_dir: () => {
          createCalled = true;
          throw { message: 'Unter diesem Namen gibt es dort schon eine Datei', expected: true };
        },
      });
      renderDialog();
      await screen.findByText('/home/anna/Dokumente/3D-Katalog');
      fireEvent.click(screen.getByText(de.catalogSetupNewCreateButton));
      expect(await screen.findByText(de.catalogSetupNewExistingFile)).toBeInTheDocument();
      expect(screen.queryByText('Unter diesem Namen gibt es dort schon eine Datei')).not.toBeInTheDocument();
    });

    it('handles a failing folder picker for "pick an existing folder directly"', async () => {
      mockNewLocation({
        pick_folder_path: () => {
          throw { message: 'Dialog kaputt', expected: false };
        },
      });
      renderDialog();
      fireEvent.click(screen.getByRole('button', { name: de.catalogSetupNewPickExistingLink }));
      expect(await screen.findByText('Dialog kaputt')).toBeInTheDocument();
    });

    it('goes back to the two choices', async () => {
      mockNewLocation();
      renderDialog();
      await screen.findByText('/home/anna/Dokumente');
      fireEvent.click(screen.getByText(de.catalogSetupBack));
      expect(screen.queryByText(de.catalogSetupNewNameLabel)).not.toBeInTheDocument();
    });
  });
});
