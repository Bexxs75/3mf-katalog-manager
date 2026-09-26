import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react';

export type UiDensity = 'compact' | 'comfort';

const STORAGE_KEY = '3mf-katalog-density';

interface UiDensityContextValue {
  density: UiDensity;
  setDensity: (next: UiDensity) => void;
}

const UiDensityContext = createContext<UiDensityContextValue | null>(null);

/**
 * Manages the app's UI density ("compact" = existing, dense view;
 * "comfort" = larger text/graphics). Persisted in localStorage
 * following the same pattern as useTheme/LanguageContext.
 */
export function UiDensityProvider({ children }: { children: ReactNode }) {
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

  return (
    <UiDensityContext.Provider value={{ density, setDensity }}>
      {children}
    </UiDensityContext.Provider>
  );
}

export function useUiDensity(): UiDensityContextValue {
  const ctx = useContext(UiDensityContext);
  if (!ctx) throw new Error('useUiDensity must be used within a UiDensityProvider');
  return ctx;
}
