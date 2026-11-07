import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { SettingsPanel, type SettingsPanelProps } from './SettingsPanel';
import { invoke } from '@tauri-apps/api/core';
import { de } from '../i18n/de';

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
  return { props, unmountAll: () => result.unmount(), rerender: (next: Partial<SettingsPanelProps>) => result.rerender(ui({ ...props, ...next })) };
}

const tab = (name: string) => screen.getByRole('tab', { name });

describe('SettingsPanel structure and keyboard', () => {
  it('is a named complementary region with real tabs, and focuses the selected tab on open', () => {
    renderPanel();
    const region = screen.getByRole('complementary', { name: 'Einstellungen' });
    const tabs = within(region).getAllByRole('tab');
    expect(tabs.map(node => node.textContent)).toEqual(['Allgemein', 'Katalog', 'Slicer & Dienste', 'Wartung', 'Info']);
    expect(tab('Allgemein')).toHaveAttribute('aria-selected', 'true');
    expect(tab('Allgemein')).toHaveFocus();
    expect(screen.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', tab('Allgemein').id);
  });

  it('moves between tabs with the arrow keys, Home and End using a roving tab stop', () => {
    renderPanel();
    fireEvent.keyDown(tab('Allgemein'), { key: 'ArrowRight' });
    expect(tab('Katalog')).toHaveFocus();
    expect(tab('Katalog')).toHaveAttribute('aria-selected', 'true');
    expect(tab('Katalog')).toHaveAttribute('tabindex', '0');
    expect(tab('Allgemein')).toHaveAttribute('tabindex', '-1');
    fireEvent.keyDown(tab('Katalog'), { key: 'End' });
    expect(tab('Info')).toHaveFocus();
    fireEvent.keyDown(tab('Info'), { key: 'ArrowRight' });
    expect(tab('Allgemein')).toHaveFocus();
    fireEvent.keyDown(tab('Allgemein'), { key: 'ArrowLeft' });
    expect(tab('Info')).toHaveFocus();
    fireEvent.keyDown(tab('Info'), { key: 'Home' });
    expect(tab('Allgemein')).toHaveFocus();
  });

  it('shows the sections of the General tab as level-3 headings in the agreed order', () => {
    renderPanel({ onDetailPanelChange: vi.fn() });
    expect(screen.getByRole('heading', { level: 2, name: 'Einstellungen' })).toBeVisible();
    // Program behavior comes last and only shows where the platform has something to offer.
    const headings = screen.getAllByRole('heading', { level: 3 }).map(h => h.textContent);
    expect(headings.slice(0, 4)).toEqual(['Darstellung', 'Sprache', 'Bedienung', 'Druckverfahren']);
    expect(screen.getByRole('switch', { name: 'Einzeltasten-Kürzel' })).toBeVisible();
    expect(screen.getByRole('radio', { name: 'Immer sichtbar' })).toBeVisible();
  });

  it('puts storage, source folders, import, sync, tags and print data on the Catalog tab', () => {
    renderPanel();
    fireEvent.click(tab('Katalog'));
    expect(screen.getAllByRole('heading', { level: 3 }).map(h => h.textContent)).toEqual([
      'Speicherort', 'Quellordner', 'Beim Import', 'Katalogordner abgleichen', 'Automatische Tags', 'Druckdaten',
    ]);
  });

  it('lists the sections of the Services, Maintenance and Info tabs in the agreed order', () => {
    renderPanel();
    const headingsOf = (name: string) => { fireEvent.click(tab(name)); return screen.getAllByRole('heading', { level: 3 }).map(h => h.textContent?.replace(/\s*Experimentell$/, '')); };
    expect(headingsOf('Slicer & Dienste')).toEqual(['Dateien weitergeben', 'Installierte Slicer', 'Meldungen vom Slicer', 'Drucker', 'Thingiverse']);
    expect(headingsOf('Wartung')).toEqual(['Katalog prüfen', 'Sichern und wiederherstellen', 'Katalog zurücksetzen']);
    expect(headingsOf('Info')).toEqual(['Version und Updates', 'Vorab-Versionen', 'Hilfe und Fehlersuche', 'App-Daten und Protokolle', 'Über die App']);
  });

  it('marks drag-out and Thingiverse as experimental and nothing else', () => {
    renderPanel();
    fireEvent.click(tab('Slicer & Dienste'));
    const experimental = screen.getAllByRole('heading', { level: 3 }).filter(h => h.textContent?.includes(de.tvExperimental));
    expect(experimental.map(h => h.textContent?.replace(de.tvExperimental, '').trim())).toEqual(['Dateien weitergeben', 'Thingiverse']);
  });

  it('keeps the reset action visibly set apart in the Maintenance tab', () => {
    renderPanel();
    fireEvent.click(tab('Wartung'));
    const heading = screen.getByRole('heading', { level: 3, name: 'Katalog zurücksetzen' });
    expect(heading).toHaveClass('text-[var(--crit)]');
    expect(screen.getByRole('button', { name: 'Katalog zurücksetzen …' })).toBeVisible();
  });

  it('hides the preview-version section in a preview build', () => {
    const view = { currentVersion: '0.15.0', info: null, phase: 'idle', progress: null, error: null, notesError: null, dismissed: false, preview: true,
      checkNow: vi.fn(), startUpdate: vi.fn(), install: vi.fn(), retry: vi.fn(), later: vi.fn(), dismiss: vi.fn(), openNotes: vi.fn() } as unknown as SettingsPanelProps['update'];
    renderPanel({ update: view });
    fireEvent.click(tab('Info'));
    expect(screen.queryByRole('heading', { name: 'Vorab-Versionen' })).toBeNull();
    expect(screen.queryByRole('checkbox', { name: de.updateChannelLabel })).toBeNull();
  });

  it('puts the preview-version checkbox under Info in its own section', () => {
    renderPanel();
    fireEvent.click(tab('Info'));
    const section = screen.getByRole('heading', { level: 3, name: 'Vorab-Versionen' }).closest('section')!;
    expect(within(section).getByRole('checkbox', { name: de.updateChannelLabel })).toBeVisible();
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
    fireEvent.click(tab('Wartung'));
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
    expect(screen.getByRole('dialog', { name: 'Einstellungen' })).toHaveClass('detail-panel-overlay');
  });

  it('scrolls its content without a second floating scroller', () => {
    renderPanel();
    expect(screen.getByRole('tabpanel')).toHaveClass('overflow-y-auto', 'scrollbar-stable');
  });
});

describe('SettingsPanel content', () => {
  it.each([
    ['printMethod', 'Allgemein', 'Druckverfahren'],
    ['sourceFolders', 'Katalog', 'Quellordner'],
    ['printData', 'Katalog', 'Druckdaten'],
    ['storage', 'Katalog', 'Speicherort'],
    ['dragOut', 'Slicer & Dienste', /Dateien weitergeben/],
    ['slicers', 'Slicer & Dienste', 'Installierte Slicer'],
    ['thingiverse', 'Slicer & Dienste', /Thingiverse/],
  ] as const)('opens the %s target on its tab and puts the focus on the section heading', (target, tabName, heading) => {
    renderPanel({ request: { target, nonce: 1 } });
    expect(tab(tabName)).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('heading', { level: 3, name: heading })).toHaveFocus();
  });

  it('moves to the requested section again for a new request, also from another tab', () => {
    const { rerender } = renderPanel({ request: { target: 'sourceFolders', nonce: 1 } });
    fireEvent.click(tab('Info'));
    rerender({ request: { target: 'sourceFolders', nonce: 2 } });
    expect(tab('Katalog')).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('heading', { name: 'Quellordner' })).toHaveFocus();
    rerender({ request: { target: 'slicers', nonce: 3 } });
    expect(tab('Slicer & Dienste')).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('heading', { name: 'Installierte Slicer' })).toHaveFocus();
  });

  it('scrolls the requested section into view', () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    try {
      renderPanel({ request: { target: 'printData', nonce: 1 } });
      expect(scrollIntoView).toHaveBeenCalled();
      expect(scrollIntoView.mock.contexts[0]).toBe(screen.getByRole('heading', { name: 'Druckdaten' }));
    } finally {
      delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
    }
  });

  it('uses the same navigation for the link under the storage location', () => {
    renderPanel();
    fireEvent.click(tab('Katalog'));
    fireEvent.click(screen.getByRole('button', { name: de.settingsGoToHandover }));
    expect(tab('Slicer & Dienste')).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('heading', { name: /Dateien weitergeben/ })).toHaveFocus();
    expect(screen.getAllByRole('switch', { name: de.dgSwitchLabel })).toHaveLength(1);
  });

  it('offers no hand-over link in the container, where drag-out does not exist', () => {
    renderPanel({}, true);
    fireEvent.click(tab('Katalog'));
    expect(screen.queryByRole('button', { name: de.settingsGoToHandover })).toBeNull();
  });

  it('starts on the tab given by the app and reports every change of tab', () => {
    const onTabChange = vi.fn();
    renderPanel({ initialTab: 'maintenance', onTabChange });
    expect(tab('Wartung')).toHaveAttribute('aria-selected', 'true');
    expect(tab('Wartung')).toHaveFocus();
    fireEvent.keyDown(tab('Wartung'), { key: 'ArrowRight' });
    expect(onTabChange).toHaveBeenLastCalledWith('info');
    fireEvent.click(tab('Katalog'));
    expect(onTabChange).toHaveBeenLastCalledWith('catalog');
  });

  it('starts every tab at the top instead of keeping the scroll position of the last one', () => {
    renderPanel();
    const body = screen.getByRole('tabpanel');
    body.scrollTop = 240;
    fireEvent.click(tab('Katalog'));
    expect(body.scrollTop).toBe(0);
  });

  it('is not a modal window while docked: no focus limit, no dialog role', () => {
    renderPanel();
    const region = screen.getByRole('complementary', { name: 'Einstellungen' });
    expect(region).not.toHaveAttribute('aria-modal');
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('shows the printer pointer under Slicer & Dienste and opens Printer Manager', () => {
    const { props } = renderPanel();
    expect(screen.queryByRole('tab', { name: 'Drucker' })).toBeNull();
    fireEvent.click(tab('Slicer & Dienste'));
    expect(screen.getByText('Drucker findest du jetzt im Printer Manager in der linken Leiste.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Printer Manager öffnen' }));
    expect(props.onOpenPrinters).toHaveBeenCalledTimes(1);
  });

  it('clears backup feedback when opening the import confirmation', () => {
    const { props } = renderPanel();
    fireEvent.click(tab('Wartung'));
    fireEvent.click(screen.getByRole('button', { name: 'Katalog importieren' }));
    expect(props.onClearCatalogBackupError).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Ersetzt den kompletten aktuellen Katalog. Fortfahren?')).toBeVisible();
    expect(screen.getByText('Dein Thingiverse-Schlüssel bleibt auf diesem Rechner erhalten und ist in keiner Sicherung enthalten.')).toBeVisible();
    expect(props.onImportCatalog).not.toHaveBeenCalled();
  });

  it('replaces container slicer settings with a translated explanation', () => {
    renderPanel({}, true);
    expect(screen.queryByRole('tab', { name: 'Slicer & Dienste' })).toBeInTheDocument();
    fireEvent.click(tab('Slicer & Dienste'));
    expect(screen.getByText('Lokale Slicer und Dateiübergabe sind im Container nicht verfügbar.')).toBeVisible();
    expect(screen.queryByText('Slicer hinzufügen')).not.toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: /Dateien weitergeben|Installierte Slicer|Meldungen vom Slicer/ })).toBeNull();
    // What the container can do stays: printers and Thingiverse.
    expect(screen.getByRole('heading', { name: 'Drucker' })).toBeVisible();
    expect(screen.getByRole('heading', { name: /Thingiverse/ })).toBeVisible();
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
    vi.mocked(invoke).mockImplementation((async (command: string) => command === 'list_blocked_auto_tags' ? ['miniatur'] : command === 'list_source_folders' ? [] : { enabled: false, untilMs: null }) as typeof invoke);
    renderPanel();
    expect(screen.queryByText('Gesperrte automatische Tags')).toBeNull();
    fireEvent.click(tab('Katalog'));
    expect(screen.getByText('Gesperrte automatische Tags')).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Sperre für #miniatur aufheben' })).toBeInTheDocument();
  });
});

describe('SettingsPanel catalog backup on the desktop', () => {
  it('keeps the export button that calls the backend and shows no web download link', () => {
    const { props } = renderPanel();
    fireEvent.click(tab('Wartung'));
    fireEvent.click(screen.getByRole('button', { name: 'Katalog exportieren' }));
    expect(props.onExportCatalog).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('link', { name: 'Katalog exportieren' })).toBeNull();
  });
});

describe('SettingsPanel selection groups', () => {
  it('offers appearance and language choices as named radio groups', () => {
    const onThemeChange = vi.fn();
    renderPanel({ onDetailPanelChange: vi.fn(), onThemeChange });
    for (const [legend, options] of [
      ['Farbschema', ['System', 'Hell', 'Dunkel']],
      ['Darstellungsdichte', ['Kompakt', 'Komfort']],
      ['Modellvorschau', ['Bild', '3D-Ansicht']],
      ['Detailbereich', ['Automatisch (erscheint beim Anklicken)', 'Immer sichtbar']],
      ['Sprache', ['Deutsch', 'English', 'Español', 'Français']],
    ] as const) {
      const group = screen.getByRole('group', { name: legend });
      expect(within(group).getAllByRole('radio').map(r => r.closest('label')?.textContent)).toEqual(options);
    }
    expect(within(screen.getByRole('group', { name: 'Sprache' })).getByRole('radio', { name: 'Deutsch' })).toBeChecked();
    fireEvent.click(within(screen.getByRole('group', { name: 'Farbschema' })).getByRole('radio', { name: 'Dunkel' }));
    expect(onThemeChange).toHaveBeenCalledWith('dark');
  });

  it('switches the language through the radio group', () => {
    renderPanel();
    fireEvent.click(screen.getByRole('radio', { name: 'English' }));
    expect(screen.getByRole('group', { name: 'Language' })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'English' })).toBeChecked();
  });
});

describe('SettingsPanel slicer list', () => {
  const slicers = [
    { id: 'a', name: 'Bambu Studio', path: '/opt/bambu' },
    { id: 'b', name: 'OrcaSlicer', path: '/opt/orca' },
    { id: 'c', name: 'PrusaSlicer', path: '/opt/prusa' },
  ];

  it('puts the drag-out section above the slicer list', async () => {
    renderPanel({ slicers, primarySlicerId: 'a' });
    fireEvent.click(tab('Slicer & Dienste'));
    const heading = await screen.findByRole('heading', { level: 3, name: new RegExp(de.settingsSecHandover) });
    expect(screen.getAllByRole('switch', { name: de.dgSwitchLabel })).toHaveLength(1);
    const firstRemove = screen.getAllByRole('button', { name: /aus der Slicer-Liste entfernen/ })[0];
    expect(heading.compareDocumentPosition(firstRemove) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('removes with a real, individually named button', () => {
    const { props } = renderPanel({ slicers, primarySlicerId: 'a' });
    fireEvent.click(tab('Slicer & Dienste'));
    const button = screen.getByRole('button', { name: 'OrcaSlicer aus der Slicer-Liste entfernen' });
    expect(button.tagName).toBe('BUTTON');
    expect(button).toHaveAttribute('type', 'button');
    expect(screen.getAllByRole('button', { name: /aus der Slicer-Liste entfernen/ })).toHaveLength(3);
    fireEvent.click(button);
    expect(props.onRemoveSlicer).toHaveBeenCalledWith('b');
  });

  it('offers the default slicer as a radio group with one named option per slicer', () => {
    const { props } = renderPanel({ slicers, primarySlicerId: 'b' });
    fireEvent.click(tab('Slicer & Dienste'));
    const group = screen.getByRole('group', { name: de.slicerDefaultLegend });
    expect(within(group).getAllByRole('radio').map(r => r.getAttribute('aria-label'))).toEqual(['Bambu Studio', 'OrcaSlicer', 'PrusaSlicer']);
    expect(within(group).getByRole('radio', { name: 'OrcaSlicer' })).toBeChecked();
    expect(within(group).getByRole('radio', { name: 'Bambu Studio' })).not.toBeChecked();
    fireEvent.click(within(group).getByRole('radio', { name: 'PrusaSlicer' }));
    expect(props.onSetPrimarySlicer).toHaveBeenCalledWith('c');
  });

  it('moves the focus to the next entry after a removal, and to the add button after the last one', () => {
    const { rerender } = renderPanel({ slicers, primarySlicerId: 'a' });
    fireEvent.click(tab('Slicer & Dienste'));
    fireEvent.click(screen.getByRole('button', { name: 'OrcaSlicer aus der Slicer-Liste entfernen' }));
    rerender({ slicers: [slicers[0], slicers[2]] });
    expect(screen.getByRole('radio', { name: 'PrusaSlicer' })).toHaveFocus();
    fireEvent.click(screen.getByRole('button', { name: 'PrusaSlicer aus der Slicer-Liste entfernen' }));
    rerender({ slicers: [slicers[0]] });
    expect(screen.getByRole('button', { name: 'Hinzufügen' })).toHaveFocus();
  });

  it('keeps the focus where it is while the removal has not happened', () => {
    const { rerender } = renderPanel({ slicers, primarySlicerId: 'a' });
    fireEvent.click(tab('Slicer & Dienste'));
    const button = screen.getByRole('button', { name: 'Bambu Studio aus der Slicer-Liste entfernen' });
    button.focus();
    fireEvent.click(button);
    rerender({ slicers: [...slicers] });
    expect(button).toHaveFocus();
  });
});

describe('SettingsPanel focus handling', () => {
  function withTrigger() {
    const trigger = document.createElement('button');
    trigger.textContent = 'Auslöser';
    document.body.append(trigger);
    trigger.focus();
    return trigger;
  }

  it('gives the focus back to the opener when a docked panel closes', () => {
    const trigger = withTrigger();
    const { unmount } = render(<span />);
    unmount();
    const panel = renderPanel();
    expect(tab('Allgemein')).toHaveFocus();
    panel.unmountAll();
    expect(trigger).toHaveFocus();
    trigger.remove();
  });

  it('as an overlay it is a modal dialog that keeps Tab inside and returns the focus when it closes', () => {
    const trigger = withTrigger();
    const outside = document.createElement('button');
    outside.textContent = 'Hintergrund';
    document.body.append(outside);
    const panel = renderPanel({ overlay: true });
    const dialog = screen.getByRole('dialog', { name: 'Einstellungen' });
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    expect(tab('Allgemein')).toHaveFocus();
    outside.focus();
    expect(dialog.contains(document.activeElement)).toBe(true);
    // Shift+Tab from the first control wraps to the last one inside the dialog.
    const close = screen.getByRole('button', { name: 'Einstellungen schließen' });
    close.focus();
    fireEvent.keyDown(close, { key: 'Tab', shiftKey: true });
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(document.activeElement).not.toBe(close);
    panel.unmountAll();
    expect(trigger).toHaveFocus();
    trigger.remove(); outside.remove();
  });

  it('as an overlay Escape closes the topmost layer first: a confirmation, then the panel', () => {
    const { props } = renderPanel({ overlay: true });
    fireEvent.click(tab('Wartung'));
    fireEvent.click(screen.getByRole('button', { name: 'Katalog zurücksetzen …' }));
    expect(screen.getAllByRole('dialog')).toHaveLength(2);
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(screen.getAllByRole('dialog')).toHaveLength(1);
    expect(props.onClose).not.toHaveBeenCalled();
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(props.onClose).toHaveBeenCalledTimes(1);
  });

  it('as an overlay the background cannot be clicked', () => {
    renderPanel({ overlay: true });
    const outside = document.createElement('button');
    const onClick = vi.fn();
    outside.addEventListener('click', onClick);
    document.body.append(outside);
    fireEvent.click(outside);
    expect(onClick).not.toHaveBeenCalled();
    outside.remove();
  });
});
