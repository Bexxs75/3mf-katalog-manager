import { useCallback, useState } from 'react';
import { loadDetailPanelWidth, saveDetailPanelWidth } from '../lib/detailPanelWidthPreference';

export function useDetailPanelWidth() {
  const [width, setWidthState] = useState(loadDetailPanelWidth);
  const setWidth = useCallback((px: number, persist = true) => {
    setWidthState(px);
    if (persist) saveDetailPanelWidth(px);
  }, []);
  const reset = useCallback(() => {
    setWidthState(null);
    saveDetailPanelWidth(null);
  }, []);
  return { width, setWidth, reset };
}
