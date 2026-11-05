import type { Bundle, BundleKind, ModelFile } from '../types';
import { MAX_BUNDLE_MEMBERS, MIN_BUNDLE_MEMBERS, indexBundles } from './bundles';

/** Longest bundle name and member label the backend accepts. */
export const MAX_BUNDLE_NAME_CHARS = 120;
export const MAX_BUNDLE_LABEL_CHARS = 60;

/**
 * Ids to hand to "make title model", one after the other, so that `current` ends up as `desired`.
 * The backend only knows one reordering step (swap a member with the title model), and swapping
 * with the first place is enough to reach every order: each place from the back is filled with
 * at most two swaps, and the places behind it are never touched again.
 */
export function titleSwapSteps(current: string[], desired: string[]): string[] {
  const order = [...current];
  const steps: string[] = [];
  const swapWithTitle = (index: number) => {
    [order[0], order[index]] = [order[index], order[0]];
  };
  for (let place = order.length - 1; place >= 1; place--) {
    if (order[place] === desired[place]) continue;
    const from = order.indexOf(desired[place]);
    if (from > 0) {
      steps.push(desired[place]);
      swapWithTitle(from);
    }
    // The wanted file is the title model now; the file standing in its place swaps back with it.
    steps.push(order[place]);
    swapWithTitle(place);
  }
  return steps;
}

export interface EditRow {
  fileId: string;
  label: string;
  /** False: leave the bundle when saved. */
  keep: boolean;
}

export interface BundleEditPlan {
  /** Fewer than two living members stay, so the bundle goes away (the files stay). */
  dissolve: boolean;
  name?: string;
  kind?: BundleKind;
  removeIds: string[];
  /** Ids for "make title model", in this order. */
  titleSteps: string[];
  labelChanges: { fileId: string; label: string }[];
}

/**
 * What saving the edit dialog has to do. `rows` hold the living members in the order of the
 * dialog; members in the trash keep their places and their membership.
 */
export function planBundleEdit(bundle: Bundle, draft: { name: string; kind: BundleKind; rows: EditRow[] }): BundleEditPlan {
  const kept = draft.rows.filter(row => row.keep);
  const removeIds = draft.rows.filter(row => !row.keep).map(row => row.fileId);
  const plan: BundleEditPlan = { dissolve: kept.length < MIN_BUNDLE_MEMBERS, removeIds, titleSteps: [], labelChanges: [] };
  if (plan.dissolve) return plan;
  const name = draft.name.trim();
  if (name !== bundle.name) plan.name = name;
  if (draft.kind !== bundle.kind) plan.kind = draft.kind;

  const gone = new Set(removeIds);
  const ordered = [...bundle.members].sort((a, b) => a.position - b.position).filter(member => !gone.has(member.fileId));
  const keptIds = kept.map(row => row.fileId);
  let next = 0;
  const wanted = ordered.map(member => (member.inTrash ? member.fileId : keptIds[next++]));
  plan.titleSteps = titleSwapSteps(ordered.map(member => member.fileId), wanted);

  const before = new Map(bundle.members.map(member => [member.fileId, member.label ?? '']));
  for (const row of kept) {
    if (row.label.trim() !== (before.get(row.fileId) ?? '')) plan.labelChanges.push({ fileId: row.fileId, label: row.label.trim() });
  }
  return plan;
}

export interface BundleSnapshot {
  name: string;
  kind: BundleKind;
  /** Living members by position; a member in the trash cannot join a new bundle. */
  members: { fileId: string; label: string }[];
}

/** The state "Undo" brings back after a bundle was dissolved. */
export function snapshotBundle(bundle: Bundle): BundleSnapshot {
  return {
    name: bundle.name,
    kind: bundle.kind,
    members: [...bundle.members]
      .sort((a, b) => a.position - b.position)
      .filter(member => !member.inTrash)
      .map(member => ({ fileId: member.fileId, label: member.label ?? '' })),
  };
}

export interface SelectionBundleState {
  /** Bundles all of whose living members are selected, so a card stands for them. */
  covered: Bundle[];
  /** Selected files that belong to a bundle. */
  bundledIds: string[];
  /** The one bundle the selection is exactly made of; the one that can be edited. */
  exact: Bundle | null;
  /** The selected files that are not part of a covered bundle. */
  loose: number;
}

/** What a selection of files means for bundles; decides which bundle entries are offered. */
export function selectionBundleState(selection: Iterable<string>, bundles: Bundle[]): SelectionBundleState {
  const selected = new Set(selection);
  const living = (bundle: Bundle) => bundle.members.filter(member => !member.inTrash).map(member => member.fileId);
  const covered = bundles.filter(bundle => {
    const ids = living(bundle);
    return ids.length >= MIN_BUNDLE_MEMBERS && ids.every(id => selected.has(id));
  });
  const byFile = indexBundles(bundles);
  const coveredIds = new Set(covered.flatMap(living));
  return {
    covered,
    bundledIds: [...selected].filter(id => byFile.has(id)),
    exact: covered.length === 1 && living(covered[0]).length === selected.size ? covered[0] : null,
    loose: [...selected].filter(id => !coveredIds.has(id)).length,
  };
}

export interface SwitchNote {
  fileId: string;
  from: Bundle;
  /** Living members that stay in the old bundle. */
  left: number;
}

/**
 * Files of the list that already belong to another bundle than `target` (null: a new one) and
 * would switch. `left` tells whether the old bundle survives.
 */
export function switchNotes(fileIds: string[], bundles: Bundle[], target: string | null): SwitchNote[] {
  const byFile = indexBundles(bundles);
  const moving = new Set(fileIds);
  return fileIds.flatMap(fileId => {
    const from = byFile.get(fileId);
    if (!from || from.id === target) return [];
    const left = from.members.filter(member => !member.inTrash && !moving.has(member.fileId)).length;
    return [{ fileId, from, left }];
  });
}

/** Files of the list whose content equals an earlier file of the list. */
export function duplicateIds(models: ModelFile[]): Set<string> {
  const seen = new Set<string>();
  const duplicates = new Set<string>();
  for (const model of models) {
    if (!model.contentHash) continue;
    if (seen.has(model.contentHash)) duplicates.add(model.id);
    seen.add(model.contentHash);
  }
  return duplicates;
}

/** Room left in a bundle for new files, after those it already holds. */
export function bundleRoom(bundle: Bundle, adding: string[]): { total: number; fits: boolean } {
  const here = new Set(bundle.members.map(member => member.fileId));
  const total = bundle.members.length + adding.filter(id => !here.has(id)).length;
  return { total, fits: total <= MAX_BUNDLE_MEMBERS };
}
