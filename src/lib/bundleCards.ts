import { bundleAggregate, type BundleCard, type CardItem } from './bundles';
import type { BundleKind } from '../types';

/** The id the keyboard navigation and the open detail panel use for a card: a bundle card stands for its title model. */
export function cardNavId(card: CardItem): string {
  return card.type === 'model' ? card.model.id : card.title.id;
}

export interface BundleCardSummary {
  kind: BundleKind;
  /** Members that passed the filters. */
  shown: number;
  /** Members outside the trash. */
  total: number;
  /** Printed members among the shown ones. */
  printed: number;
}

export function bundleCardSummary(card: BundleCard): BundleCardSummary {
  return {
    kind: card.bundle.kind,
    shown: card.visibleMembers.length,
    total: card.livingCount,
    printed: card.visibleMembers.filter(member => member.model.printStatus === 'printed').length,
  };
}

export interface BundleCardMeta {
  /** Names of the first material of the shown models (one for variants, one per part otherwise), without repeats. */
  materials: string[];
  weightG: number | null;
  /** Every value that went into the weight comes from the slicer, so it is not an estimate. */
  weightFromSlicer: boolean;
}

/**
 * What a bundle card shows next to its name. Variants are alternatives of one model, so the
 * card shows the title model's values; parts belong together, so it shows their sum.
 */
export function bundleCardMeta(card: BundleCard): BundleCardMeta {
  const models = card.visibleMembers.map(member => member.model);
  if (card.bundle.kind === 'variants') {
    const title = card.title;
    return {
      materials: title.materials[0] ? [title.materials[0].name] : [],
      weightG: title.estimatedWeightG,
      weightFromSlicer: title.weightSource === 'slicer',
    };
  }
  const names = models.flatMap(model => (model.materials[0] ? [model.materials[0].name] : []));
  const withWeight = models.filter(model => model.estimatedWeightG !== null);
  return {
    materials: [...new Set(names)],
    weightG: bundleAggregate(models).weightG,
    weightFromSlicer: withWeight.length > 0 && withWeight.every(model => model.weightSource === 'slicer'),
  };
}

/** Ids of the members a click on the card's checkbox selects: all living ones, also those a filter hides. */
export function cardMemberIds(card: BundleCard): string[] {
  return card.bundle.members.filter(member => !member.inTrash).map(member => member.fileId);
}

/** `all` when every living member is selected, `some` when only a part is. */
export function cardSelectionState(card: BundleCard, selected: ReadonlySet<string>): 'none' | 'some' | 'all' {
  const ids = cardMemberIds(card);
  const count = ids.filter(id => selected.has(id)).length;
  return count === 0 ? 'none' : count === ids.length ? 'all' : 'some';
}

/** The members whose selection has to flip so the card ends up fully selected (or, if it already is, fully cleared). */
export function cardToggleIds(card: BundleCard, selected: ReadonlySet<string>): string[] {
  const ids = cardMemberIds(card);
  const allSelected = ids.every(id => selected.has(id));
  return ids.filter(id => selected.has(id) === allSelected);
}

/** The card is the one open in the detail panel when the open model is one of its shown members. */
export function isCardOpen(card: CardItem, selectedId: string | null): boolean {
  if (selectedId === null) return false;
  return card.type === 'model' ? card.model.id === selectedId : card.visibleMembers.some(member => member.model.id === selectedId);
}
