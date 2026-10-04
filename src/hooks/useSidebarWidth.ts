import { useCallback, useEffect, useState } from 'react';

export const SIDEBAR_MIN = 180;
export const SIDEBAR_MAX = 420;
export const SIDEBAR_DEFAULT = 242;
export const SIDEBAR_STEP = 16;
const STORAGE_KEY = '3mf-katalog-sidebar-width';
const clamp = (value: number) => Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, value));

function loadStored(): number {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null');
    if (typeof value === 'number' && Number.isFinite(value)) return clamp(value);
  } catch { /* Invalid or unavailable storage uses the default. */ }
  return SIDEBAR_DEFAULT;
}

export function useSidebarWidth() {
  const [requestedWidth, setRequestedWidth] = useState(loadStored);
  const [windowWidth, setWindowWidth] = useState(() => window.innerWidth);
  useEffect(() => {
    const resize = () => setWindowWidth(window.innerWidth);
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, []);
  useEffect(() => {
    try { localStorage.setItem(STORAGE_KEY, String(requestedWidth)); } catch { /* Keep in-memory state when storage is unavailable. */ }
  }, [requestedWidth]);
  const setWidth = useCallback((value: number) => {
    if (Number.isFinite(value)) setRequestedWidth(clamp(value));
  }, []);
  const reset = useCallback(() => setRequestedWidth(SIDEBAR_DEFAULT), []);
  const width = Math.min(requestedWidth, Math.max(SIDEBAR_MIN, windowWidth - 60 - 480));
  return { width, setWidth, reset };
}
