import { useCallback, useEffect, useState } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import * as importExportApi from '../lib/api/importExport';
import * as foldersApi from '../lib/api/folders';
import { toAppError } from '../lib/errors';
import type { ArchiveImportResult, ArchiveInfo, ArchiveOutcome, ImportResultDto, Folder, SkippedFile } from '../types';

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
  catalogBaseDir,
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

  const autoFileIntoBaseDir = useCallback(
    async (result: ImportResultDto) => {
      if (!catalogBaseDir || result.imported.length === 0) return;
      let targetFolder: string | undefined = activeFolderId !== 'all' ? activeFolderId : undefined;
      if (!targetFolder) {
        const freshFolders: Folder[] = await foldersApi.listFolders();
        targetFolder = freshFolders.find((f) => f.path === catalogBaseDir && !f.parentId)?.id;
      }
      if (!targetFolder) return;
      await Promise.all(
        result.imported.map((file) =>
          foldersApi.moveFileToFolder(file.id, targetFolder!).catch((e) =>
            console.error('[import] placing into folder failed:', e),
          ),
        ),
      );
      refreshFolders();
      refreshFiles();
    },
    [catalogBaseDir, activeFolderId, refreshFolders, refreshFiles],
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
    },
    [onImported, refreshFolders],
  );

  const cancelArchives = useCallback(() => setPendingArchives(null), []);

  const importFiles = useCallback(
    () =>
      importExportApi.importFiles().then(async (result) => {
        mergeImported(result);
        await autoFileIntoBaseDir(result);
        await openArchiveDialog(result);
      }),
    [mergeImported, autoFileIntoBaseDir, openArchiveDialog],
  );

  const importFolder = useCallback(
    () => importExportApi.importFolder().then(mergeImported),
    [mergeImported],
  );

  const dismissImportBanner = useCallback(() => setImportBanner(null), []);

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== 'drop') return;
      if (!enabled) return;
      importExportApi.importDropped(event.payload.paths).then(async (result) => {
        mergeImported(result);
        await openArchiveDialog(result);
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [enabled, mergeImported, openArchiveDialog]);

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
