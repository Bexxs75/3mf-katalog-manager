import { useEffect, useState, useCallback } from 'react';

export type ThemeSetting = 'system' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';

const STORAGE_KEY = '3mf-katalog-theme';

function readStoredSetting(): ThemeSetting {
  const stored = localStorage.getItem(STORAGE_KEY);
  return (stored as ThemeSetting) || 'system';
}

function readSystemTheme(): ResolvedTheme {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

/**
 * Resolves the theme the same way useTheme() does, without subscribing to
 * changes. For one-off reads outside the hook - e.g. the crash fallback,
 * which can render before useTheme()'s own effect has set `data-app`.
 */
export function resolveTheme(): ResolvedTheme {
  const setting = readStoredSetting();
  return setting === 'system' ? readSystemTheme() : setting;
}

/**
 * App theme: "system" follows prefers-color-scheme live, "light"/"dark"
 * are fixed. The choice is stored in localStorage.
 */
export function useTheme() {
  const [setting, setSetting] = useState<ThemeSetting>(readStoredSetting);

  const [systemTheme, setSystemTheme] = useState<ResolvedTheme>(readSystemTheme);

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
