import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { de } from './i18n/de';
import { makeModelFile, makeModelFileSummary } from './test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
// WebGL rendering is unrelated to catalog actions and unavailable in jsdom.
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd) => {
    if (cmd === 'get_app_version') return '0.15.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update') return null;
    if (cmd === 'reset_catalog') return { modelCount: 0, folderCount: 0 };
    return [];
  });
});

it('opens first-run setup after reset and reloads catalog data', async () => {
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  fireEvent.click(screen.getByRole('button', { name: de.settingsTitle }));
  fireEvent.click(screen.getByText(de.settingsTabCatalog, { selector: 'button' }));
  fireEvent.click(screen.getByText('Katalog zurücksetzen …'));
  fireEvent.click(screen.getByText('Zurücksetzen'));
  expect(await screen.findByText(de.catalogSetupAdoptTitle)).toBeTruthy();
  expect(localStorage.getItem('3mf-katalog-base-dir')).toBeNull();
  expect(localStorage.getItem('3mf-katalog-setup-seen')).toBeNull();
  expect(invoke).toHaveBeenCalledWith('reset_catalog');
  await waitFor(() => expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === 'list_folders').length).toBeGreaterThan(1));
});


it('removes a model through the actual context menu and reloads the catalog', async () => {
  const model = makeModelFile({ id: '1', name: 'Cube.stl' });
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  let removed = false;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'list_file_summaries') return removed ? [] : [makeModelFileSummary(model)];
    if (cmd === 'list_files_by_ids') return removed ? [] : [model];
    if (cmd === 'list_all_file_tags') return {};
    if (cmd === 'remove_files_from_catalog') { removed = true; return 1; }
    return fallback(cmd, args);
  });
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  fireEvent.contextMenu((await screen.findAllByText('Cube.stl'))[0]);
  fireEvent.click(screen.getByText('Aus dem Katalog entfernen'));
  fireEvent.click(screen.getByText('Entfernen'));
  await waitFor(() => expect(screen.queryAllByText('Cube.stl')).toHaveLength(0));
  expect(invoke).toHaveBeenCalledWith('remove_files_from_catalog', { fileIds: ['1'] });
  expect(invoke).not.toHaveBeenCalledWith('delete_file', expect.anything());
});


it('removes a bulk selection through the new command and clears the toolbar', async () => {
  const model = makeModelFile({ id: '1', name: 'Cube.stl' });
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  let removed = false;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'list_file_summaries') return removed ? [] : [makeModelFileSummary(model)];
    if (cmd === 'list_files_by_ids') return removed ? [] : [model];
    if (cmd === 'list_all_file_tags') return {};
    if (cmd === 'remove_files_from_catalog') { removed = true; return 1; }
    return fallback(cmd, args);
  });
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  fireEvent.click(screen.getAllByRole('checkbox')[0]);
  fireEvent.click(screen.getByText('Aus Katalog entfernen'));
  expect(screen.getByText('1 Modelle aus dem Katalog entfernen?')).toBeTruthy();
  fireEvent.click(screen.getByText('Entfernen'));
  await waitFor(() => expect(screen.queryAllByText('Cube.stl')).toHaveLength(0));
  expect(screen.queryByText('Aus Katalog entfernen')).toBeNull();
  expect(invoke).toHaveBeenCalledWith('remove_files_from_catalog', { fileIds: ['1'] });
});
