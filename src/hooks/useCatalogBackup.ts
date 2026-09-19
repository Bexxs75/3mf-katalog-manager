import { useCallback, useState } from 'react';
import * as importExportApi from '../lib/api/importExport';

const CATALOG_SETTINGS_KEYS = [
  '3mf-katalog-theme',
  '3mf-katalog-display-preference',
  '3mf-katalog-language',
  '3mf-katalog-slicers',
  '3mf-katalog-density',
] as const;

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
          try {
            const settings = JSON.parse(result.settingsJson) as Record<string, string | null>;
            for (const key of CATALOG_SETTINGS_KEYS) {
              const value = settings[key];
              if (value === null || value === undefined) {
                localStorage.removeItem(key);
              } else {
                localStorage.setItem(key, value);
              }
            }
          } catch (e) {
            console.error('[catalog-backup] Einstellungen konnten nicht wiederhergestellt werden:', e);
          }
        }
        onImported();
      })
      .catch((e) => {
        console.error('[catalog-backup] Import fehlgeschlagen:', e);
        setCatalogBackupError(String(e));
      });
  }, []);

  return { catalogBackupError, exportCatalog, importCatalog };
}
