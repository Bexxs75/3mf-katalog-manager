import { useCallback, useEffect, useState } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import * as importExportApi from '../lib/api/importExport';
import { toAppError } from '../lib/errors';
import type { ArchiveImportResult, ArchiveInfo, ArchiveOutcome, ImportResultDto, SkippedFile } from '../types';

interface UseFileImportArgs {
  enabled: boolean;
  catalogBaseDir: string | null;
  activeFolderId: string;
  onImported: (result: ImportResultDto) => void;
  refreshFolders: () => void;
  refreshFiles: () => void;
}

export function useFileImport({
  enabled,
  activeFolderId,
  onImported,
  refreshFolders,
  refreshFiles,
}: UseFileImportArgs) {
  const [importBanner, setImportBanner] = useState<
    { imported: number; duplicates: number; archives?: ArchiveOutcome[]; skipped?: SkippedFile[] } | null
  >(null);
  const [pendingArchives, setPendingArchives] = useState<ArchiveInfo[] | null>(null);

  const mergeImported = useCallback(
    (result: ImportResultDto) => {
      onImported(result);
      const skipped = result.skipped ?? [];
      if (result.duplicateCount > 0 || skipped.length > 0) {
        setImportBanner({ imported: result.imported.length, duplicates: result.duplicateCount, skipped });
      }
    },
    [onImported],
  );

  const openArchiveDialog = useCallback(async (result: ImportResultDto) => {
    const paths = result.pendingArchives ?? [];
    if (paths.length === 0) return;
    try {
      const inspected = await importExportApi.inspectArchives(paths);
      // Another import while the dialog is open extends the list instead of
      // replacing it - archives already shown stay unchanged.
      setPendingArchives((prev) => {
        if (!prev) return inspected;
        const known = new Set(prev.map((a) => a.path));
        return [...prev, ...inspected.filter((a) => !known.has(a.path))];
      });
    } catch (e) {
      const err = toAppError(e);
      setImportBanner({
        imported: result.imported.length,
        duplicates: result.duplicateCount,
        skipped: result.skipped,
        archives: paths.map((path) => ({
          path,
          extractedTo: null,
          strippedRoot: null,
          existingSkipped: 0,
          unsafeSkipped: 0,
          blockedSkipped: 0,
          archiveDeleted: false,
          deleteError: null,
          error: err.message,
          unexpected: err.unexpected,
        })),
      });
    }
  }, []);

  const finishArchives = useCallback(
    (result: ArchiveImportResult) => {
      setPendingArchives(null);
      onImported({ imported: result.imported, duplicateCount: result.duplicateCount, skipped: result.skipped });
      setImportBanner({
        imported: result.imported.length,
        duplicates: result.duplicateCount,
        archives: result.archives,
        skipped: result.skipped,
      });
      refreshFolders();
      refreshFiles();
    },
    [onImported, refreshFolders, refreshFiles],
  );

  const cancelArchives = useCallback(() => {
    if (pendingArchives?.length) void importExportApi.discardArchiveImports(pendingArchives.map(a => a.path)).catch(console.error);
    setPendingArchives(null);
  }, [pendingArchives]);

  const importFiles = useCallback(
    () =>
      importExportApi.importFiles(activeFolderId === 'all' ? undefined : activeFolderId).then(async (result) => {
        mergeImported(result);
        await openArchiveDialog(result);
      }).finally(() => { refreshFolders(); refreshFiles(); }),
    [activeFolderId, mergeImported, openArchiveDialog, refreshFolders, refreshFiles],
  );

  const importFolder = useCallback(
    () => importExportApi.importFolder().then(mergeImported).finally(() => { refreshFolders(); refreshFiles(); }),
    [mergeImported, refreshFolders, refreshFiles],
  );

  const dismissImportBanner = useCallback(() => setImportBanner(null), []);

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== 'drop') return;
      if (!enabled) return;
      importExportApi.importDropped(event.payload.paths).then(async (result) => {
        mergeImported(result);
        await openArchiveDialog(result);
      }).finally(() => { refreshFolders(); refreshFiles(); });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [enabled, mergeImported, openArchiveDialog, refreshFolders, refreshFiles]);

  // mergeImported is exported as well so imports triggered outside
  // this hook (first-run dialog) go through the same duplicate banner
  // logic as the imports here.
  return {
    importBanner,
    dismissImportBanner,
    mergeImported,
    importFiles,
    importFolder,
    pendingArchives,
    finishArchives,
    cancelArchives,
  };
}
