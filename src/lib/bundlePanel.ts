import { bundleAggregate, type BundleAggregate } from './bundles';
import type { Bundle, ModelFile } from '../types';

export interface PanelMember {
  model: ModelFile;
  label: string | null;
  /** Position 1 in the bundle: lends the picture and the sort value. */
  isTitle: boolean;
}

/**
 * The living members of a bundle in their order, whether or not a filter shows them: the panel
 * and the detail page describe the whole bundle, and an action on it reaches every living member.
 */
export function panelMembers(bundle: Bundle, models: ModelFile[]): PanelMember[] {
  const byId = new Map(models.map(model => [model.id, model]));
  const living = [...bundle.members]
    .sort((a, b) => a.position - b.position)
    .filter(member => !member.inTrash);
  const members: PanelMember[] = [];
  for (const member of living) {
    const model = byId.get(member.fileId);
    if (model) members.push({ model, label: member.label, isTitle: members.length === 0 });
  }
  return members;
}

/** What a member is called next to the others: its label, else its file name. */
export function memberTitle(member: PanelMember): string {
  return member.label?.trim() ? member.label : member.model.name;
}

export interface PanelSummary extends BundleAggregate {
  /** Variants are alternatives: their times and weights are not added up, each is shown with its own member. */
  summable: boolean;
  /** Every member has a weight, so the sum is the whole weight. */
  weightComplete: boolean;
  /** The distinct first materials of the members, in order of appearance. */
  materials: string[];
}

export function panelSummary(bundle: Bundle, members: ModelFile[]): PanelSummary {
  const aggregate = bundleAggregate(members);
  const names = members.flatMap(model => (model.materials[0] ? [model.materials[0].name] : []));
  return {
    ...aggregate,
    summable: bundle.kind === 'parts',
    weightComplete: members.length > 0 && members.every(model => model.estimatedWeightG !== null),
    materials: [...new Set(names)],
  };
}

/** Tags every given model carries; adding one of those again would change nothing. */
export function commonTags(models: ModelFile[]): string[] {
  if (models.length === 0) return [];
  const [first, ...rest] = models;
  return first.tags.filter(tag => rest.every(model => model.tags.includes(tag)));
}

/** Members whose print status has to flip so that all of them end up with `printed`. */
export function printStatusFlips(models: ModelFile[], printed: boolean): ModelFile[] {
  return models.filter(model => (model.printStatus === 'printed') !== printed);
}

export function favoriteFlips(models: ModelFile[], favorite: boolean): ModelFile[] {
  return models.filter(model => model.favorite !== favorite);
}

/** Models that are not in the queue yet. */
export function notQueued(models: ModelFile[]): ModelFile[] {
  return models.filter(model => model.queuePosition === null);
}
