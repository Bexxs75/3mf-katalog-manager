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
  // activeCreator wird derzeit nirgends im UI gesetzt (siehe Konsistenz-
  // Hinweis in App.tsx vor diesem Refactor) - beibehalten, um Verhalten
  // exakt gleich zu lassen.
  const [activeCreator] = useState<string | null>(null);
  const [toolView, setToolViewState] = useState<ToolView | null>(null);
  // Eingefroren beim (Re-)Aktivieren der "recent"-Ansicht, damit ein Klick auf
  // ein Modell (der lastViewedAt aktualisiert) die Reihenfolge nicht sofort
  // veraendert - siehe Finding 1 im Abschluss-Review vom 2026-09-24.
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
