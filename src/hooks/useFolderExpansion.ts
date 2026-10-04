import { useCallback, useEffect, useState } from 'react';

const STORAGE_KEY = '3mf-katalog-sidebar-expanded';

function loadStored(): Set<string> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '[]');
    return Array.isArray(parsed) ? new Set(parsed.filter((id): id is string => typeof id === 'string')) : new Set();
  } catch {
    return new Set();
  }
}

export function useFolderExpansion() {
  const [expanded, setExpanded] = useState(loadStored);
  useEffect(() => {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify([...expanded])); } catch { /* Keep in-memory state when storage is unavailable. */ }
  }, [expanded]);
  const isExpanded = useCallback((id: string) => expanded.has(id), [expanded]);
  const toggle = useCallback((id: string) => setExpanded((previous) => {
    const next = new Set(previous);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return next;
  }), []);
  const expand = useCallback((id: string) => setExpanded((previous) => new Set([...previous, id])), []);
  const setAll = useCallback((ids: string[], open: boolean) => setExpanded(new Set(open ? ids : [])), []);
  return { expanded, isExpanded, toggle, expand, setAll };
}
