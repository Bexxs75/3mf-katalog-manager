import { useCallback, useState } from 'react';
import * as importExportApi from '../lib/api/importExport';

const CATALOG_SETTINGS_KEYS = [
  '3mf-katalog-theme',
  '3mf-katalog-display-preference',
  '3mf-katalog-language',
  '3mf-katalog-density',
] as const;

/**
 * Keys from older backups that are NEVER restored on import.
 * `3mf-katalog-slicers` contained program paths that are later launched
 * as processes; a crafted backup could store `/bin/sh` or similar
 * there. The slicer registry now lives in the backend.
 */
const IMPORT_SKIPPED_SETTINGS_KEYS: ReadonlySet<string> = new Set(['3mf-katalog-slicers']);

/**
 * Allowed values per setting (mirrored from useTheme, useDisplayPreference,
 * LanguageContext, UiDensityContext). Other values from a foreign backup
 * are silently skipped; the import still counts as successful.
 */
const IMPORT_ALLOWED_SETTINGS_VALUES: Record<string, readonly string[]> = {
  '3mf-katalog-theme': ['system', 'light', 'dark'],
  '3mf-katalog-display-preference': ['thumbnail', 'render'],
  '3mf-katalog-language': ['de', 'en', 'es', 'fr'],
  '3mf-katalog-density': ['compact', 'comfort'],
};

export function useCatalogBackup() {
  const [catalogBackupError, setCatalogBackupError] = useState<string | null>(null);

  const exportCatalog = useCallback(() => {
    setCatalogBackupError(null);
    const settings: Record<string, string | null> = {};
    for (const key of CATALOG_SETTINGS_KEYS) {
      settings[key] = localStorage.getItem(key);
    }
    return importExportApi.exportCatalog(JSON.stringify(settings)).catch((e) => {
      console.error('[catalog-backup] Export fehlgeschlagen:', e);
      setCatalogBackupError(String(e));
    });
  }, []);

  const importCatalog = useCallback((onImported: () => void) => {
    setCatalogBackupError(null);
    return importExportApi
      .importCatalog()
      .then((result) => {
        if (!result.imported) return;
        if (result.settingsJson) {
          // An error with the settings must not make the successful catalog import
          // look like a failure.
          try {
            const settings = JSON.parse(result.settingsJson) as Record<string, string | null>;
            for (const key of CATALOG_SETTINGS_KEYS) {
              if (IMPORT_SKIPPED_SETTINGS_KEYS.has(key)) continue;
              const value = settings[key];
              if (value === null || value === undefined) {
                localStorage.removeItem(key);
                continue;
              }
              const allowed = IMPORT_ALLOWED_SETTINGS_VALUES[key];
              if (allowed && !allowed.includes(value)) {
                console.warn(`[catalog-backup] Ungueltiger Wert fuer ${key} im Backup, wird ignoriert.`);
                continue;
              }
              localStorage.setItem(key, value);
            }
          } catch (e) {
            console.error('[catalog-backup] Einstellungen konnten nicht wiederhergestellt werden:', e);
          }
        }
        // The backend already uses the new catalog; the caller must reload,
        // otherwise the frontend shows stale model IDs.
        onImported();
      })
      .catch((e) => {
        console.error('[catalog-backup] Import fehlgeschlagen:', e);
        setCatalogBackupError(String(e));
      });
  }, []);

  return { catalogBackupError, exportCatalog, importCatalog };
}
