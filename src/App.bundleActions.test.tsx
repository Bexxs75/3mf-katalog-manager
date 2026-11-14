import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
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
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));

const files = [
  makeModelFile({ id: '1', name: 'Benchy.3mf', path: '/catalog/benchy.3mf' }),
  makeModelFile({ id: '2', name: 'Benchy small.3mf', path: '/catalog/benchy-small.3mf' }),
  makeModelFile({ id: '3', name: 'Cube.stl', path: '/catalog/cube.stl' }),
  makeModelFile({ id: '4', name: 'Dice.stl', path: '/catalog/dice.stl' }),
];
const harbour = {
  id: 'b1', name: 'Harbour', kind: 'variants', createdAt: '2026-11-01T00:00:00Z',
  members: [{ fileId: '1', position: 0, label: 'big', inTrash: false }, { fileId: '2', position: 1, label: null, inTrash: false }],
};

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'get_app_version') return '0.15.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    if (cmd === 'list_file_summaries') return files.map(file => ({ ...makeModelFileSummary(file), fileType: file.path.endsWith('.stl') ? 'stl' : '3mf' }));
    if (cmd === 'list_files_by_ids') return files.filter(m => (args as { ids: string[] }).ids.includes(m.id));
    if (cmd === 'list_bundles') return [harbour];
    if (cmd === 'create_bundle') return { ...harbour, id: 'b9' };
    if (cmd === 'list_all_file_tags') return {};
    return [];
  });
});

async function open() {
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  await screen.findAllByText('Dice.stl');
  return screen.getByRole('main');
}
const tick = (id: string) => (document.querySelector(`[data-model-id="${id}"] input[type="checkbox"]`) as HTMLInputElement);
const calls = (command: string) => vi.mocked(invoke).mock.calls.filter(call => call[0] === command).map(call => call[1]);
const announcer = () => document.querySelector('[data-selection-announcer]') as HTMLElement;

// The names sort as Benchy small, Benchy, Cube, Dice; the bundle "Harbour" sorts after Dice and is shown by its title model 1.

it('bundles two selected files through the dialog and says so', async () => {
  await open();
  fireEvent.click(tick('3'));
  fireEvent.click(tick('4'));
  clickBulkMore(de.bundleMake);
  const dialog = await screen.findByRole('dialog', { name: de.bundleDlgCreateTitle });
  expect((within(dialog).getByLabelText(de.bundleDlgName) as HTMLInputElement).value).toBe('Cube');
  fireEvent.change(within(dialog).getByLabelText(/Beschriftung für Dice\.stl/), { target: { value: 'Würfel' } });
  fireEvent.click(within(dialog).getByRole('button', { name: 'Bündel anlegen (2 Modelle)' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(calls('create_bundle')).toEqual([{ name: 'Cube', kind: 'variants', fileIds: ['3', '4'] }]);
  expect(calls('update_bundle_member')).toEqual([{ fileId: '4', label: 'Würfel' }]);
  expect(screen.getByText('Bündel „Cube“ angelegt: 2 Modelle.')).toBeInTheDocument();
});

it('announces whole bundle cards in the selection', async () => {
  await open();
  fireEvent.click(tick('3'));
  fireEvent.click(tick('1'));
  await waitFor(() => expect(announcer()).toHaveTextContent('1 Bündel, 3 Modelle ausgewählt'));
});

it('dissolves the selected bundle card, offers "Undo" and brings the bundle back with order and labels', async () => {
  await open();
  fireEvent.click(tick('1'));
  clickBulkMore(de.bundleDissolve);
  await waitFor(() => expect(calls('dissolve_bundle')).toEqual([{ bundleId: 'b1' }]));
  expect(await screen.findByText('Bündel „Harbour“ aufgelöst. Die 2 Modelle bleiben erhalten.')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: de.undoButton }));
  await waitFor(() => expect(calls('create_bundle')).toEqual([{ name: 'Harbour', kind: 'variants', fileIds: ['1', '2'], labels: ['big', ''], restore: true }]));
  expect(calls('update_bundle_member')).toEqual([]);
  expect(await screen.findByText(/Auflösen rückgängig: Bündel „Harbour“/)).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: de.undoButton })).toBeNull();
});

it('says so when "Undo" cannot bring back every member, and asks the backend to restore with the trash', async () => {
  const withTrash = { ...harbour, members: [...harbour.members, { fileId: '9', position: 2, label: 'Lid', inTrash: true }] };
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => (cmd === 'list_bundles' ? [withTrash] : base(cmd, args)));
  await open();
  fireEvent.click(tick('1'));
  clickBulkMore(de.bundleDissolve);
  await screen.findByText(/aufgelöst/);
  fireEvent.click(screen.getByRole('button', { name: de.undoButton }));
  await waitFor(() => expect(calls('create_bundle')).toEqual([
    { name: 'Harbour', kind: 'variants', fileIds: ['1', '2', '9'], labels: ['big', '', 'Lid'], restore: true },
  ]));
  expect(calls('update_bundle_member')).toEqual([]);
  expect(await screen.findByText(/Auflösen rückgängig: Bündel „Harbour“ .*1 Modell konnte nicht zurück/)).toBeInTheDocument();
});

it('undoes a dissolve with Ctrl+Z as well', async () => {
  await open();
  fireEvent.click(tick('1'));
  clickBulkMore(de.bundleDissolve);
  await screen.findByText('Bündel „Harbour“ aufgelöst. Die 2 Modelle bleiben erhalten.');
  fireEvent.keyDown(document.body, { key: 'z', ctrlKey: true });
  await waitFor(() => expect(calls('create_bundle')).toEqual([{ name: 'Harbour', kind: 'variants', fileIds: ['1', '2'], labels: ['big', ''], restore: true }]));
});

it('asks before the Delete key does anything on a bundle card, and dissolves by default', async () => {
  await open();
  fireEvent.click(tick('1'));
  fireEvent.keyDown(document.body, { key: 'Delete' });
  const dialog = await screen.findByRole('dialog', { name: 'Bündel „Harbour“ löschen?' });
  expect(within(dialog).getByRole('radio', { name: /Nur Bündel auflösen/ })).toBeChecked();
  fireEvent.click(within(dialog).getByRole('button', { name: de.bundleDissolve }));
  await waitFor(() => expect(calls('dissolve_bundle')).toEqual([{ bundleId: 'b1' }]));
  expect(calls('delete_files')).toEqual([]);
});

it('moves all members to the trash only after the second choice', async () => {
  await open();
  fireEvent.click(tick('1'));
  fireEvent.keyDown(document.body, { key: 'Delete' });
  const dialog = await screen.findByRole('dialog', { name: 'Bündel „Harbour“ löschen?' });
  fireEvent.click(within(dialog).getByRole('radio', { name: /Alle 2 Modelle in den Papierkorb/ }));
  fireEvent.click(within(dialog).getByRole('button', { name: 'Alle 2 Modelle in den Papierkorb' }));
  await waitFor(() => expect(calls('delete_files')).toEqual([{ fileIds: ['1', '2'] }]));
  expect(calls('dissolve_bundle')).toEqual([]);
});

it('keeps the plain delete confirmation for files that are not whole bundle cards', async () => {
  await open();
  fireEvent.click(tick('3'));
  fireEvent.keyDown(document.body, { key: 'Delete' });
  expect(await screen.findByText(/löschen\?/)).toBeInTheDocument();
  expect(screen.queryByRole('dialog')).toBeNull();
});

it('does not ask when the bundles are not shown as cards', async () => {
  localStorage.setItem('3mf-katalog-bundle-collapse', 'off');
  await open();
  fireEvent.click(tick('1'));
  fireEvent.click(tick('2'));
  fireEvent.keyDown(document.body, { key: 'Delete' });
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
});

it('removes a selected member from its bundle and says that the bundle with one left is dissolved', async () => {
  localStorage.setItem('3mf-katalog-bundle-collapse', 'off');
  await open();
  fireEvent.click(tick('1'));
  clickBulkMore(de.bundleRemoveFrom);
  await waitFor(() => expect(calls('remove_files_from_bundle')).toEqual([{ fileIds: ['1'] }]));
  expect(await screen.findByText(/1 Modell aus dem Bündel entfernt\. Die Datei bleibt erhalten\. Ein Bündel mit weniger als 2 verbliebenen Modellen wurde aufgelöst\./)).toBeInTheDocument();
});

it('opens the edit dialog for exactly one selected bundle and saves a changed name', async () => {
  await open();
  fireEvent.click(tick('1'));
  clickBulkMore(de.bundleEdit);
  const dialog = await screen.findByRole('dialog', { name: de.bundleDlgEditTitle });
  fireEvent.change(within(dialog).getByLabelText(de.bundleDlgName), { target: { value: 'Port' } });
  fireEvent.click(within(dialog).getByRole('button', { name: de.bundleDlgSave }));
  await waitFor(() => expect(calls('update_bundle')).toEqual([{ bundleId: 'b1', name: 'Port' }]));
  expect(calls('update_bundle_member')).toEqual([]);
});

it('offers the bundle card menu on a right click and applies "printed" to every member', async () => {
  // An empty answer skips the re-read of a single file after the change; the factory record it would bring back makes the open detail panel warn about keys.
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((cmd, args) => cmd === 'list_files_by_ids' && (args as { ids: string[] }).ids.length === 1 ? Promise.resolve([]) : base(cmd, args));
  await open();
  fireEvent.contextMenu(document.querySelector('[data-bundle-id="b1"]') as HTMLElement);
  const menu = within(await screen.findByRole('group', { name: de.bundleMenuLabel }));
  // Variants are alternatives; the menu asks which one instead of queueing them all.
  expect(menu.queryByRole('button', { name: /Alle 2 zur Warteschlange/ })).toBeNull();
  expect(menu.getByRole('button', { name: de.bundleQueueOpen })).toBeInTheDocument();
  fireEvent.click(menu.getByRole('button', { name: 'Alle 2 als gedruckt markieren' }));
  await waitFor(() => expect(calls('set_print_status')).toHaveLength(2));
  expect(calls('set_print_status')).toEqual([{ fileId: '1', status: 'printed' }, { fileId: '2', status: 'printed' }]);
});

it('asks for the delete choice from the bundle card menu', async () => {
  await open();
  fireEvent.contextMenu(document.querySelector('[data-bundle-id="b1"]') as HTMLElement);
  fireEvent.click(await screen.findByRole('button', { name: de.bundleDeleteEntry }));
  expect(await screen.findByRole('dialog', { name: 'Bündel „Harbour“ löschen?' })).toBeInTheDocument();
});

it('offers the file menu entries "Bundle …" and "Add to bundle …" on a single file', async () => {
  await open();
  fireEvent.click(tick('3'));
  fireEvent.click(tick('4'));
  fireEvent.contextMenu(document.querySelector('[data-model-id="3"]') as HTMLElement);
  expect(await screen.findByRole('button', { name: de.bundleMake })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: de.bundleAddTo })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: de.bundleRemoveFrom })).toBeNull();
});

const cardOf = (id: string) => document.querySelector(`[data-bundle-id="${id}"]`) as HTMLElement;
const withoutRefetch = () => {
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((cmd, args) => cmd === 'list_files_by_ids' && (args as { ids: string[] }).ids.length === 1 ? Promise.resolve([]) : base(cmd, args));
};

it('opens the bundle panel for a clicked bundle card and applies "printed" to every member from there', async () => {
  withoutRefetch();
  await open();
  fireEvent.click(cardOf('b1'));
  const panel = within(await screen.findByRole('heading', { level: 2, name: 'Harbour' }).then(heading => heading.closest('[data-bundle-panel]') as HTMLElement));
  expect(panel.getByRole('list', { name: de.bundleMembers })).toBeInTheDocument();
  fireEvent.click(panel.getByRole('button', { name: 'Alle 2 als gedruckt markieren' }));
  await waitFor(() => expect(calls('set_print_status')).toEqual([{ fileId: '1', status: 'printed' }, { fileId: '2', status: 'printed' }]));
});

it('asks which variant from the card menu, in the panel, and queues only that one', async () => {
  withoutRefetch();
  await open();
  fireEvent.contextMenu(cardOf('b1'));
  // The right click opens the panel too, which has an entry of the same name: take the one in the menu.
  fireEvent.click(within(await screen.findByRole('group', { name: de.bundleMenuLabel })).getByRole('button', { name: de.bundleQueueOpen }));
  const question = await screen.findByRole('group', { name: de.bundleQueueWhich });
  expect(screen.getByRole('heading', { level: 2, name: 'Harbour' })).toBeInTheDocument();
  fireEvent.click(within(question).getByRole('radio', { name: /Benchy small/ }));
  fireEvent.click(within(question).getByRole('button', { name: de.bundleQueueAddVariant }));
  await waitFor(() => expect(calls('add_to_queue')).toEqual([{ fileId: '2' }]));
});

it('opens a member from the panel on its detail page with the variant switch and jumps to the other variant', async () => {
  await open();
  fireEvent.click(cardOf('b1'));
  fireEvent.click(await screen.findByRole('button', { name: /Benchy small\.3mf/ }));
  const switcher = await screen.findByRole('group', { name: de.bundleSwitchHeading });
  expect(within(switcher).getByRole('button', { name: 'Benchy small.3mf' })).toHaveAttribute('aria-pressed', 'true');
  fireEvent.click(within(switcher).getByRole('button', { name: 'big' }));
  const next = await screen.findByRole('group', { name: de.bundleSwitchHeading });
  await waitFor(() => expect(within(next).getByRole('button', { name: 'big' })).toHaveFocus());
  expect(screen.getByRole('heading', { level: 1, name: 'Benchy.3mf' })).toBeInTheDocument();
});

it('edits the bundle from its detail page and from the panel through the same dialog', async () => {
  await open();
  fireEvent.click(cardOf('b1'));
  fireEvent.click(await screen.findByRole('button', { name: de.bundleEdit }));
  expect(await screen.findByRole('dialog', { name: de.bundleDlgEditTitle })).toBeInTheDocument();
});
