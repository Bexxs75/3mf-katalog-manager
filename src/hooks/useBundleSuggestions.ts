import { useCallback, useMemo, useRef, useState } from 'react';
import {
  loadSuggestionPreferences, saveSuggestionPreferences, suggestBundles, type SuggestionPreferences,
} from '../lib/bundleSuggestions';
import type { Bundle, ModelFile } from '../types';

interface Args {
  models: ModelFile[];
  bundles: Bundle[];
  /** Without the right to change bundles nothing is suggested. */
  enabled: boolean;
}

/**
 * Bundle suggestions of the catalog: the candidates, the ones the user switched off, the notice that
 * follows an import or a sync, and the dialog. Nothing is bundled from here; the dialog's action does that.
 */
export function useBundleSuggestions({ models, bundles, enabled }: Args) {
  const [preferences, setPreferences] = useState<SuggestionPreferences>(loadSuggestionPreferences);
  // Files that arrived since the notice was last closed; the notice counts only suggestions that involve them.
  const [arrived, setArrived] = useState<ReadonlySet<string>>(() => new Set());
  const [dialogOpen, setDialogOpen] = useState(false);
  const returnFocus = useRef<HTMLElement | null>(null);

  const all = useMemo(() => enabled ? suggestBundles(models, bundles) : [], [enabled, models, bundles]);
  const dismissedKeys = useMemo(() => new Set(preferences.dismissed), [preferences.dismissed]);
  const open = useMemo(() => all.filter(suggestion => !dismissedKeys.has(suggestion.key)), [all, dismissedKeys]);
  const dismissed = useMemo(() => all.filter(suggestion => dismissedKeys.has(suggestion.key)), [all, dismissedKeys]);

  const noticeCount = useMemo(() => {
    if (!preferences.hint || arrived.size === 0) return 0;
    return open.filter(suggestion => suggestion.fileIds.some(id => arrived.has(id))).length;
  }, [preferences.hint, arrived, open]);

  const update = useCallback((change: (previous: SuggestionPreferences) => SuggestionPreferences) => {
    setPreferences(previous => {
      const next = change(previous);
      saveSuggestionPreferences(next);
      return next;
    });
  }, []);

  /** Files an import or a sync has just added. The catalog list may follow a moment later. */
  const notifyAdded = useCallback((fileIds: string[]) => {
    if (fileIds.length === 0) return;
    setArrived(previous => new Set([...previous, ...fileIds]));
  }, []);
  const dismissNotice = useCallback(() => setArrived(new Set()), []);

  const openDialog = useCallback((returnTo: HTMLElement | null) => {
    returnFocus.current = returnTo ?? (document.activeElement as HTMLElement | null);
    setArrived(new Set());
    setDialogOpen(true);
  }, []);
  const closeDialog = useCallback(() => setDialogOpen(false), []);

  const setHint = useCallback((hint: boolean) => update(previous => ({ ...previous, hint })), [update]);
  const dismiss = useCallback((key: string) => update(previous => (
    previous.dismissed.includes(key) ? previous : { ...previous, dismissed: [...previous.dismissed, key] }
  )), [update]);
  const restore = useCallback((key: string) => update(previous => ({
    ...previous, dismissed: previous.dismissed.filter(entry => entry !== key),
  })), [update]);

  return {
    open,
    dismissed,
    hint: preferences.hint,
    noticeCount,
    notifyAdded, dismissNotice,
    dialogOpen, openDialog, closeDialog, returnFocus,
    setHint, dismiss, restore,
  };
}
