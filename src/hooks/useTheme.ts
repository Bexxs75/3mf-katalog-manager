import { uiSettings } from '../lib/uiSettings';
import { invoke } from '../lib/transport';
import { useEffect, useState, useCallback } from 'react';

export type ThemeSetting = 'system' | 'white' | 'light' | 'slate' | 'dark';
export type ResolvedTheme = 'white' | 'light' | 'slate' | 'dark';

const STORAGE_KEY = '3mf-katalog-theme';

function readStoredSetting(): ThemeSetting {
  const stored = uiSettings.getItem(STORAGE_KEY);
  return stored === 'white' || stored === 'light' || stored === 'slate' || stored === 'dark' ? stored : 'system';
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
 * App theme: "system" follows prefers-color-scheme live, "white"/"light" (Sand)/"slate"/"dark"
 * are fixed; "system" never resolves to white or slate. The choice is stored in uiSettings.
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
    if (import.meta.env.VITE_TARGET !== 'web' && '__TAURI_INTERNALS__' in window) {
      void invoke('save_window_theme', { setting, resolved }).catch(error => console.warn('[theme] saving window background failed:', error));
    }
  }, [setting, resolved]);

  const setTheme = useCallback((next: ThemeSetting) => {
    setSetting(next);
    uiSettings.setItem(STORAGE_KEY, next);
  }, []);

  return { setting, resolved, setTheme };
}
