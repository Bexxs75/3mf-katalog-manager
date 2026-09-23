import { useCallback, useEffect, useState } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import * as importExportApi from '../lib/api/importExport';
import * as foldersApi from '../lib/api/folders';
import type { ArchiveImportResult, ArchiveInfo, ArchiveOutcome, ImportResultDto, Folder } from '../types';

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
    { imported: number; duplicates: number; archives?: ArchiveOutcome[] } | null
  >(null);
  const [pendingArchives, setPendingArchives] = useState<ArchiveInfo[] | null>(null);

  const mergeImported = useCallback(
    (result: ImportResultDto) => {
      onImported(result);
      if (result.duplicateCount > 0) {
        setImportBanner({ imported: result.imported.length, duplicates: result.duplicateCount });
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
            console.error('[import] Einsortieren fehlgeschlagen:', e),
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
    setPendingArchives(await importExportApi.inspectArchives(paths));
  }, []);

  const finishArchives = useCallback(
    (result: ArchiveImportResult) => {
      setPendingArchives(null);
      onImported({ imported: result.imported, duplicateCount: result.duplicateCount });
      setImportBanner({
        imported: result.imported.length,
        duplicates: result.duplicateCount,
        archives: result.archives,
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

  // mergeImported wird zusaetzlich exportiert, damit Importe, die ausserhalb
  // dieses Hooks ausgeloest werden (Ersteinrichtungsdialog), dieselbe
  // Duplikat-Banner-Logik durchlaufen wie die Importe hier.
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
