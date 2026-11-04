import type { Bundle, BundleKind, ModelFile, SortDirection, SortKey } from '../types';
import { defaultSortDirection } from './catalogFilters';

/** Smallest and largest bundle the catalog accepts (the backend enforces the same numbers). */
export const MIN_BUNDLE_MEMBERS = 2;
export const MAX_BUNDLE_MEMBERS = 200;

const BUNDLE_KEY_PREFIX = 'bundle:';

/** Selection token of a bundle card; a plain file id stands for a single card. */
export function bundleKey(bundleId: string): string {
  return `${BUNDLE_KEY_PREFIX}${bundleId}`;
}

export function bundleIdOfKey(key: string): string | null {
  return key.startsWith(BUNDLE_KEY_PREFIX) ? key.slice(BUNDLE_KEY_PREFIX.length) : null;
}

/** File id -> the bundle it belongs to (a file is in at most one bundle). */
export function indexBundles(bundles: Bundle[]): Map<string, Bundle> {
  const byFile = new Map<string, Bundle>();
  for (const bundle of bundles) {
    for (const member of bundle.members) byFile.set(member.fileId, bundle);
  }
  return byFile;
}

export interface CardMember {
  model: ModelFile;
  label: string | null;
  position: number;
}

/** A file that is shown on its own. `partOf` is set when it belongs to a bundle that is not shown as a card. */
export interface SingleCard {
  type: 'model';
  key: string;
  model: ModelFile;
  partOf: { bundleId: string; name: string; kind: BundleKind } | null;
}

export interface BundleCard {
  type: 'bundle';
  key: string;
  bundle: Bundle;
  /** The visible member with the smallest position; it lends the picture and the sort value. */
  title: ModelFile;
  /** The members that passed the filters, by position. */
  visibleMembers: CardMember[];
  /** Members outside the trash, whether or not they passed the filters ("2 of 3"). */
  livingCount: number;
}

export type CardItem = SingleCard | BundleCard;

/**
 * Builds the cards of an already filtered and sorted list. A bundle becomes one card when at
 * least two of its members are in the list; with one it stays a single card marked "part of".
 * The card stands where its first visible member stood, so the list keeps its order.
 */
export function buildCards(
  models: ModelFile[],
  bundles: Bundle[],
  options: { collapse: boolean },
): CardItem[] {
  const bundleOf = indexBundles(bundles);
  const byId = new Map(models.map(model => [model.id, model]));
  const partOf = (model: ModelFile): SingleCard['partOf'] => {
    const bundle = bundleOf.get(model.id);
    return bundle ? { bundleId: bundle.id, name: bundle.name, kind: bundle.kind } : null;
  };
  const single = (model: ModelFile): SingleCard => ({ type: 'model', key: model.id, model, partOf: partOf(model) });

  const cards: CardItem[] = [];
  const placed = new Set<string>();
  for (const model of models) {
    if (placed.has(model.id)) continue;
    const bundle = options.collapse ? bundleOf.get(model.id) : undefined;
    // A member in the trash is no part of the card (the list does not normally hold one).
    if (!bundle || bundle.members.some(member => member.fileId === model.id && member.inTrash)) {
      cards.push(single(model));
      continue;
    }
    const visibleMembers: CardMember[] = [];
    for (const member of [...bundle.members].sort((a, b) => a.position - b.position)) {
      const found = byId.get(member.fileId);
      if (found && !member.inTrash) visibleMembers.push({ model: found, label: member.label, position: member.position });
    }
    if (visibleMembers.length < MIN_BUNDLE_MEMBERS) {
      cards.push(single(model));
      continue;
    }
    for (const member of visibleMembers) placed.add(member.model.id);
    cards.push({
      type: 'bundle',
      key: bundleKey(bundle.id),
      bundle,
      title: visibleMembers[0].model,
      visibleMembers,
      livingCount: bundle.members.filter(member => !member.inTrash).length,
    });
  }
  return cards;
}

/**
 * Turns the selection of a list of cards into file ids: a bundle card stands for every living
 * member (also those hidden by a filter), a plain id stays as it is. Each id appears once.
 */
export function expandSelection(selection: Iterable<string>, bundles: Bundle[]): string[] {
  const byId = new Map(bundles.map(bundle => [bundle.id, bundle]));
  const seen = new Set<string>();
  const ids: string[] = [];
  const take = (id: string) => {
    if (!seen.has(id)) {
      seen.add(id);
      ids.push(id);
    }
  };
  for (const entry of selection) {
    const bundleId = bundleIdOfKey(entry);
    if (bundleId === null) {
      take(entry);
      continue;
    }
    for (const member of byId.get(bundleId)?.members ?? []) {
      if (!member.inTrash) take(member.fileId);
    }
  }
  return ids;
}

/** Ids of the living members of every bundle whose name contains `query` (case ignored); empty for an empty query. */
export function fileIdsOfBundlesMatching(bundles: Bundle[], query: string): Set<string> {
  const needle = query.trim().toLowerCase();
  const ids = new Set<string>();
  if (!needle) return ids;
  for (const bundle of bundles) {
    if (!bundle.name.toLowerCase().includes(needle)) continue;
    for (const member of bundle.members) {
      if (!member.inTrash) ids.add(member.fileId);
    }
  }
  return ids;
}

export interface BundleAggregate {
  count: number;
  sizeBytes: number;
  /** Sum of the volumes that are known; null when no member has one. */
  volumeCm3: number | null;
  weightG: number | null;
  /** Sum of the known print times; null when no member has one. */
  printSeconds: number | null;
  /** Every member has a complete print time, so the sum is the whole time. */
  printSecondsComplete: boolean;
  printedCount: number;
  /** Union of the tags of all members, sorted. */
  tags: string[];
  /** `mixed` when the members differ (a member without a license counts as different). */
  license: { state: 'none' | 'single' | 'mixed'; value: string | null };
}

function sumKnown(values: (number | null | undefined)[]): number | null {
  let total = 0;
  let known = false;
  for (const value of values) {
    if (typeof value === 'number') {
      total += value;
      known = true;
    }
  }
  return known ? total : null;
}

/** Combined values of a bundle for its panel; nothing is stored, every value is derived from the members. */
export function bundleAggregate(members: ModelFile[]): BundleAggregate {
  const licenses = new Set(members.map(member => member.license));
  const license: BundleAggregate['license'] = licenses.size === 0 || (licenses.size === 1 && licenses.has(null))
    ? { state: 'none', value: null }
    : licenses.size === 1
      ? { state: 'single', value: [...licenses][0] }
      : { state: 'mixed', value: null };
  return {
    count: members.length,
    sizeBytes: members.reduce((sum, member) => sum + member.fileSizeBytes, 0),
    volumeCm3: sumKnown(members.map(member => member.volumeCm3)),
    weightG: sumKnown(members.map(member => member.estimatedWeightG)),
    printSeconds: sumKnown(members.map(member => member.printFacts?.printSeconds)),
    printSecondsComplete: members.length > 0 && members.every(member => member.printFacts?.printSecondsComplete === true),
    printedCount: members.filter(member => member.printStatus === 'printed').length,
    tags: [...new Set(members.flatMap(member => member.tags))].sort((a, b) => a.localeCompare(b)),
    license,
  };
}

function earliest(values: (string | null)[]): string | null {
  const known = values.filter((value): value is string => value !== null);
  return known.length ? known.reduce((a, b) => (a <= b ? a : b)) : null;
}

function latest(values: (string | null)[]): string | null {
  const known = values.filter((value): value is string => value !== null);
  return known.length ? known.reduce((a, b) => (a >= b ? a : b)) : null;
}

/**
 * The value a card is sorted by. A single card sorts by its file. A bundle card sorts by the
 * name of the bundle, the earliest import, the latest change or view, the sum of size and
 * volume, and the share of printed members.
 */
export function sortValueOfCard(card: CardItem, key: SortKey): string | number | null {
  if (card.type === 'model') {
    const m = card.model;
    switch (key) {
      case 'printStatus': return Number(m.printStatus === 'printed');
      case 'name': return m.name;
      case 'imported': return m.importedAt;
      case 'modified': return m.fileModifiedAt;
      case 'size': return m.fileSizeBytes;
      case 'vol': return m.volumeCm3;
      case 'viewed': return m.lastViewedAt;
    }
  }
  const models = card.visibleMembers.map(member => member.model);
  switch (key) {
    case 'printStatus': return models.filter(m => m.printStatus === 'printed').length / models.length;
    case 'name': return card.bundle.name;
    case 'imported': return earliest(models.map(m => m.importedAt));
    case 'modified': return latest(models.map(m => m.fileModifiedAt));
    case 'size': return models.reduce((sum, m) => sum + m.fileSizeBytes, 0);
    case 'vol': return sumKnown(models.map(m => m.volumeCm3));
    case 'viewed': return latest(models.map(m => m.lastViewedAt));
  }
}

function nameOfCard(card: CardItem): string {
  return card.type === 'model' ? card.model.name : card.bundle.name;
}

/** Orders cards like `filterAndSortModels` orders files: a card without a value goes last, ties by name. */
export function sortCards(cards: CardItem[], sort: SortKey, direction?: SortDirection): CardItem[] {
  const sign = (direction ?? defaultSortDirection(sort)) === 'asc' ? 1 : -1;
  return [...cards].sort((a, b) => {
    const x = sortValueOfCard(a, sort);
    const y = sortValueOfCard(b, sort);
    if (x === null && y === null) return nameOfCard(a).localeCompare(nameOfCard(b));
    if (x === null) return 1;
    if (y === null) return -1;
    const order = typeof x === 'number' && typeof y === 'number' ? x - y : String(x).localeCompare(String(y));
    return order * sign || nameOfCard(a).localeCompare(nameOfCard(b));
  });
}
