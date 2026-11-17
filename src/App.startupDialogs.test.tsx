import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { PrintTechnologyProvider } from './hooks/PrintTechnologyContext';
import { de } from './i18n/de';
import { makeFolder } from './test/factories';
import { compareVersions, LAST_SEEN_VERSION_KEY as key } from './lib/releaseNotesSelection';
import { currentVersion, releaseNotes } from './lib/releaseNotes';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));
let asked: boolean;
let menu: boolean;
let upgrade: unknown;
let pending: unknown[];
let printReply: Promise<unknown> | undefined;
let fail: string;
beforeEach(() => {
  vi.unstubAllEnvs(); localStorage.clear();
  localStorage.setItem('3mf-katalog-language', 'de');
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  asked = true; menu = true; upgrade = null; pending = []; printReply = undefined; fail = '';
  vi.mocked(invoke).mockReset().mockImplementation(async cmd => {
    if (cmd === fail) throw new Error('unavailable');
    if (cmd === 'get_app_version') return currentVersion;
    if (cmd === 'get_print_technology') return printReply ?? { value: 'both', asked };
    if (cmd === 'set_print_technology') return { value: 'both', asked: true };
    if (cmd === 'get_upgrade_notice') return upgrade;
    if (cmd === 'get_linux_menu_status') return { available: true, shouldPrompt: menu };
    if (cmd === 'take_open_requests') { const result = pending; pending = []; return result; }
    if (cmd === 'list_folders') return [makeFolder({ id: 'root', path: '/old' })];
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    return [];
  });
});
const mount = () => act(async () => render(<LanguageProviderWithDiagnostics><UiDensityProvider><PrintTechnologyProvider><App /></PrintTechnologyProvider></UiDensityProvider></LanguageProviderWithDiagnostics>));
const onlyDialog = async (name: string) => {
  const dialog = await screen.findByRole('dialog', { name });
  expect(screen.getAllByRole('dialog')).toHaveLength(1);
  return dialog;
};
const calls = (cmd: string) => vi.mocked(invoke).mock.calls.filter(([c]) => c === cmd);
const external = { kind: 'external', sourceId: 'one', name: 'one.stl', sizeBytes: 50, extension: 'stl' };

it('serializes fresh Linux setup, print technology, Linux menu, then pending open-with', async () => {
  asked = false; pending = [external];
  await mount();
  fireEvent.click(document.querySelector<HTMLButtonElement>('header button[title="Neuigkeiten"]')!);
  expect(screen.getAllByRole('dialog')).toHaveLength(1);
  expect(calls('get_upgrade_notice')).toHaveLength(0);
  expect(calls('get_linux_menu_status')).toHaveLength(0);
  fireEvent.click(screen.getByRole('button', { name: de.catalogSetupLater }));
  await onlyDialog(de.ptTitle);
  fireEvent.click(document.querySelector<HTMLButtonElement>('header button[title="Neuigkeiten"]')!);
  await onlyDialog(de.ptTitle);
  expect(localStorage.getItem(key)).toBe(currentVersion);
  expect(calls('get_linux_menu_status')).toHaveLength(0);
  fireEvent.click(screen.getByRole('button', { name: de.ptApply }));
  await onlyDialog(de.linuxMenuPromptTitle);
  fireEvent.click(document.querySelector<HTMLButtonElement>('header button[title="Neuigkeiten"]')!);
  await onlyDialog(de.linuxMenuPromptTitle);
  expect(screen.queryByRole('dialog', { name: de.owViewTitle })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: de.linuxMenuNotNow }));
  await onlyDialog(de.owViewTitle);
  fireEvent.click(document.querySelector<HTMLButtonElement>('header button[title="Neuigkeiten"]')!);
  await onlyDialog(de.owViewTitle);
});
it('waits for print technology loading instead of releasing Linux early', async () => {
  localStorage.setItem('3mf-katalog-setup-seen', '1'); localStorage.setItem(key, currentVersion);
  let resolve!: (value: unknown) => void;
  printReply = new Promise(r => { resolve = r; });
  await mount();
  expect(calls('get_linux_menu_status')).toHaveLength(0);
  await act(async () => resolve({ value: 'both', asked: true }));
  await onlyDialog(de.linuxMenuPromptTitle);
});
it('waits for the upgrade notice, then news, then Linux; settings reopens current notes', async () => {
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  upgrade = { fromVersion: '0.18.0', toVersion: currentVersion, fromSchema: 1, toSchema: 2, backup: null, backupFailed: true, folder: '/old', catalogFile: '/old/catalog.db', container: false };
  await mount();
  await onlyDialog(de.rwTitleFailed);
  expect(localStorage.getItem(key)).toBeNull();
  expect(calls('get_linux_menu_status')).toHaveLength(0);
  fireEvent.click(screen.getByRole('button', { name: de.rwUnderstood }));
  await onlyDialog(de.whatsNewTitle.replace('{version}', currentVersion));
  expect(calls('get_linux_menu_status')).toHaveLength(0);
  fireEvent.click(screen.getByRole('button', { name: de.whatsNewClose }));
  await onlyDialog(de.linuxMenuPromptTitle);
  fireEvent.click(screen.getByRole('button', { name: de.linuxMenuNotNow }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  fireEvent.click(screen.getByRole('button', { name: de.settingsTitle }));
  fireEvent.click(screen.getByRole('tab', { name: de.settingsTabInfo }));
  const settings = screen.getByRole('complementary', { name: de.settingsTitle });
  fireEvent.click(within(settings).getByRole('button', { name: de.whatsNew }));
  await onlyDialog(de.whatsNewTitle.replace('{version}', currentVersion));
});
it.each(['get_upgrade_notice', 'get_print_technology', 'get_linux_menu_status', 'answer_linux_menu_prompt', 'set_print_technology'])('releases failed startup step %s', async command => {
  fail = command; pending = [external];
  if (command !== 'set_print_technology') localStorage.setItem('3mf-katalog-setup-seen', '1');
  else asked = false;
  localStorage.setItem(key, currentVersion);
  await mount();
  if (command === 'set_print_technology') {
    fireEvent.click(screen.getByRole('button', { name: de.catalogSetupLater }));
    await onlyDialog(de.ptTitle);
    fireEvent.click(screen.getByRole('button', { name: de.ptApply }));
    expect(await screen.findByRole('alert')).toHaveTextContent(de.ptSaveFailed);
    await onlyDialog(de.ptTitle);
    expect(calls('get_linux_menu_status')).toHaveLength(0);
    expect(calls('set_print_technology')).toHaveLength(1);
    fireEvent.click(screen.getByRole('button', { name: de.impClose }));
    expect(calls('set_print_technology')).toHaveLength(1);
  }
  if (command !== 'get_linux_menu_status') {
    await onlyDialog(de.linuxMenuPromptTitle);
    fireEvent.click(screen.getByRole('button', { name: de.linuxMenuNotNow }));
    if (command === 'answer_linux_menu_prompt') {
      expect(await screen.findByRole('alert')).toHaveTextContent(de.linuxMenuFailed);
      await onlyDialog(de.linuxMenuPromptTitle);
      expect(screen.queryByRole('dialog', { name: de.owViewTitle })).toBeNull();
      fireEvent.click(screen.getByRole('button', { name: de.impClose }));
      expect(calls('answer_linux_menu_prompt')).toHaveLength(1);
    }
  }
  await onlyDialog(de.owViewTitle);
});
it('web with an older marker shows news without waiting for the unmounted upgrade notice', async () => {
  vi.stubEnv('VITE_TARGET', 'web');
  // Marker = the release right before this one, so exactly one version (and its titled dialog) is shown.
  const previous = Object.keys(releaseNotes).filter(v => v !== currentVersion).sort((a, b) => compareVersions(b, a))[0]!;
  localStorage.setItem(key, previous);
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    const cmd = url.split('/').slice(-1)[0];
    const value = cmd === 'get_app_version' ? currentVersion
      : cmd === 'get_print_technology' ? { value: 'both', asked: true }
      : cmd === 'has_step_preview' || cmd === 'is_preview_build' ? false : [];
    return Response.json(value);
  }));
  try {
    await mount();
    await onlyDialog(de.whatsNewTitle.replace('{version}', currentVersion));
    expect(calls('get_upgrade_notice')).toHaveLength(0);
    expect(localStorage.getItem(key)).toBe(previous);
    fireEvent.click(screen.getByRole('button', { name: de.whatsNewClose }));
    expect(localStorage.getItem(key)).toBe(currentVersion);
  } finally { vi.unstubAllGlobals(); }
});
it('skips unneeded prompts and releases a pending open-with request', async () => {
  localStorage.setItem('3mf-katalog-setup-seen', '1'); localStorage.setItem(key, currentVersion);
  menu = false; pending = [external];
  await mount();
  await onlyDialog(de.owViewTitle);
});
