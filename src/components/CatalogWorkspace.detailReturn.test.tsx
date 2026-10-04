import { useState, type ComponentProps } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CatalogWorkspace } from './CatalogWorkspace';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import type { ModelFile, ViewMode } from '../types';

vi.mock('./Sidebar', () => ({ Sidebar: () => null }));
vi.mock('./DetailPanel', () => ({ DetailPanel: () => null }));
vi.mock('./ModelDetailPage', () => ({ ModelDetailPage: ({ onClose }: { onClose: () => void }) => <button onClick={onClose}>Zurück</button> }));
beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));
const model: ModelFile = {
  id: '1', name: 'Zahnrad.3mf', path: '/tmp/Zahnrad.3mf', folderId: '', tags: [],
  origin: 'local', sync: 'local-only', dimensionsMm: null, volumeCm3: null,
  objectCount: null, plateCount: null, materials: [], fileSizeBytes: 100,
  fileModifiedAt: null, importedAt: '2026-10-01T00:00:00Z', printStatus: 'not_printed', estimatedWeightG: null,
  weightSource: 'estimated', sliceInfo: null, costEstimate: null, lastViewedAt: null,
  contentHash: null, creator: null, customImage: null, thumbnailImage: null,
  renderSnapshotImage: null, sourceUrl: null, queuePosition: null, favorite: false, deletedAt: null,
};
const props = {
  setQuery: vi.fn(),
  setActiveCollection: vi.fn(),
  setCollectionsGalleryOpen: vi.fn(),
  reorderQueue: vi.fn(),
  removeFromQueue: vi.fn(),
  selectModel: vi.fn(),
  setActiveFolderId: vi.fn(),
  onCreateFolder: vi.fn(),
  handleFolderMouseEnter: vi.fn(),
  handleFolderMouseLeave: vi.fn(),
  onDragFolderStart: vi.fn(),
  setActiveTag: vi.fn(),
  createCollection: vi.fn(),
  renameCollection: vi.fn(),
  deleteCollection: vi.fn(),
  setConfirmBulkDelete: vi.fn(),
  bulkDelete: vi.fn(),
  bulkRemove: vi.fn(),
  onCatalogRemoved: vi.fn(),
  selectAllVisible: vi.fn(),
  clearBulkSelection: vi.fn(),
  bulkAddToQueue: vi.fn(),
  setAddToCollectionMenuOpen: vi.fn(),
  bulkAddToCollection: vi.fn(),
  bulkRemoveFromCollection: vi.fn(),
  bulkSetPrintStatus: vi.fn(),
  setAddTagMenuOpen: vi.fn(),
  setTagDraft: vi.fn(),
  bulkAddTag: vi.fn(),
  setRemoveTagMenuOpen: vi.fn(),
  bulkRemoveTag: vi.fn(),
  setDetailModelId: vi.fn(),
  addTag: vi.fn(),
  removeTag: vi.fn(),
  deleteModel: vi.fn(),
  togglePrintStatus: vi.fn(),
  toggleFavorite: vi.fn(),
  addToQueue: vi.fn(),
  uploadCustomImage: vi.fn(),
  captureRenderSnapshot: vi.fn(),
  setModelSourceUrl: vi.fn(),
  openInSlicer: vi.fn(),
  rescanMetadata: vi.fn(),
  addModelToCollection: vi.fn(),
  setContextMenu: vi.fn(),
  toggleBulkSelect: vi.fn(),
  reorderCollection: vi.fn(),
  onDragFileStart: vi.fn(),
  setToolView: vi.fn(),
  onOpenCleanup: vi.fn(),
  query: '', queue: [], folders: [], models: [model], activeFolderId: 'all',
  dragOverFolderId: null, draggedFolderId: null, tags: [], activeTag: null, collections: [],
  activeCollection: null, collectionsGalleryOpen: false, detailModel: null,
  selectedForBulk: new Set(), confirmBulkDelete: false, addToCollectionMenuOpen: false,
  addTagMenuOpen: false, tagDraft: '', removeTagMenuOpen: false, tagsInSelection: [],
  slicers: [], slicerError: null, rescanFeedback: null, displayPreference: 'thumbnail',
  view: 'grid', sort: 'name', filtered: [model], collectionModels: [], selectedId: null,
  selected: null, collapsedFolders: { isCollapsed: () => false, toggle: vi.fn(), collapseAll: vi.fn(), expandAll: vi.fn(), collapsedCount: 0 },
  toolView: null, cleanupScanning: false, cleanupError: null,
  sidebarWidth: { width: 242 }, allFoldersCollapsed: false, onToggleAllFolders: vi.fn(),
} as unknown as ComponentProps<typeof CatalogWorkspace>;
function Workspace({ initialView }: { initialView: ViewMode }) {
  const [detail, setDetail] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [view, setView] = useState(initialView);
  const [query, setQuery] = useState('');
  return <LanguageProvider><UiDensityProvider>
    <button onClick={() => setView('groupedList')}>Ansicht wechseln</button>
    <button onClick={() => setQuery('Zahn')}>Filter wechseln</button>
    <CatalogWorkspace {...props} view={view} query={query} selectedId={selected}
      selectModel={setSelected} detailModel={detail ? model : null} setDetailModelId={setDetail} />
  </UiDensityProvider></LanguageProvider>;
}
const scrollContainer = (container: HTMLElement) => container.querySelector<HTMLElement>('.overflow-y-auto.p-4')!;
describe('CatalogWorkspace detail return', () => {
  it.each(['grid', 'groupedGrid', 'groupedList'] as const)('keeps scroll and selection in %s', (initialView) => {
    const { container } = render(<Workspace initialView={initialView} />);
    scrollContainer(container).scrollTop = 1200; fireEvent.scroll(scrollContainer(container));
    const tile = container.querySelector<HTMLElement>('[data-model-id="1"]')!;
    fireEvent.click(tile); fireEvent.doubleClick(tile);
    expect(scrollContainer(container)).toBeNull();
    fireEvent.click(screen.getByText('Zurück'));
    expect(scrollContainer(container).scrollTop).toBe(1200);
    expect(container.querySelector('[data-model-id="1"]')).toHaveClass(initialView === 'groupedList' ? 'bg-[var(--accent-soft)]' : 'border-[var(--accent)]');
  });
  it.each(['Ansicht wechseln', 'Filter wechseln'])('does not restore after %s during details', (button) => {
    const { container } = render(<Workspace initialView="grid" />);
    scrollContainer(container).scrollTop = 1200; fireEvent.scroll(scrollContainer(container));
    fireEvent.doubleClick(container.querySelector('[data-model-id="1"]')!);
    fireEvent.click(screen.getByText(button)); fireEvent.click(screen.getByText('Zurück'));
    expect(scrollContainer(container).scrollTop).toBe(0);
  });
});
