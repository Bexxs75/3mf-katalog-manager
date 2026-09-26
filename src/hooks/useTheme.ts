import { useEffect, useState, useCallback } from 'react';

export type ThemeSetting = 'system' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';

const STORAGE_KEY = '3mf-katalog-theme';

/**
 * App theme: "system" follows prefers-color-scheme live, "light"/"dark"
 * are fixed. The choice is stored in localStorage.
 */
export function useTheme() {
  const [setting, setSetting] = useState<ThemeSetting>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    return (stored as ThemeSetting) || 'system';
  });

  const [systemTheme, setSystemTheme] = useState<ResolvedTheme>(() =>
    window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  );

  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => setSystemTheme(e.matches ? 'dark' : 'light');
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, []);

  const resolved: ResolvedTheme = setting === 'system' ? systemTheme : setting;

  useEffect(() => {
    document.documentElement.setAttribute('data-app', resolved);
  }, [resolved]);

  const setTheme = useCallback((next: ThemeSetting) => {
    setSetting(next);
    localStorage.setItem(STORAGE_KEY, next);
  }, []);

  return { setting, resolved, setTheme };
}
