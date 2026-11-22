import { uiSettings } from './lib/uiSettings';
import { currentVersion as bundledVersion } from './lib/releaseNotes';
import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { de } from './i18n/de';
import { makeModelFile, makeModelFileSummary } from './test/factories';
import { clickBulkMore } from './test/bulkBar';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
// WebGL rendering is unrelated to catalog actions and unavailable in jsdom.
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));

beforeEach(() => {
  // jsdom has no layout; visible catalog interactions need a measured grid.
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    width: 800, height: 400, top: 0, left: 0, right: 800, bottom: 400, x: 0, y: 0, toJSON: () => ({}),
  });
  localStorage.clear();
  uiSettings.setItem('3mf-katalog-last-seen-version', bundledVersion);
  uiSettings.setItem('3mf-katalog-base-dir', '/old');
  uiSettings.setItem('3mf-katalog-setup-seen', '1');
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd) => {
    if (cmd === 'get_app_version') return '0.15.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    if (cmd === 'reset_catalog') return { modelCount: 0, folderCount: 0 };
    return [];
  });
});

it('opens first-run setup after reset and reloads catalog data', async () => {
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  fireEvent.click(screen.getByRole('button', { name: de.settingsTitle }));
  fireEvent.click(screen.getByText(de.settingsTabMaintenance, { selector: 'button' }));
  fireEvent.click(screen.getByText('Katalog zurücksetzen …'));
  fireEvent.click(screen.getByRole('button', { name: de.resetConfirm }));
  expect(await screen.findByText(de.catalogSetupAdoptTitle)).toBeTruthy();
  expect(uiSettings.getItem('3mf-katalog-base-dir')).toBeNull();
  expect(uiSettings.getItem('3mf-katalog-setup-seen')).toBeNull();
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
  fireEvent.click(screen.getByText('Aus Katalog entfernen'));
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
  clickBulkMore('Aus Katalog entfernen');
  expect(screen.getByText('1 Modell aus dem Katalog entfernen?')).toBeTruthy();
  fireEvent.click(screen.getByText('Entfernen'));
  await waitFor(() => expect(screen.queryAllByText('Cube.stl')).toHaveLength(0));
  expect(screen.queryByRole('menuitem', { name: 'Aus Katalog entfernen' })).toBeNull();
  // Nothing is left to select, so the always-visible bar is gone as well.
  expect(screen.queryByRole('toolbar')).toBeNull();
  expect(invoke).toHaveBeenCalledWith('remove_files_from_catalog', { fileIds: ['1'] });
});

it.each([
  ['Als gedruckt markieren', 'set_print_status'],
  ['Zu Favoriten hinzufügen', 'set_favorite'],
  ['Zur Warteschlange hinzufügen', 'add_to_queue'],
  ['In den Papierkorb', 'delete_file'],
  ['Aus Katalog entfernen', 'remove_files_from_catalog'],
])('context action %s applies to every selected model, excluding other models', async (label, command) => {
  const models = ['a', 'b', 'c'].map(id => makeModelFile({ id, name: `${id}.stl`, path: `/old/${id}.stl`, favorite: false, printStatus: 'not_printed', queuePosition: null }));
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'list_file_summaries') return models.map(makeModelFileSummary);
    if (cmd === 'list_files_by_ids') return models.filter(model => (args as { fileIds: string[] }).fileIds.includes(model.id));
    if (cmd === 'list_all_file_tags') return {};
    if (cmd === 'add_to_queue') return 1;
    if (cmd === 'remove_files_from_catalog') return 2;
    return fallback(cmd, args);
  });
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  for (const id of ['a', 'b']) fireEvent.click(document.querySelector(`[data-model-id="${id}"] input[type="checkbox"]`)!);
  fireEvent.contextMenu(document.querySelector('[data-model-id="b"]')!);
  fireEvent.click(screen.getByRole('button', { name: `${label} (2 Modelle)` }));
  if (command === 'delete_file') fireEvent.click(screen.getAllByRole('button', { name: 'In den Papierkorb' }).find(button => button.closest('[data-navigation-menu]'))!);
  if (command === 'remove_files_from_catalog') fireEvent.click(screen.getByRole('button', { name: 'Entfernen' }));
  await waitFor(() => {
    const calls = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === command).map(([, args]) => args);
    if (command === 'remove_files_from_catalog') expect(calls).toEqual([{ fileIds: ['a', 'b'] }]);
    else expect(calls.map(args => (args as { fileId: string }).fileId).sort()).toEqual(['a', 'b']);
  });
});

it('toggles favorites directly in the list row without opening the model', async () => {
  uiSettings.setItem('3mf-katalog-view', 'groupedList');
  const model = makeModelFile({ id: 'a', name: 'list.stl', favorite: false });
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'set_favorite') { model.favorite = true; return; }
    if (cmd === 'list_file_summaries') return [makeModelFileSummary(model)];
    if (cmd === 'list_files_by_ids') return [model];
    if (cmd === 'list_all_file_tags') return {};
    return fallback(cmd, args);
  });
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  const row = document.querySelector('[data-model-id="a"]')!;
  fireEvent.click(row.querySelector('button[aria-pressed="false"]')!);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('set_favorite', { fileId: 'a', favorite: true }));
  expect(document.querySelector('[data-model-id="a"] button[aria-pressed="true"]')).toHaveAttribute('aria-label', de.favoriteRemove);
});
