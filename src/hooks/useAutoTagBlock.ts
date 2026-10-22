import { useCallback, useEffect, useRef, useState } from 'react';
import type { ModelFile } from '../types';
import * as api from '../lib/api/autoTags';
import { toAppError, type AppError } from '../lib/errors';

export interface AutoTagBlockNotice {
  tag: string;
  /** True after the block was taken back. */
  undone: boolean;
}

/**
 * Which tags of a model the app would assign by itself (the catalog does not record
 * where a tag came from, so this is recomputed), and the "remove and block" action
 * with its undo. Only these tags offer the block; every other tag keeps the plain remove.
 */
export function useAutoTagBlock(
  model: Pick<ModelFile, 'id' | 'name'> | null,
  onRemoveTag: (tag: string) => void,
  onAddTag: (tag: string) => void,
) {
  const [suggested, setSuggested] = useState<ReadonlySet<string>>(new Set());
  const [notice, setNotice] = useState<AutoTagBlockNotice | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const modelId = model?.id;
  const modelName = model?.name;
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  useEffect(() => {
    setNotice(null);
    setError(null);
  }, [modelId]);

  useEffect(() => {
    setSuggested(new Set());
    if (!modelId) return;
    let current = true;
    // A failure only means no tag offers the block; removing keeps working as before.
    Promise.resolve()
      .then(() => api.listSuggestedAutoTags(modelId))
      .then((tags) => { if (current && Array.isArray(tags)) setSuggested(new Set(tags)); })
      .catch(() => {});
    return () => { current = false; };
  }, [modelId, modelName]);

  const removeAndBlock = useCallback(async (tag: string) => {
    setError(null);
    try {
      await api.blockAutoTag(tag);
    } catch (e) {
      if (mounted.current) setError(toAppError(e));
      return;
    }
    onRemoveTag(tag);
    if (mounted.current) setNotice({ tag, undone: false });
  }, [onRemoveTag]);

  const undo = useCallback(async () => {
    if (!notice || notice.undone) return;
    setError(null);
    try {
      await api.unblockAutoTag(notice.tag);
    } catch (e) {
      if (mounted.current) setError(toAppError(e));
      return;
    }
    onAddTag(notice.tag);
    if (mounted.current) setNotice({ tag: notice.tag, undone: true });
  }, [notice, onAddTag]);

  return { suggested, notice, error, removeAndBlock, undo };
}
