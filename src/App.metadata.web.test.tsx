import { login, authenticationFailed } from './lib/webSession';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import App from './App';
import * as transport from './lib/transport';
import { WEB_READ_COMMANDS, WEB_WRITE_COMMANDS } from './lib/readOnly';
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
    if (url === '/api/login') return Response.json({ authenticated: true, writable: true, csrfToken: 'test', mustChangePassword: false });
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
  cleanup(); authenticationFailed(401);
  // Assert attempted commands too: the transport guard must not mask a UI regression.
  expect(attempted.mock.calls.map(([command]) => command).filter(command => !([...WEB_READ_COMMANDS, ...WEB_WRITE_COMMANDS] as readonly string[]).includes(command))).toEqual([]);
  expect(commands.filter(command => !([...WEB_READ_COMMANDS, ...WEB_WRITE_COMMANDS] as readonly string[]).includes(command))).toEqual([]);
  vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.unstubAllEnvs();
});
async function start(density = 'compact') {
  await login('password');
  localStorage.setItem('3mf-katalog-density', density);
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  await act(async () => { render(<RuntimeEnvironmentProvider><LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics></RuntimeEnvironmentProvider>); });
}
const click = async (element: HTMLElement) => { await act(async () => { fireEvent.click(element); }); };
const modelItem = (name = 'Alpha.stl') => within(screen.getByRole('main')).getByText(name);
async function openDetail() { await act(async () => { fireEvent.click(modelItem()); fireEvent.doubleClick(modelItem()); }); }
it.each(['compact', 'comfort'])('exposes metadata but keeps filesystem controls absent in writable %s web', async density => {
  await start(density); await click(modelItem());
  expect(screen.getByText(de.createCollectionLabel)).toBeVisible();
  expect(screen.queryByRole('button', { name: de.createFolderLabel })).toBeNull();
  expect(screen.getByRole('button', { name: new RegExp('^' + de.queueHeading) })).toBeVisible();
  for (const label of [de.delete, de.showInFileManager, de.openInSlicer, de.restoreLabel, de.emptyTrashButton]) {
    expect(screen.queryByRole('button', { name: label })).toBeNull();
  }
  expect(screen.getAllByRole('button', { name: de.favoriteRemove }).every(button => !(button as HTMLButtonElement).disabled)).toBe(true);
  // Cover image and rescan are catalog writes the server accepts, so a writable session can use them.
  expect(screen.getByRole('button', { name: de.uploadModelImageLabel })).toBeEnabled();
  const folder = screen.getByText('Other folder').closest<HTMLElement>('[role=button]')!;
  fireEvent.contextMenu(folder); fireEvent.keyDown(folder, { key: 'F10', shiftKey: true });
  expect(document.querySelector('[data-navigation-menu]')).toBeNull();
  fireEvent.keyDown(window, { key: 'a', ctrlKey: true });
  expect(screen.queryByRole('button', { name: de.delete })).toBeNull();
  for (const key of ['Delete', 'Backspace']) fireEvent.keyDown(window, { key });
  fireEvent.keyDown(window, { key: 'Escape' });
  await openDetail();
  expect(screen.getByRole('button', { name: de.printLogAddButton })).toBeVisible();
  await click(screen.getByRole('button', { name: de.printLogAddButton }));
  expect(screen.getByLabelText(de.printLogPhotoButton)).toHaveAttribute('type', 'file');
  expect(screen.getByRole('button', { name: de.rescanMetadataButton })).toBeEnabled();
  expect(screen.getByRole('button', { name: de.uploadModelImageLabel })).toBeEnabled();
  for (const label of [de.delete, de.showInFileManager, de.openInSlicer]) {
    expect(screen.queryByRole('button', { name: label })).toBeNull();
  }
  const writes = attempted.mock.calls.map(([command]) => command).filter(command => !(WEB_READ_COMMANDS as readonly string[]).includes(command));
  expect(writes.every(command => command === 'mark_file_viewed')).toBe(true);
});
