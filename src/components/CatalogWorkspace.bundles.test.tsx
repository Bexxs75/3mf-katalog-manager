import { type ComponentProps } from 'react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
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
  return render(<LanguageProvider><UiDensityProvider><CatalogWorkspace {...props} /></UiDensityProvider></LanguageProvider>);
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
