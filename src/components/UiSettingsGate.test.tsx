import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { UiSettingsGate } from './UiSettingsGate';
import { LanguageProvider, useLanguage } from '../i18n/LanguageContext';
import { useTheme } from '../hooks/useTheme';

const backend = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('../lib/transport', () => backend);
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ onCloseRequested: async () => () => {} }) }));
beforeEach(() => { vi.stubGlobal('__TAURI_INTERNALS__', {}); backend.invoke.mockReset(); });
afterEach(() => vi.unstubAllGlobals());

it('does not mount the catalog until theme and language are loaded', async () => {
  localStorage.setItem('3mf-katalog-theme', 'dark');
  localStorage.setItem('3mf-katalog-language', 'de');
  let finish!: (value: unknown) => void;
  backend.invoke.mockReturnValue(new Promise(resolve => { finish = resolve; }));
  const renders: string[] = [];
  function Catalog() {
    const { setting } = useTheme();
    const { language } = useLanguage();
    renders.push(`${setting}:${language}`);
    return <div data-testid="catalog">{setting}:{language}</div>;
  }
  render(<LanguageProvider><UiSettingsGate><Catalog /></UiSettingsGate></LanguageProvider>);
  expect(screen.queryByTestId('catalog')).toBeNull();
  await act(async () => {
    finish({ schema_version: 1, migrated_from_webview: true, values: { '3mf-katalog-theme': 'white', '3mf-katalog-language': 'en' } });
  });
  expect(screen.getByTestId('catalog')).toHaveTextContent('white:en');
  expect(renders.every(value => value === 'white:en')).toBe(true);
});

it('announces invalid settings distinctly and offers a close action without stealing focus', async () => {
  const { UiSettingsError } = await import('./UiSettingsGate');
  const { initializeUiSettings } = await import('../lib/uiSettings');
  backend.invoke.mockRejectedValue({ message: 'uiSettingsInvalid' });
  localStorage.setItem('3mf-katalog-language', 'en');
  render(<LanguageProvider><button>Anchor</button><UiSettingsError /></LanguageProvider>);
  screen.getByText('Anchor').focus();
  await act(initializeUiSettings);
  expect(screen.getByRole('alert')).toHaveTextContent('invalid');
  expect(screen.getByText('Anchor')).toHaveFocus();
  act(() => screen.getByRole('button', { name: 'Close' }).click());
  expect(screen.queryByRole('alert')).toBeNull();
});

it('renders the browser immediately without invoking or showing a settings banner', async () => {
  vi.unstubAllGlobals();
  const { UiSettingsError } = await import('./UiSettingsGate');
  render(<LanguageProvider><UiSettingsGate><div>Browser catalog</div></UiSettingsGate><UiSettingsError /></LanguageProvider>);
  expect(screen.getByText('Browser catalog')).toBeVisible();
  expect(screen.queryByRole('alert')).toBeNull();
  expect(backend.invoke).not.toHaveBeenCalled();
});

it('mounts without waiting for slow backend baseline reads', async () => {
  const { settingBaseline, resetSettingsBaseline } = await import('../lib/settingsSession');
  resetSettingsBaseline();
  backend.invoke.mockImplementation((command: string) => command === 'get_auto_tagging' ? new Promise(() => {})
    : Promise.resolve({ schema_version: 1, migrated_from_webview: true, values: {} }));
  render(<LanguageProvider><UiSettingsGate><div data-testid="catalog" /></UiSettingsGate></LanguageProvider>);
  await act(async () => {});
  expect(screen.getByTestId('catalog')).toBeInTheDocument();
  expect(settingBaseline('theme')).toBeDefined();
  expect(settingBaseline('autoTags')).toBeUndefined();
});
