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
  return { detailPanel, setDetailPanel };
}
