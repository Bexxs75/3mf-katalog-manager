import { useCallback, useState } from 'react';
export type DetailPanelMode = 'auto' | 'pinned';
// The web build uses its own key: an automatic mode stored by the older web builds
// (where "automatic" was the default) must not override the new pinned default.
const storageKey = () => import.meta.env.VITE_TARGET === 'web' ? '3mf-katalog-detail-panel-web' : '3mf-katalog-detail-panel';
export function useDetailPanel() {
  const [detailPanel, setState] = useState<DetailPanelMode>(() => {
    // The web catalog keeps the panel pinned by default, so the grid keeps its width
    // and tiles do not jump when a model is selected. An explicit choice always wins.
    const fallback: DetailPanelMode = import.meta.env.VITE_TARGET === 'web' ? 'pinned' : 'auto';
    try {
      const stored = localStorage.getItem(storageKey());
      return stored === 'pinned' || stored === 'auto' ? stored : fallback;
    } catch { return fallback; }
  });
  const setDetailPanel = useCallback((mode: DetailPanelMode) => {
    setState(mode);
    try { localStorage.setItem(storageKey(), mode); } catch { /* The setting still works without storage. */ }
  }, []);
  return { detailPanel, setDetailPanel };
}
