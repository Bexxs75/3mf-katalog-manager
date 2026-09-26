import { useMemo, useState } from 'react';
import type { Language } from '../i18n/types';
import type { ModelFile, Folder, ViewMode, SortKey } from '../types';
import type { RecentSnapshot, ToolView } from '../lib/toolViews';
import { filterAndSortModels, selectQueuedModels } from '../lib/catalogFilters';

export function useCatalogFilters(models: ModelFile[], folders: Folder[], language: Language = 'de') {
  const [view, setView] = useState<ViewMode>('grid');
  const [sort, setSort] = useState<SortKey>('name');
  const [query, setQuery] = useState('');
  const [activeFolderId, setActiveFolderId] = useState('all');
  const [activeTag, setActiveTag] = useState<string | null>(null);
  // activeCreator is currently not set anywhere in the UI.
  const [activeCreator] = useState<string | null>(null);
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
        activeCreator,
        query,
        sort,
        language,
        toolView,
        recentSnapshot: recentSnapshot ?? undefined,
      }),
    [models, folders, activeFolderId, activeTag, activeCreator, query, sort, language, toolView, recentSnapshot],
  );

  const queue = useMemo(() => selectQueuedModels(models), [models]);

  return {
    view, setView,
    sort, setSort,
    query, setQuery,
    activeFolderId, setActiveFolderId,
    activeTag, setActiveTag,
    activeCreator,
    toolView, setToolView,
    filtered,
    queue,
  };
}
