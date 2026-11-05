import { invoke } from '../transport';
import type { Bundle, BundleKind } from '../../types';

export function listBundles() {
  return invoke<Bundle[]>('list_bundles');
}
/**
 * `labels` (one per file) are set in the same step. `restore` brings back a dissolved bundle: members
 * in the trash join too, while members that are deleted for good or belong to another bundle by now
 * are left out (the answer shows who made it).
 */
export function createBundle(name: string, kind: BundleKind, fileIds: string[], options: { labels?: string[]; restore?: boolean } = {}) {
  return invoke<Bundle>('create_bundle', { name, kind, fileIds, ...options });
}
/** A field left out stays as it is. */
export function updateBundle(bundleId: string, patch: { name?: string; kind?: BundleKind }) {
  return invoke('update_bundle', { bundleId, ...patch });
}
export function addFilesToBundle(bundleId: string, fileIds: string[]) {
  return invoke<Bundle>('add_files_to_bundle', { bundleId, fileIds });
}
export function removeFilesFromBundle(fileIds: string[]) {
  return invoke('remove_files_from_bundle', { fileIds });
}
export function dissolveBundle(bundleId: string) {
  return invoke('dissolve_bundle', { bundleId });
}
/** A field left out stays as it is; an empty label clears it. */
export function updateBundleMember(fileId: string, patch: { label?: string; makeTitle?: boolean }) {
  return invoke('update_bundle_member', { fileId, ...patch });
}
