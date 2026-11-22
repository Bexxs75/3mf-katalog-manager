import { flushUiSettings, reloadUiSettings, reportUiSettingsRestoreError } from '../lib/uiSettings';
import { useCallback, useState } from 'react';
import * as importExportApi from '../lib/api/importExport';
import { toAppError, type AppError } from '../lib/errors';

export function useCatalogBackup() {
  const [catalogBackupError, setCatalogBackupError] = useState<AppError | null>(null);

  const exportCatalog = useCallback(() => {
    setCatalogBackupError(null);
    return flushUiSettings().then(() => importExportApi.exportCatalog()).catch((e) => {
      console.error('[catalog-backup] export failed:', e);
      setCatalogBackupError(toAppError(e));
      return false;
    });
  }, []);

  const importCatalog = useCallback((onImported: () => void) => {
    setCatalogBackupError(null);
    return flushUiSettings().then(() => importExportApi.importCatalog())
      .then(async (result) => {
        if (!result.imported) return;
        if (result.settingsError) {
          setCatalogBackupError(toAppError(result.settingsError));
          reportUiSettingsRestoreError(result.settingsError);
        }
        try { await reloadUiSettings(); }
        catch (e) { setCatalogBackupError(toAppError(e)); reportUiSettingsRestoreError(e); }
        // The backend already uses the new catalog; the caller must reload,
        // otherwise the frontend shows stale model IDs.
        onImported();
      })
      .catch((e) => {
        console.error('[catalog-backup] import failed:', e);
        setCatalogBackupError(toAppError(e));
      });
  }, []);

  const clearCatalogBackupError = useCallback(() => setCatalogBackupError(null), []);
  return { catalogBackupError, clearCatalogBackupError, exportCatalog, importCatalog };
}
