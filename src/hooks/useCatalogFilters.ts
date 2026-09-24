import { useMemo, useState } from 'react';
import type { Language } from '../i18n/types';
import type { ModelFile, Folder, ViewMode, SortKey } from '../types';
import type { ToolView } from '../lib/toolViews';
import { filterAndSortModels, selectQueuedModels } from '../lib/catalogFilters';

export function useCatalogFilters(models: ModelFile[], folders: Folder[], language: Language = 'de') {
  const [view, setView] = useState<ViewMode>('grid');
  const [sort, setSort] = useState<SortKey>('name');
  const [query, setQuery] = useState('');
  const [activeFolderId, setActiveFolderId] = useState('all');
  const [activeTag, setActiveTag] = useState<string | null>(null);
  // activeCreator wird derzeit nirgends im UI gesetzt (siehe Konsistenz-
  // Hinweis in App.tsx vor diesem Refactor) - beibehalten, um Verhalten
  // exakt gleich zu lassen.
  const [activeCreator] = useState<string | null>(null);
  const [toolView, setToolView] = useState<ToolView | null>(null);

  const filtered = useMemo(
    () => filterAndSortModels(models, folders, { activeFolderId, activeTag, activeCreator, query, sort, language, toolView }),
    [models, folders, activeFolderId, activeTag, activeCreator, query, sort, language, toolView],
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
