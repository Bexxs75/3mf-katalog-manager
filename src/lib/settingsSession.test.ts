import { beforeEach, expect, it, vi } from 'vitest';
import { captureSettingsBaseline, captureUiSettingsBaseline, matchesSettingsSearch, resetSettingsBaseline, settingBaseline, settingChanged } from './settingsSession';
import { uiSettings } from './uiSettings';
const backend = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('./transport', () => backend);
beforeEach(() => { resetSettingsBaseline(); localStorage.clear(); backend.invoke.mockReset(); });
it('matches umlauts, sharp s, case, help and section names', () => {
  expect(matchesSettingsSearch('GROSSE', 'Größe')).toBe(true);
  expect(matchesSettingsSearch('anderung', 'Änderung')).toBe(true);
  expect(matchesSettingsSearch('ubersicht', 'Übersicht')).toBe(true);
  expect(matchesSettingsSearch('fehler suche', 'Hilfe', 'Fehler bei der Suche anzeigen')).toBe(true);
  expect(matchesSettingsSearch('darstellung', 'Farbschema', 'Darstellung')).toBe(true);
  expect(matchesSettingsSearch('unauffindbar', 'Sprache')).toBe(false);
  expect(matchesSettingsSearch('   ', 'Sprache')).toBe(true);
});
it('compares with the session baseline, not the default, and does not invent failed reads', () => {
  expect(settingChanged('dark', 'dark')).toBe(false);
  expect(settingChanged('system', 'dark')).toBe(true);
  expect(settingChanged(false, true)).toBe(true);
  expect(settingChanged(false, undefined)).toBe(false);
});
it('freezes authoritative startup preferences before header changes and recaptures only for a new session', () => {
  uiSettings.setItem('3mf-katalog-theme', 'dark');
  uiSettings.setItem('3mf-katalog-single-key-shortcuts', 'false');
  captureUiSettingsBaseline();
  uiSettings.setItem('3mf-katalog-theme', 'white');
  captureUiSettingsBaseline();
  expect(settingBaseline('theme')).toBe('dark');
  expect(settingBaseline('shortcuts')).toBe(false);
  resetSettingsBaseline(); captureUiSettingsBaseline();
  expect(settingBaseline('theme')).toBe('white');
});
it('shares startup reads under StrictMode and retains unknown values on failure', async () => {
  backend.invoke.mockImplementation(async (command: string) => {
    if (command === 'get_auto_tagging') return false;
    throw new Error('unavailable');
  });
  const first = captureSettingsBaseline();
  expect(captureSettingsBaseline()).toBe(first);
  await first;
  expect(settingBaseline('autoTags')).toBe(false);
  expect(settingBaseline('importRename')).toBeUndefined();
});
it('treats a missing stored primary slicer as unknown, not as a base to differ from', () => {
  resetSettingsBaseline();
  localStorage.removeItem('3mf-katalog-primary-slicer-id');
  captureUiSettingsBaseline();
  expect(settingBaseline('primarySlicer')).toBeUndefined();
  expect(settingChanged('auto-picked', settingBaseline('primarySlicer'))).toBe(false);
});
it('compares the close action with the value read at startup, not with the default', async () => {
  backend.invoke.mockImplementation(async (command: string) => {
    if (command === 'get_close_behavior') return { action: 'tray', platform: 'linux', supported: true, trayAvailable: true };
    throw new Error('unavailable');
  });
  await captureSettingsBaseline();
  // Saved "tray" before the restart: the same value now is not a change, although it differs from the default.
  expect(settingChanged('tray', settingBaseline('close'))).toBe(false);
  // Going back to the default is a change compared with the start of this session.
  expect(settingChanged('exit', settingBaseline('close'))).toBe(true);
});
