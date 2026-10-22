import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { SettingsPanel, type SettingsPanelProps } from './SettingsPanel';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async (command: string) => command === 'get_data_paths' ? { data: '/config/data', logs: '/config/logs' } : { enabled: false, untilMs: null }) }));
vi.mock('../diagnostics/DiagnosticsContext', () => ({ useDiagnostics: () => ({ openBugReport: vi.fn() }) }));

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function renderPanel(overrides: Partial<SettingsPanelProps> = {}, container = false) {
  const props: SettingsPanelProps = {
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
    update: {
      currentVersion: '0.15.0', info: null, phase: 'idle', progress: null,
      error: null, notesError: null, dismissed: false, preview: false,
      checkNow: vi.fn(), startUpdate: vi.fn(), install: vi.fn(), retry: vi.fn(),
      later: vi.fn(), dismiss: vi.fn(), openNotes: vi.fn(),
    },
    ...overrides,
  };
  const ui = (p: SettingsPanelProps) => <LanguageProvider><RuntimeEnvironmentProvider value={{ container }}><SettingsPanel {...p} /></RuntimeEnvironmentProvider></LanguageProvider>;
  const result = render(ui(props));
  return { props, rerender: (next: Partial<SettingsPanelProps>) => result.rerender(ui({ ...props, ...next })) };
}

const tab = (name: string) => screen.getByRole('tab', { name });

describe('SettingsPanel structure and keyboard', () => {
  it('is a named complementary region with real tabs, and focuses the selected tab on open', () => {
    renderPanel();
    const region = screen.getByRole('complementary', { name: 'Einstellungen' });
    const tabs = within(region).getAllByRole('tab');
    expect(tabs.map(node => node.textContent)).toEqual(['Allgemein', 'Slicer', 'Katalog', 'Info']);
    expect(tab('Allgemein')).toHaveAttribute('aria-selected', 'true');
    expect(tab('Allgemein')).toHaveFocus();
    expect(screen.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', tab('Allgemein').id);
  });

  it('moves between tabs with the arrow keys, Home and End using a roving tab stop', () => {
    renderPanel();
    fireEvent.keyDown(tab('Allgemein'), { key: 'ArrowRight' });
    expect(tab('Slicer')).toHaveFocus();
    expect(tab('Slicer')).toHaveAttribute('aria-selected', 'true');
    expect(tab('Slicer')).toHaveAttribute('tabindex', '0');
    expect(tab('Allgemein')).toHaveAttribute('tabindex', '-1');
    fireEvent.keyDown(tab('Slicer'), { key: 'End' });
    expect(tab('Info')).toHaveFocus();
    fireEvent.keyDown(tab('Info'), { key: 'ArrowRight' });
    expect(tab('Allgemein')).toHaveFocus();
    fireEvent.keyDown(tab('Allgemein'), { key: 'ArrowLeft' });
    expect(tab('Info')).toHaveFocus();
    fireEvent.keyDown(tab('Info'), { key: 'Home' });
    expect(tab('Allgemein')).toHaveFocus();
  });

  it('shows every section of the General tab, including source folders and disk sync', () => {
    renderPanel({ onDetailPanelChange: vi.fn() });
    expect(screen.getByRole('switch', { name: 'Einzeltasten-Kürzel' })).toBeVisible();
    expect(screen.getByText('Quellordner')).toBeVisible();
    expect(screen.getByText(/Abgleich mit der Festplatte/)).toBeVisible();
    expect(screen.getByRole('radio', { name: 'Immer sichtbar' })).toBeVisible();
  });

  it('closes with the cross', () => {
    const { props } = renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'Einstellungen schließen' }));
    expect(props.onClose).toHaveBeenCalledTimes(1);
  });

  it('closes with Escape from inside the panel and when nothing holds focus', () => {
    const { props } = renderPanel();
    fireEvent.keyDown(tab('Allgemein'), { key: 'Escape' });
    expect(props.onClose).toHaveBeenCalledTimes(1);
    (document.activeElement as HTMLElement).blur();
    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(props.onClose).toHaveBeenCalledTimes(2);
  });

  it('keeps Escape for a control outside the panel that holds focus', () => {
    const { props } = renderPanel();
    const outside = document.createElement('button');
    document.body.append(outside);
    outside.focus();
    fireEvent.keyDown(outside, { key: 'Escape' });
    expect(props.onClose).not.toHaveBeenCalled();
    outside.remove();
  });

  it('leaves Escape to an open menu', () => {
    const { props } = renderPanel();
    const menu = document.createElement('div');
    menu.setAttribute('role', 'menu');
    document.body.append(menu);
    fireEvent.keyDown(tab('Allgemein'), { key: 'Escape' });
    expect(props.onClose).not.toHaveBeenCalled();
    menu.remove();
  });

  it('Escape in a portalled catalog confirmation closes only that dialog', () => {
    const { props } = renderPanel();
    fireEvent.click(tab('Katalog'));
    const trigger = screen.getByRole('button', { name: 'Katalog zurücksetzen …' });
    trigger.focus(); fireEvent.click(trigger);
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(props.onClose).not.toHaveBeenCalled();
    expect(trigger).toHaveFocus();
  });
});

describe('SettingsPanel layout', () => {
  it('takes the detail panel place by default and lies over the view as an overlay', () => {
    const { rerender } = renderPanel();
    const region = screen.getByRole('complementary', { name: 'Einstellungen' });
    expect(region).toHaveClass('detail-panel-shell');
    expect(region).not.toHaveClass('detail-panel-overlay');
    expect(region.querySelector('aside')).toHaveClass('w-[336px]', 'min-h-0');
    rerender({ overlay: true });
    expect(screen.getByRole('complementary', { name: 'Einstellungen' })).toHaveClass('detail-panel-overlay');
  });

  it('scrolls its content without a second floating scroller', () => {
    renderPanel();
    expect(screen.getByRole('tabpanel')).toHaveClass('overflow-y-auto', 'scrollbar-stable');
  });
});

describe('SettingsPanel content', () => {
  it('opens the General tab again when the app requests it', () => {
    const { rerender } = renderPanel({ generalTabRequest: 0 });
    fireEvent.click(tab('Info'));
    expect(tab('Info')).toHaveAttribute('aria-selected', 'true');
    rerender({ generalTabRequest: 1 });
    expect(tab('Allgemein')).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('switch', { name: 'Einzeltasten-Kürzel' })).toBeVisible();
  });

  it('moves printer settings to General and opens Printer Manager', () => {
    const { props } = renderPanel();
    expect(screen.queryByRole('tab', { name: 'Drucker' })).toBeNull();
    expect(screen.getByText('Drucker findest du jetzt im Printer Manager in der linken Leiste.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Printer Manager öffnen' }));
    expect(props.onOpenPrinters).toHaveBeenCalledTimes(1);
  });

  it('clears backup feedback when opening the import confirmation', () => {
    const { props } = renderPanel();
    fireEvent.click(tab('Katalog'));
    fireEvent.click(screen.getByRole('button', { name: 'Katalog importieren' }));
    expect(props.onClearCatalogBackupError).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Ersetzt den kompletten aktuellen Katalog. Fortfahren?')).toBeVisible();
    expect(props.onImportCatalog).not.toHaveBeenCalled();
  });

  it('replaces container slicer settings with a translated explanation', () => {
    renderPanel({}, true);
    fireEvent.click(tab('Slicer'));
    expect(screen.getByText(/Der Slicer läuft auf deinem Rechner/)).toBeVisible();
    expect(screen.queryByText('Slicer hinzufügen')).not.toBeInTheDocument();
  });

  it.each([false, true])('keeps Info links usable with container=%s', async container => {
    renderPanel({}, container);
    fireEvent.click(tab('Info'));
    for (const [label, url] of [
      ['GitHub', 'https://github.com/Bexxs75/3mf-katalog-manager/'],
      ['Discord', 'https://discord.gg/abfVNfFqu3'],
      ['UnRAR', 'https://github.com/Bexxs75/3mf-katalog-manager/blob/master/THIRD-PARTY-LICENSES.md#unrar'],
    ]) {
      if (container) {
        expect(screen.getByText(url)).toBeVisible();
        expect(screen.getByRole('button', { name: `Link kopieren: ${label}` })).toBeVisible();
        expect(screen.queryByRole('button', { name: label })).not.toBeInTheDocument();
      } else {
        fireEvent.click(screen.getByRole('button', { name: label }));
        if (label === 'Discord') expect(invoke).toHaveBeenCalledWith('open_discord_invite');
        else expect(invoke).toHaveBeenCalledWith('open_release_url', { url });
      }
    }
    if (container) expect(await screen.findByText(/Daten unter \/config\/data/)).toBeVisible();
  });
});

describe('SettingsPanel blocked automatic tags', () => {
  it('shows the blocked-tags list in the catalog tab', async () => {
    vi.mocked(invoke).mockImplementation((async (command: string) => command === 'list_blocked_auto_tags' ? ['miniatur'] : { enabled: false, untilMs: null }) as typeof invoke);
    renderPanel();
    expect(screen.queryByText('Gesperrte automatische Tags')).toBeNull();
    fireEvent.click(tab('Katalog'));
    expect(screen.getByText('Gesperrte automatische Tags')).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Sperre für #miniatur aufheben' })).toBeInTheDocument();
  });
});
