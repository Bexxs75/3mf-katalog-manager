import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

const BASE_DIR_KEY = '3mf-katalog-base-dir';
const SETUP_SEEN_KEY = '3mf-katalog-setup-seen';

/**
 * Manages the optional catalog location (target folder for newly
 * imported single files) and whether the first-run dialog has already
 * been seen/decided. Persisted in localStorage following the same
 * pattern as theme/density/display preference (see useDisplayPreference.ts).
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
 * Once at startup: makes sure a configured location has a folder row
 * in the catalog. Otherwise the backend rejects it as an extraction target
 * (e.g. after "Adopt existing folder" without models). The backend
 * deliberately doesn't recreate a folder that was deleted in the meantime.
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
