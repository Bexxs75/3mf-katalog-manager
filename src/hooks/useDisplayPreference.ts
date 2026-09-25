import { useCallback, useState } from 'react';

export type DisplayPreference = 'thumbnail' | 'render';

const STORAGE_KEY = '3mf-katalog-display-preference';

/**
 * Ob Karten und Detailseite das eingebettete Datei-Bild ("thumbnail") oder die
 * gerenderte 3D-Ansicht ("render") bevorzugen, in localStorage gespeichert.
 * Default "thumbnail", damit sich fuer Bestandsnutzer nichts aendert.
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
