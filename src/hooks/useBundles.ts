import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import * as bundlesApi from '../lib/api/bundles';
import { indexBundles } from '../lib/bundles';
import type { BundleEditPlan, BundleSnapshot } from '../lib/bundleEdit';
import { reportMetadataError } from '../lib/metadataErrors';
import { canEditMetadata } from '../lib/webSession';
import type { Bundle, BundleKind } from '../types';

/**
 * Bundles of the catalog. The list is read once and after every change; a late answer never
 * replaces a newer one. Changes need the right to edit metadata (always given on the desktop).
 */
export function useBundles() {
  const [bundles, setBundles] = useState<Bundle[]>([]);
  const request = useRef(0);

  const refreshBundles = useCallback(async () => {
    const ticket = ++request.current;
    const list = await bundlesApi.listBundles();
    if (ticket === request.current) setBundles(list);
  }, []);

  useEffect(() => {
    refreshBundles().catch(error => console.error('[bundles] loading failed:', error));
  }, [refreshBundles]);

  const bundleOfFile = useMemo(() => indexBundles(bundles), [bundles]);

  /** Runs a change, refreshes the list and reports a failure; resolves to `undefined` without the right to edit. */
  const change = useCallback(
    async <T,>(action: () => Promise<T>): Promise<T | undefined> => {
      if (!canEditMetadata()) return undefined;
      try {
        const result = await action();
        await refreshBundles();
        return result;
      } catch (error) {
        reportMetadataError(error);
        // A failed change may still have moved files: show the real state.
        refreshBundles().catch(() => undefined);
        throw error;
      }
    },
    [refreshBundles],
  );

  const createBundle = useCallback(
    (name: string, kind: BundleKind, fileIds: string[]) => change(() => bundlesApi.createBundle(name, kind, fileIds)),
    [change],
  );
  const updateBundle = useCallback(
    (bundleId: string, patch: { name?: string; kind?: BundleKind }) => change(() => bundlesApi.updateBundle(bundleId, patch)),
    [change],
  );
  const addFilesToBundle = useCallback(
    (bundleId: string, fileIds: string[]) => change(() => bundlesApi.addFilesToBundle(bundleId, fileIds)),
    [change],
  );
  const removeFilesFromBundle = useCallback(
    (fileIds: string[]) => change(() => bundlesApi.removeFilesFromBundle(fileIds)),
    [change],
  );
  const dissolveBundle = useCallback(
    (bundleId: string) => change(() => bundlesApi.dissolveBundle(bundleId)),
    [change],
  );
  const updateBundleMember = useCallback(
    (fileId: string, patch: { label?: string; makeTitle?: boolean }) => change(() => bundlesApi.updateBundleMember(fileId, patch)),
    [change],
  );

  /** Creates a bundle and gives its members their labels (the command itself takes none). */
  const createLabelledBundle = useCallback(
    (name: string, kind: BundleKind, members: { fileId: string; label: string }[]) => change(async () => {
      const created = await bundlesApi.createBundle(name, kind, members.map(member => member.fileId));
      for (const member of members) {
        if (member.label) await bundlesApi.updateBundleMember(member.fileId, { label: member.label });
      }
      return created;
    }),
    [change],
  );
  /**
   * Brings back a dissolved bundle in one step: order, labels and members in the trash included.
   * `left` counts the members that could not come back (deleted for good, or in another bundle by now).
   */
  const restoreBundle = useCallback(
    (snapshot: BundleSnapshot) => change(async () => {
      const bundle = await bundlesApi.createBundle(snapshot.name, snapshot.kind, snapshot.members.map(member => member.fileId), {
        labels: snapshot.members.map(member => member.label),
        restore: true,
      });
      return { bundle, left: snapshot.members.length - bundle.members.length };
    }),
    [change],
  );
  /** Carries out an edit of the bundle dialog: removals first, then the order, then the labels. */
  const saveBundleEdit = useCallback(
    (bundleId: string, plan: BundleEditPlan) => change(async () => {
      if (plan.dissolve) {
        await bundlesApi.dissolveBundle(bundleId);
        return;
      }
      if (plan.name !== undefined || plan.kind !== undefined) await bundlesApi.updateBundle(bundleId, { name: plan.name, kind: plan.kind });
      if (plan.removeIds.length > 0) await bundlesApi.removeFilesFromBundle(plan.removeIds);
      for (const fileId of plan.titleSteps) await bundlesApi.updateBundleMember(fileId, { makeTitle: true });
      for (const { fileId, label } of plan.labelChanges) await bundlesApi.updateBundleMember(fileId, { label });
    }),
    [change],
  );
  /** Dissolves several bundles in one go (a selection can hold more than one). */
  const dissolveBundles = useCallback(
    (bundleIds: string[]) => change(async () => {
      for (const bundleId of bundleIds) await bundlesApi.dissolveBundle(bundleId);
    }),
    [change],
  );

  return {
    bundles,
    bundleOfFile,
    refreshBundles,
    createBundle,
    updateBundle,
    addFilesToBundle,
    removeFilesFromBundle,
    dissolveBundle,
    updateBundleMember,
    createLabelledBundle,
    restoreBundle,
    saveBundleEdit,
    dissolveBundles,
  };
}
