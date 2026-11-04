import { invoke } from '../transport';
import type { Bundle, BundleKind } from '../../types';

export function listBundles() {
  return invoke<Bundle[]>('list_bundles');
}
export function createBundle(name: string, kind: BundleKind, fileIds: string[]) {
  return invoke<Bundle>('create_bundle', { name, kind, fileIds });
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
