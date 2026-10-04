import { useMemo, useState } from 'react';
import type { Language } from '../i18n/types';
import type { ModelFile, Folder, ViewMode, SortKey, SortDirection } from '../types';
import type { RecentSnapshot, ToolView } from '../lib/toolViews';
import { defaultSortDirection, filterAndSortModels, selectQueuedModels } from '../lib/catalogFilters';

export function useCatalogFilters(models: ModelFile[], folders: Folder[], language: Language = 'de') {
  const [view, setView] = useState<ViewMode>('grid');
  const [sorting, setSorting] = useState<{ sort: SortKey; sortDirection: SortDirection }>({ sort: 'name', sortDirection: 'asc' });
  const { sort, sortDirection } = sorting;
  const setSort = (next: SortKey) => {
    setSorting(current => next === current.sort ? current : { sort: next, sortDirection: defaultSortDirection(next) });
  };
  const setSortDirection = (direction: SortDirection) => {
    setSorting(current => ({ ...current, sortDirection: direction }));
  };
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

  const filtered = useMemo(
    () =>
      filterAndSortModels(models, folders, {
        activeFolderId,
        activeTag,
        query,
        sort,
        sortDirection,
        language,
        toolView,
        recentSnapshot: recentSnapshot ?? undefined,
      }),
    [models, folders, activeFolderId, activeTag, query, sort, sortDirection, language, toolView, recentSnapshot],
  );

  const queue = useMemo(() => selectQueuedModels(models), [models]);

  return {
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
