import type { LicenseUsage } from '../lib/licenses';
import { useMemo, useState } from 'react';
import type { Language } from '../i18n/types';
import type { ModelFile, Folder, ViewMode, SortKey, SortDirection, Printer } from '../types';
import { printerBuildVolume } from '../lib/buildPlateFit';
import type { RecentSnapshot, ToolView } from '../lib/toolViews';
import { defaultSortDirection, filterAndSortModels, selectQueuedModels } from '../lib/catalogFilters';
import type { FileKindFilter } from '../lib/fileKind';
import { EMPTY_FILTER, splitByPrintData, type PrintDataFilter } from '../lib/printDataFilter';

export function useCatalogFilters(models: ModelFile[], folders: Folder[], language: Language = 'de', printers: readonly Printer[] = []) {
  const [view, setView] = useState<ViewMode>('grid');
  const [sorting, setSorting] = useState<{ sort: SortKey; sortDirection: SortDirection }>({ sort: 'name', sortDirection: 'asc' });
  const { sort, sortDirection } = sorting;
  const setSort = (next: SortKey) => {
    setSorting(current => next === current.sort ? current : { sort: next, sortDirection: defaultSortDirection(next) });
  };
  const setSortDirection = (direction: SortDirection) => {
    setSorting(current => ({ ...current, sortDirection: direction }));
  };
  const [printStatus, setPrintStatus] = useState<ModelFile['printStatus'] | null>(null);
  const [usage, setUsage] = useState<LicenseUsage | null>(null);
  const [fileKind, setFileKind] = useState<FileKindFilter>('all');
  const [fitPrinterChoice, setFitPrinterId] = useState<string | null>(null);
  // Not kept over a restart, like the other filters.
  const [printFilter, setPrintFilter] = useState<PrintDataFilter>(EMPTY_FILTER);
  const [query, setQuery] = useState('');
  const [activeFolderId, setActiveFolderId] = useState('all');
  const [activeTag, setActiveTag] = useState<string | null>(null);
  const [toolView, setToolViewState] = useState<ToolView | null>(null);
  // Frozen when the "recent" view is (re)activated so a click on a model
  // (which updates lastViewedAt) doesn't change the order right away.
  const [recentSnapshot, setRecentSnapshot] = useState<RecentSnapshot | null>(null);

  const setToolView = (next: ToolView | null) => {
    setToolViewState(next);
    setRecentSnapshot(next === 'recent' ? new Map(models.map((m) => [m.id, m.lastViewedAt])) : null);
  };

  // A printer that was deleted or lost its build volume no longer filters
  // (and no longer shows as a chip), instead of silently hiding everything.
  const fitPrinter = printers.find(printer => printer.id === fitPrinterChoice && printerBuildVolume(printer)) ?? null;
  const fitBed = useMemo(() => (fitPrinter ? printerBuildVolume(fitPrinter) : null), [fitPrinter]);

  const printSplit = useMemo(() => {
    const matched = filterAndSortModels(models, folders, {
      usage,
      printStatus,
      fileKind,
      fitBed,
      activeFolderId,
      activeTag,
      query,
      sort,
      sortDirection,
      language,
      toolView,
      recentSnapshot: recentSnapshot ?? undefined,
    });
    // After the other filters and the sort: a second pass that keeps the order.
    return splitByPrintData(matched, printFilter);
  }, [models, folders, usage, printStatus, fileKind, fitBed, activeFolderId, activeTag, query, sort, sortDirection, language, toolView, recentSnapshot, printFilter]);
  const filtered = printSplit.visible;
  const printNotice = useMemo(
    () => ({ hidden: printSplit.hidden, shownUnknown: printSplit.shownUnknown, unread: printSplit.unread }),
    [printSplit.hidden, printSplit.shownUnknown, printSplit.unread],
  );

  const queue = useMemo(() => selectQueuedModels(models), [models]);

  return {
    usage, setUsage,
    printStatus, setPrintStatus,
    fileKind, setFileKind,
    fitPrinterId: fitPrinter?.id ?? null, setFitPrinterId, fitBed,
    printFilter, setPrintFilter, printNotice,
    view, setView,
    sort, setSort, sortDirection, setSortDirection,
    query, setQuery,
    activeFolderId, setActiveFolderId,
    activeTag, setActiveTag,
    toolView, setToolView,
    filtered,
    queue,
  };
}
