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
    if (url === '/api/login') return Response.json({ authenticated: true, writable: true, fileOps:true, csrfToken: 'test', mustChangePassword: false });
    const command = url.split('/').slice(-1)[0]; commands.push(command);
    if (command === 'get_model_geometry') {
      const header = new TextEncoder().encode('{"meshes":[],"palette":[]}');
      const buffer = new ArrayBuffer(4 + header.length);
      new DataView(buffer).setUint32(0, header.length, true); new Uint8Array(buffer, 4).set(header);
      return new Response(buffer, { headers: { 'Content-Type': 'application/octet-stream' } });
    }
    const args = JSON.parse(String(options?.body ?? '{}'));
    const value = command === 'rename_file' ? null : command === 'list_file_summaries' ? models.map(makeModelFileSummary)
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
it('refreshes catalog, folders, tags, trash, collections and printers after a confirmed file action', async () => {
  await start(); await click(modelItem());
  await click(screen.getAllByRole('button', {name: de.renameLabel})[0]);
  const dialog = screen.getByRole('dialog');
  fireEvent.change(within(dialog).getByRole('textbox'), {target:{value:'Renamed'}});
  commands=[];
  await click(within(dialog).getByRole('button',{name:de.renameLabel}));
  expect(commands).toContain('rename_file');
  expect(attempted).toHaveBeenCalledWith('rename_file',{fileId:'1',name:'Renamed'});
  for (const command of ['list_file_summaries','list_all_file_tags','list_folders','list_tag_counts','list_trash','list_collections','list_printers']) expect(commands).toContain(command);
});
