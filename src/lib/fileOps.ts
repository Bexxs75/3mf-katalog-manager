import { invoke } from './transport';
import { canManageFiles, getSnapshot, blockFileOperations } from './webSession';
import { clearMetadataError, getMetadataError, reportMetadataError } from './metadataErrors';
import { messageOf } from './errors';
import { fileOpsError, fileOpsMessage, isDomainRejection } from './fileOpsErrors';
export interface ItemResult { id: string; ok: boolean; kind?: string; kept_on_disk?: boolean; companions_moved?: number; companions_kept_in_trash?: number }
/** `companionCount` and `blockedByCompanions` are absent from servers that predate companion handling. */
export interface TrashSummary { count: number; bytes: number; maxDeletedAt: string | null; companionCount?: number; blockedByCompanions?: number; looseCompanionCount?: number; looseCompanionBytes?: number }
const refreshers = new Set<() => void | Promise<unknown>>();
export function subscribeFileOpsRefresh(refresh: () => void | Promise<unknown>) {
  refreshers.add(refresh); return () => { refreshers.delete(refresh); };
}
/** Resolves true when every view reloaded; a failed reload is reported separately from a failed action. */
export async function refreshAfterFileOps(reportFailure = true): Promise<boolean> {
  const results = await Promise.allSettled([...refreshers].map(refresh => Promise.resolve().then(refresh)));
  const ok = !results.some(result => result.status === 'rejected');
  if (!ok && reportFailure) reportMetadataError(fileOpsMessage('refresh_failed'));
  return ok;
}
type AppliedListener = (ids: string[]) => void;
const appliedListeners = new Set<AppliedListener>();
/** Lets selection state drop IDs that a delete/restore/purge just touched, whatever surface started it. */
export function subscribeFileOpsApplied(listener: AppliedListener) {
  appliedListeners.add(listener); return () => { appliedListeners.delete(listener); };
}
function affectedIds(command: string, args: Record<string, unknown>, result: unknown): string[] {
  if (command === 'delete_files') return Array.isArray(args.fileIds) ? args.fileIds.map(String) : [];
  if (command === 'restore_file' || command === 'delete_file_permanently') return args.fileId === undefined ? [] : [String(args.fileId)];
  if (command === 'empty_trash_block' && result && typeof result === 'object' && Array.isArray((result as {results?: unknown}).results)) {
    return ((result as {results: ItemResult[]}).results).filter(item => item.ok && /^\d+$/.test(item.id)).map(item => item.id);
  }
  return [];
}
/** After a failure only what the server reported as done: the ok items of a partial batch or trash block. */
function appliedDespiteFailure(command: string, outcome: unknown): string[] {
  if (Array.isArray(outcome)) return (outcome as ItemResult[]).filter(item => item.ok).map(item => String(item.id));
  return command === 'empty_trash_block' ? affectedIds(command, {}, outcome) : [];
}
// The error slot holds one message; repeating the same text from nested layers adds nothing.
// Remembered so a later success can take down exactly this message and nobody else's.
let ownReport = '';
function reportOnce(reason: unknown) {
  if (getMetadataError() !== messageOf(reason)) reportMetadataError(reason);
  ownReport = getMetadataError() === messageOf(reason) ? messageOf(reason) : '';
}
function clearOwnReport() {
  if (ownReport && getMetadataError() === ownReport) clearMetadataError();
  ownReport = '';
}
/** Display names by file ID, so a refusal can say which file it is about. */
export type FileLabels = Record<string, string>;
function checkResults(results: ItemResult[], labels: FileLabels = {}) {
  const failures = results.filter(result => !result.ok);
  if (failures.some(result => result.kind === 'restart_required')) blockFileOperations();
  // A raw ID means nothing to the user; with a single result there is nothing to tell apart anyway.
  const who = (id: string) => labels[id] ? `${labels[id]}: ` : results.length > 1 ? `${id}: ` : '';
  const kept = results.filter(result => result.kept_on_disk);
  if (kept.length) reportOnce(kept.map(result => `${who(result.id)}${fileOpsMessage('kept')}`).join('\n'));
  if (failures.length) throw { fileOpsMessage: results.map(result => `${who(result.id)}${result.ok ? '✓' : fileOpsMessage(result.kind ?? 'generic')}`).join('\n'), kind: failures[0].kind, plain: failures.every(result => isDomainRejection(result.kind)) };
}
/** `quietKept`: the caller explains kept-on-disk itself (the confirmation dialog does). */
export async function fileOp<T = unknown>(command: string, args: Record<string, unknown>, options: { quietKept?: boolean; labels?: FileLabels } = {}): Promise<T> {
  if (!canManageFiles()) throw fileOpsError({kind:'revoked'});
  let failed = false; let outcome: unknown;
  try {
    const result = await invoke<T>(command, args);
    outcome = result;
    if (Array.isArray(result)) checkResults(result as ItemResult[], options.labels);
    else if (result && typeof result === 'object' && 'kind' in result) {
      // A single object with a kind is an error shape even on HTTP 200.
      if ((result as {kind?: unknown}).kind === 'restart_required') blockFileOperations();
      throw result;
    }
    clearOwnReport();
    if (!options.quietKept && result && typeof result === 'object' && 'kept_on_disk' in result && result.kept_on_disk) reportOnce(fileOpsMessage('kept'));
    return result;
  } catch (reason) {
    failed = true;
    const error = fileOpsError(reason); reportOnce(error); throw error;
  } finally {
    // A refused action changed nothing it names; announcing its IDs would clear the selection and tear down the dialog showing the refusal.
    const ids = failed ? appliedDespiteFailure(command, outcome) : affectedIds(command, args, outcome);
    if (ids.length) appliedListeners.forEach(listener => listener(ids));
    // A failed reload must not hide the action's own error.
    await refreshAfterFileOps(!failed);
  }
}
const cancelled = (pending: string[]) => ({ message: [fileOpsMessage('cancelled'), ...pending.map(id => `${id}: ${fileOpsMessage('not_started')}`)].join('\n'), expected: true as const, kind: 'cancelled' });
export async function fileOpsBatch(command: string, ids: string[], extra: Record<string, unknown> = {}, shouldStop: () => boolean = () => false, labels?: FileLabels): Promise<void> {
  const initiatingToken = getSnapshot().csrfToken;
  const unique = [...new Set(ids)];
  for (let offset = 0; offset < unique.length; offset += 200) {
    if (!canManageFiles() || getSnapshot().csrfToken !== initiatingToken) throw fileOpsError({kind:'revoked'});
    // Stops between chunks only; a chunk already sent is never aborted halfway.
    if (shouldStop()) { const error = cancelled(unique.slice(offset)); reportOnce(error); throw error; }
    try { await fileOp(command, { ...extra, fileIds: unique.slice(offset, offset + 200) }, { labels }); }
    catch (reason) {
      const pending = unique.slice(offset + 200);
      if (!pending.length) throw reason;
      const message = (reason as {message:string}).message + '\n' + pending.map(id => `${id}: ${fileOpsMessage('not_started')}`).join('\n');
      const error = {message, expected:true, ...((reason as {plain?: true}).plain ? {plain: true as const} : {})}; reportOnce(error); throw error;
    }
  }
}
const validRemaining = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value) && value >= 0;
const progressText = (done: number, total: number) => fileOpsMessage('progress').replace('{done}', String(done)).replace('{total}', String(total));
/** The token is memory-only. A failed/ambiguous block is never advanced or replayed here. */
export async function emptyWebTrash(summary: TrashSummary, password: string, onProgress: (completed: number, total: number) => void, shouldStop: () => boolean = () => false): Promise<void> {
  const initiatingToken = getSnapshot().csrfToken;
  // Individually removed companion files are emptied after the models and counted in the same progress.
  const looseCompanions = summary.looseCompanionCount ?? 0;
  const total = summary.count + looseCompanions;
  let start: {token:string; remaining:number};
  try {
    if (shouldStop()) throw cancelled([]);
    start = await fileOp('empty_trash_start', { expectedCount: summary.count, ...(looseCompanions > 0 ? { expectedCompanions: looseCompanions } : {}), maxDeletedAt: summary.maxDeletedAt, password });
  } finally { password = ''; }
  if (!start || typeof start.token !== 'string' || !validRemaining(start.remaining)) { const error = fileOpsError({kind:'not_started'}); reportOnce(error); throw error; }
  let remaining = start.remaining;
  onProgress(total - remaining, total);
  while (remaining > 0) {
    if (!canManageFiles() || getSnapshot().csrfToken !== initiatingToken) throw fileOpsError({kind:'revoked'});
    if (shouldStop()) {
      const error = { message: progressText(total - remaining, total) + ' ' + fileOpsMessage('cancelled'), expected: true as const, kind: 'cancelled' };
      reportOnce(error); throw error;
    }
    let block: {results:ItemResult[]; remaining:number};
    try { block = await fileOp<{results:ItemResult[]; remaining:number}>('empty_trash_block', {token:start.token}); }
    catch (reason) {
      const error = fileOpsError(reason);
      if (error.retryAfter === undefined) throw error;
      // A long wait ends the run; the snapshot token is dropped with this function, so nothing resumes unseen.
      const paused = { message: progressText(total - remaining, total) + ' ' + fileOpsMessage('paused') + ' ' + error.message, expected: true as const, kind: error.kind };
      reportOnce(paused); throw paused;
    }
    if (!block || !Array.isArray(block.results) || !validRemaining(block.remaining)) { const error = fileOpsError({kind:'not_started'}); reportOnce(error); throw error; }
    onProgress(total - block.remaining, total);
    try { checkResults(block.results); }
    catch (reason) { const error = fileOpsError(reason); reportOnce(error); throw error; }
    if (block.remaining >= remaining) throw fileOpsError({kind:'not_started'});
    remaining = block.remaining;
    if (remaining > 0) await new Promise(resolve => setTimeout(resolve, 2000));
  }
}

/** Removes or restores one companion file. Unlike `fileOp` it leaves the catalog views alone: nothing in them changes. */
export async function companionFileOp(command: 'remove_companion' | 'restore_companion', companionId: string): Promise<void> {
  if (!canManageFiles()) throw fileOpsError({ kind: 'revoked' });
  try { await invoke(command, { companionId }); }
  catch (reason) {
    if ((reason as { kind?: unknown } | null)?.kind === 'restart_required') blockFileOperations();
    throw fileOpsError(reason);
  }
}
