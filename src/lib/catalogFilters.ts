import { isFileInFolderOrDescendant } from './folderTree';
import type { ModelFile, Folder, SortKey } from '../types';

export interface CatalogFilterCriteria {
  activeFolderId: string;
  activeTag: string | null;
  activeCreator: string | null;
  query: string;
  sort: SortKey;
}

export function filterAndSortModels(
  models: ModelFile[],
  folders: Folder[],
  criteria: CatalogFilterCriteria,
): ModelFile[] {
  const { activeFolderId, activeTag, activeCreator, query, sort } = criteria;
  return models
    .filter((m) => activeFolderId === 'all' || isFileInFolderOrDescendant(m.folderId, activeFolderId, folders))
    .filter((m) => !activeTag || m.tags.includes(activeTag))
    .filter((m) => !activeCreator || m.creator === activeCreator)
    .filter((m) => {
      if (!query) return true;
      const q = query.toLowerCase();
      return (
        m.name.toLowerCase().includes(q) ||
        m.path.toLowerCase().includes(q) ||
        (m.creator?.toLowerCase().includes(q) ?? false) ||
        m.tags.some((tag) => tag.toLowerCase().includes(q))
      );
    })
    .sort((a, b) => {
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
