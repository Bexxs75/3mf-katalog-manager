import { useCallback, useState } from 'react';
export type DetailPanelMode = 'auto' | 'pinned';
const key = '3mf-katalog-detail-panel';
export function useDetailPanel() {
  const [detailPanel, setState] = useState<DetailPanelMode>(() => {
    // The web catalog keeps the panel pinned by default, so the grid keeps its width
    // and tiles do not jump when a model is selected. An explicit choice always wins.
    const fallback: DetailPanelMode = import.meta.env.VITE_TARGET === 'web' ? 'pinned' : 'auto';
    try {
      const stored = localStorage.getItem(key);
      return stored === 'pinned' || stored === 'auto' ? stored : fallback;
    } catch { return fallback; }
  });
  const setDetailPanel = useCallback((mode: DetailPanelMode) => {
    setState(mode);
    try { localStorage.setItem(key, mode); } catch { /* The setting still works without storage. */ }
  }, []);
  return { detailPanel, setDetailPanel };
}
