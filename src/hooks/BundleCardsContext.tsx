import { createContext, useContext, useMemo } from 'react';
import { buildCards, sortCards, type CardItem } from '../lib/bundles';
import type { Bundle, ModelFile, SortDirection, SortKey } from '../types';

export interface BundleCardsSettings {
  bundles: Bundle[];
  /** Show a bundle as one card; off keeps every file on its own card (marked as part of its bundle). */
  collapse: boolean;
  /** Order for bundle cards; absent keeps the order of the list (tool views, collections). */
  sort?: SortKey;
  sortDirection?: SortDirection;
  /** The list is the trash: a marked file says it "was part of" its bundle. */
  trash?: boolean;
}

/**
 * Bundles for the grids and lists below. Without a provider every file is a card of its own, so
 * views that know nothing about bundles (and their tests) stay as they were.
 */
export const BundleCardsContext = createContext<BundleCardsSettings | null>(null);

export function useBundleCardsSettings(): BundleCardsSettings | null {
  return useContext(BundleCardsContext);
}

/** The cards of a list of files, in list order unless the bundle cards have to be sorted among the others. */
export function useCardItems(models: ModelFile[]): CardItem[] {
  const settings = useContext(BundleCardsContext);
  return useMemo(() => {
    const cards = buildCards(models, settings?.bundles ?? [], { collapse: settings?.collapse ?? false });
    if (!settings?.sort || !cards.some(card => card.type === 'bundle')) return cards;
    return sortCards(cards, settings.sort, settings.sortDirection);
  }, [models, settings]);
}
