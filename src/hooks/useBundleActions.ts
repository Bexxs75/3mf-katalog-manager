import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useFormatCount, useT } from '../i18n/LanguageContext';
import { displayName } from '../lib/displayName';
import { planBundleEdit, selectionBundleState, snapshotBundle, type BundleSnapshot, type EditRow } from '../lib/bundleEdit';
import { toAppError } from '../lib/errors';
import { useFormatError } from '../diagnostics/useFormatError';
import { canEditMetadata } from '../lib/webSession';
import type { Bundle, BundleKind, ModelFile } from '../types';
import type { BundleDialogRequest } from '../components/BundleDialog';
import type { useBundles } from './useBundles';

interface Args {
  bundleStore: ReturnType<typeof useBundles>;
  models: ModelFile[];
  /** File ids of the selection (a bundle card selects all its members). */
  selection: ReadonlySet<string>;
  /** Replaces the selection. */
  selectIds: (ids: string[]) => void;
  /** With bundles shown as cards "Delete" on a whole card asks first; off, every file is on its own. */
  collapse: boolean;
  /** Called before a notice shows, so an older one with its own "Undo" can go. */
  onBeforeToast?: () => void;
}

interface Toast {
  text: string;
  undo?: () => Promise<void>;
}

const cardSelector = (bundleId: string) => `[data-bundle-id="${CSS.escape(bundleId)}"]`;
const tileSelector = (fileId: string) => `[data-model-id="${CSS.escape(fileId)}"]`;

/**
 * Everything the user can do with bundles from the selection: create, add, edit, remove,
 * dissolve (with "Undo") and the choice that "Delete" offers on a bundle card.
 */
export function useBundleActions({ bundleStore, models, selection, selectIds, collapse, onBeforeToast }: Args) {
  const t = useT();
  const formatCount = useFormatCount();
  const formatError = useFormatError();
  const { bundles } = bundleStore;
  const [dialog, setDialog] = useState<BundleDialogRequest | null>(null);
  const [deleteChoice, setDeleteChoice] = useState(false);
  const [toast, setToast] = useState<Toast | null>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  const [focusTarget, setFocusTarget] = useState<string | null>(null);

  const state = useMemo(() => selectionBundleState(selection, bundles), [selection, bundles]);

  // Focus goes to the card or tile that now stands for the change, once it is on screen.
  useEffect(() => {
    if (!focusTarget) return;
    const element = document.querySelector<HTMLElement>(focusTarget);
    if (element) {
      element.focus();
      setFocusTarget(null);
      return;
    }
    const timer = setTimeout(() => setFocusTarget(null), 1500);
    return () => clearTimeout(timer);
  }, [focusTarget, bundles, models]);
  // After the dialog has closed, so its own focus return does not win.
  const focusLater = useCallback((selector: string) => { setTimeout(() => setFocusTarget(selector), 0); }, []);

  const show = useCallback((text: string, undo?: () => Promise<void>) => {
    onBeforeToast?.();
    setToast({ text, undo });
  }, [onBeforeToast]);
  const dismissToast = useCallback(() => setToast(null), []);

  /** Changes that have no dialog to show a failure in report it as a notice. */
  const guarded = useCallback(async (action: () => Promise<void>) => {
    try {
      await action();
    } catch (error) {
      show(formatError(toAppError(error)));
    }
  }, [show, formatError]);

  const openDialog = useCallback((request: BundleDialogRequest, returnTo: HTMLElement | null) => {
    if (!canEditMetadata()) return;
    returnFocus.current = returnTo ?? (document.activeElement as HTMLElement | null);
    setDialog(request);
  }, []);
  const closeDialog = useCallback(() => setDialog(null), []);

  const create = useCallback(async (name: string, kind: BundleKind, rows: { fileId: string; label: string }[]) => {
    const created = await bundleStore.createLabelledBundle(name, kind, rows);
    if (!created) return;
    selectIds(rows.map(row => row.fileId));
    show(t('bundleToastCreated').replace('{name}', displayName(name)).replace('{count}', String(rows.length)));
    focusLater(cardSelector(created.id));
  }, [bundleStore, selectIds, show, t, focusLater]);

  const add = useCallback(async (bundleId: string, fileIds: string[]) => {
    const target = bundles.find(bundle => bundle.id === bundleId);
    await bundleStore.addFilesToBundle(bundleId, fileIds);
    selectIds([...(target?.members.filter(member => !member.inTrash).map(member => member.fileId) ?? []), ...fileIds]);
    show(formatCount(t('bundleToastAdded'), fileIds.length).replace('{name}', displayName(target?.name ?? '')));
    focusLater(cardSelector(bundleId));
  }, [bundleStore, bundles, selectIds, show, t, formatCount, focusLater]);

  const restore = useCallback(async (snapshots: BundleSnapshot[]) => {
    await guarded(async () => {
      let last: Bundle | undefined;
      for (const snapshot of snapshots) last = (await bundleStore.restoreBundle(snapshot)) ?? last;
      if (!last) return;
      selectIds(snapshots.flatMap(snapshot => snapshot.members.map(member => member.fileId)));
      show(snapshots.length === 1
        ? t('bundleToastRestored').replace('{name}', displayName(snapshots[0].name))
        : t('bundleToastRestoredMany').replace('{bundles}', String(snapshots.length)));
      focusLater(cardSelector(last.id));
    });
  }, [bundleStore, selectIds, show, t, focusLater, guarded]);

  const saveEdit = useCallback(async (bundle: Bundle, draft: { name: string; kind: BundleKind; rows: EditRow[] }) => {
    const plan = planBundleEdit(bundle, draft);
    await bundleStore.saveBundleEdit(bundle.id, plan);
    if (plan.dissolve) {
      show(t('bundleToastDissolved').replace('{name}', displayName(bundle.name)).replace('{count}', String(draft.rows.length)),
        () => restore([snapshotBundle(bundle)]));
      focusLater(tileSelector(draft.rows[0]?.fileId ?? ''));
      return;
    }
    show(t('bundleToastSaved').replace('{name}', displayName(draft.name.trim())));
    focusLater(cardSelector(bundle.id));
  }, [bundleStore, show, t, focusLater, restore]);

  const remove = useCallback((fileIds: string[]) => guarded(async () => {
    if (fileIds.length === 0) return;
    const leaving = new Set(fileIds);
    // A bundle is dissolved with its second to last member row, so count rows, not living files.
    const dissolved = bundles.some(bundle => {
      const hit = bundle.members.filter(member => leaving.has(member.fileId)).length;
      return hit > 0 && bundle.members.length - hit < 2;
    });
    await bundleStore.removeFilesFromBundle(fileIds);
    const text = formatCount(t('bundleToastRemoved'), fileIds.length);
    show(dissolved ? `${text} ${t('bundleToastRemovedDissolved')}` : text);
    focusLater(tileSelector(fileIds[0]));
  }), [bundleStore, bundles, show, t, formatCount, focusLater, guarded]);

  /** Throws on failure, so the delete choice can show it in its own dialog. */
  const dissolveOrThrow = useCallback(async (bundleIds: string[]) => {
    const chosen = bundleIds.flatMap(id => bundles.find(bundle => bundle.id === id) ?? []);
    const snapshots = chosen.map(snapshotBundle);
    await bundleStore.dissolveBundles(chosen.map(bundle => bundle.id));
    const count = snapshots.reduce((sum, snapshot) => sum + snapshot.members.length, 0);
    show(chosen.length === 1
      ? t('bundleToastDissolved').replace('{name}', displayName(chosen[0].name)).replace('{count}', String(count))
      : t('bundleToastDissolvedMany').replace('{bundles}', String(chosen.length)).replace('{count}', String(count)),
    () => restore(snapshots));
    focusLater(tileSelector(snapshots[0]?.members[0]?.fileId ?? ''));
  }, [bundleStore, bundles, show, t, restore, focusLater]);

  const dissolve = useCallback((bundleIds: string[]) => guarded(() => dissolveOrThrow(bundleIds)), [guarded, dissolveOrThrow]);
  const closeDeleteChoice = useCallback(() => setDeleteChoice(false), []);

  /**
   * "Delete" on a selection that holds whole bundle cards asks first. Returns true when it took
   * over, so the caller skips the plain confirmation.
   */
  const interceptDelete = useCallback((): boolean => {
    if (!collapse || !canEditMetadata() || state.covered.length === 0) return false;
    returnFocus.current = document.activeElement as HTMLElement | null;
    setDeleteChoice(true);
    return true;
  }, [collapse, state.covered.length]);

  /** The delete choice for one bundle card (context menu): the card's members become the selection first. */
  const askDeleteBundle = useCallback((bundle: Bundle, returnTo: HTMLElement | null) => {
    if (!canEditMetadata()) return;
    returnFocus.current = returnTo;
    selectIds(bundle.members.filter(member => !member.inTrash).map(member => member.fileId));
    setDeleteChoice(true);
  }, [selectIds]);

  // A bundle that vanished meanwhile (dissolved on another device) leaves nothing to ask about.
  useEffect(() => { if (deleteChoice && state.covered.length === 0) setDeleteChoice(false); }, [deleteChoice, state.covered.length]);

  const runUndo = useCallback(() => {
    const undo = toast?.undo;
    if (!undo) return;
    setToast(null);
    void undo();
  }, [toast]);

  const namesById = useMemo(() => new Map(models.map(model => [model.id, model.name])), [models]);
  return {
    state,
    dialog, openDialog, closeDialog,
    returnFocus,
    create, add, saveEdit, remove, dissolve,
    deleteChoice,
    closeDeleteChoice,
    interceptDelete, askDeleteBundle,
    dissolveCovered: () => dissolveOrThrow(state.covered.map(bundle => bundle.id)),
    deleteNames: { bundles: state.covered.map(bundle => bundle.name), files: [...selection].map(id => namesById.get(id) ?? id) },
    toast, dismissToast, runUndo, undoable: !!toast?.undo,
  };
}
