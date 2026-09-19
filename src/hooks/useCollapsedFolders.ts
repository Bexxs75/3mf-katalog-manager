import { useCallback, useState } from 'react';

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
 * Merkt sich, welche Ordner-Sektionen in der gruppierten Ansicht eingeklappt
 * sind. Persistiert in localStorage nach demselben Muster wie
 * Theme/Sprache/Slicer-Liste (siehe useSlicers.ts) - bleibt ueber
 * App-Neustarts erhalten, im Gegensatz zum ViewMode selbst.
 */
export function useCollapsedFolders() {
  const [collapsed, setCollapsed] = useState<Set<string>>(loadStored);

  const toggle = useCallback((id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      localStorage.setItem(STORAGE_KEY, JSON.stringify([...next]));
      return next;
    });
  }, []);

  const isCollapsed = useCallback((id: string) => collapsed.has(id), [collapsed]);

  return { isCollapsed, toggle };
}
