import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { initializeUiSettings, uiSettings, flushUiSettings, getUiSettingsError, reportUiSettingsRestoreError, resetUiSettings, reloadUiSettings, clearUiSettingsError } from './uiSettings';
import { useTheme } from '../hooks/useTheme';
import { useSingleKeyShortcuts } from '../hooks/useSingleKeyShortcuts';
import { useCollapsedFolders } from '../hooks/useCollapsedFolders';

const backend = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('./transport', () => backend);
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ onCloseRequested: async () => () => {} }) }));
afterEach(() => { vi.unstubAllGlobals(); vi.unstubAllEnvs(); });
const snapshot = (values: Record<string, string>, migrated = true) => ({ schema_version: 1, migrated_from_webview: migrated, values });
beforeEach(() => { vi.stubGlobal('__TAURI_INTERNALS__', {}); localStorage.clear(); sessionStorage.clear(); backend.invoke.mockReset(); });

it('loads once and migrates all namespaced legacy values in one call without deleting them', async () => {
  localStorage.setItem('3mf-katalog-theme', 'dark');
  localStorage.setItem('3mf-katalog-preview-attempt:42', '1');
  localStorage.setItem('unrelated', 'leave');
  localStorage.setItem('3mf-katalog-slicers', '/opt/old-slicer');
  backend.invoke.mockResolvedValueOnce(snapshot({}, false)).mockResolvedValueOnce(snapshot({ '3mf-katalog-theme': 'dark' }));
  await Promise.all([initializeUiSettings(), initializeUiSettings()]);
  expect(backend.invoke.mock.calls).toEqual([
    ['get_ui_settings', undefined],
    ['migrate_ui_settings', { values: { '3mf-katalog-theme': 'dark' } }],
  ]);
  expect(localStorage.getItem('3mf-katalog-theme')).toBe('dark');
  expect(localStorage.getItem('unrelated')).toBe('leave');
});

it('initializes hooks from the backend instead of stale browser defaults', async () => {
  localStorage.setItem('3mf-katalog-theme', 'dark');
  backend.invoke.mockResolvedValue(snapshot({ '3mf-katalog-theme': 'white', '3mf-katalog-single-key-shortcuts': 'false', '3mf-katalog-collapsed-folders': '["a"]' }));
  await initializeUiSettings();
  expect(renderHook(useTheme).result.current.setting).toBe('white');
  expect(renderHook(useSingleKeyShortcuts).result.current[0]).toBe(false);
  expect(renderHook(useCollapsedFolders).result.current.isCollapsed('a')).toBe(true);
  expect(backend.invoke).not.toHaveBeenCalledWith('migrate_ui_settings', expect.anything());
  await flushUiSettings();
});

it('falls back on startup failure and retains memory changes after failed writes', async () => {
  localStorage.setItem('3mf-katalog-theme', 'dark');
  backend.invoke.mockRejectedValue({ message: 'uiSettingsUnavailable', expected: true });
  await initializeUiSettings();
  const { result } = renderHook(useTheme);
  expect(result.current.setting).toBe('dark');
  act(() => result.current.setTheme('slate'));
  await flushUiSettings();
  expect(result.current.setting).toBe('slate');
  expect(uiSettings.getItem('3mf-katalog-theme')).toBe('slate');
  expect(localStorage.getItem('3mf-katalog-theme')).toBe('slate');
  expect(getUiSettingsError()).toBe('uiSettingsUnavailable');
});

it('serializes writes, continues after rejection, and sends removals explicitly', async () => {
  backend.invoke.mockResolvedValueOnce(snapshot({})).mockRejectedValueOnce('full').mockResolvedValue(undefined);
  await initializeUiSettings();
  uiSettings.setItem('3mf-katalog-theme', 'dark');
  await flushUiSettings();
  uiSettings.setItem('3mf-katalog-theme', 'white');
  await flushUiSettings();
  uiSettings.removeItem('3mf-katalog-theme');
  await flushUiSettings();
  expect(backend.invoke.mock.calls.slice(1)).toEqual([
    ['set_ui_setting', { key: '3mf-katalog-theme', value: 'dark' }],
    ['set_ui_setting', { key: '3mf-katalog-theme', value: 'white' }],
    ['set_ui_setting', { key: '3mf-katalog-theme', value: null }],
  ]);
  expect(uiSettings.getItem('3mf-katalog-theme')).toBeNull();
});

it.each(['uiSettingsInvalid', 'uiSettingsUnavailable'])('carries the %s restore warning across the required catalog reload once', async code => {
  reportUiSettingsRestoreError({ message: code });
  resetUiSettings();
  backend.invoke.mockResolvedValue(snapshot({}));
  await initializeUiSettings();
  expect(getUiSettingsError()).toBe(code);
  resetUiSettings();
  await initializeUiSettings();
  expect(getUiSettingsError()).toBeNull();
});

it('keeps the synchronous boot cache current on load, change, removal and reload', async () => {
  backend.invoke.mockResolvedValue(snapshot({ '3mf-katalog-theme': 'white', '3mf-katalog-language': 'fr', '3mf-katalog-density': 'comfort' }));
  await initializeUiSettings();
  expect(localStorage.getItem('3mf-katalog-language')).toBe('fr');
  expect(localStorage.getItem('3mf-katalog-density')).toBe('comfort');
  uiSettings.setItem('3mf-katalog-theme', 'dark');
  const { default: boot } = await import('../../public/theme-init.js?raw');
  new Function(boot)();
  expect(document.documentElement.dataset.app).toBe('dark');
  await flushUiSettings();
  await reloadUiSettings();
  expect(localStorage.getItem('3mf-katalog-theme')).toBe('white');
  uiSettings.removeItem('3mf-katalog-language');
  expect(localStorage.getItem('3mf-katalog-language')).toBeNull();
  await flushUiSettings();
});

it.each([false, true])('uses only live browser storage without a Tauri runtime (web build %s)', async webBuild => {
  vi.unstubAllGlobals();
  if (webBuild) vi.stubEnv('VITE_TARGET', 'web');
  backend.invoke.mockRejectedValue(new Error('must not invoke'));
  localStorage.setItem('3mf-katalog-language', 'fr');
  sessionStorage.setItem('3mf-ui-settings-restore-error', 'uiSettingsInvalid');
  expect(uiSettings.getItem('3mf-katalog-language')).toBe('fr');
  uiSettings.setItem('3mf-katalog-theme', 'dark');
  await initializeUiSettings();
  uiSettings.setItem('3mf-katalog-sidebar-width', '300');
  uiSettings.setItem('3mf-katalog-last-seen-version', '0.18.4');
  await flushUiSettings();
  await reloadUiSettings();
  reportUiSettingsRestoreError({ message: 'uiSettingsInvalid' });
  expect(localStorage.getItem('3mf-katalog-theme')).toBe('dark');
  expect(localStorage.getItem('3mf-katalog-sidebar-width')).toBe('300');
  expect(localStorage.getItem('3mf-katalog-last-seen-version')).toBe('0.18.4');
  localStorage.setItem('3mf-katalog-language', 'es');
  expect(uiSettings.getItem('3mf-katalog-language')).toBe('es');
  uiSettings.removeItem('3mf-katalog-sidebar-width');
  expect(localStorage.getItem('3mf-katalog-sidebar-width')).toBeNull();
  expect(getUiSettingsError()).toBeNull();
  expect(backend.invoke).not.toHaveBeenCalled();
});

it('preserves legacy preferences for future schemas and never writes their backend file', async () => {
  localStorage.setItem('3mf-katalog-theme', 'dark');
  localStorage.setItem('3mf-katalog-language', 'fr');
  localStorage.setItem('3mf-katalog-sidebar-width', '300');
  backend.invoke.mockResolvedValue({ ...snapshot({}, false), schema_version: 2 });
  await initializeUiSettings();
  expect(uiSettings.getItem('3mf-katalog-theme')).toBe('dark');
  expect(uiSettings.getItem('3mf-katalog-language')).toBe('fr');
  expect(uiSettings.getItem('3mf-katalog-sidebar-width')).toBe('300');
  expect(getUiSettingsError()).toBe('uiSettingsInvalid');
  clearUiSettingsError();
  uiSettings.setItem('3mf-katalog-sidebar-width', '310');
  await flushUiSettings();
  expect(localStorage.getItem('3mf-katalog-sidebar-width')).toBe('310');
  expect(backend.invoke).toHaveBeenCalledTimes(1);
  expect(getUiSettingsError()).toBeNull();
});

it('debounces rapid updates, skips unchanged values, and flushes on page departure', async () => {
  vi.useFakeTimers();
  try {
    backend.invoke.mockResolvedValueOnce(snapshot({ '3mf-katalog-sidebar-width': '200' })).mockResolvedValue(undefined);
    await initializeUiSettings();
    uiSettings.setItem('3mf-katalog-sidebar-width', '200');
    await flushUiSettings();
    expect(backend.invoke).toHaveBeenCalledTimes(1);
    for (let width = 201; width < 300; width++) uiSettings.setItem('3mf-katalog-sidebar-width', String(width));
    await vi.advanceTimersByTimeAsync(299);
    expect(backend.invoke).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(backend.invoke).toHaveBeenLastCalledWith('set_ui_setting', { key: '3mf-katalog-sidebar-width', value: '299' });
    expect(backend.invoke).toHaveBeenCalledTimes(2);
    uiSettings.setItem('3mf-katalog-sidebar-width', '310');
    window.dispatchEvent(new Event('pagehide'));
    await flushUiSettings();
    expect(backend.invoke).toHaveBeenLastCalledWith('set_ui_setting', { key: '3mf-katalog-sidebar-width', value: '310' });
  } finally { vi.useRealTimers(); }
});

it('clears absent authoritative boot keys without mirroring other preferences', async () => {
  localStorage.setItem('3mf-katalog-theme', 'dark');
  localStorage.setItem('3mf-katalog-sidebar-width', '200');
  backend.invoke.mockResolvedValue(snapshot({}));
  await initializeUiSettings();
  expect(localStorage.getItem('3mf-katalog-theme')).toBeNull();
  uiSettings.setItem('3mf-katalog-sidebar-width', '300');
  await flushUiSettings();
  expect(localStorage.getItem('3mf-katalog-sidebar-width')).toBe('200');
});

it('bounds the single migration request and preserves the original browser values', async () => {
  for (let i = 0; i < 24; i++) localStorage.setItem(`3mf-katalog-future-${i}`, 'x'.repeat(65536));
  backend.invoke.mockResolvedValueOnce(snapshot({}, false)).mockResolvedValueOnce(snapshot({}));
  await initializeUiSettings();
  const payload = backend.invoke.mock.calls[1][1];
  expect(new TextEncoder().encode(JSON.stringify(payload)).length).toBeLessThanOrEqual(1024 * 1024);
  expect(Object.keys(payload.values).length).toBeGreaterThan(0);
  expect(localStorage.getItem('3mf-katalog-future-23')).toHaveLength(65536);
  expect(backend.invoke).toHaveBeenCalledTimes(2);
});

it('sends final user intent even when it matches a stale startup snapshot', async () => {
  backend.invoke.mockResolvedValueOnce(snapshot({ '3mf-katalog-theme': 'light' })).mockResolvedValue(undefined);
  await initializeUiSettings();
  // Another backend writer may have changed preferences after our snapshot.
  uiSettings.setItem('3mf-katalog-theme', 'dark');
  uiSettings.setItem('3mf-katalog-theme', 'light');
  await flushUiSettings();
  expect(backend.invoke).toHaveBeenLastCalledWith('set_ui_setting', { key: '3mf-katalog-theme', value: 'light' });
});

it('migrates folder lists above 64 KiB within the dedicated limit', async () => {
  const folders = JSON.stringify(Array(2000).fill('a'.repeat(36)));
  localStorage.setItem('3mf-katalog-sidebar-expanded', folders);
  backend.invoke.mockResolvedValueOnce(snapshot({}, false)).mockResolvedValueOnce(snapshot({}));
  await initializeUiSettings();
  expect(backend.invoke).toHaveBeenLastCalledWith('migrate_ui_settings', { values: { '3mf-katalog-sidebar-expanded': folders } });
});

it.each([false, true])('never vetoes or destroys the window when close flush hangs: %s', async hangs => {
  let closeRequested!: (event: { preventDefault(): void }) => Promise<void>;
  const destroy = vi.fn();
  const onCloseRequested = vi.fn(async callback => { closeRequested = callback; return vi.fn(); });
  vi.doMock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ destroy, onCloseRequested }) }));
  try {
    backend.invoke.mockImplementation(async command => {
      if (command === 'get_ui_settings') return snapshot({});
      if (command === 'set_ui_setting') return hangs ? new Promise(() => {}) : undefined;
      throw new Error(`Unexpected command: ${command}`);
    });
    await initializeUiSettings();
    await vi.waitFor(() => expect(onCloseRequested).toHaveBeenCalled());
    vi.useFakeTimers();
    uiSettings.setItem('3mf-katalog-theme', 'dark');
    const event = { preventDefault: vi.fn() };
    const closing = closeRequested(event);
    expect(event.preventDefault).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1000);
    await closing;
    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(destroy).not.toHaveBeenCalled();
    expect(backend.invoke.mock.calls.map(([command]) => command)).toEqual(['get_ui_settings', 'set_ui_setting']);
  } finally { vi.useRealTimers(); vi.doUnmock('@tauri-apps/api/window'); }
});

it.each(['get_ui_settings', 'migrate_ui_settings'])('falls back after a stalled %s and ignores its late snapshot', async stalled => {
  vi.useFakeTimers();
  try {
    localStorage.setItem('3mf-katalog-theme', 'dark');
    let finish!: (value: unknown) => void;
    backend.invoke.mockImplementation(command => command === stalled
      ? new Promise(resolve => { finish = resolve; })
      : Promise.resolve(snapshot({}, false)));
    const initializing = initializeUiSettings();
    await vi.waitFor(() => expect(finish).toBeTypeOf('function'));
    await vi.advanceTimersByTimeAsync(3000);
    await initializing;
    expect(uiSettings.getItem('3mf-katalog-theme')).toBe('dark');
    expect(getUiSettingsError()).toBe('uiSettingsUnavailable');
    uiSettings.setItem('3mf-katalog-theme', 'white');
    finish(snapshot({ '3mf-katalog-theme': 'slate' }));
    await Promise.resolve();
    expect(uiSettings.getItem('3mf-katalog-theme')).toBe('white');
    expect(localStorage.getItem('3mf-katalog-theme')).toBe('white');
    expect(backend.invoke.mock.calls.filter(([command]) => command === 'migrate_ui_settings')).toHaveLength(stalled === 'migrate_ui_settings' ? 1 : 0);
  } finally { vi.useRealTimers(); }
});

it('clears stale boot caches after restoring absent backend values', async () => {
  backend.invoke.mockResolvedValueOnce(snapshot({ '3mf-katalog-theme': 'dark', '3mf-katalog-language': 'fr', '3mf-katalog-density': 'compact' })).mockResolvedValue(snapshot({}));
  await initializeUiSettings();
  await reloadUiSettings();
  for (const suffix of ['theme', 'language', 'density']) expect(localStorage.getItem(`3mf-katalog-${suffix}`)).toBeNull();
});

it('shares the startup deadline between reading and migration', async () => {
  vi.useFakeTimers();
  try {
    let read!: (value: unknown) => void;
    backend.invoke.mockImplementation(command => command === 'get_ui_settings'
      ? new Promise(resolve => { read = resolve; }) : new Promise(() => {}));
    const starting = initializeUiSettings();
    await vi.waitFor(() => expect(read).toBeTypeOf('function'));
    await vi.advanceTimersByTimeAsync(2000);
    read(snapshot({}, false));
    await vi.waitFor(() => expect(backend.invoke).toHaveBeenCalledWith('migrate_ui_settings', expect.anything()));
    await vi.advanceTimersByTimeAsync(1000);
    await starting;
    expect(getUiSettingsError()).toBe('uiSettingsUnavailable');
  } finally { vi.useRealTimers(); }
});
