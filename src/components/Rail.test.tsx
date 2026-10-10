import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { Rail } from './Rail';
import { icons } from './icons.generated';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function renderRail(mainView: 'catalog' | 'filament' | 'printers' | 'trash' = 'catalog', trashCount = 3, controlledSettings = false, container = false) {
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
    onCatalogReset: vi.fn(), onImportCatalog: vi.fn(), onClearCatalogBackupError: vi.fn(), catalogBackupError: null,
    catalogBaseDir: null, onOpenCatalogSetup: vi.fn(),
    update: {
      currentVersion: '0.15.0', info: null, phase: 'idle', progress: null,
      error: null, notesError: null, dismissed: false, preview: false,
      checkNow: vi.fn(), startUpdate: vi.fn(), install: vi.fn(), retry: vi.fn(),
      later: vi.fn(), dismiss: vi.fn(), openNotes: vi.fn(),
    },
  };
  function ControlledRail() {
    const [settingsOpen, setSettingsOpen] = useState(false);
    return <Rail {...props} settingsOpen={settingsOpen} onSettingsOpenChange={(open) => {
      props.onSettingsOpenChange(open);
      setSettingsOpen(open);
    }} />;
  }
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container }}>{controlledSettings ? <ControlledRail /> : <Rail {...props} />}</RuntimeEnvironmentProvider></LanguageProvider>);
  return props;
}

const buttons = [
  ['Katalog', 'catalog'], ['Material Manager', 'spool'],
  ['Printer Manager', 'printer'], ['Papierkorb', 'trash'], ['Einstellungen', 'settings'],
] as const;

describe('Rail', () => {
  it('keeps five named 42px buttons in order with their approved 24px icons and focus style', () => {
    renderRail();
    const rendered = within(screen.getByRole('navigation')).getAllByRole('button');
    expect(rendered).toHaveLength(5);
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
    expect(within(rendered[3]).getByText('3')).toHaveClass('absolute', '-top-1', '-right-1');
  });

  it.each([
    ['Katalog', 'catalog'], ['Material Manager', 'filament'], ['Printer Manager', 'printers'], ['Papierkorb', 'trash'],
  ] as const)('keeps %s active and dispatches its view on click', (label, view) => {
    const props = renderRail(view);
    const button = screen.getByRole('button', { name: label });
    expect(button).toHaveClass('text-[var(--accent)]');
    for (const [other] of buttons.slice(0, 4)) {
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

describe('Rail nonmodal settings keyboard behavior', () => {
  it.each(['Enter', ' '])('focuses the first panel button when opened with %s', (key) => {
    const props = renderRail('catalog', 0, true);
    const gear = screen.getByRole('button', { name: 'Einstellungen' });
    gear.focus();
    fireEvent.keyDown(gear, { key });
    // Native buttons synthesize a click with detail=0 for keyboard activation.
    fireEvent.click(gear, { detail: 0 });
    expect(props.onSettingsOpenChange).toHaveBeenCalledWith(true);
    expect(screen.getByRole('button', { name: 'Allgemein' })).toHaveFocus();
  });

  it('closes with Escape inside the panel and returns focus to the gear', () => {
    const props = renderRail('catalog', 0, true);
    const gear = screen.getByRole('button', { name: 'Einstellungen' });
    fireEvent.click(gear);
    const first = screen.getByRole('button', { name: 'Allgemein' });
    first.focus();
    fireEvent.keyDown(first, { key: 'Escape' });
    expect(props.onSettingsOpenChange).toHaveBeenLastCalledWith(false);
    expect(screen.queryByRole('button', { name: 'Allgemein' })).not.toBeInTheDocument();
    expect(gear).toHaveFocus();
  });

  it('keeps settings open when Escape is pressed outside and does not trap Tab', () => {
    const props = renderRail('catalog', 0, true);
    fireEvent.click(screen.getByRole('button', { name: 'Einstellungen' }));
    const first = screen.getByRole('button', { name: 'Allgemein' });
    first.focus();
    expect(fireEvent.keyDown(first, { key: 'Tab', shiftKey: true })).toBe(true);
    const last = screen.getByRole('button', { name: 'Français' });
    last.focus();
    expect(fireEvent.keyDown(last, { key: 'Tab' })).toBe(true);
    const catalog = screen.getByLabelText('Katalog');
    catalog.focus();
    fireEvent.keyDown(catalog, { key: 'Escape' });
    expect(props.onSettingsOpenChange).not.toHaveBeenCalledWith(false);
    expect(first).toBeInTheDocument();
    expect(catalog).toHaveFocus();
    expect(document.querySelector('[role="dialog"][aria-modal="true"]')).toBeNull();
  });
});

it('Escape in a portalled catalog confirmation leaves settings open and restores its trigger', () => {
  const props = renderRail('catalog', 0, true);
  fireEvent.click(screen.getByRole('button', { name: 'Einstellungen' }), { detail: 0 });
  const catalogButtons = screen.getAllByRole('button', { name: 'Katalog' });
  fireEvent.click(catalogButtons[catalogButtons.length - 1]);
  const trigger = screen.getByRole('button', { name: 'Katalog zurücksetzen …' });
  trigger.focus(); fireEvent.click(trigger);
  expect(screen.getByRole('dialog')).toBeInTheDocument();
  fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(props.onSettingsOpenChange).not.toHaveBeenCalledWith(false);
  expect(trigger).toHaveFocus();
});

it('closes settings with the cross and restores focus to the gear', () => {
  const props = renderRail('catalog', 0, true);
  const gear = screen.getByRole('button', { name: 'Einstellungen' });
  fireEvent.click(gear);
  const close = screen.getByRole('button', { name: 'Einstellungen schließen' });
  expect(close.closest('[style]')?.getAttribute('style')).toContain('var(--sidebar-width');
  fireEvent.click(close);
  expect(props.onSettingsOpenChange).toHaveBeenLastCalledWith(false);
  expect(screen.queryByRole('button', { name: 'Einstellungen schließen' })).toBeNull();
  expect(gear).toHaveFocus();
});

it.each(['catalog', 'filament', 'trash'] as const)('measures the panel offset in %s including the rail border and bottom padding', (mainView) => {
  const rect = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
    if (this.tagName === 'NAV') return { right: 60 } as DOMRect;
    return { left: 8.5, bottom: window.innerHeight - 10 } as DOMRect;
  });
  try {
    renderRail(mainView, 0, true);
    fireEvent.click(screen.getByRole('button', { name: 'Einstellungen' }));
    const panel = screen.getByRole('button', { name: 'Einstellungen schließen' }).closest('[style]') as HTMLElement;
    expect(panel.style.left).toBe('calc(51.5px + var(--sidebar-width, 0px) + 10px)');
    expect(panel.style.bottom).toBe('0px');
    rect.mockImplementation(function (this: HTMLElement) {
      if (this.tagName === 'NAV') return { right: 60 } as DOMRect;
      return { left: 8, bottom: window.innerHeight + 20 } as DOMRect;
    });
    fireEvent(window, new Event('resize'));
    expect(panel.style.left).toBe('calc(52px + var(--sidebar-width, 0px) + 10px)');
    expect(panel.style.bottom).toBe('30px');
  } finally { rect.mockRestore(); }
});

it('moves printer settings to General and opens Printer Manager', () => {
  const props = renderRail('catalog', 0, true);
  fireEvent.click(screen.getByRole('button', { name: 'Einstellungen' }));
  expect(screen.queryByRole('button', { name: 'Drucker' })).toBeNull();
  expect(screen.getByText('Drucker findest du jetzt im Printer Manager in der linken Leiste.')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Printer Manager öffnen' }));
  expect(props.onMainViewChange).toHaveBeenCalledWith('printers');
  expect(props.onSettingsOpenChange).toHaveBeenLastCalledWith(false);
});

it('clears backup feedback when opening the import confirmation', () => {
  const props = renderRail('catalog', 0, true);
  fireEvent.click(screen.getByRole('button', { name: 'Einstellungen' }));
  const catalogButtons = screen.getAllByRole('button', { name: 'Katalog' });
  fireEvent.click(catalogButtons[catalogButtons.length - 1]);
  fireEvent.click(screen.getByRole('button', { name: 'Katalog importieren' }));
  expect(props.onClearCatalogBackupError).toHaveBeenCalledTimes(1);
  expect(screen.getByText('Ersetzt den kompletten aktuellen Katalog. Fortfahren?')).toBeVisible();
  expect(props.onImportCatalog).not.toHaveBeenCalled();
});

it('reserves scrollbar space in the settings scroller before messages grow', () => {
  renderRail('catalog', 0, true);
  fireEvent.click(screen.getByRole('button', {name: 'Einstellungen'}));
  const panel = screen.getByRole('button', {name: 'Allgemein'}).closest('.scrollbar-stable');
  expect(panel).toHaveClass('scrollbar-stable', 'overflow-y-scroll');
});


it('replaces container slicer settings with a translated explanation', () => {
  renderRail('catalog', 0, true, true);
  fireEvent.click(screen.getByRole('button', { name: 'Einstellungen' }));
  fireEvent.click(screen.getByRole('button', { name: 'Slicer' }));
  expect(screen.getByText(/Der Slicer läuft auf deinem Rechner/)).toBeVisible();
  expect(screen.queryByText('Slicer hinzufügen')).not.toBeInTheDocument();
});
