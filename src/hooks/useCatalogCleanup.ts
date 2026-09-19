import { useCallback, useState } from 'react';
import * as cleanupApi from '../lib/api/cleanup';
import * as filesApi from '../lib/api/files';
import type { CatalogIssues } from '../types';

export function useCatalogCleanup() {
  const [cleanupDialogOpen, setCleanupDialogOpen] = useState(false);
  const [cleanupIssues, setCleanupIssues] = useState<CatalogIssues | null>(null);
  const [cleanupScanning, setCleanupScanning] = useState(false);
  const [cleanupError, setCleanupError] = useState<string | null>(null);

  const scanCatalogIssues = useCallback(() => {
    setCleanupScanning(true);
    setCleanupError(null);
    return cleanupApi
      .scanCatalogIssues()
      .then((issues) => {
        setCleanupIssues(issues);
        setCleanupDialogOpen(true);
      })
      .catch((e) => {
        console.error('[cleanup] Scan fehlgeschlagen:', e);
        setCleanupError(String(e));
      })
      .finally(() => setCleanupScanning(false));
  }, []);

  const closeCleanupDialog = useCallback(() => setCleanupDialogOpen(false), []);

  const deleteSelectedCleanupFiles = useCallback(
    (fileIds: string[], onDeleted: (fileIds: string[]) => void) =>
      filesApi
        .deleteFiles(fileIds)
        .then(() => {
          onDeleted(fileIds);
          setCleanupDialogOpen(false);
          setCleanupIssues(null);
        })
        .catch((e) => {
          console.error('[cleanup] Löschen fehlgeschlagen:', e);
          setCleanupError(String(e));
        }),
    [],
  );

  return {
    cleanupDialogOpen,
    cleanupIssues,
    cleanupScanning,
    cleanupError,
    scanCatalogIssues,
    closeCleanupDialog,
    deleteSelectedCleanupFiles,
  };
}
