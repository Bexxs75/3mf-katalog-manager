import { login, authenticationFailed } from './lib/webSession';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import App from './App';
import * as transport from './lib/transport';
import { WEB_READ_COMMANDS, WEB_WRITE_COMMANDS } from './lib/readOnly';
import { de } from './i18n/de';
import { chooseSelect } from './test/select';
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

let commands: { command: string; args: unknown }[];
const spyInvoke = () => vi.spyOn(transport, 'invoke');
let attempted: ReturnType<typeof spyInvoke>;
const models = [
  makeModelFile({ id: '1', name: 'Alpha.stl', folderId: 'root' }),
  makeModelFile({ id: '2', name: 'Beta.stl', folderId: 'root' }),
  makeModelFile({ id: '3', name: 'Gamma.stl', folderId: 'root' }),
];
const harbour = {
  id: 'b1', name: 'Harbour', kind: 'variants', createdAt: '2026-11-01T00:00:00Z',
  members: [{ fileId: '1', position: 0, label: 'big', inTrash: false }, { fileId: '2', position: 1, label: null, inTrash: false }],
};

beforeEach(() => {
  vi.stubEnv('VITE_TARGET', 'web'); localStorage.clear();
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ width: 800, height: 400, top: 0, left: 0, right: 800, bottom: 400, x: 0, y: 0, toJSON: () => ({}) });
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(400);
  attempted = vi.spyOn(transport, 'invoke'); commands = [];
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (url === '/api/login') return Response.json({ authenticated: true, writable: true, fileOps: true, csrfToken: 'test', mustChangePassword: false });
    const command = url.split('/').slice(-1)[0];
    const args = JSON.parse(String(options?.body ?? '{}'));
    commands.push({ command, args });
    const value = command === 'list_file_summaries' ? models.map(makeModelFileSummary)
      : command === 'list_files_by_ids' ? (args.ids.length === 1 ? [] : models.filter(m => args.ids.includes(m.id)))
      : command === 'list_bundles' ? [harbour]
      : command === 'list_folders' ? [makeFolder({ id: 'root', name: 'Web folder' }), makeFolder({ id: 'dest', name: 'Target folder' })]
      : command === 'empty_trash_summary' ? { count: 0, bytes: 0 }
      : command === 'get_app_version' ? '0.1.0' : ['has_step_preview', 'is_preview_build'].includes(command) ? false : command === 'list_all_file_tags' ? {} : [];
    return Response.json(value);
  }));
});
afterEach(() => {
  cleanup(); authenticationFailed(401);
  // The transport guard must not mask a UI regression: every command the bundle views use is known to the web server.
  const known = [...WEB_READ_COMMANDS, ...WEB_WRITE_COMMANDS] as readonly string[];
  expect(attempted.mock.calls.map(([command]) => command).filter(command => !known.includes(command))).toEqual([]);
  expect(commands.map(entry => entry.command).filter(command => !known.includes(command))).toEqual([]);
  vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.unstubAllEnvs();
});

async function start(signedIn: boolean) {
  if (signedIn) await login('password');
  localStorage.setItem('3mf-katalog-density', 'compact');
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  await act(async () => { render(<RuntimeEnvironmentProvider><LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics></RuntimeEnvironmentProvider>); });
  await screen.findAllByText('Gamma.stl');
}
const click = async (element: HTMLElement) => { await act(async () => { fireEvent.click(element); }); };
const card = () => document.querySelector('[data-bundle-id="b1"]') as HTMLElement;
const called = (name: string) => commands.filter(entry => entry.command === name).map(entry => entry.args);


async function moveInto(dialog: HTMLElement) {
  chooseSelect(within(dialog).getByRole('combobox'), 'dest');
  await click(within(dialog).getByRole('button', { name: de.moveDialogConfirm }));
}

it('moves the whole bundle from its card menu entry "Move to"', async () => {
  await start(true);
  await act(async () => { fireEvent.contextMenu(card()); });
  await click(screen.getByRole('button', { name: de.moveContextLabel }));
  await moveInto(await screen.findByRole('dialog'));
  await waitFor(() => expect(called('move_files_to_folder')).toEqual([{ folderId: 'dest', fileIds: ['1', '2'] }]));
});

it('moves the whole bundle on F6 when only its card is open, as the menu does', async () => {
  await start(true);
  await click(card());
  await screen.findByRole('heading', { level: 2, name: 'Harbour' });
  await act(async () => { fireEvent.keyDown(document.body, { key: 'F6' }); });
  await moveInto(await screen.findByRole('dialog'));
  await waitFor(() => expect(called('move_files_to_folder')).toEqual([{ folderId: 'dest', fileIds: ['1', '2'] }]));
});

it('still moves only the model itself on F6 for a single model', async () => {
  await start(true);
  await click(screen.getByText('Gamma.stl'));
  await act(async () => { fireEvent.keyDown(document.body, { key: 'F6' }); });
  await moveInto(await screen.findByRole('dialog'));
  await waitFor(() => expect(called('move_files_to_folder')).toEqual([{ folderId: 'dest', fileIds: ['3'] }]));
});
