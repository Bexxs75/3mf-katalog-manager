import type { ModelFile } from '../types';

export interface DuplicateGroup<T> {
  /** The content hash all members share. */
  hash: string;
  /** Oldest import first; equal times by id, so the order never changes between renders. */
  members: T[];
}

type Candidate = Pick<ModelFile, 'id' | 'name' | 'contentHash' | 'importedAt'>;

/** A missing or broken time sorts last: such a record is never taken for the oldest. */
function importedMs(value: string): number {
  const ms = Date.parse(value);
  return Number.isNaN(ms) ? Number.POSITIVE_INFINITY : ms;
}

/**
 * Groups of models with the same content: two or more members with the same hash. Models without a
 * hash are never in a group. The one function behind the sidebar count, the tool view and the
 * duplicates view, so they cannot disagree.
 */
export function groupDuplicates<T extends Candidate>(models: readonly T[]): DuplicateGroup<T>[] {
  const byHash = new Map<string, T[]>();
  for (const model of models) {
    if (!model.contentHash) continue;
    const group = byHash.get(model.contentHash);
    if (group) group.push(model);
    else byHash.set(model.contentHash, [model]);
  }
  return [...byHash.entries()]
    .filter(([, members]) => members.length >= 2)
    .map(([hash, members]) => ({
      hash,
      members: members
        .map(model => ({ ms: importedMs(model.importedAt), model }))
        .sort((a, b) => (a.ms === b.ms ? (a.model.id < b.model.id ? -1 : a.model.id > b.model.id ? 1 : 0) : a.ms < b.ms ? -1 : 1))
        .map(entry => entry.model),
    }))
    .sort((a, b) => a.members[0].name.localeCompare(b.members[0].name) || (a.hash < b.hash ? -1 : a.hash > b.hash ? 1 : 0));
}
