import { useDetailPanelWidth } from '../hooks/useDetailPanelWidth';
import { useUiDensity } from '../hooks/UiDensityContext';
import { clampDetailPanelWidth } from '../lib/detailPanelWidthPreference';
import { DetailPanelResizer } from './DetailPanelResizer';
import type { CSSProperties } from 'react';
import { displayName } from '../lib/displayName';
import { usageLabels, type LicenseUsage } from '../lib/licenses';
import { printerBuildVolume } from '../lib/buildPlateFit';
import { ModelLayoutContext } from '../hooks/ModelLayoutContext';
import { scrollTileIntoView } from '../hooks/useKeyboardShortcuts';
import { shouldIgnoreCatalogShortcut } from '../lib/keyboardGuard';
import { Icon } from './Icon';
import { useLastPrinter } from '../hooks/useLastPrinter';
import { EmptyCatalogTips } from './KeyboardTipsDialog';
import { detailNeighbor, type DetailDirection } from '../hooks/useDetailNavigation';
import { useImportLock } from '../hooks/ImportLockContext';
import type { useFolderExpansion } from '../hooks/useFolderExpansion';
import type { useSidebarWidth } from '../hooks/useSidebarWidth';
import { useMemo, useRef, useLayoutEffect, useEffect, useState, useCallback, useContext, type ReactNode } from 'react';
import { useCatalogScroll } from '../hooks/useCatalogScroll';
import { Sidebar } from './Sidebar';
import { ModelGrid } from './ModelGrid';
import { SelectionAnnouncer } from './SelectionAnnouncer';
import { GroupedModelGrid } from './GroupedModelGrid';
import { GroupedModelList } from './GroupedModelList';
import { DetailPanel } from './DetailPanel';
import { BundlePanel, type BundleViewActions } from './BundlePanel';
import { buildCards, indexBundles, type BundleCard } from '../lib/bundles';
import { isCardOpen } from '../lib/bundleCards';
import { panelMembers } from '../lib/bundlePanel';
import { ModelDetailPage } from './ModelDetailPage';
import { CollectionsGallery } from './CollectionsGallery';
import { BulkActionToolbar, type BulkBundleActions } from './BulkActionToolbar';
import type { ModelFile, Folder, TagCount, ViewMode, SortKey, SortDirection, Collection, SlicerConfig, FolderTrashResult, Printer, Bundle } from '../types';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
import type { useCollapsedFolders } from '../hooks/useCollapsedFolders';
import { useLanguage, useT, useFormatCount } from '../i18n/LanguageContext';
import { tagLabel } from '../lib/autoTags';
import { useFilamentCheck } from '../hooks/useFilamentCheck';
import { selectFromQueue } from '../lib/queueSelect';
import { toolCounts as computeToolCounts, TOOL_VIEW_LABEL_KEY, type ToolView } from '../lib/toolViews';
import type { AppError } from '../lib/errors';
import { PrintDataFilterBar } from './PrintDataFilterBar';
import { PrintDataNotice } from './PrintDataNotice';
import { DuplicatesView } from './DuplicatesView';
import { PrintFilterContext } from '../hooks/PrintFilterContext';
import { BundleCardsContext, type BundleCardsSettings } from '../hooks/BundleCardsContext';
import { ToggleSwitch } from './ToggleSwitch';
import { isActive as printFilterActive, printDataFacets, type FilterKey, type PrintDataFilter } from '../lib/printDataFilter';
import { printFilterChips } from '../lib/printFilterLabels';
import { countByKind, FILE_KIND_FILTERS, type FileKindFilter } from '../lib/fileKind';
import { usePrintTechnology } from '../hooks/PrintTechnologyContext';
import { showsKind } from '../lib/printTechnology';

interface CatalogWorkspaceProps {
  initialLoading?: boolean;
  initialLoadFailed?: boolean;
  detailPanel?: 'auto' | 'pinned';
  onCloseDetails?: () => void;
  /** Settings panel; while given it takes the detail panel's place and the detail panel stays hidden. */
  settingsPanel?: (overlay: boolean) => ReactNode;
  printerRefreshKey?: string;
  onOpenTips?: () => void;
  importRow?: ReactNode;
  expansion: ReturnType<typeof useFolderExpansion>;
  catalogKey?: string | null;
  onNavigateDetail?: (direction: DetailDirection) => void;
  sidebarWidth: ReturnType<typeof useSidebarWidth>;
  allFoldersCollapsed: boolean;
  onToggleAllFolders: () => void;
  onClearFilters: () => void;
  query: string;
  setQuery: (q: string) => void;
  setActiveCollection: (id: string | null) => void;
  setCollectionsGalleryOpen: (open: boolean) => void;
  queue: ModelFile[];
  reorderQueue: (orderedIds: string[]) => void;
  removeFromQueue: (id: string) => void;
  selectModel: (id: string) => void;
  folders: Folder[];
  models: ModelFile[];
  activeFolderId: string;
  setActiveFolderId: (id: string) => void;
  onCreateFolder: (parentId: string | null, name: string) => void;
  draggedFileId?: string | null;
  /** Folder all dragged files are in, or null when they are not all in the same one. */
  draggedFileFolderId?: string | null;
  draggedFileCount?: number;
  dragOverCollectionId?: string | null;
  handleCollectionMouseEnter?: (id: string) => void;
  handleCollectionMouseLeave?: (id: string) => void;
  dragOverFolderId: string | null;
  draggedFolderId: string | null;
  handleFolderMouseEnter: (id: string) => void;
  handleFolderMouseLeave: (id: string) => void;
  onDragFolderStart: (id: string) => void;
  tags: TagCount[];
  activeTag: string | null;
  setActiveTag: (tag: string | null) => void;
  collections: Collection[];
  activeCollection: string | null;
  collectionsGalleryOpen: boolean;
  createCollection: (name: string) => void;
  renameCollection: (id: string, name: string) => void;
  deleteCollection: (id: string) => void | Promise<void>;
  detailModel: ModelFile | null;
  selectedForBulk: Set<string>;
  confirmBulkDelete: boolean;
  setConfirmBulkDelete: (value: boolean) => void;
  bulkDelete: () => void;
  onBulkMove?: () => void;
  bulkRemove?: () => Promise<void>;
  onCatalogRemoved?: () => void;
  onFolderRenamed?: (info: { id: string; from: string; to: string; count: number }) => void;
  onFolderTrashed?: (result: FolderTrashResult) => void;
  onDateRenamed?: (outcome: { renamed: number; failed: number }) => void;
  onRemoveModelFromCatalog?: (id: string) => Promise<void>;
  selectAllVisible: () => void;
  clearBulkSelection: () => void;
  bulkAddToQueue: () => void;
  addToCollectionMenuOpen: boolean;
  setAddToCollectionMenuOpen: (value: boolean) => void;
  bulkAddToCollection: (collectionId: string) => void;
  bulkRemoveFromCollection: () => void;
  bulkSetPrintStatus: (status: 'printed' | 'not_printed') => void;
  addTagMenuOpen: boolean;
  setAddTagMenuOpen: (value: boolean) => void;
  tagDraft: string;
  setTagDraft: (value: string) => void;
  bulkAddTag: (tag?: string) => void;
  removeTagMenuOpen: boolean;
  setRemoveTagMenuOpen: (value: boolean) => void;
  tagsInSelection: string[];
  tagsInAllSelection: string[];
  bulkRemoveTag: (tag: string) => void;
  setDetailModelId: (id: string | null) => void;
  addTag: (id: string, tag: string) => void;
  removeTag: (id: string, tag: string) => void;
  deleteModel: (id: string) => void;
  togglePrintStatus: (id: string) => void;
  toggleFavorite: (id: string) => void;
  addToQueue: (id: string) => void;
  uploadCustomImage: (id: string, file?: File) => void | Promise<void>;
  captureRenderSnapshot: (id: string, base64: string) => void;
  setModelLicenses?: (ids: string[], license: string | null) => Promise<void>;
  printStatus?: ModelFile['printStatus'] | null;
  setPrintStatus?: (status: ModelFile['printStatus'] | null) => void;
  /** File type filter (models, G-code); the group only appears once the catalog holds G-code. */
  fileKind?: FileKindFilter;
  setFileKind?: (kind: FileKindFilter) => void;
  usage?: LicenseUsage | null;
  setUsage?: (usage: LicenseUsage | null) => void;
  /** Printers of the Printer Manager; those with a build volume feed the "Fits on" filter and the print-bed section. */
  printers?: Printer[];
  fitPrinterId?: string | null;
  setFitPrinterId?: (printerId: string | null) => void;
  /** Print data filter (target printer, nozzle, layer height, print time, color). */
  printFilter?: PrintDataFilter;
  setPrintFilter?: (filter: PrintDataFilter) => void;
  /** Files the print data filter holds back or shows although a value is missing. */
  printNotice?: { hidden: number; shownUnknown: number; unread: number };
  /** Puts duplicate models in the trash; resolves with the ids still in the catalog afterwards. */
  onRemoveDuplicates?: (ids: string[]) => Promise<string[]>;
  /** Opens the settings at the block that reads the print data of unread files. */
  onOpenPrintDataReread?: () => void;
  setModelSourceUrl: (id: string, url: string | null) => void;
  openInSlicer: (id: string) => void;
  rescanMetadata: (id: string) => void;
  addModelToCollection: (fileId: string, collectionId: string) => void;
  slicers: SlicerConfig[];
  slicerError: AppError | null;
  rescanFeedback: { fileId: string; status: 'success' | 'error'; message?: string; unexpected?: boolean } | null;
  displayPreference: DisplayPreference;
  view: ViewMode;
  sort: SortKey;
  sortDirection?: SortDirection;
  /** Bundles of the catalog; with "show as one card" on, the lists show each as a single card. */
  bundles?: Bundle[];
  collapseBundles?: boolean;
  setCollapseBundles?: (collapse: boolean) => void;
  /** Bundle entries of the selection bar; absent where the catalog has no bundles to offer. */
  bundleActions?: BulkBundleActions;
  /** Whole bundle cards in the selection, for the announcement. */
  selectedBundleCount?: number;
  /** What the bundle panel and the detail page of a member can do with their bundle. */
  bundleView?: BundleViewActions;
  filtered: ModelFile[];
  collectionLoading?: boolean;
  selectedId: string | null;
  setContextMenu: (value: { modelId: string; x: number; y: number; bundleId?: string } | null) => void;
  toggleBulkSelect: (id: string) => void;
  reorderCollection: (orderedIds: string[]) => void;
  onDragFileStart: (id: string) => void;
  selected: ModelFile | null;
  collapsedFolders: ReturnType<typeof useCollapsedFolders>;
  toolView: ToolView | null;
  setToolView: (v: ToolView | null) => void;
  onOpenCleanup: () => void;
  cleanupScanning: boolean;
  cleanupError: AppError | null;
  bundleSuggestionCount?: number;
  onOpenBundleSuggestions?: (returnTo: HTMLElement | null) => void;
}

const NO_BUNDLES: Bundle[] = [];

export function CatalogWorkspace({
  initialLoading = false, initialLoadFailed = false,
  detailPanel = 'auto', onCloseDetails, settingsPanel, printerRefreshKey = '',
  onOpenTips,
  importRow,
  expansion,
  catalogKey,
  onNavigateDetail,
  sidebarWidth,
  allFoldersCollapsed,
  onToggleAllFolders,
  onClearFilters,
  query,
  setQuery,
  setActiveCollection,
  setCollectionsGalleryOpen,
  queue,
  reorderQueue,
  removeFromQueue,
  selectModel,
  folders,
  models,
  activeFolderId,
  setActiveFolderId,
  onCreateFolder,
  draggedFileId = null,
  draggedFileFolderId = null,
  draggedFileCount = 1,
  dragOverCollectionId,
  handleCollectionMouseEnter,
  handleCollectionMouseLeave,
  dragOverFolderId,
  draggedFolderId,
  handleFolderMouseEnter,
  handleFolderMouseLeave,
  onDragFolderStart,
  tags,
  activeTag,
  setActiveTag,
  collections,
  activeCollection,
  collectionsGalleryOpen,
  createCollection,
  renameCollection,
  deleteCollection,
  detailModel,
  selectedForBulk,
  confirmBulkDelete,
  setConfirmBulkDelete,
  bulkDelete,
  onBulkMove,
  bulkRemove,
  onCatalogRemoved,
  onFolderRenamed,
  onFolderTrashed,
  onDateRenamed,
  onRemoveModelFromCatalog,
  selectAllVisible,
  clearBulkSelection,
  bulkAddToQueue,
  addToCollectionMenuOpen,
  setAddToCollectionMenuOpen,
  bulkAddToCollection,
  bulkRemoveFromCollection,
  bulkSetPrintStatus,
  addTagMenuOpen,
  setAddTagMenuOpen,
  tagDraft,
  setTagDraft,
  bulkAddTag,
  removeTagMenuOpen,
  setRemoveTagMenuOpen,
  tagsInSelection,
  tagsInAllSelection,
  bulkRemoveTag,
  setDetailModelId,
  addTag,
  removeTag,
  deleteModel,
  togglePrintStatus,
  toggleFavorite,
  addToQueue,
  uploadCustomImage,
  captureRenderSnapshot,
  setModelSourceUrl,
  setModelLicenses,
  printStatus = null,
  setPrintStatus,
  fileKind = 'all',
  setFileKind,
  usage = null,
  setUsage,
  printers = [],
  fitPrinterId = null,
  setFitPrinterId,
  printFilter,
  setPrintFilter,
  printNotice,
  onRemoveDuplicates,
  onOpenPrintDataReread,
  openInSlicer,
  rescanMetadata,
  addModelToCollection,
  slicers,
  slicerError,
  rescanFeedback,
  displayPreference,
  view,
  sort,
  sortDirection,
  bundles = NO_BUNDLES,
  collapseBundles = false,
  setCollapseBundles,
  bundleActions,
  selectedBundleCount,
  bundleView,
  filtered,
  collectionLoading = false,
  selectedId,
  setContextMenu,
  toggleBulkSelect,
  reorderCollection,
  onDragFileStart,
  selected,
  collapsedFolders,
  toolView,
  setToolView,
  onOpenCleanup,
  cleanupScanning,
  cleanupError,
  bundleSuggestionCount,
  onOpenBundleSuggestions,
}: CatalogWorkspaceProps) {
  const [narrow, setNarrow] = useState(() => window.matchMedia?.('(max-width: 999px)').matches ?? false);
  useEffect(() => {
    const media = window.matchMedia?.('(max-width: 999px)');
    if (!media) return;
    const update = () => setNarrow(media.matches);
    update();
    media.addEventListener('change', update);
    return () => media.removeEventListener('change', update);
  }, []);
  const layouts = useContext(ModelLayoutContext);
  const panelVisible = !detailModel && !settingsPanel && (detailPanel === 'pinned' || selectedId !== null);
  useLayoutEffect(() => {
    if (!selectedId || detailModel) return;
    // Let the grid measure its new width before locating the selected row.
    let secondFrame = 0;
    const frame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        const layout = [...(layouts?.layouts.values() ?? [])].find(section => section.order.includes(selectedId));
        layout?.scrollToIndex(layout.order.indexOf(selectedId));
        document.querySelector<HTMLElement>(`[data-model-id="${CSS.escape(selectedId)}"]`)?.scrollIntoView?.({ block: 'nearest' });
      });
    });
    return () => { cancelAnimationFrame(frame); cancelAnimationFrame(secondFrame); };
  }, [panelVisible, narrow, selectedId, detailModel, layouts]);
  const rootRef = useRef<HTMLDivElement>(null);
  const barWrapRef = useRef<HTMLDivElement>(null);
  const { density } = useUiDensity();
  const { width: preferredWidth, setWidth: setDetailWidth, reset: resetDetailWidth } = useDetailPanelWidth();
  const [workspaceWidth, setWorkspaceWidth] = useState(1200);
  useLayoutEffect(() => {
    const root = rootRef.current;
    const main = root?.querySelector<HTMLElement>('[data-catalog-main]');
    if (!root || !main) return;
    const update = () => {
      const bounds = root.getBoundingClientRect();
      setWorkspaceWidth(Math.max(0, bounds.right - main.getBoundingClientRect().left));
    };
    update();
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(update);
    observer?.observe(root);
    observer?.observe(main);
    window.addEventListener('resize', update);
    return () => { observer?.disconnect(); window.removeEventListener('resize', update); };
  }, []);
  const defaultDetailWidth = density === 'comfort' ? 380 : 336;
  const displayedDetailWidth = clampDetailPanelWidth(preferredWidth ?? defaultDetailWidth, workspaceWidth);
  const resizedDetail = !narrow && (preferredWidth !== null || displayedDetailWidth !== defaultDetailWidth);

  // The narrow-window detail panel floats over the catalog. It starts below the
  // selection bar so the bar stays fully visible and usable at its full width.
  useLayoutEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const bar = barWrapRef.current;
    const update = () => {
      root.style.setProperty('--detail-overlay-top', bar ? `${bar.offsetTop + bar.offsetHeight}px` : '0px');
    };
    update();
    if (!bar) return;
    window.addEventListener('resize', update);
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(update);
    observer?.observe(bar);
    return () => { window.removeEventListener('resize', update); observer?.disconnect(); };
  });
  const closeDetails = useCallback(() => {
    const tile = selectedId ? document.querySelector<HTMLElement>(`[data-model-id="${CSS.escape(selectedId)}"]`) : null;
    onCloseDetails?.();
    tile?.focus({ preventScroll: true });
    if (selectedId) requestAnimationFrame(() => {
      const layout = [...(layouts?.layouts.values() ?? [])].find(section => section.order.includes(selectedId));
      scrollTileIntoView(selectedId, layout);
    });
  }, [onCloseDetails, selectedId, layouts]);
  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented || settingsPanel || detailPanel !== 'auto' || !selectedId || detailModel || selectedForBulk.size > 0 ||
        shouldIgnoreCatalogShortcut(event, { preserveEscapeSelection: true })) return;
      event.preventDefault();
      closeDetails();
    };
    window.addEventListener('keydown', escape);
    return () => window.removeEventListener('keydown', escape);
  }, [detailPanel, selectedId, detailModel, selectedForBulk.size, closeDetails, settingsPanel]);
  const containerRef = useRef<HTMLDivElement>(null);
  const savedScroll = useRef(0);
  const wasDetail = useRef(false);
  const resetKey = JSON.stringify([view, activeFolderId, activeTag, activeCollection, toolView, query]);
  const previousResetKey = useRef(resetKey);
  useLayoutEffect(() => {
    const container = containerRef.current;
    const keyChanged = previousResetKey.current !== resetKey;
    // A view or filter change while the detail page is open must not restore the old position.
    if (keyChanged) savedScroll.current = 0;
    if (container) {
      container.scrollTop = keyChanged ? 0 : wasDetail.current ? savedScroll.current : container.scrollTop;
      container.dispatchEvent(new Event('scroll'));
    }
    previousResetKey.current = resetKey;
    wasDetail.current = !!detailModel;
  }, [resetKey, !!detailModel]);
  const { language } = useLanguage();
  const { jobActive } = useImportLock();
  const t = useT();
  const formatCount = useFormatCount();
  const scrollRef = useCatalogScroll(detailModel !== null, JSON.stringify([
    view, sort, language, query, usage, printStatus, fileKind, fitPrinterId, printFilter, activeFolderId, activeTag, activeCollection, collectionsGalleryOpen, toolView,
  ]), selectedId);
  const refreshKey = `${printerRefreshKey}:${models.map(m => `${m.id}:${m.sliceInfo?.totalWeightG ?? ''}`).join('|')}`;
  const queueFilament = useFilamentCheck(queue.map(m => m.id), refreshKey);
  const selectedFilament = useFilamentCheck(selected ? [selected.id] : [], refreshKey);
  const lastPrinter = useLastPrinter(import.meta.env.VITE_TARGET === 'web' ? null : selected?.id ?? null, printerRefreshKey);
  // Counts over the whole catalog (without folder/tag/search) so they stay stable.
  const counts = useMemo(() => computeToolCounts(models, new Date()), [models]);
  const tagHues = useMemo(() => Object.fromEntries(tags.map(tag => [tag.label, tag.colorHue])), [tags]);
  const displayedModels = filtered;
  // Tool views and open collections have an order of their own and list files, not bundles.
  const bundleCards = useMemo<BundleCardsSettings>(() => {
    const flat = toolView !== null || activeCollection !== null;
    return { bundles, collapse: collapseBundles && !flat, sort: flat ? undefined : sort, sortDirection };
  }, [bundles, collapseBundles, toolView, activeCollection, sort, sortDirection]);
  // The card that is open in the detail panel; a bundle card then gets the bundle panel instead.
  const openBundleCard = useMemo<BundleCard | null>(() => {
    if (!bundleView || !bundleCards.collapse || selectedId === null) return null;
    const card = buildCards(displayedModels, bundles, { collapse: true }).find(item => item.type === 'bundle' && isCardOpen(item, selectedId));
    return card?.type === 'bundle' ? card : null;
  }, [bundleView, bundleCards.collapse, selectedId, displayedModels, bundles]);
  const bundleOfFile = useMemo(() => indexBundles(bundles), [bundles]);
  const detailBundle = detailModel && bundleView ? bundleOfFile.get(detailModel.id) : undefined;
  const detailBundleMembers = useMemo(() => detailBundle ? panelMembers(detailBundle, models) : [], [detailBundle, models]);
  // The switch is the way back to single files, so it stays while it is on, and appears once bundles exist.
  const showBundleSwitch = !!setCollapseBundles && (bundles.length > 0 || collapseBundles);
  // The selection survives filter changes, so the confirmation must say how much of it is out of sight.
  const visibleIds = useMemo(() => new Set(displayedModels.map(model => model.id)), [displayedModels]);
  const selectedNames = useMemo(() => {
    const byId = new Map(models.map(model => [model.id, model.name]));
    return [...selectedForBulk].map(id => byId.get(id) ?? id);
  }, [models, selectedForBulk]);
  const hiddenSelectedCount = [...selectedForBulk].filter(id => !visibleIds.has(id)).length;
  const collectionName = collections.find(c => c.id === activeCollection)?.name ?? activeCollection ?? '';
  const folderName = folders.find(f => f.id === activeFolderId)?.name ?? activeFolderId;
  const kindCounts = useMemo(() => countByKind(models), [models]);
  const kindLabel = (kind: FileKindFilter) => t(kind === 'all' ? 'fileKindAll' : kind === 'model' ? 'fileKindModels' : kind === 'resin' ? 'fileKindResin' : 'fileKindGcode');
  // Only worth a control once there is slicer output to filter out; a chosen filter stays visible until removed.
  const showKindFilter = !!setFileKind && (kindCounts.gcode + kindCounts.resin > 0 || fileKind !== 'all');
  const printStatusLabel = printStatus === 'printed' ? t('printStatusPrinted') : t('printStatusNotPrinted');
  const technology = usePrintTechnology().technology;
  // Printers the printer list hides for the chosen technology are no choice here either.
  const fitPrinters = printers.filter(printer => printerBuildVolume(printer) && showsKind(technology, printer.kind));
  const fitPrinter = fitPrinters.find(printer => printer.id === fitPrinterId) ?? null;
  const [printFilterOpen, setPrintFilterOpen] = useState<FilterKey | null>(null);
  const printFilterIsActive = !!printFilter && printFilterActive(printFilter);
  // The values the popovers and chips name; counted over the whole catalog, and only when needed.
  const printFacets = useMemo(
    () => (printFilterOpen !== null || printFilterIsActive ? printDataFacets(models) : null),
    [models, printFilterOpen, printFilterIsActive],
  );
  const barRef = useRef<HTMLElement>(null);
  const filterHeadingRef = useRef<HTMLSpanElement>(null);
  const focusFilterTarget = (target: FilterKey | 'heading') => {
    const button = target === 'heading' ? null : barRef.current?.querySelector<HTMLElement>(`[data-pf-button="${target}"]`);
    (button ?? filterHeadingRef.current)?.focus();
  };
  const baseChips = [
    ...(printStatus ? [{kind: 'printStatusFilter' as const, value: printStatusLabel,
      removeLabel: t('chipRemove').replace('{label}', printStatusLabel), remove: () => setPrintStatus?.(null)}] : []),
    ...(fileKind !== 'all' ? [{kind: 'fileKindFilter' as const, value: kindLabel(fileKind),
      removeLabel: t('chipRemove').replace('{label}', `${t('fileKindFilter')} ${kindLabel(fileKind)}`), remove: () => setFileKind?.('all')}] : []),
    ...(usage ? [{kind: 'licenseUsage' as const, value: t(usageLabels[usage]),
      removeLabel: t('chipRemove').replace('{label}', t(usageLabels[usage])), remove: () => setUsage?.(null)}] : []),
    ...(fitPrinter ? [{kind: 'fitPrinterFilter' as const, value: fitPrinter.name,
      removeLabel: t('chipRemove').replace('{label}', `${t('fitPrinterFilter')} ${fitPrinter.name}`), remove: () => setFitPrinterId?.(null)}] : []),
    ...(toolView ? [{kind: 'filterBarView' as const, value: t(TOOL_VIEW_LABEL_KEY[toolView]),
      removeLabel: t('chipRemove').replace('{label}', t(TOOL_VIEW_LABEL_KEY[toolView])), remove: () => setToolView(null)}] : []),
    ...(activeCollection ? [{kind: 'filterBarCollection' as const, value: collectionName,
      removeLabel: t('filterBarRemoveCollection').replace('{label}', collectionName), remove: () => setActiveCollection(null)}] : []),
    ...(activeFolderId && activeFolderId !== 'all' ? [{kind: 'filterBarFolder' as const, value: displayName(folderName),
      removeLabel: t('filterBarRemoveFolder').replace('{label}', displayName(folderName)), remove: () => setActiveFolderId('all')}] : []),
    ...(activeTag ? [{kind: 'filterBarTag' as const, value: `#${tagLabel(activeTag, language)}`,
      removeLabel: t('chipRemove').replace('{label}', `#${tagLabel(activeTag, language)}`), remove: () => setActiveTag(null)}] : []),
    ...(query ? [{kind: 'filterBarSearch' as const, value: query,
      removeLabel: t('filterBarRemoveSearch').replace('{label}', query), remove: () => setQuery('')}] : []),
  ];
  const printChips = printFilter && setPrintFilter ? printFilterChips(printFilter, printFacets, setPrintFilter, t, language) : [];
  const chips = [
    ...baseChips,
    ...printChips.map(chip => ({
      kind: chip.kind, value: chip.value,
      removeLabel: t('chipRemove').replace('{label}', `${t(chip.kind)} ${chip.value}`),
      // The last chip hands the focus to the heading; the others to their own filter button.
      remove: () => { chip.remove(); focusFilterTarget(baseChips.length + printChips.length === 1 ? 'heading' : chip.focus); },
    })),
  ];
  const clearAllFilters = () => { onClearFilters(); filterHeadingRef.current?.focus(); };
  const clearButtonClass = 'px-2.5 py-1 rounded-[5px] border border-[var(--accent)] text-[var(--accent)] text-[length:var(--font-size-control)] font-semibold hover:bg-[var(--accent-soft)] cursor-pointer';
  // Always shown (also without a selection) so "select all" can be found and the
  // grid does not jump on the first checkbox; only views with nothing to select hide it.
  // The duplicates view stands in for the catalog list: it has its own selection and ignores the filters.
  const duplicatesMode = toolView === 'duplicates' && !detailModel && !collectionsGalleryOpen && !activeCollection;
  const showBulkBar = !detailModel && !duplicatesMode && (selectedForBulk.size > 0 || (!collectionsGalleryOpen && models.length > 0));
  const toolOnly = toolView && !activeCollection && activeFolderId === 'all' && !activeTag && !query && !usage && !printStatus && fileKind === 'all' && !fitPrinter && !printFilterIsActive;
  return (
    <PrintFilterContext.Provider value={printFilterIsActive && printFilter ? printFilter : null}>
    <BundleCardsContext.Provider value={bundleCards}>
    <div ref={rootRef} className="relative flex-1 flex min-h-0">
      <Sidebar
        catalogKey={catalogKey}
        expansion={expansion}
        width={sidebarWidth.width}
        setWidth={sidebarWidth.setWidth}
        resetWidth={sidebarWidth.reset}
        allFoldersCollapsed={allFoldersCollapsed}
        onToggleAllFolders={onToggleAllFolders}
        query={query}
        onQueryChange={(q) => {
          setQuery(q);
          setActiveCollection(null);
          setCollectionsGalleryOpen(false);
        }}
        queue={queue}
        onQueueReorder={reorderQueue}
        onQueueRemove={removeFromQueue}
        onQueueSelect={(id) =>
          selectFromQueue(id, { detailOpen: detailModel !== null, selectModel, openDetail: setDetailModelId })
        }
        queueFilament={queueFilament.checks}
        folders={folders}
        totalModelCount={models.length}
        onCatalogRemoved={onCatalogRemoved}
        onFolderRenamed={onFolderRenamed}
        onFolderTrashed={onFolderTrashed}
        activeFolderId={activeFolderId}
        onFolderSelect={(id) => {
          setActiveFolderId(id);
          setActiveCollection(null);
          setCollectionsGalleryOpen(false);
        }}
        onCreateFolder={onCreateFolder}
        dragOverCollectionId={dragOverCollectionId}
        onCollectionMouseEnter={handleCollectionMouseEnter}
        onCollectionMouseLeave={handleCollectionMouseLeave}
        dragOverFolderId={dragOverFolderId}
        draggedFolderId={draggedFolderId}
        draggedFileId={draggedFileId}
        draggedFileFolderId={draggedFileFolderId}
        draggedFileCount={draggedFileCount}
        onFolderMouseEnter={handleFolderMouseEnter}
        onFolderMouseLeave={handleFolderMouseLeave}
        onDragFolderStart={jobActive ? () => {} : onDragFolderStart}
        tags={tags}
        activeTag={activeTag}
        onTagSelect={(tag) => {
          setActiveTag(tag);
          setActiveCollection(null);
          setCollectionsGalleryOpen(false);
        }}
        collections={collections}
        activeCollection={activeCollection}
        collectionsGalleryOpen={collectionsGalleryOpen}
        onSelectCollection={(id) => {
          setActiveCollection(id);
          setCollectionsGalleryOpen(false);
          setToolView(null);
        }}
        onOpenCollectionsGallery={() => {
          setCollectionsGalleryOpen(true);
          setActiveCollection(null);
          setToolView(null);
        }}
        onCreateCollection={createCollection}
        onRenameCollection={renameCollection}
        onDeleteCollection={async (id) => {
          await deleteCollection(id);
          if (activeCollection === id && !collectionsGalleryOpen) {
            setActiveFolderId('all'); setActiveTag(null); setToolView(null);
          }
        }}
        toolView={toolView}
        onToolViewChange={(v) => {
          setToolView(v);
          // View and collection are mutually exclusive.
          if (v) {
            setActiveCollection(null);
            setCollectionsGalleryOpen(false);
          }
        }}
        toolCounts={counts}
        onOpenCleanup={onOpenCleanup}
        cleanupScanning={cleanupScanning}
        cleanupError={cleanupError}
        bundleSuggestionCount={bundleSuggestionCount}
        onOpenBundleSuggestions={onOpenBundleSuggestions}
      />

      <main tabIndex={-1} data-catalog-main className="isolate overflow-hidden flex-1 min-w-0 flex flex-col min-h-0">
        <SelectionAnnouncer count={selectedForBulk.size} bundleCount={selectedBundleCount} />
        {importRow}
        {!duplicatesMode && (setUsage || setPrintStatus || setPrintFilter || showBundleSwitch || (setFitPrinterId && fitPrinters.length > 0) || chips.length > 0) && (
          <section ref={barRef} aria-label={t('filterBarAria')} className="flex-none flex flex-wrap items-center gap-x-2 gap-y-1.5 px-4 py-2 border-b border-[var(--line)] bg-[var(--bg)]">
            <span ref={filterHeadingRef} tabIndex={-1} className="ui-label text-[var(--ink-3)]">{t('filterBarHeading')}</span>
            {setUsage && <label className="flex items-center gap-2 text-small text-[var(--ink-2)]">
              {t('licenseUsage')}
              <select aria-label={t('licenseUsage')} value={usage ?? ''} onChange={event => setUsage((event.target.value || null) as LicenseUsage | null)}
                className="max-w-full rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] px-2 py-1">
                <option value="">{t('licenseUsageAll')}</option>
                {(['commercial', 'private', 'unknown'] as const).map(value => <option key={value} value={value}>{t(usageLabels[value])}</option>)}
              </select>
            </label>}
            {setPrintStatus && <label className="flex items-center gap-2 text-small text-[var(--ink-2)]">
              {t('printStatusFilter')}
              <select aria-label={t('printStatusFilter')} value={printStatus ?? ''} onChange={event => setPrintStatus((event.target.value || null) as ModelFile['printStatus'] | null)}
                className="max-w-full rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] px-2 py-1">
                <option value="">{t('printStatusAll')}</option>
                <option value="printed">{t('printStatusPrinted')}</option>
                <option value="not_printed">{t('printStatusNotPrinted')}</option>
              </select>
            </label>}
            {showBundleSwitch && (
              <div className="flex items-center gap-2 text-small text-[var(--ink-2)]">
                <span id="filter-bundle-collapse">{t('bundleCollapseToggle')}</span>
                <ToggleSwitch checked={collapseBundles} label={t('bundleCollapseToggle')} onChange={setCollapseBundles} />
              </div>
            )}
            {showKindFilter && setFileKind && (
              <div role="group" aria-labelledby="filter-file-kind" className="flex flex-wrap items-center gap-1.5 text-small text-[var(--ink-2)]">
                <span id="filter-file-kind">{t('fileKindFilter')}</span>
                {FILE_KIND_FILTERS.map(kind => (
                  <button key={kind} type="button" aria-pressed={fileKind === kind} data-file-kind={kind} onClick={() => setFileKind(kind)}
                    className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 cursor-pointer text-[var(--ink)] hover:border-[var(--accent)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)] ${fileKind === kind ? 'border-[var(--accent)] bg-[var(--accent-soft)]' : 'border-[var(--line-strong)]'}`}>
                    {kindLabel(kind)} <span className="tabular-nums text-[var(--ink-3)]">{kindCounts[kind]}</span>
                  </button>
                ))}
              </div>
            )}
            {setFitPrinterId && fitPrinters.length > 0 && <label className="flex items-center gap-2 text-small text-[var(--ink-2)]">
              {t('fitPrinterFilter')}
              <select aria-label={t('fitPrinterFilter')} value={fitPrinter?.id ?? ''} onChange={event => setFitPrinterId(event.target.value || null)}
                className="max-w-full rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] px-2 py-1">
                <option value="">{t('fitPrinterAll')}</option>
                {fitPrinters.map(printer => <option key={printer.id} value={printer.id}>{printer.name}</option>)}
              </select>
            </label>}
            {printFilter && setPrintFilter && (
              <PrintDataFilterBar filter={printFilter} onChange={setPrintFilter} facets={printFacets}
                open={printFilterOpen} onOpenChange={setPrintFilterOpen} hits={displayedModels.length} />
            )}
            {chips.map(chip => (
              <span key={chip.kind} className="inline-flex max-w-full items-center gap-1.5 pl-2 pr-1 py-0.5 rounded-full border border-[var(--line-strong)] bg-[var(--panel)] text-[length:var(--font-size-control)]">
                <span data-filter-kind className="ui-label text-[var(--ink-3)]">{t(chip.kind)}</span>
                <b className="min-w-0 break-words font-semibold">{chip.value}</b>
                <button type="button" onClick={chip.remove} aria-label={chip.removeLabel}
                  className="flex-none w-5 h-5 grid place-items-center rounded-full text-[var(--ink-3)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] cursor-pointer"><Icon name="close" size={14} /></button>
              </span>
            ))}
            <div className="ml-auto flex flex-wrap items-center gap-2">
              <span aria-live="polite" className="font-medium tabular-nums text-[length:var(--font-size-meta)] text-[var(--ink-2)] tabular-nums">
                {collectionLoading ? t('catalogLoading') : t('filterBarCount').replace('{count}', String(displayedModels.length)).replace('{total}', String(models.length))}
              </span>
              <button type="button" onClick={clearAllFilters} className={clearButtonClass}>{t('filterBarClearAll')}</button>
            </div>
          </section>
        )}

        {!duplicatesMode && printFilter && setPrintFilter && printFilterIsActive && printNotice && (
          <PrintDataNotice hidden={printNotice.hidden} shownUnknown={printNotice.shownUnknown} unread={printNotice.unread} onReread={onOpenPrintDataReread} includeUnknown={printFilter.includeUnknown}
            onIncludeUnknownChange={include => setPrintFilter({ ...printFilter, includeUnknown: include })} />
        )}

        {showBulkBar && (
          <div ref={barWrapRef} className="flex-none relative z-30">
          <BulkActionToolbar
            compact={narrow}
            selectedCount={selectedForBulk.size}
            selectedIds={[...selectedForBulk]}
            selectedNames={selectedNames}
            hiddenSelectedCount={hiddenSelectedCount}
            confirmBulkDelete={confirmBulkDelete}
            onConfirmBulkDeleteChange={setConfirmBulkDelete}
            onSelectAllVisible={selectAllVisible}
            onClearSelection={clearBulkSelection}
            onBulkAddToQueue={bulkAddToQueue}
            addToCollectionMenuOpen={addToCollectionMenuOpen}
            onAddToCollectionMenuOpenChange={setAddToCollectionMenuOpen}
            collections={collections}
            onBulkAddToCollection={bulkAddToCollection}
            activeCollection={activeCollection}
            onBulkRemoveFromCollection={bulkRemoveFromCollection}
            onBulkSetLicense={setModelLicenses ? value => setModelLicenses([...selectedForBulk], value) : undefined}
            onBulkSetPrintStatus={bulkSetPrintStatus}
            onBulkDelete={bulkDelete}
            onBulkMove={onBulkMove}
            onBulkRemove={bulkRemove}
            addTagMenuOpen={addTagMenuOpen}
            onAddTagMenuOpenChange={setAddTagMenuOpen}
            allTags={tags.map(tag => tag.label)}
            tagsInAllSelection={tagsInAllSelection}
            tagDraft={tagDraft}
            onTagDraftChange={setTagDraft}
            onSubmitBulkAddTag={bulkAddTag}
            removeTagMenuOpen={removeTagMenuOpen}
            onRemoveTagMenuOpenChange={setRemoveTagMenuOpen}
            tagsInSelection={tagsInSelection}
            onBulkRemoveTag={bulkRemoveTag}
            onDateRenamed={onDateRenamed}
            bundleActions={bundleActions}
          />
          </div>
        )}

        {collectionsGalleryOpen ? (
          <CollectionsGallery
            collections={collections}
            onSelect={(id) => {
              setActiveCollection(id);
              setCollectionsGalleryOpen(false);
            }}
            onCreate={createCollection}
            onRename={renameCollection}
            onDelete={deleteCollection}
            onBack={() => setCollectionsGalleryOpen(false)}
          />
        ) : detailModel ? (
          <ModelDetailPage
            key={detailModel.id}
            onNavigate={onNavigateDetail}
            hasPrevious={detailNeighbor(filtered.map(model => model.id), detailModel.id, 'previous') !== null}
            hasNext={detailNeighbor(filtered.map(model => model.id), detailModel.id, 'next') !== null}
            position={filtered.some(model => model.id === detailModel.id)
              ? { index: filtered.findIndex(model => model.id === detailModel.id) + 1, total: filtered.length }
              : undefined}
            allTags={tags.map(tag => tag.label)}
            tagHues={tagHues}
            model={detailModel}
            printers={printers}
            onClose={() => setDetailModelId(null)}
            onAddTag={(t) => addTag(detailModel.id, t)}
            onRemoveTag={(t) => removeTag(detailModel.id, t)}
            onDelete={() => {
              deleteModel(detailModel.id);
              setDetailModelId(null);
            }}
            onTogglePrintStatus={() => togglePrintStatus(detailModel.id)}
            onToggleFavorite={() => toggleFavorite(detailModel.id)}
            onToggleQueue={() =>
              detailModel.queuePosition !== null ? removeFromQueue(detailModel.id) : addToQueue(detailModel.id)
            }
            onUploadImage={(file) => uploadCustomImage(detailModel.id, file)}
            onSnapshotCaptured={(base64) => captureRenderSnapshot(detailModel.id, base64)}
            onSetLicense={setModelLicenses}
            onSetSourceUrl={(fileId, url) => setModelSourceUrl(fileId, url)}
            onOpenInSlicer={() => openInSlicer(detailModel.id)}
            onRemoveFromCatalog={onRemoveModelFromCatalog ? () => onRemoveModelFromCatalog(detailModel.id) : undefined}
            onRescanMetadata={() => rescanMetadata(detailModel.id)}
            onAddToCollection={(collectionId) => addModelToCollection(detailModel.id, collectionId)}
            collections={collections}
            slicers={slicers}
            slicerError={slicerError}
            rescanError={
              rescanFeedback?.fileId === detailModel.id && rescanFeedback.status === 'error'
                ? { message: rescanFeedback.message ?? '', unexpected: rescanFeedback.unexpected ?? false }
                : null
            }
            rescanSuccess={rescanFeedback?.fileId === detailModel.id && rescanFeedback.status === 'success'}
            displayPreference={displayPreference}
            bundleView={detailBundle && bundleView && detailBundleMembers.length >= 2
              ? { bundle: detailBundle, members: detailBundleMembers, onOpenMember: setDetailModelId, actions: bundleView }
              : null}
          />
        ) : (
          <div ref={node => { containerRef.current = node; scrollRef(node); }} data-catalog-scroller onClick={event => {
            const target = event.target;
            if (!settingsPanel && detailPanel === 'auto' && selectedId && target instanceof Element &&
              !target.closest('[data-model-id], button, input, a, [role="button"], [role="row"], [data-folder-header], [data-model-list-header], [data-navigation-menu]')) closeDetails();
          }} onScroll={event => { savedScroll.current = event.currentTarget.scrollTop; }} className="flex-1 overflow-y-auto overscroll-contain p-4">
            {initialLoading || collectionLoading ? <div role="status" className="p-6 text-body text-[var(--ink-2)]">
              {t('catalogLoading')}
              <div aria-hidden="true" className="mt-4 grid grid-cols-3 gap-4">
                {[0, 1, 2].map(key => <div key={key} className="h-40 rounded-lg bg-[var(--panel-2)]" />)}
              </div>
            </div> : initialLoadFailed ? <div role="alert" className="p-6 text-body text-[var(--crit)]">{t('catalogLoadFailed')}</div> : models.length === 0 && onOpenTips ? <EmptyCatalogTips onOpenTips={onOpenTips} /> : duplicatesMode ? <DuplicatesView models={models} onRemove={onRemoveDuplicates} /> : toolOnly && displayedModels.length === 0 ? (
              <div className="font-medium tabular-nums text-[length:var(--font-size-item)] text-[var(--ink-3)] px-1.5 py-8 text-center">
                {t('toolViewEmpty')}
              </div>
            ) : chips.length > 0 && displayedModels.length === 0 ? (
              <div className="flex flex-col items-center gap-2.5 border border-dashed border-[var(--line-strong)] rounded-[10px] px-5 py-8 text-center">
                <h3 className="font-semibold text-[length:var(--font-size-body)]">{t('filterBarEmptyTitle')}</h3>
                <p className="max-w-[48ch] text-[length:var(--font-size-body)] text-[var(--ink-2)]">{formatCount(t('filterBarEmptyText'), models.length)}</p>
                <button type="button" onClick={clearAllFilters} className={clearButtonClass}>{t('filterBarClearAll')}</button>
              </div>
            ) : view === 'grid' ? (
              <ModelGrid
                containerRef={containerRef}
                models={displayedModels}
                selectedId={selectedId}
                onSelect={selectModel}
                onOpenDetail={setDetailModelId}
                onContextMenu={(id, x, y, bundleId) => setContextMenu({ modelId: id, x, y, bundleId })}
                onToggleFavorite={toggleFavorite}
                selectedForBulk={selectedForBulk}
                onToggleBulkSelect={toggleBulkSelect}
                displayPreference={displayPreference}
                reorderable={activeCollection !== null}
                onReorder={reorderCollection}
                onDragFileStart={jobActive ? undefined : onDragFileStart}
              />
            ) : view === 'groupedGrid' ? (
              <GroupedModelGrid
                containerRef={containerRef}
                models={displayedModels}
                folders={folders}
                selectedId={selectedId}
                onSelect={selectModel}
                onOpenDetail={setDetailModelId}
                onContextMenu={(id, x, y, bundleId) => setContextMenu({ modelId: id, x, y, bundleId })}
                onToggleFavorite={toggleFavorite}
                selectedForBulk={selectedForBulk}
                onToggleBulkSelect={toggleBulkSelect}
                displayPreference={displayPreference}
                onDragFileStart={jobActive ? undefined : onDragFileStart}
                draggedFolderId={draggedFolderId}
                draggedFileId={draggedFileId}
                draggedFileFolderId={draggedFileFolderId}
        draggedFileCount={draggedFileCount}
                dragOverFolderId={dragOverFolderId}
                onDragFolderStart={jobActive ? () => {} : onDragFolderStart}
                onFolderMouseEnter={handleFolderMouseEnter}
                onFolderMouseLeave={handleFolderMouseLeave}
                collapsedFolders={collapsedFolders}
              />
            ) : (
              <GroupedModelList
                containerRef={containerRef}
                models={displayedModels}
                folders={folders}
                selectedId={selectedId}
                onSelect={selectModel}
                onOpenDetail={setDetailModelId}
                onContextMenu={(id, x, y, bundleId) => setContextMenu({ modelId: id, x, y, bundleId })}
                selectedForBulk={selectedForBulk}
                onToggleBulkSelect={toggleBulkSelect}
                onDragFileStart={jobActive ? undefined : onDragFileStart}
                draggedFolderId={draggedFolderId}
                draggedFileId={draggedFileId}
                draggedFileFolderId={draggedFileFolderId}
        draggedFileCount={draggedFileCount}
                dragOverFolderId={dragOverFolderId}
                onDragFolderStart={jobActive ? () => {} : onDragFolderStart}
                onFolderMouseEnter={handleFolderMouseEnter}
                onFolderMouseLeave={handleFolderMouseLeave}
                collapsedFolders={collapsedFolders}
              />
            )}
          </div>
        )}
      </main>

      {settingsPanel ? settingsPanel(narrow || !!detailModel) : !detailModel && (detailPanel === 'pinned' || selectedId !== null) && (
        <div role="complementary" aria-label={t('detailPanelTitle')} className={`detail-panel-shell ${narrow ? 'detail-panel-overlay' : ''} ${resizedDetail ? 'detail-panel-resized' : ''}`}
          style={resizedDetail ? { '--detail-panel-width': `${displayedDetailWidth}px` } as CSSProperties : undefined}>
        {!narrow && <DetailPanelResizer width={displayedDetailWidth} workspaceWidth={workspaceWidth} setWidth={setDetailWidth} reset={resetDetailWidth} />}
        {detailPanel === 'auto' && <button type="button" className="detail-panel-close" aria-label={t('detailPanelClose')} title={`${t('detailPanelClose')} (Esc)`} onClick={closeDetails}><Icon name="close" size={16} /></button>}
        {openBundleCard && bundleView ? (
          <BundlePanel
            card={openBundleCard}
            members={panelMembers(openBundleCard.bundle, models)}
            allTags={tags.map(tag => tag.label)}
            displayPreference={displayPreference}
            actions={bundleView}
            onOpenMember={setDetailModelId}
            onSetPrinted={(targets) => targets.forEach(model => togglePrintStatus(model.id))}
            onSetFavorite={(targets) => targets.forEach(model => toggleFavorite(model.id))}
            onAddTag={(targets, tag) => targets.forEach(model => addTag(model.id, tag))}
            onSetLicense={setModelLicenses}
            onQueue={(targets) => targets.forEach(model => addToQueue(model.id))}
          />
        ) : <DetailPanel
          filament={selectedFilament.error ? null : selectedFilament.checks?.get(selected?.id ?? '')}
          lastPrinter={lastPrinter}
          printers={printers}
          allTags={tags.map(tag => tag.label)}
          tagHues={tagHues}
          model={selected}
          onAddTag={(t) => selected && addTag(selected.id, t)}
          onRemoveTag={(t) => selected && removeTag(selected.id, t)}
          onDelete={() => selected && deleteModel(selected.id)}
          onTogglePrintStatus={() => selected && togglePrintStatus(selected.id)}
          onToggleFavorite={() => selected && toggleFavorite(selected.id)}
          onToggleQueue={() =>
            selected && (selected.queuePosition !== null ? removeFromQueue(selected.id) : addToQueue(selected.id))
          }
          onUploadImage={(file) => selected ? uploadCustomImage(selected.id, file) : undefined}
          onSnapshotCaptured={(base64) => selected && captureRenderSnapshot(selected.id, base64)}
          onSetLicense={setModelLicenses}
          onSetSourceUrl={(fileId, url) => setModelSourceUrl(fileId, url)}
          onOpenInSlicer={() => selected && openInSlicer(selected.id)}
          hasSlicer={slicers.length > 0}
          onRemoveFromCatalog={selected && onRemoveModelFromCatalog ? () => onRemoveModelFromCatalog(selected.id) : undefined}
          slicerError={slicerError}
        />}
        </div>
      )}
    </div>
    </BundleCardsContext.Provider>
    </PrintFilterContext.Provider>
  );
}
