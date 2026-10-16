import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import App from './App';
import * as transport from './lib/transport';
import { WEB_READ_COMMANDS } from './lib/readOnly';
import { de } from './i18n/de';
import { RuntimeEnvironmentProvider } from './hooks/useRuntimeEnvironment';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { makeFolder, makeModelFile, makeModelFileSummary } from './test/factories';
vi.mock('three', async importOriginal => ({
  ...await importOriginal<typeof import('three')>(),
  WebGLRenderer: function () {
    return { domElement: document.createElement('canvas'), setPixelRatio() {}, setSize() {}, render() {}, forceContextLoss() {}, dispose() {} };
  },
}));
let commands: string[];
const spyInvoke = () => vi.spyOn(transport, 'invoke');
let attempted: ReturnType<typeof spyInvoke>;
const models = [
  makeModelFile({ id: '1', name: 'Alpha.stl', folderId: 'root', tags: ['web-tag'], sourceUrl: 'https://example.org/model', license: 'CC0-1.0', favorite: true, lastViewedAt: '2026-10-01T00:00:00Z' }),
  makeModelFile({ id: '2', name: 'Beta.stl', folderId: 'child', tags: ['web-tag'] }),
];
const trash = makeModelFile({ id: 'trash', name: 'Trashed.stl' });
beforeEach(() => {
  vi.stubEnv('VITE_TARGET', 'web'); localStorage.clear();
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ width: 800, height: 400, top: 0, left: 0, right: 800, bottom: 400, x: 0, y: 0, toJSON: () => ({}) });
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(400);
  attempted = vi.spyOn(transport, 'invoke'); commands = [];
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    const command = url.split('/').slice(-1)[0]; commands.push(command);
    if (command === 'get_model_geometry') {
      const header = new TextEncoder().encode('{"meshes":[],"palette":[]}');
      const buffer = new ArrayBuffer(4 + header.length);
      new DataView(buffer).setUint32(0, header.length, true); new Uint8Array(buffer, 4).set(header);
      return new Response(buffer, { headers: { 'Content-Type': 'application/octet-stream' } });
    }
    const args = JSON.parse(String(options?.body ?? '{}'));
    const value = command === 'list_file_summaries' ? models.map(makeModelFileSummary)
      : command === 'list_files_by_ids' ? models.filter(m => args.ids.includes(m.id))
      : command === 'list_all_file_tags' ? { '1': ['web-tag'], '2': ['web-tag'] }
      : command === 'list_folders' ? [makeFolder({ id: 'root', name: 'Web folder' }), makeFolder({ id: 'child', name: 'Other folder' })]
      : command === 'list_tag_counts' ? [{ label: 'web-tag', count: 2, colorHue: 20 }]
      : command === 'list_collections' ? [{ id: 'collection', name: 'Web collection', modelCount: 2 }]
      : command === 'list_collection_files' ? models
      : command === 'list_trash' ? [trash]
      : command === 'list_print_log_entries' ? [{ id: 'entry', printedAt: '2026-10-01', note: 'Existing print note', photoImage: null }]
      : command === 'get_app_version' ? '0.1.0' : ['has_step_preview', 'is_preview_build'].includes(command) ? false : [];
    return Response.json(value);
  }));
});
afterEach(() => {
  cleanup();
  // Assert attempted commands too: the transport guard must not mask a UI regression.
  expect(attempted.mock.calls.map(([command]) => command).filter(command => !(WEB_READ_COMMANDS as readonly string[]).includes(command))).toEqual([]);
  expect(commands.filter(command => !(WEB_READ_COMMANDS as readonly string[]).includes(command))).toEqual([]);
  vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.unstubAllEnvs();
});
async function start(density = 'compact') {
  localStorage.setItem('3mf-katalog-density', density);
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  await act(async () => { render(<RuntimeEnvironmentProvider><LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics></RuntimeEnvironmentProvider>); });
}
const click = async (element: HTMLElement) => { await act(async () => { fireEvent.click(element); }); };
const modelItem = (name = 'Alpha.stl') => within(screen.getByRole('main')).getByText(name);
async function openDetail() { await act(async () => { fireEvent.click(modelItem()); fireEvent.doubleClick(modelItem()); }); }
function noMutationControls() {
  for (const label of [de.delete, de.uploadModelImageLabel, de.addToQueue, de.addToCollectionLabel, de.rescanMetadataButton, de.printLogAddButton, de.licenseSave, de.showInFileManager, de.openInSlicer, de.restoreLabel, de.emptyTrashButton]) {
    if (typeof label === 'string') expect(screen.queryByRole('button', { name: label })).toBeNull();
  }
  expect(screen.queryByPlaceholderText(de.sourceUrlPlaceholder)).toBeNull();
  expect(screen.queryByRole('checkbox')).toBeNull();
}
it('navigates all read views, search, filters, sorts, and previous/next details using only allowed commands', async () => {
  await start();
  const header = within(screen.getByRole('banner'));
  for (const view of [de.viewList, de.viewFolder, de.viewGrid]) await click(header.getByRole('button', { name: view }));
  const search = screen.getByPlaceholderText(de.searchPlaceholder);
  fireEvent.change(search, { target: { value: 'Beta' } });
  expect(within(screen.getByRole('main')).queryByText('Alpha.stl')).toBeNull();
  fireEvent.change(search, { target: { value: '' } });
  for (const [label, value] of [[de.licenseUsage, 'unknown'], [de.printStatusFilter, 'not_printed']]) {
    fireEvent.change(screen.getByRole('combobox', { name: label }), { target: { value } });
    fireEvent.change(screen.getByRole('combobox', { name: label }), { target: { value: '' } });
  }
  await click(header.getByRole('button', { name: de.sortName }));
  await click(screen.getByRole('menuitemradio', { name: de.sortLastViewed }));
  fireEvent.keyDown(window, { key: 'Escape' });
  await openDetail();
  expect(screen.getByText('Existing print note')).toBeInTheDocument();
  // Viewed sorting has Alpha first (the stored timestamp is not changed).
  await click(screen.getByRole('button', { name: de.detailNext }));
  expect(screen.getByRole('heading', { name: 'Beta.stl' })).toBeInTheDocument();
  await click(screen.getByRole('button', { name: de.detailPrevious }));
  expect(screen.getByRole('heading', { name: 'Alpha.stl' })).toBeInTheDocument();
  await click(screen.getByTitle(de.backToCatalog));
  const sidebar = within(screen.getByRole('separator').closest('aside')!);
  await click(sidebar.getByText('Web folder'));
  await click(sidebar.getByText(de.tagsHeading));
  await click(sidebar.getByRole('button', { name: /#web-tag/ }));
  await click(sidebar.getByRole('button', { name: 'Web collection' }));
  expect(modelItem()).toBeInTheDocument();
  await click(screen.getByRole('button', { name: de.trashHeading }));
  await click(modelItem('Trashed.stl'));
  expect(commands).toEqual(expect.arrayContaining(['list_files_by_ids', 'list_collection_files', 'list_trash', 'list_print_log_entries', 'get_model_geometry']));
});
it.each(['compact', 'comfort'])('hides model editors in %s density and preserves disabled status indicators', async density => {
  await start(density); await click(modelItem());
  noMutationControls();
  for (const button of screen.getAllByRole('button', { name: de.favoriteRemove })) {
    expect(button).toBeDisabled(); expect(button).toHaveAttribute('title', de.webReadOnly); fireEvent.click(button);
  }
  await openDetail(); noMutationControls();
  expect(screen.getByText('Existing print note')).toBeInTheDocument();
  expect(screen.getByRole('link', { name: 'https://example.org/model' })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: de.pmEdit })).toBeNull();
  expect(screen.queryByRole('button', { name: de.chipRemove.replace('{label}', 'web-tag') })).toBeNull();
  expect(screen.getByRole('button', { name: de.notPrintedLabel })).toBeDisabled();
});
it('makes model, folder and collection context menus, drag/drop, bulk shortcuts and Delete inert', async () => {
  await start(); await click(modelItem());
  const before = commands.length;
  const tile = modelItem().closest('[data-model-id]')!;
  const folder = screen.getByText('Other folder').closest<HTMLElement>('[role=button]')!;
  const collection = screen.getByRole('button', { name: 'Web collection' });
  for (const element of [tile, folder, collection]) {
    fireEvent.contextMenu(element); fireEvent.keyDown(element, { key: 'F10', shiftKey: true });
    fireEvent.mouseDown(element, { clientX: 0, clientY: 0 });
    fireEvent.mouseMove(document, { clientX: 100, clientY: 100 });
    fireEvent.mouseEnter(folder); fireEvent.mouseEnter(collection); fireEvent.mouseUp(document);
    fireEvent.dragStart(element); fireEvent.drop(folder); fireEvent.drop(collection);
  }
  for (const key of ['Delete', 'Backspace', ' ']) fireEvent.keyDown(window, { key });
  fireEvent.keyDown(window, { key: 'a', ctrlKey: true }); fireEvent.keyDown(window, { key: 'Backspace', metaKey: true });
  expect(document.querySelector('[data-navigation-menu]')).toBeNull();
  expect(document.querySelector('[data-drag-grip]')).toBeNull();
  noMutationControls(); expect(commands.length).toBe(before);
});
it('keeps collection navigation but hides creation, rename, deletion and folder creation', async () => {
  await start();
  expect(screen.queryByText(de.createFolderLabel)).toBeNull(); expect(screen.queryByText(de.createCollectionLabel)).toBeNull();
  await click(screen.getByText(de.viewAllCollectionsLabel));
  expect(screen.queryByRole('button', { name: de.createCollectionLabel })).toBeNull();
  expect(screen.queryByLabelText(de.renameCollectionAria)).toBeNull();
  expect(screen.queryByLabelText(de.deleteCollectionConfirmQuestion)).toBeNull();
  await click(within(screen.getByRole('main')).getByText('Web collection'));
  expect(modelItem()).toBeInTheDocument();
});
it('shows trash contents without restore, permanent delete or empty-trash actions', async () => {
  await start(); await click(screen.getByRole('button', { name: de.trashHeading })); await click(modelItem('Trashed.stl'));
  noMutationControls();
  expect(screen.queryByRole('button', { name: de.deletePermanentlyLabel })).toBeNull();
});
it('offers browser display preferences and read-only information without desktop tools or import', async () => {
  await start();
  for (const label of [de.railFilament, de.railPrinters, de.queueHeading]) expect(screen.queryByRole('button', { name: label })).toBeNull();
  await click(screen.getByRole('button', { name: de.settingsTitle }));
  for (const label of [de.settingsTabSlicer, de.settingsTabCatalog, de.singleKeyShortcutsTitle]) expect(screen.queryByText(label)).toBeNull();
  await click(screen.getByRole('button', { name: de.themeDark }));
  expect(document.documentElement.dataset.app).toBe('dark');
  await click(screen.getByRole('button', { name: de.settingsTabInfo }));
  for (const name of ['GitHub', 'Discord', 'Open CASCADE', 'UnRAR']) {
    const link = screen.getByRole('link', { name });
    expect(link).toHaveAttribute('target', '_blank');
    await click(link);
  }
});
