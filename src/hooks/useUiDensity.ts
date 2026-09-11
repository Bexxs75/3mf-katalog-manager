import { useEffect, useState, useCallback } from 'react';

export type UiDensity = 'compact' | 'comfort';

const STORAGE_KEY = '3mf-katalog-density';

/**
 * Verwaltet die UI-Dichte der Anwendung ("compact" = bestehende, dichte
 * Ansicht; "comfort" = größere Schrift/Grafiken). Persistiert in
 * localStorage nach demselben Muster wie useTheme.
 */
export function useUiDensity() {
  const [density, setDensityState] = useState<UiDensity>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === 'comfort' ? 'comfort' : 'compact';
  });

  useEffect(() => {
    document.documentElement.setAttribute('data-density', density);
  }, [density]);

  const setDensity = useCallback((next: UiDensity) => {
    setDensityState(next);
    localStorage.setItem(STORAGE_KEY, next);
  }, []);

  return { density, setDensity };
}
