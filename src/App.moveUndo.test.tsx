import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { de } from './i18n/de';
import { makeFolder, makeModelFile, makeModelFileSummary } from './test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));

const folders = [
  makeFolder({ id: 'from', name: 'Alt', path: '/old/Alt', parentId: null }),
  makeFolder({ id: 'to', name: 'Druckerteile', path: '/old/Druckerteile', parentId: null }),
];
const models = [
  makeModelFile({ id: '1', name: 'Halter.3mf', folderId: 'from' }),
  makeModelFile({ id: '2', name: 'Clip.3mf', folderId: 'from' }),
];
// Ids are sorted: the selection follows the visible (name) order, which is not what these tests are about.
const moveCalls = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === 'move_files_to_folder')
  .map(([, args]) => { const { fileIds, folderId } = args as { fileIds: string[]; folderId: string }; return { fileIds: [...fileIds].sort(), folderId }; });

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    width: 800, height: 400, top: 0, left: 0, right: 800, bottom: 400, x: 0, y: 0, toJSON: () => ({}),
  });
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'get_app_version') return '0.15.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    if (cmd === 'list_file_summaries') return models.map(makeModelFileSummary);
    if (cmd === 'list_files_by_ids') return models;
    if (cmd === 'list_folders') return folders;
    if (cmd === 'list_all_file_tags') return {};
    if (cmd === 'move_files_to_folder') {
      return (args as { fileIds: string[] }).fileIds.map((id) => ({ id, status: 'moved', fromFolderId: 'from', fromPath: '/old/Alt' }));
    }
    return [];
  });
});

async function start() {
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  await screen.findAllByText('Halter.3mf');
}
const key = (init: KeyboardEventInit, target: EventTarget = document.body) =>
  act(async () => { target.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init })); });

it('moves the selection with M and the keyboard, then takes it back with Ctrl+Z', async () => {
  await start();
  await key({ key: 'a', ctrlKey: true });
  await key({ key: 'm' });
  const dialog = await screen.findByRole('dialog', { name: '2 Modelle verschieben nach …' });
  const tree = within(dialog).getByRole('tree', { name: 'Zielordner' });
  fireEvent.keyDown(tree, { key: 'ArrowDown' });
  fireEvent.keyDown(tree, { key: 'ArrowDown' });
  fireEvent.keyDown(tree, { key: 'Enter' });
  await waitFor(() => expect(moveCalls()).toEqual([{ fileIds: ['1', '2'], folderId: 'to' }]));
  expect(await screen.findByText('2 Modelle nach „Druckerteile“ verschoben.')).toBeVisible();
  expect(screen.queryByRole('dialog')).toBeNull();

  await key({ key: 'z', ctrlKey: true });
  await waitFor(() => expect(moveCalls()).toHaveLength(2));
  expect(moveCalls()[1]).toEqual({ fileIds: ['1', '2'], folderId: 'from' });
  expect(await screen.findByText('Verschieben rückgängig: 2 Modelle zurückgelegt.')).toBeVisible();
  // Nothing left to undo.
  await key({ key: 'z', ctrlKey: true });
  expect(moveCalls()).toHaveLength(2);
});

it('offers the same dialog from the selection bar and cancelling calls nothing', async () => {
  await start();
  await key({ key: 'a', ctrlKey: true });
  fireEvent.click(screen.getByRole('button', { name: de.moveBulkLabel }));
  const dialog = await screen.findByRole('dialog');
  fireEvent.click(within(dialog).getByRole('button', { name: 'Abbrechen' }));
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(moveCalls()).toEqual([]);
});

it('uses the single selected model for M when nothing is ticked', async () => {
  await start();
  const card = document.querySelector<HTMLElement>('[data-model-id="1"]')!;
  fireEvent.click(card);
  await key({ key: 'm' }, card);
  expect(await screen.findByRole('dialog', { name: '1 Modell verschieben nach …' })).toBeVisible();
});

it('ignores M while typing in the search field and Ctrl+Z leaves the field its own undo', async () => {
  await start();
  await key({ key: 'a', ctrlKey: true });
  const search = screen.getByPlaceholderText(de.searchPlaceholder);
  search.focus();
  await key({ key: 'm' }, search);
  expect(screen.queryByRole('dialog')).toBeNull();
  const undo = new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, bubbles: true, cancelable: true });
  await act(async () => { search.dispatchEvent(undo); });
  expect(undo.defaultPrevented).toBe(false);
});
