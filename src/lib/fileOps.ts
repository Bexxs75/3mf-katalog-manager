import { invoke } from './transport';
import { canManageFiles, getSnapshot, blockFileOperations } from './webSession';
import { reportMetadataError } from './metadataErrors';
import { fileOpsError, fileOpsMessage } from './fileOpsErrors';
export interface ItemResult { id: string; ok: boolean; kind?: string; kept_on_disk?: boolean }
export interface TrashSummary { count: number; bytes: number; maxDeletedAt: string | null }
const refreshers = new Set<() => void | Promise<unknown>>();
export function subscribeFileOpsRefresh(refresh: () => void | Promise<unknown>) {
  refreshers.add(refresh); return () => { refreshers.delete(refresh); };
}
export async function refreshAfterFileOps() {
  const results = await Promise.allSettled([...refreshers].map(refresh => Promise.resolve().then(refresh)));
  if (results.some(result => result.status === 'rejected')) reportMetadataError(fileOpsMessage('generic'));
}
function checkResults(results: ItemResult[]) {
  const failures = results.filter(result => !result.ok);
  if (failures.some(result => result.kind === 'restart_required')) blockFileOperations();
  const kept = results.filter(result => result.kept_on_disk);
  if (kept.length) reportMetadataError(kept.map(result => `${result.id}: ${fileOpsMessage('kept')}`).join('\n'));
  if (failures.length) throw { fileOpsMessage: results.map(result => `${result.id}: ${result.ok ? '✓' : fileOpsMessage(result.kind ?? 'generic')}`).join('\n'), kind: failures[0].kind };
}
/** Never optimistically mutate and always reconcile, even after an ambiguous lost response. */
export async function fileOp<T = unknown>(command: string, args: Record<string, unknown>): Promise<T> {
  if (!canManageFiles()) throw fileOpsError({kind:'revoked'});
  try {
    const result = await invoke<T>(command, args);
    if (Array.isArray(result)) checkResults(result as ItemResult[]);
    else if (result && typeof result === 'object' && 'kind' in result) throw result;
    if (result && typeof result === 'object' && 'kept_on_disk' in result && result.kept_on_disk) reportMetadataError(fileOpsMessage('kept'));
    return result;
  } catch (reason) {
    const error = fileOpsError(reason); reportMetadataError(error); throw error;
  } finally { await refreshAfterFileOps(); }
}
export async function fileOpsBatch(command: string, ids: string[], extra: Record<string, unknown> = {}): Promise<void> {
  const initiatingToken = getSnapshot().csrfToken;
  const unique = [...new Set(ids)];
  for (let offset = 0; offset < unique.length; offset += 200) {
    if (!canManageFiles() || getSnapshot().csrfToken !== initiatingToken) throw fileOpsError({kind:'revoked'});
    try { await fileOp(command, { ...extra, fileIds: unique.slice(offset, offset + 200) }); }
    catch (reason) {
      const pending = unique.slice(offset + 200);
      if (!pending.length) throw reason;
      const message = (reason as {message:string}).message + '\n' + pending.map(id => `${id}: ${fileOpsMessage('not_started')}`).join('\n');
      const error = {message, expected:true}; reportMetadataError(error); throw error;
    }
  }
}
/** The token is memory-only. A failed/ambiguous block is never advanced or replayed here. */
export async function emptyWebTrash(summary: TrashSummary, password: string, onProgress: (completed: number, total: number) => void): Promise<void> {
  const initiatingToken = getSnapshot().csrfToken;
  let start: {token:string; remaining:number};
  try {
    start = await fileOp('empty_trash_start', { expectedCount: summary.count, maxDeletedAt: summary.maxDeletedAt, password });
  } finally { password = ''; }
  let remaining = start.remaining;
  onProgress(summary.count - remaining, summary.count);
  while (remaining > 0) {
    if (!canManageFiles() || getSnapshot().csrfToken !== initiatingToken) throw fileOpsError({kind:'revoked'});
    const block = await fileOp<{results:ItemResult[]; remaining:number}>('empty_trash_block', {token:start.token});
    onProgress(summary.count - block.remaining, summary.count);
    try { checkResults(block.results); }
    catch (reason) { const error = fileOpsError(reason); reportMetadataError(error); throw error; }
    if (block.remaining >= remaining) throw fileOpsError({kind:'not_started'});
    remaining = block.remaining;
    if (remaining > 0) await new Promise(resolve => setTimeout(resolve, 2000));
  }
}
