import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

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

/**
 * Einmal beim Start: stellt sicher, dass ein gesetzter Speicherort eine
 * Ordnerzeile im Katalog hat. Sonst lehnt das Backend ihn als Entpack-Ziel
 * ab (z.B. nach "Bestehenden Ordner uebernehmen" ohne Modelle). Das Backend
 * legt dabei bewusst keinen inzwischen geloeschten Ordner neu an.
 */
export function useRegisterCatalogBaseDirOnStartup(catalogBaseDir: string | null, refreshFolders: () => void) {
  const done = useRef(false);
  useEffect(() => {
    if (done.current || !catalogBaseDir) return;
    done.current = true;
    invoke<unknown>('register_existing_catalog_base_dir', { path: catalogBaseDir })
      .then((folder) => {
        if (folder) refreshFolders();
      })
      .catch((e) => console.error('[catalog-base-dir] Registrieren beim Start fehlgeschlagen:', e));
  }, [catalogBaseDir, refreshFolders]);
}
