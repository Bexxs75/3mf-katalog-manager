import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { Rail } from './Rail';
import { icons } from './icons.generated';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function renderRail(mainView: 'catalog' | 'filament' | 'trash' = 'catalog', trashCount = 3) {
  const props: Parameters<typeof Rail>[0] = {
    mainView, trashCount, onMainViewChange: vi.fn(),
    settingsOpen: false, onSettingsOpenChange: vi.fn(),
    themeSetting: 'system', onThemeChange: vi.fn(),
    uiDensity: 'compact', onUiDensityChange: vi.fn(),
    displayPreference: 'thumbnail', onDisplayPreferenceChange: vi.fn(),
    slicers: [], primarySlicerId: null, onAddSlicer: vi.fn(), addSlicerError: null,
    onRemoveSlicer: vi.fn(), onSetPrimarySlicer: vi.fn(),
    onScanCatalogIssues: vi.fn(), cleanupScanning: false, cleanupError: null,
    onExportCatalog: vi.fn(async () => true), catalogModelCount: 0, catalogFolderCount: 0,
    onCatalogReset: vi.fn(), onImportCatalog: vi.fn(), catalogBackupError: null,
    catalogBaseDir: null, onOpenCatalogSetup: vi.fn(), printerList: [],
    printerLink: {
      enabled: false, connections: [], jobs: [], error: null,
      refresh: vi.fn(), setEnabled: vi.fn(), testConnection: vi.fn(),
      removeConnection: vi.fn(), syncNow: vi.fn(), ignoreJob: vi.fn(),
      confirmJobs: vi.fn(), previewJob: vi.fn(),
    },
    update: {
      currentVersion: '0.15.0', info: null, phase: 'idle', progress: null,
      error: null, notesError: null, dismissed: false, preview: false,
      checkNow: vi.fn(), startUpdate: vi.fn(), install: vi.fn(), retry: vi.fn(),
      later: vi.fn(), dismiss: vi.fn(), openNotes: vi.fn(),
    },
  };
  render(<LanguageProvider><Rail {...props} /></LanguageProvider>);
  return props;
}

const buttons = [
  ['Katalog', 'catalog'], ['Material Manager', 'spool'],
  ['Papierkorb', 'trash'], ['Einstellungen', 'settings'],
] as const;

describe('Rail', () => {
  it('keeps four named 42px buttons in order with their approved 24px icons and focus style', () => {
    renderRail();
    const rendered = within(screen.getByRole('navigation')).getAllByRole('button');
    expect(rendered).toHaveLength(4);
    buttons.forEach(([label, name], index) => {
      const button = rendered[index];
      expect(button).toHaveAttribute('aria-label', label);
      expect(button).toHaveAttribute('title', label);
      expect(button).toHaveClass('w-[42px]', 'h-[42px]', 'grid', 'place-items-center',
        'focus-visible:outline', 'focus-visible:outline-2', 'focus-visible:outline-[var(--accent)]');
      const svg = button.querySelector('svg[aria-hidden="true"]');
      expect(svg).toHaveAttribute('width', '24');
      const expected = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      expected.innerHTML = icons[name];
      expect(svg?.innerHTML).toBe(expected.innerHTML);
    });
    expect(within(rendered[2]).getByText('3')).toHaveClass('absolute', '-top-1', '-right-1');
  });

  it.each([
    ['Katalog', 'catalog'], ['Material Manager', 'filament'], ['Papierkorb', 'trash'],
  ] as const)('keeps %s active and dispatches its view on click', (label, view) => {
    const props = renderRail(view);
    const button = screen.getByRole('button', { name: label });
    expect(button).toHaveClass('text-[var(--accent)]');
    for (const [other] of buttons.slice(0, 3)) {
      if (other !== label) expect(screen.getByRole('button', { name: other })).not.toHaveClass('text-[var(--accent)]');
    }
    fireEvent.click(button);
    expect(props.onMainViewChange).toHaveBeenCalledExactlyOnceWith(view);
  });

  it('opens settings on click and omits the badge for an empty trash', () => {
    const props = renderRail('catalog', 0);
    expect(screen.getByRole('button', { name: 'Papierkorb' }).querySelector('span')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Einstellungen' }));
    expect(props.onSettingsOpenChange).toHaveBeenCalledExactlyOnceWith(true);
  });
});
