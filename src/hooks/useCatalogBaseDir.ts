import { useCallback, useState } from 'react';

const BASE_DIR_KEY = '3mf-katalog-base-dir';
const SETUP_SEEN_KEY = '3mf-katalog-setup-seen';

/**
 * Verwaltet den optionalen Katalog-Speicherort (Zielordner fuer neu
 * importierte Einzeldateien) und ob der Ersteinrichtungsdialog schon
 * gesehen/entschieden wurde. Persistiert in localStorage nach demselben
 * Muster wie Theme/Dichte/Anzeige-Praeferenz (siehe useDisplayPreference.ts).
 */
export function useCatalogBaseDir() {
  const [catalogBaseDir, setCatalogBaseDirState] = useState<string | null>(() =>
    localStorage.getItem(BASE_DIR_KEY),
  );
  const [setupSeen, setSetupSeenState] = useState<boolean>(
    () => localStorage.getItem(SETUP_SEEN_KEY) === '1',
  );

  const setCatalogBaseDir = useCallback((path: string | null) => {
    setCatalogBaseDirState(path);
    if (path) localStorage.setItem(BASE_DIR_KEY, path);
    else localStorage.removeItem(BASE_DIR_KEY);
  }, []);

  const markSetupSeen = useCallback(() => {
    setSetupSeenState(true);
    localStorage.setItem(SETUP_SEEN_KEY, '1');
  }, []);

  return { catalogBaseDir, setCatalogBaseDir, setupSeen, markSetupSeen };
}
