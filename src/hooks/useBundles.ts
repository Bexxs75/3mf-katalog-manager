import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import * as bundlesApi from '../lib/api/bundles';
import { indexBundles } from '../lib/bundles';
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
  };
}
