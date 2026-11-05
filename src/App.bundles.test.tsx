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

const files = [
  makeModelFile({ id: '1', name: 'Benchy.3mf', path: '/catalog/benchy.3mf' }),
  makeModelFile({ id: '2', name: 'Benchy small.3mf', path: '/catalog/benchy-small.3mf' }),
  makeModelFile({ id: '3', name: 'Cube.stl', path: '/catalog/cube.stl' }),
];
const bundle = {
  id: 'b1', name: 'Harbour', kind: 'variants', createdAt: '2026-11-01T00:00:00Z',
  members: [{ fileId: '1', position: 0, label: null, inTrash: false }, { fileId: '2', position: 1, label: null, inTrash: false }],
};

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'get_app_version') return '0.15.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    if (cmd === 'list_file_summaries') return files.map(file => ({ ...makeModelFileSummary(file), fileType: file.path.endsWith('.stl') ? 'stl' : '3mf' }));
    if (cmd === 'list_files_by_ids') return files.filter(m => (args as { ids: string[] }).ids.includes(m.id));
    if (cmd === 'list_bundles') return [bundle];
    if (cmd === 'list_all_file_tags') return {};
    return [];
  });
});

async function open() {
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  await screen.findAllByText('Cube.stl');
  return screen.getByRole('main');
}
const cardIds = (main: HTMLElement) => [...main.querySelectorAll('[role="row"][data-model-id]')].map(row => row.getAttribute('data-model-id'));

// The names sort as Benchy small, Benchy, Cube; the bundle "Harbour" sorts after Cube.
it('keeps every file on its own card at first and marks the bundle members', async () => {
  const main = await open();
  await waitFor(() => expect(screen.getByRole('switch', { name: de.bundleCollapseToggle })).toBeInTheDocument());
  expect(screen.getByRole('switch', { name: de.bundleCollapseToggle })).toHaveAttribute('aria-checked', 'false');
  expect(cardIds(main)).toEqual(['2', '1', '3']);
  expect(main.querySelectorAll('[data-part-of]')).toHaveLength(2);
});

it('shows the bundle as one card after the switch is turned on, and remembers the choice', async () => {
  const main = await open();
  fireEvent.click(await screen.findByRole('switch', { name: de.bundleCollapseToggle }));
  await waitFor(() => expect(cardIds(main)).toEqual(['3', '1']));
  // The grid stays hidden until it has a measured width, which jsdom never gives, so look it up by attribute.
  expect(main.querySelector('[data-bundle-id="b1"]')).toHaveAttribute('aria-label', 'Bündel Harbour, 2 Varianten');
  expect(localStorage.getItem('3mf-katalog-bundle-collapse')).toBe('on');
});

it('starts with the bundle as one card when the choice was saved, and finds it by its name', async () => {
  localStorage.setItem('3mf-katalog-bundle-collapse', 'on');
  const main = await open();
  await waitFor(() => expect(cardIds(main)).toEqual(['3', '1']));
  fireEvent.change(screen.getByPlaceholderText(de.searchPlaceholder), { target: { value: 'harbour' } });
  await waitFor(() => expect(cardIds(main)).toEqual(['1']));
});

it('does not find the files by the bundle name while the bundle is not shown as a card', async () => {
  const main = await open();
  await screen.findByRole('switch', { name: de.bundleCollapseToggle });
  fireEvent.change(screen.getByPlaceholderText(de.searchPlaceholder), { target: { value: 'harbour' } });
  await waitFor(() => expect(cardIds(main)).toEqual([]));
});
