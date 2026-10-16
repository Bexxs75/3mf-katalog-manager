import { useCallback, useState } from 'react';
export type DetailPanelMode = 'auto' | 'pinned';
const key = '3mf-katalog-detail-panel';
export function useDetailPanel() {
  const [detailPanel, setState] = useState<DetailPanelMode>(() => {
    try { return localStorage.getItem(key) === 'pinned' ? 'pinned' : 'auto'; }
    catch { return 'auto'; }
  });
  const setDetailPanel = useCallback((mode: DetailPanelMode) => {
    setState(mode);
    try { localStorage.setItem(key, mode); } catch { /* The setting still works without storage. */ }
  }, []);
  // The web catalog always keeps the panel open: an automatic panel changes the width of
  // the grid on every selection, so the tiles would jump. There is no choice to make.
  if (import.meta.env.VITE_TARGET === 'web') return { detailPanel: 'pinned' as DetailPanelMode, setDetailPanel: () => {} };
  return { detailPanel, setDetailPanel };
}
