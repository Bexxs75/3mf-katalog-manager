import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { resetStartSyncSession } from './hooks/useStartSync';
import { de } from './i18n/de';
import { makeFolder } from './test/factories';
import type { StartSyncResult } from './lib/api/diskSync';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));

const syncResult = (over: Partial<StartSyncResult> = {}): StartSyncResult => ({
  ran: true, busy: false, newFiles: 3, renamed: 1, missingFiles: 2, missingFolders: 0, duplicates: 0, unreachableRoots: 0, applied: null, ...over,
});
const startCalls = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === 'run_start_sync').length;
let result: StartSyncResult;
let folders: unknown[];
beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    width: 800, height: 400, top: 0, left: 0, right: 800, bottom: 400, x: 0, y: 0, toJSON: () => ({}),
  });
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  resetStartSyncSession();
  result = syncResult();
  folders = [makeFolder({ id: '1', path: '/old' })];
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd) => {
    if (cmd === 'get_app_version') return '0.15.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    if (cmd === 'list_folders') return folders;
    if (cmd === 'run_start_sync') return result;
    if (cmd === 'scan_catalog_disk') return { newFiles: [], newFolders: [], renamedFiles: [], duplicateFiles: [], missingFiles: [], missingFolders: [], unreachableRoots: [], skippedPaths: [], trashBlockedCount: 0 };
    return [];
  });
});
const mount = () => act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });

it('checks once after the catalog loaded, shows the notice and opens the sync from View', async () => {
  await mount();
  expect(await screen.findByText(/Beim Start gefunden: 3 neue Dateien, 1 umbenannt\/verschoben, 2 Dateien fehlen\./)).toBeTruthy();
  expect(startCalls()).toBe(1);
  // Stays until closed, whatever else happens.
  await act(async () => { await new Promise(r => setTimeout(r, 50)); });
  expect(screen.getByText(/Beim Start gefunden/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: de.startSyncView }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('scan_catalog_disk'));
  expect(screen.queryByText(/Beim Start gefunden/)).toBeNull();
  expect(startCalls()).toBe(1);
});
it('does not check during first-run setup', async () => {
  localStorage.removeItem('3mf-katalog-setup-seen');
  await mount();
  await act(async () => { await new Promise(r => setTimeout(r, 50)); });
  expect(startCalls()).toBe(0);
});
it('does not check for a catalog without folders', async () => {
  folders = [];
  await mount();
  await act(async () => { await new Promise(r => setTimeout(r, 50)); });
  expect(startCalls()).toBe(0);
});
it('stays quiet when the backend reports no changes', async () => {
  result = syncResult({ newFiles: 0, renamed: 0, missingFiles: 0 });
  await mount();
  await waitFor(() => expect(startCalls()).toBe(1));
  await act(async () => { await new Promise(r => setTimeout(r, 50)); });
  expect(screen.queryByText(/Beim Start/)).toBeNull();
});
it('reloads the lists after an automatic take-over and reports it', async () => {
  result = syncResult({ newFiles: 0, renamed: 0, missingFiles: 1, applied: { added: 2, renamed: 1, failed: 0 } });
  await mount();
  expect(await screen.findByText(/Beim Start aufgenommen: 2 neue Dateien, 1 Umbenennung\. Weiter offen: 1 Datei fehlt\./)).toBeTruthy();
  expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === 'list_file_summaries').length).toBeGreaterThan(1);
});
