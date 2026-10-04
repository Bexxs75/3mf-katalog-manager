import { useCallback, useEffect, useState } from 'react';

export const NO_FOLDER_COLLAPSE_KEY = 'no-folder';

const STORAGE_KEY = '3mf-katalog-collapsed-folders';

function loadStored(): Set<string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? new Set(parsed.filter((x) => typeof x === 'string')) : new Set();
  } catch {
    return new Set();
  }
}

/**
 * Remembers the collapsed folder sections of the grouped view in
 * localStorage so they survive restarts.
 */
export function useCollapsedFolders() {
  const [collapsed, setCollapsed] = useState<Set<string>>(loadStored);

  useEffect(() => {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify([...collapsed])); } catch { /* Keep in-memory state when storage is unavailable. */ }
  }, [collapsed]);

  const collapseAll = useCallback((ids: string[]) => {
    setCollapsed((previous) => new Set([...previous, ...ids, NO_FOLDER_COLLAPSE_KEY]));
  }, []);
  const expandAll = useCallback(() => setCollapsed(new Set()), []);

  const toggle = useCallback((id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const isCollapsed = useCallback((id: string) => collapsed.has(id), [collapsed]);

  return { isCollapsed, toggle, collapseAll, expandAll, collapsedCount: collapsed.size };
}
