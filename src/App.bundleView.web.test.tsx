import { login, authenticationFailed } from './lib/webSession';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
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
      : command === 'list_folders' ? [makeFolder({ id: 'root', name: 'Web folder' })]
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

it('shows the bundle as one card with its name and count, as in the desktop version', async () => {
  await start(false);
  expect(screen.getByRole('row', { name: 'Bündel Harbour, 2 Varianten' })).toBeInTheDocument();
  expect(called('list_bundles')).not.toEqual([]);
});

it('opens the panel read-only for a session that may not edit: members and values, no way to change anything', async () => {
  await start(false);
  await click(card());
  const heading = await screen.findByRole('heading', { level: 2, name: 'Harbour' });
  const panel = within(heading.closest('[data-bundle-panel]') as HTMLElement);
  expect(panel.getByRole('list', { name: de.bundleMembers })).toBeInTheDocument();
  expect(panel.getByText(de.bundleViewOnly)).toBeInTheDocument();
  for (const name of [de.bundleEdit, de.bundleDissolve, de.bundleApplyTag, de.bundleQueueOpen, /als gedruckt markieren/, /als Favoriten markieren/]) {
    expect(panel.queryByRole('button', { name })).toBeNull();
  }
  // No check box either: selecting for a change needs the right to edit.
  expect(card().querySelector('input[type="checkbox"]')).toBeNull();
});

it('lets a writing session change every member from the panel through the web commands', async () => {
  await start(true);
  await click(card());
  const heading = await screen.findByRole('heading', { level: 2, name: 'Harbour' });
  await click(within(heading.closest('[data-bundle-panel]') as HTMLElement).getByRole('button', { name: 'Alle 2 als gedruckt markieren' }));
  await waitFor(() => expect(called('set_print_status')).toEqual([{ fileId: '1', status: 'printed' }, { fileId: '2', status: 'printed' }]));
});

it('asks the bundle delete choice before the web trash button opens its own confirmation', async () => {
  await start(true);
  await click(card().querySelector('input[type="checkbox"]') as HTMLElement);
  await click(screen.getByRole('button', { name: 'In den Papierkorb' }));
  const choice = await screen.findByRole('dialog', { name: 'Bündel „Harbour“ löschen?' });
  // Only the choice is open; the server trash confirmation has not started.
  expect(screen.getAllByRole('dialog')).toHaveLength(1);
  expect(called('delete_files')).toEqual([]);
  await click(within(choice).getByRole('radio', { name: /Alle 2 Modelle in den Papierkorb/ }));
  await click(within(choice).getByRole('button', { name: 'Alle 2 Modelle in den Papierkorb' }));
  // The second choice goes on to the web version's own confirmation.
  await waitFor(() => expect(screen.getByText(/Papierkorb des Servers/)).toBeInTheDocument());
  expect(called('delete_files')).toEqual([]);
});

it('goes straight to the web confirmation for files that are not whole bundle cards', async () => {
  await start(true);
  await click(document.querySelector('[data-model-id="3"] input[type="checkbox"]') as HTMLElement);
  await click(screen.getByRole('button', { name: 'In den Papierkorb' }));
  expect(await screen.findByText(/Papierkorb des Servers/)).toBeInTheDocument();
  expect(screen.queryByRole('dialog', { name: /löschen\?/ })).toBeNull();
});
