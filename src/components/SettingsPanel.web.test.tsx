import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { LanguageProvider } from '../i18n/LanguageContext';
import { SettingsPanel, type SettingsPanelProps } from './SettingsPanel';
import { getSession } from '../lib/webSession';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';

vi.mock('../diagnostics/DiagnosticsContext', () => ({ useDiagnostics: () => ({ openBugReport: vi.fn() }) }));

beforeEach(async () => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    if (url.startsWith('/api/invoke/')) return Response.json(url.endsWith('list_blocked_auto_tags') ? [] : true);
    return Response.json({ authenticated: true, writable: false, csrfToken: 'token', mustChangePassword: false });
  }));
  await getSession();
});
afterEach(() => { cleanup(); vi.useRealTimers(); vi.unstubAllGlobals(); });

function renderCatalogTab() {
  const props = {
    overlay: false, onClose: vi.fn(), onOpenPrinters: vi.fn(),
    themeSetting: 'system', onThemeChange: vi.fn(),
    uiDensity: 'compact', onUiDensityChange: vi.fn(),
    displayPreference: 'thumbnail', onDisplayPreferenceChange: vi.fn(),
    slicers: [], primarySlicerId: null, onAddSlicer: vi.fn(), addSlicerError: null,
    onRemoveSlicer: vi.fn(), onSetPrimarySlicer: vi.fn(),
    onScanCatalogIssues: vi.fn(), cleanupScanning: false, cleanupError: null,
    onExportCatalog: vi.fn(async () => true), catalogModelCount: 0, catalogFolderCount: 0,
    onCatalogReset: vi.fn(), onImportCatalog: vi.fn(), onClearCatalogBackupError: vi.fn(), catalogBackupError: null,
    catalogBaseDir: null, onOpenCatalogSetup: vi.fn(),
    update: {} as SettingsPanelProps['update'],
  } as SettingsPanelProps;
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container: true }}><SettingsPanel {...props} /></RuntimeEnvironmentProvider></LanguageProvider>);
  fireEvent.click(screen.getByRole('tab', { name: 'Katalog' }));
  return props;
}

it('offers the backup as a plain download link, even for a read-only session, and no desktop import', async () => {
  const props = renderCatalogTab();
  const link = await screen.findByRole('link', { name: de.exportCatalogButton });
  expect(link).toHaveAttribute('href', '/api/backup/export');
  expect(link).toHaveAttribute('download');
  expect(screen.getByText(de.catalogBackupWebHint)).toBeInTheDocument();
  expect(screen.getByText(de.catalogBackupWebFailureHint)).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: de.importCatalogButton })).toBeNull();
  expect(screen.queryByRole('button', { name: de.exportCatalogButton })).toBeNull();
  link.addEventListener('click', event => event.preventDefault());
  fireEvent.click(link);
  expect(props.onExportCatalog).not.toHaveBeenCalled();
});

it('keeps the same focused link while paused, blocks further clicks and re-enables it afterwards', async () => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
  renderCatalogTab();
  const link = screen.getByRole('link', { name: de.exportCatalogButton });
  // jsdom would try to navigate on the first (allowed) click; swallow that one only.
  link.addEventListener('click', event => event.preventDefault(), { once: true });
  link.focus();
  fireEvent.click(link);
  const paused = screen.getByRole('link', { name: de.exportCatalogButton });
  expect(paused).toBe(link);
  expect(paused).toHaveFocus();
  expect(paused).toHaveAttribute('aria-disabled', 'true');
  expect(paused).toHaveAttribute('href', '/api/backup/export');
  const second = new MouseEvent('click', { bubbles: true, cancelable: true });
  paused.dispatchEvent(second);
  expect(second.defaultPrevented).toBe(true);
  await act(async () => { vi.advanceTimersByTime(30_100); });
  const again = screen.getByRole('link', { name: de.exportCatalogButton });
  expect(again).toBe(link);
  expect(again).not.toHaveAttribute('aria-disabled');
});

it('has the new texts in all four languages', () => {
  for (const texts of [de, en, es, fr]) {
    expect(texts.catalogBackupWebHint.length).toBeGreaterThan(20);
    expect(texts.catalogBackupWebFailureHint.length).toBeGreaterThan(20);
  }
  expect(new Set([de, en, es, fr].map(t => t.catalogBackupWebHint)).size).toBe(4);
  // The real limits of a desktop restore are named.
  for (const texts of [de, en, es, fr]) expect(texts.catalogBackupWebHint).toContain('256');
  expect(de.catalogBackupWebHint).toContain('Windows');
  expect(de.catalogBackupWebFailureHint).toContain('neu an');
});
