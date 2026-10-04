import { isFileInFolderOrDescendant } from './folderTree';
import { tagMatches } from './autoTags';
import { applyToolView, type RecentSnapshot, type ToolView } from './toolViews';
import type { Language } from '../i18n/types';
import type { ModelFile, Folder, SortKey, SortDirection } from '../types';

export interface CatalogFilterCriteria {
  activeFolderId: string;
  activeTag: string | null;
  query: string;
  sort: SortKey;
  sortDirection?: SortDirection;
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

export function defaultSortDirection(sort: SortKey): SortDirection {
  return sort === 'name' ? 'asc' : 'desc';
}

const sortValues: Record<SortKey, (model: ModelFile) => string | number | null> = {
  name: m => m.name,
  imported: m => m.importedAt,
  modified: m => m.fileModifiedAt,
  size: m => m.fileSizeBytes,
  vol: m => m.volumeCm3,
  viewed: m => m.lastViewedAt,
};

const sortComparators: Record<SortKey, (a: ModelFile, b: ModelFile) => number> = {
  name: (a, b) => a.name.localeCompare(b.name),
  imported: (a, b) => a.importedAt.localeCompare(b.importedAt),
  modified: (a, b) => (a.fileModifiedAt ?? '').localeCompare(b.fileModifiedAt ?? ''),
  size: (a, b) => a.fileSizeBytes - b.fileSizeBytes,
  vol: (a, b) => (a.volumeCm3 ?? 0) - (b.volumeCm3 ?? 0),
  viewed: (a, b) => (a.lastViewedAt ?? '').localeCompare(b.lastViewedAt ?? ''),
};

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
  const sign = (criteria.sortDirection ?? defaultSortDirection(sort)) === 'asc' ? 1 : -1;
  return matched.sort((a, b) => {
    const x = sortValues[sort](a);
    const y = sortValues[sort](b);
    if (x == null && y == null) return a.name.localeCompare(b.name);
    if (x == null) return 1;
    if (y == null) return -1;
    return sortComparators[sort](a, b) * sign || a.name.localeCompare(b.name);
  });
}

export function selectQueuedModels(models: ModelFile[]): ModelFile[] {
  return models
    .filter((m) => m.queuePosition !== null)
    .sort((a, b) => (a.queuePosition ?? 0) - (b.queuePosition ?? 0));
}
