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
    undoTarget.current = null;
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

  // The callbacks handed in are bound to the model that was shown when they were created.
  // Keeping the ones from the moment an action starts makes a late answer or the undo act on
  // that model, never on whichever model happens to be shown by then.
  const latestModelId = useRef(modelId);
  latestModelId.current = modelId;
  const undoTarget = useRef<{ modelId: string | undefined; addTag: (tag: string) => void } | null>(null);

  const removeAndBlock = useCallback(async (tag: string) => {
    const startedFor = modelId;
    setError(null);
    try {
      await api.blockAutoTag(tag);
    } catch (e) {
      if (mounted.current && latestModelId.current === startedFor) setError(toAppError(e));
      return;
    }
    onRemoveTag(tag);
    // The user moved on while the call was running: the tag is gone from the model it was
    // started for, but the undo hint would now sit next to a different model.
    if (!mounted.current || latestModelId.current !== startedFor) return;
    undoTarget.current = { modelId: startedFor, addTag: onAddTag };
    setNotice({ tag, undone: false });
  }, [modelId, onRemoveTag, onAddTag]);

  const undo = useCallback(async () => {
    const target = undoTarget.current;
    if (!notice || notice.undone || !target || target.modelId !== latestModelId.current) return;
    const { tag } = notice;
    setError(null);
    try {
      await api.unblockAutoTag(tag);
    } catch (e) {
      if (mounted.current && latestModelId.current === target.modelId) setError(toAppError(e));
      return;
    }
    target.addTag(tag);
    if (mounted.current && latestModelId.current === target.modelId) setNotice({ tag, undone: true });
  }, [notice]);

  return { suggested, notice, error, removeAndBlock, undo };
}
