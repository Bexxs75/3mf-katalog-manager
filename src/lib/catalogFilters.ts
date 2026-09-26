import { isFileInFolderOrDescendant } from './folderTree';
import { tagMatches } from './autoTags';
import { applyToolView, type RecentSnapshot, type ToolView } from './toolViews';
import type { Language } from '../i18n/types';
import type { ModelFile, Folder, SortKey } from '../types';

export interface CatalogFilterCriteria {
  activeFolderId: string;
  activeTag: string | null;
  query: string;
  sort: SortKey;
  // For searching in translated names of automatic tags.
  language?: Language;
  // Tool view of the sidebar; determines selection AND order.
  toolView?: ToolView | null;
  // Reference time for "Recently added" (tests pin it).
  now?: Date;
  // Frozen lastViewedAt state for the "recent" view (see
  // useCatalogFilters); only relevant when toolView === 'recent'.
  recentSnapshot?: RecentSnapshot;
}

export function filterAndSortModels(
  models: ModelFile[],
  folders: Folder[],
  criteria: CatalogFilterCriteria,
): ModelFile[] {
  const { activeFolderId, activeTag, query, sort, language = 'de', toolView = null, now, recentSnapshot } = criteria;
  const base = toolView ? applyToolView(models, toolView, now ?? new Date(), recentSnapshot) : models;
  const matched = base
    .filter((m) => activeFolderId === 'all' || isFileInFolderOrDescendant(m.folderId, activeFolderId, folders))
    .filter((m) => !activeTag || m.tags.includes(activeTag))
    .filter((m) => {
      if (!query) return true;
      const q = query.toLowerCase();
      return (
        m.name.toLowerCase().includes(q) ||
        m.path.toLowerCase().includes(q) ||
        (m.creator?.toLowerCase().includes(q) ?? false) ||
        m.tags.some((tag) => tagMatches(tag, q, language))
      );
    });
  if (toolView) return matched;
  return matched.sort((a, b) => {
    if (sort === 'name') return a.name.localeCompare(b.name);
    if (sort === 'size') return a.fileSizeBytes - b.fileSizeBytes;
    if (sort === 'date') return b.importedAt.localeCompare(a.importedAt);
    if (sort === 'viewed') return (b.lastViewedAt ?? '').localeCompare(a.lastViewedAt ?? '');
    return 0;
  });
}

export function selectQueuedModels(models: ModelFile[]): ModelFile[] {
  return models
    .filter((m) => m.queuePosition !== null)
    .sort((a, b) => (a.queuePosition ?? 0) - (b.queuePosition ?? 0));
}
