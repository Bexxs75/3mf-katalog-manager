import { invoke } from '../transport';

export interface DiskEntry { path: string; relativePath: string; root: string; id: string | null }
export interface DiskScan {
  newFolders: DiskEntry[];
  newFiles: DiskEntry[];
  missingFiles: DiskEntry[];
  unreachableRoots: string[];
  skippedPaths: string[];
}
export interface DiskFailedPath { path: string; reason: string }
export interface DiskApplied { fileIds: string[]; folderIds: string[]; removedCount: number; failedPaths: DiskFailedPath[] }
export const scanCatalogDisk = () => invoke<DiskScan>('scan_catalog_disk');
export const applyCatalogDiskSync = (scan: DiskScan, removeMissing: boolean) => invoke<DiskApplied>('apply_catalog_disk_sync', {
  selection: {
    newPaths: [...scan.newFolders, ...scan.newFiles].map(e => e.path),
    missingIds: scan.missingFiles.flatMap(e => e.id ? [e.id] : []),
    removeMissing,
  },
});

export const diskSyncErrorCodes = ['diskSyncStale', 'diskSyncPathChanged', 'diskSyncRootUnreachable', 'diskSyncImportFailed', 'diskSyncFailed'] as const;
export type DiskSyncErrorCode = typeof diskSyncErrorCodes[number] | 'importLockedHint';
export interface DiskSyncError { code: DiskSyncErrorCode; paths: string[] }
export function diskSyncError(error: unknown): DiskSyncError {
  const value = error as { code?: unknown; message?: unknown; paths?: unknown } | null;
  const code = value?.code ?? value?.message ?? error;
  return {
    code: code === 'importActive' ? 'importLockedHint' : diskSyncErrorCodes.find(c => c === code) ?? 'diskSyncFailed',
    paths: Array.isArray(value?.paths) ? value.paths.filter((p): p is string => typeof p === 'string') : [],
  };
}
