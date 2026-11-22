import { uiSettings } from '../lib/uiSettings';
import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '../lib/transport';

const BASE_DIR_KEY = '3mf-katalog-base-dir';
const SETUP_SEEN_KEY = '3mf-katalog-setup-seen';

/**
 * Manages the optional catalog location (target folder for newly
 * imported single files) and whether the first-run dialog has already
 * been seen/decided. Persisted in uiSettings following the same
 * pattern as theme/density/display preference (see useDisplayPreference.ts).
 */
export function useCatalogBaseDir() {
  const [catalogBaseDir, setCatalogBaseDirState] = useState<string | null>(() =>
    uiSettings.getItem(BASE_DIR_KEY),
  );
  const [setupSeen, setSetupSeenState] = useState<boolean>(
    () => uiSettings.getItem(SETUP_SEEN_KEY) === '1',
  );

  const setCatalogBaseDir = useCallback((path: string | null) => {
    setCatalogBaseDirState(path);
    if (path) uiSettings.setItem(BASE_DIR_KEY, path);
    else uiSettings.removeItem(BASE_DIR_KEY);
  }, []);

  const markSetupSeen = useCallback(() => {
    setSetupSeenState(true);
    uiSettings.setItem(SETUP_SEEN_KEY, '1');
  }, []);

  const resetCatalogSetup = useCallback(() => {
    setCatalogBaseDir(null);
    setSetupSeenState(false);
    uiSettings.removeItem(SETUP_SEEN_KEY);
  }, [setCatalogBaseDir]);

  return { catalogBaseDir, setCatalogBaseDir, setupSeen, markSetupSeen, resetCatalogSetup };
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
    if (import.meta.env.VITE_TARGET === 'web' || done.current || !catalogBaseDir) return;
    done.current = true;
    invoke<unknown>('register_existing_catalog_base_dir', { path: catalogBaseDir })
      .then((folder) => {
        if (folder) refreshFolders();
      })
      .catch((e) => console.error('[catalog-base-dir] registering at startup failed:', e));
  }, [catalogBaseDir, refreshFolders]);
}
