import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
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

const benchy = makeModelFile({ id: '1', name: 'Benchy.3mf', path: '/catalog/benchy.3mf' });
const cube = makeModelFile({ id: '2', name: 'Cube.stl', path: '/catalog/cube.stl' });
const duct = makeModelFile({ id: '3', name: 'Duct.gcode', path: '/catalog/duct.gcode', printDataState: 'sliced' });
const box = makeModelFile({ id: '4', name: 'Box.bgcode', path: '/catalog/box.bgcode', printDataState: 'not_recognized' });
let files = [benchy, cube, duct, box];
const summary = (model: ReturnType<typeof makeModelFile>) => ({ ...makeModelFileSummary(model), fileType: model.path.endsWith('code') ? 'gcode' : model.path.endsWith('.ctb') ? 'resin' : model.path.endsWith('.stl') ? 'stl' : '3mf' });

beforeEach(() => {
  files = [benchy, cube, duct, box];
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'get_app_version') return '0.15.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    if (cmd === 'list_file_summaries') return files.map(summary);
    if (cmd === 'list_files_by_ids') return files.filter(m => (args as { ids: string[] }).ids.includes(m.id));
    if (cmd === 'list_all_file_tags') return {};
    return [];
  });
});

async function open() {
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  await screen.findAllByText('Benchy.3mf');
  return { main: screen.getByRole('main'), bar: screen.getByRole('region', { name: de.filterBarAria }) };
}

it('offers the file type filter once the catalog holds G-code, with counts, and filters the list', async () => {
  const { main, bar } = await open();
  const group = within(bar).getByRole('group', { name: de.fileKindFilter });
  const button = (name: string) => within(group).getByRole('button', { name: new RegExp(`^${name}`) });
  expect(button(de.fileKindAll)).toHaveTextContent('4');
  expect(button(de.fileKindModels)).toHaveTextContent('2');
  expect(button(de.fileKindGcode)).toHaveTextContent('2');
  expect(button(de.fileKindAll)).toHaveAttribute('aria-pressed', 'true');

  fireEvent.click(button(de.fileKindGcode));
  await waitFor(() => expect(main).not.toHaveTextContent('Benchy.3mf'));
  expect(main).toHaveTextContent('Duct.gcode');
  expect(main).toHaveTextContent('Box.bgcode');
  expect(main).not.toHaveTextContent('Cube.stl');
  expect(button(de.fileKindGcode)).toHaveAttribute('aria-pressed', 'true');
  expect(within(bar).getByText('2 von 4')).toBeInTheDocument();
  // The active choice also shows as a removable chip.
  expect(bar.querySelector('[data-filter-kind]')).toHaveTextContent(de.fileKindFilter);

  fireEvent.click(button(de.fileKindModels));
  await waitFor(() => expect(main).toHaveTextContent('Benchy.3mf'));
  expect(main).toHaveTextContent('Cube.stl');
  expect(main).not.toHaveTextContent('Duct.gcode');

  fireEvent.click(within(bar).getByRole('button', { name: de.chipRemove.replace('{label}', `${de.fileKindFilter} ${de.fileKindModels}`) }));
  await waitFor(() => expect(main).toHaveTextContent('Duct.gcode'));
  expect(main).toHaveTextContent('Benchy.3mf');
});

it('is reset by "clear all filters"', async () => {
  const { main, bar } = await open();
  fireEvent.click(within(within(bar).getByRole('group', { name: de.fileKindFilter })).getByRole('button', { name: new RegExp(`^${de.fileKindGcode}`) }));
  await waitFor(() => expect(main).not.toHaveTextContent('Benchy.3mf'));
  fireEvent.click(within(bar).getByRole('button', { name: de.filterBarClearAll }));
  await waitFor(() => expect(main).toHaveTextContent('Benchy.3mf'));
  expect(main).toHaveTextContent('Duct.gcode');
});

it('does not show the file type filter in a catalog without G-code', async () => {
  files = [benchy, cube];
  const { bar } = await open();
  expect(within(bar).queryByRole('group', { name: de.fileKindFilter })).toBeNull();
});

it('offers "Resin" with its own count and filters the list to resin files', async () => {
  files = [benchy, cube, duct, makeModelFile({ id: '5', name: 'Layers.ctb', path: '/catalog/layers.ctb' })];
  const { main, bar } = await open();
  const group = within(bar).getByRole('group', { name: de.fileKindFilter });
  const button = (name: string) => within(group).getByRole('button', { name: new RegExp(`^${name}`) });
  expect(button(de.fileKindAll)).toHaveTextContent('4');
  expect(button(de.fileKindModels)).toHaveTextContent('2');
  expect(button(de.fileKindGcode)).toHaveTextContent('1');
  expect(button(de.fileKindResin)).toHaveTextContent('1');
  fireEvent.click(button(de.fileKindResin));
  await waitFor(() => expect(main).not.toHaveTextContent('Benchy.3mf'));
  expect(main).toHaveTextContent('Layers.ctb');
  expect(main).not.toHaveTextContent('Duct.gcode');
  expect(within(bar).getByText('1 von 4')).toBeInTheDocument();
});

it('offers the file type filter for a catalog that holds only resin files besides models', async () => {
  files = [benchy, makeModelFile({ id: '5', name: 'Layers.ctb', path: '/catalog/layers.ctb' })];
  const { bar } = await open();
  expect(within(bar).getByRole('group', { name: de.fileKindFilter })).toBeInTheDocument();
});
