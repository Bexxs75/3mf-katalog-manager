import { invoke } from '../transport';
import type { StartedImport } from '../../types';

/** Frontend event of the backend: files were handed over by the system, collect them. */
export const OPEN_FILES_EVENT = 'open-files-requested';

/**
 * A file the operating system asked us to open. "catalog": already cataloged, show its detail page.
 * "external": not cataloged, only `sourceId` stands for it (never a path) in later commands.
 */
export type OpenRequest =
  | { kind: 'catalog'; fileId: string }
  | { kind: 'external'; sourceId: string; name: string; sizeBytes: number; extension: string };

/** Hands over what the system passed since the last call; the backend empties its queue. */
export function takeOpenRequests() {
  return invoke<OpenRequest[]>('take_open_requests');
}

/** Forgets a source when its view closes; the backend keeps at most a handful of them. */
export function releaseExternalSource(sourceId: string) {
  return invoke<void>('release_external_source', { sourceId });
}

/**
 * Hands a viewed file to the normal import. The id replaces the path: the backend vouches for it
 * as it does for a dropped file. `folderId` is the catalog folder the file is moved into.
 */
export function addExternalToCatalog(sourceId: string, folderId?: string) {
  return invoke<StartedImport>('add_external_to_catalog', { sourceId, folderId });
}
