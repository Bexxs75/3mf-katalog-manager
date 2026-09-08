import { useEffect, useState, useCallback } from 'react';

export type ThemeSetting = 'system' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';

const STORAGE_KEY = '3mf-katalog-theme';

/**
 * Verwaltet das Theme der Anwendung.
 * - "system": folgt automatisch der OS Einstellung (prefers-color-scheme) und reagiert live auf Änderungen
 * - "light" / "dark": manuell fixiert, überschreibt den Systemmodus
 * Die Auswahl wird in localStorage gespeichert und beim nächsten Start wiederhergestellt.
 *
 * In der Tauri App kann prefers-color-scheme durch die native OS Theme API
 * ersetzt werden (z. B. @tauri-apps/api/window getTheme / onThemeChanged),
 * das Interface dieses Hooks bleibt dabei unverändert.
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
