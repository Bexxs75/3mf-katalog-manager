import { useCallback } from 'react';
import * as filesApi from '../lib/api/files';
import { fileOpsBatch } from '../lib/fileOps';
import { isReadOnly } from '../lib/readOnly';
import { canManageFiles } from '../lib/webSession';

/**
 * Puts duplicate models in the app trash and reports which of them are still in the catalog afterwards.
 * The backend skips files it cannot move (locked, read-only) without an error, so only the reloaded
 * list tells what is left; it is reloaded even after a failed call, as the bulk delete does.
 */
export function useDuplicateRemoval(
  refetchAfterPartialDelete: (ids: string[]) => void | Promise<void | readonly { id: string }[]>,
) {
  return useCallback(async (ids: string[]): Promise<string[]> => {
    if (ids.length === 0) return [];
    if (!canManageFiles()) return [...ids];
    let failure: unknown;
    let failed = false;
    try {
      if (isReadOnly()) await fileOpsBatch('delete_files', ids);
      else await filesApi.deleteFiles(ids);
    } catch (error) {
      failed = true;
      failure = error;
    }
    let remaining: readonly { id: string }[] | void = undefined;
    try {
      remaining = await refetchAfterPartialDelete(ids);
    } catch (error) {
      // A failed delete is the more useful error to show.
      if (!failed) throw error;
    }
    if (failed) throw failure;
    if (!remaining) return [];
    const stillThere = new Set(remaining.map(model => model.id));
    return ids.filter(id => stillThere.has(id));
  }, [refetchAfterPartialDelete]);
}
