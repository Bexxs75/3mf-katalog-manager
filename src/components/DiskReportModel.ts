import { diskSkipReasons, type DiskReport, type DiskRowKind, type DiskSkipReason, type DiskSyncSelection } from '../lib/api/diskSync';
import type { DiskSyncFailure, DiskSyncOutcome } from '../lib/fileOps';

/** The steps of the dialog; `error` carries its own kind (see `FailKind`). */
export type DiskStep = 'scanning' | 'select' | 'previewing' | 'preview' | 'starting' | 'running' | 'cancelling' | 'done' | 'cancelled' | 'error';
/** The three lists of the report, in the order they are shown. */
export type DiskSection = 'new' | 'missing' | 'companions';

export const newKey = (file: { root: number; relativePath: string }) => `${file.root}:${file.relativePath}`;

export type DiskSelection = Record<DiskSection, ReadonlySet<string>>;

/**
 * What is ticked at first: new files and dangling companion entries, never the models whose file is gone.
 * A companion row that points at something other than a file is not offered at all (the server never removes it).
 */
export function initialDiskSelection(report: DiskReport): DiskSelection {
  return {
    new: new Set(report.newFiles.map(newKey)),
    missing: new Set(),
    companions: new Set(report.danglingCompanions.filter(c => c.reason === 'missing').map(c => c.id)),
  };
}

export const diskSelectionCount = (selection: DiskSelection) => selection.new.size + selection.missing.size + selection.companions.size;

/** Only keys that exist in the report can be sent, whatever the state holds. */
export function diskSyncSelection(report: DiskReport, selection: DiskSelection): DiskSyncSelection {
  return {
    newFiles: report.newFiles.filter(f => selection.new.has(newKey(f))).map(f => ({ root: f.root, relativePath: f.relativePath })),
    missingModels: report.missingModels.filter(m => selection.missing.has(m.id)).map(m => m.id),
    danglingCompanions: report.danglingCompanions.filter(c => c.reason === 'missing' && selection.companions.has(c.id)).map(c => c.id),
  };
}

/** The path a plan row stands for, with the folder name when there are several folders. */
export function diskRowLabel(report: DiskReport, kind: DiskRowKind, key: string): string {
  const withRoot = (root: number, path: string) => report.roots.length > 1 && report.roots[root] ? `${report.roots[root].name}: ${path}` : path;
  if (kind === 'import') {
    const file = report.newFiles.find(f => newKey(f) === key);
    return file ? withRoot(file.root, file.relativePath) : key;
  }
  if (kind === 'removeModel') {
    const model = report.missingModels.find(m => m.id === key);
    return model ? withRoot(model.root, model.relativePath) : key;
  }
  const companion = report.danglingCompanions.find(c => c.id === key);
  return companion ? withRoot(companion.root, companion.relativePath) : key;
}

/** Text key of a skip reason; a code this client does not know reads as a plain failure. */
export const skipReasonKey = (reason: string): `diskSkip_${DiskSkipReason}` =>
  (diskSkipReasons as readonly string[]).includes(reason) ? `diskSkip_${reason as DiskSkipReason}` : 'diskSkip_failed';

export type FailKind = 'scan' | 'expired' | 'changed' | 'limit' | 'maintenanceStart' | 'maintenanceRun' | 'backupFailed' | 'restart' | 'interrupted' | 'other';
export interface DiskFailure { kind: FailKind; message: string; retryAfter?: number; outcome: DiskSyncOutcome | null; raw?: unknown }

/** Sorts a refusal or failure of a disk-sync step into what the dialog tells the user. */
export function classifyDiskFailure(failure: DiskSyncFailure): DiskFailure {
  const base = { message: failure.message, retryAfter: failure.retryAfter, outcome: failure.outcome };
  switch (failure.kind) {
    case 'snapshot_not_found': return { ...base, kind: 'expired' };
    case 'snapshot_mismatch': return { ...base, kind: 'changed' };
    case 'limit_exceeded': case 'rate_limited': return { ...base, kind: 'limit' };
    case 'maintenance': return { ...base, kind: failure.outcome ? 'maintenanceRun' : 'maintenanceStart' };
    case 'backup_failed': return { ...base, kind: 'backupFailed' };
    case 'restart_required': return { ...base, kind: 'restart' };
    default: return { ...base, kind: failure.outcome ? 'interrupted' : 'other' };
  }
}

/** Any failure that reached the dialog, whatever shape the transport threw. */
export function asDiskFailure(reason: unknown): DiskSyncFailure {
  const value = reason as Partial<DiskSyncFailure> | null;
  if (value && typeof value === 'object' && typeof value.message === 'string' && 'outcome' in value) return value as DiskSyncFailure;
  return { message: typeof value?.message === 'string' ? value.message : String(reason), expected: true, outcome: null };
}

type MissingModel = DiskReport['missingModels'][number];
const parentOf = (path: string) => { const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')); return cut > 0 ? path.slice(0, cut) : ''; };
type MissingFolder = { key: string; root: number; path: string; models: MissingModel[] };
/**
 * Missing models that share a folder are shown together as one folder row, at least two models per row; a single missing model stays a plain row.
 * Which folder is really gone is the server's call: when it sends `missingFolders` (also an empty array), only those folders (verified) form groups.
 * Only an older server without the field falls back to "two or more missing models below the same parent".
 */
export function groupMissingFolders(report: DiskReport): { folders: MissingFolder[]; single: MissingModel[] } {
  const folders: MissingFolder[] = [];
  const grouped = new Set<string>();
  if (Array.isArray(report.missingFolders)) {
    const byId = new Map(report.missingModels.map(m => [m.id, m]));
    for (const entry of report.missingFolders) {
      const models = entry.models.flatMap(id => { const m = byId.get(id); return m && !grouped.has(id) ? [m] : []; });
      if (models.length < 2) continue;
      models.forEach(m => grouped.add(m.id));
      folders.push({ key: `${entry.root}|${entry.relativePath}`, root: entry.root, path: entry.relativePath, models });
    }
    return { folders, single: report.missingModels.filter(m => !grouped.has(m.id)) };
  }
  const byParent = new Map<string, MissingModel[]>();
  for (const model of report.missingModels) {
    const key = `${model.root}|${parentOf(model.relativePath)}`;
    byParent.set(key, [...(byParent.get(key) ?? []), model]);
  }
  const single: MissingModel[] = [];
  for (const [key, models] of byParent) {
    const path = parentOf(models[0].relativePath);
    if (path && models.length > 1) folders.push({ key, root: models[0].root, path, models }); else single.push(...models);
  }
  return { folders, single };
}
