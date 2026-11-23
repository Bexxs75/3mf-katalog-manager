import { invoke } from '../transport';

/**
 * The server command that copies a browsed folder into the catalog is not available yet, so
 * the web UI does not offer the action. Switch the flag on and set the command name when it is.
 */
export const WEB_SOURCE_IMPORT_ENABLED = false;
const IMPORT_COMMAND = 'import_source_dir';

/** Starts the copy import of one server folder (root index plus relative path); web only. */
export function importServerFolder(rootIdx: number, rel: string) {
  return invoke<unknown>(IMPORT_COMMAND, { rootIdx, rel });
}
