import { type ComponentProps } from 'react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { CatalogWorkspace } from './CatalogWorkspace';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile } from '../test/factories';
import type { Bundle } from '../types';
vi.mock('./Sidebar', () => ({ Sidebar: () => null }));
vi.mock('./ModelPreview', () => ({ ModelPreview: () => null }));
vi.mock('../lib/api/filamentCheck', () => ({ checkFilament: vi.fn().mockResolvedValue([]) }));
vi.mock('../lib/api/lastPrinter', () => ({ getLastPrinterForFile: vi.fn().mockResolvedValue(null) }));
vi.mock('../lib/api/files', () => ({ listFileImages: vi.fn().mockResolvedValue([]) }));

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    width: 800, height: 400, top: 0, left: 0, right: 800, bottom: 400, x: 0, y: 0, toJSON: () => ({}),
  });
  localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de');
});
afterEach(() => vi.restoreAllMocks());

const files = [
  makeModelFile({ id: 'a', name: 'Alpha.stl', fileSizeBytes: 10 }),
  makeModelFile({ id: 'b', name: 'Beta.stl', fileSizeBytes: 20 }),
  makeModelFile({ id: 'c', name: 'Gamma.stl', fileSizeBytes: 30 }),
];
const bundle: Bundle = {
  id: 'b1', name: 'Zeta set', kind: 'variants', createdAt: '2026-11-01T00:00:00Z',
  members: [{ fileId: 'a', position: 0, label: null, inTrash: false }, { fileId: 'b', position: 1, label: null, inTrash: false }],
};

function setup(extra: Partial<ComponentProps<typeof CatalogWorkspace>> = {}) {
  const props = { query: '', activeFolderId: 'all', slicers: [], queue: [], models: files, filtered: files,
    collectionModels: [], folders: [], selectedForBulk: new Set<string>(), detailModel: null, selected: null,
    selectedId: null, selectModel: vi.fn(), onCloseDetails: vi.fn(), activeCollection: null,
    activeTag: null, toolView: null, view: 'grid', sort: 'name', tags: [], collections: [],
    sidebarWidth: { width: 242 }, collapsedFolders: { isCollapsed: () => false, toggle: vi.fn() },
    displayPreference: 'thumbnail', toggleFavorite: vi.fn(), setContextMenu: vi.fn(),
    selectAllVisible: vi.fn(), clearBulkSelection: vi.fn(), toggleBulkSelect: vi.fn(),
    onAddTag: vi.fn(), onRemoveTag: vi.fn(), addTag: vi.fn(), removeTag: vi.fn(),
    setUsage: vi.fn(),
    ...extra } as unknown as ComponentProps<typeof CatalogWorkspace>;
  const wrap = (next: Partial<ComponentProps<typeof CatalogWorkspace>>) => <LanguageProvider><UiDensityProvider><CatalogWorkspace {...props} {...next} /></UiDensityProvider></LanguageProvider>;
  const result = render(wrap({}));
  return { ...result, update: (next: Partial<ComponentProps<typeof CatalogWorkspace>>) => result.rerender(wrap(next)) };
}
const cardIds = () => [...document.querySelectorAll('[role="row"][data-model-id]')].map(row => row.getAttribute('data-model-id'));

it('offers no switch while the catalog has no bundle', () => {
  setup({ setCollapseBundles: vi.fn() });
  expect(screen.queryByRole('switch', { name: 'Bündel als eine Karte zeigen' })).toBeNull();
});

it('offers the switch once bundles exist (off while the caller says so), and reports the choice', () => {
  const setCollapse = vi.fn();
  setup({ bundles: [bundle], setCollapseBundles: setCollapse });
  const toggle = screen.getByRole('switch', { name: 'Bündel als eine Karte zeigen' });
  expect(toggle).toHaveAttribute('aria-checked', 'false');
  fireEvent.click(toggle);
  expect(setCollapse).toHaveBeenCalledWith(true);
});

it('keeps the switch while it is on, so the way back stays', () => {
  setup({ bundles: [], collapseBundles: true, setCollapseBundles: vi.fn() });
  expect(screen.getByRole('switch', { name: 'Bündel als eine Karte zeigen' })).toHaveAttribute('aria-checked', 'true');
});

it('lists every file on its own while the switch is off', () => {
  setup({ bundles: [bundle], collapseBundles: false, setCollapseBundles: vi.fn() });
  expect(cardIds()).toEqual(['a', 'b', 'c']);
});

it('shows a bundle as one card and sorts it by the bundle name among the files', () => {
  setup({ bundles: [bundle], collapseBundles: true, setCollapseBundles: vi.fn(), sortDirection: 'asc' });
  // "Zeta set" sorts after "Gamma.stl", although its first member would come first by file name.
  expect(cardIds()).toEqual(['c', 'a']);
  expect(screen.getByRole('row', { name: 'Bündel Zeta set, 2 Varianten' })).toBeInTheDocument();
});

it('sorts bundle cards by the summed size, the largest first', () => {
  const sized = [files[0], files[1], { ...files[2], fileSizeBytes: 25 }];
  setup({ models: sized, filtered: sized, bundles: [bundle], collapseBundles: true, setCollapseBundles: vi.fn(), sort: 'size', sortDirection: 'desc' });
  // The bundle weighs 10 + 20 bytes, more than Gamma alone.
  expect(cardIds()).toEqual(['a', 'c']);
});

it('sorts bundle cards by the summed size, the smallest first', () => {
  const sized = [files[0], files[1], { ...files[2], fileSizeBytes: 25 }];
  setup({ models: sized, filtered: sized, bundles: [bundle], collapseBundles: true, setCollapseBundles: vi.fn(), sort: 'size', sortDirection: 'asc' });
  expect(cardIds()).toEqual(['c', 'a']);
});

it('leaves files single in a tool view, which has an order of its own', () => {
  setup({ bundles: [bundle], collapseBundles: true, setCollapseBundles: vi.fn(), toolView: 'recent' });
  expect(cardIds()).toEqual(['a', 'b', 'c']);
});

const view = () => ({ onEdit: vi.fn(), onDissolve: vi.fn(), onRemoveMember: vi.fn(), queueAskBundleId: null, setQueueAskBundleId: vi.fn() });

it('shows the bundle panel instead of the detail panel when a bundle card is open', () => {
  setup({ bundles: [bundle], collapseBundles: true, setCollapseBundles: vi.fn(), bundleView: view(), selectedId: 'a', selected: files[0] });
  expect(screen.getByRole('heading', { level: 2, name: 'Zeta set' })).toBeInTheDocument();
  expect(screen.getByRole('list', { name: 'Mitglieder' })).toBeInTheDocument();
});

it('also shows it when another visible member than the title is the open one', () => {
  setup({ bundles: [bundle], collapseBundles: true, setCollapseBundles: vi.fn(), bundleView: view(), selectedId: 'b', selected: files[1] });
  expect(screen.getByRole('heading', { level: 2, name: 'Zeta set' })).toBeInTheDocument();
});

it('keeps the detail panel for a single card, with the switch off, and in tool views', () => {
  const common = { bundles: [bundle], setCollapseBundles: vi.fn(), bundleView: view() };
  const { unmount } = setup({ ...common, collapseBundles: true, selectedId: 'c', selected: files[2] });
  expect(screen.queryByRole('list', { name: 'Mitglieder' })).toBeNull();
  unmount();
  const off = setup({ ...common, collapseBundles: false, selectedId: 'a', selected: files[0] });
  expect(screen.queryByRole('list', { name: 'Mitglieder' })).toBeNull();
  off.unmount();
  setup({ ...common, collapseBundles: true, toolView: 'recent', selectedId: 'a', selected: files[0] });
  expect(screen.queryByRole('list', { name: 'Mitglieder' })).toBeNull();
});

it('keeps the detail panel when the bundle shrinks to one visible member under a filter', () => {
  setup({ bundles: [bundle], collapseBundles: true, setCollapseBundles: vi.fn(), bundleView: view(), models: files, filtered: [files[0], files[2]], selectedId: 'a', selected: files[0] });
  expect(screen.queryByRole('list', { name: 'Mitglieder' })).toBeNull();
});

it('opens a member of the panel in the detail page', () => {
  const setDetailModelId = vi.fn();
  setup({ bundles: [bundle], collapseBundles: true, setCollapseBundles: vi.fn(), bundleView: view(), selectedId: 'a', selected: files[0], setDetailModelId });
  fireEvent.click(screen.getByRole('button', { name: /Beta.stl/ }));
  expect(setDetailModelId).toHaveBeenCalledWith('b');
});

it('applies "mark as printed" to the members that are not printed, through the single-file actions', () => {
  const togglePrintStatus = vi.fn();
  const mixed = [makeModelFile({ id: 'a', name: 'Alpha.stl', printStatus: 'printed' }), files[1], files[2]];
  setup({ bundles: [bundle], collapseBundles: true, setCollapseBundles: vi.fn(), bundleView: view(), models: mixed, filtered: mixed, selectedId: 'a', selected: mixed[0], togglePrintStatus });
  fireEvent.click(screen.getByRole('button', { name: 'Alle 2 als gedruckt markieren' }));
  expect(togglePrintStatus.mock.calls).toEqual([['b']]);
});

it('adds the switch and the section to the detail page of a member', () => {
  const setDetailModelId = vi.fn();
  setup({ bundles: [bundle], collapseBundles: true, bundleView: view(), detailModel: files[0], selectedId: 'a', selected: files[0], setDetailModelId });
  expect(screen.getByRole('group', { name: 'Variante wählen' })).toBeInTheDocument();
  expect(screen.getByText('Teil von „Zeta set“ · Varianten')).toBeInTheDocument();
  fireEvent.click(within(screen.getByRole('group', { name: 'Variante wählen' })).getByRole('button', { name: 'Beta.stl' }));
  expect(setDetailModelId).toHaveBeenCalledWith('b');
});

it('shows no bundle elements on the detail page of a file outside any bundle', () => {
  setup({ bundles: [bundle], collapseBundles: true, bundleView: view(), detailModel: files[2], selectedId: 'c', selected: files[2] });
  expect(screen.queryByRole('group', { name: 'Variante wählen' })).toBeNull();
});

it('keeps a stored width through model and bundle switches and removes it on reset', () => {
  localStorage.setItem('3mf-katalog-detail-width', '440');
  const { update } = setup({ bundles: [bundle], collapseBundles: true, bundleView: view(), selectedId: 'c', selected: files[2] });
  const shellWidth = () => screen.getByRole('complementary', { name: 'Details' }).style.getPropertyValue('--detail-panel-width');
  expect(shellWidth()).toBe('440px');
  update({ selectedId: 'a', selected: files[0] });
  expect(screen.getByRole('list', { name: 'Mitglieder' })).toBeInTheDocument();
  expect(shellWidth()).toBe('440px');
  fireEvent.keyDown(screen.getByRole('separator'), { key: 'ArrowLeft' });
  update({ selectedId: 'c', selected: files[2] });
  expect(screen.queryByRole('list', { name: 'Mitglieder' })).toBeNull();
  expect(shellWidth()).toBe('456px');
  fireEvent.doubleClick(screen.getByRole('separator'));
  expect(localStorage.getItem('3mf-katalog-detail-width')).toBeNull();
  expect(screen.getByRole('complementary', { name: 'Details' })).not.toHaveClass('detail-panel-resized');
});
