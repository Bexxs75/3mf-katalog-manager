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
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));

// "Hidden" is selected first (sorted by name), "Visible" survives both filters below.
const hidden = makeModelFile({ id: '1', name: 'Hidden.stl', printStatus: 'not_printed', license: null });
const visible = makeModelFile({ id: '2', name: 'Visible.stl', printStatus: 'printed', license: 'cc-by' });

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
    if (cmd === 'list_file_summaries') return [hidden, visible].map(makeModelFileSummary);
    if (cmd === 'list_files_by_ids') return (args as { ids?: string[] })?.ids ? [hidden, visible].filter(m => (args as { ids: string[] }).ids.includes(m.id)) : [hidden, visible];
    if (cmd === 'list_all_file_tags') return {};
    return [];
  });
});

async function selectHiddenModel() {
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  await screen.findAllByText('Hidden.stl');
  fireEvent.click(screen.getAllByRole('checkbox')[0]);
  expect(screen.getByText('Aus Katalog entfernen')).toBeTruthy();
}

it.each([
  ['print status', de.printStatusFilter, 'printed'],
  ['license usage', de.licenseUsage, 'commercial'],
])('clears the bulk selection when the %s filter hides the selected model', async (_label, filterName, value) => {
  await selectHiddenModel();
  fireEvent.change(screen.getByRole('combobox', { name: filterName }), { target: { value } });
  await waitFor(() => expect(screen.queryAllByText('Hidden.stl')).toHaveLength(0));
  expect(screen.getAllByText('Visible.stl').length).toBeGreaterThan(0);
  expect(screen.queryByText('Aus Katalog entfernen')).toBeNull();
});
