import { invoke } from '../transport';

export interface DiskEntry { path: string; relativePath: string; root: string; id: string | null }
export interface DiskRename {
  id: string; oldPath: string; oldRelativePath: string; oldRoot: string;
  newPath: string; newRelativePath: string; root: string;
}
/** A catalog folder whose directory is gone from disk; `modelCount` includes subfolders, not the trash. */
export interface DiskFolderEntry { id: string; path: string; relativePath: string; root: string; modelCount: number }
export interface DiskDuplicate extends DiskEntry { duplicateOfName: string; duplicateOfPath: string }
export interface DiskScan {
  newFolders: DiskEntry[];
  newFiles: DiskEntry[];
  renamedFiles: DiskRename[];
  duplicateFiles: DiskDuplicate[];
  missingFiles: DiskEntry[];
  missingFolders: DiskFolderEntry[];
  unreachableRoots: string[];
  skippedPaths: string[];
}
export interface DiskFailedPath { path: string; reason: string }
export interface DiskApplied { fileIds: string[]; folderIds: string[]; renamedIds: string[]; renamedCount: number; removedCount: number; failedPaths: DiskFailedPath[] }
/** What the user ticked: paths of new files/copies and ids of renamed rows. */
export interface DiskPicked { paths: ReadonlySet<string>; renameIds: ReadonlySet<string> }
export const scanCatalogDisk = () => invoke<DiskScan>('scan_catalog_disk');
export const applyCatalogDiskSync = (scan: DiskScan, removeMissing: boolean, picked: DiskPicked) => invoke<DiskApplied>('apply_catalog_disk_sync', {
  selection: {
    // Folders are created for the files below them; one is sent only when a ticked file lies in it.
    newPaths: [
      ...scan.newFolders.filter(e => [...picked.paths].some(p => p.startsWith(e.path) && /[\\/]/.test(p.charAt(e.path.length)))).map(e => e.path),
      ...[...scan.newFiles, ...scan.duplicateFiles].filter(e => picked.paths.has(e.path)).map(e => e.path),
    ],
    missingIds: scan.missingFiles.flatMap(e => e.id ? [e.id] : []),
    removeMissing,
    renames: scan.renamedFiles.filter(r => picked.renameIds.has(r.id)).map(r => ({ id: r.id, newPath: r.newPath })),
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

/** What the start sync did; `applied` is set only when the automatic take-over was on. */
export interface StartSyncResult {
  ran: boolean; busy: boolean;
  newFiles: number; renamed: number; missingFiles: number; missingFolders: number; duplicates: number; unreachableRoots: number;
  applied: { added: number; renamed: number; failed: number } | null;
}
export const runStartSync = () => invoke<StartSyncResult>('run_start_sync');
export const getSyncOnStart = () => invoke<boolean>('get_sync_on_start');
export const setSyncOnStart = (enabled: boolean) => invoke<void>('set_sync_on_start', { enabled });
export const getSyncAutoAdd = () => invoke<boolean>('get_sync_auto_add');
export const setSyncAutoAdd = (enabled: boolean) => invoke<void>('set_sync_auto_add', { enabled });

// The sync dialog lives in the header; other parts of the app ask for it through this event
// instead of owning a second copy of the dialog.
const DISK_SYNC_REQUEST_EVENT = 'catalog-disk-sync-request';
export const requestDiskSync = () => document.dispatchEvent(new Event(DISK_SYNC_REQUEST_EVENT));
export const onDiskSyncRequest = (handler: () => void) => {
  document.addEventListener(DISK_SYNC_REQUEST_EVENT, handler);
  return () => document.removeEventListener(DISK_SYNC_REQUEST_EVENT, handler);
};
