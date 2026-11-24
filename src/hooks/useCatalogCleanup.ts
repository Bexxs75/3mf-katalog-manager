import { useCallback, useRef, useState } from 'react';
import * as cleanupApi from '../lib/api/cleanup';
import { toAppError, type AppError } from '../lib/errors';
import * as filesApi from '../lib/api/files';
import { fileOpsBatch, subscribeFileOpsApplied } from '../lib/fileOps';
import type { CatalogIssues } from '../types';

const web = () => import.meta.env.VITE_TARGET === 'web';

/** What is left of the scan after some models were removed: a duplicate group of one model is no group any more. */
function withoutRemoved(issues: CatalogIssues, removed: ReadonlySet<string>): CatalogIssues {
  return {
    ...issues,
    orphaned: issues.orphaned.filter(model => !removed.has(model.id)),
    duplicateGroups: issues.duplicateGroups.map(group => group.filter(model => !removed.has(model.id))).filter(group => group.length > 1),
  };
}

export function useCatalogCleanup() {
  const [cleanupDialogOpen, setCleanupDialogOpen] = useState(false);
  const [cleanupIssues, setCleanupIssues] = useState<CatalogIssues | null>(null);
  const [cleanupScanning, setCleanupScanning] = useState(false);
  const [cleanupError, setCleanupError] = useState<AppError | null>(null);
  // What the last removal did, so the open dialog can say so; a new object per removal, so the dialog notices each one.
  const [cleanupNotice, setCleanupNotice] = useState<{ count: number } | null>(null);
  // The button that started the scan is disabled while it runs and loses the focus, so the dialog cannot find it by itself.
  const cleanupOpener = useRef<HTMLElement | null>(null);

  const scanCatalogIssues = useCallback(() => {
    cleanupOpener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setCleanupScanning(true);
    setCleanupError(null);
    setCleanupNotice(null);
    return cleanupApi
      .scanCatalogIssues()
      .then((issues) => {
        setCleanupIssues(issues);
        setCleanupDialogOpen(true);
      })
      .catch((e) => {
        console.error('[cleanup] scan failed:', e);
        setCleanupError(toAppError(e));
      })
      .finally(() => setCleanupScanning(false));
  }, []);

  const closeCleanupDialog = useCallback(() => setCleanupDialogOpen(false), []);

  // The web server takes 200 models per request and answers each one on its own, so a single failed model
  // must not make the others look removed (or the dialog vanish with the refusal).
  // The dialog stays open for what is left: removed models leave the lists, the rest can be dealt with next.
  const afterRemoval = useCallback((removed: string[]) => {
    setCleanupIssues(current => current ? withoutRemoved(current, new Set(removed)) : current);
    setCleanupNotice({ count: removed.length });
  }, []);

  const deleteInWeb = useCallback(async (fileIds: string[], onDeleted: (fileIds: string[]) => void) => {
    const removed = new Set<string>();
    const stop = subscribeFileOpsApplied(ids => ids.forEach(id => removed.add(id)));
    setCleanupError(null);
    setCleanupNotice(null);
    try {
      await fileOpsBatch('delete_files', fileIds);
      onDeleted(fileIds);
      afterRemoval(fileIds);
    } catch (e) {
      console.error('[cleanup] deleting failed:', e);
      const done = fileIds.filter(id => removed.has(id));
      if (done.length > 0) {
        onDeleted(done);
        afterRemoval(done);
      }
      setCleanupError(toAppError(e));
    } finally {
      stop();
    }
  }, [afterRemoval]);

  const deleteSelectedCleanupFiles = useCallback(
    (fileIds: string[], onDeleted: (fileIds: string[]) => void): Promise<void> => web()
      ? deleteInWeb(fileIds, onDeleted)
      : filesApi
        .deleteFiles(fileIds)
        .then(() => {
          setCleanupError(null);
          setCleanupNotice(null);
          onDeleted(fileIds);
          afterRemoval(fileIds);
        })
        .catch((e) => {
          console.error('[cleanup] deleting failed:', e);
          setCleanupError(toAppError(e));
        }),
    [deleteInWeb, afterRemoval],
  );

  return {
    cleanupDialogOpen,
    cleanupIssues,
    cleanupScanning,
    cleanupError,
    cleanupNotice,
    cleanupOpener,
    scanCatalogIssues,
    closeCleanupDialog,
    deleteSelectedCleanupFiles,
  };
}
