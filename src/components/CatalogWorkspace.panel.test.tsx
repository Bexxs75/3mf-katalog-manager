import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';
import { useState, type ComponentProps } from 'react';
import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { CatalogWorkspace } from './CatalogWorkspace';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile } from '../test/factories';
vi.mock('./Sidebar', () => ({ Sidebar: () => null }));
vi.mock('./ModelPreview', () => ({ ModelPreview: () => null }));
vi.mock('../lib/api/filamentCheck', () => ({ checkFilament: vi.fn().mockResolvedValue([]) }));
vi.mock('../lib/api/lastPrinter', () => ({ getLastPrinterForFile: vi.fn().mockResolvedValue(null) }));
beforeEach(() => { localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); });
function setup(extra: Partial<ComponentProps<typeof CatalogWorkspace>> = {}) {
  const file = makeModelFile({ id: 'one', name: 'Cube', materials: [{ name: 'PLA', displayColor: null }] });
  function Fixture() {
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const [bulk, setBulk] = useState(new Set<string>());
    useKeyboardShortcuts({ filteredIds: ['one'], selectedId, selectModel: setSelectedId, hasBulkSelection: bulk.size > 0, openBulkDeleteConfirm: vi.fn(), navigationEnabled: true, toggleBulkSelect: vi.fn() });
    const props = { query: '', activeFolderId: 'all', slicers: [], queue: [], models: [file], filtered: [file],
      collectionModels: [], folders: [], selectedForBulk: bulk, detailModel: null, selected: selectedId ? file : null,
      selectedId, selectModel: setSelectedId, onCloseDetails: () => setSelectedId(null), activeCollection: null,
      activeTag: null, toolView: null, view: 'grid', sort: 'name', tags: [], collections: [],
      sidebarWidth: { width: 242 }, collapsedFolders: { isCollapsed: () => false, toggle: vi.fn() },
      displayPreference: 'thumbnail', toggleFavorite: vi.fn(), setContextMenu: vi.fn(),
      clearBulkSelection: () => setBulk(new Set()), toggleBulkSelect: () => setBulk(new Set(['one'])),
      onAddTag: vi.fn(), onRemoveTag: vi.fn(), addTag: vi.fn(), removeTag: vi.fn(),
      ...extra } as unknown as ComponentProps<typeof CatalogWorkspace>;
    return <LanguageProvider><UiDensityProvider><CatalogWorkspace {...props} /></UiDensityProvider></LanguageProvider>;
  }
  return render(<Fixture />);
}
it('auto appears on selection, same card stays open, cross closes and restores focus', () => {
  const { container } = setup();
  expect(screen.queryByRole('complementary')).toBeNull();
  fireEvent.click(screen.getByText('Cube'));
  expect(screen.getByRole('complementary', { name: 'Details' })).toBeVisible();
  fireEvent.click(screen.getByText('Cube', { selector: '[data-model-id] *' }));
  expect(screen.getByRole('complementary')).toBeVisible();
  const close = screen.getByRole('button', { name: 'Details schließen' });
  expect(close.title).toContain('Esc');
  fireEvent.click(close);
  expect(screen.queryByRole('complementary')).toBeNull();
  expect(container.querySelector('[data-model-id="one"]')).toHaveFocus();
});
it('Esc and free area close the selection', () => {
  const { container } = setup();
  fireEvent.click(screen.getByText('Cube'));
  fireEvent.keyDown(window, { key: 'Escape' });
  expect(screen.queryByRole('complementary')).toBeNull();
  fireEvent.click(screen.getByText('Cube'));
  fireEvent.click(container.querySelector('[data-catalog-scroller]')!);
  expect(screen.queryByRole('complementary')).toBeNull();
});
it('gives dialogs, menus, and text entry priority', () => {
  setup(); fireEvent.click(screen.getByText('Cube'));
  const dialog = document.createElement('div'); dialog.setAttribute('role', 'dialog'); dialog.setAttribute('aria-modal', 'true'); document.body.append(dialog);
  fireEvent.keyDown(window, { key: 'Escape' }); expect(screen.getByRole('complementary')).toBeVisible(); dialog.remove();
  const menu = document.createElement('div'); menu.setAttribute('role', 'menu'); document.body.append(menu);
  fireEvent.keyDown(window, { key: 'Escape' }); expect(screen.getByRole('complementary')).toBeVisible(); menu.remove();
  fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Escape' }); expect(screen.getByRole('complementary')).toBeVisible();
});
it('clears bulk selection before closing details', () => {
  setup(); fireEvent.click(screen.getByText('Cube'));
  fireEvent.click(screen.getByRole('checkbox'));
  fireEvent.keyDown(window, { key: 'Escape' });
  expect(screen.getByRole('complementary')).toBeVisible();
  fireEvent.keyDown(window, { key: 'Escape' });
  expect(screen.queryByRole('complementary')).toBeNull();
});
it('pinned always shows, has no cross, and stays open on Esc and blank clicks', () => {
  const { container } = setup({ detailPanel: 'pinned' });
  expect(screen.getByRole('complementary')).toBeVisible();
  expect(screen.queryByRole('button', { name: 'Details schließen' })).toBeNull();
  fireEvent.click(screen.getByText('Cube'));
  fireEvent.keyDown(window, { key: 'Escape' }); fireEvent.click(container.querySelector('[data-catalog-scroller]')!);
  expect(screen.getByRole('complementary')).toBeVisible();
});
it('uses the window media query for overlay and drops duplicate material metadata', () => {
  const original = window.matchMedia;
  window.matchMedia = vi.fn(query => ({ matches: query === '(max-width: 999px)', addEventListener: vi.fn(), removeEventListener: vi.fn() }) as unknown as MediaQueryList);
  try {
    setup(); fireEvent.click(screen.getByText('Cube'));
    expect(screen.getByRole('complementary')).toHaveClass('detail-panel-overlay');
    expect(screen.queryByText('Material')).toBeNull();
    expect(screen.getByText('PLA')).toBeVisible();
  } finally { window.matchMedia = original; }
});

it('arrow selection opens details and keeps focus on the model', () => {
  const { container } = setup();
  fireEvent.keyDown(window, { key: 'ArrowRight' });
  expect(screen.getByRole('complementary')).toBeVisible();
  expect(container.querySelector('[data-model-id="one"]')).toHaveFocus();
});
it.each(['compact', 'comfort'])('shows material only in badges at density %s', density => {
  localStorage.setItem('3mf-katalog-density', density);
  setup(); fireEvent.click(screen.getByText('Cube'));
  expect(screen.queryByText('Material')).toBeNull();
  expect(screen.getByText('PLA')).toBeVisible();
});
it.each(['groupedGrid', 'groupedList'] as const)('does not close when clicking a group header in %s', view => {
  setup({ view }); fireEvent.click(screen.getByText('Cube'));
  fireEvent.click(screen.getByText('Ohne Ordner'));
  expect(screen.getByRole('complementary')).toBeVisible();
});

it('responds to window width changes while open', () => {
  const original = window.matchMedia;
  let change!: () => void;
  const media = { matches: false, addEventListener: vi.fn((_name, cb) => { change = cb; }), removeEventListener: vi.fn() };
  window.matchMedia = vi.fn(() => media as unknown as MediaQueryList);
  try {
    setup(); fireEvent.click(screen.getByText('Cube'));
    expect(screen.getByRole('complementary')).not.toHaveClass('detail-panel-overlay');
    media.matches = true;
    fireEvent(window, new Event('resize'));
    // matchMedia, rather than grid measurements, owns this breakpoint.
    act(() => change());
    expect(screen.getByRole('complementary')).toHaveClass('detail-panel-overlay');
    media.matches = false; act(() => change());
    expect(screen.getByRole('complementary')).not.toHaveClass('detail-panel-overlay');
  } finally { window.matchMedia = original; }
});
