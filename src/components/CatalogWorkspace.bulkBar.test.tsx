import { useState, type ComponentProps } from 'react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { CatalogWorkspace } from './CatalogWorkspace';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile } from '../test/factories';
import { openBulkMore } from '../test/bulkBar';
vi.mock('./Sidebar', () => ({ Sidebar: () => null }));
vi.mock('./ModelPreview', () => ({ ModelPreview: () => null }));
vi.mock('../lib/api/filamentCheck', () => ({ checkFilament: vi.fn().mockResolvedValue([]) }));
vi.mock('../lib/api/lastPrinter', () => ({ getLastPrinterForFile: vi.fn().mockResolvedValue(null) }));

const originalMatchMedia = window.matchMedia;
beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    width: 800, height: 400, top: 0, left: 0, right: 800, bottom: 400, x: 0, y: 0, toJSON: () => ({}),
  });
  localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de');
});
afterEach(() => { window.matchMedia = originalMatchMedia; vi.restoreAllMocks(); });

function narrowWindow() {
  window.matchMedia = vi.fn(query => ({ matches: query === '(max-width: 999px)', addEventListener: vi.fn(), removeEventListener: vi.fn() }) as unknown as MediaQueryList);
}

function setup(extra: Partial<ComponentProps<typeof CatalogWorkspace>> = {}) {
  const file = makeModelFile({ id: 'one', name: 'Cube' });
  const selectAllVisible = vi.fn();
  function Fixture() {
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const [bulk, setBulk] = useState(new Set<string>());
    const props = { query: '', activeFolderId: 'all', slicers: [], queue: [], models: [file], filtered: [file],
      collectionModels: [], folders: [], selectedForBulk: bulk, detailModel: null, selected: selectedId ? file : null,
      selectedId, selectModel: setSelectedId, onCloseDetails: () => setSelectedId(null), activeCollection: null,
      activeTag: null, toolView: null, view: 'grid', sort: 'name', tags: [], collections: [],
      sidebarWidth: { width: 242 }, collapsedFolders: { isCollapsed: () => false, toggle: vi.fn() },
      displayPreference: 'thumbnail', toggleFavorite: vi.fn(), setContextMenu: vi.fn(),
      selectAllVisible: () => { selectAllVisible(); setBulk(new Set(['one'])); },
      clearBulkSelection: () => setBulk(new Set()), toggleBulkSelect: () => setBulk(new Set(['one'])),
      onAddTag: vi.fn(), onRemoveTag: vi.fn(), addTag: vi.fn(), removeTag: vi.fn(),
      ...extra } as unknown as ComponentProps<typeof CatalogWorkspace>;
    return <LanguageProvider><UiDensityProvider><CatalogWorkspace {...props} /></UiDensityProvider></LanguageProvider>;
  }
  return { ...render(<Fixture />), selectAllVisible };
}

it('shows the selection bar before anything is selected and "Alle auswählen" selects without a prior selection', () => {
  const { selectAllVisible } = setup();
  const bar = screen.getByRole('toolbar', { name: 'Aktionen für die Auswahl' });
  expect(within(bar).getByText('0 ausgewählt')).toBeVisible();
  fireEvent.click(within(bar).getByRole('button', { name: 'Alle auswählen' }));
  expect(selectAllVisible).toHaveBeenCalledTimes(1);
  expect(within(screen.getByRole('toolbar')).getByText('1 ausgewählt')).toBeVisible();
});

it('keeps the selection bar visible while the detail panel is open', () => {
  setup();
  fireEvent.click(screen.getByText('Cube'));
  expect(screen.getByRole('complementary', { name: 'Details' })).toBeVisible();
  expect(screen.getByRole('toolbar', { name: 'Aktionen für die Auswahl' })).toBeVisible();
});

it('starts the floating detail panel below the bar in a narrow window so the bar is not covered', () => {
  narrowWindow();
  vi.spyOn(HTMLElement.prototype, 'offsetTop', 'get').mockReturnValue(100);
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(48);
  const { container } = setup();
  fireEvent.click(screen.getByText('Cube'));
  expect(screen.getByRole('complementary', { name: 'Details' })).toHaveClass('detail-panel-overlay');
  expect((container.firstElementChild as HTMLElement).style.getPropertyValue('--detail-overlay-top')).toBe('148px');
  // One row: the lesser used actions moved into the menu.
  const bar = screen.getByRole('toolbar');
  expect(within(bar).queryByRole('button', { name: 'Gedruckt' })).toBeNull();
  fireEvent.click(within(bar).getByRole('button', { name: 'Alle auswählen' }));
  const menu = openBulkMore();
  expect(within(menu).getByRole('menuitem', { name: 'Gedruckt' })).toBeVisible();
});

it('hides the bar when there is nothing to select', () => {
  setup({ models: [], filtered: [] });
  expect(screen.queryByRole('toolbar')).toBeNull();
});
