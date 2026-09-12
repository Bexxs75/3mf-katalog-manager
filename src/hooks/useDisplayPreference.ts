import { useCallback, useState } from 'react';

export type DisplayPreference = 'thumbnail' | 'render';

const STORAGE_KEY = '3mf-katalog-display-preference';

/**
 * Verwaltet die globale Einstellung, ob Katalog-Karten und die Modell-
 * Detailseite standardmaessig das eingebettete Datei-Bild ("thumbnail")
 * oder eine gerenderte 3D-Ansicht ("render") bevorzugen. Persistiert in
 * localStorage nach demselben Muster wie Theme/Dichte (useTheme.ts).
 * Default ist "thumbnail" - entspricht dem Verhalten vor Einfuehrung
 * dieser Einstellung, damit Bestandsnutzer keine Aenderung ohne aktives
 * Zutun erleben.
 */
export function useDisplayPreference() {
  const [preference, setPreferenceState] = useState<DisplayPreference>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === 'render' ? 'render' : 'thumbnail';
  });

  const setPreference = useCallback((next: DisplayPreference) => {
    setPreferenceState(next);
    localStorage.setItem(STORAGE_KEY, next);
  }, []);

  return { preference, setPreference };
}
