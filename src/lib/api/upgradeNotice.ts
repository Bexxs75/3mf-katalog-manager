import { invoke } from '../transport';

export interface UpgradeBackupFile {
  fileName: string;
  path: string;
  sizeBytes: number;
  /** RFC 3339, local time; null when the file system does not say. */
  modified: string | null;
}

export interface UpgradeNotice {
  fromVersion: string | null;
  /** Worked out from the schema, not read from the copy the updater made. */
  fromVersionDerived: boolean;
  toVersion: string;
  fromSchema: number | null;
  toSchema: number;
  backup: UpgradeBackupFile | null;
  /** The copy before the migration failed and no other copy exists. */
  backupFailed: boolean;
  folder: string;
  catalogFile: string;
  /** Release page of the previous version; null when it cannot be opened from here. */
  previousReleaseUrl: string | null;
  container: boolean;
}

const str = (v: unknown): v is string => typeof v === 'string' && v.length > 0;
const int = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

function normalizeBackup(value: unknown): UpgradeBackupFile | null {
  const v = value as Partial<UpgradeBackupFile> | null;
  if (!v || typeof v !== 'object' || !str(v.fileName) || !str(v.path) || !int(v.sizeBytes)) return null;
  return { fileName: v.fileName, path: v.path, sizeBytes: v.sizeBytes, modified: str(v.modified) ? v.modified : null };
}

/** A malformed reply counts as "nothing to show", so a broken answer never opens a dialog. */
export function normalizeUpgradeNotice(value: unknown): UpgradeNotice | null {
  const v = value as Partial<UpgradeNotice> | null;
  if (!v || typeof v !== 'object' || !str(v.toVersion) || !int(v.toSchema) || !str(v.folder) || !str(v.catalogFile)) return null;
  const backup = normalizeBackup(v.backup);
  const backupFailed = v.backupFailed === true;
  if (!backup && !backupFailed) return null;
  return {
    fromVersion: str(v.fromVersion) ? v.fromVersion : null,
    fromVersionDerived: v.fromVersionDerived === true,
    toVersion: v.toVersion,
    fromSchema: int(v.fromSchema) ? v.fromSchema : null,
    toSchema: v.toSchema,
    backup,
    backupFailed,
    folder: v.folder,
    catalogFile: v.catalogFile,
    previousReleaseUrl: str(v.previousReleaseUrl) ? v.previousReleaseUrl : null,
    container: v.container === true,
  };
}

export const getUpgradeNotice = () =>
  invoke<unknown>('get_upgrade_notice').then(normalizeUpgradeNotice);

export const dismissUpgradeNotice = () => invoke<void>('dismiss_upgrade_notice');

/** Opens the fixed folder with the update copies; the backend takes no path. */
export const openUpdateBackupFolder = () => invoke<void>('open_update_backup_folder');
