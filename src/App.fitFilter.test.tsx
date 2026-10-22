import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { de } from './i18n/de';
import { makeModelFile, makeModelFileSummary, makePrinter } from './test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));

const small = makeModelFile({ id: '1', name: 'Small.stl', dimensionsMm: [50, 50, 50] });
const wide = makeModelFile({ id: '2', name: 'Wide.stl', dimensionsMm: [300, 50, 50] });
const unknown = makeModelFile({ id: '3', name: 'Unknown.stl', dimensionsMm: null });
const files = [small, wide, unknown];
const printers = [makePrinter({ id: 'x1c', name: 'X1C' }), makePrinter({ id: 'mini', name: 'Mini', bedXMm: 180, bedYMm: 180, bedZMm: 180 })];

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'get_app_version') return '0.15.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    if (cmd === 'list_printers') return printers;
    if (cmd === 'list_file_summaries') return files.map(makeModelFileSummary);
    if (cmd === 'list_files_by_ids') return files.filter(m => (args as { ids: string[] }).ids.includes(m.id));
    if (cmd === 'list_all_file_tags') return {};
    return [];
  });
});

it('filters by printer, keeps models without size visible, shows a chip, and resets with "clear all"', async () => {
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  await screen.findAllByText('Small.stl');
  const select = await screen.findByRole('combobox', { name: de.fitPrinterFilter });
  await waitFor(() => expect(within(select).getAllByRole('option')).toHaveLength(3));
  const main = screen.getByRole('main');
  expect(main).toHaveTextContent('Wide.stl');

  fireEvent.change(select, { target: { value: 'mini' } });
  await waitFor(() => expect(main).not.toHaveTextContent('Wide.stl'));
  expect(main).toHaveTextContent('Small.stl');
  expect(main).toHaveTextContent('Unknown.stl');
  const bar = screen.getByRole('region', { name: de.filterBarAria });
  expect(bar.querySelector('[data-filter-kind]')).toHaveTextContent(de.fitPrinterFilter);
  expect(bar).toHaveTextContent('Mini');

  fireEvent.click(screen.getByRole('button', { name: de.filterBarClearAll }));
  await waitFor(() => expect(main).toHaveTextContent('Wide.stl'));
  expect(screen.getByRole('combobox', { name: de.fitPrinterFilter })).toHaveValue('');
  expect(bar.querySelectorAll('[data-filter-kind]')).toHaveLength(0);
});
