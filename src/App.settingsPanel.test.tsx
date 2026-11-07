import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { de } from './i18n/de';
import { makeModelFileSummary } from './test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  localStorage.setItem('3mf-katalog-view', 'grid');
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd) => {
    if (cmd === 'get_app_version') return '0.15.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    if (cmd === 'list_file_summaries') return [makeModelFileSummary({ id: 'a', name: 'Alpha.stl' }), makeModelFileSummary({ id: 'b', name: 'Beta.stl' })];
    return [];
  });
});

async function setup() {
  return await act(async () => render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>));
}
const gear = () => screen.getByRole('button', { name: de.settingsTitle });
// Docked it is a region next to the catalog; laid over a view it is a modal window.
const settings = () => screen.queryByRole('complementary', { name: de.settingsTitle }) ?? screen.queryByRole('dialog', { name: de.settingsTitle });
const details = () => screen.queryByRole('complementary', { name: de.detailPanelTitle });
const tile = (id: string) => document.querySelector<HTMLElement>(`[data-model-id="${id}"]`)!;

describe('settings in the right-hand panel', () => {
  it('opens in the detail panel place and hides the details; the gear closes it again with the same model', async () => {
    await setup();
    fireEvent.click(tile('a'));
    expect(details()).toBeVisible();
    fireEvent.click(gear());
    expect(settings()).toBeVisible();
    expect(details()).toBeNull();
    // Models, folders and the rail stay uncovered: the panel is a layout sibling, not an overlay.
    expect(settings()).not.toHaveClass('settings-panel-overlay');
    expect(tile('a')).toBeInTheDocument();
    fireEvent.click(gear());
    expect(settings()).toBeNull();
    expect(within(details()!).getByText('Alpha.stl')).toBeVisible();
  });

  it('closes with the cross and with Escape, and returns focus to the gear', async () => {
    await setup();
    fireEvent.click(tile('a'));
    fireEvent.click(gear());
    fireEvent.click(screen.getByRole('button', { name: de.settingsClose }));
    expect(settings()).toBeNull();
    expect(gear()).toHaveFocus();
    expect(within(details()!).getByText('Alpha.stl')).toBeVisible();

    fireEvent.click(gear());
    expect(screen.getByRole('tab', { name: de.settingsTabGeneral })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole('tab', { name: de.settingsTabGeneral }), { key: 'Escape' });
    expect(settings()).toBeNull();
    expect(gear()).toHaveFocus();
    expect(within(details()!).getByText('Alpha.stl')).toBeVisible();
  });

  it('keeps the selection when Escape closes the settings', async () => {
    await setup();
    fireEvent.click(tile('a'));
    fireEvent.click(gear());
    await act(async () => { fireEvent.keyDown(window, { key: 'Escape' }); });
    expect(settings()).toBeNull();
    // The catalog's own Escape must not also clear the selection.
    expect(within(details()!).getByText('Alpha.stl')).toBeVisible();
  });

  it('keeps the settings open when a model is clicked and shows its details after closing', async () => {
    await setup();
    fireEvent.click(tile('a'));
    fireEvent.click(gear());
    fireEvent.click(tile('b'));
    expect(settings()).toBeVisible();
    expect(details()).toBeNull();
    fireEvent.click(gear());
    expect(within(details()!).getByText('Beta.stl')).toBeVisible();
  });

  it('opens only the settings when the detail panel is off and leaves it off afterwards', async () => {
    await setup();
    expect(details()).toBeNull();
    fireEvent.click(gear());
    expect(settings()).toBeVisible();
    expect(details()).toBeNull();
    fireEvent.click(gear());
    expect(settings()).toBeNull();
    expect(details()).toBeNull();
  });

  it('replaces a pinned detail panel and brings it back on close', async () => {
    localStorage.setItem('3mf-katalog-detail-panel', 'pinned');
    await setup();
    expect(details()).toBeVisible();
    fireEvent.click(gear());
    expect(details()).toBeNull();
    expect(settings()).toBeVisible();
    fireEvent.click(gear());
    expect(details()).toBeVisible();
  });

  it('switches tabs inside the panel and keeps every General section', async () => {
    await setup();
    fireEvent.click(gear());
    expect(screen.getByRole('heading', { name: de.settingsSecAppearance })).toBeVisible();
    expect(screen.getByRole('heading', { name: de.settingsSecControls })).toBeVisible();
    fireEvent.click(screen.getByRole('tab', { name: de.settingsTabCatalog }));
    expect(screen.getByRole('tab', { name: de.settingsTabCatalog })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('heading', { name: de.settingsSecSources })).toBeVisible();
    expect(screen.getByRole('heading', { name: de.settingsSecSync })).toBeVisible();
  });

  it('Escape in an open dialog closes only the dialog', async () => {
    await setup();
    fireEvent.click(gear());
    const tips = within(settings()!).getByRole('button', { name: de.keyboardTipsTitle });
    tips.focus(); fireEvent.click(tips);
    const dialog = screen.getByRole('dialog', { name: de.keyboardTipsTitle });
    fireEvent.keyDown(within(dialog).getByRole('tab', { name: de.keyboardTab }), { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(settings()).toBeVisible();
  });

  it('lies over the Printer Manager as an overlay with the same panel', async () => {
    await setup();
    fireEvent.click(screen.getByRole('button', { name: de.railPrinters }));
    fireEvent.click(gear());
    expect(settings()).toBeVisible();
    expect(settings()).toHaveClass('settings-panel-overlay');
    expect(screen.getByRole('dialog', { name: de.settingsTitle })).toHaveAttribute('aria-modal', 'true');
    fireEvent.click(screen.getByRole('button', { name: de.settingsClose }));
    expect(settings()).toBeNull();
    expect(gear()).toHaveFocus();
  });

  it('keeps the panel open across views and offers the Printer Manager shortcut', async () => {
    await setup();
    fireEvent.click(gear());
    fireEvent.click(screen.getByRole('button', { name: de.railPrinters }));
    expect(settings()).toHaveClass('settings-panel-overlay');
    // As a modal window the overlay keeps the rail out of reach.
    fireEvent.click(screen.getByRole('button', { name: de.railCatalog }));
    expect(settings()).toHaveClass('settings-panel-overlay');
    fireEvent.click(screen.getByRole('button', { name: de.settingsClose }));
    fireEvent.click(screen.getByRole('button', { name: de.railCatalog }));
    fireEvent.click(gear());
    expect(settings()).not.toHaveClass('settings-panel-overlay');
    fireEvent.click(screen.getByRole('tab', { name: de.settingsTabServices }));
    fireEvent.click(screen.getByRole('button', { name: de.pmOpen }));
    expect(settings()).toBeNull();
  });

  it('closes only the settings with Escape when the detail view is open underneath', async () => {
    await setup();
    await act(async () => { fireEvent.doubleClick(tile('a')); });
    const previous = () => screen.queryByRole('button', { name: de.detailPrevious });
    expect(previous()).toBeInTheDocument();
    fireEvent.click(gear());
    expect(settings()).toBeVisible();
    fireEvent.keyDown(screen.getByRole('tab', { name: de.settingsTabGeneral }), { key: 'Escape' });
    expect(settings()).toBeNull();
    expect(previous()).toBeInTheDocument();
    // With the settings gone, Escape closes the detail view as before.
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(previous()).toBeNull();
  });

  it('opens on the tab used last in this session, and on General the first time', async () => {
    await setup();
    fireEvent.click(gear());
    expect(screen.getByRole('tab', { name: de.settingsTabGeneral })).toHaveAttribute('aria-selected', 'true');
    fireEvent.click(screen.getByRole('tab', { name: de.settingsTabMaintenance }));
    fireEvent.click(screen.getByRole('button', { name: de.settingsClose }));
    fireEvent.click(gear());
    expect(screen.getByRole('tab', { name: de.settingsTabMaintenance })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: de.settingsTabMaintenance })).toHaveFocus();
  });

  it('returns the focus to the gear that opened the docked panel', async () => {
    await setup();
    gear().focus();
    fireEvent.click(gear());
    fireEvent.click(screen.getByRole('button', { name: de.settingsClose }));
    expect(gear()).toHaveFocus();
  });
});

describe('direct entries into the settings', () => {
  it('leads the source-folder check without folders to the source folders section', async () => {
    await setup();
    fireEvent.click(screen.getByRole('button', { name: de.import }));
    await act(async () => { fireEvent.click(screen.getByRole('menuitem', { name: de.sfMenuCheck })); });
    expect(await screen.findByRole('tab', { name: de.settingsTabCatalog, selected: true })).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 3, name: de.settingsSecSources })).toHaveFocus();
  });

  it('leads "open in slicer" without a configured slicer to the installed slicers', async () => {
    await setup();
    fireEvent.contextMenu(tile('a'));
    fireEvent.click((await screen.findAllByRole('button', { name: de.openInSlicer })).slice(-1)[0]);
    expect(await screen.findByRole('tab', { name: de.settingsTabServices, selected: true })).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 3, name: de.settingsSecSlicers })).toHaveFocus();
  });

  it('does not move the panel again when it is opened later without a request', async () => {
    await setup();
    fireEvent.contextMenu(tile('a'));
    fireEvent.click((await screen.findAllByRole('button', { name: de.openInSlicer })).slice(-1)[0]);
    await screen.findByRole('heading', { level: 3, name: de.settingsSecSlicers });
    fireEvent.click(screen.getByRole('tab', { name: de.settingsTabInfo }));
    fireEvent.click(screen.getByRole('button', { name: de.settingsClose }));
    fireEvent.click(gear());
    expect(screen.getByRole('tab', { name: de.settingsTabInfo })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: de.settingsTabInfo })).toHaveFocus();
  });

  it('leads the Thingiverse area to the Thingiverse section', async () => {
    localStorage.setItem('3mf-katalog-view', 'grid');
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'get_thingiverse_settings') return { enabled: true, hasToken: false, tokenMask: null, usageToday: 0, dailyLimit: 1000 };
      if (cmd === 'get_app_version') return '0.15.0';
      if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
      if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
      return [];
    });
    await setup();
    fireEvent.click(await screen.findByRole('button', { name: de.tvRail }));
    fireEvent.click(await screen.findByRole('button', { name: de.tvOpenSettings }));
    expect(await screen.findByRole('tab', { name: de.settingsTabServices, selected: true })).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 3, name: new RegExp(de.settingsSecThingiverse) })).toHaveFocus();
  });
});
