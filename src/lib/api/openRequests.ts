import { invoke } from '../transport';

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
