import { uiSettings } from '../lib/uiSettings';
import { useCallback, useState } from 'react';
export type DetailPanelMode = 'auto' | 'pinned';
const key = '3mf-katalog-detail-panel';
export function useDetailPanel() {
  const [detailPanel, setState] = useState<DetailPanelMode>(() => {
    try { return uiSettings.getItem(key) === 'pinned' ? 'pinned' : 'auto'; }
    catch { return 'auto'; }
  });
  const [defaultDetailPanel, setDefaultState] = useState(detailPanel);
  const setDetailPanel = useCallback((mode: DetailPanelMode) => {
    setState(mode);
  }, []);
  const setDefaultDetailPanel = useCallback((mode: DetailPanelMode) => {
    setDefaultState(mode);
    try { uiSettings.setItem(key, mode); }
    catch { /* storage unavailable: the default then only holds for this session */ }
  }, []);
  return { detailPanel, setDetailPanel, defaultDetailPanel, setDefaultDetailPanel };
}
